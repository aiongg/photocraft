use super::appearance::{AppearanceRows, appearance_rows};
use super::dialogs::{mismatch_body, presets_body, presets_io_body};
use super::graphics::read_at_launch;
use super::shortcuts::shortcuts_body;
use super::{Color32, Map, PhotocraftApp, RichText, Tokens, Value, json, prefs};
use craft_ui::theme::StrokeExt;

// ------------------------------------------------------------------ bodies

/// Render one of our dialogs' bodies.
pub fn body(app: &mut PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>, interactive: bool) -> Option<bool> {
    match f.get("__prefsui").and_then(Value::as_str).unwrap_or("") {
        "prefs" => return super::window::show(app, ui, f, interactive),
        "shortcuts" => shortcuts_body(app, ui, f),
        "presets" => presets_body(app, ui, f),
        "presetsIO" => presets_io_body(ui, f),
        "mismatch" => mismatch_body(ui, f),
        _ => {}
    }
    None
}

/// The label a preference key shows in the dialog: a few keys have their own wording, the rest
/// split camelCase into words (`convertToSrgb` → "Convert to srgb").
pub(super) fn humanize(key: &str) -> String {
    match key {
        "appearanceMode" => return "Appearance Mode".into(),
        "exrAcesCg" => return "Open color-less EXR as ACEScg".into(),
        "darkTheme" => return "Dark Theme".into(),
        "lightTheme" => return "Light Theme".into(),
        "automaticallyCheckUpdates" => return "Automatically Check for Updates".into(),
        _ => {}
    }
    // These controls appear only for WebP, so reuse the existing translated labels.
    if key == "webpLossless" {
        return "Lossless".into();
    }
    if key == "webpQuality" {
        return "Quality".into();
    }
    // The auto-hide notice settings read best with their own labels (Interface › Notification, #2022).
    if key == "notificationAutoHide" {
        return "Auto Hide Notifications".into();
    }
    if key == "notificationDurationSeconds" {
        return "Notification Duration (seconds)".into();
    }
    // Tools: arrow-key nudges bundle into one History state (#2259).
    if key == "bundleNudges" {
        return "Bundle Arrow-Key Nudges".into();
    }
    if key == "nudgeBundlePauseMs" {
        return "Nudge Bundle Pause (ms)".into();
    }
    // Performance: presentation without vsync (#1343); like the other graphics settings, it
    // takes effect when the window next opens.
    // File Handling: Photoshop's wording (#8080).
    if key == "doNotAppendCopy" {
        return "Do not append \"copy\" to filename when saving a copy".into();
    }
    // Plug-ins: Photoshop's wording (#9392).
    if key == "generatorEnabled" {
        return "Enable Generator".into();
    }
    if key == "lowLatencyDrawing" {
        return "Low-Latency Drawing (vsync off, applies at next launch)".into();
    }
    // Performance: the canvas compositor's GPU memory, 0 for the automatic budget (L3).
    if key == "gpuMemoryMb" {
        return "GPU Memory (MB, 0 = Automatic)".into();
    }
    let mut s = String::new();
    for (i, ch) in key.chars().enumerate() {
        if i == 0 {
            s.extend(ch.to_uppercase());
        } else if ch.is_uppercase() {
            s.push(' ');
            s.extend(ch.to_lowercase());
        } else {
            s.push(ch);
        }
    }
    s.replace("Psd", "PSD")
        .replace("Gpu", "GPU")
        .replace("Ui ", "UI ")
        .replace("Mb", "(MB)")
        .replace("Exif", "EXIF")
        .replace("Hud", "HUD")
        .replace("Lut ", "LUT ")
}

pub(super) fn choice_label(v: &str) -> String {
    match v {
        "exr" => "EXR".into(),
        "cm" => "Centimeters".into(),
        "mm" => "Millimeters".into(),
        "75" | "80" | "85" | "90" | "95" | "100" | "105" | "110" | "115" | "120" | "125" | "150" | "175" | "200" | "250" | "300" => format!("{v}%"),
        "8" => "8 Bits/Channel".into(),
        "16" => "16 Bits/Channel".into(),
        "postScript" => "PostScript (72 points/inch)".into(),
        "traditional" => "Traditional (72.27 points/inch)".into(),
        "vulkan" => "Vulkan".into(),
        "dx12" => "DirectX 12".into(),
        "metal" => "Metal".into(),
        "gl" => "OpenGL".into(),
        "ahd" => "AHD (adaptive homogeneity)".into(),
        "mhc" => "Malvar–He–Cutler".into(),
        "cpu" => "CPU (no GPU acceleration)".into(),
        v => humanize(v),
    }
}

/// An editable `#rrggbb` field beside a colour swatch (#668): typing a valid value sets the
/// colour; while it's being typed a partial value is kept, and it shows the colour otherwise.
pub(super) fn hex_field(ui: &mut egui::Ui, key: &str, rgb: &mut [u8; 3]) {
    let id = egui::Id::unique(("pref-hex", key));
    let shown = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
    let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| shown.clone());
    let r = crate::widgets::text_field_ex(ui, id, &mut text, "", 72.0, true, Some(7)).accessible_name(tl!("Color"));
    if r.changed()
        && let Some(c) = prefs::parse_hex(&text)
    {
        *rgb = c;
    }
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
}

pub(super) fn color_of(s: &str) -> Color32 {
    prefs::parse_hex(s).map_or(Color32::GRAY, |c| Color32::from_rgb(c[0], c[1], c[2]))
}

/// Search the same visible, editable preference labels used in the dialog.
pub(super) fn preference_matches(section: &str, key: &str, query: &str) -> bool {
    let label = humanize(key);
    format!("{section}.{key}").to_lowercase().contains(query) || label.to_lowercase().contains(query) || tl!(&label).to_lowercase().contains(query)
}

/// JPEG and WebP have independent settings; unrelated format controls stay out of view.
pub(super) fn export_field_visible(obj: &Map<String, Value>, key: &str) -> bool {
    let format = obj.get("quickExportFormat").and_then(Value::as_str).unwrap_or("png");
    match key {
        "convertToSrgb" | "metadata" => format != "exr",
        "jpegQuality" => format == "jpg",
        "webpLossless" => format == "webp",
        "webpQuality" => format == "webp" && obj.get("webpLossless").and_then(Value::as_bool) != Some(true),
        _ => true,
    }
}

/// The pause that bundles arrow-key nudges means nothing with Bundle Arrow-Key Nudges off (#2259).
pub(super) fn tools_field_visible(obj: &Map<String, Value>, key: &str) -> bool {
    key != "nudgeBundlePauseMs" || obj.get("bundleNudges").and_then(Value::as_bool) != Some(false)
}

/// A titled, outlined group of settings (Photoshop's Interface page has Appearance,
/// Presentation and Options).
fn pref_group(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new().stroke(craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.card_border)).corner_radius(t.radius).inner_margin(10.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(tl!(title)).strong().color(t.text));
        ui.add_space(craft_ui::theme::scale::Space::S4.px());
        add(ui);
    });
    ui.add_space(craft_ui::theme::scale::Space::S8.px());
}

/// Preferences › Interface laid out like Photoshop's: the theme and canvas settings, then UI
/// language, font size and scaling, then everything else. Compact enough that the language
/// shows without scrolling in a small window (#2532). A search (`query`, lowercased) hides the
/// rows that don't match and skips a group left empty.
fn interface_groups(ui: &mut egui::Ui, obj: &mut Map<String, Value>, keys: &[String], lang: crate::i18n::Lang, system: Option<egui::Theme>, query: &str) {
    const APPEARANCE: [&str; 3] = ["canvasColor", "canvasCustomColor", "canvasBorder"];
    const PRESENTATION: [&str; 3] = ["language", "uiFontSize", "uiScale"];
    let shown = |k: &str| keys.iter().any(|key| key == k) && field_visible("interface", obj, k, query);
    // In Photoshop's order, not the stored one.
    let pick = |names: &[&str]| names.iter().filter(|n| shown(n)).map(|n| n.to_string()).collect::<Vec<_>>();
    let appearance = pick(&APPEARANCE);
    let presentation = pick(&PRESENTATION);
    let rest = keys.iter().filter(|k| !APPEARANCE.contains(&k.as_str()) && !PRESENTATION.contains(&k.as_str()) && shown(k));
    // Checkboxes line up at the left edge, as in Photoshop; the other options keep a label column.
    let (checks, other): (Vec<String>, Vec<String>) = rest.cloned().partition(|k| obj.get(k).is_some_and(Value::is_boolean));
    let rows = AppearanceRows::matching(query);
    let grid = |id: &str| egui::Grid::new(("prefs-grid", id.to_string())).num_columns(2).spacing([14.0, 7.0]).min_col_width(150.0);
    if rows.any() || !appearance.is_empty() {
        pref_group(ui, "Appearance", |ui| {
            grid("interface-appearance").show(ui, |ui| {
                appearance_rows(ui, obj, system, rows);
                field_rows(ui, "interface", obj, appearance, lang);
            });
        });
    }
    if !presentation.is_empty() {
        pref_group(ui, "Presentation", |ui| {
            grid("interface-presentation").show(ui, |ui| field_rows(ui, "interface", obj, presentation, lang));
        });
    }
    if !checks.is_empty() || !other.is_empty() {
        pref_group(ui, "Options", |ui| {
            egui::Grid::new("prefs-grid-interface-checks").num_columns(2).spacing([0.0, 7.0]).show(ui, |ui| field_rows(ui, "interface", obj, checks, lang));
            grid("interface-options").show(ui, |ui| field_rows(ui, "interface", obj, other, lang));
        });
    }
}

/// Where a clicked colour swatch leaves its preference path and label for the dialog body, which
/// holds the fields the Color Picker request goes in.
const PICK_ID: &str = "prefs-pick-color";

/// The colour swatch clicked this frame, if any: (preference path, label).
pub(super) fn ctx_take_pick(ui: &egui::Ui) -> Option<(String, String)> {
    ui.ctx().data_mut(|d| d.remove_temp::<(String, String)>(egui::Id::unique(PICK_ID)))
}

/// Whether `section.key` gets a generic row for this search (`query`, lowercased; empty matches
/// everything). Settings nothing reads yet stay out of the dialog (issue #204), as do those drawn
/// by their own widgets; their stored values pass through untouched.
pub(super) fn field_visible(section: &str, obj: &Map<String, Value>, k: &str, query: &str) -> bool {
    let path = format!("{section}.{k}");
    let drawn_elsewhere = (section == "interface" && matches!(k, "theme" | "appearanceMode" | "darkTheme" | "lightTheme" | "favoriteThemes" | "themeLight" | "themeDark" | "size" | "appearanceImported" | "lastSuite"))
        // View › Rulers (⌘R) sets it, as in Photoshop.
        || path == "unitsAndRulers.showRulers"
        || (section == "performance" && matches!(k, "useGpu" | "gpuBackend" | "gpuAdapter" | "renderingMode" | "memoryUsageMb"))
        // The browser presents frames itself: Low-Latency Drawing does nothing there.
        || (cfg!(target_arch = "wasm32") && path == "performance.lowLatencyDrawing")
        || (section == "export" && !export_field_visible(obj, k))
        || (section == "tools" && !tools_field_visible(obj, k));
    (query.is_empty() || preference_matches(section, k, query))
        && !drawn_elsewhere
        && !prefs::is_hidden(&path)
        && obj.get(k).is_some_and(|v| !matches!(v, Value::Null | Value::Object(_)))
}

/// Preferences › Interface: its grouped rows (the page the library's row model does not
/// cover). A search (`query`, lowercased) hides the rows that don't match.
pub(super) fn interface_page(
    ui: &mut egui::Ui,
    obj: &mut Map<String, Value>,
    order: &[String],
    lang: crate::i18n::Lang,
    system: Option<egui::Theme>,
    query: &str,
) {
    let mut keys: Vec<String> = order.iter().filter(|k| obj.contains_key(*k)).cloned().collect();
    keys.extend(obj.keys().filter(|k| !order.contains(k)).cloned());
    interface_groups(ui, obj, &keys, lang, system, query);
}

/// One grid row per setting in `keys` (already filtered by `field_visible`).
fn field_rows(ui: &mut egui::Ui, section: &str, obj: &mut Map<String, Value>, keys: Vec<String>, lang: crate::i18n::Lang) {
    let t = Tokens::get(ui.ctx());
    for k in keys {
        let path = format!("{section}.{k}");
        let v = obj.get(&k).cloned().unwrap_or(Value::Null);
        let human = humanize(&k);
        let label = tl!(&human).to_string();
        match &v {
            Value::Bool(b) => {
                ui.label("");
                let mut b = *b;
                if read_at_launch(&path) {
                    ui.vertical(|ui| {
                        craft_ui::controls::Checkbox::bool(tl!(&label), &mut b).show(ui).response;
                        ui.label(RichText::new(tl!("Applies at next launch.")).color(t.text_dim));
                    });
                } else {
                    craft_ui::controls::Checkbox::bool(tl!(&label), &mut b).show(ui).response;
                }
                obj.insert(k, json!(b));
            }
            Value::String(s) if path == "interface.language" => {
                ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                let mut pairs: Vec<(String, &str)> = vec![("auto".into(), tl!("Auto"))];
                pairs.extend(crate::i18n::Lang::for_picker().into_iter().map(|l| (l.code().to_string(), l.name())));
                let mut cur = s.clone();
                craft_ui::controls::Select::new(&format!("pref-{path}"), tl!(&label), &mut cur)
                    .rows(pairs.len(), |i| pairs[i].0.clone(), |i| tl!(pairs[i].1))
                    .show(ui)
                    .response
                    .changed();
                obj.insert(k, json!(cur));
            }
            Value::String(s) if prefs::choices(&path).is_some() => {
                ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                let opts = prefs::choices(&path).unwrap_or(&[]);
                let labels: Vec<String> = opts.iter().map(|o| choice_label(o)).collect();
                let pairs: Vec<(String, &str)> = opts.iter().map(|o| o.to_string()).zip(labels.iter().map(String::as_str)).collect();
                let mut cur = s.clone();
                craft_ui::controls::Select::new(&format!("pref-{path}"), tl!(&label), &mut cur)
                    .rows(pairs.len(), |i| pairs[i].0.clone(), |i| tl!(pairs[i].1))
                    .show(ui)
                    .response
                    .changed();
                obj.insert(k, json!(cur));
            }
            Value::String(s) if prefs::is_color(&path) => {
                ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                let c = prefs::parse_hex(s).unwrap_or([128, 128, 128]);
                let mut rgb = c;
                ui.horizontal(|ui| {
                    // PhotoCraft's Color Picker, as everywhere else (#2144).
                    if crate::widgets::color_swatch_button(ui, egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]), &label).clicked() {
                        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique(PICK_ID), (path.clone(), label.clone())));
                    }
                    hex_field(ui, &path, &mut rgb);
                });
                obj.insert(k, json!(format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])));
                let _ = color_of(s);
            }
            Value::String(s) => {
                ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                let mut s = s.clone();
                ui.add(egui::TextEdit::singleline(&mut s).desired_width(260.0)).accessible_name(tl!(&label));
                obj.insert(k, json!(s));
            }
            Value::Number(n) => {
                let value_label = ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                let (lo, hi) = prefs::range(&path).unwrap_or((-1e9, 1e9));
                if n.is_u64() || n.is_i64() {
                    let mut x = n.as_i64().unwrap_or(0);
                    ui.add(egui::DragValue::new(&mut x).range(lo as i64..=hi as i64).custom_parser(crate::widgets::parse_num)).labelled_by(value_label.id);
                    obj.insert(k, json!(x));
                    // A row of its own, so the label stays level with its field.
                    if read_at_launch(&path) {
                        ui.end_row();
                        ui.label("");
                        ui.label(RichText::new(tl!("Applies at next launch.")).color(t.text_dim));
                    }
                } else {
                    let mut x = n.as_f64().unwrap_or(0.0);
                    ui.add(egui::DragValue::new(&mut x).range(lo..=hi).speed(0.1).max_decimals(3).custom_parser(crate::widgets::parse_num))
                        .labelled_by(value_label.id);
                    obj.insert(k, json!(x));
                }
            }
            Value::Array(items) => {
                ui.label(RichText::new(tl!(&label)).color(t.text_dim));
                ui.label(RichText::new(crate::i18n::trn(lang, items.len() as u64, "{n} item", "{n} items")).color(t.text_faint));
                if !items.is_empty() && ui.small_button(tl!("Clear")).clicked() {
                    obj.insert(k, json!([]));
                }
            }
            _ => continue,
        }
        ui.end_row();
    }
}
