//! Image › Mode › Bitmap… (#5559), laid out like Photoshop's dialog: Resolution (Input, read-only,
//! and Output, in pixels/inch or pixels/centimeter), Method (Use: 50% Threshold, Pattern Dither,
//! Diffusion Dither, Halftone Screen, Custom Pattern) with the Custom Pattern picker, enabled only
//! for Custom Pattern. The Halftone Screen settings (Frequency in lines/inch or lines/centimeter,
//! Angle, Shape), which Photoshop asks for in a second dialog after OK, show here only while
//! Halftone Screen is the method.
//!
//! It is the hosted `ParamDialog` of `image.mode.bitmap` with custom rows for the unit fields and
//! the pattern picker: the fields are the command's params (`resolution` in pixels/inch,
//! `frequency` in lines/inch), so OK, the live preview, `ui.dialog.set` and `ui.dialog.confirm` work
//! as for every schema dialog. The units are view state kept in the dialog's values
//! (`__resUnit`, `__freqUnit`). The pattern picker draws from a snapshot of the pattern library
//! taken when the dialog opens.

use std::sync::Arc;

use craft::ui::dialog::{DialogFields, ParamKind as Lib};
use craft::ui::localized::Unit;
use craft::ui::theme::ToEgui;
use egui::Response;
use photocraft_engine::pattern_cmds::PatternLibrary;
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;

type LibParam = craft::ui::dialog::Param;

pub(crate) const COMMAND: &str = "image.mode.bitmap";

/// Centimetres per inch.
const CM: f32 = 2.54;
/// The command's Output resolution range (pixels/inch).
const RESOLUTION: (f32, f32) = (1.0, 30_000.0);
/// The command's Halftone Screen frequency range (lines/inch).
const FREQUENCY: (f32, f32) = (1.0, 999.0);

const METHODS: [(&str, &str); 5] = [
    ("threshold", "50% Threshold"),
    ("pattern", "Pattern Dither"),
    ("diffusion", "Diffusion Dither"),
    ("halftone", "Halftone Screen"),
    ("customPattern", "Custom Pattern"),
];

/// Photoshop's order of the Halftone Screen shapes.
const SHAPES: [(&str, &str); 6] =
    [("round", "Round"), ("diamond", "Diamond"), ("ellipse", "Ellipse"), ("line", "Line"), ("square", "Square"), ("cross", "Cross")];

/// Open defaults: Output starts at the document's resolution (Input, as in Photoshop), Custom
/// Pattern at the Patterns panel's pattern.
pub(crate) fn defaults(app: &PhotocraftApp, f: &mut Map<String, Value>) {
    let input = app.session.active().map_or(72.0, |d| d.doc.resolution_dpi);
    f.insert("__inputResolution".into(), json!(input));
    f.insert("resolution".into(), json!(input));
    match photocraft_engine::pattern_cmds::resolve(&app.session, "") {
        Some(p) => f.insert("pattern".into(), json!(p.id)),
        None => f.remove("pattern"),
    };
}

const RES_UNIT: &str = "__resUnit";
const FREQ_UNIT: &str = "__freqUnit";

fn num(f: &DialogFields, key: &str, default: f32) -> f32 {
    f.number(key).map_or(default, |v| v as f32)
}

fn text<'a>(f: &'a DialogFields, key: &str, default: &'a str) -> &'a str {
    f.text(key).unwrap_or(default)
}

/// The view state the rows keep in the dialog's values: the units start at per inch.
pub(crate) fn view_state(fields: &mut DialogFields) {
    fields.set(RES_UNIT, "ppi");
    fields.set(FREQ_UNIT, "lpi");
}

/// A value stored in per-inch units, edited in per-inch or per-centimeter ones: `field` holds the
/// per-inch value, `unit` the view's unit (`per_cm` when its value is per centimeter).
// The explicit accessible name accompanies the existing numeric-field parameters.
#[expect(clippy::too_many_arguments)]
fn per_unit_row(
    ui: &mut egui::Ui,
    f: &mut DialogFields,
    field: &str,
    name: &str,
    range: (f32, f32),
    default: f32,
    unit: &str,
    units: [(&str, &str); 2],
) -> Response {
    let per_inch = num(f, field, default);
    let mut cur = text(f, unit, units[0].0).to_owned();
    let k = if cur == units[1].0 { CM } else { 1.0 };
    let mut shown = per_inch / k;
    let mut response = None;
    ui.horizontal(|ui| {
        let r = crate::widgets::value_field(ui, &mut shown, range.0 / k..=range.1 / k, "", 70.0).accessible_name(name);
        if r.changed() {
            f.set(field, ((shown * k).clamp(range.0, range.1) * 1000.0).round() as f64 / 1000.0);
        }
        response = Some(r);
        let opts: Vec<(String, &str)> = units.iter().map(|(id, l)| (id.to_string(), *l)).collect();
        if crate::widgets::dropdown(ui, &format!("bitmap-{unit}"), tl!("Units"), &mut cur, &opts, 150.0) {
            f.set(unit, cur.as_str());
        }
    });
    response.expect("the row draws its field")
}

/// The rows of the dialog. Photoshop greys the pattern out unless the method is Custom Pattern;
/// the Halftone Screen settings, which it asks for in a second dialog, are greyed unless the
/// method is Halftone Screen. `input` is the document's resolution in pixels/inch.
pub(crate) fn params(library: Arc<PatternLibrary>, input: f32) -> Vec<LibParam> {
    let res_units = [("ppi", tl!("Pixels/Inch")), ("ppcm", tl!("Pixels/Centimeter"))];
    let freq_units = [("lpi", tl!("Lines/Inch")), ("lpcm", tl!("Lines/Centimeter"))];
    let choices = |list: &[(&str, &str)]| list.iter().map(|(id, label)| ((*id).to_owned(), (*label).to_owned())).collect::<Vec<_>>();
    vec![
        LibParam::custom("input", "Input", move |ui, f| {
            let t = craft::ui::theme::current(ui.ctx());
            let (v, unit) = if text(f, RES_UNIT, "ppi") == "ppcm" { (input / CM, res_units[1].1) } else { (input, res_units[0].1) };
            ui.label(egui::RichText::new(format!("{} {unit}", crate::widgets::fmt_num_prec(f64::from(v), 3))).color(t.color.text.normal.to_egui()))
        }),
        LibParam::custom("resolutionRow", "Output", move |ui, f| per_unit_row(ui, f, "resolution", tl!("Resolution"), RESOLUTION, input, RES_UNIT, res_units)),
        LibParam::new("method", "Use", Lib::Choice { options: choices(&METHODS), default: Some("diffusion".into()) }),
        LibParam::custom("patternRow", "Custom Pattern", move |ui, f| {
            let t = craft::ui::theme::current(ui.ctx());
            if library.items.is_empty() {
                return ui.label(egui::RichText::new(tl!("No patterns")).color(t.color.text.faint.to_egui()));
            }
            let (response, picked) = crate::preset_panels::pattern_swatch(&library, ui, text(f, "pattern", ""));
            if let Some(id) = picked {
                f.set("pattern", id);
            }
            response
        })
        .enabled_when("method", "customPattern"),
        LibParam::custom("frequencyRow", "Frequency", move |ui, f| per_unit_row(ui, f, "frequency", tl!("Frequency"), FREQUENCY, 53.0, FREQ_UNIT, freq_units))
            .enabled_when("method", "halftone"),
        LibParam::number("angle", "Angle", 45.0, 1.0, 1).bounds(Some(-180.0), Some(180.0)).unit(Unit::new("°")).enabled_when("method", "halftone"),
        LibParam::new("shape", "Shape", Lib::Choice { options: choices(&SHAPES), default: Some("round".into()) }).enabled_when("method", "halftone"),
    ]
}

#[cfg(test)]
mod tests;
