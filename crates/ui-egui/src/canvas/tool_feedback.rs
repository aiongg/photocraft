//! The selection operation a selection tool's next gesture has, from the options bar's mode and the
//! held modifiers, and the Patch tool's cursor badge (#170).
//!
//! Photoshop marks the selection-tool cursor with a small `+` (add), `−` (subtract) or `×`
//! (intersect) reflecting the effective operation. The badge and the commit read the same rule
//! ([`selection_op`]), so the cursor shows what the gesture will do.

use craft::behavior::CursorBadge;
use craft::behavior::geometry::SelectionOperation;
use craft::keymap::Mods;

use crate::PhotocraftApp;
use crate::state::Tool;

/// Tools whose cursor carries the selection-mode badge.
pub fn is_selection_tool(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::RectMarquee
            | Tool::EllipseMarquee
            | Tool::SingleRowMarquee
            | Tool::SingleColumnMarquee
            | Tool::Lasso
            | Tool::PolygonLasso
            | Tool::MagneticLasso
            | Tool::MagicWand
            | Tool::QuickSelection
            | Tool::ObjectSelection
    )
}

/// The operation `tool`'s gesture will use for the options-bar mode `bar` (0 New, 1 Add,
/// 2 Subtract, 3 Intersect) and the held modifiers `mods`. With nothing selected, ⌥ has nothing to
/// subtract from or intersect with, so it draws a new selection, as in Photoshop; an options-bar
/// Subtract or Intersect is kept (#1106).
pub fn selection_op(tool: Tool, bar: u8, mods: Mods, has_selection: bool) -> SelectionOperation {
    match tool {
        // Quick Selection always adds (⌥ subtracts), like Photoshop's after the first stroke.
        Tool::QuickSelection => {
            if mods.alt {
                SelectionOperation::Subtract
            } else {
                SelectionOperation::Add
            }
        }
<<<<<<< HEAD
        // Object Selection: ⌥ subtracts, ⇧ adds (see `retouch_ui::finish_object_selection`).
        Tool::ObjectSelection => {
            if mods.alt {
                SelectionOperation::Subtract
            } else if mods.shift {
                SelectionOperation::Add
            } else {
                SelectionOperation::New
=======
        // Every other selection tool, Object Selection included: ⇧⌥ intersects, ⇧ adds, ⌥
        // subtracts, otherwise the options-bar mode (`retouch_ui::finish_object_selection`).
        _ => {
            if m.shift && m.alt {
                "intersect"
            } else if m.shift {
                "add"
            } else if m.alt {
                "subtract"
            } else {
                ["replace", "add", "subtract", "intersect"][bar.min(3) as usize]
>>>>>>> upstream/main
            }
        }
        _ => SelectionOperation::resolve(SelectionOperation::ALL[usize::from(bar.min(3))], mods, has_selection),
    }
}

/// The engine's word for `op`: its selection commands call New "replace", where
/// `SelectionOperation::name` says "new".
pub fn mode_name(op: SelectionOperation) -> &'static str {
    match op {
        SelectionOperation::New => "replace",
        SelectionOperation::Add => "add",
        SelectionOperation::Subtract => "subtract",
        SelectionOperation::Intersect => "intersect",
        _ => "replace",
    }
}

<<<<<<< HEAD
/// Whether the active document has a non-empty selection. Reads the bounds the selection outline
/// caches (same key), since the canvas asks every frame while a Patch lasso is drawn or hovered
/// with ⌥.
pub fn has_selection(app: &PhotocraftApp) -> bool {
    app.session.active().is_some_and(|st| st.doc.selection.as_ref().is_some_and(|s| !app.known_bounds(u64::MAX - st.doc.id.0, s).is_empty()))
}

/// The badge the Patch or Content-Aware Move cursor shows at `at` (document point), `None` for any
/// other tool. `press` is the running drag's press point and modifiers: a drag keeps what its press
/// decided, as the release does. Over the selection without modifiers a press drags the patch
/// (an arrow for Patch, none for Content-Aware Move); otherwise the tool lassos its area in the
/// Lasso's operation (⇧ adds, ⌥ subtracts, #11024). A ⌘ press is the Move tool's, through the
/// Command stand-in.
pub fn patch_badge(app: &PhotocraftApp, at: [f64; 2], press: Option<([f64; 2], Mods)>, mods: Mods) -> Option<CursorBadge> {
    let tool = app.ui.tool;
    if !matches!(tool, Tool::Patch | Tool::ContentAwareMove) {
=======
/// The badge the cursor of `tool` shows with modifiers `m` (None for non-selection tools or New;
/// the anchor point tools show theirs).
pub fn badge(app: &PhotocraftApp, tool: Tool, m: egui::Modifiers) -> Option<Badge> {
    // Add / Delete Anchor Point: + or − beside the crosshair, as on Photoshop's pen cursors (⌥
    // swaps them; ⌘ is Direct Selection, which shows none).
    if let Some(t) = crate::anchor_tools::effective(tool, m) {
        // Add Anchor Point shows its "+" only over a segment a click would split (#12729); ⌥'s
        // swapped tools keep theirs everywhere.
        let over_segment = || app.hover_doc.is_some_and(|p| crate::anchor_tools::adds_anchor_at(app, tool, p));
        return (!m.command && (m.alt || tool != Tool::AddAnchorPoint || over_segment())).then_some(if t == Tool::AddAnchorPoint {
            Badge::Add
        } else {
            Badge::Subtract
        });
    }
    // The Curvature Pen over a point: + on the active path's segments (a click adds an anchor
    // there), four arrows on an anchor (a press drags it).
    if let Some(p) = app.hover_doc.filter(|_| tool == Tool::CurvaturePen) {
        return crate::direct_select::curvature_hover(app, p, m)
            .map(|h| if matches!(h, photocraft_vector::edit::Hit::Segment(..)) { Badge::Add } else { Badge::Move });
    }
    // The Pen over the active path with Auto Add/Delete on: what a click would do (#4144).
    if let Some(p) = app.hover_doc.filter(|_| tool == Tool::Pen) {
        return crate::anchor_tools::pen_auto(app, tool, m, p).map(|(t, ..)| if t == Tool::AddAnchorPoint { Badge::Add } else { Badge::Subtract });
    }
    // Patch and Content-Aware Move lasso their area in the Lasso's mode (⇧ adds, ⌥ subtracts,
    // #11024). Over the selection without them a press drags the patch, which combines nothing.
    if matches!(tool, Tool::Patch | Tool::ContentAwareMove) {
        // A drag in progress keeps what its press decided (the release reads `drag.modifiers`).
        let drag = app.drag.as_ref().filter(|d| d.tool == tool);
        let at = drag.map(|d| d.start).or(app.hover_doc);
        let m = drag.map_or(m, |d| d.modifiers);
        if at.is_some_and(|p| crate::retouch_ui::patch_drags_selection(app, p, m)) {
            return (tool == Tool::Patch).then_some(Badge::Arrow);
        }
        return Badge::from_mode(document_selection_mode(app, Tool::Lasso, m));
    }
    if !is_selection_tool(tool) {
>>>>>>> upstream/main
        return None;
    }
    let (at, mods) = press.unwrap_or((at, mods));
    if crate::retouch_ui::patch_drags_selection(app, at, mods) {
        return (tool == Tool::Patch).then_some(CursorBadge::Arrow);
    }
    selection_op(Tool::Lasso, app.ui.selection_mode, mods, has_selection(app)).badge()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALT: Mods = Mods { alt: true, ..Mods::NONE };

    /// ⌥'s "is anything selected" reads the bounds the selection outline caches, as the canvas
    /// asks every frame, and scans only a selection that changed since.
    #[test]
    fn alt_reads_the_cached_selection_bounds() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", serde_json::json!({"width": 40, "height": 30})).unwrap();
        app.run("select.rect", serde_json::json!({"x": 5, "y": 5, "width": 10, "height": 10})).unwrap();
        assert!(has_selection(&app), "nothing cached: scanned");
        assert_eq!(selection_op(Tool::Lasso, 0, ALT, has_selection(&app)), SelectionOperation::Subtract);
        let st = app.session.active().unwrap();
        let (key, fingerprint) = (u64::MAX - st.doc.id.0, crate::surface_fingerprint(st.doc.selection.as_ref().unwrap()));
        // An empty box cached for this very selection is believed, so nothing is scanned.
        app.bounds_cache.insert(key, (fingerprint, photocraft_geom::Rect::EMPTY));
        assert!(!has_selection(&app), "the cached bounds are used");
        assert_eq!(selection_op(Tool::Lasso, 0, ALT, has_selection(&app)), SelectionOperation::New);
        app.run("select.rect", serde_json::json!({"x": 20, "y": 5, "width": 10, "height": 10})).unwrap();
        assert!(has_selection(&app), "a new selection is scanned again");
    }

    /// (tool, options-bar mode, modifiers, anything selected) → the operation the gesture and the
    /// badge share.
    #[test]
    fn selection_op_table() {
        use SelectionOperation::{Add, Intersect, New, Subtract};
        let (shift, both) = (Mods { shift: true, ..Mods::NONE }, Mods { shift: true, alt: true, ..Mods::NONE });
        let rows = [
            (Tool::RectMarquee, 0, Mods::NONE, true, New),
            (Tool::EllipseMarquee, 0, shift, true, Add),
            (Tool::Lasso, 0, ALT, true, Subtract),
            (Tool::PolygonLasso, 0, both, true, Intersect),
            (Tool::MagicWand, 0, ALT, true, Subtract),
            // #1106: with nothing selected ⌥ and ⇧⌥ have nothing to combine with, so they draw a new one.
            (Tool::Lasso, 0, ALT, false, New),
            (Tool::RectMarquee, 0, both, false, New),
            (Tool::RectMarquee, 0, shift, false, Add),
            // The options bar's mode shows with no key held; keys still override it, and a bar
            // Subtract or Intersect is kept with nothing selected.
            (Tool::Lasso, 2, Mods::NONE, true, Subtract),
            (Tool::Lasso, 2, ALT, false, Subtract),
            (Tool::Lasso, 2, shift, true, Add),
            (Tool::RectMarquee, 3, Mods::NONE, false, Intersect),
            (Tool::RectMarquee, 200, Mods::NONE, true, Intersect),
            // Smart selection tools ignore the bar.
            (Tool::QuickSelection, 2, Mods::NONE, false, Add),
            (Tool::QuickSelection, 0, ALT, true, Subtract),
            (Tool::ObjectSelection, 3, Mods::NONE, true, New),
            (Tool::ObjectSelection, 0, shift, true, Add),
            (Tool::ObjectSelection, 0, ALT, false, Subtract),
        ];
        for (tool, bar, mods, selected, op) in rows {
            assert_eq!(selection_op(tool, bar, mods, selected), op, "{tool:?} bar={bar} {mods:?} selected={selected}");
        }
<<<<<<< HEAD
        assert_eq!([New, Add, Subtract, Intersect].map(mode_name), ["replace", "add", "subtract", "intersect"], "the engine's words");
=======
        // With nothing selected the Lasso's Alt draws straight segments: no − badge.
        app.run("select.deselect", serde_json::json!({})).unwrap();
        assert_eq!(badge(&app, Tool::Lasso, alt), None);
        assert_eq!(badge(&app, Tool::RectMarquee, alt), Some(Badge::Subtract));
        // The options bar's mode shows without any key held; keys still override it.
        app.ui.selection_mode = 2;
        assert_eq!(badge(&app, Tool::Lasso, none), Some(Badge::Subtract));
        assert_eq!(badge(&app, Tool::Lasso, shift), Some(Badge::Add));
        app.ui.selection_mode = 3;
        assert_eq!(badge(&app, Tool::RectMarquee, none), Some(Badge::Intersect));
        // Smart selection tools.
        assert_eq!(badge(&app, Tool::QuickSelection, none), Some(Badge::Add));
        assert_eq!(badge(&app, Tool::QuickSelection, alt), Some(Badge::Subtract));
        // Object Selection follows the options bar like the other selection tools (#12853).
        assert_eq!(badge(&app, Tool::ObjectSelection, none), Some(Badge::Intersect));
        app.ui.selection_mode = 0;
        assert_eq!(badge(&app, Tool::ObjectSelection, none), None);
        assert_eq!(badge(&app, Tool::ObjectSelection, shift), Some(Badge::Add));
        assert_eq!(badge(&app, Tool::ObjectSelection, both), Some(Badge::Intersect));
        // Other tools never show one.
        for t in [Tool::Brush, Tool::Move, Tool::Zoom, Tool::Crop] {
            assert_eq!(badge(&app, t, shift), None);
        }
        // Distinct glyphs: + has two strokes, − one, × two diagonals.
        let c = pos2(10.0, 10.0);
        assert_eq!(badge_glyph(Badge::Add, c, 3.0).len(), 2);
        assert_eq!(badge_glyph(Badge::Subtract, c, 3.0).len(), 1);
        assert!(badge_glyph(Badge::Intersect, c, 3.0).iter().all(|[a, b]| a.x != b.x && a.y != b.y));
    }

    /// #7495: an outline still being drawn is one unbroken line (no dashes, nothing marching),
    /// black over a light edge.
    #[test]
    fn solid_path_is_one_unbroken_dark_line_on_a_light_edge() {
        let pts = [pos2(0.0, 0.0), pos2(40.0, 0.0), pos2(40.0, 30.0)];
        for closed in [false, true] {
            let shapes = solid_path(&pts, closed);
            assert!(shapes.iter().all(|s| matches!(s, Shape::Path(p) if p.closed == closed && p.points.len() == 3)), "{shapes:?}");
            assert_eq!(strokes(&shapes), vec![Color32::from_white_alpha(140), Color32::BLACK], "the dark line on top");
        }
    }

    #[test]
    fn shape_previews_contrast_and_ellipses_close() {
        let pts = [pos2(0.0, 0.0), pos2(40.0, 0.0), pos2(40.0, 30.0)];
        // Shape previews: the accent path over a dark halo.
        let s = contrast_path(pts.to_vec(), true, Color32::from_rgb(0, 120, 255));
        let cols = strokes(&s);
        assert_eq!(cols.len(), 2);
        assert!(cols[0].r() < 10 && cols[0].a() > 100 && cols[1] == Color32::from_rgb(0, 120, 255));
        // Ellipses close smoothly.
        let e = ellipse_points(egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 50.0)));
        assert!(e.len() >= 16 && e.iter().all(|p| p.x >= -0.01 && p.x <= 100.01 && p.y >= -0.01 && p.y <= 50.01));
>>>>>>> upstream/main
    }
}

#[cfg(test)]
#[path = "../patch_cursor_badge_tests.rs"]
mod patch_cursor_badge_tests;
