//! Image Size and Canvas Size on the dialog host.
//!
//! The framework's forms own the fields, the linked size model and the column conversion. This file
//! builds their values from the document and the preferences, and maps the confirmed result to
//! `image.imageSize` / `image.canvasSize`.

use craft::behavior::{Anchor9, PixelSize};
use craft::ui::contract::Bind;
use craft::ui::dialog::{DialogId, DialogResult, dialog_id};
use craft::ui::patterns::form_dialogs::{
    CanvasExtension, CanvasSize, CanvasSizeResult, CanvasSizeValues, HostedCanvasSize, HostedImageSize, ImageSize, ImageSizeResult, ImageSizeValues, Resample,
};
use craft::ui::runtime::env::{ColumnSize, UnitKind};
use serde_json::{Value, json};

use crate::PhotocraftApp;

const IMAGE_COMMAND: &str = "image.imageSize";
const CANVAS_COMMAND: &str = "image.canvasSize";

/// The dialog unit of the Units & Rulers preference.
pub(crate) fn pref_unit(u: photocraft_engine::prefs::Unit) -> UnitKind {
    use photocraft_engine::prefs::Unit;
    match u {
        Unit::Pixels => UnitKind::Pixels,
        Unit::Percent => UnitKind::Percent,
        Unit::Inches => UnitKind::Inches,
        Unit::Centimeters => UnitKind::Centimetres,
        Unit::Millimeters => UnitKind::Millimetres,
        Unit::Points => UnitKind::Points,
        Unit::Picas => UnitKind::Picas,
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
<<<<<<< HEAD
=======

/// Whether the Resample list offers `method`. Indexed Color: every method but Automatic and
/// Nearest Neighbor blends colours the colour table doesn't have, which the command doesn't use
/// and a save quantises away (#6019).
fn resample_offered(f: &Map<String, Value>, method: &str) -> bool {
    let indexed = f.get("__indexed").and_then(Value::as_bool) == Some(true);
    !indexed || method == "automatic" || method == "nearest"
}

/// Alt+1 to Alt+8 (Option on macOS) and the Resample method each picks, as in the reference
/// application. Alt+3 is Preserve Details 2.0, which PhotoCraft doesn't have yet (#2942), and
/// Lanczos has no key.
const RESAMPLE_KEYS: [(egui::Key, Option<&str>); 8] = [
    (egui::Key::Num1, Some("automatic")),
    (egui::Key::Num2, Some("preserveDetails")),
    (egui::Key::Num3, None),
    (egui::Key::Num4, Some("bicubicSmoother")),
    (egui::Key::Num5, Some("bicubicSharper")),
    (egui::Key::Num6, Some("bicubic")),
    (egui::Key::Num7, Some("nearest")),
    (egui::Key::Num8, Some("bilinear")),
];

/// Takes Alt+1 to Alt+8 presses out of this frame's input and picks the Resample method of the
/// last one that Resample (on) and the list offer; the others do nothing. It runs before the
/// fields are drawn, so the press never reaches a focused field, and neither does the text the
/// key types (the digit, or Option's character on macOS), which winit sends right after its press.
/// Other typing in the same frame is kept. Ctrl+Alt is left alone: on Windows it's AltGr, which
/// types characters on many layouts.
fn resample_shortcut(ui: &egui::Ui, f: &mut Map<String, Value>) {
    let resampling = f.get("resample").and_then(Value::as_str).unwrap_or("bicubic") != "none";
    let picked = ui.ctx().input_mut(|i| {
        let mut picked = None;
        let mut swallow_text = false;
        i.events.retain(|e| {
            if let egui::Event::Text(_) = e {
                return !std::mem::take(&mut swallow_text);
            }
            swallow_text = false;
            let egui::Event::Key { key, pressed, modifiers, .. } = e else { return true };
            if !modifiers.alt || modifiers.ctrl || modifiers.command || modifiers.mac_cmd {
                return true;
            }
            let Some(&(_, method)) = RESAMPLE_KEYS.iter().find(|(k, _)| k == key) else { return true };
            if *pressed {
                swallow_text = true;
                if let Some(method) = method.filter(|m| resampling && resample_offered(f, m)) {
                    picked = Some(method);
                }
            }
            false
        });
        picked
    });
    if let Some(method) = picked {
        f.insert("resample".into(), json!(method));
    }
}

const ANCHORS: [&str; 9] = ["topLeft", "top", "topRight", "left", "center", "right", "bottomLeft", "bottom", "bottomRight"];
>>>>>>> upstream/main

/// The name `image.imageSize` reads a method by.
fn resample_key(method: Resample) -> &'static str {
    match method {
        Resample::Automatic => "automatic",
        Resample::PreserveDetails => "preserveDetails",
        Resample::BicubicSmoother => "bicubicSmoother",
        Resample::BicubicSharper => "bicubicSharper",
        Resample::Bicubic => "bicubic",
        Resample::NearestNeighbor => "nearest",
        Resample::Bilinear => "bilinear",
        Resample::Lanczos => "lanczos",
        // The enum is non-exhaustive: a method added later falls back to the preference default.
        _ => "automatic",
    }
}

/// The method an engine name stands for, through the one name table.
fn resample_from_key(key: &str) -> Option<Resample> {
    Resample::ALL.into_iter().find(|method| resample_key(*method) == key)
}

/// The method the Image Interpolation preference opens the dialog with.
fn resample_of(i: photocraft_engine::prefs::Interpolation) -> Resample {
    resample_from_key(pref_resample(i)).unwrap_or(Resample::Automatic)
}

/// The anchor name `image.canvasSize` reads.
fn anchor_key(anchor: Anchor9) -> &'static str {
    match anchor {
        Anchor9::TopLeft => "topLeft",
        Anchor9::Top => "top",
        Anchor9::TopRight => "topRight",
        Anchor9::Left => "left",
        Anchor9::Centre => "center",
        Anchor9::Right => "right",
        Anchor9::BottomLeft => "bottomLeft",
        Anchor9::Bottom => "bottom",
        Anchor9::BottomRight => "bottomRight",
        _ => "center",
    }
}

/// The `extensionColor` parameter of a canvas extension.
fn extension_param(extension: &CanvasExtension) -> Value {
    let hex = |rgb: [f32; 3]| {
        let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}", byte(rgb[0]), byte(rgb[1]), byte(rgb[2]))
    };
    match extension {
        CanvasExtension::Foreground => json!("foreground"),
        CanvasExtension::Background => json!("background"),
        CanvasExtension::White => json!("white"),
        CanvasExtension::Black => json!("black"),
        CanvasExtension::Gray => json!(hex([0.5; 3])),
        CanvasExtension::Other(rgb) => json!(hex(*rgb)),
        CanvasExtension::Transparent => json!("transparent"),
        _ => json!("background"),
    }
}

/// Whether `command` is Image Size or Canvas Size.
pub fn is_sizing(command: &str) -> bool {
<<<<<<< HEAD
    matches!(command, IMAGE_COMMAND | CANVAS_COMMAND)
=======
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
    let column_width = app.session.prefs().units_and_rulers.column_width;
    let column_gutter = app.session.prefs().units_and_rulers.gutter;
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
    f.insert("__colWidth".into(), json!(column_width));
    f.insert("__colGutter".into(), json!(column_gutter));
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
        // The "Image Size:" readout counts the flat image (#14768): the mode's colour channels at
        // the document's depth, no alpha, whatever the first layer's surface stores.
        // An Indexed image is one index channel and a Bitmap one 1-bit channel, though both are
        // stored expanded for editing. A Multichannel image counts its inks, not a fixed three; with
        // no inks it keeps the old count.
        let image_bytes_per_pixel = match d.mode {
            photocraft_color::ColorMode::Bitmap => 1.0 / 8.0,
            photocraft_color::ColorMode::Indexed => 1.0,
            photocraft_color::ColorMode::Multichannel => {
                let inks = d.channels.iter().filter(|c| c.spot.is_some()).count();
                (if inks == 0 { d.mode.color_channels() } else { inks } * d.depth.bytes()) as f64
            }
            mode => (mode.color_channels() * d.depth.bytes()) as f64,
        };
        f.insert("__imageBytesPerPixel".into(), json!(image_bytes_per_pixel));
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
>>>>>>> upstream/main
}

/// A byte count as the dialogs show it (`138.1M`).
pub fn human_bytes(b: f64) -> String {
    if b >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.2}G", b / (1024.0 * 1024.0 * 1024.0))
    } else if b >= 1024.0 * 1024.0 {
        format!("{:.1}M", b / (1024.0 * 1024.0))
    } else {
        format!("{:.0}K", (b / 1024.0).max(1.0))
    }
}

<<<<<<< HEAD
/// Opens Image Size or Canvas Size for the active document; `None` without one.
pub(crate) fn open(app: &mut PhotocraftApp, command: &str) -> Option<DialogId> {
    open_with(app, command, &Value::Null)
}

/// [`open`] with the `width`, `height`, `resolution` and `resample` of a recorded step applied, in pixels.
pub(crate) fn open_with(app: &mut PhotocraftApp, command: &str, params: &Value) -> Option<DialogId> {
    let prefs = app.session.prefs();
    let ur = &prefs.units_and_rulers;
    let seeded = params.get("width").is_some() || params.get("height").is_some();
    // A recorded size is in pixels, whatever the ruler unit.
    let unit = if seeded { UnitKind::Pixels } else { pref_unit(ur.rulers) };
    let points = f64::from(ur.point_size.per_inch());
    let columns = ColumnSize { width: f64::from(ur.column_width), gutter: f64::from(ur.gutter) };
    let interpolation = prefs.general.image_interpolation;
    let d = &app.session.active()?.doc;
    let size = PixelSize::new(d.size.width, d.size.height);
    let dpi = f64::from(d.resolution_dpi);
    let bytes_per_pixel = d.layers.first().and_then(|l| l.surface()).map_or(4, |s| s.format().bytes_per_pixel()) as f64;
    let id = if command == IMAGE_COMMAND {
        // Indexed Color resamples with Nearest Neighbor only, as the command does (#6019).
        let indexed = d.mode == photocraft_color::ColorMode::Indexed;
        let mut method = resample_of(interpolation);
        if indexed && method != Resample::Automatic {
            method = Resample::NearestNeighbor;
        }
        let methods = if indexed { vec![Resample::Automatic, Resample::NearestNeighbor] } else { Vec::new() };
        let values = ImageSizeValues::new(size).pixels_per_inch(dpi).points_per_inch(points).unit(unit).method(method).column_size(columns);
        app.framework.dialogs.open(HostedImageSize::new(values, move |values: Bind<'_, ImageSizeValues>| {
            ImageSize::new(values).methods(methods.clone()).bytes_per_pixel(bytes_per_pixel)
        }))
=======
/// The Image Size dialog's "Image Size:" figure (#14768): whole bytes below 1K (`3 bytes`), then
/// K, M, G or T (1K = 1024 bytes) with one decimal in the system's decimal mark (`351.6K`, `351,6K`).
fn image_size_bytes(b: f64) -> String {
    let n = if b.is_finite() { b.max(0.0).round() } else { 0.0 };
    if n < 1024.0 {
        return crate::i18n::trn(crate::i18n::current(), n as u64, "{n} byte", "{n} bytes");
    }
    let (mut v, mut unit) = (n / 1024.0, "K");
    for next in ["M", "G", "T"] {
        // A figure that would round to 1024.0 reads as 1.0 of the next unit.
        if (v * 10.0).round() < 10240.0 {
            break;
        }
        (v, unit) = (v / 1024.0, next);
    }
    let text = format!("{v:.1}{unit}");
    match crate::i18n::decimal_separator() {
        Some(',') => text.replace('.', ","),
        _ => text,
    }
}

pub fn body(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    if f.get("__command").and_then(Value::as_str) == Some("image.canvasSize") {
        canvas_size(ui, f);
>>>>>>> upstream/main
    } else {
        let values =
            CanvasSizeValues::new(size).pixels_per_inch(dpi).points_per_inch(points).unit(unit).has_background(d.background().is_some()).column_size(columns);
        app.framework
            .dialogs
            .open(HostedCanvasSize::new(values, move |values: Bind<'_, CanvasSizeValues>| CanvasSize::new(values).bytes_per_pixel(bytes_per_pixel)))
    };
    seed(app, id, command == IMAGE_COMMAND, params);
    Some(id)
}

/// Sets the recorded fields of `params` on the dialog `id`. A value the dialog refuses leaves the opening one.
fn seed(app: &mut PhotocraftApp, id: DialogId, image: bool, params: &Value) {
    let ctx = app.ctx().clone();
    // Resample first: it decides which fields take input.
    let keys: &[&str] = if image { &["resample", "resolution", "width", "height"] } else { &["width", "height"] };
    for key in keys {
        let Some(value) = params.get(*key) else { continue };
        let value = match (*key, value.as_str()) {
            ("resample", Some("none")) => Value::Null,
            ("resample", Some(name)) => resample_from_key(name).and_then(|m| serde_json::to_value(m).ok()).unwrap_or_else(|| value.clone()),
            _ => value.clone(),
        };
        let _ = app.framework.dialogs.set_json_field(&ctx, id, key, value);
    }
}

/// Runs the answer of Image Size or Canvas Size, when one finished this pass.
pub(crate) fn take(app: &mut PhotocraftApp, _ctx: &egui::Context) {
    if let Some(result) = app.framework.dialogs.take::<HostedImageSize>() {
        let params = match result {
            DialogResult::Ok(r) => Some(image_params(app, &r)),
            _ => None,
        };
        // One dialog of each type is open at a time, so its key names the dialog a paused playback holds.
        finish(app, dialog_id(HostedImageSize::KEY), IMAGE_COMMAND, params);
    }
    if let Some(result) = app.framework.dialogs.take::<HostedCanvasSize>() {
        let params = match result {
            DialogResult::Ok(r) => Some(canvas_params(&r)),
            _ => None,
        };
        finish(app, dialog_id(HostedCanvasSize::KEY), CANVAS_COMMAND, params);
    }
}

/// The `image.imageSize` parameters of a result. While the resolution is the opening one the
/// document keeps its own, which may lie outside the dialog's range (#9260).
fn image_params(app: &PhotocraftApp, r: &ImageSizeResult) -> Value {
    let own = app.session.active().map(|st| f64::from(st.doc.resolution_dpi));
    let resolution = if r.resolution_changed { r.pixels_per_inch } else { own.unwrap_or(r.pixels_per_inch) };
    json!({
        "width": r.size.width,
        "height": r.size.height,
        "resolution": resolution,
        "resample": r.resample.map_or("none", resample_key),
    })
}

/// The `image.canvasSize` parameters of a result: the size is absolute.
fn canvas_params(r: &CanvasSizeResult) -> Value {
    json!({
        "width": r.size.width,
        "height": r.size.height,
        "relative": false,
        "anchor": anchor_key(r.anchor),
        "extensionColor": extension_param(&r.extension),
    })
}

/// Runs `command` with `params` (`None`: the dialog was cancelled), then continues or abandons the
/// paused playback that opened the dialog.
fn finish(app: &mut PhotocraftApp, id: DialogId, command: &str, params: Option<Value>) {
    let token = crate::actions::token_of(app, id);
    let abandon = |app: &mut PhotocraftApp| {
        if let Some(token) = token {
            crate::actions::abandoned(app, token);
        }
    };
    let Some(params) = params else {
        abandon(app);
        return;
<<<<<<< HEAD
    };
    match crate::actions::run_step_dialog(app, id, command, params) {
        Ok(_) => {
            if let Some(token) = token
                && let Err(e) = crate::actions::resume(app, token)
            {
                app.ui.status = e;
                app.ui.status_error = true;
            }
        }
        Err(e) => {
            app.ui.status = e;
            app.ui.status_error = true;
            abandon(app);
=======
    }
    if key == "width" {
        f.remove("__unitW");
    }
    f.insert("__unit".into(), json!(unit));
}

fn image_size(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    resample_shortcut(ui, f);
    let (ow, oh) = (num(f, "__origW"), num(f, "__origH"));
    let resample = f.get("resample").and_then(Value::as_str).unwrap_or("bicubic").to_string();
    let resampling = resample != "none";
    if !resampling {
        // Without resampling the pixel count is fixed (`image.imageSize` keeps it): only the
        // resolution changes, so the readouts and fields always show the current pixels.
        f.insert("width".into(), json!(ow));
        f.insert("height".into(), json!(oh));
    }
    // The sides the command applies: whole pixels from 1 to MAX_CANVAS_SIDE, as image.imageSize clamps.
    let side = |key| num(f, key).round().clamp(1.0, f64::from(photocraft_format::MAX_CANVAS_SIDE));
    let (w, h, orig_w, orig_h) = (side("width"), side("height"), side("__origW"), side("__origH"));
    // "(was …)" appears only once the pixel dimensions differ from the original (#14768).
    let image_bpp = num(f, "__imageBytesPerPixel").max(0.0);
    // Rows hold whole bytes, so a Bitmap row of w bits takes ceil(w / 8) bytes.
    let flat = |width: f64, height: f64| (width * image_bpp).ceil() * height;
    let (size, old) = (flat(w, h), flat(orig_w, orig_h));
    let readout = if (w, h) == (orig_w, orig_h) {
        image_size_bytes(size)
    } else {
        crate::i18n::fmt(tl!("{size} (was {old})"), &[("size", &image_size_bytes(size)), ("old", &image_size_bytes(old))])
    };
    info_row(ui, tl!("Image Size:"), readout);
    info_row(ui, tl!("Dimensions:"), format!("{} px × {} px", w as i64, h as i64));
    ui.add_space(6.0);
    // Photoshop keeps width and height linked while Resample is off.
    let mut constrain = !resampling || f.get("__constrain").and_then(Value::as_bool).unwrap_or(true);
    // With Resample off, a size in pixels or percent can't change (Photoshop disables those
    // fields); a physical size (inches, cm…) changes the resolution instead.
    // Width in Columns is a physical size, so it stays editable while Height (in pixels) isn't.
    let editable = |f: &Map<String, Value>, key: &str| resampling || !matches!(field_unit(f, key).as_str(), "px" | "percent");
    let w_row = ui.horizontal(|ui| {
        label(ui, tl!("Width:"), 90.0);
        let editable = editable(f, "width");
        let c = ui.add_enabled_ui(editable, |ui| dim_field(ui, f, "width", "__origW", "is-w")).inner;
        unit_dropdown(ui, f, "width", "is-unit-w");
        c
    });
    let h_row = ui.horizontal(|ui| {
        label(ui, tl!("Height:"), 90.0);
        let editable = editable(f, "height");
        let c = ui.add_enabled_ui(editable, |ui| dim_field(ui, f, "height", "__origH", "is-h")).inner;
        unit_dropdown(ui, f, "height", "is-unit-h");
        c
    });
    let (w_changed, h_changed) = (w_row.inner, h_row.inner);
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
    // `dim_field` allows negatives for Canvas Size's relative mode, but an image is at least
    // one pixel: clamp here so the preview shows the size `image.imageSize` will apply.
    for (key, changed) in [("width", w_changed), ("height", h_changed)] {
        if changed && num(f, key) < 1.0 {
            f.insert(key.into(), json!(1.0));
        }
    }
    // Photoshop's chain bracket linking width and height (right of the unit dropdowns).
    let x0 = w_row.response.rect.right().max(h_row.response.rect.right()) + 4.0;
    let bracket = Rect::from_min_max(pos2(x0, w_row.response.rect.center().y), pos2(x0 + 14.0, h_row.response.rect.center().y));
    let link = ui.interact(bracket.expand(2.0), ui.scope_id().with("constrain"), Sense::click());
    let col = if constrain { t.text } else { t.text_faint };
    if constrain {
        let p = ui.painter();
        let x = bracket.left() + 8.0;
        p.line_segment([pos2(bracket.left(), bracket.top()), pos2(x, bracket.top())], Stroke::new(1.0, col));
        p.line_segment([pos2(x, bracket.top()), pos2(x, bracket.center().y - 8.0)], Stroke::new(1.0, col));
        p.line_segment([pos2(x, bracket.center().y + 8.0), pos2(x, bracket.bottom())], Stroke::new(1.0, col));
        p.line_segment([pos2(x, bracket.bottom()), pos2(bracket.left(), bracket.bottom())], Stroke::new(1.0, col));
    }
    let chain = Rect::from_center_size(pos2(bracket.left() + 8.0, bracket.center().y), vec2(14.0, 14.0));
    ui.painter().rect_filled(chain, 2.0, if link.hovered() { t.hover } else { Color32::TRANSPARENT });
    crate::icons::paint(ui, chain, if constrain { "link" } else { "unlink" }, 11.0, col);
    if link.on_hover_text(tl!("Constrain proportions")).clicked() && resampling {
        constrain = !constrain;
        f.insert("__constrain".into(), json!(constrain));
    }
    if constrain && resampling {
        if w_changed {
            f.insert("height".into(), json!((num(f, "width") * oh / ow.max(1.0)).round().max(1.0)));
        } else if h_changed {
            f.insert("width".into(), json!((num(f, "height") * ow / oh.max(1.0)).round().max(1.0)));
        }
    }
    ui.horizontal(|ui| {
        label(ui, tl!("Resolution:"), 90.0);
        let mut r = f.get("resolution").and_then(Value::as_f64).unwrap_or(72.0) as f32;
        if crate::widgets::value_field(ui, &mut r, 1.0..=30000.0, "", 90.0).accessible_name(tl!("Resolution")).changed() {
            let r = r.max(1.0) as f64;
            if !resampling {
                // Without resampling the pixel count is fixed: physical size follows resolution.
                f.insert("width".into(), json!(ow));
                f.insert("height".into(), json!(oh));
            } else {
                // Resampling in a physical unit keeps the print size: the pixels follow the
                // resolution (Photoshop). In pixels or percent the pixel count stays. Each field
                // has its own unit (Width can be Columns, a physical unit, beside Height in
                // pixels); with Constrain Proportions on, one physical side scales both.
                let physical = |key: &str| !matches!(field_unit(f, key).as_str(), "px" | "percent");
                let (pw, ph) = (physical("width"), physical("height"));
                let old = f.get("resolution").and_then(Value::as_f64).unwrap_or(num(f, "__origRes"));
                if old > 0.0 && old.is_finite() {
                    for (key, scale) in [("width", pw || (constrain && ph)), ("height", ph || (constrain && pw))] {
                        if scale {
                            f.insert(key.into(), json!((num(f, key) * r / old).max(1.0)));
                        }
                    }
                }
            }
            f.insert("resolution".into(), json!(r));
        }
        ui.label(egui::RichText::new(tl!("Pixels/Inch")).color(t.text_dim).size(12.0));
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        label(ui, "", 90.0);
        let mut on = resampling;
        if crate::widgets::checkbox(ui, &mut on, tl!("Resample")).changed() {
            // Back on, it takes the method it had when the dialog opened.
            let method = f.get("__resample").cloned().unwrap_or_else(|| json!("bicubic"));
            f.insert("resample".into(), if on { method } else { json!("none") });
            if !on {
                f.insert("width".into(), json!(ow));
                f.insert("height".into(), json!(oh));
            }
        }
        if resampling {
            let mut cur = resample.clone();
            // Photoshop's list, then Lanczos.
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
            let opts: Vec<_> = opts.into_iter().filter(|(k, _)| resample_offered(f, k)).collect();
            if crate::widgets::dropdown(ui, "is-resample", tl!("Resample"), &mut cur, &opts, 200.0) {
                f.insert("resample".into(), json!(cur));
            }
        }
    });
}

fn canvas_size(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let (ow, oh) = (num(f, "__origW"), num(f, "__origH"));
    let bpp = num(f, "__bytesPerPixel").max(1.0);
    let relative = f.get("relative").and_then(Value::as_bool).unwrap_or(false);
    ui.label(egui::RichText::new(tl!("Current Size")).font(crate::theme::semibold(12.0)).color(t.text));
    info_row(ui, tl!("Size:"), human_bytes(ow * oh * bpp));
    info_row(ui, tl!("Width:"), format!("{} px", ow as i64));
    info_row(ui, tl!("Height:"), format!("{} px", oh as i64));
    ui.add_space(6.0);
    crate::widgets::hairline(ui);
    ui.add_space(6.0);
    let (nw, nh) = if relative { (ow + num(f, "width"), oh + num(f, "height")) } else { (num(f, "width"), num(f, "height")) };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!("New Size")).font(crate::theme::semibold(12.0)).color(t.text));
        ui.label(egui::RichText::new(human_bytes(nw.max(0.0) * nh.max(0.0) * bpp)).color(t.text_dim).size(12.0));
    });
    ui.horizontal(|ui| {
        label(ui, tl!("Width:"), 90.0);
        dim_field(ui, f, "width", "__origW", "cs-w");
        unit_dropdown(ui, f, "width", "cs-unit-w");
    });
    ui.horizontal(|ui| {
        label(ui, tl!("Height:"), 90.0);
        dim_field(ui, f, "height", "__origH", "cs-h");
        unit_dropdown(ui, f, "height", "cs-unit-h");
    });
    ui.horizontal(|ui| {
        label(ui, "", 90.0);
        let mut rel = relative;
        if crate::widgets::checkbox(ui, &mut rel, tl!("Relative")).changed() {
            // Convert between absolute and delta so the resulting canvas is unchanged.
            let (w, h) = (num(f, "width"), num(f, "height"));
            let (w, h) = if rel { (w - ow, h - oh) } else { (w + ow, h + oh) };
            f.insert("width".into(), json!(w));
            f.insert("height".into(), json!(h));
            f.insert("relative".into(), json!(rel));
        }
    });
    ui.horizontal(|ui| {
        label(ui, tl!("Anchor:"), 90.0);
        let cur = f.get("anchor").and_then(Value::as_str).unwrap_or("center").to_string();
        if let Some(a) = anchor_grid(ui, &cur, nw - ow, nh - oh) {
            f.insert("anchor".into(), json!(a));
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        label(ui, tl!("Canvas extension color:"), 150.0);
        let mut cur = f.get("extensionColor").and_then(Value::as_str).unwrap_or("background").to_string();
        let opts = [
            ("foreground".to_string(), tl!("Foreground")),
            ("background".to_string(), tl!("Background")),
            ("white".to_string(), tl!("White")),
            ("black".to_string(), tl!("Black")),
            ("transparent".to_string(), tl!("Transparent")),
        ];
        if crate::widgets::dropdown(ui, "cs-ext", tl!("Canvas extension color:"), &mut cur, &opts, 140.0) {
            f.insert("extensionColor".into(), json!(cur));
        }
    });
}

/// 3×3 anchor picker; arrows point away from the anchor in the directions the canvas grows.
fn anchor_grid(ui: &mut egui::Ui, current: &str, dw: f64, dh: f64) -> Option<&'static str> {
    let t = Tokens::get(ui.ctx());
    let cell = 24.0;
    let (rect, _) = ui.allocate_exact_size(vec2(cell * 3.0, cell * 3.0), Sense::hover());
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
            2.0,
            if selected {
                t.field.gamma_multiply(1.6)
            } else if resp.hovered() {
                t.hover
            } else {
                t.field
            },
        );
        ui.painter().rect_stroke(r, 2.0, Stroke::new(1.0, t.separator), StrokeKind::Inside);
        if selected {
            ui.painter().rect_filled(Rect::from_center_size(r.center(), vec2(8.0, 8.0)), 1.0, t.text);
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
                    let col = t.text_dim;
                    ui.painter().line_segment([tail, tip], Stroke::new(1.3, col));
                    ui.painter().add(egui::Shape::convex_polygon(vec![tip + dir * 1.5, tip - dir * 3.5 + side, tip - dir * 3.5 - side], col, Stroke::NONE));
                }
            }
        }
        if resp.clicked() {
            picked = Some(*a);
>>>>>>> upstream/main
        }
    }
}

#[cfg(test)]
#[path = "image_size_readout_tests.rs"]
mod image_size_readout_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "image_size_resample_shortcuts_tests.rs"]
mod image_size_resample_shortcuts_tests;
