//! Library fields over a dialog's JSON `fields` map: each edits a plain value in place and reports
//! whether the user changed it.

use craft_ui::controls::{NumberField, TextField};
use craft_ui::localized::{LocaleNumbers, Unit};
use egui::{AsIdSalt, Ui};

/// A [`NumberField`] named `label` over `value`, clamped to `range`, showing `decimals` places and
/// `unit` (a template such as `"{n}%"`, or a bare suffix). Typed arithmetic (`1920/2`) applies as
/// in every PhotoCraft number field. True when the user changed the value.
pub fn number(ui: &mut Ui, id_salt: impl AsIdSalt, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, unit: &str, decimals: usize) -> bool {
    number_field(ui, id_salt, label, value, range, unit, decimals).changed
}

/// What [`number_field`] reports.
pub struct NumberEdit {
    pub changed: bool,
    pub response: egui::Response,
}

/// [`number`] returning the response too, for a row that names its control by it.
pub fn number_field(
    ui: &mut Ui,
    id_salt: impl AsIdSalt,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    unit: &str,
    decimals: usize,
) -> NumberEdit {
    let numbers = LocaleNumbers::of(ui);
    let parse = |text: &str| super::numeric_expression::parse(text);
    let edit = NumberField::new(id_salt, label, *value, &numbers)
        .range(f64::from(*range.start())..=f64::from(*range.end()))
        .decimals(0..=decimals)
        .unit(Unit::new(unit))
        .or_parse(parse)
        .show(ui);
    let mut changed = false;
    if let Some(v) = edit.event.as_ref().and_then(|e| e.value()).copied() {
        let v = v as f32;
        if v != *value {
            *value = v;
            changed = true;
        }
    }
    NumberEdit { changed, response: edit.response }
}

/// A single-line [`TextField`] named `label` over `text`; `changed` is set when the user changed the text.
pub fn text(ui: &mut Ui, id_salt: impl AsIdSalt, label: &str, text: &mut String) -> TextEdit {
    let edit = TextField::new(id_salt, label, text).show(ui);
    let mut changed = false;
    if let Some(v) = edit.event.as_ref().and_then(|e| e.value()) {
        if v != text {
            *text = v.clone();
            changed = true;
        }
    }
    TextEdit { changed, response: edit.response }
}

/// What [`text`] reports.
pub struct TextEdit {
    pub changed: bool,
    pub response: egui::Response,
}

/// [`text`] for a dialog that opens on its name: the first time the field has the focus (the host
/// focuses the first field), its text is selected so typing replaces it, as Photoshop does. `fields`
/// holds the private `__named` marker that says it happened.
pub fn text_opening(ui: &mut Ui, id_salt: impl AsIdSalt, label: &str, value: &mut String, fields: &mut serde_json::Map<String, serde_json::Value>) -> TextEdit {
    let edit = text(ui, id_salt, label, value);
    if !fields.contains_key("__named") && !ui.is_sizing_pass() && edit.response.has_focus() {
        super::field_tab::select_all(ui.ctx(), edit.response.id, value);
        fields.insert("__named".into(), serde_json::Value::Bool(true));
    }
    edit
}

/// The response a `FormGrid` row returns for a control that keeps its own accessible name (a
/// colour swatch named "Fill color"): the row would otherwise replace the name with its visible
/// label. This is a nodeless stand-in at the control's position, so the row has nothing to name.
pub fn keep_own_name(ui: &mut Ui, _control: &egui::Response) -> egui::Response {
    ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover())
}
