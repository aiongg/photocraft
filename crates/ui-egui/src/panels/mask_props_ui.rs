//! Non-destructive layer-mask controls (pixel and vector), shared by docked and floating Properties.

use craft::ui::composites::SliderRow;
use craft::ui::edit::EditEvent;
use craft::ui::localized::{LocaleNumbers, Unit};
use craft::ui::theme::ToEgui;
use photocraft_doc::{Layer, LayerContent};
use serde_json::json;

use crate::PhotocraftApp;
use crate::props_layout::COL_GAP;
use crate::theme;

/// Image › Adjustments › Invert, which inverts a targeted layer mask in place (#780).
const INVERT: &str = "image.adjustments.invert";

/// The Layers panel targets the active layer's pixel mask (not its vector mask) and Properties
/// shows it: a just-created adjustment or fill layer shows its own settings instead (#3369).
pub fn targeted(app: &PhotocraftApp, layer: &Layer) -> bool {
    app.ui.mask_target && !app.ui.vector_mask_target && layer.mask.is_some() && app.ui.properties_layer != Some(layer.id.0)
}

/// The Layers panel targets the active layer's vector mask.
pub fn vector_targeted(app: &PhotocraftApp, layer: &Layer) -> bool {
    app.ui.vector_mask_target && layer.vector_mask.is_some()
}

/// The pixel-mask section: Density, Feather and the Refine row. Pixels stay intact; the two
/// properties ride on the mask and are applied by the compositor.
pub fn properties(app: &mut PhotocraftApp, ui: &mut egui::Ui, layer: &Layer) {
    let Some(mask) = &layer.mask else { return };
    let Some(document) = app.session.active().map(|s| s.doc.id.0) else { return };
    ui.push_id((document, layer.id, "pixel-mask-properties"), |ui| {
        if !crate::props_layout::section(ui, "pixel-mask", "Layer Mask") {
            return;
        }
        let (density, feather) = (mask.density, mask.feather);
        sliders(app, ui, layer.id.0, document, "layer.layerMask.edit", "pixel-mask", density, feather);
        refine_row(app, ui);
    });
}

/// The vector-mask section: Density and Feather, likewise non-destructive.
pub fn vector_properties(app: &mut PhotocraftApp, ui: &mut egui::Ui, layer: &Layer) {
    let Some(vm) = &layer.vector_mask else { return };
    let Some(document) = app.session.active().map(|s| s.doc.id.0) else { return };
    ui.push_id((document, layer.id, "vector-mask-properties"), |ui| {
        if !crate::props_layout::section(ui, "vector-mask", "Vector Mask") {
            return;
        }
        let (density, feather) = (vm.density, vm.feather);
        sliders(app, ui, layer.id.0, document, "layer.vectorMask.edit", "vector-mask", density, feather);
    });
}

/// A shape layer's path is its vector mask (Photoshop's Properties › Masks): its Density and
/// Feather soften the fill through `layer.vectorMask.edit`, the shape staying editable.
pub fn shape_properties(app: &mut PhotocraftApp, ui: &mut egui::Ui, layer: &Layer) {
    let LayerContent::Shape(sh) = &layer.content else { return };
    if layer.vector_mask.is_some() {
        return;
    }
    let Some(document) = app.session.active().map(|s| s.doc.id.0) else { return };
    ui.push_id((document, layer.id, "shape-mask-properties"), |ui| {
        if !crate::props_layout::section(ui, "shape-mask", "Vector Mask") {
            return;
        }
        let (density, feather) = (sh.density, sh.feather);
        sliders(app, ui, layer.id.0, document, "layer.vectorMask.edit", "shape-mask", density, feather);
    });
}

/// Density (%) and Feather (px) slider rows writing one undoable edit per gesture: the value lives
/// on the mask, not in its pixels. `command` is the mask's edit command and `prefix` keys its
/// coalescing.
#[allow(clippy::too_many_arguments)]
fn sliders(app: &mut PhotocraftApp, ui: &mut egui::Ui, layer: u64, document: u64, command: &str, prefix: &str, density: f32, feather: f32) {
    let numbers = LocaleNumbers::of(ui);
    // Feather's track is curved like Photoshop's, so a few px can be dragged to (#6545).
    for (field, label, value, max, unit, curve) in [
        ("density", tl!("Density"), f64::from(density) * 100.0, 100.0, "{n} %", None),
        ("feather", tl!("Feather"), f64::from(feather), 1000.0, "{n} px", Some(3.0)),
    ] {
        let mut row = SliderRow::new((prefix, field), label, value, 0.0..=max, &numbers).unit(Unit::new(unit)).fill(false).field_width(74.0);
        if let Some(k) = curve {
            row = row.curve(k);
        }
        for event in row.show(ui).edits() {
            // A drag, a scrub or typing is one gesture and one history step; a key or wheel step
            // is its own.
            let (value, gesture) = match event {
                EditEvent::Update { id, value, .. } => (value, id.to_string()),
                EditEvent::Commit { id, value, changed, .. } => {
                    if !changed {
                        continue;
                    }
                    (value, id.to_string())
                }
                EditEvent::Set { value, .. } => (value, format!("set{}", ui.ctx().cumulative_pass_nr())),
                _ => continue,
            };
            let mut params = json!({"document": document, "layer": layer,
                "coalesce": format!("{prefix}:{document}:{layer}:{field}:{gesture}")});
            params[field] = json!(value);
            if let Err(error) = app.run(command, params) {
                app.ui.status = error;
                app.ui.status_error = true;
            }
        }
    }
}

/// Photoshop's Refine row below Density and Feather: Select and Mask… and Color Range… open their
/// dialogs (the mask is Select and Mask's input when nothing is selected), and Invert flips the
/// mask. Invert names its target itself (`"target":"mask"`) rather than leaning on the shell's
/// target routing, so it edits the mask and never the layer's pixels (#1137).
fn refine_row(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let refine: [(&str, &str); 3] = [(tl!("Select and Mask…"), "select.selectAndMask"), (tl!("Color Range…"), "select.colorRange"), (tl!("Invert"), INVERT)];
    ui.add_space(theme::ROW_GAP / 2.0);
    ui.label(egui::RichText::new(tl!("Refine")).color(craft::ui::theme::current(ui.ctx()).color.text.muted.to_egui()));
    ui.add_space(theme::ROW_GAP / 2.0);
    let avail = ui.available_width();
    // Two buttons to a row in a wide panel, as Quick Actions do; a narrow one stacks them.
    let cols = if avail >= 360.0 { 2 } else { 1 };
    for row in refine.chunks(cols) {
        // A short last row (or a lone button) still spans the panel.
        let w = ((avail - (row.len() - 1) as f32 * COL_GAP) / row.len() as f32).floor();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = COL_GAP;
            for &(label, id) in row {
                // Greyed when its command can't run, by the same rule as the menu item.
                let enabled = crate::menus::is_enabled(app, id);
                if ui
                    .add_enabled_ui(enabled, |ui| {
                        ui.add(craft::ui::controls::Button::new(label).variant(craft::ui::controls::ButtonVariant::Secondary).min_width(w)).clicked()
                    })
                    .inner
                {
                    let params = if id == INVERT { json!({"target": "mask"}) } else { json!({}) };
                    if let Err(error) = crate::menus::invoke(app, ui.ctx(), id, params) {
                        app.ui.status = error;
                        app.ui.status_error = true;
                    }
                }
            }
        });
        ui.add_space(theme::ROW_GAP);
    }
}
