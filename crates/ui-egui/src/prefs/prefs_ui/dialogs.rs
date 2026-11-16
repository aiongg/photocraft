//! The Edit-menu front end (`invoke`) and the app-held Preferences window.
//!
//! The window is a [`SettingsDocument`] over the engine's preferences, shown by `PreferencesWindow` once per pass
//! while `app.prefs_rt.window` holds a session. Apply and OK leave the keys they changed in the document;
//! `show` writes just those through `prefs.set` and `prefs.reset`, so a value changed elsewhere while the window is
//! open survives.

use std::sync::Arc;

use craft::host::Host;
use craft::i18n::Lang;
use craft::ui::contract::{Bind, Props as _};
use craft::ui::dialog::DialogId;
use craft::ui::patterns::preferences::{PageInfo, PrefValue, PreferencesModel, PreferencesState, PreferencesWindow, RowKind, SettingsDocument, StandardPage};
use craft::ui::runtime::memory::SaveKey;
use photocraft_engine::prefs::SECTIONS;
use serde_json::{Value, json};

use super::custom_rows::PrefsCustom;
use super::fields::{PrefPages, rules};
use crate::PhotocraftApp;

/// The document the window edits between passes. The custom rows borrow the app, so each pass attaches `PrefsCustom`
/// for its own duration and takes it off again before the app is used mutably.
type Doc = SettingsDocument<PrefPages, ()>;

/// The open Preferences window: its document, the state the window keeps between passes, and what `show` tracks.
pub(crate) struct PrefsSession {
    /// The saved values, the draft and the rows' rules.
    pub(super) doc: Doc,
    /// The page, search text and Apply error the window keeps.
    pub(super) state: PreferencesState,
    /// The engine's preferences revision the document was last based on.
    pub(super) saved_rev: u64,
    /// The colour picker this window opened, until it answers or closes.
    pub(super) picker: Option<DialogId>,
    /// The language the document's labels are in: the draft's language, which previews before Apply.
    pub(super) labels: Lang,
}

// ------------------------------------------------------------------ menu routing

/// Shell front ends for Edit-menu commands invoked without parameters (dialogs, pickers).
/// `None` when `id` isn't handled here.
pub fn invoke(app: &mut PhotocraftApp, ctx: &egui::Context, id: &str, params: &Value) -> Option<Result<Value, String>> {
    let empty = params.as_object().is_none_or(|o| o.is_empty());
    if !empty {
        return None;
    }
    if let Some(section) = id.strip_prefix("edit.preferences.") {
        open_preferences(app, section);
        return Some(Ok(json!({"dialog": "preferences"})));
    }
    let dialog = |d: Result<DialogId, String>| Some(d.map(|d| json!({"dialog": d.value().to_string()})));
    match id {
        "edit.keyboardShortcuts" => {
            super::shortcuts::open(app, 0);
            Some(Ok(json!({"dialog": "shortcuts"})))
        }
        "edit.menus" => {
            super::shortcuts::open(app, 1);
            Some(Ok(json!({"dialog": "shortcuts"})))
        }
        "edit.toolbar" => {
            crate::panels::toolbar::open_drawer(app);
            Some(Ok(json!({})))
        }
        "edit.colorSettings" => {
            // What Monitor Profile resolves to, so a fallback to sRGB is visible (#569): read the displays again now. The
            // dialog draws the line from the note's cell, which `read_now` fills before the dialog's first frame.
            crate::monitor_status::read_now(app);
            dialog(crate::filter_dialog::open(app, "edit.colorSettings"))
        }
        "edit.fade" | "edit.findAndReplaceText" | "edit.defineBrushPreset" | "edit.defineCustomShape" | "edit.autoAlignLayers" | "edit.autoBlendLayers" => {
            if !app.session.is_enabled(id) {
                return Some(Err(photocraft_engine::commands::find(id)
                    .map_or("not available".into(), |c| format!("{} is not available right now", c.label.trim_end_matches('…')))));
            }
            dialog(crate::filter_dialog::open(app, id))
        }
        // Photoshop's Content-Aware Fill workspace (caf_ui); the engine command with params
        // still runs directly.
        "edit.contentAwareFill" => Some(crate::caf_ui::open(app).map(|_| json!({"contentAwareFill": crate::caf_ui::describe(app)}))),
        // Photoshop has no dialog: the menu opens the scale box (with params: the engine).
        crate::cas_transform::COMMAND => Some(crate::cas_transform::begin(app, ctx)),
        "edit.presets.presetManager" => Some(Ok(json!({"dialog": super::presets::open_manager(app).value().to_string()}))),
        "edit.presets.exportImportPresets" => Some(Ok(json!({"dialog": super::presets::open_io(app).value().to_string()}))),
        _ => None,
    }
}

// ------------------------------------------------------------------ the window

/// Whether the Preferences window is open.
pub fn window_open(app: &PhotocraftApp) -> bool {
    app.prefs_rt.window.is_some()
}

/// Opens Edit › Preferences on `section` (`general` for an unknown one). With the window open only its page changes.
pub fn open_preferences(app: &mut PhotocraftApp, section: &str) {
    let section = if SECTIONS.iter().any(|(id, _)| *id == section) { section } else { "general" };
    if let Some(open) = app.prefs_rt.window.as_mut() {
        open.state.selected = section.to_string();
        return;
    }
    let doc = match SettingsDocument::new(PrefPages::of(app.session.prefs())) {
        Ok(doc) => doc,
        Err(error) => {
            app.ui.status = error.to_string();
            return;
        }
    };
    let pages: Vec<PageInfo> = StandardPage::ALL.into_iter().map(StandardPage::info).collect();
    let mut doc = doc.pages(pages).rules(rules());
    let host = Arc::clone(app.framework.host());
    doc.relabel(&host);
    let mut state = PreferencesState::default();
    state.selected = section.to_string();
    app.prefs_rt.window = Some(PrefsSession { doc, state, saved_rev: app.session.prefs.rev(), picker: None, labels: host.lang() });
}

/// The language the draft's `interface.language` names, as the code stored in the draft.
fn draft_language(doc: &Doc) -> Option<String> {
    doc.pages().iter().filter(|page| page.id == "interface").flat_map(|page| page.groups.iter()).flat_map(|group| group.rows.iter()).find_map(|row| match &row
        .kind
    {
        RowKind::Choice { value, .. } if row.key == "interface.language" => Some((*value).to_string()),
        _ => None,
    })
}

/// The language `code` stands for, `host`'s own where it names none.
fn language_of(host: &Host, code: &str) -> Lang {
    let Some(i18n) = host.i18n_shared() else { return host.lang() };
    if code == "auto" { i18n.system_lang() } else { i18n.from_code(code).unwrap_or_else(|| host.lang()) }
}

/// Writes the keys Apply or OK changed through the engine, and returns the `prefs.set` reply. Only these keys go,
/// so a setting changed elsewhere while the window is open is not overwritten. Keys Reset Section removed go through
/// `prefs.reset`.
fn write_edits(app: &mut PhotocraftApp, doc: &mut Doc) -> Result<Value, String> {
    let Some(edits) = doc.take_applied_edits() else { return Ok(json!({})) };
    let (values, resets) = PrefPages::edits_to_values(edits);
    let mut reply = json!({});
    if values.as_object().is_some_and(|map| !map.is_empty()) {
        reply = app.run("prefs.set", json!({"values": values}))?;
    }
    for path in resets {
        app.run("prefs.reset", json!({"path": path}))?;
    }
    Ok(reply)
}

/// Shows the Preferences window for this pass while it is open.
///
/// A pass applies only its own output: `pick_color`, `closed`, the colour picker's answer and the applied edits
/// are each taken once, so a second pass in the same frame, which gets no input, does none of it again.
pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let Some(PrefsSession { mut doc, mut state, mut saved_rev, mut picker, mut labels }) = app.prefs_rt.window.take() else { return };
    // Saved values changed outside the window: unedited rows follow them. `Err` keeps the document as it was.
    let rev = app.session.prefs.rev();
    if rev != saved_rev {
        let _ = doc.rebase(PrefPages::of(app.session.prefs()));
        saved_rev = rev;
    }
    // The draft's language previews inside the window; the main window keeps the committed one.
    let host = Arc::clone(app.framework.host());
    let wanted = draft_language(&doc).map_or_else(|| host.lang(), |code| language_of(&host, &code));
    if wanted != labels {
        // PLAN: the window's own words (OK, Cancel) follow the scope's host; only the document's labels preview here.
        doc.relabel(&host.with_lang(wanted));
        labels = wanted;
    }
    // PLAN: SaveKey needs a crate prefix, so the key is `photocraft.preferences`.
    // The custom rows carry the dialog host's answer to "is the window's colour picker open", which OK and Apply refuse on.
    let picker_open = crate::color_picker_ui::window_picker_open(app);
    let mut with_custom = doc.custom(PrefsCustom::new(&*app, app.framework.suite(), picker_open));
    let out = PreferencesWindow::new(Bind::bound(&mut state)).remember(SaveKey::new("photocraft.preferences")).show(ui, &mut with_custom);
    let mut doc = with_custom.custom(());
    if let Some(request) = out.pick_color {
        picker = Some(crate::color_picker_ui::open_for_window(app, request));
    }
    if let Some((key, color)) = crate::color_picker_ui::answer_for_window(app) {
        doc.set_color(&key, color);
    }
    if let Err(error) = write_edits(app, &mut doc) {
        state.error = Some(error);
    }
    if out.closed.is_some() {
        if let Some(id) = picker {
            app.framework.dialogs.cancel_by_id(ui.ctx(), id);
        }
        return;
    }
    if picker.is_some() && !crate::color_picker_ui::window_picker_open(app) {
        picker = None;
    }
    app.prefs_rt.window = Some(PrefsSession { doc, state, saved_rev, picker, labels });
}

/// Applies the open window's draft as its Apply button does and keeps the window open. `Err` when it is closed, a
/// colour picker it opened is still open, or the engine refuses a value.
pub fn apply_window(app: &mut PhotocraftApp) -> Result<Value, String> {
    let Some(mut session) = app.prefs_rt.window.take() else { return Err("Preferences is not open".into()) };
    let picker_open = crate::color_picker_ui::window_picker_open(app);
    let mut with_custom = session.doc.custom(PrefsCustom::new(&*app, app.framework.suite(), picker_open));
    let applied = if with_custom.nested_open() { Err(tl!("Close the Color Picker first.").to_string()) } else { with_custom.apply() };
    session.doc = with_custom.custom(());
    let result = applied.and_then(|()| write_edits(app, &mut session.doc));
    session.state.error = result.as_ref().err().cloned();
    app.prefs_rt.window = Some(session);
    result
}

/// Sets a row of the open window's draft as the user editing it would (type and range checks included).
/// `false` when the window is closed.
#[cfg(test)]
pub(crate) fn set_draft(app: &mut PhotocraftApp, key: &str, value: PrefValue) -> bool {
    match app.prefs_rt.window.as_mut() {
        Some(session) => {
            session.doc.set(key, value);
            true
        }
        None => false,
    }
}
