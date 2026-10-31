//! Photoshop-style guide layout dialog. Its preview is a pure guide overlay, never an edit.

use craft::ui::theme::ToEgui;
use egui::Color32;
use photocraft_doc::{Document, Guides};
use serde_json::{Map, Value, json};

use craft::ui::composites::{FormGrid, GroupBox, LabelAlign};
use craft::ui::controls::Select;

use crate::{PhotocraftApp, widgets};

const COMMAND: &str = "view.newGuideLayout";
const MARK: &str = "__guideLayout";

/// A `"#rrggbb"` colour as drawn.
fn rgb(hex: &str) -> Option<Color32> {
    photocraft_engine::prefs::parse_hex(hex).map(|[r, g, b]| Color32::from_rgb(r, g, b))
}

/// The Color list: New Guide's colours (`GUIDE_COLORS`) by their `"#rrggbb"`, then `current` as
/// Custom when it's none of them.
fn color_options(current: &str) -> Vec<(String, &'static str)> {
    let mut colors: Vec<(String, &'static str)> = photocraft_engine::edit_cmds::GUIDE_COLORS
        .iter()
        .map(|(name, [r, g, b])| (format!("#{r:02x}{g:02x}{b:02x}"), crate::new_guide_ui::color_label(name)))
        .collect();
    if !colors.iter().any(|(v, _)| v == current) {
        colors.push((current.to_string(), "Custom"));
    }
    colors
}

pub fn owns(f: &Map<String, Value>) -> bool {
    f.contains_key(MARK)
}

/// Alt+Cancel (Reset): factory New Guide Layout values, not the last-used remembered layout.
pub fn reset(app: &PhotocraftApp, f: &mut Map<String, Value>) {
    if !owns(f) {
        return;
    }
    let Value::Object(defaults) = crate::view_cmds::default_guide_layout() else { return };
    // Drop layout params and interactive toggles; keep dialog chrome (`__command`, …).
    f.retain(|k, _| {
        k.starts_with("__")
            && !matches!(
                k.as_str(),
                "__preset" | "__columnsEnabled" | "__rowsEnabled" | "__marginEnabled" | "__previewGuides" | "__previewParams" | "__previewSource"
            )
    });
    f.extend(defaults);
    f.insert("color".into(), json!(app.session.prefs().guides_grid_and_slices.guide_color));
    f.insert("target".into(), json!("document"));
    f.insert("__preset".into(), json!("custom"));
    f.insert("width".into(), Value::Null);
    f.insert("height".into(), Value::Null);
    let margin = f.get("margin").cloned().unwrap_or(json!(0));
    if !margin.is_array() {
        f.insert("margin".into(), json!([margin.clone(), margin.clone(), margin.clone(), margin]));
    }
    f.insert("__preview".into(), json!(true));
}

fn on(f: &Map<String, Value>, key: &str, fallback: bool) -> bool {
    f.get(key).and_then(Value::as_bool).unwrap_or(fallback)
}
fn positive(f: &Map<String, Value>, key: &str) -> bool {
    f.get(key).and_then(Value::as_f64).is_some_and(|n| n > 0.0)
}

pub fn open(app: &mut PhotocraftApp) -> u64 {
    let mut f = app.ui.view.guide_layout.as_object().cloned().unwrap_or_default();
    f.insert(MARK.into(), json!(true));
    f.insert("__command".into(), json!(COMMAND));
    f.insert("__label".into(), json!("New Guide Layout…"));
    f.insert("__document".into(), app.session.active().map(|s| json!(s.doc.id)).unwrap_or(Value::Null));
    f.insert("__preview".into(), json!(true));
    f.insert("color".into(), json!(app.session.prefs().guides_grid_and_slices.guide_color));
    f.entry("target").or_insert_with(|| json!("document"));
    if f.get("target").and_then(Value::as_str) == Some("selectedArtboards") && photocraft_engine::guide_layout::selected_artboards(&app.session).is_empty() {
        f.insert("target".into(), json!("document"));
    }
    f.entry("__preset").or_insert_with(|| json!("custom"));
    f.entry("width").or_insert(Value::Null);
    f.entry("height").or_insert(Value::Null);
    let margin = f.get("margin").cloned().unwrap_or(json!(0));
    if !margin.is_array() {
        f.insert("margin".into(), json!([margin.clone(), margin.clone(), margin.clone(), margin]));
    }
    for key in ["__previewGuides", "__previewParams", "__previewSource"] {
        f.remove(key);
    }
    app.ui.open_dialog(crate::state::DialogKind::Command, f)
}

/// Preserve legacy command parameter names for automation and remembered defaults.
pub fn params(f: &Map<String, Value>) -> Value {
    let mut p: Map<String, Value> = ["columns", "rows", "width", "height", "gutter", "rowGutter", "margin", "centerColumns", "clearExisting", "target"]
        .into_iter()
        .filter_map(|k| f.get(k).cloned().map(|v| (k.to_string(), v)))
        .collect();
    for (toggle, key) in [("__columnsEnabled", "columns"), ("__rowsEnabled", "rows")] {
        if !on(f, toggle, positive(f, key)) {
            p.insert(key.into(), json!(0));
        }
    }
    if !on(f, "__marginEnabled", true) {
        p.insert("margin".into(), json!(0));
    }
    for key in ["width", "height"] {
        if let Some(Value::String(s)) = p.get(key) {
            let value = if s.trim().is_empty() {
                Value::Null
            } else {
                widgets::parse_num(s).and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or_else(|| json!(s))
            };
            p.insert(key.into(), value);
        }
    }
    // A disabled axis must not be rejected for a stale width or gap typed before disabling it.
    for (key, size, gap) in [("columns", "width", "gutter"), ("rows", "height", "rowGutter")] {
        if p.get(key).and_then(Value::as_u64) == Some(0) {
            p.insert(size.into(), Value::Null);
            p.insert(gap.into(), json!(0));
        }
    }
    Value::Object(p)
}

fn source(app: &PhotocraftApp) -> Value {
    app.session.active().map(|st| json!([st.doc.id, st.revision, st.active_layer, st.selected_layers])).unwrap_or(Value::Null)
}

fn calculate(app: &PhotocraftApp, f: &Map<String, Value>) -> Result<Guides, String> {
    let st = app.session.active().ok_or("no document")?;
    if f.get("__document") != Some(&json!(st.doc.id)) {
        return Err("the guide layout belongs to another document".into());
    }
    let params = params(f);
    // Painting an unchanged dialog must not repeat the duplicate scans every frame.
    if f.get("__previewParams") == Some(&params)
        && f.get("__previewSource") == Some(&source(app))
        && let Some(guides) = f.get("__previewGuides").and_then(|v| serde_json::from_value(v.clone()).ok())
    {
        return Ok(guides);
    }
    photocraft_engine::guide_layout::calculate(&app.session, &params).map(|l| l.guides).map_err(|e| e.to_string())
}

pub fn validate(app: &PhotocraftApp, f: &Map<String, Value>) -> Result<(), String> {
    let color = f.get("color").and_then(Value::as_str).unwrap_or("#4affff");
    rgb(color).ok_or("invalid guide color")?;
    calculate(app, f).map(|_| ())
}

pub fn confirm(app: &mut PhotocraftApp, f: &Map<String, Value>) -> Result<Value, String> {
    validate(app, f)?;
    let color = f.get("color").and_then(Value::as_str).unwrap_or("#4affff");
    // Colour is the existing global guide display preference, not document data.
    let color = rgb(color).ok_or("invalid guide color")?;
    let hex = format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b());
    let result = app.run(COMMAND, params(f))?;
    // The preview is visible even when guides were hidden. Keep a committed layout visible too.
    app.ui.view.extras = true;
    app.ui.view.show.canvas_guides = true;
    app.ui.extras.guides = true;
    app.session.prefs.edit(|prefs| prefs.guides_grid_and_slices.guide_color = hex);
    let mut remembered = f.clone();
    for key in [MARK, "__command", "__label", "__document", "__previewGuides", "__previewParams", "__previewSource"] {
        remembered.remove(key);
    }
    app.ui.view.guide_layout = Value::Object(remembered);
    Ok(result)
}

/// Full replacement guide set for the dialog's own document. Cancel/removal drops it immediately.
pub fn preview(app: &PhotocraftApp, doc: &Document) -> Option<(Guides, Color32)> {
    let dialog = app.ui.dialogs.iter().rev().find(|d| owns(&d.fields))?;
    let f = &dialog.fields;
    if !on(f, "__preview", true) || f.get("__document") != Some(&json!(doc.id)) {
        return None;
    }
    let guides = calculate(app, f).ok().or_else(|| f.get("__previewGuides").and_then(|v| serde_json::from_value(v.clone()).ok()))?;
    let color = f.get("color").and_then(Value::as_str).and_then(rgb).unwrap_or(Color32::from_rgb(74, 255, 255));
    Some((guides, color))
}

/// Whether the Preview check is on.
pub fn preview_on(f: &Map<String, Value>) -> bool {
    on(f, "__preview", true)
}

/// The Preview check under the footer's Cancel button; `Some(on)` when the user toggled it.
pub fn preview_check(ui: &mut egui::Ui, on: bool) -> Option<bool> {
    let mut value = on;
    craft::ui::controls::Checkbox::bool(tl!("Preview"), &mut value).show(ui).response.changed().then_some(value)
}

fn check(ui: &mut egui::Ui, f: &mut Map<String, Value>, key: &str, label: &str, fallback: bool) -> bool {
    let mut value = on(f, key, fallback);
    if craft::ui::controls::Checkbox::bool(tl!(label), &mut value).show(ui).response.changed() {
        f.insert(key.into(), json!(value));
    }
    value
}

fn number(ui: &mut egui::Ui, f: &mut Map<String, Value>, key: &str, name: &str, count: bool) -> egui::Response {
    let mut value = f.get(key).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let (range, unit, decimals) = if count { (1.0..=1000.0, "{n}", 0) } else { (0.0..=300_000.0, "{n} px", 2) };
    let edit = widgets::dialog_number_field(ui, ("guide-layout", key), tl!(name), &mut value, range, unit, decimals);
    // A disabled field still clamps its value into range (a zero count to 1); writing that back
    // would switch the axis on through its `positive` fallback (#5625).
    if edit.changed && ui.is_enabled() {
        f.insert(key.into(), if count { json!(value.round() as u64) } else { json!(value) });
    }
    edit.response
}

fn optional(ui: &mut egui::Ui, f: &mut Map<String, Value>, key: &str, name: &str) -> egui::Response {
    let mut value = match f.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(v) if v.is_number() => v.to_string(),
        _ => String::new(),
    };
    let edit = widgets::dialog_text(ui, ("guide-layout", key), name, &mut value);
    if edit.changed {
        f.insert(key.into(), json!(value));
    }
    edit.response.on_hover_text(tl!("Leave empty for automatic size"))
}

fn axis(ui: &mut egui::Ui, f: &mut Map<String, Value>, rows: bool) {
    let (toggle, count, size, gap, title, size_label) = if rows {
        ("__rowsEnabled", "rows", "height", "rowGutter", tl!("Rows"), tl!("Height"))
    } else {
        ("__columnsEnabled", "columns", "width", "gutter", tl!("Columns"), tl!("Width"))
    };
    let mut enabled = on(f, toggle, positive(f, count));
    GroupBox::new(title).checkbox(&mut enabled).fill(false).id_salt(count).show(ui, |ui| {
        FormGrid::new(count).label_align(LabelAlign::Start).show(ui, |form| {
            form.row(tl!("Number"), |ui, _| number(ui, f, count, "Number", true));
            form.row(size_label, |ui, _| optional(ui, f, size, size_label));
            form.row(tl!("Spacing"), |ui, _| number(ui, f, gap, "Spacing", false));
        });
    });
    if enabled != on(f, toggle, positive(f, count)) {
        f.insert(toggle.into(), json!(enabled));
    }
    if enabled && f.get(count).and_then(Value::as_f64) == Some(0.0) {
        f.insert(count.into(), json!(1));
    }
}

fn main_fields(app: &PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let before: Map<String, Value> = f.iter().filter(|(key, _)| !key.starts_with("__preview")).map(|(k, v)| (k.clone(), v.clone())).collect();
    FormGrid::new("guide-header").label_align(LabelAlign::Start).show(ui, |form| {
        let mut preset = f.get("__preset").and_then(Value::as_str).unwrap_or("custom").to_string();
        let edit = form.row(tl!("Preset:"), |ui, _| {
            Select::new("guide-preset", tl!("Preset:"), &mut preset)
                .fit(true)
                .option("custom".into(), tl!("Custom"))
                .option("five".into(), tl!("5 Columns, 3 Rows"))
                .option("eight".into(), tl!("8 Columns"))
                .option("twelve".into(), tl!("12 Columns"))
                .show(ui)
        });
        if edit.inner.response.changed() {
            f.insert("__preset".into(), json!(preset));
            match preset.as_str() {
                "five" | "eight" | "twelve" => {
                    let (cols, rows, gap) = match preset.as_str() {
                        "five" => (5, 3, 0),
                        "eight" => (8, 0, 20),
                        _ => (12, 0, 24),
                    };
                    for (k, v) in [
                        ("columns", json!(cols)),
                        ("rows", json!(rows)),
                        ("gutter", json!(gap)),
                        ("rowGutter", json!(0)),
                        ("width", Value::Null),
                        ("height", Value::Null),
                        ("margin", json!([0, 0, 0, 0])),
                        ("__columnsEnabled", json!(true)),
                        ("__rowsEnabled", json!(rows > 0)),
                        ("__marginEnabled", json!(true)),
                        ("centerColumns", json!(false)),
                    ] {
                        f.insert(k.into(), v);
                    }
                }
                _ => {}
            }
        }
        let mut target = f.get("target").and_then(Value::as_str).unwrap_or("document").to_string();
        let mut options = vec![("document".to_string(), "Document")];
        if !photocraft_engine::guide_layout::selected_artboards(&app.session).is_empty() {
            options.push(("selectedArtboards".into(), "Selected Artboards"));
        }
        let edit = form.row(tl!("Target:"), |ui, _| {
            Select::new("guide-target", tl!("Target"), &mut target).rows(options.len(), |i| options[i].0.clone(), |i| tl!(options[i].1)).show(ui)
        });
        if edit.inner.response.changed() {
            f.insert("target".into(), json!(target));
        }
    });
    ui.add_space(craft::ui::theme::scale::Space::S12.px());
    GroupBox::new(tl!("Color")).show(ui, |ui| {
        FormGrid::new("guide-color-row").label_align(LabelAlign::Start).show(ui, |form| {
            form.row(tl!("Color:"), |ui, _| {
                let mut color = f.get("color").and_then(Value::as_str).unwrap_or("#4affff").to_string();
                let colors = color_options(&color);
                let edit = Select::new("guide-color", tl!("Color"), &mut color).rows(colors.len(), |i| colors[i].0.clone(), |i| tl!(colors[i].1)).show(ui);
                if edit.response.changed() {
                    f.insert("color".into(), json!(color));
                }
                let shown = rgb(&color).unwrap_or(craft::ui::theme::current(ui.ctx()).color.accent.solid.to_egui());
                let swatch = widgets::color_swatch_button(ui, shown, tl!("Guide color"));
                if swatch.clicked() {
                    crate::color_picker_ui::request(f, tl!("Guide color"));
                }
                (edit, swatch)
            });
        });
    });
    ui.add_space(craft::ui::theme::scale::Space::S12.px());
    ui.horizontal_top(|ui| {
        axis(ui, f, false);
        ui.add_space(craft::ui::theme::scale::Space::S4.px());
        axis(ui, f, true);
    });
    ui.add_space(craft::ui::theme::scale::Space::S12.px());
    let mut enabled = on(f, "__marginEnabled", true);
    let mut margins = f.get("margin").and_then(Value::as_array).cloned().unwrap_or_else(|| vec![json!(0); 4]);
    GroupBox::new(tl!("Margin")).checkbox(&mut enabled).show(ui, |ui| {
        ui.horizontal(|ui| {
            for (value, label) in margins.iter_mut().zip([tl!("Top:"), tl!("Left:"), tl!("Bottom:"), tl!("Right:")]) {
                ui.vertical(|ui| {
                    ui.label(label);
                    let mut n = value.as_f64().unwrap_or(0.0) as f32;
                    if widgets::dialog_number(ui, ("guide-layout-margin", label), label, &mut n, 0.0..=300_000.0, "{n} px", 2) {
                        *value = json!(n);
                    }
                });
            }
        });
    });
    if enabled != on(f, "__marginEnabled", true) {
        f.insert("__marginEnabled".into(), json!(enabled));
    }
    f.insert("margin".into(), Value::Array(margins));
    ui.add_space(craft::ui::theme::scale::Space::S8.px());
    check(ui, f, "centerColumns", tl!("Center Columns"), false);
    check(ui, f, "clearExisting", tl!("Clear Existing Guides"), false);
    // A typed edit makes a named preset custom; changing target/colour/preview does not.
    if ["columns", "rows", "width", "height", "gutter", "rowGutter", "margin", "centerColumns", "__columnsEnabled", "__rowsEnabled", "__marginEnabled"]
        .iter()
        .any(|k| before.get(*k) != f.get(*k))
        && before.get("__preset") == f.get("__preset")
    {
        f.insert("__preset".into(), json!("custom"));
    }
}

/// The dialog's fields and the live preview; OK and Cancel are the host's side column.
pub fn body(app: &PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let before = (params(f), on(f, "__preview", true), f.get("color").cloned());
    main_fields(app, ui, f);
    match calculate(app, f) {
        Ok(guides) => {
            f.insert("__previewGuides".into(), json!(guides));
            f.insert("__previewParams".into(), params(f));
            f.insert("__previewSource".into(), source(app));
        }
        Err(error) => {
            ui.colored_label(craft::ui::theme::current(ui.ctx()).color.text.muted.to_egui(), tl!("Invalid guide layout")).on_hover_text(error);
        }
    }
    // The canvas paints before the dialog. Schedule the frame that displays this draft edit.
    if before != (params(f), on(f, "__preview", true), f.get("color").cloned()) {
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
mod tests;
