use super::overlay::path_lines;
use super::shapes::hex;
use super::{Color32, Document, LayerContent, Path, PhotocraftApp, Rect, Value, json, pos2, vec2};
use craft_ui::behavior::ListDrag;
use craft_ui::composites::PanelFooter;
use craft_ui::icons::Icon;
use craft_ui::list::{ListRow, RowEvent, ThumbSize};
use craft_ui::menu::{ContextMenu, MenuEntry, MenuItem};
use craft_ui::theme::StrokeExt;
use craft_ui::theme::{Tokens as LibTokens, TokensOf as _};
use std::cell::Cell;

use crate::theme::Tokens;

// ---------------------------------------------------------------------------------------------
// Paths panel

fn thumb(painter: &egui::Painter, r: Rect, path: &Path, doc: &Document) {
    let t = Tokens::get(painter.ctx());
    // At the document's aspect ratio, like the Channels thumbnails.
    let r = crate::channels_panel::fit_thumb(r, doc.size.width, doc.size.height).0;
    painter.rect_filled(r, craft_ui::theme::scale::Radius::R0.px(), Color32::from_gray(if t.pro { 222 } else { 240 }));
    let (w, h) = (doc.size.width.max(1) as f64, doc.size.height.max(1) as f64);
    let s = (r.width() as f64 / w).min(r.height() as f64 / h);
    let off = r.center() - vec2((w * s) as f32 / 2.0, (h * s) as f32 / 2.0);
    let map = |q: [f64; 2]| pos2(off.x + (q[0] * s) as f32, off.y + (q[1] * s) as f32);
    for (pts, closed) in path_lines(path, &map) {
        if closed && pts.len() >= 3 {
            painter.add(egui::Shape::closed_line(pts, craft_ui::theme::scale::Stroke::W1.egui(painter.ctx(), Color32::from_gray(40))));
        } else {
            painter.add(egui::Shape::line(pts, craft_ui::theme::scale::Stroke::W1.egui(painter.ctx(), Color32::from_gray(40))));
        }
    }
}

/// What a Paths panel row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathRow {
    Saved,
    Work,
    /// The selected layer's shape path or vector mask (temporary, italic).
    Layer,
}

/// A Paths panel row.
#[derive(Clone, Debug)]
pub struct PathEntry {
    pub name: String,
    pub path: Path,
    pub kind: PathRow,
}

/// The Paths panel's rows, top to bottom: saved paths, the work path, then the selected layer's
/// shape path ("<Layer> Shape Path") or vector mask ("<Layer> Vector Mask"), like Photoshop.
pub fn path_rows(doc: &Document, active: Option<photocraft_doc::LayerId>) -> Vec<PathEntry> {
    let mut rows: Vec<_> = doc.paths.iter().map(|p| PathEntry { name: p.name.clone(), path: p.path.clone(), kind: PathRow::Saved }).collect();
    if let Some(wp) = &doc.work_path {
        rows.push(PathEntry { name: "Work Path".into(), path: wp.clone(), kind: PathRow::Work });
    }
    if let Some(l) = active.and_then(|id| doc.layer(id)) {
        let layer_path = match &l.content {
            LayerContent::Shape(sh) => Some((format!("{} Shape Path", l.name), sh.path.clone())),
            _ => l.vector_mask.as_ref().map(|v| (format!("{} Vector Mask", l.name), v.path.clone())),
        };
        if let Some((name, path)) = layer_path {
            rows.push(PathEntry { name, path, kind: PathRow::Layer });
        }
    }
    rows
}

fn footer_id() -> egui::Id {
    egui::Id::unique("paths-footer")
}

/// A Paths panel row being dragged (egui's `DragAndDrop` payload), for a drop on the trash
/// (#6553): its key is `PathEntry::key`.
type PathDrag = ListDrag<String>;

fn list_id() -> egui::Id {
    egui::Id::unique("pc-paths-list")
}

fn ctx_data_footer(ctx: &egui::Context, r: Rect) {
    ctx.data_mut(|d| d.insert_temp(footer_id(), r));
}

/// Where the Paths panel's footer buttons were drawn last frame (the rect around them all).
pub fn paths_footer(ctx: &egui::Context) -> Option<Rect> {
    ctx.data(|d| d.get_temp(footer_id()))
}

/// Photoshop's default name for a saved path: the first free "Path N", counting from the number of
/// saved paths.
fn free_path_name(doc: &Document) -> String {
    let mut n = doc.paths.len().saturating_add(1);
    while doc.paths.iter().any(|p| p.name == format!("Path {n}")) {
        n = n.saturating_add(1);
        if n == usize::MAX {
            break;
        }
    }
    format!("Path {n}")
}

/// Whether the Paths panel is renaming a row (the row's key is "work" or the saved path's name).
/// It lives in egui's temp data, like the Channels panel's rename flag.
fn rename_id(doc: &Document, key: &str) -> egui::Id {
    egui::Id::unique(("path-rename", doc.id.0, key.to_owned()))
}

/// Commands offered by a Paths row's context menu. The row's path is passed explicitly so a
/// right-click acts on that row even when another path is selected in the panel. `layer` is the
/// layer a `PathRow::Layer` row belongs to (the active layer).
pub(super) fn path_context_actions(entry: &PathEntry, doc: &Document, layer: Option<photocraft_doc::LayerId>) -> Vec<(&'static str, &'static str, Value)> {
    let key = match entry.kind {
        PathRow::Work => "work".to_string(),
        PathRow::Layer => "layer".to_string(),
        PathRow::Saved => entry.name.clone(),
    };
    let mut actions = vec![
        ("Make Selection", "path.toSelection", json!({"name": key})),
        ("Fill Path", "path.fill", json!({"name": key})),
        ("Stroke Path…", "path.stroke", json!({"name": key})),
    ];
    if entry.kind == PathRow::Work {
        actions.push(("Save Path", "path.rename", json!({"name": "work", "to": free_path_name(doc)})));
    } else {
        let mut n = 1usize;
        while doc.paths.iter().any(|p| p.name == format!("{} copy {n}", entry.name)) {
            n = n.saturating_add(1);
            if n == usize::MAX {
                break;
            }
        }
        let copy_name = format!("{} copy {n}", entry.name);
        actions.push(("Duplicate Path", "path.set", json!({"name": copy_name, "path": photocraft_engine::vector_cmds::path_json(&entry.path)})));
    }
    if entry.kind == PathRow::Saved {
        // No `to`: the panel asks for the new name in place.
        actions.push(("Rename Path…", "path.rename", json!({"name": key})));
    }
    if entry.kind != PathRow::Layer {
        actions.push(("Delete Path", "path.delete", json!({"name": key})));
    } else if entry.name.ends_with(" Vector Mask") {
        actions.push(("Delete Vector Mask", "layer.vectorMask.delete", json!({})));
    } else if let Some(id) = layer {
        // A shape layer's path is the shape: Photoshop's Delete Path deletes the layer (#6552).
        actions.push(("Delete Path", "layer.delete", json!({"layer": id.0})));
    }
    actions
}

impl PathEntry {
    /// The row's reference in the path commands and the panel's selection.
    fn key(&self) -> String {
        match self.kind {
            PathRow::Work => "work".to_string(),
            PathRow::Layer => "layer".to_string(),
            PathRow::Saved => self.name.clone(),
        }
    }

    /// What dropping the row on the trash runs; a shape layer's path is not dragged.
    fn delete_command(&self) -> Option<(&'static str, Value)> {
        match self.kind {
            PathRow::Work => Some(("path.delete", json!({"name": "work"}))),
            PathRow::Saved => Some(("path.delete", json!({"name": self.name}))),
            PathRow::Layer if self.name.ends_with(" Vector Mask") => Some(("layer.vectorMask.delete", json!({}))),
            PathRow::Layer => None,
        }
    }
}

type Cmd = (&'static str, Value);

/// The Paths panel's actions that need more than a command.
#[derive(Default)]
struct PanelOut {
    action: Option<(&'static str, Value)>,
    stroke_dialog: Option<String>,
}

fn show_row(app: &mut PhotocraftApp, ui: &mut egui::Ui, doc: &Document, layer: Option<photocraft_doc::LayerId>, entry: &PathEntry, out: &mut PanelOut) {
    let PathEntry { name, path, kind } = entry;
    let key = entry.key();
    let rename_id = rename_id(doc, &key);
    let renaming = ui.data(|d| d.get_temp::<bool>(rename_id)).unwrap_or(false);
    // The name offered when a rename starts: the Work Path is saved under a free "Path N".
    let proposed = match kind {
        PathRow::Work => Some(free_path_name(doc)),
        PathRow::Saved => Some(name.clone()),
        PathRow::Layer => None,
    };
    let label = match kind {
        PathRow::Work if renaming => proposed.clone().unwrap_or_default(),
        PathRow::Work => tl!(name).to_string(),
        _ => name.clone(),
    };
    // Temporary paths (the work path, the selected layer's shape path or vector mask) are italic.
    let thumb_rect = Cell::new(None::<Rect>);
    let shown = ListRow::new(key.clone(), label.as_str())
        .selected(app.ui.selected_path.as_deref() == Some(key.as_str()))
        .layer(true)
        .italic(*kind != PathRow::Saved)
        .renaming(renaming)
        .thumb(ThumbSize::Small, |painter, cell| {
            thumb(painter, cell, path, doc);
            thumb_rect.set(Some(cell));
        })
        .show(ui);
    let resp = shown.response.clone();
    let mut renamed = false;
    for event in &shown.events {
        match event {
            RowEvent::Renamed { text, .. } => {
                let to = text.trim();
                // An unchanged saved name is left alone.
                if !to.is_empty() && to != name {
                    out.action = Some(("path.rename", json!({"name": key, "to": to})));
                    renamed = true;
                }
            }
            RowEvent::RenameEnded { end, .. } => {
                ui.data_mut(|d| d.remove::<bool>(rename_id));
                // The Work Path is saved even under its proposed name.
                if *kind == PathRow::Work && !renamed && *end != craft_ui::list::RenameEnd::Escape {
                    out.action = Some(("path.rename", json!({"name": key, "to": proposed.clone().unwrap_or_default()})));
                }
            }
            _ => {}
        }
    }
    if resp.drag_started() && entry.delete_command().is_some() {
        let order = [craft_ui::behavior::RowInfo { key: key.clone(), depth: 0, has_children: false, open: false, section: false }];
        egui::DragAndDrop::set_payload(ui.ctx(), PathDrag::new(list_id(), &order[..], &[], key.clone(), label.clone()));
    }
    // Ctrl/Cmd-click loads only the thumbnail's path, without changing the panel target.
    let modifiers = ui.input(|i| i.modifiers);
    let load = resp.clicked() && modifiers.command && thumb_rect.get().zip(resp.interact_pointer_pos()).is_some_and(|(r, p)| r.contains(p));
    if load {
        let mode = match crate::channels_panel::load_operation(modifiers) {
            "new" => "replace",
            mode => mode,
        };
        out.action = Some(("path.toSelection", json!({"name": key, "mode": mode})));
    } else if resp.clicked() {
        // Selecting the Work Path ends the hide from Make Selection (#6079): deselected
        // again, it is an ordinary unselected Work Path (#6551).
        if *kind == PathRow::Work {
            app.ui.hidden_work_path = None;
        }
        app.ui.selected_path = Some(key.clone());
    }
    // As in Photoshop, double-clicking a saved path's name renames it, and double-clicking
    // the Work Path asks for the name to save it under (#6238).
    if !load && resp.double_clicked() && proposed.is_some() {
        ui.data_mut(|d| d.insert_temp(rename_id, true));
    }
    let can_paint = app.session.active().and_then(|s| s.active_layer.and_then(|id| s.doc.layer(id))).is_some_and(|l| l.surface().is_some());
    let entries: Vec<MenuEntry<Cmd>> = path_context_actions(entry, doc, layer)
        .into_iter()
        .map(|(label, cmd, params)| MenuItem::new(tl!(label), (cmd, params)).enabled(!matches!(cmd, "path.fill" | "path.stroke") || can_paint).into())
        .collect();
    if let Some((cmd, params)) = ContextMenu::new(&entries).show(ui, &resp).chosen {
        if cmd == "path.stroke" {
            // Stroke Path… asks for the tool first, as in Photoshop (#2978).
            out.stroke_dialog = Some(params.get("name").and_then(Value::as_str).unwrap_or("work").to_string());
        } else if cmd == "path.rename" && params.get("to").is_none() {
            ui.data_mut(|d| d.insert_temp(rename_id, true));
        } else {
            out.action = Some((cmd, params));
        }
    }
}

pub fn paths_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(st) = app.session.active() else {
        ui.label(egui::RichText::new(tl!("No document")).color(t.text_faint));
        return;
    };
    let doc = st.doc.clone();
    let st_layer = st.active_layer;
    let rows = path_rows(&doc, st_layer);
    let has_layer_path = rows.iter().any(|r| r.kind == PathRow::Layer);
    let mut out = PanelOut::default();
    let ctx = ui.ctx().clone();
    let dragged = egui::DragAndDrop::payload::<PathDrag>(&ctx);
    // The buttons sit in a footer at the panel's bottom, like Photoshop's.
    let tokens = LibTokens::of(ui);
    let footer = PanelFooter::reserved_height(&PanelFooter::style(&tokens, ui.pixels_per_point())) + ui.spacing().item_spacing.y;
    let fill = ui.available_height() > footer + 60.0;
    let rows_h = if fill { ui.available_height() - footer } else { f32::INFINITY };
    egui::ScrollArea::vertical().id_salt("path-rows").max_height(rows_h).min_scrolled_height(if fill { rows_h } else { 0.0 }).auto_shrink([false, !fill]).show(
        ui,
        |ui| {
            if rows.is_empty() {
                ui.label(egui::RichText::new(tl!("Draw with the Pen tool (P) or make a work path from a selection.")).color(t.text_faint).size(11.5));
            }
            for entry in &rows {
                show_row(app, ui, &doc, st_layer, entry, &mut out);
            }
            // Clicking the empty area deselects the path, as in Photoshop: new fill and
            // adjustment layers then take the selection as their mask instead (#1419).
            let rest = ui.available_rect_before_wrap();
            let rest = rest.intersect(Rect::from_min_size(rest.min, vec2(rest.width(), rest.height().min(4000.0))));
            if rest.is_positive() && ui.interact(rest, ui.scope_id().with("paths-empty"), egui::Sense::click()).clicked() {
                app.ui.selected_path = None;
            }
        },
    );
    // The dragged row follows the cursor.
    if let Some(drag) = &dragged
        && let Some(p) = ctx.input(|i| i.pointer.interact_pos())
    {
        crate::layer_transfer::show_ghost(ui, p, &drag.label);
    }
    let sel = app.ui.selected_path.clone().filter(|s| s != "layer" || has_layer_path).unwrap_or_else(|| "work".into());
    let buttons = PanelFooter::show(ui, |ui| {
        let fg = hex(app.session.tools.foreground);
        let mut n = doc.paths.len() + 1;
        while doc.paths.iter().any(|p| p.name == format!("Path {n}")) {
            n += 1;
        }
        let items: [(Icon, &str, &str, Value); 7] = [
            (Icon::PaintBucket, tl!("Fill path with foreground color"), "path.fill", json!({"name": sel, "color": fg})),
            (Icon::Circle, tl!("Stroke path with brush"), "path.stroke", json!({"name": sel, "tool": "brush"})),
            (Icon::MarqueeRect, tl!("Load path as a selection"), "path.toSelection", json!({"name": sel})),
            (Icon::Spline, tl!("Make work path from selection"), "select.toWorkPath", json!({"tolerance": 2.0})),
            (Icon::Square, tl!("Add vector mask"), "layer.vectorMask.fromPath", json!({"name": sel})),
            (Icon::Plus, tl!("Create new path"), "path.set", json!({"name": format!("Path {n}"), "path": {"subpaths": []}})),
            (Icon::Trash, tl!("Delete path"), "path.delete", json!({"name": sel})),
        ];
        // The layer's own path: deleting it deletes the vector mask (a shape keeps its path).
        let shape = rows.iter().any(|r| r.kind == PathRow::Layer && r.name.ends_with(" Shape Path"));
        let delete_layer_path = sel == "layer" && !shape;
        let items = items.map(|(icon, tip, cmd, p)| {
            if cmd == "path.delete" && delete_layer_path {
                (icon, tl!("Delete vector mask"), "layer.vectorMask.delete", json!({}))
            } else {
                (icon, tip, cmd, p)
            }
        });
        // Laid out from the right, so the last button goes first.
        let mut buttons = Rect::NOTHING;
        for (icon, tip, cmd, p) in items.into_iter().rev() {
            let b = PanelFooter::button(ui, icon, tip);
            if b.clicked() {
                if cmd == "path.stroke" && ui.input(|i| i.modifiers.alt) {
                    // Alt-click opens the Stroke Path dialog, as in Photoshop (#2978).
                    out.stroke_dialog = Some(sel.clone());
                } else {
                    out.action = Some((cmd, p));
                }
            }
            // A row dropped on the trash deletes that row's path, selected or not (#6553).
            if matches!(cmd, "path.delete" | "layer.vectorMask.delete")
                && let Some(drag) = &dragged
                && crate::panels::dropped_on(ui, &b)
                && let Some(entry) = drag.items.first().and_then(|k| rows.iter().find(|r| r.key() == *k))
                && let Some(drop) = entry.delete_command()
            {
                out.action = Some(drop);
            }
            buttons = buttons.union(b.rect);
        }
        buttons
    });
    ctx_data_footer(ui.ctx(), buttons);
    let PanelOut { action, stroke_dialog } = out;
    if let Some(name) = stroke_dialog {
        crate::canvas_tool_menu::stroke_path_dialog(app, &name);
    }
    if let Some((cmd, p)) = action {
        let renamed = if cmd == "path.rename" { p.get("to").and_then(Value::as_str).map(str::to_string) } else { None };
        match app.run(cmd, p) {
            // The renamed (or just saved) path stays the panel's target.
            Ok(_) => {
                if renamed.is_some() {
                    app.ui.selected_path = renamed;
                }
            }
            Err(e) => {
                app.ui.status = e;
                app.ui.status_error = true;
            }
        }
    }
}
