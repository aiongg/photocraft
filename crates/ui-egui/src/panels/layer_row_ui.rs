//! The Layers panel's flyout menu, and the row text helpers other panels share: a label cut to
//! its room with an ellipsis.

use egui::{Align2, FontId, Galley, Painter, Rect, pos2};
use photocraft_doc::{Adjustment, Layer, LayerContent};
use serde_json::json;
use std::sync::Arc;

/// The PSD key of the adjustment `l` holds when PhotoCraft keeps it as imported but cannot
/// render it.
pub fn unsupported_key(l: &Layer) -> Option<&str> {
    match &l.content {
        LayerContent::Adjustment(Adjustment::Unsupported { psd_key, .. }) => Some(psd_key),
        _ => None,
    }
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
    use crate::state::LayersPanelDialog;

    let has_doc = app.session.active().is_some();
    if ui.add_enabled(has_doc, egui::Button::new(tl!("New Layer…"))).clicked() {
        app.ui.layers_panel_dialog = Some(LayersPanelDialog::NewLayer(String::new()));
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
    run_menu_command(app, ui, tl!("Flatten Image"), "layer.flattenImage", Some(json!({})));
    ui.separator();
    let mut lock_layers = egui::Button::new(tl!("Lock Layers…"));
    if let Some(key) = crate::shortcuts::shortcut_label(app, "layer.lockLayers") {
        lock_layers = lock_layers.shortcut_text(key);
    }
    if ui.add_enabled(app.session.is_enabled("layer.lockLayers"), lock_layers).clicked() {
        app.ui.layers_panel_dialog = Some(LayersPanelDialog::LockLayers { transparency: false, pixels: false, position: false, artboard: false, all: true });
        ui.close();
    }
    run_menu_command(app, ui, tl!("Rename Layer…"), "layer.renameLayer", Some(json!({})));
    let any_hidden = app.session.active().is_some_and(|st| st.selected_layers().iter().any(|id| st.doc.layer(*id).is_some_and(|l| !l.visible)));
    if any_hidden {
        run_menu_command(app, ui, tl!("Show Layers"), "layer.showLayers", Some(json!({})));
    } else {
        run_menu_command(app, ui, tl!("Hide Layers"), "layer.hideLayers", Some(json!({})));
    }
    ui.separator();
    run_menu_command(app, ui, tl!("New Artboard…"), "layer.new.artboard", Some(json!({})));
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
        let mut options = app.ui.layers_panel_options.clone();
        options.thumbnail_contents = crate::layers_panel_ui::thumbnail_contents(app);
        app.ui.layers_panel_dialog = Some(LayersPanelDialog::PanelOptions(options));
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
    if crate::widgets::checkbox(ui, &mut clip, tl!("Clip Thumbnails to Layer Bounds")).changed() {
        if let Err(e) = app.run("prefs.set", json!({"path": "interface.clipThumbnailsToLayerBounds", "value": clip})) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
        ui.close();
    }
}
