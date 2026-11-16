//! File › New: the framework's New Document dialog, hosted. This file is the app side: it opens the dialog on the
//! last created settings, carries the colour picker, the saved presets and the recent list between the dialog and the
//! engine, and runs `file.new` with what Create answers.

use craft::behavior::PixelSize;
use craft::theme::Srgba;
use craft::ui::contract::{Bind, SaveKey};
use craft::ui::dialog::{DialogId, DialogResult, StackDialog};
use craft::ui::patterns::form_dialogs::{
    BackgroundContents, BitDepth, ColorMode as DialogMode, ColorProfiles, DocumentOptions, HostedNewDocument, NewDocumentDialog,
};
use craft::ui::patterns::presets::{GroupPath, NamedPresetEvent, PresetLibrary};
use craft::ui::runtime::env::UnitKind;
use photocraft_color::ColorMode;
use photocraft_engine::document_preset_cmds::DocumentSettings;
use serde_json::{Value, json};

<<<<<<< HEAD
use crate::PhotocraftApp;
=======
pub mod pixel_aspect;

/// A blank-document preset: (name, width px, height px, ppi).
pub type Preset = (&'static str, u32, u32, f32);
>>>>>>> upstream/main

/// The SAVED-tier key of the last created settings (without their name): the next New Document opens on them.
pub(crate) const LAST_CREATED: SaveKey = SaveKey::new("photocraft.file.new");

/// The SAVED-tier key of the dialog's place and size.
const WINDOW: SaveKey = SaveKey::new("photocraft.file.new.window");

<<<<<<< HEAD
/// The colour modes the dialog offers, with the engine's mode of each.
const MODES: [(ColorMode, DialogMode); 4] =
    [(ColorMode::Grayscale, DialogMode::Grayscale), (ColorMode::Rgb, DialogMode::Rgb), (ColorMode::Cmyk, DialogMode::Cmyk), (ColorMode::Lab, DialogMode::Lab)];
=======
/// A Bitmap document's only depth (#14030).
const BITMAP_DEPTH_OPTIONS: &[(u64, &str, &str)] = &[(1, "1 bit", "Integer")];

/// The Color Mode menu, in Photoshop's order (#14030).
pub(crate) const MODE_OPTIONS: &[(&str, &str)] =
    &[("bitmap", "Bitmap"), ("gray", "Grayscale"), ("rgb", "RGB Color"), ("cmyk", "CMYK Color"), ("lab", "Lab Color")];

/// The Depth menu for `mode`: Bitmap is 1 bit only.
pub(crate) fn depth_options(mode: &str) -> &'static [(u64, &'static str, &'static str)] {
    if mode == "bitmap" { BITMAP_DEPTH_OPTIONS } else { DEPTH_OPTIONS }
}

/// The Background Contents menu for `mode`: a Bitmap document has no Transparent (#14030).
pub(crate) fn background_options(mode: &str) -> Vec<(String, &'static str)> {
    [
        ("white", tl!("White")),
        ("black", tl!("Black")),
        ("backgroundColor", tl!("Background Color")),
        ("transparent", tl!("Transparent")),
        (CUSTOM, tl!("Custom…")),
    ]
    .into_iter()
    .filter(|(v, _)| !(mode == "bitmap" && *v == "transparent"))
    .map(|(v, l)| (v.to_string(), l))
    .collect()
}

/// `depth` if `mode` offers it, else the mode's first depth: switching to Bitmap locks it to 1 bit,
/// and leaving Bitmap goes back to 8 bit, as Photoshop does (#14030).
pub(crate) fn fit_depth(mode: &str, depth: u64) -> u64 {
    let options = depth_options(mode);
    if options.iter().any(|(bits, _, _)| *bits == depth) { depth } else { options.first().map_or(8, |(bits, _, _)| *bits) }
}

/// Width/Height units: (key, label, units per inch; 0 = pixels).
pub const UNITS: &[(&str, &str, f32)] =
    &[("px", "Pixels", 0.0), ("in", "Inches", 1.0), ("cm", "Centimeters", 2.54), ("mm", "Millimeters", 25.4), ("pt", "Points", 72.0), ("pica", "Picas", 6.0)];
>>>>>>> upstream/main

/// Opens File › New on the last created settings and returns the dialog's id.
pub(crate) fn open(app: &mut PhotocraftApp) -> DialogId {
    let options = app.new_document_options();
    // An image on the clipboard is offered as the Clipboard preset; the dialog opens on it, at its size.
    let clipboard = app.new_document_clipboard();
    let background = app.session.tools.background;
    let background = Srgba::from_rgb_array([background[0], background[1], background[2]]);
    let units = app.session.prefs().units_and_rulers.clone();
    // A resolution that is not a positive finite number leaves the dialog's own default.
    let usable = |ppi: f64| Some(ppi).filter(|ppi| ppi.is_finite() && *ppi > 0.0);
    let (print, screen) = (usable(units.print_resolution as f64), usable(units.screen_resolution as f64));
    let profiles = profiles(app);
    let dialog = HostedNewDocument::new(app.ctx(), options, move |options| {
        let mut dialog =
            NewDocumentDialog::new(Bind::bound(options)).background_color(background).profiles(profiles.clone()).app_name("PhotoCraft").remember(WINDOW);
        if let Some(size) = clipboard {
            dialog = dialog.clipboard(size);
        }
        if let Some(ppi) = print {
            dialog = dialog.print_resolution(ppi);
        }
        if let Some(ppi) = screen {
            dialog = dialog.screen_resolution(ppi);
        }
        dialog
    })
    .saved(saved_library(app))
    .recent(app.session.presets.recent_documents.iter().map(to_options).collect());
    app.framework.dialogs.open(dialog)
}

/// The dialog's pump (a `TAKERS` row). Each call carries only what the dialog queued since the last one: a colour
/// request, the saved-preset edits and the answer of a closed dialog are each taken once, so a second call in the
/// same frame finds nothing to do.
pub(crate) fn take(app: &mut PhotocraftApp, _ctx: &egui::Context) {
    carry_color(app);
    store_preset_events(app);
    if let Some(DialogResult::Ok(result)) = app.framework.dialogs.take::<HostedNewDocument>() {
        create(app, &result.options);
    }
}

/// Opens the colour picker for the dialog's background colour request, and hands the picked colour back.
/// A cancelled picker needs no answer: Background Contents only changes when `set_color` answers.
fn carry_color(app: &mut PhotocraftApp) {
    let Some((id, request)) = app.framework.dialogs.get_mut::<HostedNewDocument>().map(|dialog| (dialog.id(), dialog.pick_color())) else { return };
    if let Some(request) = request {
        crate::color_picker_ui::open_for_request(app, id, request);
    }
    if let Some((key, color)) = crate::color_picker_ui::answer_for(app, id)
        && let Some(dialog) = app.framework.dialogs.get_mut::<HostedNewDocument>()
    {
        dialog.set_color(&key, color);
    }
}

/// Runs the dialog's saved-preset edits on the engine's preset store. The dialog has already applied each to its own
/// library, so a created preset's value is read from it by name.
fn store_preset_events(app: &mut PhotocraftApp) {
    let Some(dialog) = app.framework.dialogs.get_mut::<HostedNewDocument>() else { return };
    let commands: Vec<(&str, Value)> = dialog
        .take_preset_events()
        .into_iter()
        .filter_map(|event| match event {
            NamedPresetEvent::Create { name, .. } => {
                let item = dialog.saved_presets().items().into_iter().find(|item| item.name == name)?;
                Some(("document.presets.save", json!({"name": name, "settings": to_settings(&item.value)})))
            }
            NamedPresetEvent::Delete(name) => Some(("document.presets.delete", json!({"name": name}))),
            NamedPresetEvent::Rename { key, text, .. } => Some(("document.presets.rename", json!({"name": key, "newName": text}))),
            _ => None,
        })
        .collect();
    for (command, params) in commands {
        if let Err(error) = app.run(command, params) {
            app.ui.status = error;
        }
    }
}

/// Creates the document from what Create answered. Only a created document updates the last created settings and the
/// recent list, so a failed Create stores nothing.
fn create(app: &mut PhotocraftApp, options: &DocumentOptions) {
    if let Err(error) = app.run("file.new", create_params(options)) {
        app.ui.status = error;
        return;
    }
    if let Some(i) = app.session.active_index() {
        app.ui.views[i].fit_pending = true;
    }
    let mut kept = options.clone();
    kept.name.clear();
    craft::ui::runtime::memory::set_saved_value(app.ctx(), &LAST_CREATED, Some(kept));
    // Best effort: the document exists either way.
    let _ = app.run("document.recent.add", json!({"settings": to_settings(options)}));
}

/// The engine's saved document presets as the dialog's library, in the engine's order.
fn saved_library(app: &PhotocraftApp) -> PresetLibrary<DocumentOptions> {
    let mut library = PresetLibrary::new();
    for preset in &app.session.presets.documents {
        let mut options = to_options(&preset.settings);
        options.name.clone_from(&preset.name);
        if library.add(&GroupPath::root(), &preset.name, options).is_err() {
            break;
        }
    }
    library
}

/// The Color Profile lists of the dialog: per mode, the working space (with its profile's description), then the
/// built-in and installed profiles of that mode's colour space. A profile is named by what `file.new`'s `profile`
/// takes: a built-in id or a path.
pub(crate) fn profiles(app: &PhotocraftApp) -> Vec<ColorProfiles> {
    let installed = photocraft_engine::installed_profiles::installed();
    MODES
        .iter()
        .map(|(mode, dialog_mode)| {
            let space = photocraft_engine::color_cmds::mode_space(*mode);
            let mut others: Vec<String> =
                photocraft_cms::Builtin::ALL.iter().filter(|b| b.profile().color_space == space).map(|b| b.id().to_string()).collect();
            others.extend(installed.iter().filter(|i| i.is_destination() && i.color_space == space).map(|i| i.path.clone()));
            ColorProfiles::new(*dialog_mode, app.session.color.working(*mode).description.clone(), others)
        })
        .collect()
}

/// The params of `file.new` for what the dialog answered. The display units are not among them.
pub(crate) fn create_params(options: &DocumentOptions) -> Value {
    let mode = MODES.iter().find(|(_, shown)| *shown == options.color_mode).map_or("rgb", |(mode, _)| mode_key(*mode));
    let mut params = json!({
        "name": options.name,
        "width": options.size.width,
        "height": options.size.height,
        "resolution": options.pixels_per_inch,
        "mode": mode,
        "depth": match options.depth {
            BitDepth::Sixteen => 16,
            BitDepth::ThirtyTwo => 32,
            _ => 8,
        },
        "background": match options.background {
            BackgroundContents::Black => "black".to_owned(),
            BackgroundContents::BackgroundColor => "backgroundColor".to_owned(),
            BackgroundContents::Transparent => "transparent".to_owned(),
            BackgroundContents::Custom(color) => crate::color_picker_ui::hex(color),
            _ => "white".to_owned(),
        },
        "artboard": options.artboard,
    });
    if let Some(profile) = &options.profile {
        params["profile"] = json!(profile);
    }
    params
}

<<<<<<< HEAD
/// The key of a colour mode in the engine's settings.
fn mode_key(mode: ColorMode) -> &'static str {
    match mode {
        ColorMode::Grayscale => "gray",
        ColorMode::Cmyk => "cmyk",
        ColorMode::Lab => "lab",
        _ => "rgb",
    }
}

/// The key of a size unit in the engine's settings.
fn unit_key(unit: UnitKind) -> &'static str {
    match unit {
        UnitKind::Inches => "in",
        UnitKind::Centimetres => "cm",
        UnitKind::Millimetres => "mm",
        UnitKind::Points => "pt",
        UnitKind::Picas => "pica",
        _ => "px",
=======
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

/// The category tabs whose presets start the document as an artboard, as in Photoshop (#14042).
const ARTBOARD_CATEGORIES: &[&str] = &["Web", "Mobile"];

/// Set when a Web or Mobile preset ticked Artboards, so a preset of another tab unticks only that
/// and never a box ticked by hand (#14042).
const PRESET_ARTBOARD: &str = "__presetArtboard";

/// Apply a built-in preset (at the preference resolutions, see [`at_pref_resolutions`]) to the
/// dialog fields. A preset of the category tab picked in `__category` also sets Artboards: on for
/// Web and Mobile; the other tabs turn it back off when a Web or Mobile preset turned it on, and
/// leave a box ticked by hand as it is (#14042). Recent's built-in cards and the Clipboard preset
/// leave it as it is.
pub fn apply_preset(f: &mut Map<String, Value>, p: &Preset, units: &UnitsAndRulers) {
    let (name, w, h, ppi) = at_pref_resolutions(p, units);
    let unit = preset_unit(p, f);
    let cat = get_s(f, "__category", CATEGORIES[0].0);
    let tab = CATEGORIES.iter().find(|c| c.0 == cat && c.0 != CATEGORIES[0].0 && c.1.iter().any(|q| q.0 == name));
    f.remove("__savedPreset");
    f.insert("width".into(), json!(w));
    f.insert("height".into(), json!(h));
    f.insert("resolution".into(), json!(ppi));
    f.insert("__preset".into(), json!(name));
    f.insert("__unit".into(), json!(unit));
    match tab {
        Some((tab, _)) if ARTBOARD_CATEGORIES.contains(tab) => {
            if f.get("artboard").and_then(Value::as_bool) != Some(true) {
                f.insert("artboard".into(), json!(true));
                f.insert(PRESET_ARTBOARD.into(), json!(true));
            }
        }
        // Off is the checkbox's and `file.new`'s default.
        Some(_) if f.contains_key(PRESET_ARTBOARD) => {
            f.remove(PRESET_ARTBOARD);
            f.remove("artboard");
        }
        _ => {}
    }
}

/// The colour mode of the dialog's `mode` value.
fn color_mode_of(mode: &str) -> Option<photocraft_color::ColorMode> {
    use photocraft_color::ColorMode;
    match mode {
        "bitmap" => Some(ColorMode::Bitmap),
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
    space: Option<craft_cms::ColorSpace>,
    working: &str,
    installed: &[photocraft_engine::installed_profiles::InstalledProfile],
) -> Vec<(String, String)> {
    let mut v = vec![("working".to_string(), crate::i18n::fmt(tl!("Working: {working}"), &[("working", working)]))];
    let Some(space) = space else { return v };
    v.extend(craft_cms::Builtin::ALL.iter().filter(|b| b.profile().color_space == space).map(|b| (b.id().to_string(), b.description().to_string())));
    v.extend(installed.iter().filter(|i| i.is_destination() && i.color_space == space).map(|i| (i.path.clone(), i.description.clone())));
    v
}

/// Fields `file.new` takes (drops the dialog's `__` UI keys). The depth is fitted to the mode here
/// too, not only while the dialog draws: an agent can set the mode and confirm before the next
/// frame (#14030).
pub fn command_params(f: &Map<String, Value>) -> Value {
    let mut p: Map<String, Value> = f.iter().filter(|(k, _)| !k.starts_with("__")).map(|(k, v)| (k.clone(), v.clone())).collect();
    if let Some(depth) = p.get("depth").and_then(Value::as_u64) {
        p.insert("depth".into(), json!(fit_depth(&get_s(f, "mode", "rgb"), depth)));
    }
    Value::Object(p)
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

fn small_label(ui: &mut egui::Ui, s: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(s).size(11.5).color(t.text_dim));
}

/// Lay out a preset card's title centred in `width`: wrapped onto at most two lines, the rest
/// elided, so long translations stay inside the card.
fn card_title(painter: &egui::Painter, title: &str, width: f32, color: egui::Color32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(title.to_owned(), egui::FontId::proportional(12.0), color, width);
    job.wrap.max_rows = 2;
    job.halign = egui::Align::Center;
    painter.layout_job(job)
}

/// Paint a page thumbnail with the preset's aspect ratio.
fn page_icon(ui: &egui::Ui, r: Rect, w: u32, h: u32, t: &Tokens) {
    let s = 30.0 / (w.max(h) as f32);
    let page = Rect::from_center_size(r.center(), vec2(w as f32 * s, h as f32 * s));
    ui.painter().rect_stroke(page, 1.0, Stroke::new(1.2, t.text_dim), StrokeKind::Inside);
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
>>>>>>> upstream/main
    }
}

/// The settings as `document.presets.save` and `document.recent.add` take them: what `file.new` takes (but the name and
/// the artboard, which the engine's settings do not hold) plus the display units.
pub(crate) fn to_settings(options: &DocumentOptions) -> Value {
    let mut settings = create_params(options);
    if let Some(fields) = settings.as_object_mut() {
        fields.remove("name");
        fields.remove("artboard");
    }
    settings["unit"] = json!(unit_key(options.unit));
    settings["resolutionUnit"] = json!(if options.resolution_unit == UnitKind::PixelsPerCentimetre { "cm" } else { "in" });
    settings
}

<<<<<<< HEAD
/// The engine's stored settings as the dialog's options, without a name. A stored Background Color that carries its
/// captured colour (a preset saved by an earlier version) becomes that custom colour, so it keeps the colour it had.
#[allow(clippy::field_reassign_with_default)] // `DocumentOptions` is `#[non_exhaustive]`: it cannot be built with a struct expression here.
pub(crate) fn to_options(settings: &DocumentSettings) -> DocumentOptions {
    let mut options = DocumentOptions::default();
    options.size = PixelSize::new(settings.width, settings.height);
    options.pixels_per_inch = f64::from(settings.resolution);
    options.unit = match settings.unit.as_str() {
        "in" => UnitKind::Inches,
        "cm" => UnitKind::Centimetres,
        "mm" => UnitKind::Millimetres,
        "pt" => UnitKind::Points,
        "pica" => UnitKind::Picas,
        _ => UnitKind::Pixels,
    };
    options.resolution_unit = if settings.resolution_unit == "cm" { UnitKind::PixelsPerCentimetre } else { UnitKind::PixelsPerInch };
    options.color_mode = match settings.mode.as_str() {
        "gray" | "grayscale" => DialogMode::Grayscale,
        "cmyk" => DialogMode::Cmyk,
        "lab" => DialogMode::Lab,
        _ => DialogMode::Rgb,
    };
    options.depth = match settings.depth {
        16 => BitDepth::Sixteen,
        32 => BitDepth::ThirtyTwo,
        _ => BitDepth::Eight,
    };
    options.profile.clone_from(&settings.profile);
    options.background = match settings.background.as_str() {
        "black" => BackgroundContents::Black,
        "transparent" => BackgroundContents::Transparent,
        "backgroundColor" => settings.background_color.map_or(BackgroundContents::BackgroundColor, BackgroundContents::Custom),
        "white" => BackgroundContents::White,
        hex => crate::color_picker_ui::parse_hex(hex).map_or(BackgroundContents::White, BackgroundContents::Custom),
    };
    options
=======
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
            ui.push_id(&preset.name, |ui| {
                ui.horizontal(|ui| {
                    let selected = get_s(f, "__savedPreset", "") == preset.name;
                    let label = egui::Button::new(&preset.name).selected(selected).truncate();
                    let response = ui.add_sized(vec2(422.0, 32.0), label).on_hover_text(&preset.name);
                    if response.clicked() {
                        apply_saved_preset(f, preset);
                        // Touch-tolerant double-tap (finger jitter > egui's 6 px); see `widgets::second_click`.
                        create |= widgets::second_click(ui, &response);
                    }
                    if widgets::secondary_button(ui, tl!("Delete"), 76.0).clicked() {
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
                    }
                });
                let s = &preset.settings;
                small_label(ui, &format!("{} × {} px @ {} ppi · {}/{}", s.width, s.height, s.resolution, s.mode.to_uppercase(), s.depth));
                ui.add_space(8.0);
            });
        }
    });
    create
}

/// The name Save Document Preset proposes: the first `Saved N` no saved preset has (#14043).
pub fn next_saved_name(presets: &[photocraft_engine::document_preset_cmds::DocumentPreset]) -> String {
    // n presets leave one of `Saved 1`..`Saved n+1` free.
    let free = (1..=presets.len().saturating_add(1)).map(|n| format!("Saved {n}")).find(|name| !presets.iter().any(|p| p.name.eq_ignore_ascii_case(name)));
    free.unwrap_or_else(|| "Saved".into())
}

/// Whether the Save Document Preset pane stands in for Preset Details.
fn saving_preset(f: &Map<String, Value>) -> bool {
    f.get("__savingPreset").and_then(Value::as_bool) == Some(true)
}

/// Photoshop's Save Document Preset pane, opened by the icon beside Name (#14043): the header
/// SAVE DOCUMENT PRESET, a name proposing the next free `Saved N` with its text selected, and
/// Save Preset / Cancel. Cancel goes back to Preset Details without saving.
fn save_preset(app: &mut crate::PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(tl!("SAVE DOCUMENT PRESET")).size(11.0).color(t.text_faint));
    ui.add_space(4.0);
    // Consume every Enter, Ctrl+Enter too, before TextEdit sees it, so naming can never confirm the
    // outer dialog. A dialog stacked above disables this pane and keeps its own Enter.
    let enter = ui.is_enabled() && ui.input_mut(consume_any_enter);
    let mut name = f.get("__presetName").and_then(Value::as_str).map_or_else(|| next_saved_name(&app.session.presets.documents), str::to_owned);
    let r = ui
        .add(egui::TextEdit::singleline(&mut name).id_salt("document-preset-name").desired_width(250.0).font(egui::FontId::proportional(15.0)).char_limit(255))
        .accessible_name(tl!("Preset Name"));
    f.insert("__presetName".into(), json!(name));
    crate::field_tab::register(ui.ctx(), r.id);
    // The pane opens with the proposed name selected, so typing replaces it.
    if f.remove("__presetNameFocus").is_some() {
        crate::field_tab::focus(ui.ctx(), r.id);
    }
    if r.gained_focus() {
        crate::field_tab::select_all(ui.ctx(), r.id, &name);
    }
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if widgets::secondary_button(ui, tl!("Save Preset"), 120.0).clicked() || enter {
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
        if widgets::secondary_button(ui, tl!("Cancel"), 120.0).clicked() {
            f.remove("__savingPreset");
            f.remove("__presetName");
            f.remove("__presetError");
        }
    });
    preset_error(ui, f);
}

/// Takes this frame's Enter presses whatever their modifiers (`consume_key` leaves Ctrl+Enter for
/// the outer dialog's Create) and says whether there was one.
fn consume_any_enter(i: &mut egui::InputState) -> bool {
    let before = i.events.len();
    i.events.retain(|e| !matches!(e, egui::Event::Key { key: egui::Key::Enter, pressed: true, .. }));
    i.events.len() != before
}

/// The last failed save or delete of a preset.
fn preset_error(ui: &mut egui::Ui, f: &Map<String, Value>) {
    if let Some(error) = f.get("__presetError").and_then(Value::as_str) {
        // The engine's message stays copyable, unlike the static labels around it.
        ui.add(egui::Label::new(error).wrap().selectable(true));
    }
}

/// Returns whether a preset double-click requested creation, after applying its settings.
/// A recent category click can make egui count the preset double-click as a triple-click.
pub fn body(app: &mut crate::PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) -> bool {
    let t = Tokens::get(ui.ctx());
    // Headings and field labels are static, as in Photoshop: no text cursor over them, no
    // drag-selecting them (#8331). Text fields keep theirs.
    ui.style_mut().interaction.selectable_labels = false;
    let cat = get_s(f, "__category", "Recent");
    let mut create = false;
    // Category tabs.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        for name in CATEGORIES.iter().map(|c| c.0).chain(["Saved"]) {
            let on = cat == name;
            let label = if name == "Saved" { tl!("Saved") } else { tl!(name) };
            let r = ui
                .add(egui::Label::new(RichText::new(label).size(13.0).color(if on { t.text } else { t.text_dim })).selectable(false).sense(Sense::click()))
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if on {
                ui.painter().line_segment([r.rect.left_bottom() + vec2(0.0, 3.0), r.rect.right_bottom() + vec2(0.0, 3.0)], Stroke::new(2.0, t.text));
            }
            if r.clicked() {
                f.insert("__category".into(), json!(name));
            }
        }
    });
    ui.add_space(6.0);
    widgets::hairline(ui);
    ui.add_space(8.0);
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
    let chosen = get_s(f, "__preset", "");
    ui.horizontal_top(|ui| {
        // Left: preset grid.
        ui.vertical(|ui| {
            ui.set_width(520.0);
            if cat == "Saved" {
                create |= saved_presets(app, ui, f);
                return;
            }
            ui.label(RichText::new(crate::i18n::fmt(tl!("BLANK DOCUMENT PRESETS ({n})"), &[("n", &presets.len().to_string())])).size(11.0).color(t.text_faint));
            ui.add_space(6.0);
            let card = vec2(164.0, 112.0);
            // Up to 20 recent documents: scroll past three rows rather than grow the dialog.
            egui::ScrollArea::vertical().id_salt("new-document-presets").max_height(3.0 * (card.y + 8.0)).show(ui, |ui| {
                for row in presets.chunks(3) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        for c in row {
                            let (r, resp) = ui.allocate_exact_size(card, Sense::click());
                            let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                            let (w, h, _) = c.size(&units);
                            let on = match c {
                                Card::Builtin(p) => chosen == p.0,
                                Card::Recent(settings) => holds(f, settings),
                            };
                            ui.painter().rect_filled(
                                r,
                                t.radius,
                                if on {
                                    t.row_selected
                                } else if resp.hovered() {
                                    t.hover
                                } else {
                                    t.field
                                },
                            );
                            if on {
                                ui.painter().rect_stroke(r, t.radius, Stroke::new(1.5, t.accent), StrokeKind::Inside);
                            }
                            page_icon(ui, Rect::from_center_size(pos2(r.center().x, r.top() + 34.0), vec2(40.0, 40.0)), w, h, &t);
                            let title = card_title(ui.painter(), tl!(c.title()), card.x - 12.0, t.text);
                            let elided = title.elided;
                            ui.painter().galley(pos2(r.center().x, r.top() + 70.0 - title.size().y / 2.0), title, t.text);
                            let size = c.size_line(&units, f);
                            ui.painter().text(pos2(r.center().x, r.top() + 97.0), Align2::CENTER_CENTER, &size, egui::FontId::proportional(10.5), t.text_faint);
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
                                    resp.on_hover_text(format!("{} · {size} · {}/{} · {background}", tl!(c.title()), s.mode.to_uppercase(), s.depth))
                                }
                                Card::Builtin(_) if elided => resp.on_hover_text(tl!(c.title())),
                                Card::Builtin(_) => resp,
                            };
                            if resp.clicked() {
                                match c {
                                    Card::Builtin(p) => apply_preset(f, p, &units),
                                    Card::Recent(settings) => apply_settings(f, settings),
                                }
                                // Touch-tolerant double-tap (finger jitter > egui's 6 px); see `widgets::second_click`.
                                create |= widgets::second_click(ui, &resp);
                            }
                        }
                    });
                    ui.add_space(8.0);
                }
            });
        });
        ui.add_space(10.0);
        // Right: Preset Details.
        ui.vertical(|ui| {
            ui.set_width(260.0);
            // The Save Document Preset pane keeps the height Preset Details had, so the dialog
            // doesn't shrink and jump while saving.
            let details_height = egui::Id::unique("new-document-details-height");
            if saving_preset(f) {
                if let Some(h) = ui.ctx().data(|d| d.get_temp::<f32>(details_height)) {
                    ui.set_min_height(h);
                }
                save_preset(app, ui, f);
                return;
            }
            ui.label(RichText::new(tl!("PRESET DETAILS")).size(11.0).color(t.text_faint));
            ui.add_space(4.0);
            // Keep the canonical English default in state; only the TextEdit shows a translation.
            let raw = get_s(f, "name", "Untitled-1");
            let mut name = if raw == "Untitled-1" { tl!("Untitled-1").to_string() } else { raw };
            let r = ui
                .horizontal(|ui| {
                    const ICON: f32 = 24.0;
                    let width = 250.0 - ICON - ui.spacing().item_spacing.x;
                    let r =
                        ui.add(egui::TextEdit::singleline(&mut name).desired_width(width).font(egui::FontId::proportional(15.0))).accessible_name(tl!("Name"));
                    // Photoshop's save icon right of Name opens the Save Document Preset pane (#14043).
                    if icons::button(ui, icons::FOOTER_NEW, ICON, false, "Save Document Preset").clicked() {
                        f.insert("__savingPreset".into(), json!(true));
                        f.insert("__presetNameFocus".into(), json!(true));
                        f.remove("__presetName");
                        f.remove("__presetError");
                    }
                    r
                })
                .inner;
            if r.changed() {
                f.insert("name".into(), json!(name));
            }
            crate::field_tab::register(ui.ctx(), r.id);
            // Photoshop opens the dialog with the name selected, so typing replaces it and Tab goes
            // straight on to Width. (`__focused` marks that this instance has had its turn.)
            if !f.contains_key("__focused") {
                f.insert("__focused".into(), json!(true));
                crate::field_tab::focus(ui.ctx(), r.id);
            }
            if r.gained_focus() {
                crate::field_tab::select_all(ui.ctx(), r.id, &name);
            }
            ui.add_space(8.0);
            let ppi = get_f(f, "resolution", 72.0);
            let mut unit = get_s(f, "__unit", "px");
            // A typed unit (`210mm` while in pixels) converts at the new document's resolution.
            let units = widgets::FieldUnits { unit: widgets::unit_of_key(&unit), dpi: f64::from(ppi), ppi: 72.0 };
            small_label(ui, tl!("Width"));
            ui.horizontal(|ui| {
                let mut w = shown_size(f, "width", 1920.0, &unit, ppi);
                if widgets::value_field_units(ui, &mut w, 0.0..=300_000.0, 110.0, widgets::unit_decimals(&unit), units).accessible_name(tl!("Width")).changed()
                {
                    set_size(f, "width", w, &unit, ppi);
                }
                let opts: Vec<(String, &str)> = UNITS.iter().map(|u| (u.0.to_string(), u.1)).collect();
                if widgets::dropdown(ui, "nd-unit", tl!("Units"), &mut unit, &opts, 120.0) {
                    f.insert("__unit".into(), json!(unit));
                    f.remove("__savedPreset");
                }
            });
            small_label(ui, tl!("Height"));
            ui.horizontal(|ui| {
                let mut h = shown_size(f, "height", 1080.0, &unit, ppi);
                if widgets::value_field_units(ui, &mut h, 0.0..=300_000.0, 110.0, widgets::unit_decimals(&unit), units).accessible_name(tl!("Height")).changed()
                {
                    set_size(f, "height", h, &unit, ppi);
                }
                ui.add_space(6.0);
                // Page orientation (≠ Type › Orientation → Hướng chữ).
                small_label(ui, crate::i18n::tr_ctx(crate::i18n::current(), "document", "Orientation"));
                let (w, h) = (get_f(f, "width", 1920.0), get_f(f, "height", 1080.0));
                for (icon, portrait) in [("rectangle-vertical", true), ("rectangle-horizontal", false)] {
                    // Tooltip strings are looked up via `icons::button` → `tl!`; enum/state is aspect swap only.
                    if icons::button(ui, icon, 24.0, (h > w) == portrait, if portrait { "Portrait" } else { "Landscape" }).clicked() {
                        // The pick also turns the presets (`oriented`), even when the size already has it.
                        f.insert("__orientation".into(), json!(if portrait { "portrait" } else { "landscape" }));
                        if (h > w) != portrait {
                            swap_size(f);
                        }
                    }
                }
            });
            // Under Orientation, as in Photoshop: the document starts as one artboard (#9086).
            let mut artboard = f.get("artboard").and_then(Value::as_bool).unwrap_or(false);
            if widgets::checkbox(ui, &mut artboard, tl!("Artboards")).changed() {
                f.insert("artboard".into(), json!(artboard));
                // Now the user's choice: another tab's preset leaves it alone.
                f.remove(PRESET_ARTBOARD);
            }
            ui.add_space(4.0);
            small_label(ui, tl!("Resolution"));
            ui.horizontal(|ui| {
                let per_cm = get_s(f, "__resUnit", "in") == "cm";
                let mut r = if per_cm { ppi / 2.54 } else { ppi };
                if widgets::value_field(ui, &mut r, 1.0..=30_000.0, "", 110.0).accessible_name(tl!("Resolution")).changed() {
                    set_resolution(f, if per_cm { r * 2.54 } else { r });
                }
                let mut ru = get_s(f, "__resUnit", "in");
                if widgets::dropdown(
                    ui,
                    "nd-resunit",
                    tl!("Resolution"),
                    &mut ru,
                    &[("in".to_string(), tl!("Pixels/Inch")), ("cm".to_string(), tl!("Pixels/Centimeter"))],
                    120.0,
                ) {
                    f.insert("__resUnit".into(), json!(ru));
                    f.remove("__savedPreset");
                }
            });
            ui.add_space(4.0);
            small_label(ui, tl!("Color Mode"));
            ui.horizontal(|ui| {
                let mut mode = get_s(f, "mode", "rgb");
                let modes: Vec<(String, &str)> = MODE_OPTIONS.iter().map(|(v, l)| (v.to_string(), tl!(*l))).collect();
                if widgets::dropdown(ui, "nd-mode", tl!("Color Mode"), &mut mode, &modes, 110.0) {
                    f.insert("mode".into(), json!(mode));
                    // A profile of the old mode's colour space doesn't suit the new one.
                    f.remove("profile");
                    f.remove("__savedPreset");
                }
                let held = f.get("depth").and_then(Value::as_u64).unwrap_or(8);
                let mut depth = fit_depth(&mode, held);
                if depth != held {
                    // Not a depth of this mode (as after switching into or out of Bitmap): Create
                    // must not send it.
                    f.insert("depth".into(), json!(depth));
                }
                if widgets::dropdown_with_tooltips(ui, "nd-depth", tl!("Depth"), &mut depth, depth_options(&mode), 120.0) {
                    f.insert("depth".into(), json!(depth));
                    f.remove("__savedPreset");
                }
            });
            ui.add_space(4.0);
            // Photoshop's Advanced Options › Color Profile (#321): the working space by default,
            // or another profile of the mode's colour space, built in or installed in the system.
            small_label(ui, tl!("Color Profile"));
            {
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
                if widgets::dropdown(ui, "nd-profile", tl!("Color Profile"), &mut profile, &shown, 240.0) {
                    if profile == "working" {
                        f.remove("profile");
                    } else {
                        f.insert("profile".into(), json!(profile));
                    }
                    f.remove("__savedPreset");
                }
            }
            ui.add_space(4.0);
            // Photoshop's Advanced Options › Pixel Aspect Ratio, under Color Profile (#14034).
            small_label(ui, tl!("Pixel Aspect Ratio"));
            {
                let opts = pixel_aspect::options(&app.ui.shell.custom_pars);
                let mut par = pixel_aspect::picked(f, &app.ui.shell.custom_pars);
                let shown: Vec<(String, &str)> = opts.iter().map(|(v, l)| (v.clone(), l.as_str())).collect();
                if widgets::dropdown_literal(ui, "nd-pixel-aspect", tl!("Pixel Aspect Ratio"), &mut par, &shown, 240.0) {
                    f.insert(pixel_aspect::FIELD.into(), json!(par));
                }
            }
            ui.add_space(4.0);
            small_label(ui, tl!("Background Contents"));
            // A custom colour is kept as its "#rrggbb", which `file.new` takes as is (#4789).
            let current = get_s(f, "background", "white");
            let mut bg = if crate::color_picker_ui::parse_hex(&current).is_some() { CUSTOM.to_string() } else { current.clone() };
            let opts = background_options(&get_s(f, "mode", "rgb"));
            let shown = background_rgb(app, f);
            let mut pick = false;
            ui.horizontal(|ui| {
                if widgets::dropdown(ui, "nd-bg", tl!("Background"), &mut bg, &opts, 240.0 - SWATCH - 6.0) {
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
                // Photoshop's swatch beside the menu: the colour the canvas starts with; a click
                // opens the Color Picker, as Custom… does.
                let (r, resp) = ui.allocate_exact_size(vec2(SWATCH, SWATCH), Sense::click());
                match shown {
                    Some(c) => {
                        ui.painter().rect_filled(
                            r,
                            2.0,
                            egui::Color32::from_rgb((c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8),
                        );
                    }
                    None => widgets::checker(ui.painter(), r, 4.0),
                }
                ui.painter().rect_stroke(r, 2.0, Stroke::new(1.0, Tokens::get(ui.ctx()).field_border), StrokeKind::Inside);
                let resp = resp.on_hover_text(tl!("Custom background color")).on_hover_cursor(egui::CursorIcon::PointingHand);
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("Custom background color")));
                if resp.clicked() {
                    pick = true;
                    f.remove("__savedPreset");
                }
            });
            if pick {
                let start = shown.unwrap_or([1.0; 3]);
                f.insert("background".into(), json!(crate::color_picker_ui::hex(start)));
                f.remove("backgroundColor");
                crate::color_picker_ui::request_field(f, "background", &crate::color_picker_ui::title_for(tl!("Background Contents")));
            }
            ui.add_space(12.0);
            preset_error(ui, f);
            let h = ui.min_rect().height();
            ui.ctx().data_mut(|d| d.insert_temp(details_height, h));
        });
    });
    create
>>>>>>> upstream/main
}

#[cfg(test)]
mod tests;
