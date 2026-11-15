//! From the overlay's reports to engine commands. `vector_ui::show_paths` shows the one
//! `BezierOverlay` of the path tools and hands its output here: each edit that ends (a `Commit`
//! or a `Set`) becomes at most one command, as does the Insert key, and a delete becomes one
//! `path.deleteAnchor` per anchor. Edits to the Pen path being drawn change the pen draft instead.
//!
//! A pass applies only its own output, so a second pass of a frame applies nothing twice.

use craft::behavior::EditEvent;
use craft::draw::ui::pen::{BezierHit, BezierOutput, EditPath, HandleSide, KnotRef, PathEdit, PathSelection};
use craft::keymap::Mods;
use egui::Pos2;
use serde_json::{Value, json};

use super::{DirectSelection, Owner, PathRef, keep_after_command};
use crate::PhotocraftApp;
use crate::state::Tool;

/// Applies the reports of one overlay pass. `refs` are the refs of the `EditPath`s the pass showed
/// (`edit_paths`); a report names its path by key, and only a key of `refs` is applied.
pub(crate) fn apply(app: &mut PhotocraftApp, output: &BezierOutput, refs: &[PathRef], tool: Tool, mods: Mods) {
    if let Some(selection) = &output.select {
        select(app, selection);
    }
    let mut commit = false;
    for event in &output.edit.events {
        match event {
            EditEvent::Begin { .. } => app.ui.direct_selection.editing = Owner::current(app),
            // A gesture runs only on the document, revision and target it began on.
            EditEvent::Commit { .. } => {
                let began = app.ui.direct_selection.editing.take();
                commit = began.is_some() && began == Owner::current(app);
            }
            EditEvent::Cancel { .. } => app.ui.direct_selection.editing = None,
            EditEvent::Set { .. } => commit = true,
            _ => {}
        }
    }
    if commit && let Some(change) = &output.change {
        change_path(app, change, refs, tool, mods);
    }
    if let Some(delete) = &output.delete
        && let Some(r) = known(delete.path, refs)
    {
        let anchors: Vec<[usize; 2]> = delete.anchors.iter().map(|k| [k.subpath, k.knot]).collect();
        delete_anchors(app, r, &anchors, delete.segments);
    }
    if let Some((key, knot, t)) = output.insert
        && let Some(r) = known(key, refs)
    {
        insert(app, r, knot, t);
    }
    if let Some((hit, at)) = &output.context {
        context(app, hit, *at, tool, mods);
    }
    if output.pressed_empty && mods.cmd && app.ui.pen.is_some() {
        // A ⌘-press off the path being drawn ends it, which becomes editable, as in Photoshop; a
        // refused commit keeps the draft and says why.
        crate::vector_ui::pen_commit(app, false);
    }
}

/// A pass that shows no editing overlay: a gesture it began has nobody to end it.
pub(crate) fn not_shown(app: &mut PhotocraftApp) {
    app.ui.direct_selection.editing = None;
}

/// Deletes anchors `[subpath, knot]` of `path`, highest index first so the lower ones stay valid.
/// A live shape asks first (`live_shape_prompt`); the selection then keeps only the path.
pub(crate) fn delete_anchors(app: &mut PhotocraftApp, path: PathRef, anchors: &[[usize; 2]], segments: bool) {
    let mut order = anchors.to_vec();
    order.sort_unstable_by(|a, b| b.cmp(a));
    if order.is_empty() {
        return;
    }
    if path == PathRef::Pen {
        edit_draft(app, |draft| {
            let Some(sub) = draft.subpaths.get_mut(0) else { return false };
            order.iter().filter(|[s, _]| *s == 0).fold(false, |changed, [_, k]| sub.remove_knot(*k) || changed)
        });
        app.ui.direct_selection = DirectSelection { target: Some(path), ..Default::default() };
        return;
    }
    let params = |&[s, k]: &[usize; 2]| {
        let mut params = json!({"subpath": s, "knot": k});
        if segments {
            params["segments"] = json!(true);
        }
        with_target(app, path, params)
    };
    // PLAN: the prompt's confirm runs one `path.deleteAnchor`, so a live shape with several anchors selected loses only the first one asked about (dialogs-w21)
    if crate::dialogs::live_shape_prompt::ask(app, path, &params(&order[0])) {
        return;
    }
    let key = (order.len() > 1).then(|| format!("delete-anchor-{}", app.ui.alloc_id()));
    let mut deleted = false;
    for anchor in &order {
        let mut params = params(anchor);
        if let Some(key) = &key {
            params["coalesce"] = json!(key);
        }
        match app.run("path.deleteAnchor", params) {
            Ok(_) => deleted = true,
            Err(e) => {
                fail(app, e);
                break;
            }
        }
    }
    // The anchors shift: a selection on this path would point at the wrong ones.
    if deleted {
        app.ui.direct_selection = DirectSelection { target: Some(path), ..Default::default() };
    }
}

/// The path of `key` when this pass showed it.
fn known(key: u64, refs: &[PathRef]) -> Option<PathRef> {
    PathRef::from_key(key).filter(|r| refs.contains(r))
}

/// The overlay's selection replaces this one and its owner; a running gesture stays.
fn select(app: &mut PhotocraftApp, selection: &PathSelection) {
    let owner = Owner::current(app);
    let ds = &mut app.ui.direct_selection;
    let editing = ds.editing.take();
    *ds = DirectSelection {
        target: selection.path.and_then(PathRef::from_key),
        anchors: selection.anchors.iter().map(|k| [k.subpath, k.knot]).collect(),
        segment: selection.segment.map(|k| [k.subpath, k.knot]),
        owner,
        editing,
        ..Default::default()
    };
}

fn side_name(side: HandleSide) -> &'static str {
    match side {
        HandleSide::In => "in",
        _ => "out",
    }
}

fn ids(anchors: &[KnotRef]) -> Vec<[usize; 2]> {
    anchors.iter().map(|k| [k.subpath, k.knot]).collect()
}

/// Did a drag move at all? A click moves nothing and commits nothing.
fn moved(x: f64, y: f64) -> bool {
    x != 0.0 || y != 0.0
}

/// Runs the command a finished edit stands for.
fn change_path(app: &mut PhotocraftApp, change: &PathEdit, refs: &[PathRef], tool: Tool, mods: Mods) {
    let Some(r) = known(change.path(), refs) else { return };
    if r == PathRef::Pen {
        edit_draft(app, |path| change.apply(path));
        return;
    }
    // The Convert Point tool names its steps as the reference does; the Pen's ⌥ and Direct
    // Selection (Convert Point with ⌘ too) keep the commands' own names.
    let convert_tool = tool == Tool::ConvertPoint && !mods.cmd;
    match change {
        PathEdit::MoveAnchors { anchors, by, .. } if moved(by.x, by.y) => {
            let mut params = json!({"anchors": ids(anchors), "move": [by.x, by.y]});
            if convert_tool {
                params["stepName"] = json!("Drag Path");
            }
            command(app, r, "path.moveAnchors", params);
        }
        PathEdit::MoveCurvature { anchors, by, .. } if moved(by.x, by.y) => {
            command(app, r, "path.moveAnchors", json!({"anchors": ids(anchors), "move": [by.x, by.y], "curvature": true}));
        }
        PathEdit::MoveHandle { knot, side, to, alone, .. } => {
            let mut params =
                json!({"subpath": knot.subpath, "knot": knot.knot, "handle": side_name(*side), "to": [to.x, to.y], "independent": alone});
            if convert_tool && *alone {
                params["stepName"] = json!("Corner Drag");
            }
            command(app, r, "path.moveHandle", params);
        }
        PathEdit::BendSegment { segment, t, by, .. } if moved(by.x, by.y) => {
            command(app, r, "path.bendSegment", json!({"subpath": segment.subpath, "knot": segment.knot, "t": t, "move": [by.x, by.y]}));
        }
        PathEdit::Convert { knot, out, .. } => {
            let mut params = json!({"subpath": knot.subpath, "knot": knot.knot});
            if let Some(out) = out {
                params["out"] = json!([out.x, out.y]);
            }
            if convert_tool {
                params["stepName"] = json!("Change Anchor Point");
            }
            command(app, r, "path.convertPoint", params);
        }
        PathEdit::ToggleCurvature { knot, .. } => {
            command(app, r, "path.convertPoint", json!({"subpath": knot.subpath, "knot": knot.knot, "curvature": true}));
        }
        PathEdit::InsertCurvature { segment, t, by, .. } => {
            add_and_move(app, r, "curvature-add", *segment, t.clamp(0.01, 0.99), [by.x, by.y], true);
        }
        // A hit right at a segment's end is an anchor (anchors win the hit test), so `t` is inside.
        PathEdit::InsertAnchor { segment, t, by, .. } => add_and_move(app, r, "add-anchor", *segment, *t, [by.x, by.y], false),
        _ => {}
    }
}

/// `path.addAnchor` on the segment leaving `segment`, then the drag's move of the new anchor
/// (`knot + 1`); one history step when both run.
fn add_and_move(app: &mut PhotocraftApp, r: PathRef, prefix: &str, segment: KnotRef, t: f64, by: [f64; 2], curvature: bool) {
    let dragged = moved(by[0], by[1]);
    let key = dragged.then(|| format!("{prefix}-{}", app.ui.alloc_id()));
    let mut add = json!({"subpath": segment.subpath, "knot": segment.knot, "t": t});
    if let Some(key) = &key {
        add["coalesce"] = json!(key);
    }
    if command(app, r, "path.addAnchor", add).is_none() || !dragged {
        return;
    }
    let mut mv = json!({"anchors": [[segment.subpath, segment.knot + 1]], "move": by, "coalesce": key});
    if curvature {
        mv["curvature"] = json!(true);
    }
    command(app, r, "path.moveAnchors", mv);
}

/// The Insert key: an anchor in the middle of the segment leaving `knot`.
fn insert(app: &mut PhotocraftApp, r: PathRef, knot: KnotRef, t: f64) {
    if r == PathRef::Pen {
        edit_draft(app, |path| path.subpaths.get_mut(knot.subpath).and_then(|sub| sub.insert_at(knot.knot, t)).is_some());
        return;
    }
    command(app, r, "path.addAnchor", json!({"subpath": knot.subpath, "knot": knot.knot, "t": t}));
}

/// A secondary click on a path part opens the canvas tool menu with that part as its point entry.
fn context(app: &mut PhotocraftApp, hit: &BezierHit, at: Pos2, tool: Tool, mods: Mods) {
    let Some(r) = PathRef::from_key(hit.path()) else { return };
    let Some(doc) = crate::canvas::document_display::doc_point_at(app, at) else { return };
    crate::canvas_tool_menu::open_at(app, tool, mods, at, [doc.x, doc.y], Some((r, hit.clone())));
}

fn with_target(app: &PhotocraftApp, r: PathRef, mut params: Value) -> Value {
    if let (Some(p), Some(t)) = (params.as_object_mut(), r.params(app).as_object()) {
        p.extend(t.clone());
    }
    params
}

fn fail(app: &mut PhotocraftApp, error: String) {
    app.ui.status = error;
    app.ui.status_error = true;
}

/// Runs `id` on `r`. The selection stays off the live UI while its own command runs (`sync_views`
/// checks ownership then) and is bound to the revision the command made.
fn command(app: &mut PhotocraftApp, r: PathRef, id: &str, params: Value) -> Option<Value> {
    let params = with_target(app, r, params);
    let selection = std::mem::take(&mut app.ui.direct_selection);
    match app.run(id, params) {
        Ok(v) => {
            keep_after_command(app, selection);
            Some(v)
        }
        Err(e) => {
            if selection.owner.as_ref().is_some_and(|owner| owner.valid(app)) {
                app.ui.direct_selection = selection;
            }
            fail(app, e);
            None
        }
    }
}

/// Edits the pen draft as a path of its own key and hands the result back to the pen.
fn edit_draft(app: &mut PhotocraftApp, edit: impl FnOnce(&mut EditPath) -> bool) {
    let Some(draft) = app.ui.pen.as_ref().map(|pen| pen.draft.clone()) else { return };
    let mut path = EditPath::new(PathRef::Pen.key(), vec![draft]);
    if !edit(&mut path) {
        return;
    }
    if let Some(draft) = path.subpaths.into_iter().next() {
        crate::vector_ui::replace_draft(app, &draft);
    }
}

#[cfg(test)]
#[path = "../anchor_tools/tests.rs"]
mod tests;
#[cfg(test)]
#[path = "../anchor_tools/add_anchor_delete_key_tests.rs"]
mod add_anchor_delete_key_tests;
#[cfg(test)]
#[path = "../anchor_tools/add_anchor_lock_tests.rs"]
mod add_anchor_lock_tests;
#[cfg(test)]
#[path = "../anchor_tools/add_anchor_targeted_path_tests.rs"]
mod add_anchor_targeted_path_tests;
#[cfg(test)]
#[path = "../anchor_tools/delete_anchor_alt_tests.rs"]
mod delete_anchor_alt_tests;
#[cfg(test)]
#[path = "../anchor_tools/delete_anchor_delete_key_tests.rs"]
mod delete_anchor_delete_key_tests;
#[cfg(test)]
#[path = "../anchor_tools/delete_anchor_drag_segment_tests.rs"]
mod delete_anchor_drag_segment_tests;
#[cfg(test)]
#[path = "../anchor_tools/delete_anchor_escape_tests.rs"]
mod delete_anchor_escape_tests;
#[cfg(test)]
#[path = "../anchor_tools/delete_anchor_live_shape_tests.rs"]
mod delete_anchor_live_shape_tests;
#[cfg(test)]
#[path = "../anchor_tools/delete_anchor_lock_tests.rs"]
mod delete_anchor_lock_tests;
#[cfg(test)]
#[path = "../anchor_tools/delete_anchor_targeted_path_tests.rs"]
pub(crate) mod delete_anchor_targeted_path_tests;
