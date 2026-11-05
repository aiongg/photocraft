//! Filter dialogs on the library's `ParamDialog`, shown by the `DialogHost`.
//!
//! The legacy opening fields (`opening_fields`) stay the source of the values: defaults, remembered
//! values and the command's hooks. They are split into the library's [`DialogFields`] (what the
//! dialog draws), `extra` (JSON the library cannot hold) and the private `__` keys, and `Wire`
//! converts between that and the JSON automation and the commands use.

use craft::ui::controls::SliderScale;
use craft::ui::dialog::{DialogFields, FieldValue, ParamDialog, ParamKind as Lib, PathKind};
use craft::ui::localized::Unit;
use egui::{Response, Sense, Ui};
use serde_json::{Map, Value, json};

use super::{Kind, REPLACE_COLOR, RESULT_FROM, RESULT_PICKED, choice_label, installed_choices, installed_filter, parse_spec, source_label};
use crate::PhotocraftApp;
use crate::dialogs::hosted::{Hosted, HostedKind, Pending, open_params};

type LibParam = craft::ui::dialog::Param;

/// Whether the filter dialog of `command`, with parameters `spec`, is a `ParamDialog`. The rest
/// stays on the legacy list, each blocked by a library row:
/// - a seed row with a Random button (row 148);
/// - Replace Color: the pipette toggle (148), the Result picker (151), the points list (152);
/// - Indexed Color and Radial Blur: `Param::full_width` (149);
/// - Color Settings: `ParamDialog::below` (150);
/// - Color Table: the colour list (152);
/// - Bitmap: its pattern picker reads the app (`preset_panels::pattern_picker` takes `&PhotocraftApp`);
/// - Select and Mask and the Blur Gallery workspaces: canvas-side keys (gap K).
pub(crate) fn hosts(command: &str, spec: &str) -> bool {
    const LEGACY: [&str; 6] =
        [REPLACE_COLOR, "image.mode.indexedColor", super::RADIAL_BLUR, "edit.colorSettings", "image.mode.colorTable", crate::bitmap_dialog::COMMAND];
    !(LEGACY.contains(&command)
        || command == crate::select_mask_view::COMMAND
        || crate::blur_pins::handles(command)
        || parse_spec(spec).iter().any(|p| p.key == "seed"))
}

/// How automation's JSON and the library's fields correspond for one hosted filter dialog.
pub(crate) struct Wire<'a> {
    command: &'a str,
    params: &'a [LibParam],
    documents: &'a [String],
    whole: &'a [String],
}

impl<'a> Wire<'a> {
    pub(crate) fn of(record: &'a Hosted) -> Self {
        Self { command: &record.key, params: &record.params, documents: &record.documents, whole: &record.whole }
    }

    fn param(&self, key: &str) -> Option<&LibParam> {
        self.params.iter().find(|p| p.key == key)
    }

    fn channel(index: i64) -> String {
        super::channel_choice(usize::try_from(index).unwrap_or(0))
    }

    /// Sets `key` from a JSON value by the row's kind. Lens Blur's Source takes a channel as
    /// `depthMap: "channel"` plus `depthChannel`, which the dialog shows as one menu entry.
    pub(crate) fn set(&self, fields: &mut DialogFields, key: &str, value: &Value) -> Result<(), String> {
        if self.command == super::LENS_BLUR {
            match key {
                "depthChannel" => {
                    let index = match value {
                        Value::String(name) => self.channel_named(name),
                        other => other.as_i64(),
                    }
                    .ok_or_else(|| format!("{value} is not a channel of this document"))?;
                    if fields.text("depthMap").is_some_and(|m| m.starts_with("channel:")) {
                        fields.set("depthMap", Self::channel(index));
                    }
                    fields.set("depthChannel", index);
                    return Ok(());
                }
                "depthMap" if value.as_str() == Some("channel") => {
                    let entry = Self::channel(fields.int("depthChannel").unwrap_or(0));
                    let Some(Lib::Choice { options, .. }) = self.param("depthMap").map(|p| &p.kind) else { return Err("no Source menu".into()) };
                    if !options.iter().any(|(id, _)| *id == entry) {
                        return Err("this document has no such channel".into());
                    }
                    fields.set("depthMap", entry);
                    return Ok(());
                }
                _ => {}
            }
        }
        let value = self.value(fields, key, value)?;
        fields.set(key, value);
        Ok(())
    }

    /// The index of the alpha channel called `name`, by the Source menu's entries.
    fn channel_named(&self, name: &str) -> Option<i64> {
        let Lib::Choice { options, .. } = &self.param("depthMap")?.kind else { return None };
        let id = options.iter().find(|(id, label)| id.starts_with("channel:") && label == name)?.0.as_str();
        id.strip_prefix("channel:")?.parse().ok()
    }

    fn value(&self, fields: &DialogFields, key: &str, value: &Value) -> Result<FieldValue, String> {
        let wrong = || format!("{value} is not a value this field takes");
        if self.documents.iter().any(|d| d == key) {
            return value.as_i64().map(|i| FieldValue::Text(i.to_string())).ok_or_else(wrong);
        }
        match self.param(key).map(|p| &p.kind) {
            Some(Lib::Color { .. }) => value.as_str().and_then(crate::color_picker_ui::parse_hex).map(FieldValue::Color).ok_or_else(wrong),
            Some(Lib::Grid { .. }) => {
                let cells: Option<Vec<i64>> =
                    value.as_array().map(|a| a.iter().map(|v| v.as_f64().map(|n| n.round() as i64)).collect::<Option<_>>()).unwrap_or(None);
                cells.map(FieldValue::Grid).ok_or_else(wrong)
            }
            Some(kind) => crate::dialogs::hosted::field_from_json(kind, value),
            // A key a host-drawn row keeps (`centerY`), or one the command takes that no row shows.
            None => match (value, fields.get(key)) {
                (Value::Number(n), Some(FieldValue::Number(_))) => n.as_f64().map(FieldValue::Number).ok_or_else(wrong),
                (Value::Bool(b), _) => Ok(FieldValue::Bool(*b)),
                (Value::Number(n), _) => Ok(n.as_i64().map_or_else(|| FieldValue::Number(n.as_f64().unwrap_or_default()), FieldValue::Int)),
                (Value::String(s), _) => Ok(FieldValue::Text(s.clone())),
                (Value::Array(a), _) if a.iter().all(|v| v.is_i64()) => Ok(FieldValue::Grid(a.iter().filter_map(Value::as_i64).collect())),
                _ => Err(wrong()),
            },
        }
    }

    /// The library fields for a legacy fields map, and the JSON the library cannot hold.
    pub(crate) fn fields(&self, map: &Map<String, Value>) -> (DialogFields, Map<String, Value>) {
        let (mut fields, mut extra) = (DialogFields::new(), Map::new());
        for (key, value) in map.iter().filter(|(k, _)| !k.starts_with("__")) {
            if self.set(&mut fields, key, value).is_err() {
                extra.insert(key.clone(), value.clone());
            }
        }
        (fields, extra)
    }

    /// The command's JSON for the dialog's fields: a colour as `#rrggbb`, a document by its index,
    /// a text left empty absent, a slider value that opened whole as an integer.
    pub(crate) fn json(&self, fields: &DialogFields) -> Map<String, Value> {
        let mut out = Map::new();
        let lens = self.command == super::LENS_BLUR;
        for (key, value) in fields.iter() {
            let json = match value {
                FieldValue::Color(c) => json!(crate::color_picker_ui::hex(*c)),
                FieldValue::Text(t) if self.documents.iter().any(|d| d == key) => json!(t.parse::<i64>().unwrap_or(-1)),
                FieldValue::Text(t) if t.is_empty() => continue,
                FieldValue::Text(t) if lens && key == "depthMap" && t.starts_with("channel:") => {
                    out.insert("depthChannel".into(), json!(t["channel:".len()..].parse::<i64>().unwrap_or(0)));
                    json!("channel")
                }
                _ if lens && key == "depthChannel" => continue,
                _ if key == RESULT_PICK => continue,
                // A slider holds an f64; a value that opened as an integer reads back as one.
                FieldValue::Number(n) if self.whole.iter().any(|w| w == key) && n.fract() == 0.0 && n.abs() < 9.0e15 => json!(*n as i64),
                other => match serde_json::to_value(other) {
                    Ok(v) => v,
                    Err(_) => continue,
                },
            };
            out.insert(key.to_owned(), json);
        }
        out
    }
}

/// The unit PhotoCraft shows after a range's number.
fn unit_of(command: &str, key: &str, max: f32) -> Option<Unit> {
    if super::is_pixel_param_of(command, key) {
        Some(Unit::new("px"))
    } else if key == "angle" {
        Some(Unit::new("°"))
    } else if key == "amount" && max <= 500.0 {
        Some(Unit::new("%"))
    } else {
        None
    }
}

/// The library rows for `spec`, and the parameters that hold a document's index. `fields` is the
/// opening map, which carries what the menus list (`__channels`, `__profiles`, `__docs`).
fn params_for(command: &str, spec: &str, fields: &Map<String, Value>) -> (Vec<LibParam>, Vec<String>) {
    let (mut out, mut documents) = (Vec::new(), Vec::new());
    for p in parse_spec(spec) {
        let name = source_label(&p.key);
        let key = p.key.clone();
        match p.kind {
            Kind::Range { min, max, default } => {
                if command == "image.mode.indexedColor" && key == "colors" {
                    out.push(LibParam::custom("colors", name, |ui, fields, _| color_count(ui, fields)));
                    continue;
                }
                if command == super::RADIAL_BLUR && matches!(key.as_str(), "centerX" | "centerY") {
                    if key == "centerX" {
                        out.push(LibParam::custom("centerX", "Center", |ui, fields, _| super::zoom_center::row(ui, fields)));
                    }
                    continue;
                }
                let kind = if super::uses_angle_dial(command, &key) {
                    Lib::Angle { default: f64::from(default) }
                } else {
                    let scale = if super::uses_logarithmic_slider(min, max) { SliderScale::Log } else { SliderScale::Linear };
                    Lib::Range { min: f64::from(min), max: f64::from(max), default: f64::from(default), scale }
                };
                let mut row = LibParam::new(key.as_str(), name, kind);
                if let Some(unit) = unit_of(command, &key, max) {
                    row = row.unit(unit);
                }
                // Replace Color's Range only applies with Localized Color Clusters on, as in Photoshop.
                if command == REPLACE_COLOR && key == "range" {
                    row = row.enabled_when("localized", true);
                }
                out.push(row);
            }
            Kind::Choice { options, default } => {
                let mut listed: Vec<(String, String)> = options.iter().map(|o| (o.clone(), choice_label(o))).collect();
                // Lens Blur's Source: each alpha channel by its name, in place of `channel`.
                if command == super::LENS_BLUR && key == "depthMap" {
                    listed.retain(|(id, _)| id != "channel");
                    let names = fields.get("__channels").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str);
                    listed.extend(names.enumerate().map(|(i, n)| (super::channel_choice(i), n.to_owned())));
                }
                // The profile menus also list the profiles installed in the system, by description.
                let lists_installed = installed_filter(command, &key).is_some();
                if lists_installed {
                    listed.extend(installed_choices(command, &key, &photocraft_engine::installed_profiles::installed()));
                }
                // Assign Profile: only what the document's colour mode can take (#4951).
                if key == "profile"
                    && let Some(allowed) = fields.get("__profiles").and_then(Value::as_array)
                {
                    listed.retain(|(id, _)| allowed.iter().any(|a| a.as_str() == Some(id.as_str())));
                }
                // A profile given by path that isn't listed still shows, by its file name.
                if lists_installed
                    && let Some(current) = fields.get(&key).and_then(Value::as_str)
                    && !listed.iter().any(|(id, _)| id == current)
                {
                    let stem = std::path::Path::new(current).file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    listed.push((current.to_owned(), stem));
                }
                let default = default.or_else(|| listed.first().map(|(id, _)| id.clone()));
                out.push(LibParam::new(key, name, Lib::Choice { options: listed, default }));
            }
            Kind::Bool(default) => out.push(LibParam::new(key, name, Lib::Bool { default })),
            Kind::Text => {
                let mut row = LibParam::new(key.as_str(), name, Lib::Text { default: String::new() });
                // Install Plug-in…: pick the .wasm file instead of typing its path (#7531).
                if command == "plugin.install" && key == "path" {
                    row = row.browse(PathKind::open_file(&["wasm"]));
                }
                out.push(row);
            }
            Kind::Document => {
                let names = fields.get("__docs").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str);
                let mut listed = vec![("-1".to_owned(), tl!("None").to_owned())];
                listed.extend(names.enumerate().map(|(i, n)| (i.to_string(), n.to_owned())));
                documents.push(key.clone());
                out.push(LibParam::new(key, name, Lib::Choice { options: listed, default: Some("-1".into()) }));
            }
            Kind::Grid(cells) => out.push(LibParam::new(key, name, Lib::Grid { cells, default: 0 })),
            Kind::Json => {}
            Kind::Color(default) => out.push(LibParam::new(key, name, Lib::Color { default })),
            Kind::Int { default } => out.push(LibParam::new(key, name, Lib::Int { min: Some(-30000), max: Some(30000), default })),
        }
    }
    if command == REPLACE_COLOR {
        out.push(LibParam::custom("result", "Result", |ui, fields, _| replace_color_result(ui, fields)));
    }
    // Color Settings: the monitor profile in use, which the app pushes before each pass.
    if command == "edit.colorSettings" {
        out.push(LibParam::custom(NOTE, "", |ui, fields, _| {
            let t = craft::ui::theme::current(ui.ctx());
            use craft::ui::theme::ToEgui as _;
            let note = fields.text(NOTE).unwrap_or_default().to_owned();
            ui.add(egui::Label::new(egui::RichText::new(note).color(t.color.text.muted.to_egui())).wrap())
        }));
    }
    (out, documents)
}

const NOTE: &str = "__note";
/// Set by the Result swatch of Replace Color when pressed; the app opens the Color Picker for it.
const RESULT_PICK: &str = "resultPick";

/// Indexed Color's colour count: a number field with a slider that snaps to 8, 16 … 256.
fn color_count(ui: &mut Ui, fields: &mut DialogFields) -> Response {
    let mut count = fields.int("colors").unwrap_or(256).clamp(2, 256) as u32;
    let response = crate::widgets::color_count_row(ui, &super::label("colors"), &mut count);
    fields.set("colors", i64::from(count));
    response
}

/// Replace Color's Result swatch (#6278): the sampled colour after the Hue, Saturation and
/// Lightness shift. Pressing it asks for the Color Picker (`requests`).
fn replace_color_result(ui: &mut Ui, fields: &mut DialogFields) -> Response {
    use photocraft_algo::tone;
    let t = craft::ui::theme::current(ui.ctx());
    use craft::ui::theme::ToEgui as _;
    let from = fields.color("color").unwrap_or([0.0; 3]);
    let num = |key: &str| fields.number(key).unwrap_or(0.0) as f32;
    let rgb = tone::replace_color_result(from, num("hue"), num("saturation"), num("lightness"));
    let bytes = rgb.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    let name = tl!("Result");
    let mut response = ui.allocate_response(egui::Vec2::ZERO, Sense::hover());
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let swatch = crate::widgets::color_swatch_button(ui, egui::Color32::from_rgb(bytes[0], bytes[1], bytes[2]), name);
        ui.label(egui::RichText::new(crate::color_picker_ui::hex(rgb)).color(t.color.text.muted.to_egui()).monospace());
        if swatch.clicked() {
            fields.set(RESULT_PICK, true);
        }
        response = swatch;
    });
    response
}

/// Opens the filter dialog of `command`: on the host, or on the legacy list for the commands
/// [`hosts`] leaves there.
pub fn open(app: &mut PhotocraftApp, command: &str) -> Option<u64> {
    let spec = photocraft_engine::commands::find(command)?;
    if !hosts(command, spec.params) {
        return super::open_legacy(app, command);
    }
    let map = super::opening_fields(app, command)?;
    Some(host(app, command, spec.label, spec.params, map))
}

/// The hosted dialog of `command` whatever [`hosts`] says: the tests of rows the legacy route still
/// serves in the app.
#[cfg(test)]
pub(crate) fn open_forced(app: &mut PhotocraftApp, command: &str) -> Option<u64> {
    let spec = photocraft_engine::commands::find(command)?;
    let map = super::opening_fields(app, command)?;
    Some(host(app, command, spec.label, spec.params, map))
}

/// [`open`] for a filter whose parameters follow `spec` (a plug-in's) instead of the command's own;
/// `fixed` params (the plug-in id) ride along to the command.
pub fn open_with_spec(app: &mut PhotocraftApp, command: &str, label: &str, spec: &str, fixed: Map<String, Value>) -> u64 {
    if !hosts(command, spec) {
        return super::open_with_spec_legacy(app, command, label, spec, fixed);
    }
    let map = super::spec_fields(command, label, spec, fixed);
    host(app, command, label, spec, map)
}

fn host(app: &mut PhotocraftApp, command: &str, label: &str, spec: &str, map: Map<String, Value>) -> u64 {
    let (params, documents) = params_for(command, spec, &map);
    let record = Hosted {
        id: 0,
        kind: HostedKind::Params,
        key: command.to_owned(),
        params: params.clone(),
        title: label.trim_end_matches('…').to_owned(),
        extra: Map::new(),
        private: map.iter().filter(|(k, _)| k.starts_with("__")).map(|(k, v)| (k.clone(), v.clone())).collect(),
        documents,
        whole: map.iter().filter(|(_, v)| v.is_i64() || v.is_u64()).map(|(k, _)| k.clone()).collect(),
        table: Pending::default(),
        preview: Default::default(),
    };
    let wire = Wire::of(&record);
    let (values, extra) = wire.fields(&map);
    let mut dialog = ParamDialog::new(command, crate::dialogs::display_label(label), params)
        .values(values)
        // Cmd/Ctrl+Z steps the dialog's own settings (#4697), and the close box follows the platform.
        .history(true)
        .close_label(crate::dialogs::CLOSE_BOX.then(|| tl!("Close").to_string()))
        // The Preview check box sits in the bottom footer, as PhotoCraft's does.
        .preview(map.contains_key("__preview"))
        .layout(craft::ui::dialog::ParamLayout::BottomFooter);
    if crate::color_table_ui::owns(&map) {
        let pending = record.table.clone();
        dialog = dialog.footer_extra(move |ui| {
            if let Some(action) = crate::color_table_ui::buttons(ui)
                && let Ok(mut slot) = pending.0.lock()
            {
                *slot = Some(action);
            }
        });
    }
    open_params(app, dialog, Hosted { extra, ..record })
}

/// The open hosted filter dialog as the live preview reads it: its id and its JSON fields.
pub(crate) fn live_fields(app: &PhotocraftApp) -> Option<(u64, Map<String, Value>)> {
    let record = app.ui.hosted.iter().find(|h| h.kind == HostedKind::Params)?;
    Some((record.id, app.ui.dialog_fields(record.id)?))
}

/// What the app pushes into the open dialog before it draws: whether OK takes input, the preview
/// pane's image, and the Color Settings note.
pub(crate) fn push_state(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(record) = app.ui.hosted.iter().find(|h| h.kind == HostedKind::Params).cloned() else { return };
    let Some(fields) = app.ui.dialog_fields(record.id) else { return };
    let ok = crate::dialogs::enabled_for(app, &fields).0;
    let note = (record.key == "edit.colorSettings").then(|| crate::monitor_status::note(app));
    let image = live_fields(app).and_then(|(id, fields)| {
        let on = fields.get("__preview").and_then(Value::as_bool) == Some(true);
        // The record's cache is out of the record while the textures are built from the app.
        let mut cache = app.ui.hosted.iter_mut().find(|h| h.id == id).map(|h| std::mem::take(&mut h.preview))?;
        let image = crate::filter_preview_ui::pane_image(app, ctx, &mut cache, id, &fields, on);
        app.ui.hosted.iter_mut().find(|h| h.id == id)?.preview = cache;
        image
    });
    let Some(dialog) = app.ui.host.get_mut::<ParamDialog>() else { return };
    dialog.set_ok_enabled(ok);
    dialog.set_preview_image(image);
    if let Some(note) = note {
        dialog.fields_mut().set(NOTE, note);
    }
}

/// What the dialog asked for while it drew: PhotoCraft's Color Picker for a colour row, the Result
/// swatch, and the Color Table's Load and Save. Also settles what a picked Result sets.
pub(crate) fn requests(app: &mut PhotocraftApp) {
    let Some(dialog) = app.ui.host.get_mut::<ParamDialog>() else { return };
    let (key, pick, mut fields) = (dialog.key().to_owned(), dialog.pick_color(), dialog.fields().clone());
    let Some(id) = app.ui.hosted.iter().find(|h| h.key == key && h.kind == HostedKind::Params).map(|h| h.id) else { return };
    if let Some(request) = pick {
        let name = request.title.clone().unwrap_or_default();
        crate::color_picker_ui::open_for_field(app, &crate::color_picker_ui::title_for(&name), request.color.to_rgb_array(), id, &request.key);
    }
    if fields.bool(RESULT_PICK) == Some(true) {
        if let Some(dialog) = app.ui.host.get_mut::<ParamDialog>() {
            dialog.fields_mut().remove(RESULT_PICK);
        }
        let from = fields.color("color").unwrap_or([0.0; 3]);
        use photocraft_algo::tone;
        let num = |key: &str| fields.number(key).unwrap_or(0.0) as f32;
        let rgb = tone::replace_color_result(from, num("hue"), num("saturation"), num("lightness"));
        let hex = crate::color_picker_ui::hex(rgb);
        if let Some(record) = app.ui.hosted.iter_mut().find(|h| h.id == id) {
            record.private.insert(RESULT_PICKED.into(), json!(hex));
            record.private.insert(RESULT_FROM.into(), json!(hex));
        }
        crate::color_picker_ui::open_for_field(app, &crate::color_picker_ui::title_for(tl!("Result")), rgb, id, RESULT_PICKED);
    }
    // A Result picked in the Color Picker sets the three sliders so the sampled colour becomes it.
    let picked = app.ui.hosted.iter().find(|h| h.id == id).and_then(|h| {
        let (to, was) = (h.private.get(RESULT_PICKED)?, h.private.get(RESULT_FROM));
        (Some(to) != was).then(|| to.as_str().and_then(crate::color_picker_ui::parse_hex)).flatten()
    });
    if let Some(to) = picked {
        let (hue, saturation, lightness) = photocraft_algo::tone::replace_color_shift(fields.color("color").unwrap_or([0.0; 3]), to);
        if let Some(dialog) = app.ui.host.get_mut::<ParamDialog>() {
            for (key, v) in [("hue", hue), ("saturation", saturation), ("lightness", lightness)] {
                dialog.fields_mut().set(key, f64::from(v.round()));
            }
        }
        if let Some(record) = app.ui.hosted.iter_mut().find(|h| h.id == id) {
            record.private.remove(RESULT_PICKED);
            record.private.remove(RESULT_FROM);
        }
        fields = app.ui.hosted_fields(id).cloned().unwrap_or(fields);
    }
    // The Color Table: a preset picked after Load… drops the loaded colours.
    if crate::color_table_ui::owns(&Map::from_iter([("__command".to_owned(), json!(key))])) {
        if let Some(record) = app.ui.hosted.iter_mut().find(|h| h.id == id) {
            let mut table = Map::from_iter(fields.text("table").map(|t| ("table".to_owned(), json!(t))));
            table.extend(record.extra.clone());
            crate::color_table_ui::body_done(&mut table);
            record.extra.retain(|k, _| table.contains_key(k));
        }
        let action = app.ui.hosted(id).and_then(|h| h.table.0.lock().ok()?.take());
        if let Some(action) = action
            && let Some(map) = app.ui.dialog_fields(id)
        {
            crate::color_table_ui::run(app, id, &map, action);
        }
    }
}

/// Whether the top dialog is a Replace Color with its canvas eyedropper armed (hosted).
pub(crate) fn eyedropper_armed(app: &PhotocraftApp) -> Option<u64> {
    let h = app.ui.hosted.iter().find(|h| h.kind == HostedKind::Params && h.key == REPLACE_COLOR)?;
    let armed = h.private.get(super::COLOR_EYEDROPPER).and_then(Value::as_bool) == Some(true);
    let on_top = app.ui.legacy_top().is_none() && app.ui.host.stack().is_top(craft::ui::dialog::dialog_id(h.key.as_str()));
    (armed && on_top).then_some(h.id)
}

/// Writes a sampled colour and its point into the armed hosted Replace Color.
pub(crate) fn eyedropper_sample(app: &mut PhotocraftApp, id: u64, rgb: [f32; 3], x: f64, y: f64) {
    if let Some(fields) = app.ui.hosted_fields_mut(id) {
        fields.set("color", FieldValue::Color(rgb));
    }
    // Where it was picked: Localized Color Clusters match the colour near this point (#4457).
    if let Some(record) = app.ui.hosted.iter_mut().find(|h| h.id == id) {
        record.extra.insert("points".into(), json!([[x.floor(), y.floor()]]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    fn harness() -> Harness<'static, PhotocraftApp> {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 64, "height": 48})).unwrap();
        app.sync_views();
        let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                let ctx = ui.ctx().clone();
                if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                    return;
                }
                crate::dialogs::show(app, &ctx);
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
        h.run_steps(4);
        h
    }

    fn open(h: &mut Harness<'static, PhotocraftApp>, command: &str) -> u64 {
        let id = open_forced(h.state_mut(), command).unwrap();
        h.run_steps(3);
        id
    }

    #[test]
    fn a_filter_opens_on_the_host_and_ok_runs_its_command_with_the_edited_values() {
        let mut h = harness();
        let id = open(&mut h, "filter.blur.gaussianBlur");
        assert!(h.state().ui.dialogs.is_empty() && h.state().ui.host.is_open::<ParamDialog>());
        let fields = h.state().ui.dialog_fields(id).unwrap();
        assert_eq!(fields["__command"], "filter.blur.gaussianBlur");
        assert!(fields["radius"].is_number());

        crate::dialogs::hosted::set_field(h.state_mut(), id, "radius", json!(7.5)).unwrap();
        assert!(crate::dialogs::hosted::set_field(h.state_mut(), id, "radius", json!("wide")).is_err(), "a value of the wrong shape is refused");
        h.run_steps(2);
        let past = h.state().session.active().unwrap().history.past_len();
        h.get_by_label("OK").click();
        h.run_steps(4);
        assert!(h.state().ui.hosted(id).is_none() && !h.state().ui.host.is_open::<ParamDialog>());
        assert_eq!(h.state().session.active().unwrap().history.past_len(), past + 1, "the blur ran once");
        // The values are remembered for the next opening (the app writes `prefs.dialogs`).
        assert_eq!(h.state().session.prefs().dialogs["filter.blur.gaussianBlur"]["radius"], 7.5);
    }

    #[test]
    fn cancel_by_escape_and_by_automation_runs_no_command() {
        let mut h = harness();
        let id = open(&mut h, "filter.blur.gaussianBlur");
        let past = h.state().session.active().unwrap().history.past_len();
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert!(h.state().ui.hosted(id).is_none() && !h.state().ui.any_dialog_open());

        let id = open(&mut h, "filter.blur.gaussianBlur");
        let ctx = h.ctx.clone();
        crate::dialogs::cancel_in(h.state_mut(), &ctx, id).unwrap();
        assert!(h.state().ui.hosted(id).is_none() && !h.state().ui.host.is_open::<ParamDialog>());
        assert_eq!(h.state().session.active().unwrap().history.past_len(), past);
    }

    #[test]
    fn automation_confirm_runs_the_command_and_a_second_filter_replaces_the_first() {
        let mut h = harness();
        let first = open(&mut h, "filter.blur.gaussianBlur");
        let second = open(&mut h, "filter.stylize.emboss");
        assert!(h.state().ui.hosted(first).is_none(), "the replaced dialog's record is gone");
        let ctx = h.ctx.clone();
        let past = h.state().session.active().unwrap().history.past_len();
        crate::dialogs::confirm_in(h.state_mut(), &ctx, second).unwrap();
        assert!(h.state().ui.hosted(second).is_none());
        assert_eq!(h.state().session.active().unwrap().history.past_len(), past + 1);
    }

    #[test]
    fn ok_follows_the_apps_enablement() {
        let mut h = harness();
        let id = open(&mut h, "filter.blur.gaussianBlur");
        h.state_mut().ui.host.get_mut::<ParamDialog>().unwrap().set_ok_enabled(false);
        let ctx = h.ctx.clone();
        let error = crate::dialogs::confirm_in(h.state_mut(), &ctx, id).unwrap_err();
        assert!(error.contains("OK is disabled"), "{error}");
        assert!(h.state().ui.hosted(id).is_some());
    }

    #[test]
    fn replace_colors_range_waits_for_localized_color_clusters() {
        let mut h = harness();
        let id = open(&mut h, "image.adjustments.replaceColor");
        let range_disabled = |h: &Harness<'static, PhotocraftApp>| h.get_all_by_label_contains("Range").any(|n| n.accesskit_node().is_disabled());
        assert!(range_disabled(&h), "Range is off while Localized Color Clusters is off");
        crate::dialogs::hosted::set_field(h.state_mut(), id, "localized", json!(true)).unwrap();
        h.run_steps(3);
        assert!(!range_disabled(&h));
        // The Result row is the host's own and the From colour is a colour field.
        let fields = h.state().ui.dialog_fields(id).unwrap();
        assert!(fields["color"].as_str().is_some_and(|c| c.starts_with('#')), "{fields:?}");
    }

    #[test]
    fn lens_blur_keeps_a_channel_source_as_two_keys() {
        let mut h = harness();
        let id = open(&mut h, "filter.blur.lensBlur");
        crate::dialogs::hosted::set_field(h.state_mut(), id, "depthMap", json!("none")).unwrap();
        let fields = h.state().ui.dialog_fields(id).unwrap();
        assert_eq!(fields["depthMap"], "none");
        assert!(!fields.contains_key("depthChannel"));
        // No channel exists in this document, so a channel source is refused.
        assert!(crate::dialogs::hosted::set_field(h.state_mut(), id, "depthMap", json!("channel")).is_err());
    }

    #[test]
    fn the_color_table_dialog_has_load_and_save_in_its_footer() {
        let mut h = harness();
        let _ = open(&mut h, "image.mode.colorTable");
        assert!(h.query_by_label("Load…").is_some() && h.query_by_label("Save…").is_some());
    }

    #[test]
    fn alt_reset_restores_the_values_the_dialog_opened_with() {
        let mut h = harness();
        h.state_mut().session.prefs.edit(|p| p.dialogs.insert("filter.blur.gaussianBlur".into(), json!({"radius": 9.0})));
        let id = open(&mut h, "filter.blur.gaussianBlur");
        assert_eq!(h.state().ui.dialog_fields(id).unwrap()["radius"], 9.0, "the dialog opens on the remembered value");
        crate::dialogs::hosted::set_field(h.state_mut(), id, "radius", json!(20.0)).unwrap();
        h.run_steps(2);
        h.get_by_label("Cancel").click_modifiers(egui::Modifiers::ALT);
        h.run_steps(3);
        assert!(h.state().ui.hosted(id).is_some(), "Reset keeps the dialog open");
        assert_eq!(h.state().ui.dialog_fields(id).unwrap()["radius"], 9.0);
    }
}
