//! Photoshop's Color Picker (Foreground / Background Color) dialog: a 2D colour field and a slider
//! for the selected component (H, S, B, R, G, B, L, a or b radio), new/current swatches with the
//! out-of-gamut and web-colour warnings, and HSB, RGB, Lab, CMYK and hex fields. The colour lives in the dialog fields (`color` as `#rrggbb`, plus the HSB
//! floats so hue survives greys), so `ui.dialog.set` drives it; OK runs `tools.setColors`.
//! While it is the top dialog the image is its eyedropper, as in Photoshop: the pointer over the
//! canvas is a pipette, and a click or drag there samples into the new colour ([`sample_at`]).

use craft_theme::{Channel, ColorModel, DefaultOn, HsbModel, LabModel, MAX_CHANNELS, ModelValues, RgbModel, Srgba};
use craft_ui::composites::{AreaMode, ColorPicker, ColorPickerLayout, ColorWarning, PickedColor};
use craft_ui::controls::{Button, ButtonVariant};
use craft_ui::localized::LocaleNumbers;
use craft_ui::theme::scale::Space;
use egui::{Rect, UiBuilder, pos2};
use photocraft_color::convert::{hsb_to_rgb, rgb_to_hsb};
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;
use crate::state::DialogKind;
use crate::widgets;

/// Colour differences the CMYK round trip leaves inside the printable gamut (ΔE ≤ 0.4, black ≈ 1.4
/// with black-point compensation) stay below this; out-of-gamut colours move far more.
const GAMUT_DE: f32 = 3.0;

/// The nearest printable colour, when `rgb` is outside the CMYK gamut (Photoshop's ⚠ warning).
pub fn out_of_gamut(rgb: [f32; 3]) -> Option<[f32; 3]> {
    use photocraft_color::convert::{cmyk_to_rgb, rgb_to_cmyk, srgb_to_lab};
    let back = cmyk_to_rgb(rgb_to_cmyk(rgb)).map(|c| c.clamp(0.0, 1.0));
    let (a, b) = (srgb_to_lab(rgb), srgb_to_lab(back));
    let de = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    (de > GAMUT_DE).then_some(back)
}

/// The nearest web colour (channels in steps of 51), when `rgb` isn't one.
pub fn not_web_safe(rgb: [f32; 3]) -> Option<[f32; 3]> {
    let web = web_safe(rgb);
    (hex(web) != hex(rgb)).then_some(web)
}

pub fn hex(c: [f32; 3]) -> String {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", q(c[0]), q(c[1]), q(c[2]))
}

pub fn parse_hex(s: &str) -> Option<[f32; 3]> {
    let h = s.trim().trim_start_matches('#');
    // `from_str_radix` accepts a leading '+', so require plain hex digits first.
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    // `get`, not `[..]`: typed text may be multi-byte, and a byte range can split a character.
    let c = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok().map(|v| f32::from(v) / 255.0);
    Some([c(0)?, c(2)?, c(4)?])
}

/// Open the picker for the foreground or background colour.
pub fn open(app: &mut PhotocraftApp, target: &str) -> u64 {
    let c = if target == "background" { app.session.tools.background } else { app.session.tools.foreground };
    let rgb = [c[0], c[1], c[2]];
    let label = if target == "background" { "Color Picker (Background Color)" } else { "Color Picker (Foreground Color)" };
    let mut fields = fields_with_web(app, target, label, rgb);
    // A colour that was picked as CMYK shows the inks it was picked as, not what its sRGB stand-in converts to (#4196).
    if let Some(cmyk) = app.session.tools.picked_cmyk(rgb) {
        fields.insert("__cmyk".into(), json!(cmyk));
    }
    app.ui.open_dialog(DialogKind::Command, fields)
}

/// Shared initial state for every target; the picker layout is independent of its destination.
pub(crate) fn initial_fields(target: &str, label: &str, rgb: [f32; 3]) -> Map<String, Value> {
    let mut f = Map::new();
    f.insert("__colorPicker".into(), json!(target));
    f.insert("__label".into(), json!(label));
    f.insert("color".into(), json!(hex(rgb)));
    f.insert("__orig".into(), json!(hex(rgb)));
    f.insert("__hsv".into(), json!(rgb_to_hsb(rgb)));
    f.insert("__mode".into(), json!("h"));
    f.insert("__webOnly".into(), json!(false));
    f
}

/// Seed [`initial_fields`] with the last Only Web Colors setting. With web-only on, the opening
/// colour is snapped and `__orig` follows it: OK without an edit is "unchanged", so opening the
/// picker never rewrites a fill behind the user's back (#4995).
pub(crate) fn fields_with_web(app: &PhotocraftApp, target: &str, label: &str, rgb: [f32; 3]) -> Map<String, Value> {
    let mut f = initial_fields(target, label, rgb);
    f.insert("__webOnly".into(), json!(app.ui.color_panel.web_only));
    if app.ui.color_panel.web_only {
        set_rgb(&mut f, rgb, Keep::Nothing);
        f.insert("__orig".into(), f["color"].clone());
    }
    f
}

/// Open the picker on `rgb` for something other than the tool colours: OK runs `command` with
/// `params` plus `"color": "#rrggbb"` (e.g. a gradient stop's colour).
pub fn open_for_command(app: &mut PhotocraftApp, label: &str, rgb: [f32; 3], command: &str, params: Value) -> u64 {
    let mut f = fields_with_web(app, "command", label, rgb);
    f.insert("__command".into(), json!(command));
    f.insert("__params".into(), params);
    app.ui.open_dialog(DialogKind::Command, f)
}

/// Open the picker on `rgb` for a colour field of another dialog (e.g. Edit › Fill's Color…): OK
/// writes `"#rrggbb"` into `field` of dialog `dialog`, which stays open; Cancel leaves it alone.
pub fn open_for_field(app: &mut PhotocraftApp, label: &str, rgb: [f32; 3], dialog: u64, field: &str) -> u64 {
    let mut f = fields_with_web(app, "field", label, rgb);
    f.insert("__dialog".into(), json!(dialog));
    f.insert("__field".into(), json!(field));
    app.ui.open_dialog(DialogKind::Command, f)
}

/// Open the picker on `rgb` for a colour the shell keeps itself (e.g. the Crop shield's custom
/// colour): OK hands `"#rrggbb"` to `target`'s setter in [`confirm`]; Cancel leaves it alone.
pub fn open_for_target(app: &mut PhotocraftApp, target: &str, label: &str, rgb: [f32; 3]) -> u64 {
    app.ui.open_dialog(DialogKind::Command, fields_with_web(app, target, label, rgb))
}

/// Set by a dialog body (to the picker's title) to open the Color Picker on its `color` field.
const REQUEST: &str = "__pickColor";

/// A dialog body asks for the Color Picker on its `color` field (Edit › Fill's Color… and colour
/// swatch, Edit › Stroke's swatch). The dialog loop opens it ([`open_requested`]), since that
/// needs the dialog's id.
pub fn request(f: &mut Map<String, Value>, title: &str) {
    request_field(f, "color", title);
}

/// [`request`] for another `"#rrggbb"` field of the dialog: a top-level key (`vineColor`), or a
/// JSON pointer into the fields (`/values/guidesGridAndSlices/guideColor`).
pub fn request_field(f: &mut Map<String, Value>, field: &str, title: &str) {
    f.insert(REQUEST.into(), json!({ "title": title, "field": field }));
}

/// The picker's title for the colour called `name`: "Color Picker (Vine Color)".
pub fn title_for(name: &str) -> String {
    crate::i18n::fmt(tl!("Color Picker ({name})"), &[("name", name)])
}

/// The `"#rrggbb"` at `field` of `f` (a key or a JSON pointer).
fn field_value<'a>(f: &'a Map<String, Value>, field: &str) -> Option<&'a str> {
    match field.strip_prefix('/') {
        Some(rest) => {
            let (first, tail) = rest.split_once('/').unwrap_or((rest, ""));
            let v = f.get(first)?;
            if tail.is_empty() { v.as_str() } else { v.pointer(&format!("/{tail}"))?.as_str() }
        }
        None => f.get(field)?.as_str(),
    }
}

/// Sets `field` of `f` (a key or a JSON pointer to an existing value) to `value`.
fn set_field(f: &mut Map<String, Value>, field: &str, value: Value) -> Result<(), String> {
    match field.strip_prefix('/') {
        Some(rest) => {
            let (first, tail) = rest.split_once('/').unwrap_or((rest, ""));
            let v = f.get_mut(first).ok_or("the dialog has no such colour")?;
            let slot = if tail.is_empty() { v } else { v.pointer_mut(&format!("/{tail}")).ok_or("the dialog has no such colour")? };
            *slot = value;
        }
        None => {
            f.insert(field.into(), value);
        }
    }
    Ok(())
}

/// Opens the Color Picker dialog `id`'s body asked for ([`request`], [`request_field`]), on its
/// colour: OK sets that field of the dialog (not the foreground), Cancel leaves it as it was.
pub fn open_requested(app: &mut PhotocraftApp, id: u64) {
    let Some(d) = app.ui.dialog_mut(id) else { return };
    let (title, field) = match d.fields.remove(REQUEST) {
        Some(Value::String(title)) => (title, "color".to_string()),
        Some(Value::Object(o)) => {
            let get = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_string);
            let (Some(title), Some(field)) = (get("title"), get("field")) else { return };
            (title, field)
        }
        _ => return,
    };
    let rgb = field_value(&d.fields, &field).and_then(parse_hex).unwrap_or([0.0; 3]);
    open_for_field(app, &title, rgb, id, &field);
}

pub fn owns(f: &Map<String, Value>) -> bool {
    f.contains_key("__colorPicker")
}

/// Alt+Cancel (Reset): restore the colour from when the picker opened (same as clicking Current).
pub fn reset(f: &mut Map<String, Value>) {
    if !owns(f) {
        return;
    }
    let orig = f.get("__orig").and_then(Value::as_str).and_then(parse_hex).unwrap_or([0.0; 3]);
    set_rgb(f, orig, Keep::Nothing);
}

/// The Color Picker, when it is the top dialog (another dialog opened over it takes the input).
pub fn top(app: &PhotocraftApp) -> Option<u64> {
    app.ui.dialogs.last().filter(|d| owns(&d.fields)).map(|d| d.id)
}

/// Eyedropper: the top Color Picker's new colour becomes the image's composite colour at document
/// point (x, y). Off the image or over transparency nothing changes.
pub fn sample_at(app: &mut PhotocraftApp, x: f64, y: f64) {
    let Some(id) = top(app) else { return };
    let Some(rgb) = crate::canvas::composite_color(app, x, y) else { return };
    if let Some(d) = app.ui.dialog_mut(id) {
        set_rgb(&mut d.fields, rgb, Keep::Nothing);
    }
}

/// The colour in every model the dialog shows.
pub struct Components {
    pub rgb: [f32; 3],
    pub hsv: [f32; 3],
    pub lab: [f32; 3],
    pub cmyk: [f32; 4],
}

/// Component values an edit stores next to the new `color`. sRGB alone loses them: greys have no
/// hue, and Lab or CMYK values outside sRGB are clamped. Typing "60" into L passes through L = 6
/// first; without the stored Lab the clamped colour would change `a` and `b` under the user.
enum Keep {
    Nothing,
    Hsv([f32; 3]),
    Lab([f32; 3]),
    Cmyk([f32; 4]),
}

/// The stored `key` components, if they still produce `rgb`.
fn kept<const N: usize>(f: &Map<String, Value>, key: &str, rgb: [f32; 3], to_rgb: impl Fn([f32; N]) -> [f32; 3]) -> Option<[f32; N]>
where
    [f32; N]: serde::de::DeserializeOwned,
{
    let v: [f32; N] = f.get(key).and_then(|v| serde_json::from_value(v.clone()).ok())?;
    (hex(to_rgb(v)) == hex(rgb)).then_some(v)
}

/// The CMYK values typed into the picker, while they still match its colour: the inks a command
/// should keep in a CMYK document (#4196, #8568).
pub(crate) fn picked_cmyk(f: &Map<String, Value>) -> Option<[f32; 4]> {
    let rgb = f.get("color").and_then(Value::as_str).and_then(parse_hex)?;
    kept::<4>(f, "__cmyk", rgb, photocraft_color::convert::cmyk_to_rgb)
}

/// Current colour, keeping the stored HSB, Lab or CMYK values while they still match `color`.
pub fn current(f: &Map<String, Value>) -> Components {
    let rgb = f.get("color").and_then(Value::as_str).and_then(parse_hex).unwrap_or([0.0; 3]);
    Components {
        rgb,
        hsv: kept(f, "__hsv", rgb, hsb_to_rgb).unwrap_or_else(|| rgb_to_hsb(rgb)),
        lab: kept(f, "__lab", rgb, photocraft_color::convert::lab_to_srgb).unwrap_or_else(|| photocraft_color::convert::srgb_to_lab(rgb)),
        cmyk: kept(f, "__cmyk", rgb, photocraft_color::convert::cmyk_to_rgb).unwrap_or_else(|| photocraft_color::convert::rgb_to_cmyk(rgb)),
    }
}

fn set_rgb(f: &mut Map<String, Value>, rgb: [f32; 3], keep: Keep) {
    let web = f.get("__webOnly").and_then(Value::as_bool).unwrap_or(false);
    let (rgb, keep) = if web { (web_safe(rgb), Keep::Nothing) } else { (rgb, keep) };
    f.insert("color".into(), json!(hex(rgb)));
    for key in ["__hsv", "__lab", "__cmyk"] {
        f.remove(key);
    }
    match keep {
        Keep::Nothing => {}
        Keep::Hsv(h) => {
            f.insert("__hsv".into(), json!(h));
        }
        Keep::Lab(l) => {
            f.insert("__lab".into(), json!(l));
        }
        Keep::Cmyk(k) => {
            f.insert("__cmyk".into(), json!(k));
        }
    }
}

/// Snap each channel to the web-safe ramp (00, 33, 66, 99, CC, FF).
fn web_safe(c: [f32; 3]) -> [f32; 3] {
    c.map(|v| (v.clamp(0.0, 1.0) * 5.0).round() / 5.0)
}

/// Room for both component columns, including units, without overflowing the title's width; with
/// the frame and title the window fits Photoshop's (about 534 x 376 points).
pub(crate) const CONTENT_WIDTH: f32 = 488.0;
/// Draw the picker and its top-right actions. `Some(true)` confirms, `Some(false)` cancels.
pub fn body(ui: &mut egui::Ui, f: &mut Map<String, Value>) -> Option<bool> {
    ui.set_width(CONTENT_WIDTH);
    let now = current(f);
    let orig = f.get("__orig").and_then(Value::as_str).and_then(parse_hex).unwrap_or(now.rgb);
    let mut mode = area_mode(f.get("__mode").and_then(Value::as_str).unwrap_or("h"));
    let mut web = f.get("__webOnly").and_then(Value::as_bool).unwrap_or(false);
    let web_before = web;
    let cmyk = CmykModel(picked_cmyk(f));
    let models: [&dyn ColorModel; 4] = [&HsbModel, &RgbModel, &LabModel, &cmyk];
    let numbers = LocaleNumbers::of(ui);
    let screen_id = ui.scope_id().with("screen-color");
    let source = srgba(now.rgb);
    let mut picker = ColorPicker::new("color-picker", tl!("Color"), source, &numbers)
        .layout(ColorPickerLayout::Full)
        .models(models)
        .mode(&mut mode)
        .web_colors(&mut web)
        .compare(srgba(orig))
        .eyedropper(tl!("Pick screen color"))
        .eyedropper_busy(!crate::screen_picker::available(ui.ctx()) || crate::screen_picker::busy(ui.ctx()))
        .focus_on_open(false)
        .text(word);
    // Photoshop's warnings beside the swatch, each over a chip of the nearest colour that passes
    // (a click takes it): out of gamut for printing on top, not a web colour below.
    if let Some(near) = out_of_gamut(now.rgb) {
        picker = picker.warning(ColorWarning::new(tl!("Out of gamut for printing: click for the nearest printable colour"), srgba(near)));
    }
    if let Some(near) = not_web_safe(now.rgb) {
        picker = picker.warning(ColorWarning::web_colors(tl!("Not a web colour: click for the nearest web colour"), srgba(near)));
    }
    let shown = picker.show(ui);
    // The hex field takes the focus, its text selected, once the dialog is on show: the library
    // asks on the first pass, which a window's hidden sizing pass spends.
    if ui.is_visible() && !f.contains_key("__focused") {
        f.insert("__focused".into(), json!(true));
        ColorPicker::focus_hex(ui.ctx(), shown.edit.response.id);
    }
    if shown.eyedropper_requested {
        crate::screen_picker::request(ui.ctx(), screen_id);
    }
    if let Some(rgb) = crate::screen_picker::take(ui.ctx(), screen_id) {
        ColorPicker::sampled(ui.ctx(), shown.edit.response.id, srgba(rgb));
    }
    if web != web_before {
        f.insert("__webOnly".into(), json!(web));
    }
    if mode_key(mode) != f.get("__mode").and_then(Value::as_str).unwrap_or("h") {
        f.insert("__mode".into(), json!(mode_key(mode)));
    }
    if let Some(picked) = shown.edit.event.as_ref().and_then(|e| e.value()) {
        apply_picked(f, picked);
    }
    actions(ui, f, shown.edit.response.rect.top())
}

/// OK, Cancel and Add to Swatches in a column at the picker's top-right corner, beside the
/// compare swatch and above the channel fields, as Photoshop's are. `top` is the picker's top.
fn actions(ui: &mut egui::Ui, f: &mut Map<String, Value>, top: f32) -> Option<bool> {
    let mut outcome = None;
    // As wide as the longest label, at the right edge like Photoshop's.
    let width = widgets::button_width(ui, &[tl!("OK"), tl!("Cancel"), tl!("Add to Swatches")]).max(96.0);
    let right = ui.max_rect().right();
    let rect = Rect::from_min_max(pos2(right - width, top), pos2(right, top + 120.0));
    let mut column = ui.new_child(UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Max)));
    let ui = &mut column;
    if ui.add(Button::new(tl!("OK")).variant(ButtonVariant::Primary).min_width(width)).clicked() {
        outcome = Some(true);
    }
    ui.add_space(Space::S4.px());
    if ui.add(Button::new(tl!("Cancel")).variant(ButtonVariant::Secondary).min_width(width)).clicked() {
        outcome = Some(false);
    }
    ui.add_space(Space::S4.px());
    // Handled by the dialog host, which has the app (`take_add_swatch`).
    if ui.add(Button::new(tl!("Add to Swatches")).variant(ButtonVariant::Secondary).min_width(width)).clicked() {
        f.insert("__addSwatch".into(), json!(true));
    }
    outcome
}

/// Stores what the picker reported: the colour, and the values of the model it was edited in
/// while they still give it (greys keep their hue, a typed Lab or CMYK colour outside sRGB its
/// values).
fn apply_picked(f: &mut Map<String, Value>, picked: &PickedColor) {
    let rgb = [picked.color.r, picked.color.g, picked.color.b];
    let keep = match picked.model {
        Some(m) if m.model == HsbModel.id() => Keep::Hsv([m.values[0], m.values[1] / 100.0, m.values[2] / 100.0]),
        Some(m) if m.model == LabModel.id() => Keep::Lab([m.values[0], m.values[1], m.values[2]]),
        Some(m) if m.model == CMYK_ID => Keep::Cmyk([0, 1, 2, 3].map(|i| m.values[i] / 100.0)),
        _ => Keep::Nothing,
    };
    if hex(rgb) != f.get("color").and_then(Value::as_str).unwrap_or_default() || !matches!(keep, Keep::Nothing) {
        set_rgb(f, rgb, keep);
    }
}

pub(crate) fn srgba(c: [f32; 3]) -> Srgba {
    Srgba::rgb(c[0], c[1], c[2])
}

/// Area modes in the order of the `__mode` field's keys.
const MODE_KEYS: [(&str, AreaMode); 9] = [
    ("h", AreaMode::Hue),
    ("s", AreaMode::Saturation),
    ("v", AreaMode::Brightness),
    ("r", AreaMode::Red),
    ("g", AreaMode::Green),
    ("b", AreaMode::Blue),
    ("lab_l", AreaMode::LabL),
    ("lab_a", AreaMode::LabA),
    ("lab_b", AreaMode::LabB),
];

fn area_mode(key: &str) -> AreaMode {
    MODE_KEYS.iter().find(|(k, _)| *k == key).map_or(AreaMode::Hue, |(_, m)| *m)
}

fn mode_key(mode: AreaMode) -> &'static str {
    MODE_KEYS.iter().find(|(_, m)| *m == mode).map_or("h", |(k, _)| k)
}

/// The picker's words, in PhotoCraft's: the channel fields and radios keep their "H:" names, the
/// hex field its "Color" name and the compare swatch its restore tip.
fn word(key: &str) -> String {
    let text: &str = match key {
        "Hue" => tl!("H:"),
        "Saturation" => tl!("S:"),
        "Brightness" | "Blue" => tl!("B:"),
        "Red" => tl!("R:"),
        "Green" => tl!("G:"),
        "Lightness" => tl!("L:"),
        "Green-red" => tl!("a:"),
        "Blue-yellow" => tl!("b:"),
        "Cyan" => tl!("C:"),
        "Magenta" => tl!("M:"),
        "Yellow" => tl!("Y:"),
        "Black" => tl!("K:"),
        "Hex colour" => tl!("Color"),
        "Restore current colour" => tl!("Click to restore the current colour"),
        other => tl!(other),
    };
    text.to_string()
}

const CMYK_ID: &str = "cmyk";

const CMYK_CHANNELS: [Channel; 4] = [
    Channel { name: "Cyan", qualified: "CMYK cyan", label: "C", min: 0.0, max: 100.0, unit: "%", decimals: 0, step: 1.0, track_stops: 2 },
    Channel { name: "Magenta", qualified: "CMYK magenta", label: "M", min: 0.0, max: 100.0, unit: "%", decimals: 0, step: 1.0, track_stops: 2 },
    Channel { name: "Yellow", qualified: "CMYK yellow", label: "Y", min: 0.0, max: 100.0, unit: "%", decimals: 0, step: 1.0, track_stops: 2 },
    Channel { name: "Black", qualified: "CMYK black", label: "K", min: 0.0, max: 100.0, unit: "%", decimals: 0, step: 1.0, track_stops: 2 },
];

/// CMYK through PhotoCraft's profile conversion, in percent. The inks the colour was picked as
/// (#4196) are the model's values for that colour while they still give it.
pub(crate) struct CmykModel(pub(crate) Option<[f32; 4]>);

impl ColorModel for CmykModel {
    fn id(&self) -> &'static str {
        CMYK_ID
    }
    fn name(&self) -> &'static str {
        "CMYK"
    }
    fn channels(&self) -> &[Channel] {
        &CMYK_CHANNELS
    }
    fn from_srgb(&self, color: Srgba) -> ModelValues {
        let rgb = [color.r, color.g, color.b];
        let inks =
            self.0.filter(|k| hex(photocraft_color::convert::cmyk_to_rgb(*k)) == hex(rgb)).unwrap_or_else(|| photocraft_color::convert::rgb_to_cmyk(rgb));
        ModelValues::new(CMYK_ID, &inks.map(|v| v * 100.0))
    }
    fn to_srgb(&self, values: &[f32; MAX_CHANNELS]) -> Srgba {
        let rgb = photocraft_color::convert::cmyk_to_rgb([0, 1, 2, 3].map(|i| values[i] / 100.0)).map(|c| c.clamp(0.0, 1.0));
        srgba(rgb)
    }
    fn default_on(&self) -> DefaultOn {
        DefaultOn { full: true, compact: false }
    }
}

/// Add to Swatches was clicked: add the picked colour to the Swatches panel.
pub fn take_add_swatch(app: &mut PhotocraftApp, f: &mut Map<String, Value>) {
    if f.remove("__addSwatch").and_then(|v| v.as_bool()) == Some(true) {
        crate::swatches_ui::add_from_picker(app, f);
    }
}

/// OK: set the foreground or background colour.
pub fn confirm(app: &mut PhotocraftApp, f: &Map<String, Value>) -> Result<Value, String> {
    if crate::type_panels_ui::color_picker::owns(f) {
        return crate::type_panels_ui::color_picker::confirm(app, f);
    }
    if crate::layer_style::color_picker::owns(f) {
        return crate::layer_style::color_picker::confirm(app, f);
    }
    if crate::solid_fill_ui::owns_new(f) {
        return crate::solid_fill_ui::confirm_new(app, f);
    }
    if crate::solid_fill_ui::owns(f) {
        return crate::solid_fill_ui::confirm(app, f);
    }
    if crate::shape_fill_ui::owns(f) {
        return crate::shape_fill_ui::confirm(app, f);
    }
    let target = f.get("__colorPicker").and_then(Value::as_str).unwrap_or("foreground");
    let color = f.get("color").and_then(Value::as_str).unwrap_or("#000000");
    if let Some(cmd) = f.get("__command").and_then(Value::as_str) {
        let mut p = f.get("__params").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
        p["color"] = json!(color);
        return app.run(cmd, p);
    }
    if target == "field" {
        let (Some(dialog), Some(field)) = (f.get("__dialog").and_then(Value::as_u64), f.get("__field").and_then(Value::as_str)) else {
            return Err("the Color Picker has no field to set".into());
        };
        let d = app.ui.dialog_mut(dialog).ok_or("the dialog this colour was for is closed")?;
        set_field(&mut d.fields, field, json!(color))?;
        return Ok(json!({ "color": color }));
    }
    if target == "cropShield" {
        crate::crop_shield::set_custom_color(app, color)?;
        return Ok(json!({ "color": color }));
    }
    // The Note tool's colour for new notes (#2144).
    if target == "noteColor" {
        app.ui.analysis.note_color = parse_hex(color).ok_or("not a colour")?;
        return Ok(json!({ "color": color }));
    }
    if target == "pasteboard" {
        // The pasteboard menu's Select Custom Color…: the custom colour, and use it.
        return app.run("prefs.set", json!({"values": {"interface.canvasColor": "custom", "interface.canvasCustomColor": color}}));
    }
    let mut params = json!({ target: color });
    // The CMYK values the person entered stay the colour's inks: a CMYK document is filled with them (#4196).
    if let Some(cmyk) = parse_hex(color).and_then(|rgb| kept::<4>(f, "__cmyk", rgb, photocraft_color::convert::cmyk_to_rgb)) {
        params[format!("{target}Cmyk")] = json!(cmyk);
    }
    let r = app.run("tools.setColors", params)?;
    if target == "foreground" {
        crate::apply_color::foreground_changed(app, None);
    }
    Ok(r)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod screen_integration_tests {
    use super::*;
    use crate::screen_picker::{Capture, Pending};
    use craft_ui::theme::scale::Space;
    use egui::{Rect, UiBuilder, pos2, vec2};
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn text_swatch_opens_full_dialog_and_screen_pick_waits_for_ok() {
        let mut session = photocraft_engine::Session::new();
        session.execute("file.new", json!({"width":160,"height":120})).unwrap();
        let id = session.execute("type.create", json!({"text":"Hello world","color":"#000000","size":24,"x":10,"y":60})).unwrap()["layer"].as_u64().unwrap();
        let services = crate::Services {
            screen_pick: Some(Box::new(|_| {
                let (tx, receiver) = std::sync::mpsc::channel();
                tx.send(Ok(Capture::Color(Some([1.0, 136.0 / 255.0, 0.0])))).unwrap();
                Pending { receiver, cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)) }
            })),
            ..Default::default()
        };
        let mut h = Harness::builder().with_size(vec2(1440.0, 900.0)).with_max_steps(64).build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            let mut app = PhotocraftApp::new(session, services);
            app.ui.tool = crate::state::Tool::Type;
            app
        });
        h.run_steps(8);
        let before = h.state().session.active().unwrap().history.entries().len();
        let fg = h.state().session.tools.foreground;
        h.get_by_label("Set the text color").click();
        h.run_steps(6);
        assert_eq!(h.state().ui.dialogs.last().unwrap().fields["__label"], "Color Picker (Text Color)");
        h.get_by_label("Pick screen color").click();
        h.run_steps(6);
        assert_eq!(h.state().ui.dialogs.last().unwrap().fields["color"], "#ff8800");
        assert_eq!(h.state().session.active().unwrap().history.entries().len(), before, "sampling is only a dialog field change");
        assert_eq!(h.state().session.tools.foreground, fg);
        h.get_by_label("OK").click();
        h.run_steps(4);
        let doc = &h.state().session.active().unwrap().doc;
        let layer = doc.layer(photocraft_doc::LayerId(id)).unwrap();
        let photocraft_doc::LayerContent::Text(text) = &layer.content else { panic!("not text") };
        assert_eq!(text.color.to_rgba8(), [255, 136, 0, 255]);
        assert_eq!(h.state().session.active().unwrap().history.entries().len(), before + 1);
    }
}
