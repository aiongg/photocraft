//! Image › Mode › Bitmap… (#5559), laid out like Photoshop's dialog: Resolution (Input, read-only,
//! and Output, in pixels/inch or pixels/centimeter), Method (Use: 50% Threshold, Pattern Dither,
//! Diffusion Dither, Halftone Screen, Custom Pattern) with the Custom Pattern picker, enabled only
//! for Custom Pattern. The Halftone Screen settings (Frequency in lines/inch or lines/centimeter,
//! Angle, Shape), which Photoshop asks for in a second dialog after OK, show here only while
//! Halftone Screen is the method.
//!
//! It is the hosted `ParamDialog` of `image.mode.bitmap`: sections, number rows with units for
//! Output and Frequency, and a custom row for the pattern picker. The fields are the command's
//! params (`resolution` in pixels/inch, `frequency` in lines/inch, the chosen units under
//! `resolution.unit` and `frequency.unit`), so OK, the live preview, `ui.dialog.set` and
//! `ui.dialog.confirm` work as for every schema dialog. The pattern picker draws from a snapshot of
//! the pattern library taken when the dialog opens.

use std::sync::Arc;

use craft::ui::dialog::{DialogFields, NumberUnit, ParamKind as Lib};
use craft::ui::localized::Unit;
use craft::ui::theme::ToEgui;
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

const RES_UNIT: &str = "resolution.unit";

fn text<'a>(f: &'a DialogFields, key: &str, default: &'a str) -> &'a str {
    f.text(key).unwrap_or(default)
}

/// The rows of the dialog. Photoshop greys the pattern out unless the method is Custom Pattern;
/// the Halftone Screen settings, which it asks for in a second dialog, show only while the method
/// is Halftone Screen. `input` is the document's resolution in pixels/inch.
pub(crate) fn params(library: Arc<PatternLibrary>, input: f32) -> Vec<LibParam> {
    let choices = |list: &[(&str, &str)]| list.iter().map(|(id, label)| ((*id).to_owned(), (*label).to_owned())).collect::<Vec<_>>();
    let res_units = vec![NumberUnit::new("ppi", tl!("Pixels/Inch"), 1.0), NumberUnit::new("ppcm", tl!("Pixels/Centimeter"), f64::from(CM))];
    let freq_units = vec![NumberUnit::new("lpi", tl!("Lines/Inch"), 1.0), NumberUnit::new("lpcm", tl!("Lines/Centimeter"), f64::from(CM))];
    let halftone = |p: LibParam| p.visible_when("method", "halftone");
    vec![
        LibParam::section(tl!("Resolution")),
        LibParam::custom("input", "Input", move |ui, f, _| {
            let t = craft::ui::theme::current(ui.ctx());
            let (v, unit) = if text(f, RES_UNIT, "ppi") == "ppcm" { (input / CM, tl!("Pixels/Centimeter")) } else { (input, tl!("Pixels/Inch")) };
            ui.label(egui::RichText::new(format!("{} {unit}", crate::widgets::fmt_num_prec(f64::from(v), 3))).color(t.color.text.normal.to_egui()))
        }),
        LibParam::number_with_units("resolution", "Output", f64::from(input), 1.0, 3, res_units)
            .bounds(Some(f64::from(RESOLUTION.0)), Some(f64::from(RESOLUTION.1))),
        LibParam::section(tl!("Method")),
        LibParam::new("method", "Use", Lib::Choice { options: choices(&METHODS), default: Some("diffusion".into()) }),
        LibParam::custom("patternRow", "Custom Pattern", move |ui, f, _| {
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
        halftone(LibParam::section(tl!("Halftone Screen"))),
        halftone(
            LibParam::number_with_units("frequency", "Frequency", 53.0, 1.0, 3, freq_units).bounds(Some(f64::from(FREQUENCY.0)), Some(f64::from(FREQUENCY.1))),
        ),
        halftone(LibParam::number("angle", "Angle", 45.0, 1.0, 1).bounds(Some(-180.0), Some(180.0)).unit(Unit::new("°"))),
        halftone(LibParam::new("shape", "Shape", Lib::Choice { options: choices(&SHAPES), default: Some("round".into()) })),
    ]
}

#[cfg(test)]
mod tests;
