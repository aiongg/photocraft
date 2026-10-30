//! Layers panel row support: the right-hand indicators' widths, the rects each row drew
//! ([`recorded`], for tests and automation), the in-place rename state and the panel's flyout.
//!
//! The row itself is a craft-ui `ListRow` ([`layer_row`](super::layer_row)); its trailing items
//! drop out and its label is cut by the library.

use craft_ui::list::{ListRow, RenameEnd, RowEvent};
use egui::{Align2, FontId, Galley, Painter, Rect, pos2};
use photocraft_doc::{Adjustment, Layer, LayerContent};
use serde_json::{Value, json};
use std::sync::Arc;

/// Space kept free at a row's right edge: the overlay scrollbar sits there.
pub const RIGHT_PAD: f32 = 10.0;
/// Width of an icon indicator (lock, link).
pub(super) const ICON_W: f32 = 14.0;
/// Width of the effects triangle beside the fx badge.
pub(super) const TRIANGLE_W: f32 = 10.0;
/// The name keeps at least this much room.
pub const MIN_NAME_W: f32 = 64.0;

/// A right-hand row indicator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Indicator {
    /// An adjustment PhotoCraft keeps as imported but cannot render (#6541).
    Unsupported,
    Lock,
    FxTriangle,
    Fx,
    SmartFilters,
    Link,
    Blend,
}

/// Where one row's name and indicators went this frame (screen points).
#[derive(Clone, Debug)]
pub struct RowRects {
    pub layer: u64,
    pub row: Rect,
    /// Visible layer thumbnail; None when thumbnails are disabled.
    pub thumbnail: Option<Rect>,
    /// The name as painted (after truncation); `None` when there was no room for it.
    pub name: Option<Rect>,
    pub indicators: Vec<(Indicator, Rect)>,
}

/// The PSD key of the adjustment `l` holds when PhotoCraft keeps it as imported but cannot
/// render it.
pub fn unsupported_key(l: &Layer) -> Option<&str> {
    match &l.content {
        LayerContent::Adjustment(Adjustment::Unsupported { psd_key, .. }) => Some(psd_key),
        _ => None,
    }
}

/// Group indentation for a row at `depth`: 14 pt a level, squeezed when the panel is so narrow
/// that it would leave the name less than [`MIN_NAME_W`] (`fixed` = everything else in the row).
pub fn indent(depth: usize, row_width: f32, fixed: f32) -> f32 {
    (depth as f32 * 14.0).min((row_width - fixed - MIN_NAME_W).max(0.0))
}

/// A single-line galley cut with "…" to `max_width` (nothing when there's no room).
pub fn truncated(painter: &Painter, text: &str, font: FontId, color: egui::Color32, italics: bool, max_width: f32) -> Option<Arc<Galley>> {
    if max_width < 8.0 {
        return None;
    }
    let format = egui::text::TextFormat { font_id: font, color, italics, ..Default::default() };
    let layout = |s: &str, wrap: bool| {
        let mut job = egui::text::LayoutJob::default();
        job.append(s, 0.0, format.clone());
        if wrap {
            job.wrap = egui::text::TextWrapping { max_width, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
        }
        painter.layout_job(job)
    };
    // A right-to-left name (Arabic, Hebrew) too long for the row is cut at its logical end, so its
    // start stays and "…" is at the left (`bidi_text::elide`).
    if let Some(shown) = crate::bidi_text::elide(text, |s| layout(s, false).size().x <= max_width) {
        return Some(layout(&shown, false));
    }
    // egui puts a right-to-left name into display order itself; left-to-right names are cut at
    // their right end by egui.
    Some(layout(text, true))
}

/// A small disclosure triangle: pointing down when open, right when closed.
pub(super) fn paint_triangle(painter: &Painter, c: egui::Pos2, open: bool, color: egui::Color32) {
    let s = 3.0;
    let pts = if open {
        vec![pos2(c.x - s, c.y - s * 0.6), pos2(c.x + s, c.y - s * 0.6), pos2(c.x, c.y + s * 0.6)]
    } else {
        vec![pos2(c.x - s * 0.6, c.y - s), pos2(c.x + s * 0.6, c.y), pos2(c.x - s * 0.6, c.y + s)]
    };
    painter.add(egui::Shape::convex_polygon(pts, color, egui::Stroke::NONE));
}

/// Paint `text` left-aligned at `x`, vertically centred on `cy`, cut to end before `right`.
/// Returns the painted rect.
pub fn label(painter: &Painter, x: f32, cy: f32, right: f32, text: &str, font: FontId, color: egui::Color32) -> Option<Rect> {
    let g = truncated(painter, text, font, color, false, right - x)?;
    let r = Align2::LEFT_CENTER.anchor_size(pos2(x, cy), g.size());
    painter.galley(r.min, g, color);
    Some(r)
}

/// Photoshop-style Layers flyout commands. The dock owns tab-group controls, not this menu.
pub fn panel_menu(app: &mut crate::PhotocraftApp, ui: &mut egui::Ui) {
    use crate::layers_panel_ui::run_menu_command;

    let has_doc = app.session.active().is_some();
    if ui.add_enabled(has_doc, egui::Button::new(tl!("New Layer…"))).clicked() {
        // The New Layer dialog, as for ⌥-click on the panel's New Layer button.
        let ctx = ui.ctx().clone();
        if let Err(e) = crate::menus::invoke(app, &ctx, "layer.new.layer", Value::Null) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
        ui.close();
    }
    // New Group… and New Group from Layers… open the New Group dialog, like their Layer › New twins.
    for (label, id) in [(tl!("New Group…"), crate::new_group_ui::NEW_GROUP), (tl!("New Group from Layers…"), crate::new_group_ui::FROM_LAYERS)] {
        if ui.add_enabled(crate::menus::is_enabled(app, id), egui::Button::new(label)).clicked() {
            let ctx = ui.ctx().clone();
            if let Err(e) = crate::menus::invoke(app, &ctx, id, json!({})) {
                app.ui.status = e;
                app.ui.status_error = true;
            }
            ui.close();
        }
    }
    ui.separator();
    run_menu_command(app, ui, tl!("Flatten Image"), "layer.flattenImage");
    ui.separator();
    if ui.add_enabled(app.session.is_enabled("layer.lockLayers"), egui::Button::new(tl!("Lock Layers…"))).clicked() {
        crate::layers_panel_ui::open_lock(app);
        ui.close();
    }
    run_menu_command(app, ui, tl!("Rename Layer…"), "layer.renameLayer");
    let any_hidden = app.session.active().is_some_and(|st| st.selected_layers().iter().any(|id| st.doc.layer(*id).is_some_and(|l| !l.visible)));
    if any_hidden {
        run_menu_command(app, ui, tl!("Show Layers"), "layer.showLayers");
    } else {
        run_menu_command(app, ui, tl!("Hide Layers"), "layer.hideLayers");
    }
    ui.separator();
    run_menu_command(app, ui, tl!("New Artboard…"), "layer.new.artboard");
    ui.separator();
    ui.menu_button(tl!("Filter Options"), |ui| {
        let mut show = app.ui.layers_panel_options.show_filters;
        if ui.checkbox(&mut show, tl!("Show")).changed() {
            app.ui.layers_panel_options.show_filters = show;
            crate::layers_panel_ui::persist_options(app);
            ui.close();
        }
    });
    // Animation Options control Photoshop's frame-unification buttons. PhotoCraft's Layers
    // panel does not expose those buttons; do not offer nonfunctional toggles.
    ui.add_enabled_ui(false, |ui| {
        ui.menu_button(tl!("Animation Options"), |_ui| {});
    });
    if ui.button(tl!("Panel Options…")).clicked() {
        crate::layers_panel_ui::open_options(app);
        ui.close();
    }
    ui.separator();
    // Preserve PhotoCraft's existing group disclosure action when expanding the flyout.
    let can_collapse = app.session.is_enabled("layer.setExpanded");
    if ui.add_enabled(can_collapse, egui::Button::new(tl!("Collapse All Groups"))).clicked() {
        if let Err(e) = app.run("layer.setExpanded", json!({"all": true, "expanded": false})) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
        ui.close();
    }
    ui.separator();
    // Photoshop's Clip Thumbnails to Layer Bounds: frame each layer's pixels, not the canvas (#4281).
    let mut clip = app.session.prefs().interface.clip_thumbnails_to_layer_bounds;
    if craft_ui::controls::Checkbox::bool(tl!("Clip Thumbnails to Layer Bounds"), &mut clip).show(ui).response.changed() {
        if let Err(e) = app.run("prefs.set", json!({"path": "interface.clipThumbnailsToLayerBounds", "value": clip})) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
        ui.close();
    }
}

fn rects_id() -> egui::Id {
    egui::Id::unique("layer-row-rects")
}

/// Start a frame's record (the Layers panel calls it before drawing its rows).
pub fn begin(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(rects_id(), Vec::<RowRects>::new()));
}

pub fn record(ctx: &egui::Context, r: RowRects) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Vec<RowRects>>(rects_id()).push(r));
}

/// The rows the Layers panel drew last frame.
pub fn recorded(ctx: &egui::Context) -> Vec<RowRects> {
    ctx.data(|d| d.get_temp::<Vec<RowRects>>(rects_id())).unwrap_or_default()
}

// ------------------------------------------------------------------ in-place rename (#314)

/// The Layers panel's in-place rename. One at a time, like Photoshop: Enter, Tab or a click
/// anywhere else commits it, Esc cancels it, and starting another rename commits this one.
#[derive(Clone, Debug, PartialEq)]
pub struct Rename {
    pub layer: u64,
    /// The name when the rename started (unchanged text renames nothing).
    pub original: String,
    pub text: String,
    /// Focus was given to the field (only once: asking every frame kept it from ever losing focus).
    pub focused: bool,
    /// The pass its field was last drawn in.
    pub pass: u64,
}

impl Rename {
    /// The command that applies it, if it changes the name.
    pub fn commit_action(&self) -> Option<(String, Value)> {
        let name = self.text.trim();
        (!name.is_empty() && name != self.original).then(|| ("layer.setProps".to_string(), json!({"layer": self.layer, "name": name})))
    }
}

fn rename_id() -> egui::Id {
    egui::Id::unique("layer-rename")
}

/// The rename in progress, if any.
pub fn rename(ctx: &egui::Context) -> Option<Rename> {
    ctx.data(|d| d.get_temp::<Rename>(rename_id()))
}

/// The layer being renamed, if any.
pub fn renaming(ctx: &egui::Context) -> Option<u64> {
    rename(ctx).map(|r| r.layer)
}

/// Is a rename field on screen (drawn this pass or the last)? It owns the keyboard meanwhile.
pub fn rename_active(ctx: &egui::Context) -> bool {
    rename(ctx).is_some_and(|r| r.pass + 1 >= ctx.cumulative_pass_nr())
}

fn store(ctx: &egui::Context, r: Option<Rename>) {
    ctx.data_mut(|d| match r {
        Some(r) => {
            d.insert_temp(rename_id(), r);
        }
        None => {
            d.remove::<Rename>(rename_id());
        }
    });
}

/// Start renaming `layer` (now called `name`). A rename open on another layer is committed: its
/// command is returned for the caller to run.
pub fn start_rename(ctx: &egui::Context, layer: u64, name: &str) -> Option<(String, Value)> {
    let previous = rename(ctx);
    if previous.as_ref().is_some_and(|r| r.layer == layer) {
        return None;
    }
    store(ctx, Some(Rename { layer, original: name.to_string(), text: name.to_string(), focused: false, pass: ctx.cumulative_pass_nr() }));
    previous.and_then(|r| r.commit_action())
}

/// [`start_rename`] from a panel pass: the text typed into the rename it replaces is taken from
/// its row and committed with it (#314).
pub fn start_rename_in(ui: &egui::Ui, layer: u64, name: &str) -> Option<(String, Value)> {
    if let Some(mut previous) = rename(ui.ctx()).filter(|r| r.layer != layer) {
        if let Some(text) = ListRow::<u64>::take_rename_draft(ui, &previous.layer) {
            previous.text = text;
            store(ui.ctx(), Some(previous));
        }
    }
    start_rename(ui.ctx(), layer, name)
}

/// [`start_rename`] from a command that has the context only (a menu item): the text typed into
/// the rename it replaces is taken from the library's open rename and committed with it (#314).
pub fn start_rename_from_menu(ctx: &egui::Context, layer: u64, name: &str) -> Option<(String, Value)> {
    if rename(ctx).is_some_and(|r| r.layer != layer)
        && let Some((key, text)) = ListRow::<u64>::take_open_rename_draft(ctx)
        && let Some(mut previous) = rename(ctx).filter(|r| r.layer == key)
    {
        previous.text = text;
        store(ctx, Some(previous));
    }
    start_rename(ctx, layer, name)
}

/// End the rename in progress: commit it (its command is returned) or cancel it.
pub fn end_rename(ctx: &egui::Context, commit: bool) -> Option<(String, Value)> {
    let r = rename(ctx)?;
    store(ctx, None);
    if commit { r.commit_action() } else { None }
}

/// Where Tab or ⇧Tab sends a rename: the layer it left and whether the next one is above.
fn next_rename_id() -> egui::Id {
    egui::Id::unique("layer-rename-next")
}

/// The rename that Tab (`up` false) or ⇧Tab (`up` true) just left on `layer`, for the Layers panel
/// to continue on the layer below or above, as Photoshop does. Taken once.
pub fn take_next_rename(ctx: &egui::Context) -> Option<(u64, bool)> {
    ctx.data_mut(|d| d.remove_temp::<(u64, bool)>(next_rename_id()))
}

/// Applies what a row's rename field reported this frame, for the row of `layer` that is being
/// renamed: the text typed, and how the rename ended. Returns the rename command when it was
/// committed. Tab and ⇧Tab commit too and ask for the next rename ([`take_next_rename`]).
pub fn apply_rename_events(ctx: &egui::Context, layer: u64, events: &[RowEvent<u64>]) -> Option<(String, Value)> {
    let mut r = rename(ctx).filter(|r| r.layer == layer)?;
    r.pass = ctx.cumulative_pass_nr();
    r.focused = true;
    for event in events {
        match event {
            RowEvent::Renamed { row, text } if *row == layer => r.text.clone_from(text),
            RowEvent::RenameEnded { row, end } if *row == layer => {
                store(ctx, None);
                return match end {
                    RenameEnd::Escape => None,
                    RenameEnd::Tab | RenameEnd::ShiftTab => {
                        ctx.data_mut(|d| d.insert_temp(next_rename_id(), (layer, *end == RenameEnd::ShiftTab)));
                        r.commit_action()
                    }
                    _ => r.commit_action(),
                };
            }
            _ => {}
        }
    }
    store(ctx, Some(r));
    None
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod rename_tests;

#[cfg(test)]
mod edit_tests;
