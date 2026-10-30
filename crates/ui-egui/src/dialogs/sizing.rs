//! Photoshop-style Image Size and Canvas Size dialogs.
//!
//! Field values are stored in pixels (plus a display unit), so automation can set `width`/`height`
//! directly and the dialog maps them to `image.imageSize` / `image.canvasSize` unchanged.

use craft_ui::composites::{FieldControl, FormGrid, SectionHeader, SectionLabel};
use craft_ui::theme::StrokeExt;
use craft_ui::theme::ToEgui;
use egui::{Color32, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;
use crate::state::DialogKind;

const UNITS: [(&str, &str); 7] =
    [("px", "Pixels"), ("percent", "Percent"), ("in", "Inches"), ("cm", "Centimeters"), ("mm", "Millimeters"), ("pt", "Points"), ("pica", "Picas")];

/// Dialog unit for the Units & Rulers preference.
fn pref_unit(u: photocraft_engine::prefs::Unit) -> &'static str {
    use photocraft_engine::prefs::Unit;
    match u {
        Unit::Pixels => "px",
        Unit::Percent => "percent",
        Unit::Inches => "in",
        Unit::Centimeters => "cm",
        Unit::Millimeters => "mm",
        Unit::Points => "pt",
        Unit::Picas => "pica",
    }
}

/// Image Size resampling for Preferences › General › Image Interpolation.
pub(crate) fn pref_resample(i: photocraft_engine::prefs::Interpolation) -> &'static str {
    use photocraft_engine::prefs::Interpolation;
    match i {
        Interpolation::BicubicAutomatic => "automatic",
        Interpolation::Nearest => "nearest",
        Interpolation::Bilinear => "bilinear",
        Interpolation::Bicubic => "bicubic",
        Interpolation::BicubicSmoother => "bicubicSmoother",
        Interpolation::BicubicSharper => "bicubicSharper",
        Interpolation::PreserveDetails => "preserveDetails",
    }
}
const ANCHORS: [&str; 9] = ["topLeft", "top", "topRight", "left", "center", "right", "bottomLeft", "bottom", "bottomRight"];

pub fn is_sizing(command: &str) -> bool {
    matches!(command, "image.imageSize" | "image.canvasSize")
}

/// Alt+Cancel (Reset): restore the dimensions and options from when the dialog opened.
pub fn reset(f: &mut Map<String, Value>) {
    if !f.contains_key("__sizing") {
        return;
    }
    let w = num(f, "__origW");
    let h = num(f, "__origH");
    f.insert("width".into(), json!(w));
    f.insert("height".into(), json!(h));
    if f.get("__command").and_then(Value::as_str) == Some("image.imageSize") {
        f.insert("resolution".into(), json!(num(f, "__origRes")));
        f.insert("__constrain".into(), json!(true));
        if let Some(r) = f.get("__resample").cloned() {
            f.insert("resample".into(), r);
        }
    } else {
        f.insert("relative".into(), json!(false));
        f.insert("anchor".into(), json!("center"));
        f.insert("extensionColor".into(), json!("background"));
    }
}

pub fn open(app: &mut PhotocraftApp, command: &str) -> Option<u64> {
    let unit = pref_unit(app.session.prefs().units_and_rulers.rulers);
    let mut resample = pref_resample(app.session.prefs().general.image_interpolation);
    let pt_per_in = app.session.prefs().units_and_rulers.point_size.per_inch();
    let d = &app.session.active()?.doc;
    let (w, h, res) = (d.size.width as f64, d.size.height as f64, d.resolution_dpi as f64);
    let label = photocraft_engine::commands::find(command).map(|c| c.label).unwrap_or("Image Size…");
    let mut f = Map::new();
    f.insert("__command".into(), json!(command));
    f.insert("__label".into(), json!(label));
    f.insert("__sizing".into(), json!(true));
    f.insert("__origW".into(), json!(w));
    f.insert("__origH".into(), json!(h));
    f.insert("__origRes".into(), json!(res));
    f.insert("__unit".into(), json!(unit));
    f.insert("__ptPerIn".into(), json!(pt_per_in));
    f.insert("__bytesPerPixel".into(), json!(d.layers.first().and_then(|l| l.surface()).map_or(4, |s| s.format().bytes_per_pixel())));
    f.insert("width".into(), json!(w));
    f.insert("height".into(), json!(h));
    if command == "image.imageSize" {
        // Indexed Color resamples with Nearest Neighbor only, as the command does (#6019).
        let indexed = d.mode == photocraft_color::ColorMode::Indexed;
        if indexed && resample != "automatic" {
            resample = "nearest";
        }
        f.insert("__indexed".into(), json!(indexed));
        f.insert("__constrain".into(), json!(true));
        f.insert("resolution".into(), json!(res));
        f.insert("resample".into(), json!(resample));
        f.insert("__resample".into(), json!(resample));
    } else {
        f.insert("relative".into(), json!(false));
        f.insert("anchor".into(), json!("center"));
        f.insert("extensionColor".into(), json!("background"));
    }
    Some(app.ui.open_dialog(DialogKind::Command, f))
}

fn num(f: &Map<String, Value>, k: &str) -> f64 {
    f.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}

/// Points per inch from Preferences › Units & Rulers › Point/Pica Size, stored when the dialog
/// opens: 72 (PostScript) when missing or not a positive finite number.
fn pt_per_in(f: &Map<String, Value>) -> f64 {
    f.get("__ptPerIn").and_then(Value::as_f64).filter(|v| v.is_finite() && *v > 0.0).unwrap_or(72.0)
}

/// Pixels → display unit; `ppi` is points per inch (a pica is 12 points).
fn to_unit(px: f64, unit: &str, orig: f64, res: f64, ppi: f64) -> f64 {
    match unit {
        "percent" => px / orig.max(1.0) * 100.0,
        "in" => px / res.max(1.0),
        "cm" => px / res.max(1.0) * 2.54,
        "mm" => px / res.max(1.0) * 25.4,
        "pt" => px / res.max(1.0) * ppi,
        "pica" => px / res.max(1.0) * ppi / 12.0,
        _ => px,
    }
}

fn from_unit(v: f64, unit: &str, orig: f64, res: f64, ppi: f64) -> f64 {
    match unit {
        "percent" => v * orig / 100.0,
        "in" => v * res,
        "cm" => v / 2.54 * res,
        "mm" => v / 25.4 * res,
        "pt" => v / ppi * res,
        "pica" => v / ppi * 12.0 * res,
        _ => v,
    }
}

pub fn human_bytes(b: f64) -> String {
    if b >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.2}G", b / (1024.0 * 1024.0 * 1024.0))
    } else if b >= 1024.0 * 1024.0 {
        format!("{:.1}M", b / (1024.0 * 1024.0))
    } else {
        format!("{:.0}K", (b / 1024.0).max(1.0))
    }
}

pub fn body(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    if f.get("__command").and_then(Value::as_str) == Some("image.canvasSize") {
        canvas_size(ui, f);
    } else {
        image_size(ui, f);
    }
}

/// The width of the label column both Canvas Size grids share, so their controls line up.
const LABEL_COLUMN: f32 = 90.0;

/// Width / height field in the current unit with the unit menu beside it, writing back pixels.
/// `changed` is set when the field was edited.
#[allow(clippy::too_many_arguments)]
fn dim_cell(
    ui: &mut egui::Ui,
    f: &mut Map<String, Value>,
    key: &str,
    orig_key: &str,
    id: &str,
    unit_id: &str,
    label: &str,
    editable: bool,
    changed: &mut bool,
) -> (craft_ui::composites::OwnName<egui::Response>, egui::Response) {
    let unit = f.get("__unit").and_then(Value::as_str).unwrap_or("px").to_string();
    let res = f.get("resolution").and_then(Value::as_f64).unwrap_or(num(f, "__origRes"));
    let orig = num(f, orig_key);
    let px = num(f, key);
    let ppi = pt_per_in(f);
    let mut v = to_unit(px, &unit, orig, res, ppi) as f32;
    // The engine's 300000 px limit in this unit (never below 30000), so a large document's size
    // isn't clamped when the field is drawn: the field clamps the shown value to the range, and
    // reports that as an edit (#4162).
    let lim = if unit == "px" { 300000.0 } else { to_unit(300000.0, &unit, orig, res, ppi).max(30000.0) as f32 };
    // Each unit keeps its own precision: whole pixels, 3 places for inches, 2 for cm/mm/picas (#2434).
    // A typed unit (`2in` while in pixels) converts at the dialog's resolution.
    let units = crate::widgets::FieldUnits { unit: crate::widgets::unit_of_key(&unit), dpi: res, ppi };
    let decimals = crate::widgets::unit_decimals(&unit) as usize;
    let edit = ui.add_enabled_ui(editable, |ui| crate::widgets::dialog_number_in_units(ui, id, label, &mut v, -lim..=lim, decimals, units)).inner;
    if edit.changed {
        let px = from_unit(v as f64, &unit, orig, res, ppi);
        f.insert(key.into(), json!(if unit == "px" { px.round() } else { px }));
        *changed = true;
    }
    (edit.response.own_name(), unit_select(ui, f, unit_id))
}

fn unit_select(ui: &mut egui::Ui, f: &mut Map<String, Value>, id: &str) -> egui::Response {
    let mut unit = f.get("__unit").and_then(Value::as_str).unwrap_or("px").to_string();
    let opts: Vec<(String, &str)> = UNITS.iter().map(|(k, l)| (k.to_string(), *l)).collect();
    let edit = craft_ui::controls::Select::new(id, tl!("Units"), &mut unit).rows(opts.len(), |i| opts[i].0.clone(), |i| tl!(opts[i].1)).show(ui);
    if edit.response.changed() {
        f.insert("__unit".into(), json!(unit));
    }
    edit.response
}

fn image_size(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    use craft_ui::controls::Checkbox;
    let t = craft_ui::theme::current(ui.ctx());
    let (ow, oh) = (num(f, "__origW"), num(f, "__origH"));
    let bpp = num(f, "__bytesPerPixel").max(1.0);
    let resample = f.get("resample").and_then(Value::as_str).unwrap_or("bicubic").to_string();
    let resampling = resample != "none";
    if !resampling {
        // Without resampling the pixel count is fixed (`image.imageSize` keeps it): only the
        // resolution changes, so the readouts and fields always show the current pixels.
        f.insert("width".into(), json!(ow));
        f.insert("height".into(), json!(oh));
    }
    let (w, h) = (num(f, "width").round(), num(f, "height").round());
    // Photoshop keeps width and height linked while Resample is off.
    let mut constrain = !resampling || f.get("__constrain").and_then(Value::as_bool).unwrap_or(true);
    // With Resample off, a size in pixels or percent can't change (Photoshop disables those
    // fields); a physical size (inches, cm…) changes the resolution instead.
    let unit = f.get("__unit").and_then(Value::as_str).unwrap_or("px").to_string();
    let editable = resampling || !matches!(unit.as_str(), "px" | "percent");
    let (mut w_changed, mut h_changed) = (false, false);
    let mut rows = None;
    FormGrid::new("image-size").show(ui, |form| {
        form.value(
            tl!("Image Size:"),
            crate::i18n::fmt(tl!("{size} (was {old})"), &[("size", &human_bytes(w * h * bpp)), ("old", &human_bytes(ow * oh * bpp))]),
        );
        form.value(tl!("Dimensions:"), format!("{} px × {} px", w as i64, h as i64));
        form.space(craft_ui::theme::scale::Space::S6.px());
        let w_row = form.row(tl!("Width:"), |ui, _| dim_cell(ui, f, "width", "__origW", "is-w", "is-unit-w", tl!("Width"), editable, &mut w_changed));
        let h_row = form.row(tl!("Height:"), |ui, _| dim_cell(ui, f, "height", "__origH", "is-h", "is-unit-h", tl!("Height"), editable, &mut h_changed));
        rows = Some((w_row.inner, h_row.inner));
        if !resampling && (w_changed || h_changed) {
            // The edited side's new pixel count at the current resolution gives its new physical size;
            // keep the pixels and fit the resolution to that size instead.
            let (key, orig) = if w_changed { ("width", ow) } else { ("height", oh) };
            let res = f.get("resolution").and_then(Value::as_f64).unwrap_or(num(f, "__origRes"));
            let px = num(f, key);
            if px > 0.0 && px.is_finite() {
                f.insert("resolution".into(), json!((res * orig / px).clamp(1.0, 30000.0)));
            }
            f.insert("width".into(), json!(ow));
            f.insert("height".into(), json!(oh));
        }
        // The size field allows negatives for Canvas Size's relative mode, but an image is at
        // least one pixel: clamp here so the preview shows the size `image.imageSize` will apply.
        for (key, changed) in [("width", w_changed), ("height", h_changed)] {
            if changed && num(f, key) < 1.0 {
                f.insert(key.into(), json!(1.0));
            }
        }
        if constrain && resampling {
            if w_changed {
                f.insert("height".into(), json!((num(f, "width") * oh / ow.max(1.0)).round().max(1.0)));
            } else if h_changed {
                f.insert("width".into(), json!((num(f, "height") * ow / oh.max(1.0)).round().max(1.0)));
            }
        }
        form.row(tl!("Resolution:"), |ui, _| {
            let mut r = f.get("resolution").and_then(Value::as_f64).unwrap_or(72.0) as f32;
            let edit = crate::widgets::dialog_number_field(ui, "is-res", tl!("Resolution"), &mut r, 1.0..=30000.0, "", 2);
            if edit.changed {
                let r = r.max(1.0) as f64;
                if !resampling {
                    // Without resampling the pixel count is fixed: physical size follows resolution.
                    f.insert("width".into(), json!(ow));
                    f.insert("height".into(), json!(oh));
                } else if !matches!(unit.as_str(), "px" | "percent") {
                    // Resampling in a physical unit keeps the print size: the pixels follow the
                    // resolution (Photoshop). In pixels or percent the pixel count stays.
                    let old = f.get("resolution").and_then(Value::as_f64).unwrap_or(num(f, "__origRes"));
                    if old > 0.0 && old.is_finite() {
                        for key in ["width", "height"] {
                            f.insert(key.into(), json!((num(f, key) * r / old).max(1.0)));
                        }
                    }
                }
                f.insert("resolution".into(), json!(r));
            }
            (edit.response.own_name(), ui.label(tl!("Pixels/Inch")))
        });
        form.space(t.space.gap.stack);
        form.unlabelled(|ui| {
            let mut on = resampling;
            let edit = Checkbox::bool(tl!("Resample"), &mut on).show(ui);
            if edit.response.changed() {
                // Back on, it takes the method it had when the dialog opened.
                let method = f.get("__resample").cloned().unwrap_or_else(|| json!("bicubic"));
                f.insert("resample".into(), if on { method } else { json!("none") });
                if !on {
                    f.insert("width".into(), json!(ow));
                    f.insert("height".into(), json!(oh));
                }
            }
            if resampling {
                resample_select(ui, f, &resample);
            }
            edit
        });
    });
    let Some(((w_field, w_unit), (h_field, h_unit))) = rows else { return };
    // Photoshop's chain bracket linking width and height (right of the unit menus). PhotoCraft's
    // own widget: the library has no link between stacked rows.
    let x0 = w_unit.rect.right().max(h_unit.rect.right()) + 4.0;
    let bracket = Rect::from_min_max(pos2(x0, w_field.rect.center().y), pos2(x0 + 14.0, h_field.rect.center().y));
    let link = ui.interact(bracket.expand(2.0), ui.scope_id().with("constrain"), Sense::click());
    link.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("Constrain proportions")));
    let col = if constrain { t.color.text.normal.to_egui() } else { t.color.text.faint.to_egui() };
    if constrain {
        let p = ui.painter();
        let x = bracket.left() + 8.0;
        p.line_segment([pos2(bracket.left(), bracket.top()), pos2(x, bracket.top())], craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), col));
        p.line_segment([pos2(x, bracket.top()), pos2(x, bracket.center().y - 8.0)], craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), col));
        p.line_segment([pos2(x, bracket.center().y + 8.0), pos2(x, bracket.bottom())], craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), col));
        p.line_segment([pos2(x, bracket.bottom()), pos2(bracket.left(), bracket.bottom())], craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), col));
    }
    let chain = Rect::from_center_size(pos2(bracket.left() + 8.0, bracket.center().y), vec2(14.0, 14.0));
    ui.painter().rect_filled(chain, craft_ui::theme::scale::Radius::R2.px(), if link.hovered() { t.color.state.hover.to_egui() } else { Color32::TRANSPARENT });
    crate::icons::paint(ui, chain, if constrain { "link" } else { "unlink" }, 11.0, col);
    if link.on_hover_text(tl!("Constrain proportions")).clicked() && resampling {
        constrain = !constrain;
        f.insert("__constrain".into(), json!(constrain));
    }
}

/// The Resample method menu: Photoshop's list, then Lanczos.
fn resample_select(ui: &mut egui::Ui, f: &mut Map<String, Value>, current: &str) {
    let mut cur = current.to_string();
    let opts = [
        ("automatic".to_string(), tl!("Automatic")),
        ("preserveDetails".to_string(), tl!("Preserve Details")),
        ("bicubicSmoother".to_string(), tl!("Bicubic Smoother (enlargement)")),
        ("bicubicSharper".to_string(), tl!("Bicubic Sharper (reduction)")),
        ("bicubic".to_string(), tl!("Bicubic (smooth gradients)")),
        ("nearest".to_string(), tl!("Nearest Neighbor (hard edges)")),
        ("bilinear".to_string(), tl!("Bilinear")),
        ("lanczos".to_string(), tl!("Lanczos (sharp)")),
    ];
    // Indexed Color: every other method blends colours the colour table doesn't have,
    // which the command doesn't use and a save quantises away (#6019).
    let indexed = f.get("__indexed").and_then(Value::as_bool) == Some(true);
    let opts: Vec<_> = opts.into_iter().filter(|(k, _)| !indexed || k == "automatic" || k == "nearest").collect();
    let edit = craft_ui::controls::Select::new("is-resample", tl!("Resample"), &mut cur).rows(opts.len(), |i| opts[i].0.clone(), |i| tl!(opts[i].1)).show(ui);
    if edit.response.changed() {
        f.insert("resample".into(), json!(cur));
    }
}

fn canvas_size(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    use craft_ui::controls::{Checkbox, Select};
    let t = craft_ui::theme::current(ui.ctx());
    let (ow, oh) = (num(f, "__origW"), num(f, "__origH"));
    let bpp = num(f, "__bytesPerPixel").max(1.0);
    let relative = f.get("relative").and_then(Value::as_bool).unwrap_or(false);
    SectionLabel::new(tl!("Current Size")).show(ui);
    FormGrid::new("canvas-current").min_label_width(LABEL_COLUMN).show(ui, |form| {
        form.value(tl!("Size:"), human_bytes(ow * oh * bpp));
        form.value(tl!("Width:"), format!("{} px", ow as i64));
        form.value(tl!("Height:"), format!("{} px", oh as i64));
    });
    let (nw, nh) = if relative { (ow + num(f, "width"), oh + num(f, "height")) } else { (num(f, "width"), num(f, "height")) };
    let new_size = human_bytes(nw.max(0.0) * nh.max(0.0) * bpp);
    let muted = t.color.text.muted.to_egui();
    SectionHeader::new("canvas-new-size", tl!("New Size"))
        .collapsible(false)
        .controls(move |ui| {
            ui.label(egui::RichText::new(new_size).color(muted));
        })
        .show(ui, |ui| {
            FormGrid::new("canvas-new").min_label_width(LABEL_COLUMN).show(ui, |form| {
                let mut ignored = false;
                form.row(tl!("Width:"), |ui, _| dim_cell(ui, f, "width", "__origW", "cs-w", "cs-unit-w", tl!("Width"), true, &mut ignored));
                form.row(tl!("Height:"), |ui, _| dim_cell(ui, f, "height", "__origH", "cs-h", "cs-unit-h", tl!("Height"), true, &mut ignored));
                form.unlabelled(|ui| {
                    let mut rel = relative;
                    let edit = Checkbox::bool(tl!("Relative"), &mut rel).show(ui);
                    if edit.response.changed() {
                        // Convert between absolute and delta so the resulting canvas is unchanged.
                        let (w, h) = (num(f, "width"), num(f, "height"));
                        let (w, h) = if rel { (w - ow, h - oh) } else { (w + ow, h + oh) };
                        f.insert("width".into(), json!(w));
                        f.insert("height".into(), json!(h));
                        f.insert("relative".into(), json!(rel));
                    }
                    edit
                });
                form.row(tl!("Anchor:"), |ui, _| {
                    let cur = f.get("anchor").and_then(Value::as_str).unwrap_or("center").to_string();
                    let (picked, response) = anchor_grid(ui, &cur, nw - ow, nh - oh);
                    if let Some(a) = picked {
                        f.insert("anchor".into(), json!(a));
                    }
                    response
                });
            });
        });
    FormGrid::new("canvas-extension").show(ui, |form| {
        form.space(t.space.gap.stack);
        form.row(tl!("Canvas extension color:"), |ui, _| {
            let mut cur = f.get("extensionColor").and_then(Value::as_str).unwrap_or("background").to_string();
            let opts = [
                ("foreground".to_string(), tl!("Foreground")),
                ("background".to_string(), tl!("Background")),
                ("white".to_string(), tl!("White")),
                ("black".to_string(), tl!("Black")),
                ("transparent".to_string(), tl!("Transparent")),
            ];
            let edit = Select::new("cs-ext", tl!("Canvas extension color:"), &mut cur).rows(opts.len(), |i| opts[i].0.clone(), |i| tl!(opts[i].1)).show(ui);
            if edit.response.changed() {
                f.insert("extensionColor".into(), json!(cur));
            }
            edit
        });
    });
}

/// 3×3 anchor picker; arrows point away from the anchor in the directions the canvas grows.
/// PhotoCraft's own widget: the library has no direction-arrow grid. The response is the
/// grid's area, for the row that holds it.
fn anchor_grid(ui: &mut egui::Ui, current: &str, dw: f64, dh: f64) -> (Option<&'static str>, egui::Response) {
    let t = craft_ui::theme::current(ui.ctx());
    let cell = 24.0;
    let (rect, grid) = ui.allocate_exact_size(vec2(cell * 3.0, cell * 3.0), Sense::hover());
    let ci = ANCHORS.iter().position(|a| *a == current).unwrap_or(4) as i32;
    let (ax, ay) = (ci % 3, ci / 3);
    let mut picked = None;
    for (i, a) in ANCHORS.iter().enumerate() {
        let (x, y) = (i as i32 % 3, i as i32 / 3);
        let r = Rect::from_min_size(rect.min + vec2(x as f32 * cell, y as f32 * cell), vec2(cell, cell)).shrink(1.0);
        let resp = ui.interact(r, ui.scope_id().with(("anchor", i)), Sense::click());
        let selected = i as i32 == ci;
        ui.painter().rect_filled(
            r,
            craft_ui::theme::scale::Radius::R2.px(),
            if selected {
                t.color.surface.sunken.to_egui().gamma_multiply(1.6)
            } else if resp.hovered() {
                t.color.state.hover.to_egui()
            } else {
                t.color.surface.sunken.to_egui()
            },
        );
        ui.painter().rect_stroke(
            r,
            craft_ui::theme::scale::Radius::R2.px(),
            craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.color.line.divider.to_egui()),
            StrokeKind::Inside,
        );
        if selected {
            ui.painter().rect_filled(
                Rect::from_center_size(r.center(), vec2(8.0, 8.0)),
                craft_ui::theme::scale::Radius::R2.px(),
                t.color.text.normal.to_egui(),
            );
        } else {
            let (dx, dy) = (x - ax, y - ay);
            // Only neighbouring cells show arrows (Photoshop behaviour); direction flips when shrinking.
            if dx.abs() <= 1 && dy.abs() <= 1 {
                let sx = if dw < 0.0 { -1.0 } else { 1.0 };
                let sy = if dh < 0.0 { -1.0 } else { 1.0 };
                let dir = vec2(dx as f32 * sx, dy as f32 * sy);
                if dir != egui::Vec2::ZERO {
                    let dir = dir.normalized();
                    let c = r.center();
                    let tip = c + dir * 6.0;
                    let tail = c - dir * 5.0;
                    let side = vec2(-dir.y, dir.x) * 3.5;
                    let col = t.color.text.muted.to_egui();
                    ui.painter().line_segment([tail, tip], Stroke::new(1.3, col));
                    ui.painter().add(egui::Shape::convex_polygon(vec![tip + dir * 1.5, tip - dir * 3.5 + side, tip - dir * 3.5 - side], col, Stroke::NONE));
                }
            }
        }
        if resp.clicked() {
            picked = Some(*a);
        }
    }
    (picked, grid)
}

#[cfg(test)]
mod tests;
