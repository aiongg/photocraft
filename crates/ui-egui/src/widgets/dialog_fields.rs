//! Library fields over a dialog's JSON `fields` map: each edits a plain value in place and reports
//! whether the user changed it.

use craft::ui::controls::{NumberField, TextField};
use craft::ui::localized::{LocaleNumbers, Unit};
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
    number_field_parsed(ui, id_salt, label, value, range, unit, decimals, decimals, false, |text| super::numeric_expression::parse(text))
}

/// A size field in `units.unit` with no suffix of its own (a unit menu sits beside it): typed
/// arithmetic applies, and a number may carry a unit (`2in`, `5mm`) that converts at `units`'
/// resolution. The field shows `decimals` places (the unit's precision, #2434) but commits two
/// more of what the user types, so `72.27 pt` stays one inch.
pub fn number_field_in_units(
    ui: &mut Ui,
    id_salt: impl AsIdSalt,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    decimals: usize,
    units: super::FieldUnits,
) -> NumberEdit {
    let read = move |text: &str| {
        let typed = text.split_whitespace().collect::<String>();
        super::numeric_expression::parse_with_units(
            &typed,
            Some(super::numeric_expression::Units { unit: Some(units.unit), suffix: "", dpi: units.dpi, ppi: units.ppi, extent: None }),
        )
    };
    let before = *value;
    let mut edit = number_field_parsed(ui, id_salt, label, value, range, "", decimals, decimals + if decimals > 0 { 2 } else { 0 }, true, read);
    edit.changed = *value != before;
    edit
}

#[allow(clippy::too_many_arguments)]
fn number_field_parsed(
    ui: &mut Ui,
    id_salt: impl AsIdSalt,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    unit: &str,
    decimals: usize,
    commit_decimals: usize,
    ungrouped: bool,
    parse: impl Fn(&str) -> Option<f64>,
) -> NumberEdit {
    // Pixel sizes and the sizes of a unit menu's field read `1920`, not `1,920`.
    let numbers = if unit.contains("px") || ungrouped { (*LocaleNumbers::of(ui)).clone().ungrouped() } else { (*LocaleNumbers::of(ui)).clone() };
    let edit = NumberField::new(id_salt, label, *value, &numbers)
        .range(f64::from(*range.start())..=f64::from(*range.end()))
        .decimals(0..=decimals)
        .commit_decimals(commit_decimals)
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
