use super::dialogs::{mismatch_body, presets_body, presets_io_body};
use super::shortcuts::shortcuts_body;
use super::{Color32, Map, PhotocraftApp, Value, prefs};

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
