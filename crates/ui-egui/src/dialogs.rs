//! Dialogs rendered from `UiState::dialogs`. Field values live in the dialog data, so automation can
//! set them (`ui.dialog.set`) and confirm (`ui.dialog.confirm`) exactly like a user.
//!
//! The frame, footer buttons, placement, stacking, Alt-reset, local undo and pass-through are
//! `craft_ui::dialog::Dialog`; [`show`] feeds it the dialog's JSON fields (as [`Settings`]).
//! A dialog body plugs in with four pieces:
//! - a marker key in its fields and an `owns(fields)` test, routed in `command_body` (body),
//!   `confirm_command` and [`ROUTES`] (confirm);
//! - `body(ui, fields)`, which draws the controls only: no title, rule or buttons;
//! - the footer in `footer_for` (labels, roles, enabled state), chosen from the same markers;
//! - its width class in `width_of`, or its sizes in `resize_of`.

use serde_json::{Value, json};
use std::cell::{Cell, RefCell};

use crate::PhotocraftApp;
use crate::state::{Dialog, DialogKind};
use craft_ui::dialog::{Alert, Body, Dialog as HostDialog, DialogLabels, DialogStack, Footer, InitialFocus, Outcome, PlaceKey, Reset, dialog_id};

/// A dialog body sets this field to the key of the text field whose Browse… button was clicked;
/// [`show`] takes it out again and opens the matching file or folder dialog.
pub(crate) const BROWSE_FIELD: &str = "__browse";

/// A text field that names a folder: an export's `dir`, and the source (`input`) and destination
/// (`output`) folders of Image Processor, Batch, Lens Correction, Contact Sheet and Create
/// Droplet, which Photoshop picks with Select Folder… / Choose… (#9357).
pub(crate) fn is_folder_field(key: &str) -> bool {
    matches!(key, "dir" | "input" | "output")
}

/// Browse… beside a path field of dialog `id`: a folder picker for folder fields
/// ([`is_folder_field`]), a `.wasm` picker for Install Plug-in…'s `path`. The answer fills the field.
fn browse_field(app: &mut PhotocraftApp, id: u64, fields: &serde_json::Map<String, Value>, key: &str) {
    let key = key.to_string();
    let r = if is_folder_field(&key) {
        app.ask_folder(move |app, path| fill_field(app, id, &key, path))
    } else if fields.get("__command").and_then(Value::as_str) == Some("plugin.install") {
        let initial_dir = crate::file_dialog::last_used_dir(&app.ui.recent_files);
        let request = crate::file_dialog::FileDialogRequest::Open { multiple: false, initial_dir, extensions: Some(vec!["wasm".into()]) };
        app.ask_file(request, move |app, answer| match answer {
            crate::file_dialog::FileDialogAnswer::Paths(mut paths) if !paths.is_empty() => fill_field(app, id, &key, paths.remove(0)),
            _ => Err("Install Plug-in reads a .wasm file from disk".into()),
        })
    } else {
        return;
    };
    if let Err(e) = r.map(|_| ()).or_else(|e| if e == crate::file_dialog::CANCELLED { Ok(()) } else { Err(e) }) {
        app.ui.status = e;
    }
}

fn fill_field(app: &mut PhotocraftApp, id: u64, key: &str, path: String) -> Result<Value, String> {
    let d = app.ui.dialog_mut(id).ok_or("the dialog was closed")?;
    d.fields.insert(key.into(), json!(path));
    Ok(Value::Null)
}

// Canvas input under an open dialog (pan, zoom, eyedropper picks) is the library's: the dialogs
// shown by `craft_ui::dialog` record where they are.
pub use craft_ui::dialog::pass::{free_pointer_over, free_press, pan_delta};

/// The primary button's drag and click on the free canvas under an open dialog, as the canvas
/// response would report them if it weren't inert: for the held Zoom (⌘Space / ⌥Space, #6460).
pub fn free_buttons(ctx: &egui::Context, canvas: egui::Rect) -> crate::paint_mouse::Buttons {
    let b = craft_ui::dialog::pass::free_buttons(ctx, canvas);
    crate::paint_mouse::Buttons { started: b.started, dragged: b.dragged, stopped: b.stopped, clicked: b.clicked }
}

/// The egui id of the area dialog `id` is drawn in.
#[cfg(test)]
pub(crate) fn area_id(id: u64) -> egui::Id {
    craft_ui::dialog::dialog_id(id)
}

/// Where the open dialogs are on screen.
#[cfg(test)]
pub(crate) fn rects(h: &egui_kittest::Harness<'_, PhotocraftApp>) -> Vec<egui::Rect> {
    h.state().ui.dialogs.iter().filter_map(|d| h.ctx.memory(|m| m.area_rect(area_id(d.id)))).collect()
}

/// Draw a dialog body that opens on its first number, selected, as Photoshop's value dialogs do:
/// typing replaces it and Enter applies it (#1757). Once per dialog `id`, on the first laid-out
/// frame.
fn first_field_focused<R>(ui: &mut egui::Ui, id: egui::Id, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let ctx = ui.ctx().clone();
    let focused = id.with("first-field");
    let first = !ui.is_sizing_pass() && !ctx.data(|m| m.get_temp::<bool>(focused).unwrap_or(false));
    if first {
        ctx.data_mut(|m| m.insert_temp(focused, true));
    }
    crate::widgets::focus_first_field(&ctx, first);
    let r = body(ui);
    crate::widgets::focus_first_field(&ctx, false);
    r
}

/// Where a dialog's last position is remembered: per command for command dialogs (each filter has
/// its own), per kind for the rest.
fn place_id(d: &Dialog) -> egui::Id {
    match d.fields.get("__command").and_then(Value::as_str) {
        Some(cmd) => egui::Id::unique(("dialog-place", cmd)),
        None => egui::Id::unique(("dialog-place", format!("{:?}", d.kind))),
    }
}

/// A dialog's fields as the library's dialog keeps them: the whole map, but two maps are equal
/// when their public keys are. The private `__` keys (preview toggle, command, label…) are chrome
/// and not a step of the dialog's own undo (#4697).
#[derive(Clone, Debug, Default)]
pub(crate) struct Settings(pub Fields);

impl Settings {
    fn public(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.0.iter().filter(|(k, _)| !k.starts_with("__"))
    }
}

impl PartialEq for Settings {
    fn eq(&self, other: &Self) -> bool {
        self.public().count() == other.public().count() && self.public().all(|(k, v)| other.0.get(k) == Some(v))
    }
}

impl std::ops::Deref for Settings {
    type Target = Fields;
    fn deref(&self) -> &Fields {
        &self.0
    }
}

impl std::ops::DerefMut for Settings {
    fn deref_mut(&mut self) -> &mut Fields {
        &mut self.0
    }
}

/// Whether `fields` are the Image › Adjustments › Curves dialog's.
fn is_curves(fields: &serde_json::Map<String, Value>) -> bool {
    fields.get("__adjust").and_then(Value::as_str) == Some("curves")
}

/// The size the Curves dialog first opens at: the 300-point graph it had before it could be
/// resized (#5113); it grows to fit its controls below the graph.
const CURVES_SIZE: egui::Vec2 = egui::vec2(300.0, 420.0);

/// The sizes of a resizable dialog and whether it reopens at the size it was left at (Curves only,
/// #8984), or `None` for a dialog sized by its contents. The smallest size keeps Curves a usable
/// graph (#8984) and the others their two-column layouts; Curves is no wider than its graph can
/// grow, so no blank column.
fn resize_of(ctx: &egui::Context, fields: &Fields) -> Option<(craft_ui::dialog::Resize, bool)> {
    use craft_ui::dialog::{Resize, ResizeMax};
    let min = egui::vec2(560.0, 400.0);
    if crate::prefs_ui::is_preferences(fields) {
        Some((Resize { default: crate::prefs_ui::PREFERENCES_SIZE, min, max: ResizeMax::Window }, false))
    } else if fields.contains_key("__web") {
        Some((Resize { default: egui::vec2(960.0, 900.0), min, max: ResizeMax::Window }, false))
    } else if is_curves(fields) {
        let max = ResizeMax::Fixed(egui::vec2(crate::adjust_editors::curve_side_max(ctx), f32::MAX));
        Some((Resize { default: CURVES_SIZE, min: CURVES_SIZE, max }, true))
    } else {
        None
    }
}

/// The content width class of a dialog that isn't resizable.
fn width_of(d: &Dialog) -> craft_ui::dialog::Width {
    use craft_ui::dialog::Width;
    let f = &d.fields;
    // The About window: room for the contributor table, the same width on every tab.
    let about_tabs = d.kind == DialogKind::About && f.get("systemInfo").and_then(Value::as_bool) != Some(true);
    if crate::duplicate_layer_ui::owns(f) {
        // A fixed small grid (labels + 260 pt fields ≈ 352 pt).
        Width::Fixed(360.0)
    } else if let Some(w) = crate::file_ui::dialog_width(f) {
        Width::Fixed(w)
    } else if crate::color_picker_ui::owns(f) {
        Width::Fixed(crate::color_picker_ui::CONTENT_WIDTH)
    } else if d.kind == DialogKind::NewDocument {
        Width::Fixed(800.0)
    } else if about_tabs {
        Width::Fixed(700.0)
    } else if let Some(width) = crate::prefs_ui::width(f, 0.0) {
        width
    } else if f.contains_key("__compact") {
        // A dialog of a few short fields (Fit Image) is narrower (#4673).
        Width::Compact
    } else if d.kind == DialogKind::LayerStyle || f.contains_key("__export") {
        Width::Wide
    } else {
        Width::Standard
    }
}

/// Whether dialogs have a close box (×) in their title bar (#5496). Photoshop's modal dialogs have
/// one on Windows, where clicking it is Cancel, and none on macOS; Linux and the web follow Windows.
pub(crate) const CLOSE_BOX: bool = !cfg!(target_os = "macos");

/// The dialog's contents under the title rule, picked by its kind and the marker field that routes
/// it. Returns `Some(true)` / `Some(false)` when the body itself answered OK / Cancel.
/// The body of a command dialog: the first handler whose marker the fields carry. The prompts and
/// small editors come first; the rest are in `command_body_more`.
fn command_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, d: &Dialog, fields: &mut Fields, id: egui::Id, interactive: bool) -> Option<bool> {
    let mut outcome: Option<bool> = None;
    match d.kind {
        _ if crate::open_prompt::owns_damage(fields) => crate::open_prompt::damage_body(ui, fields),
        _ if crate::guide_layout_ui::owns(fields) => outcome = crate::guide_layout_ui::body(app, ui, fields, interactive),
        _ if crate::brush_preset_dialog::owns(fields) => outcome = crate::brush_preset_dialog::body(app, ui, fields, interactive),
        _ if crate::fill_ui::owns(fields) => crate::fill_ui::body(app, ui, fields),
        _ if crate::pattern_fill_ui::owns(fields) => crate::pattern_fill_ui::body(app, ui, fields),
        _ if crate::stroke_ui::owns(fields) => crate::stroke_ui::body(ui, fields),
        _ if crate::new_group_ui::owns(fields) => crate::new_group_ui::body(ui, fields),
        _ if crate::snapshot_dialog::owns(fields) => crate::snapshot_dialog::body(ui, fields),
        _ if crate::channel_options_ui::owns(fields) => crate::channel_options_ui::body(ui, fields),
        _ if crate::channel_options_ui::owns_new_spot(fields) => crate::channel_options_ui::new_spot_body(ui, fields),
        _ if crate::shape_dialog::owns(fields) => crate::shape_dialog::body(app, ui, fields),
        _ if crate::delete_layer_prompt::owns(fields) => crate::delete_layer_prompt::body(ui, fields),
        _ if crate::disk_change_prompt::owns(fields) => crate::disk_change_prompt::body(ui, fields),
        _ if crate::replace_prompt::owns(fields) => crate::replace_prompt::body(ui, fields),
        _ if crate::actions::owns_insert_stop(fields) => crate::actions::insert_stop_body(ui, fields),
        _ if crate::new_layer_ui::owns(fields) => crate::new_layer_ui::body(ui, fields),
        _ if crate::mask_trash_prompt::owns(fields) => crate::mask_trash_prompt::body(ui, fields),
        _ if crate::rasterize_prompt::owns(fields) => crate::rasterize_prompt::body(ui, fields),
        _ if crate::duplicate_layer_ui::owns(fields) => crate::duplicate_layer_ui::body(app, ui, fields),
        _ if crate::stack_dialog::owns(fields) => crate::stack_dialog::body(app, ui, fields, d.id),
        _ if crate::tool_reset::owns(fields) => crate::tool_reset::body(ui),
        _ if crate::crop_presets::owns(fields) => crate::crop_presets::body(app, ui, fields),
        _ if crate::variables_ui::owns(fields) => crate::variables_ui::body(app, ui, fields),
        _ => return command_body_more(app, ui, d, fields, id),
    }
    outcome
}

/// The rest of the command dialogs: files, colour, export, sizing, filters and forms.
fn command_body_more(app: &mut PhotocraftApp, ui: &mut egui::Ui, d: &Dialog, fields: &mut Fields, id: egui::Id) -> Option<bool> {
    let mut outcome: Option<bool> = None;
    match d.kind {
        _ if crate::file_ui::owns(fields) => crate::file_ui::body(app, ui, fields, d.id),
        _ if crate::color_picker_ui::owns(fields) => {
            outcome = crate::color_picker_ui::body(ui, fields);
            crate::color_picker_ui::take_add_swatch(app, fields);
            if let Some(web) = fields.get("__webOnly").and_then(Value::as_bool) {
                app.ui.color_panel.web_only = web;
            }
        }
        _ if crate::color_range_ui::owns(fields) => crate::color_range_ui::body(app, ui, fields),
        _ if crate::prefs_ui::owns(fields) => crate::prefs_ui::body(app, ui, fields),
        _ if fields.contains_key("__export") => crate::export_dialog::body(app, ui, fields),
        // Image Size and Canvas Size open on Width, selected (#2329).
        _ if fields.contains_key("__sizing") => first_field_focused(ui, id, |ui| crate::sizing::body(ui, fields)),
        _ if crate::smart_filter_dialog::owns(fields) => crate::smart_filter_dialog::body(app, ui, fields),
        _ if crate::adjust_dialog::owns(fields) => crate::adjust_dialog::body(app, ui, fields),
        _ if crate::blur_pins::owns(fields) => crate::blur_pins::body(ui, fields),
        _ if is_filter_route(fields) => filter_body(app, ui, fields, id),
        _ if crate::new_guide_ui::owns(fields) => crate::new_guide_ui::body(ui, fields),
        _ if fields.contains_key("__form") => crate::view_cmds::form_body(ui, fields),
        _ => {}
    }
    outcome
}

fn dialog_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, d: &Dialog, fields: &mut Fields, id: egui::Id, interactive: bool) -> Option<bool> {
    let mut outcome: Option<bool> = None;
    match d.kind {
        DialogKind::NewDocument => {
            if crate::new_doc_ui::body(app, ui, fields) {
                outcome = Some(true);
            }
        }
        DialogKind::About if fields.get("systemInfo").and_then(Value::as_bool) == Some(true) => {
            let lines = crate::gpu_status::system_info_ui(app);
            for l in &lines {
                ui.add(egui::Label::new(egui::RichText::new(l).font(crate::theme::mono(12.0))).selectable(true));
            }
            ui.add_space(craft_ui::theme::scale::Space::S8.px());
            if ui.add(craft_ui::controls::Button::new(tl!("Copy")).variant(craft_ui::controls::ButtonVariant::Secondary).min_width(84.0)).clicked() {
                ui.ctx().copy_text(crate::gpu_status::system_info(app).join("\n"));
            }
        }
        DialogKind::About => {
            // Tabs About · Contributors · Models (craftrules standards/contributors.md). The
            // tab is a dialog field, so automation can switch it with `ui.dialog.set`.
            let tab = about_tab(fields);
            let mut chosen = tab;
            ui.horizontal(|ui| {
                for (key, label) in [("about", tl!("About")), ("contributors", tl!("Contributors")), ("models", tl!("Models"))] {
                    if crate::widgets::pill_tab(ui, label, tab == key).clicked() {
                        chosen = key;
                    }
                }
            });
            if chosen != tab {
                fields.insert("tab".into(), json!(chosen));
            }
            ui.add_space(craft_ui::theme::scale::Space::S8.px());
            about_tab_body(app, ui, chosen);
        }
        DialogKind::Command => outcome = command_body(app, ui, d, fields, id, interactive),
        DialogKind::LayerStyle => crate::layer_style::body(app, ui, fields),
        DialogKind::Error => {
            ui.label(fields.get("message").and_then(Value::as_str).unwrap_or("Error"));
        }
    }
    outcome
}

/// Whether the fields route to [`filter_body`] rather than to one of the editors that come before
/// it in [`command_body_more`] (the adjustments, Smart Filters, blur pins, sizing and the files).
fn is_filter_route(fields: &Fields) -> bool {
    fields.contains_key("__filter")
        && !(crate::file_ui::owns(fields)
            || crate::color_picker_ui::owns(fields)
            || crate::color_range_ui::owns(fields)
            || crate::prefs_ui::owns(fields)
            || fields.contains_key("__export")
            || fields.contains_key("__sizing")
            || crate::smart_filter_dialog::owns(fields)
            || crate::adjust_dialog::owns(fields)
            || crate::blur_pins::owns(fields))
}

/// A filter-style dialog's parameters. The library scrolls them (`Body::Scroll`) above the
/// in-dialog preview window, which [`filter_below`] draws.
fn filter_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, fields: &mut Fields, id: egui::Id) {
    // Color Settings: the monitor profile in use can change while it is open.
    if fields.get("__command").and_then(Value::as_str) == Some("edit.colorSettings") {
        fields.insert("__note".into(), Value::String(crate::monitor_status::note(app)));
    }
    let body = |ui: &mut egui::Ui, fields: &mut serde_json::Map<String, Value>| {
        if crate::bitmap_dialog::owns(fields) {
            crate::bitmap_dialog::body(app, ui, fields);
        } else {
            crate::filter_dialog::body(ui, fields);
        }
    };
    // Select and Mask leaves the keyboard to its view keys (F, X, O, V…).
    if crate::select_mask_view::owns(fields) {
        body(ui, fields);
    } else {
        first_field_focused(ui, id, |ui| body(ui, fields));
    }
}

/// The in-dialog preview window under a filter dialog's scrolling parameters (#3769), so it never
/// shifts them.
fn filter_below(app: &PhotocraftApp, d: &Dialog, ui: &mut egui::Ui, fields: &mut Fields) {
    if d.kind == DialogKind::Command && is_filter_route(fields) {
        crate::filter_preview_ui::window(app, d.id, ui, fields);
    }
}

/// The default button's label: what OK means for this dialog.
fn ok_label(d: &Dialog) -> &'static str {
    if d.kind == DialogKind::NewDocument {
        tl!("Create")
    } else if crate::open_prompt::owns_damage(&d.fields) {
        tl!("Continue")
    } else if crate::delete_layer_prompt::owns(&d.fields) {
        tl!("Delete")
    } else if crate::disk_change_prompt::owns(&d.fields) || crate::replace_prompt::owns(&d.fields) {
        tl!("Replace")
    } else if d.fields.contains_key("__export") {
        tl!("Export")
    } else {
        crate::file_ui::ok_label(&d.fields).unwrap_or(tl!("OK"))
    }
}

/// Whether OK and Apply take input, from the dialog's current fields (`(ok, apply)`). Dialogs that
/// need an input first (a preset name, a selection) grey OK out. The dialog body refreshes it each
/// pass, and the footer reads it after the body ([`Footer::ok_enabled_with`]).
fn enabled_for(app: &PhotocraftApp, d: &Dialog, fields: &Fields) -> (bool, bool) {
    if d.kind == DialogKind::Command && crate::prefs_ui::is_preferences(fields) {
        (true, crate::prefs_ui::preferences_changed(app, fields))
    } else if crate::mask_trash_prompt::owns(fields) && !crate::mask_trash_prompt::is_vector(fields) {
        (crate::mask_trash_prompt::can_apply(fields), true)
    } else if crate::rasterize_prompt::is_filter(fields) {
        (true, true)
    } else {
        (crate::crop_presets::can_confirm(app, fields) && crate::stack_dialog::can_confirm(fields), true)
    }
}

/// The button area of a dialog: OK and Cancel, with Apply, an Alternate or Alt-Reset where the
/// dialog has them. `None` for the dialogs that draw their own buttons (Guide Layout, Brush Preset
/// and the Color Picker: a column at the body's side that validates Enter itself). OK and Apply
/// are enabled as `(ok, apply)` says once the body has run.
fn footer_for<'a>(d: &Dialog, enabled: &'a Cell<(bool, bool)>) -> Footer<'a> {
    let fields = &d.fields;
    if matches!(d.kind, DialogKind::About | DialogKind::Error) {
        return Footer::standard().ok(tl!("OK")).without_cancel();
    }
    if crate::guide_layout_ui::owns(fields) || crate::brush_preset_dialog::owns(fields) || crate::color_picker_ui::owns(fields) {
        return Footer::none();
    }
    let mut footer = Footer::standard().cancel(if d.kind == DialogKind::NewDocument { tl!("Close") } else { tl!("Cancel") });
    footer = footer.labels(dialog_labels());
    if d.kind == DialogKind::Command && crate::prefs_ui::is_preferences(fields) {
        return footer.ok(ok_label(d)).apply(tl!("Apply")).apply_enabled_with(move || enabled.get().1);
    }
    if crate::mask_trash_prompt::owns(fields) && !crate::mask_trash_prompt::is_vector(fields) {
        // Photoshop: Apply / Cancel / Delete. Delete answers with the choice set.
        return footer.ok(tl!("Apply")).ok_enabled_with(move || enabled.get().0).alternate(tl!("Delete"));
    }
    if crate::rasterize_prompt::is_filter(fields) {
        // Photoshop: Convert to Smart Object / Rasterize / Cancel (#4303).
        footer = footer.ok(tl!("Rasterize"));
        return if crate::rasterize_prompt::offers_smart_object(fields) { footer.alternate(tl!("Convert to Smart Object")) } else { footer };
    }
    footer = footer.ok(ok_label(d)).ok_enabled_with(move || enabled.get().0);
    // Photoshop: holding Alt turns Cancel into Reset (the dialog stays open). Dialogs opt in via
    // `dialog_alt_reset::supported` (per marker).
    if crate::dialog_alt_reset::supported(fields) {
        footer = footer.reset(Reset::AltCancel(tl!("Reset").into()));
    }
    footer
}

/// The footer's own strings in the UI language.
fn dialog_labels() -> DialogLabels {
    let mut labels = DialogLabels::default();
    labels.default_hint = tl!("Default").into();
    labels
}

/// What the answer to a prompt that offers a choice (`choice` field) is, for the role pressed.
fn set_choice(fields: &mut Fields, outcome: Outcome) {
    let alternate = outcome == Outcome::Alternate;
    if crate::mask_trash_prompt::owns(fields) && !crate::mask_trash_prompt::is_vector(fields) {
        fields.insert("choice".into(), json!(if alternate { "delete" } else { "apply" }));
    } else if crate::rasterize_prompt::is_filter(fields) {
        fields.insert("choice".into(), json!(if alternate { "smartObject" } else { "rasterize" }));
    }
}

pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    crate::layer_style::color_picker::prune(app);
    crate::type_panels_ui::color_picker::prune(app);
    let dialogs = app.ui.dialogs.clone();
    app.dialog_chrome.retain(|id, _| dialogs.iter().any(|d| d.id == *id));
    // The open dialogs in the order they were opened: only the last takes input, and it is drawn
    // above the others.
    let mut stack = DialogStack::default();
    for d in &dialogs {
        stack.push(dialog_id(d.id), None);
    }
    let mut shown = Vec::new();
    for d in dialogs {
        let lang = if crate::prefs_ui::is_preferences(&d.fields) {
            crate::i18n::Lang::from_pref(d.fields.get("values").and_then(|v| v.pointer("/interface/language")).and_then(Value::as_str).unwrap_or("auto"))
        } else {
            crate::i18n::current()
        };
        let _language = crate::i18n::language_scope(lang);
        let interactive = stack.is_top(dialog_id(d.id));
        let title = display_title(&d);
        if d.kind == DialogKind::Error {
            let message = d.fields.get("message").and_then(Value::as_str).unwrap_or("Error");
            if Alert::new(d.id, title, message).ok_label(tl!("OK")).stack(&stack).show(ctx) {
                let _ = cancel(app, d.id);
            }
            continue;
        }
        let id = egui::Id::unique(("dialog", d.id));
        let mut chrome = app.dialog_chrome.remove(&d.id).unwrap_or_default();
        let mut fields = Settings(d.fields.clone());
        let color_picker = crate::color_picker_ui::owns(&fields);
        // Tab / ⇧Tab walk the topmost dialog's text fields, not every widget (field_tab.rs).
        let enabled = Cell::new(enabled_for(app, &d, &fields));
        let tab = if interactive && !egui::Popup::is_any_open(ctx) { crate::field_tab::take_step(ctx) } else { 0 };
        let mut dialog = HostDialog::new(d.id, title)
            .stack(&stack)
            .place_key(PlaceKey(place_id(&d)))
            .footer(footer_for(&d, &enabled))
            .history(d.kind == DialogKind::Command)
            .close_label(CLOSE_BOX.then(|| tl!("Close").to_string()))
            .roomy(color_picker)
            // The dialogs that take the keyboard themselves open with their focus where they put it.
            .initial_focus(InitialFocus::None);
        if let Some(offset) = crate::blur_pins::first_place(&d.fields, ctx) {
            dialog = dialog.first_place(move |_| Some(offset));
        }
        dialog = match resize_of(ctx, &fields) {
            Some((resize, remember)) => dialog.resizable(resize).remember_size(remember),
            None => dialog.width(width_of(&d)),
        };

        if d.kind == DialogKind::Command && is_filter_route(&fields) {
            dialog = dialog.body(Body::Scroll);
        }
        let body_outcome = Cell::new(None::<bool>);
        let private = RefCell::new(Fields::new());
        // The body and the region under it both need the app; they run one after the other.
        let app_cell = RefCell::new(&mut *app);
        let response = dialog.show_with_below(
            ctx,
            &mut chrome,
            &mut fields,
            |ui, fields| {
                let mut app = app_cell.borrow_mut();
                crate::field_tab::begin(ui.ctx());
                if let Some(done) = dialog_body(&mut app, ui, &d, fields, id, interactive) {
                    body_outcome.set(Some(done));
                }
                crate::field_tab::end(ui.ctx(), tab);
                if let Some(Value::String(key)) = fields.remove(BROWSE_FIELD) {
                    browse_field(&mut app, d.id, fields, &key);
                }
                // The Color Table dialog loads and saves `.act` files, as in Photoshop.
                if d.kind == DialogKind::Command && crate::color_table_ui::owns(fields) {
                    crate::color_table_ui::buttons(&mut app, ui, d.id, fields);
                }
                enabled.set(enabled_for(&app, &d, fields));
                *private.borrow_mut() = fields.iter().filter(|(k, _)| k.starts_with("__")).map(|(k, v)| (k.clone(), v.clone())).collect();
            },
            |ui, fields| filter_below(&app_cell.borrow(), &d, ui, fields),
        );
        drop(app_cell);
        shown.push(response.rect);

        // ⌘Z / ⇧⌘Z stepped the settings back: the private keys are not part of the history.
        if response.history.is_some() {
            fields.retain(|k, _| !k.starts_with("__"));
            fields.extend(private.take());
        }
        let mut done = match response.outcome {
            Some(Outcome::Ok) if matches!(d.kind, DialogKind::About) => Some(false),
            Some(Outcome::Ok | Outcome::Alternate) => {
                set_choice(&mut fields, response.outcome.unwrap_or(Outcome::Ok));
                Some(true)
            }
            Some(Outcome::Cancel | Outcome::Close) => Some(false),
            _ => body_outcome.get(),
        };
        // Enter confirms the Color Picker, which draws no button row.
        if done.is_none() && color_picker && response.active && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            done = Some(true);
        }
        if response.outcome == Some(Outcome::Reset) {
            crate::dialog_alt_reset::apply(app, ctx, &mut fields);
        }
        if let Some(dm) = app.ui.dialog_mut(d.id) {
            dm.fields = fields.0;
        }
        // A colour swatch (Edit › Fill, Edit › Stroke) opens the Color Picker over the dialog.
        if done.is_none() {
            crate::color_picker_ui::open_requested(app, d.id);
        }
        if response.outcome == Some(Outcome::Apply) && done.is_none() {
            let _ = crate::prefs_ui::apply(app, d.id);
        }
        match done {
            Some(true) => {
                if let Err(error) = confirm(app, d.id) {
                    app.ui.status = error;
                }
            }
            Some(false) => {
                let _ = cancel(app, d.id);
            }
            None => {
                app.dialog_chrome.insert(d.id, chrome);
                if let Err(error) = crate::layer_style::color_picker::take_request(app, d.id) {
                    app.ui.status = error;
                }
            }
        }
    }
    for rect in shown {
        craft_ui::title_bar::ModalRects::push(ctx, ctx.viewport_id(), rect);
    }
}

/// The About window's tabs, as stored in its `tab` field.
pub const ABOUT_TABS: [&str; 3] = ["about", "contributors", "models"];

/// The About tab to show: the `tab` field when it names one, otherwise "about".
fn about_tab(fields: &serde_json::Map<String, Value>) -> &'static str {
    let want = fields.get("tab").and_then(Value::as_str).unwrap_or("");
    ABOUT_TABS.iter().copied().find(|t| *t == want).unwrap_or("about")
}

/// One About tab's contents: the credits lists, or the product blurb and links.
fn about_tab_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, tab: &str) {
    match tab {
        "contributors" => crate::credits::contributors_ui(app, ui),
        "models" => crate::credits::models_ui(ui),
        _ => {
            ui.label(tl!("PhotoCraft — an open-source, native image editor written in Rust."));
            ui.label(crate::i18n::fmt(tl!("Version {version}"), &[("version", &photocraft_engine::build_info::long_version())]));
            ui.add_space(craft_ui::theme::scale::Space::S12.px());
            ui.vertical_centered(|ui| {
                crate::links::discord_button(app, ui, 220.0);
                ui.add_space(craft_ui::theme::scale::Space::S8.px());
                crate::links::link_row(app, ui);
            });
            ui.add_space(craft_ui::theme::scale::Space::S12.px());
            ui.weak("egui · wgpu · photocraft-engine");
        }
    }
}

/// The dialog title as shown: [`title`] in the UI language.
fn display_title(d: &Dialog) -> String {
    if crate::smart_filter_dialog::owns(&d.fields) {
        return crate::smart_filter_dialog::title(&d.fields);
    }
    match d.kind {
        DialogKind::Command => {
            let label = d.fields.get("__label").and_then(Value::as_str).unwrap_or("Command");
            // Dialog labels are catalogued with their "…" ("Export As…"); some commands omit it.
            let with_dots = format!("{}…", label.trim_end_matches('…'));
            let shown = if crate::i18n::has(crate::i18n::current(), label) { tl!(label) } else { tl!(&with_dots) };
            shown.trim_end_matches('…').to_string()
        }
        _ => tl!(&title(d)).to_string(),
    }
}

pub fn title(d: &Dialog) -> String {
    match d.kind {
        DialogKind::NewDocument => "New Document".into(),
        DialogKind::About if d.fields.get("systemInfo").and_then(Value::as_bool) == Some(true) => "System Info".into(),
        DialogKind::About => "About PhotoCraft".into(),
        DialogKind::LayerStyle => "Layer Style".into(),
        DialogKind::Command => d.fields.get("__label").and_then(Value::as_str).unwrap_or("Command").trim_end_matches('…').to_string(),
        DialogKind::Error => "Error".into(),
    }
}

type Fields = serde_json::Map<String, Value>;

/// The markers [`confirm`] and [`cancel`] choose a dialog's handler by: keys the code opening a
/// dialog puts in its fields. Keep in step with the arms of `confirm`.
const ROUTES: &[fn(&Fields) -> bool] = &[
    crate::guide_layout_ui::owns,
    crate::brush_preset_dialog::owns,
    crate::fill_ui::owns,
    crate::pattern_fill_ui::owns,
    crate::stroke_ui::owns,
    crate::channel_options_ui::owns,
    crate::channel_options_ui::owns_new_spot,
    crate::shape_dialog::owns,
    crate::delete_layer_prompt::owns,
    crate::disk_change_prompt::owns,
    crate::actions::owns_insert_stop,
    crate::new_layer_ui::owns,
    crate::new_group_ui::owns,
    crate::mask_trash_prompt::owns,
    crate::rasterize_prompt::owns,
    crate::duplicate_layer_ui::owns,
    crate::stack_dialog::owns,
    crate::tool_reset::owns,
    crate::crop_presets::owns,
    crate::variables_ui::owns,
    crate::file_ui::owns,
    crate::new_guide_ui::owns,
    crate::color_picker_ui::owns,
    crate::color_range_ui::owns,
    crate::prefs_ui::owns,
    crate::open_prompt::owns,
    crate::replace_prompt::owns,
    crate::layer_style::color_picker::owns,
    |f| f.contains_key("__export"),
    |f| f.contains_key("__smartFilter"),
    |f| f.contains_key(crate::actions::RESUME),
];

/// Whether `key` is a marker that picks a dialog's handler. Automation may change the values of
/// a dialog's fields, private ones included, but never add a marker: a dialog it opened would
/// otherwise confirm through another dialog's handler, one its command was never judged for.
pub fn is_route_marker(key: &str) -> bool {
    let probe = Fields::from_iter([(key.to_string(), Value::Bool(true))]);
    ROUTES.iter().any(|owns| owns(&probe))
}

/// The file command a dialog's handler stands for when it reads or writes files itself, outside
/// the engine (a disk-change save, Export As, Save for Web, Print), so automation is judged for
/// that command rather than for the dialog's `__command`.
fn file_command(fields: &Fields) -> Option<&'static str> {
    if crate::disk_change_prompt::owns(fields) {
        Some("file.save")
    } else if fields.contains_key("__export") {
        Some("file.export.exportAs")
    } else if fields.contains_key("__web") {
        Some("file.export.saveForWebLegacy")
    } else if fields.contains_key("__print") {
        Some("file.print")
    } else {
        None
    }
}

/// Confirm a command dialog no specific handler owns: run its `__command` with the dialog's
/// parameters, then settle the filter preview and the playing action.
fn confirm_generic_command(app: &mut PhotocraftApp, d: Dialog) -> Result<Value, String> {
    let Some(cmd) = d.fields.get("__command").and_then(Value::as_str).map(str::to_string) else {
        app.filter_preview = None;
        crate::actions::abandoned(app, &d.fields);
        return Err("dialog has no command".into());
    };
    if crate::filter_dialog::needs_displace_map(&cmd, &crate::filter_dialog::params_of(&d.fields)) {
        app.filter_preview = None;
        let result = crate::filter_dialog::ask_displace_map(app, cmd, d.fields.clone());
        if result.is_err() {
            crate::actions::abandoned(app, &d.fields);
        } else {
            // Cancel and an unavailable picker finish before the Displace callback
            // runs. Observe the whole file-dialog result, including those exits.
            app.after_file_dialog(move |app, result| {
                if result.is_err() {
                    crate::actions::abandoned(app, &d.fields);
                }
                result
            });
        }
        return result;
    }
    let (cmd, params) = match crate::smart_ui::confirm_command(app, &d.fields, cmd, crate::filter_dialog::params_of(&d.fields)) {
        Ok(edit) => edit,
        Err(e) => {
            app.filter_preview = None;
            return Err(e);
        }
    };
    let result = app.run(&cmd, params.clone());
    // A filter that runs as a background job keeps its preview on screen until the job
    // lands (`canvas::committed_filter_preview`) instead of flashing the unfiltered image.
    let running =
        app.session.active().is_some_and(|st| app.filter_preview.as_ref().is_some_and(|p| p.key.doc == st.doc.id) && app.session.job_on(st.doc.id).is_some());
    match app.filter_preview.as_mut() {
        Some(p) if result.is_ok() && running => p.committing = true,
        _ => app.filter_preview = None,
    }
    if result.is_ok()
        && cmd == "path.toSelection"
        && d.fields.contains_key("__deselectPath")
        && let Some(name) = params.get("name").and_then(Value::as_str)
    {
        crate::vector_ui::deselect_loaded_path(app, name);
    }
    if result.is_ok() && cmd == "view.newGuideLayout" {
        app.ui.view.guide_layout = params;
    }
    if result.is_ok() {
        if crate::adjust_dialog::owns(&d.fields) {
            crate::adjust_dialog::remember(app, &d.fields);
        } else {
            crate::filter_dialog::remember(app, &cmd, &d.fields);
        }
        // A step dialog of a playing action: the action continues after it.
        crate::actions::resume(app, &d.fields)?;
    } else {
        crate::actions::abandoned(app, &d.fields);
    }
    result
}

/// The checks before a dialog is confirmed: no open child picker, the automation gate for a handler
/// that touches files itself, and a valid guide layout.
fn confirm_preflight(app: &mut PhotocraftApp, id: u64) -> Result<(), String> {
    if crate::layer_style::color_picker::has_child(app, id) {
        return Err("confirm or cancel the Layer Style color picker first".into());
    }
    // A control request installs the session gate; synthetic input sets `automation_input`.
    // Either way the handler's own file access is judged before it runs, as its command would be.
    if (app.automation_input || app.session.authorize.is_some())
        && let Some(d) = app.ui.dialogs.iter().find(|d| d.id == id)
        && let Some(command) = file_command(&d.fields)
        && let Some(authorize) = app.services.automation_command.as_ref()
    {
        authorize(command, &Value::Object(d.fields.clone()))?;
    }
    if let Some(d) = app.ui.dialogs.iter().find(|d| d.id == id).filter(|d| crate::guide_layout_ui::owns(&d.fields)) {
        if app.ui.dialogs.iter().any(|child| crate::color_picker_ui::owns(&child.fields) && child.fields.get("__dialog").and_then(Value::as_u64) == Some(id)) {
            return Err("confirm or cancel the guide color picker first".into());
        }
        crate::guide_layout_ui::validate(app, &d.fields)?;
    }
    Ok(())
}

/// Confirm a dialog: run its action and close it. Used by the OK button and by automation.
/// Confirm a command dialog through the first handler whose marker its fields carry; the prompts
/// and small editors are here, the rest in `confirm_command_more`.
fn confirm_command(app: &mut PhotocraftApp, d: Dialog) -> Result<Value, String> {
    match d.kind {
        _ if crate::open_prompt::owns_damage(&d.fields) => crate::open_prompt::answer_damage(app, true),
        _ if crate::guide_layout_ui::owns(&d.fields) => crate::guide_layout_ui::confirm(app, &d.fields),
        _ if crate::brush_preset_dialog::owns(&d.fields) => crate::brush_preset_dialog::confirm(app, &d.fields),
        _ if crate::fill_ui::owns(&d.fields) => crate::fill_ui::confirm(app, &d.fields),
        _ if crate::pattern_fill_ui::owns(&d.fields) => crate::pattern_fill_ui::confirm(app, &d.fields),
        _ if crate::stroke_ui::owns(&d.fields) => crate::stroke_ui::confirm(app, &d.fields),
        _ if crate::new_group_ui::owns(&d.fields) => crate::new_group_ui::confirm(app, &d.fields),
        _ if crate::snapshot_dialog::owns(&d.fields) => crate::snapshot_dialog::confirm(app, &d.fields),
        _ if crate::channel_options_ui::owns(&d.fields) => crate::channel_options_ui::confirm(app, &d.fields),
        _ if crate::channel_options_ui::owns_new_spot(&d.fields) => crate::channel_options_ui::confirm_new_spot(app, &d.fields),
        _ if crate::shape_dialog::owns(&d.fields) => crate::shape_dialog::confirm(app, &d.fields),
        _ if crate::delete_layer_prompt::owns(&d.fields) => crate::delete_layer_prompt::confirm(app, &d.fields),
        _ if crate::disk_change_prompt::owns(&d.fields) => crate::disk_change_prompt::confirm(app, &d.fields),
        _ if crate::replace_prompt::owns(&d.fields) => crate::replace_prompt::confirm(app),
        _ if crate::actions::owns_insert_stop(&d.fields) => crate::actions::insert_stop_confirm(app, &d.fields),
        _ if crate::new_layer_ui::owns(&d.fields) => crate::new_layer_ui::confirm(app, &d.fields),
        _ if crate::mask_trash_prompt::owns(&d.fields) => crate::mask_trash_prompt::confirm(app, &d.fields),
        _ if crate::rasterize_prompt::owns(&d.fields) => crate::rasterize_prompt::confirm(app, &d.fields),
        _ if crate::duplicate_layer_ui::owns(&d.fields) => crate::duplicate_layer_ui::confirm(app, &d.fields),
        _ if crate::stack_dialog::owns(&d.fields) => crate::stack_dialog::confirm(app, &d.fields),
        _ if crate::tool_reset::owns(&d.fields) => crate::tool_reset::confirm(app),
        _ if crate::crop_presets::owns(&d.fields) => crate::crop_presets::confirm(app, &d.fields),
        _ if crate::variables_ui::owns(&d.fields) => crate::variables_ui::confirm(app, &d.fields),
        _ => confirm_command_more(app, d),
    }
}

/// The rest of the command dialogs: files, colour, preferences and export, then any other command.
fn confirm_command_more(app: &mut PhotocraftApp, d: Dialog) -> Result<Value, String> {
    match d.kind {
        _ if crate::file_ui::owns(&d.fields) => crate::file_ui::confirm(app, &d.fields),
        _ if crate::new_guide_ui::owns(&d.fields) => crate::new_guide_ui::confirm(app, &d.fields),
        _ if crate::color_picker_ui::owns(&d.fields) => crate::color_picker_ui::confirm(app, &d.fields),
        _ if crate::color_range_ui::owns(&d.fields) => crate::color_range_ui::confirm(app, &d.fields),
        _ if crate::prefs_ui::owns(&d.fields) => crate::prefs_ui::confirm(app, &d.fields),
        _ if d.fields.contains_key("__export") => crate::export_dialog::confirm(app, &d.fields),
        _ => confirm_generic_command(app, d),
    }
}

pub fn confirm(app: &mut PhotocraftApp, id: u64) -> Result<Value, String> {
    confirm_preflight(app, id)?;
    let d = app.ui.close_dialog(id).ok_or_else(|| format!("no dialog {id}"))?;
    match d.kind {
        DialogKind::NewDocument => {
            let r = app.run("file.new", crate::new_doc_ui::command_params(&d.fields));
            if let Some(i) = app.session.active_index() {
                app.ui.views[i].fit_pending = true;
            }
            if r.is_ok() {
                crate::remember_new_document(app, &d.fields);
                // File › New › Recent. Best effort: the document exists either way.
                let _ = app.run("document.recent.add", json!({"settings": crate::new_doc_ui::document_settings(&d.fields)}));
            }
            r
        }
        DialogKind::Command => confirm_command(app, d),
        DialogKind::LayerStyle => crate::layer_style::confirm(app, &d.fields),
        DialogKind::About | DialogKind::Error => Ok(Value::Null),
    }
}

/// Cancel a dialog and its dependent Layer Style picker, without applying edits.
pub fn cancel(app: &mut PhotocraftApp, id: u64) -> Result<Value, String> {
    let closed = app.ui.close_dialog(id).ok_or_else(|| format!("no dialog {id}"))?;
    crate::actions::stopped(app, &closed.fields);
    if crate::guide_layout_ui::owns(&closed.fields) {
        app.ui.dialogs.retain(|child| child.fields.get("__dialog").and_then(Value::as_u64) != Some(id));
    }
    // Cancel on a profile prompt opens neither its file nor the ones still to come.
    if crate::open_prompt::owns_damage(&closed.fields) {
        crate::open_prompt::answer_damage(app, false)?;
    } else if crate::open_prompt::owns(&closed.fields) {
        crate::open_prompt::answer(app, None)?;
    }
    // Cancel on the question whether to replace a file writes nothing.
    if crate::replace_prompt::owns(&closed.fields) {
        crate::replace_prompt::cancel(app)?;
    }
    app.ui.dialogs.retain(|d| !crate::layer_style::color_picker::child_of(&d.fields, id));
    app.filter_preview = None;
    app.color_range = None;
    crate::type_panels_ui::color_picker::prune(app);
    Ok(Value::Null)
}

/// Open the parameter dialog of `command`: the adjustment editor for `image.adjustments.*`, the
/// schema dialog for filters, the Color Range dialog for `select.colorRange`, otherwise a bare
/// confirm dialog.
pub fn open_command_dialog(app: &mut PhotocraftApp, command: &str, label: &str) -> u64 {
    if command == "view.newGuideLayout" {
        return crate::guide_layout_ui::open(app);
    }
    if command == crate::stroke_ui::COMMAND {
        return crate::stroke_ui::open(app);
    }
    if crate::new_group_ui::fronts(command) {
        return crate::new_group_ui::open(app, command);
    }
    if command == crate::color_range_ui::COMMAND {
        return crate::color_range_ui::open(app);
    }
    if let Some(id) = crate::adjust_dialog::open(app, command) {
        return id;
    }
    if crate::filter_dialog::has_dialog(command)
        && let Some(id) = crate::filter_dialog::open(app, command)
    {
        return id;
    }
    let mut fields = serde_json::Map::new();
    fields.insert("__command".into(), json!(command));
    fields.insert("__label".into(), json!(label));
    app.ui.open_dialog(DialogKind::Command, fields)
}

pub mod adjust_dialog;
pub mod adjust_editors;
pub mod adjust_preview;
pub(crate) mod bitmap_dialog;
pub mod blur_pins;
pub mod brush_preset_dialog;
pub mod caf_ui;
pub(super) mod camera_raw_detail_ui;
pub(super) mod camera_raw_gpu;
pub(super) mod camera_raw_preview_ui;
pub(super) mod camera_raw_scope_ui;
pub mod camera_raw_ui;
pub mod channel_options_ui;
pub mod color_picker_ui;
pub mod color_range_ui;
pub mod color_table_ui;
pub mod credits;
pub mod dds_options_ui;
pub mod delete_layer_prompt;
pub mod dialog_alt_reset;
pub mod dialog_blend_ui;
pub mod discard_ui;
pub mod disk_change_prompt;
pub mod distort_ui;
pub mod duplicate_layer_ui;
pub mod edit_history;
pub mod export_dialog;
pub mod file_ui;
pub mod fill_ui;
pub mod filter_dialog;
pub(super) mod filter_preview_ui;
#[cfg(not(target_arch = "wasm32"))]
pub(super) mod filter_preview_worker;
pub mod gallery_ui;
pub mod guide_layout_ui;
pub mod history_options_ui;
pub mod jpeg_options_ui;
pub mod layer_picker;
pub mod layer_style;
pub mod liquify_ui;
pub mod mask_trash_prompt;
pub mod new_doc_ui;
pub mod new_group_ui;
pub mod new_guide_ui;
pub mod new_layer_ui;
pub mod open_prompt;
pub mod palette;
pub(crate) mod pattern_fill_ui;
pub mod rasterize_prompt;
pub mod replace_prompt;
pub mod screen_picker;
pub mod select_mask_ui;
pub mod shape_dialog;
pub(crate) mod shape_fill_ui;
pub(super) mod sizing;
pub mod smart_filter_dialog;
pub mod snapshot_dialog;
pub(crate) mod solid_fill_ui;
pub mod stack_dialog;
pub mod stroke_ui;
pub mod targa_options_ui;
#[cfg(test)]
mod tests;
pub mod tiff_options_ui;
pub mod vp_ui;
pub mod wide_angle_ui;

#[cfg(test)]
mod preference_resize_tests {
    use super::*;

    #[test]
    fn preferences_can_resize_while_standard_native_dialogs_keep_their_layout() {
        let ctx = egui::Context::default();
        let prefs = json!({"__prefsui": "prefs"});
        let shortcuts = json!({"__prefsui": "shortcuts"});
        let ordinary = json!({});
        let web = json!({"__web": true});
        let size = |fields: &serde_json::Value| resize_of(&ctx, fields.as_object().unwrap()).map(|(resize, _)| resize.default);

        assert_eq!(size(&prefs), Some(crate::prefs_ui::PREFERENCES_SIZE));
        assert_eq!(size(&shortcuts), None);
        assert_eq!(size(&ordinary), None);
        assert_eq!(size(&web), Some(egui::vec2(960.0, 900.0)));
    }

    /// Drags the dialog's resize grip (its frame's bottom-right corner) by `by`.
    fn drag_corner(h: &mut egui_kittest::Harness<'_, PhotocraftApp>, by: egui::Vec2) {
        let corner = frame(h).max - egui::vec2(14.0, 14.0);
        h.event(egui::Event::PointerMoved(corner));
        h.step();
        h.event(egui::Event::PointerButton { pos: corner, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.step();
        h.event(egui::Event::PointerMoved(corner + by));
        h.step();
        h.event(egui::Event::PointerButton { pos: corner + by, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(4);
    }

    fn frame(h: &egui_kittest::Harness<'_, PhotocraftApp>) -> egui::Rect {
        rects(h)[0]
    }

    /// The button row sits in the frame's bottom-right corner (its order follows the platform):
    /// the contents fill the dialog, no empty space.
    fn assert_buttons_in_corner(h: &egui_kittest::Harness<'_, PhotocraftApp>, when: &str) {
        use egui_kittest::kittest::Queryable;
        let f = frame(h);
        let row = ["OK", "Cancel", "Apply"].map(|l| h.get_by_label(l).rect()).into_iter().reduce(|a, b| a.union(b)).unwrap();
        // Inside the frame (not past its edge) and near it.
        let near = |gap: f32| (0.0..24.0).contains(&gap);
        assert!(near(f.bottom() - row.bottom()) && near(f.right() - row.right()), "{when}: frame {f:?}, buttons {row:?}");
    }

    #[test]
    fn preferences_contents_follow_the_dialog_when_it_is_resized() {
        // #7420: the dialog resized but its contents kept a size from the window, leaving the
        // buttons part-way up a dialog that opened, or was dragged, larger than them.
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        crate::prefs_ui::open_preferences(&mut app, "interface");
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).build_ui_state(
            |ui, app| {
                if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("semibold".into()))) {
                    show(app, ui.ctx());
                }
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, Default::default());
        h.run_steps(5);
        assert_buttons_in_corner(&h, "on open");
        let opened = frame(&h);
        drag_corner(&mut h, egui::vec2(220.0, 80.0));
        assert!(frame(&h).width() > opened.width() + 150.0 && frame(&h).height() > opened.height() + 50.0, "wider: {:?}", frame(&h));
        assert_buttons_in_corner(&h, "wider");
        drag_corner(&mut h, egui::vec2(-500.0, -300.0));
        let smaller = frame(&h);
        assert!(smaller.width() < opened.width() - 200.0 && smaller.height() < opened.height() - 150.0, "smaller: {smaller:?}");
        assert_buttons_in_corner(&h, "smaller");
        // The contents never ask for more than the dialog gives them, so it keeps its size.
        h.run_steps(10);
        assert_eq!(frame(&h), smaller);
    }

    /// #8199: no outline is drawn just around the contents, which left the title flush against a
    /// second, inner box instead of the frame's margin like the other dialogs.
    #[test]
    fn preferences_has_no_inner_outline_hugging_its_title() {
        fn find<'a>(shape: &'a egui::Shape, out: &mut Vec<&'a egui::Shape>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| find(s, out)),
                s => out.push(s),
            }
        }
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        crate::prefs_ui::open_preferences(&mut app, "general");
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).build_ui_state(
            |ui, app| {
                if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("semibold".into()))) {
                    show(app, ui.ctx());
                }
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, Default::default());
        h.run_steps(5);
        let mut shapes = Vec::new();
        h.output().shapes.iter().for_each(|s| find(&s.shape, &mut shapes));
        let title = shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Text(t) if t.galley.job.text == "Preferences" => Some(t.pos),
                _ => None,
            })
            .expect("the title is painted");
        let hugging: Vec<egui::Rect> = shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Rect(r) if r.stroke.width > 0.0 && r.rect.contains(title) && (0.0..4.0).contains(&(title.x - r.rect.left())) => Some(r.rect),
                _ => None,
            })
            .collect();
        assert!(hugging.is_empty(), "an outline within 4 points of the title at {title:?}: {hugging:?}");
    }
}
