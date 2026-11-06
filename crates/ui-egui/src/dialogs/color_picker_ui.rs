//! Photoshop's Color Picker (Foreground / Background Color) on the library's [`ColorPickerDialog`],
//! shown by the `DialogHost`. Every colour request goes the same way: a component or a command asks
//! for a colour ([`open`], [`open_for_command`], [`open_for_field`], [`open_for_target`],
//! [`request`]), the picker opens from a [`ColorRequest`], and the answer reaches the asker in
//! [`settle`]. The record of an open picker is a [`Hosted`] of kind `ColorPicker`; its `private`
//! map holds the target (`__colorPicker`, `__command`, ...) that [`confirm`] reads.

use craft::color::ui::{ColorPickerDialog, ColorPickerResult, PickedColor};
use craft::theme::{Channel, ColorModel, DefaultOn, MAX_CHANNELS, ModelValues, Srgba};
use craft::ui::dialog::{ColorRequest, DialogResult};
use serde_json::{Map, Value, json};

use super::hosted::{Hosted, HostedKind, Pending};
use crate::PhotocraftApp;
use crate::state::{Dialog, DialogKind};

/// Colour differences the CMYK round trip leaves inside the printable gamut (dE <= 0.4, black ~ 1.4
/// with black-point compensation) stay below this; out-of-gamut colours move far more.
const GAMUT_DE: f32 = 3.0;

/// The nearest printable colour, when `rgb` is outside the CMYK gamut (Photoshop's warning).
pub fn out_of_gamut(rgb: [f32; 3]) -> Option<[f32; 3]> {
    use photocraft_color::convert::{cmyk_to_rgb, rgb_to_cmyk, srgb_to_lab};
    let back = cmyk_to_rgb(rgb_to_cmyk(rgb)).map(|c| c.clamp(0.0, 1.0));
    let (a, b) = (srgb_to_lab(rgb), srgb_to_lab(back));
    let de = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    (de > GAMUT_DE).then_some(back)
}

pub use craft::color::{not_web_safe, web_safe};

/// The colour of `text`: six hex digits after an optional `#`. PhotoCraft's hex fields take
/// exactly six; the shorthand and alpha forms `Srgba::from_hex` also reads are refused.
pub fn parse_hex(text: &str) -> Option<[f32; 3]> {
    if text.trim().trim_start_matches('#').len() != 6 {
        return None;
    }
    craft::theme::Srgba::from_hex(text).map(|c| [c.r, c.g, c.b])
}

/// `color` as lowercase `#rrggbb`.
pub fn hex(color: [f32; 3]) -> String {
    craft::theme::Srgba { r: color[0], g: color[1], b: color[2], a: 1.0 }.to_hex()
}

type Fields = Map<String, Value>;

/// Open the picker for the foreground or background colour.
pub fn open(app: &mut PhotocraftApp, target: &str) -> u64 {
    let c = if target == "background" { app.session.tools.background } else { app.session.tools.foreground };
    let rgb = [c[0], c[1], c[2]];
    let label = if target == "background" { "Color Picker (Background Color)" } else { "Color Picker (Foreground Color)" };
    // A colour that was picked as CMYK shows the inks it was picked as, not what its sRGB stand-in converts to (#4196).
    let kept = app.session.tools.picked_cmyk(rgb);
    open_picker(app, target, label, rgb, kept, Fields::new())
}

/// Opens the library's picker for the colour `target` names and records it. The compare half of
/// the picker follows Only Web Colors, so `__orig` is the opening colour the picker shows.
fn open_picker(app: &mut PhotocraftApp, target: &str, label: &str, rgb: [f32; 3], kept: Option<[f32; 4]>, mut private: Fields) -> u64 {
    let id = app.ui.alloc_id();
    let key = format!("{target}.{id}");
    let web_only = crate::host_settings::recall_ref(app, "colorPicker.memory").and_then(|v| v.get("webColors").and_then(Value::as_bool)).unwrap_or(false);
    let orig = if web_only { web_safe(rgb) } else { rgb };
    let request = ColorRequest::new(key.clone(), srgba(rgb)).title(label);
    let mut dialog = ColorPickerDialog::from_request(request).cmyk(&crate::cmyk_converter::working(app));
    if let Some(inks) = kept {
        dialog = dialog.kept(ModelValues::new(CMYK_ID, &inks.map(|v| v * 100.0)));
    }
    let did = app.ui.host.open(dialog);
    private.insert("__colorPicker".into(), json!(target));
    private.insert("__label".into(), json!(label));
    private.insert("__orig".into(), json!(hex(orig)));
    app.ui.hosted.push(Hosted {
        kind: HostedKind::ColorPicker,
        id,
        did,
        key,
        params: Vec::new(),
        title: label.to_owned(),
        extra: Fields::new(),
        private,
        documents: Vec::new(),
        table: Pending::default(),
        preview: Default::default(),
        remembers: false,
    });
    id
}

/// Open the picker on `rgb` for something other than the tool colours: OK runs `command` with
/// `params` plus `"color": "#rrggbb"` (e.g. a gradient stop's colour).
pub fn open_for_command(app: &mut PhotocraftApp, label: &str, rgb: [f32; 3], command: &str, params: Value) -> u64 {
    let mut f = Fields::new();
    f.insert("__command".into(), json!(command));
    f.insert("__params".into(), params);
    open_picker(app, "command", label, rgb, None, f)
}

/// [`open_for_command`] with more private fields (a marker a handler claims the answer by).
pub fn open_for_command_marked(app: &mut PhotocraftApp, label: &str, rgb: [f32; 3], command: &str, params: Value, mut private: Fields) -> u64 {
    private.insert("__command".into(), json!(command));
    private.insert("__params".into(), params);
    open_picker(app, "command", label, rgb, None, private)
}

/// [`open_for_command`] for a colour picked as CMYK `inks`: the fields show the inks (#8568).
pub fn open_for_command_kept(app: &mut PhotocraftApp, label: &str, rgb: [f32; 3], inks: [f32; 4], command: &str, params: Value) -> u64 {
    let mut f = Fields::new();
    f.insert("__command".into(), json!(command));
    f.insert("__params".into(), params);
    open_picker(app, "command", label, rgb, Some(inks), f)
}

/// Open the picker on `rgb` for a colour field of another dialog (e.g. Edit > Fill's Color...): OK
/// writes `"#rrggbb"` into `field` of dialog `dialog`, which stays open; Cancel leaves it alone.
pub fn open_for_field(app: &mut PhotocraftApp, label: &str, rgb: [f32; 3], dialog: u64, field: &str) -> u64 {
    let mut f = Fields::new();
    f.insert("__dialog".into(), json!(dialog));
    f.insert("__field".into(), json!(field));
    open_picker(app, "field", label, rgb, None, f)
}

/// Open the picker on `rgb` for a colour the shell keeps itself (e.g. the Crop shield's custom
/// colour): OK hands `"#rrggbb"` to `target`'s setter in [`confirm`]; Cancel leaves it alone.
pub fn open_for_target(app: &mut PhotocraftApp, target: &str, label: &str, rgb: [f32; 3]) -> u64 {
    open_picker(app, target, label, rgb, None, Fields::new())
}

/// Open the picker for a transaction that keeps its own target in `private` (a text style, a
/// Layer Style parameter): its marker key names the handler [`confirm`] calls.
pub(crate) fn open_with(app: &mut PhotocraftApp, label: &str, rgb: [f32; 3], private: Fields) -> u64 {
    open_picker(app, "dialog", label, rgb, None, private)
}

/// Set by a dialog body (to the picker's title) to open the Color Picker on its `color` field.
const REQUEST: &str = "__pickColor";

/// A dialog body asks for the Color Picker on its `color` field (Edit > Fill's Color... and colour
/// swatch, Edit > Stroke's swatch). The dialog loop opens it ([`open_requested`]), since that
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

/// Whether `f` is the private record of a Color Picker.
pub fn owns(f: &Map<String, Value>) -> bool {
    f.contains_key("__colorPicker")
}

/// The open Color Pickers as dialogs, oldest first: each one's record fields plus the colour it
/// shows now (`color`, and the inks or Lab values it was typed in).
pub fn pickers(app: &PhotocraftApp) -> Vec<Dialog> {
    app.ui
        .hosted
        .iter()
        .filter(|h| h.kind == HostedKind::ColorPicker)
        .filter_map(|h| Some(Dialog { id: h.id, kind: DialogKind::Command, fields: fields_of(&app.ui, h.id)? }))
        .collect()
}

/// The Color Picker that takes the input, when one is on top of the stack.
pub fn top(app: &PhotocraftApp) -> Option<u64> {
    let top = app.ui.host.top_id()?;
    app.ui.hosted.iter().find(|h| h.kind == HostedKind::ColorPicker && h.did == top).map(|h| h.id)
}

/// [`top`] as a dialog.
pub fn top_dialog(app: &PhotocraftApp) -> Option<Dialog> {
    let id = top(app)?;
    Some(Dialog { id, kind: DialogKind::Command, fields: fields_of(&app.ui, id)? })
}

/// The fields of picker `id`: its record plus the colour it shows.
pub fn fields_of(ui: &crate::state::UiState, id: u64) -> Option<Fields> {
    let record = ui.hosted(id).filter(|h| h.kind == HostedKind::ColorPicker)?;
    let dialog = ui.host.get_by_id::<ColorPickerDialog>(record.did)?;
    Some(picked_fields(record.private.clone(), &dialog.current()))
}

/// `ui.dialog.set` on picker `id`: `color` (`#rrggbb`) is the colour an edit would give; the
/// picker's other fields are its own.
pub fn set_color(app: &mut PhotocraftApp, id: u64, field: &str, value: &Value) -> Result<(), String> {
    if field != "color" {
        return Err(format!("the Color Picker has no `{field}` field to set"));
    }
    let rgb = value.as_str().and_then(parse_hex).ok_or("not a colour")?;
    let did = app.ui.hosted(id).map(|h| h.did).ok_or_else(|| format!("no dialog {id}"))?;
    let dialog = app.ui.host.get_mut_by_id::<ColorPickerDialog>(did).ok_or_else(|| format!("no dialog {id}"))?;
    // `PickedColor` has no constructor outside the library: edit the current one.
    let mut picked = dialog.current();
    (picked.color, picked.model) = (srgba(rgb), None);
    dialog.set_color(picked);
    Ok(())
}

/// Types CMYK `inks` into picker `id` as the CMYK fields would: the colour is their sRGB value and
/// the inks stay as typed.
#[cfg(test)]
pub(crate) fn set_inks(app: &mut PhotocraftApp, id: u64, inks: [f32; 4]) {
    let did = app.ui.hosted(id).expect("an open picker").did;
    let dialog = app.ui.host.get_mut_by_id::<ColorPickerDialog>(did).expect("an open picker");
    let model = CmykModel(None);
    let values = ModelValues::new(CMYK_ID, &inks.map(|v| v * 100.0));
    let mut picked = dialog.current();
    (picked.color, picked.model) = (model.to_srgb(&values.values), Some(values));
    dialog.set_color(picked);
}

/// `private` plus the picked colour: `color`, and the values of the model it was edited in.
fn picked_fields(mut f: Fields, picked: &PickedColor) -> Fields {
    let rgb = [picked.color.r, picked.color.g, picked.color.b];
    f.insert("color".into(), json!(hex(rgb)));
    if let Some(m) = picked.model {
        match m.model {
            CMYK_ID => {
                f.insert("__cmyk".into(), json!([0, 1, 2, 3].map(|i| m.values[i] / 100.0)));
            }
            "lab" => {
                f.insert("__lab".into(), json!([m.values[0], m.values[1], m.values[2]]));
            }
            _ => {}
        }
    }
    f
}

/// The CMYK values typed into the picker, while they still match its colour: the inks a command
/// should keep in a CMYK document (#4196, #8568).
pub(crate) fn picked_cmyk(f: &Map<String, Value>) -> Option<[f32; 4]> {
    let rgb = f.get("color").and_then(Value::as_str).and_then(parse_hex)?;
    let v: [f32; 4] = f.get("__cmyk").and_then(|v| serde_json::from_value(v.clone()).ok())?;
    (hex(photocraft_color::convert::cmyk_to_rgb(v)) == hex(rgb)).then_some(v)
}

/// The answers of the open pickers, after `DialogHost::show`: OK runs the target, Cancel drops
/// the record; Add to Swatches adds the colour and leaves the picker open.
pub fn collect(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let ids: Vec<u64> = app.ui.hosted.iter().filter(|h| h.kind == HostedKind::ColorPicker).map(|h| h.id).collect();
    for id in ids {
        let Some(did) = app.ui.hosted(id).map(|h| h.did) else { continue };
        let swatch = app.ui.host.get_mut_by_id::<ColorPickerDialog>(did).and_then(ColorPickerDialog::take_add_to_swatches);
        if let Some(picked) = swatch {
            let fields = picked_fields(Fields::new(), &picked);
            crate::swatches_ui::add_from_picker(app, &fields);
        }
        if let Some(Err(error)) = settle(app, ctx, id) {
            app.ui.status = error;
        }
    }
}

/// Takes picker `id`'s answer, if it has one.
pub fn settle(app: &mut PhotocraftApp, _ctx: &egui::Context, id: u64) -> Option<Result<Value, String>> {
    let record = app.ui.hosted(id).filter(|h| h.kind == HostedKind::ColorPicker)?.clone();
    let answer = app.ui.host.take_by_id::<ColorPickerDialog>(record.did)?;
    app.ui.hosted.retain(|h| h.id != id);
    match answer {
        DialogResult::Ok(ColorPickerResult { picked, .. }) => Some(confirm(app, &picked_fields(record.private, &picked))),
        DialogResult::Cancel => None,
    }
}

/// Closes picker `id` without an answer.
pub fn close(app: &mut PhotocraftApp, ctx: &egui::Context, id: u64) {
    let Some(did) = app.ui.hosted(id).map(|h| h.did) else { return };
    app.ui.host.cancel_by_id::<ColorPickerDialog>(ctx, did);
    app.ui.host.take_by_id::<ColorPickerDialog>(did);
    app.ui.hosted.retain(|h| h.id != id);
}

/// Closes the pickers that belong to `parent` (a dialog they set a colour in, or the Layer Style
/// dialog their parameter is in).
pub fn close_children(app: &mut PhotocraftApp, ctx: &egui::Context, parent: u64) {
    let children: Vec<u64> = app
        .ui
        .hosted
        .iter()
        .filter(|h| h.kind == HostedKind::ColorPicker)
        .filter(|h| h.private.get("__dialog").and_then(Value::as_u64) == Some(parent) || crate::layer_style::color_picker::child_of(&h.private, parent))
        .map(|h| h.id)
        .collect();
    for id in children {
        close(app, ctx, id);
    }
}

/// Whether a picker opened for `parent`'s colour field is open.
pub fn has_child(app: &PhotocraftApp, parent: u64) -> bool {
    app.ui.hosted.iter().any(|h| h.kind == HostedKind::ColorPicker && h.private.get("__dialog").and_then(Value::as_u64) == Some(parent))
}

/// The canvas's colour at document point (x, y) goes to the armed dropper (a picker's).
pub fn sample_at(app: &mut PhotocraftApp, ctx: &egui::Context, x: f64, y: f64) {
    if let Some(rgb) = crate::canvas::composite_color(app, x, y) {
        craft::color::ui::DropperState::deliver(ctx, srgba(rgb));
    }
}

pub(crate) fn srgba(c: [f32; 3]) -> Srgba {
    Srgba::rgb(c[0], c[1], c[2])
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
        if app.ui.hosted(dialog).is_some() {
            crate::dialogs::hosted::set_color(app, dialog, field, color)?;
            return Ok(json!({ "color": color }));
        }
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
    if let Some(cmyk) = picked_cmyk(f) {
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
