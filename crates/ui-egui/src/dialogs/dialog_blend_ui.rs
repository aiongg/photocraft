//! Blending controls shared by Edit › Fill and Edit › Stroke.
//!
//! Both commands support the same layer blend modes, opacity and transparency lock.
//! Keeping one widget and validation implementation prevents their options diverging.

use craft::ui::composites::{FormGrid, SectionLabel};
use craft::ui::controls::{Checkbox, Select};
use craft::ui::primitives::Divider;
use serde_json::{Map, Value, json};

/// Validate a persisted blending choice before restoring it from Preferences.
pub fn valid(key: &str, v: &Value) -> bool {
    match key {
        "mode" => v.as_str().and_then(photocraft_engine::commands::blend_from_str).is_some_and(|m| m != photocraft_color::BlendMode::PassThrough),
        "opacity" => v.as_f64().is_some_and(|o| o.is_finite() && (0.0..=100.0).contains(&o)),
        "preserveTransparency" => v.is_boolean(),
        _ => false,
    }
}

/// The Blending section of Fill and Stroke: Mode, Opacity and Preserve Transparency.
pub fn body(ui: &mut egui::Ui, fields: &mut Map<String, Value>, prefix: &str) {
    ui.add_space(craft::ui::theme::scale::Space::S4.px());
    SectionLabel::new(tl!("Blending")).show(ui);
    Divider::horizontal().show(ui);
    FormGrid::new(format!("{prefix}-blending")).min_label_width(LABEL_WIDTH).show(ui, |form| {
        form.row(tl!("Mode:"), |ui, _| {
            let mut mode = fields.get("mode").and_then(Value::as_str).unwrap_or("normal").to_string();
            let opts: Vec<(String, &str)> = photocraft_color::BlendMode::LAYER_MODES.iter().map(|m| (mode_key(*m), m.label())).collect();
            let edit = Select::new(format!("{prefix}-mode"), tl!("Blend Mode"), &mut mode).rows(opts.len(), |i| opts[i].0.clone(), |i| tl!(opts[i].1)).show(ui);
            if edit.response.changed() {
                fields.insert("mode".into(), json!(mode));
            }
            edit
        });
        form.row(tl!("Opacity:"), |ui, _| {
            let mut opacity = fields.get("opacity").and_then(Value::as_f64).unwrap_or(100.0) as f32;
            let edit = crate::widgets::dialog_number_field(ui, format!("{prefix}-opacity"), tl!("Opacity:"), &mut opacity, 0.0..=100.0, "{n}%", 0);
            if edit.changed {
                fields.insert("opacity".into(), json!(opacity.round()));
            }
            edit.response
        });
        form.unlabelled(|ui| {
            let mut preserve = fields.get("preserveTransparency").and_then(Value::as_bool).unwrap_or(false);
            let edit = Checkbox::bool(tl!("Preserve Transparency"), &mut preserve).show(ui);
            if edit.response.changed() {
                fields.insert("preserveTransparency".into(), json!(preserve));
            }
            edit
        });
    });
}

/// The label column of Fill, Stroke and their Blending section: all grids share it so their
/// controls line up.
pub(crate) const LABEL_WIDTH: f32 = 112.0;

/// The `mode` param naming a blend mode ("normal", "colorBurn"…), parsed by the engine.
pub(crate) fn mode_key(m: photocraft_color::BlendMode) -> String {
    let s = format!("{m:?}");
    let mut c = s.chars();
    c.next().map(|f| f.to_ascii_lowercase().to_string() + c.as_str()).unwrap_or_default()
}
