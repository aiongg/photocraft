//! File › New: Photoshop's New Document dialog. Category tabs with blank-document presets on the
//! left, Preset Details on the right. Values live in the dialog fields (`width`/`height` in pixels,
//! `resolution` in ppi, `mode`, `depth`, `background`, `name`), so `ui.dialog.set` drives it and
//! Create runs `file.new` with them.

use craft::theme::TypeRole;
use craft::ui::composites::{Field, SectionHeader};
use craft::ui::controls::{Button, ButtonVariant, Checkbox, ControlSize, IconButton, Select, Swatch};
use craft::ui::icons::Icon;
use craft::ui::list::ListRow;
use craft::ui::primitives::{Content, Interactive, SurfaceKind, Text};
use craft::ui::tab_strip::{Tab, TabEvent, TabKind, TabStrip};
use craft::ui::theme::{StrokeExt, ToEgui, Tokens as LibTokens, TokensOf as _};
use egui::accesskit::Role;
use egui::{Align2, Rect, Stroke, StrokeKind, pos2, vec2};
use serde_json::{Map, Value, json};

use photocraft_engine::prefs::UnitsAndRulers;

use crate::widgets;
use photocraft_engine::document_preset_cmds::DocumentSettings;

/// A blank-document preset: (name, width px, height px, ppi).
pub type Preset = (&'static str, u32, u32, f32);

/// Photoshop's New Document categories and their blank-document presets.
pub const CATEGORIES: &[(&str, &[Preset])] = &[
    ("Recent", &[("Default PhotoCraft Size", 2100, 1500, 300.0), ("HDTV 1080p", 1920, 1080, 72.0)]),
    (
        "Photo",
        &[
            ("Landscape, 6 x 4", 1800, 1200, 300.0),
            ("Landscape, 7 x 5", 2100, 1500, 300.0),
            ("Landscape, 10 x 8", 3000, 2400, 300.0),
            ("Portrait, 4 x 6", 1200, 1800, 300.0),
            ("Portrait, 5 x 7", 1500, 2100, 300.0),
            ("Square, 5 x 5", 1500, 1500, 300.0),
        ],
    ),
    (
        "Print",
        &[
            ("Letter", 2550, 3300, 300.0),
            ("Legal", 2550, 4200, 300.0),
            ("Tabloid", 3300, 5100, 300.0),
            ("A4", 2480, 3508, 300.0),
            ("A3", 3508, 4961, 300.0),
            ("A5", 1748, 2480, 300.0),
            ("B3", 4169, 5906, 300.0),
            ("B4", 2953, 4169, 300.0),
        ],
    ),
    (
        "Art & Illustration",
        &[("Poster", 5400, 7200, 300.0), ("Postcard", 1800, 1200, 300.0), ("Comic Book", 1988, 3075, 300.0), ("Square, 12 x 12", 3600, 3600, 300.0)],
    ),
    (
        "Web",
        &[
            ("Web Most Common", 1366, 768, 72.0),
            ("Web Minimum", 1024, 768, 72.0),
            ("Web Large", 1920, 1080, 72.0),
            ("MacBook Pro 16\"", 3456, 2234, 72.0),
            ("iMac 24\"", 4480, 2520, 72.0),
        ],
    ),
    (
        "Mobile",
        &[
            ("iPhone 16", 1179, 2556, 72.0),
            ("iPhone 16 Pro Max", 1320, 2868, 72.0),
            ("iPad Pro 13\"", 2064, 2752, 72.0),
            ("Android 1080p", 1080, 1920, 72.0),
            ("Apple Watch 45mm", 396, 484, 72.0),
        ],
    ),
    (
        "Film & Video",
        &[
            ("HDTV 1080p", 1920, 1080, 72.0),
            ("HDTV 720p", 1280, 720, 72.0),
            ("UHD 4K", 3840, 2160, 72.0),
            ("DCI 4K", 4096, 2160, 72.0),
            ("UHD 8K", 7680, 4320, 72.0),
        ],
    ),
];

const DEPTH_OPTIONS: &[(u64, &str, &str)] = &[(8, "8 bit", "Integer"), (16, "16 bit", "Integer"), (32, "32 bit (float)", "Floating point")];

/// Width/Height units: (key, label, units per inch; 0 = pixels).
pub const UNITS: &[(&str, &str, f32)] =
    &[("px", "Pixels", 0.0), ("in", "Inches", 1.0), ("cm", "Centimeters", 2.54), ("mm", "Millimeters", 25.4), ("pt", "Points", 72.0), ("pica", "Picas", 6.0)];

/// Pixels to the display unit at `ppi`.
pub fn to_unit(px: f32, unit: &str, ppi: f32) -> f32 {
    match UNITS.iter().find(|u| u.0 == unit) {
        Some((_, _, per_in)) if *per_in > 0.0 => px / ppi.max(1.0) * per_in,
        _ => px,
    }
}

/// Display unit back to pixels at `ppi`.
pub fn from_unit(v: f32, unit: &str, ppi: f32) -> f32 {
    match UNITS.iter().find(|u| u.0 == unit) {
        Some((_, _, per_in)) if *per_in > 0.0 => (v / per_in * ppi.max(1.0)).round(),
        _ => v.round(),
    }
}

/// A typed size as the whole pixel count `file.new` takes (#254: a float like `512.0` isn't one, so
/// the command fell back to its 1920 x 1080 default). Clamped to the command's 1–300000 range.
pub fn px_value(px: f32) -> Value {
    json!(if px.is_finite() { px.round().clamp(1.0, 300_000.0) as u32 } else { 1 })
}

/// Name of the preset that takes the clipboard image's size.
pub const CLIPBOARD: &str = "Clipboard";

/// Offer the Clipboard preset (`w` × `h` px, at 72 ppi) first under the Recent presets, and
/// select it.
pub fn set_clipboard(f: &mut Map<String, Value>, w: u32, h: u32) {
    if w == 0 || h == 0 {
        return;
    }
    f.insert("__clipboard".into(), json!([w, h]));
    apply_preset(f, &(CLIPBOARD, w, h, 72.0), &UnitsAndRulers::default());
}

/// The clipboard image's size, when the dialog offers the Clipboard preset.
fn clipboard_preset(f: &Map<String, Value>) -> Option<Preset> {
    let size = f.get("__clipboard")?.as_array()?;
    let dim = |i: usize| size.get(i)?.as_u64().and_then(|v| u32::try_from(v).ok()).filter(|v| *v > 0);
    Some((CLIPBOARD, dim(0)?, dim(1)?, 72.0))
}

/// Set Width or Height (`key`) from a value typed in `unit`: whole pixels for `file.new`, plus the
/// typed value so the field keeps showing it (see [`shown_size`]).
pub fn set_size(f: &mut Map<String, Value>, key: &str, v: f32, unit: &str, ppi: f32) {
    f.insert(key.into(), px_value(from_unit(v, unit, ppi)));
    f.insert(typed_key(key), json!({"value": v, "unit": unit}));
    f.remove("__preset");
    f.remove("__savedPreset");
}

/// What Width or Height (`key`, `d` pixels when unset) shows in `unit`: the value the user typed
/// while it still gives the field's pixel count, else the pixels converted. Converting the rounded
/// pixels back on every keystroke turned a typed `5` mm into 14 px and the text into `4.9` (#1147).
/// A size over the 300000 px limit shows the limit, not the typed value the document won't have.
pub fn shown_size(f: &Map<String, Value>, key: &str, d: f32, unit: &str, ppi: f32) -> f32 {
    let px = get_f(f, key, d);
    let typed = f.get(&typed_key(key)).and_then(Value::as_object).filter(|t| unit != "px" && t.get("unit").and_then(Value::as_str) == Some(unit));
    match typed.and_then(|t| t.get("value")).and_then(Value::as_f64) {
        Some(v) if from_unit(v as f32, unit, ppi).max(1.0) == px => v as f32,
        _ => to_unit(px, unit, ppi),
    }
}

fn typed_key(key: &str) -> String {
    format!("__{key}Typed")
}

/// Swap Width and Height (the Orientation buttons), with the values typed for them, so a typed
/// 841 x 1189 mm turns into 1189 x 841 mm, not 1188.9 x 841.
pub fn swap_size(f: &mut Map<String, Value>) {
    f.remove("__savedPreset");
    let (w, h) = (get_f(f, "width", 1920.0), get_f(f, "height", 1080.0));
    f.insert("width".into(), px_value(h));
    f.insert("height".into(), px_value(w));
    let (typed_w, typed_h) = (f.remove(&typed_key("width")), f.remove(&typed_key("height")));
    if let Some(t) = typed_h {
        f.insert(typed_key("width"), t);
    }
    if let Some(t) = typed_w {
        f.insert(typed_key("height"), t);
    }
}

/// Print and photo presets (listed at 300 ppi) are specified in inches, screen presets in pixels.
fn is_print(p: &Preset) -> bool {
    p.3 >= 300.0
}

/// A built-in preset turned to the orientation picked with the Orientation buttons (#4668), so the
/// cards and what they apply follow it. Square presets and the Clipboard preset stay as they are.
fn oriented(p: &Preset, f: &Map<String, Value>) -> Preset {
    let portrait = match f.get("__orientation").and_then(Value::as_str) {
        Some("portrait") => true,
        Some("landscape") => false,
        _ => return *p,
    };
    if p.0 == CLIPBOARD || p.1 == p.2 || (p.2 > p.1) == portrait {
        return *p;
    }
    (p.0, p.2, p.1, p.3)
}

/// The unit a built-in preset shows and applies: a print preset in the physical unit picked for
/// Width/Height (#4668), else in inches; a screen preset in pixels.
fn preset_unit(p: &Preset, f: &Map<String, Value>) -> String {
    let unit = get_s(f, "__unit", "px");
    if !is_print(p) {
        "px".into()
    } else if unit != "px" && UNITS.iter().any(|u| u.0 == unit) {
        unit
    } else {
        "in".into()
    }
}

/// A built-in preset at Preferences › Units & Rulers › New Document Preset Resolutions, as in
/// Photoshop: a print preset keeps its size in inches at the Print Resolution (Letter at 200 ppi is
/// 1700 x 2200 px), a screen preset keeps its pixels at the Screen Resolution. The Clipboard preset
/// is the copied pixels and keeps its own resolution. A resolution that isn't a positive finite
/// number leaves the preset as listed.
pub fn at_pref_resolutions(p: &Preset, units: &UnitsAndRulers) -> Preset {
    let ppi = (if is_print(p) { units.print_resolution } else { units.screen_resolution }) as f32;
    if p.0 == CLIPBOARD || !(ppi.is_finite() && ppi > 0.0) {
        return *p;
    }
    if !is_print(p) {
        return (p.0, p.1, p.2, ppi);
    }
    let scale = ppi / p.3;
    let px = |v: u32| (v as f32 * scale).round().clamp(1.0, 300_000.0) as u32;
    (p.0, px(p.1), px(p.2), ppi)
}

/// Apply a built-in preset (at the preference resolutions, see [`at_pref_resolutions`]) to the
/// dialog fields.
pub fn apply_preset(f: &mut Map<String, Value>, p: &Preset, units: &UnitsAndRulers) {
    let (name, w, h, ppi) = at_pref_resolutions(p, units);
    let unit = preset_unit(p, f);
    f.remove("__savedPreset");
    f.insert("width".into(), json!(w));
    f.insert("height".into(), json!(h));
    f.insert("resolution".into(), json!(ppi));
    f.insert("__preset".into(), json!(name));
    f.insert("__unit".into(), json!(unit));
}

/// The colour mode of the dialog's `mode` value.
fn color_mode_of(mode: &str) -> Option<photocraft_color::ColorMode> {
    use photocraft_color::ColorMode;
    match mode {
        "rgb" => Some(ColorMode::Rgb),
        "gray" | "grayscale" => Some(ColorMode::Grayscale),
        "cmyk" => Some(ColorMode::Cmyk),
        "lab" => Some(ColorMode::Lab),
        _ => None,
    }
}

/// The Color Profile menu for a document of colour `space` (#321): the working space (shown with
/// its profile's description), then the built-in and installed profiles of that space, as
/// `file.new`'s `profile` values (an id or a path) with their labels.
fn profile_options(
    space: Option<photocraft_cms::ColorSpace>,
    working: &str,
    installed: &[photocraft_engine::installed_profiles::InstalledProfile],
) -> Vec<(String, String)> {
    let mut v = vec![("working".to_string(), crate::i18n::fmt(tl!("Working: {working}"), &[("working", working)]))];
    let Some(space) = space else { return v };
    v.extend(photocraft_cms::Builtin::ALL.iter().filter(|b| b.profile().color_space == space).map(|b| (b.id().to_string(), b.description().to_string())));
    v.extend(installed.iter().filter(|i| i.is_destination() && i.color_space == space).map(|i| (i.path.clone(), i.description.clone())));
    v
}

/// Fields `file.new` takes (drops the dialog's `__` UI keys).
pub fn command_params(f: &Map<String, Value>) -> Value {
    Value::Object(f.iter().filter(|(k, _)| !k.starts_with("__")).map(|(k, v)| (k.clone(), v.clone())).collect())
}

/// Set the resolution (pixels/inch) the way Photoshop's New Document does (#758): with Width/Height
/// in a physical unit the physical size is kept and the pixel count changes; in pixels the pixels
/// are kept.
pub fn set_resolution(f: &mut Map<String, Value>, new_ppi: f32) {
    let old_ppi = get_f(f, "resolution", 72.0);
    f.insert("resolution".into(), json!(new_ppi));
    if old_ppi != new_ppi {
        f.remove("__savedPreset");
    }
    if get_s(f, "__unit", "px") == "px" || !(old_ppi > 0.0 && new_ppi > 0.0) || old_ppi == new_ppi {
        return;
    }
    let scale = new_ppi / old_ppi;
    let (w, h) = (get_f(f, "width", 1920.0), get_f(f, "height", 1080.0));
    f.insert("width".into(), px_value(w * scale));
    f.insert("height".into(), px_value(h * scale));
    f.remove("__preset");
    f.remove("__savedPreset");
}

fn get_f(f: &Map<String, Value>, k: &str, d: f32) -> f32 {
    f.get(k).and_then(Value::as_f64).map_or(d, |v| v as f32)
}

fn get_s(f: &Map<String, Value>, k: &str, d: &str) -> String {
    f.get(k).and_then(Value::as_str).unwrap_or(d).to_string()
}

/// The card's title laid out centred in `width`: wrapped onto at most two lines, the rest elided,
/// so long translations stay inside the card.
fn card_title(painter: &egui::Painter, title: &str, width: f32, format: &egui::TextFormat) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(title.to_owned(), format.font_id.clone(), format.color, width);
    job.wrap.max_rows = 2;
    job.halign = egui::Align::Center;
    painter.layout_job(job)
}

/// Paint a page thumbnail with the preset's aspect ratio.
fn page_icon(ui: &egui::Ui, r: Rect, w: u32, h: u32, color: egui::Color32) {
    let s = 30.0 / (w.max(h) as f32);
    let page = Rect::from_center_size(r.center(), vec2(w as f32 * s, h as f32 * s));
    ui.painter().rect_stroke(page, craft::ui::theme::scale::Radius::R2.px(), Stroke::new(1.2, color), StrokeKind::Inside);
}

/// The Background Contents menu's entry for a colour of the user's own ([`body`]).
const CUSTOM: &str = "custom";
/// Side of the Background Contents colour swatch.
const SWATCH: f32 = 22.0;

/// The colour the new document's background starts with: white, black, the captured or current
/// toolbox background colour, or a custom `"#rrggbb"`; `None` for Transparent.
fn background_rgb(app: &crate::PhotocraftApp, f: &Map<String, Value>) -> Option<[f32; 3]> {
    let bg = get_s(f, "background", "white");
    match bg.as_str() {
        "transparent" => None,
        "black" => Some([0.0; 3]),
        "backgroundColor" => Some(
            f.get("backgroundColor")
                .and_then(Value::as_array)
                .and_then(|a| Some([a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32]))
                .unwrap_or_else(|| {
                    let b = app.session.tools.background;
                    [b[0], b[1], b[2]]
                }),
        ),
        hex => Some(crate::color_picker_ui::parse_hex(hex).unwrap_or([1.0; 3])),
    }
}

/// Restore a snapshot without replacing the new document's name or its clipboard offer.
pub fn apply_saved_preset(f: &mut Map<String, Value>, preset: &photocraft_engine::document_preset_cmds::DocumentPreset) {
    apply_settings(f, &preset.settings);
    f.insert("__savedPreset".into(), json!(preset.name));
}

/// Restore every setting of a saved or recent document (all but its name).
pub fn apply_settings(f: &mut Map<String, Value>, settings: &DocumentSettings) {
    f.remove("backgroundColor");
    // A preset without a Color Profile is on the working space, whatever was picked before.
    f.remove("profile");
    if let Some(params) = settings.command_params().as_object() {
        f.extend(params.clone());
    }
    f.insert("__unit".into(), json!(settings.unit));
    f.insert("__resUnit".into(), json!(settings.resolution_unit));
    f.remove("__savedPreset");
    f.remove("__preset");
    // Typed Width/Height belong to the previous values (`shown_size`).
    f.remove(&typed_key("width"));
    f.remove(&typed_key("height"));
}

/// The dialog's settings as `document.presets.save` / `document.recent.add` take them: the
/// `file.new` params plus the display units.
pub fn document_settings(f: &Map<String, Value>) -> Value {
    let mut settings = command_params(f);
    let unit = get_s(f, "__unit", "px");
    settings["unit"] = json!(if UNITS.iter().any(|u| u.0 == unit) { unit.as_str() } else { "px" });
    settings["resolutionUnit"] = json!(if get_s(f, "__resUnit", "in") == "cm" { "cm" } else { "in" });
    settings
}

/// Whether the dialog currently holds `settings` (so its Recent card shows selected).
fn holds(f: &Map<String, Value>, settings: &DocumentSettings) -> bool {
    // Unset fields hold the dialog's defaults, as the details pane shows them.
    let (w, h) = (get_f(f, "width", 1920.0), get_f(f, "height", 1080.0));
    (w, h, get_f(f, "resolution", 72.0)) == (settings.width as f32, settings.height as f32, settings.resolution)
        && f.get("depth").and_then(Value::as_u64).unwrap_or(8) == u64::from(settings.depth)
        && get_s(f, "mode", "rgb") == settings.mode
        && get_s(f, "background", "white") == settings.background
}

/// A card in the preset grid: a built-in preset or a recently created document.
enum Card {
    Builtin(Preset),
    Recent(DocumentSettings),
}

impl Card {
    /// Pixel size and resolution; a built-in preset at the Units & Rulers preset resolutions.
    fn size(&self, units: &UnitsAndRulers) -> (u32, u32, f32) {
        match self {
            Card::Builtin(listed) => {
                let p = at_pref_resolutions(listed, units);
                (p.1, p.2, p.3)
            }
            Card::Recent(s) => (s.width, s.height, s.resolution),
        }
    }

    /// The preset's name; a recent document takes the name of the built-in preset it matches.
    fn title(&self) -> &'static str {
        match self {
            Card::Builtin(p) => p.0,
            Card::Recent(s) => {
                CATEGORIES.iter().flat_map(|c| c.1.iter()).find(|p| (p.1, p.2, p.3) == (s.width, s.height, s.resolution)).map_or("Custom", |p| p.0)
            }
        }
    }

    /// The size line, in the unit the preset applies (a recent one: the unit it was made in).
    fn size_line(&self, units: &UnitsAndRulers, f: &Map<String, Value>) -> String {
        let (w, h, ppi) = self.size(units);
        let unit = match self {
            Card::Builtin(p) => preset_unit(p, f),
            Card::Recent(s) => s.unit.clone(),
        };
        if unit == "px" {
            format!("{w} x {h} px @ {} ppi", widgets::fmt_num(f64::from(ppi)))
        } else {
            let v = |n: u32| widgets::fmt_num(f64::from(to_unit(n as f32, &unit, ppi)));
            format!("{} x {} {unit} @ {} ppi", v(w), v(h), widgets::fmt_num(f64::from(ppi)))
        }
    }
}

/// The Recent category: the Clipboard preset, the documents recently created here (newest
/// first), then the built-in defaults they don't already cover.
fn recent_cards(f: &Map<String, Value>, recent: &[DocumentSettings], units: &UnitsAndRulers) -> Vec<Card> {
    let mut cards: Vec<Card> = clipboard_preset(f).map(Card::Builtin).into_iter().collect();
    cards.extend(recent.iter().cloned().map(Card::Recent));
    let defaults = CATEGORIES[0].1.iter().filter(|p| {
        let (_, w, h, ppi) = at_pref_resolutions(p, units);
        !recent.iter().any(|s| (s.width, s.height, s.resolution) == (w, h, ppi))
    });
    cards.extend(defaults.copied().map(Card::Builtin));
    cards
}

fn saved_presets(app: &mut crate::PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) -> bool {
    let presets = app.session.presets.documents.clone();
    if presets.is_empty() {
        ui.label(tl!("No saved presets yet."));
        return false;
    }
    let mut create = false;
    egui::ScrollArea::vertical().id_salt("saved-document-presets").max_height(320.0).show(ui, |ui| {
        for preset in &presets {
            let selected = get_s(f, "__savedPreset", "") == preset.name;
            let s = &preset.settings;
            let detail = format!("{} × {} px @ {} ppi · {}/{}", s.width, s.height, s.resolution, s.mode.to_uppercase(), s.depth);
            let mut delete = false;
            let row = ListRow::new(preset.name.clone(), preset.name.as_str())
                .detail(detail)
                .selected(selected)
                .trailing_item(84.0, |ui| {
                    delete = ui.add(Button::new(tl!("Delete")).variant(ButtonVariant::Secondary).size(ControlSize::Sm)).clicked();
                })
                .show(ui);
            if delete {
                match app.run("document.presets.delete", json!({"name":preset.name})) {
                    Ok(_) => {
                        f.remove("__presetError");
                        if selected {
                            f.remove("__savedPreset");
                        }
                    }
                    Err(e) => {
                        f.insert("__presetError".into(), json!(e.to_string()));
                    }
                }
            } else if row.response.clicked() {
                apply_saved_preset(f, preset);
                // Touch-tolerant double-tap (finger jitter > egui's 6 px); see `widgets::second_click`.
                create |= widgets::second_click(ui, &row.response);
            }
        }
    });
    create
}

fn save_preset(app: &mut crate::PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    ui.add_space(craft::ui::theme::scale::Space::S12.px());
    if f.get("__savingPreset").and_then(Value::as_bool) != Some(true) {
        if ui.add(Button::new(tl!("Save Preset…")).variant(ButtonVariant::Secondary).min_width(240.0)).clicked() {
            f.insert("__savingPreset".into(), json!(true));
            f.remove("__presetError");
        }
    } else {
        // Consume before the field sees Enter, so naming can never confirm the outer dialog.
        let enter = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let mut name = get_s(f, "__presetName", "");
        Field::new(tl!("Preset Name")).above().show(ui, |ui, _| {
            let edit = widgets::dialog_text(ui, "document-preset-name", tl!("Preset Name"), &mut name);
            if edit.changed {
                f.insert("__presetName".into(), json!(name));
            }
            edit.response
        });
        ui.horizontal(|ui| {
            if ui.add(Button::new(tl!("Save")).variant(ButtonVariant::Secondary).min_width(110.0)).clicked() || enter {
                match app.run("document.presets.save", json!({"name":name,"settings":document_settings(f)})) {
                    Ok(_) => {
                        if let Some(preset) = app.session.presets.documents.last() {
                            apply_saved_preset(f, preset);
                        }
                        f.insert("__category".into(), json!("Saved"));
                        f.remove("__savingPreset");
                        f.remove("__presetName");
                        f.remove("__presetError");
                        ui.ctx().request_repaint();
                    }
                    Err(e) => {
                        f.insert("__presetError".into(), json!(e.to_string()));
                    }
                }
            }
            if ui.add(Button::new(tl!("Cancel")).variant(ButtonVariant::Secondary).min_width(110.0)).clicked() {
                f.remove("__savingPreset");
                f.remove("__presetError");
            }
        });
    }
    if let Some(error) = f.get("__presetError").and_then(Value::as_str) {
        // The engine's message stays copyable, unlike the static labels around it.
        ui.add(egui::Label::new(error).wrap().selectable(true));
    }
}

/// A preset card: a page thumbnail at the preset's aspect, its title (two lines at most) and its
/// size line. Returns the card's response and whether the title was elided.
fn preset_card(ui: &mut egui::Ui, index: usize, c: &Card, units: &UnitsAndRulers, f: &Map<String, Value>, on: bool) -> (egui::Response, String) {
    let tokens = LibTokens::of(ui);
    let size = c.size_line(units, f);
    let title = tl!(c.title());
    let card = vec2(164.0, 112.0);
    let resp = Interactive::new(("new-document-card", index), Role::Button, format!("{title}, {size}"))
        .surface(SurfaceKind::Sunken)
        .selected(on)
        .show(ui, card)
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let r = resp.rect;
    if on {
        let radius: egui::CornerRadius = tokens.shape.radius.control.into();
        ui.painter().rect_stroke(r, radius, craft::ui::theme::scale::Stroke::W1_5.egui(ui.ctx(), tokens.color.accent.solid.to_egui()), StrokeKind::Inside);
    }
    let (w, h, _) = c.size(units);
    page_icon(ui, Rect::from_center_size(pos2(r.center().x, r.top() + 34.0), vec2(40.0, 40.0)), w, h, tokens.color.text.muted.to_egui());
    let title_format = Text::format(&tokens, TypeRole::Label, Content::Normal);
    let galley = card_title(ui.painter(), title, card.x - 12.0, &title_format);
    let elided = galley.elided;
    let color = title_format.color;
    ui.painter().galley(pos2(r.center().x, r.top() + 70.0 - galley.size().y / 2.0), galley, color);
    let size_format = Text::format(&tokens, TypeRole::Caption, Content::Faint);
    ui.painter().text(pos2(r.center().x, r.top() + 97.0), Align2::CENTER_CENTER, &size, size_format.font_id, size_format.color);
    let resp = match c {
        // A recent document's tooltip names everything it restores.
        Card::Recent(s) => {
            let background = match s.background.as_str() {
                "white" => tl!("White"),
                "black" => tl!("Black"),
                "transparent" => tl!("Transparent"),
                "backgroundColor" => tl!("Background Color"),
                hex => hex,
            };
            resp.on_hover_text(format!("{title} · {size} · {}/{} · {background}", s.mode.to_uppercase(), s.depth))
        }
        Card::Builtin(_) if elided => resp.on_hover_text(title),
        Card::Builtin(_) => resp,
    };
    (resp, size)
}

/// The category tabs; a click on one stores it in `__category`.
fn category_tabs(ui: &mut egui::Ui, f: &mut Map<String, Value>, cat: &str) {
    let tabs: Vec<Tab<&'static str>> =
        CATEGORIES.iter().map(|c| c.0).chain(["Saved"]).map(|name| Tab::new(name, if name == "Saved" { tl!("Saved") } else { tl!(name) })).collect();
    let selected = tabs.iter().map(|t| *t.key()).find(|k| *k == cat);
    let mut strip = TabStrip::new("new-document-categories", tl!("Categories"), TabKind::Text);
    if let Some(key) = &selected {
        strip = strip.selected(key);
    }
    let out = strip.show(ui, &tabs);
    for event in out.events {
        if let TabEvent::Activate(name) = event {
            f.insert("__category".into(), json!(name));
        }
    }
}

/// The left column: the preset cards of the category, or the saved presets.
fn preset_grid(app: &mut crate::PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>, cat: &str, presets: &[Card], units: &UnitsAndRulers) -> bool {
    if cat == "Saved" {
        return saved_presets(app, ui, f);
    }
    let chosen = get_s(f, "__preset", "");
    let mut create = false;
    let caption = crate::i18n::fmt(tl!("BLANK DOCUMENT PRESETS ({n})"), &[("n", &presets.len().to_string())]);
    SectionHeader::new("new-document-presets-caption", caption).collapsible(false).show(ui, |ui| {
        ui.add_space(craft::ui::theme::scale::Space::S6.px());
        let card = vec2(164.0, 112.0);
        // Up to 20 recent documents: scroll past three rows rather than grow the dialog.
        egui::ScrollArea::vertical().id_salt("new-document-presets").max_height(3.0 * (card.y + 8.0)).show(ui, |ui| {
            for (n, row) in presets.chunks(3).enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for (i, c) in row.iter().enumerate() {
                        let on = match c {
                            Card::Builtin(p) => chosen == p.0,
                            Card::Recent(settings) => holds(f, settings),
                        };
                        let (resp, _) = preset_card(ui, n * 3 + i, c, units, f, on);
                        if resp.clicked() {
                            match c {
                                Card::Builtin(p) => apply_preset(f, p, units),
                                Card::Recent(settings) => apply_settings(f, settings),
                            }
                            // Touch-tolerant double-tap (finger jitter > egui's 6 px); see `widgets::second_click`.
                            create |= widgets::second_click(ui, &resp);
                        }
                    }
                });
                ui.add_space(craft::ui::theme::scale::Space::S8.px());
            }
        });
    });
    create
}

/// Returns whether a preset double-click requested creation, after applying its settings.
/// A recent category click can make egui count the preset double-click as a triple-click.
pub fn body(app: &mut crate::PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) -> bool {
    // Headings and field labels are static, as in Photoshop: no text cursor over them, no
    // drag-selecting them (#8331). Text fields keep theirs.
    ui.style_mut().interaction.selectable_labels = false;
    let cat = get_s(f, "__category", "Recent");
    let mut create = false;
    category_tabs(ui, f, &cat);
    let units = app.session.prefs().units_and_rulers.clone();
    let presets: Vec<Card> = match CATEGORIES.iter().find(|c| c.0 == cat) {
        Some(c) if c.0 != CATEGORIES[0].0 => c.1.iter().copied().map(Card::Builtin).collect(),
        _ => recent_cards(f, &app.session.presets.recent_documents, &units),
    };
    let presets: Vec<Card> = presets
        .into_iter()
        .map(|c| match c {
            Card::Builtin(p) => Card::Builtin(oriented(&p, f)),
            recent => recent,
        })
        .collect();
    ui.horizontal_top(|ui| {
        // Left: preset grid.
        ui.vertical(|ui| {
            ui.set_width(520.0);
            create |= preset_grid(app, ui, f, &cat, &presets, &units);
        });
        ui.add_space(craft::ui::theme::scale::Space::S12.px());
        // Right: Preset Details.
        ui.vertical(|ui| {
            ui.set_width(260.0);
            SectionHeader::new("new-document-details", tl!("PRESET DETAILS")).collapsible(false).show(ui, |ui| details(app, ui, f));
        });
    });
    create
}

/// The Preset Details column: name, size, orientation, resolution, mode, profile and background.
fn details(app: &mut crate::PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    ui.add_space(craft::ui::theme::scale::Space::S4.px());
    // Keep the canonical English default in state; only the field shows a translation. The
    // dialog opens with the name selected (its first field), so typing replaces it and Tab goes
    // straight on to Width.
    let raw = get_s(f, "name", "Untitled-1");
    let mut name = if raw == "Untitled-1" { tl!("Untitled-1").to_string() } else { raw };
    Field::unlabelled().show(ui, |ui, _| {
        let edit = widgets::dialog_text(ui, "new-document-name", tl!("Name"), &mut name);
        if edit.changed {
            f.insert("name".into(), json!(name));
        }
        edit.response
    });
    ui.add_space(craft::ui::theme::scale::Space::S8.px());
    let ppi = get_f(f, "resolution", 72.0);
    let mut unit = get_s(f, "__unit", "px");
    // A typed unit (`210mm` while in pixels) converts at the new document's resolution.
    let units = widgets::FieldUnits { unit: widgets::unit_of_key(&unit), dpi: f64::from(ppi), ppi: 72.0 };
    let decimals = widgets::unit_decimals(&unit) as usize;
    Field::new(tl!("Width")).above().show(ui, |ui, _| {
        ui.horizontal(|ui| {
            let mut w = shown_size(f, "width", 1920.0, &unit, ppi);
            let edit = widgets::dialog_number_in_units(ui, "new-document-width", tl!("Width"), &mut w, 0.0..=300_000.0, decimals, units);
            if edit.changed {
                set_size(f, "width", w, &unit, ppi);
            }
            let opts: Vec<(String, &str)> = UNITS.iter().map(|u| (u.0.to_string(), u.1)).collect();
            let select = Select::new("nd-unit", tl!("Units"), &mut unit).rows(opts.len(), |i| opts[i].0.clone(), |i| tl!(opts[i].1)).show(ui);
            if select.response.changed() {
                f.insert("__unit".into(), json!(unit));
                f.remove("__savedPreset");
            }
            (edit.response, select.response)
        })
        .inner
    });
    ui.add_space(craft::ui::theme::scale::Space::S4.px());
    Field::new(tl!("Height")).above().show(ui, |ui, _| {
        ui.horizontal(|ui| {
            let mut h = shown_size(f, "height", 1080.0, &unit, ppi);
            let edit = widgets::dialog_number_in_units(ui, "new-document-height", tl!("Height"), &mut h, 0.0..=300_000.0, decimals, units);
            if edit.changed {
                set_size(f, "height", h, &unit, ppi);
            }
            ui.add_space(craft::ui::theme::scale::Space::S6.px());
            // Page orientation (≠ Type › Orientation → Hướng chữ).
            ui.label(crate::i18n::tr_ctx(crate::i18n::current(), "document", "Orientation"));
            let (w, h) = (get_f(f, "width", 1920.0), get_f(f, "height", 1080.0));
            let mut buttons = Vec::new();
            for (icon, portrait) in [(Icon::OrientationPortrait, true), (Icon::OrientationLandscape, false)] {
                let button = ui
                    .add(IconButton::new(icon, if portrait { tl!("Portrait") } else { tl!("Landscape") }).size(ControlSize::Md).selected((h > w) == portrait));
                if button.clicked() {
                    // The pick also turns the presets (`oriented`), even when the size already has it.
                    f.insert("__orientation".into(), json!(if portrait { "portrait" } else { "landscape" }));
                    if (h > w) != portrait {
                        swap_size(f);
                    }
                }
                buttons.push(button);
            }
            (edit.response, buttons.remove(0), buttons.remove(0))
        })
        .inner
    });
    // Under Orientation, as in Photoshop: the document starts as one artboard (#9086).
    let mut artboard = f.get("artboard").and_then(Value::as_bool).unwrap_or(false);
    if Checkbox::bool(tl!("Artboards"), &mut artboard).show(ui).response.changed() {
        f.insert("artboard".into(), json!(artboard));
    }
    ui.add_space(craft::ui::theme::scale::Space::S4.px());
    Field::new(tl!("Resolution")).above().show(ui, |ui, _| {
        ui.horizontal(|ui| {
            let per_cm = get_s(f, "__resUnit", "in") == "cm";
            let mut r = if per_cm { ppi / 2.54 } else { ppi };
            let edit = widgets::dialog_number_field(ui, "new-document-resolution", tl!("Resolution"), &mut r, 1.0..=30_000.0, "", 2);
            if edit.changed {
                set_resolution(f, if per_cm { r * 2.54 } else { r });
            }
            let mut ru = get_s(f, "__resUnit", "in");
            let select = Select::new("nd-resunit", tl!("Resolution"), &mut ru)
                .fit(true)
                .option("in".to_string(), tl!("Pixels/Inch"))
                .option("cm".to_string(), tl!("Pixels/Centimeter"))
                .show(ui);
            if select.response.changed() {
                f.insert("__resUnit".into(), json!(ru));
                f.remove("__savedPreset");
            }
            (edit.response, select.response)
        })
        .inner
    });
    ui.add_space(craft::ui::theme::scale::Space::S4.px());
    Field::new(tl!("Color Mode")).above().show(ui, |ui, _| {
        ui.horizontal(|ui| {
            let mut mode = get_s(f, "mode", "rgb");
            let mode_edit = Select::new("nd-mode", tl!("Color Mode"), &mut mode)
                .fit(true)
                .option("gray".to_string(), tl!("Grayscale"))
                .option("rgb".to_string(), tl!("RGB Color"))
                .option("cmyk".to_string(), tl!("CMYK Color"))
                .option("lab".to_string(), tl!("Lab Color"))
                .show(ui);
            if mode_edit.response.changed() {
                f.insert("mode".into(), json!(mode));
                // A profile of the old mode's colour space doesn't suit the new one.
                f.remove("profile");
                f.remove("__savedPreset");
            }
            let mut depth = f.get("depth").and_then(Value::as_u64).unwrap_or(8);
            let mut select = Select::new("nd-depth", tl!("Depth"), &mut depth).fit(true);
            for (bits, label, tooltip) in DEPTH_OPTIONS {
                select = select.option(*bits, tl!(label)).option_tooltip(tl!(tooltip));
            }
            let depth_edit = select.show(ui);
            if depth_edit.response.changed() {
                f.insert("depth".into(), json!(depth));
                f.remove("__savedPreset");
            }
            (mode_edit, depth_edit.response)
        })
        .inner
    });
    ui.add_space(craft::ui::theme::scale::Space::S4.px());
    // Photoshop's Advanced Options › Color Profile (#321): the working space by default,
    // or another profile of the mode's colour space, built in or installed in the system.
    Field::new(tl!("Color Profile")).above().show(ui, |ui, _| {
        let mode = get_s(f, "mode", "rgb");
        let space = color_mode_of(&mode).map(photocraft_engine::color_cmds::mode_space);
        let working = color_mode_of(&mode).map(|m| app.session.color.working(m).description.clone()).unwrap_or_default();
        let installed = photocraft_engine::installed_profiles::installed();
        let opts = profile_options(space, &working, &installed);
        let mut profile = get_s(f, "profile", "working");
        if !opts.iter().any(|(v, _)| *v == profile) {
            // Not a profile of this mode (or no longer installed): Create must not send it.
            profile = "working".into();
            f.remove("profile");
        }
        let shown: Vec<(String, &str)> = opts.iter().map(|(v, l)| (v.clone(), l.as_str())).collect();
        let edit = fill_width(ui, |ui| {
            Select::new("nd-profile", tl!("Color Profile"), &mut profile).rows(shown.len(), |i| shown[i].0.clone(), |i| tl!(shown[i].1)).show(ui)
        });
        if edit.response.changed() {
            if profile == "working" {
                f.remove("profile");
            } else {
                f.insert("profile".into(), json!(profile));
            }
            f.remove("__savedPreset");
        }
        edit
    });
    ui.add_space(craft::ui::theme::scale::Space::S4.px());
    background(app, ui, f);
    save_preset(app, ui, f);
}

/// `add` over the column's whole width: a menu fills the width a justified layout gives it.
fn fill_width<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.with_layout(egui::Layout::top_down(egui::Align::Min).with_cross_justify(true), add).inner
}

/// Background Contents: the menu and the swatch of the colour the canvas starts with.
fn background(app: &mut crate::PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    // A custom colour is kept as its "#rrggbb", which `file.new` takes as is (#4789).
    let current = get_s(f, "background", "white");
    let mut bg = if crate::color_picker_ui::parse_hex(&current).is_some() { CUSTOM.to_string() } else { current.clone() };
    let opts = [
        ("white".to_string(), tl!("White")),
        ("black".to_string(), tl!("Black")),
        ("backgroundColor".to_string(), tl!("Background Color")),
        ("transparent".to_string(), tl!("Transparent")),
        (CUSTOM.to_string(), tl!("Custom…")),
    ];
    let shown = background_rgb(app, f);
    let mut pick = false;
    Field::new(tl!("Background Contents")).above().show(ui, |ui, _| {
        ui.horizontal(|ui| {
            let menu = ui
                .allocate_ui_with_layout(vec2(240.0 - SWATCH - 6.0, 0.0), egui::Layout::top_down(egui::Align::Min).with_cross_justify(true), |ui| {
                    Select::new("nd-bg", tl!("Background"), &mut bg).rows(opts.len(), |i| opts[i].0.clone(), |i| tl!(opts[i].1)).show(ui)
                })
                .inner;
            if menu.response.changed() {
                if bg == CUSTOM {
                    // Custom… opens the Color Picker on the colour shown so far.
                    pick = true;
                } else {
                    f.insert("background".into(), json!(bg));
                }
                f.remove("__savedPreset");
                // A fresh choice of Background Color uses today's toolbox colour. Selecting a
                // saved preset instead restores the colour captured when it was saved.
                f.remove("backgroundColor");
            }
            // Photoshop's swatch beside the menu: the colour the canvas starts with (a checker for
            // Transparent); a click opens the Color Picker, as Custom… does.
            let color = match shown {
                Some(c) => egui::Color32::from_rgb((c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8),
                None => egui::Color32::TRANSPARENT,
            };
            let swatch =
                ui.add(Swatch::new("new-document-background-swatch", tl!("Custom background color"), color)).on_hover_text(tl!("Custom background color"));
            if swatch.clicked() {
                pick = true;
                f.remove("__savedPreset");
            }
            (menu, swatch)
        })
        .inner
    });
    if pick {
        let start = shown.unwrap_or([1.0; 3]);
        f.insert("background".into(), json!(crate::color_picker_ui::hex(start)));
        f.remove("backgroundColor");
        crate::color_picker_ui::request_field(f, "background", &crate::color_picker_ui::title_for(tl!("Background Contents")));
    }
}

#[cfg(test)]
mod tests;
