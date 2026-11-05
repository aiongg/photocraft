//! Photoshop keyboard shortcut sets (`.kys`, Edit › Keyboard Shortcuts › Save Set) imported
//! into Edit › Keyboard Shortcuts (#1163): the dialog's Import Shortcuts… button, File › Open
//! and a drop on the window. Parsing and matching rows to commands are the engine's
//! ([`photocraft_engine::kys`], also behind `edit.keyboardShortcuts {"importKys": …}`); here
//! the candidates are the app's menus, in Photoshop's menu order, so ties go its way.
//!
//! The import fills the open dialog's overrides, so OK applies them like hand-made changes
//! (a shortcut moved to a command is taken from its old owner) and Cancel discards them.
//!
//! At first launch beside Photoshop, its live set is offered, never applied unasked: a library prompt
//! names the set, and Import applies it ([`offer_import`], [`take_offer_answer`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use craft::keymap::ImportOptions;
use craft::keymap::ui::{ImportOffer, OfferAnswer};
use craft::ui::dialog::{DialogResult, StartupStage};

use crate::PhotocraftApp;
pub use photocraft_engine::kys::{KysCommand, KysSet, MAX_BYTES, Plan, parse};

/// Match the rows to our commands: the menus' items first, then engine commands the menus list
/// under another label (Preferences › General… is the Settings… item).
pub fn plan(app: &PhotocraftApp, set: &KysSet) -> Plan {
    use photocraft_engine::kys::Candidate;
    let items = crate::menus::menu_items(app);
    let menu = items.iter().filter(|i| i.label != "---" && crate::menus::is_live(&i.id)).map(|i| Candidate {
        id: i.id.clone(),
        label: i.label.clone(),
        default: crate::shortcuts::default_shortcut(&i.id),
    });
    let specs = photocraft_engine::kys::command_candidates().into_iter().map(|c| Candidate { default: crate::shortcuts::default_shortcut(&c.id), ..c });
    photocraft_engine::kys::plan(menu.chain(specs), set)
}

/// The set's tool keys matched to our tools by name (Photoshop's names are ours): tool
/// shortcut id (`tools.select.move`) → letter, and the tools PhotoCraft doesn't have.
pub fn plan_tools(set: &KysSet) -> (BTreeMap<String, String>, Vec<String>) {
    photocraft_engine::kys::plan_tools(photocraft_engine::shell_commands::TOOLS.iter().map(|t| (t.0, t.1)), set)
}

/// Put the plan into the open Keyboard Shortcuts dialog (opening one when none is), as
/// overrides the OK button applies: a key equal to the command's default clears its override.
/// Returns the dialog id and the plan as JSON.
pub fn import_text(app: &mut PhotocraftApp, file_name: &str, text: &str) -> Result<Value, String> {
    let set = parse(text).map_err(|e| format!("{file_name}: {e}"))?;
    let plan = plan(app, &set);
    let (tool_keys, unknown_tools) = plan_tools(&set);
    let id = match app.ui.dialogs.iter().find(|d| d.fields.get("__prefsui").and_then(Value::as_str) == Some("shortcuts")) {
        Some(d) => d.id,
        None => crate::prefs_ui::open_shortcuts(app, 0),
    };
    let f = &mut app.ui.dialog_mut(id).ok_or("the Keyboard Shortcuts dialog is gone")?.fields;
    let mut overrides: BTreeMap<String, String> = f.get("overrides").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    for (cmd, sc) in &plan.set {
        let default = crate::shortcuts::default_shortcut(cmd).and_then(|d| photocraft_engine::prefs::normalize_shortcut(&d));
        if default.as_deref() == Some(sc.as_str()) {
            overrides.remove(cmd);
        } else {
            overrides.insert(cmd.clone(), sc.clone());
        }
    }
    // Tool keys (#5036): a letter other than the tool's own becomes its override; its own
    // letter clears one, so the tool keeps cycling with the others that share it.
    for tool in crate::state::Tool::ALL {
        let id = crate::tool_shortcuts::id(tool);
        let Some(key) = tool_keys.get(&id) else { continue };
        if crate::tool_shortcuts::default(tool).is_some_and(|d| d.eq_ignore_ascii_case(key)) {
            overrides.remove(&id);
        } else {
            overrides.insert(id, key.clone());
        }
    }
    // A row's other keys are the command's further keys (#3665): Copy's F3 beside ⌘C.
    let mut alternates: BTreeMap<String, Vec<String>> = f.get("alternates").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    for (cmd, keys) in &plan.alternate_keys {
        let main = plan.set.get(cmd).map(String::as_str);
        if photocraft_engine::prefs::Preferences::default().alternates(cmd, main) == *keys {
            alternates.remove(cmd);
        } else {
            alternates.insert(cmd.clone(), keys.clone());
        }
    }
    let shown = std::path::Path::new(file_name).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| file_name.to_string());
    let n = (plan.set.len() + tool_keys.len()).to_string();
    let mut message = crate::i18n::fmt(tl!("Imported {n} shortcuts from {file}. Click OK to keep them."), &[("n", &n), ("file", &shown)]);
    if !plan.unknown.is_empty() || !unknown_tools.is_empty() {
        let u = (plan.unknown.len() + unknown_tools.len()).to_string();
        message.push(' ');
        message.push_str(&crate::i18n::fmt(tl!("{n} commands are not in PhotoCraft."), &[("n", &u)]));
    }
    f.insert("tab".into(), json!(0));
    f.insert("overrides".into(), json!(overrides));
    f.insert("alternates".into(), json!(alternates));
    f.insert("selected".into(), json!(""));
    f.insert("capture".into(), json!(false));
    f.insert("message".into(), json!(message));
    f.insert("messageWarning".into(), json!(false));
    Ok(json!({
        "dialog": id,
        "name": set.name,
        "imported": plan.set.len(),
        "set": plan.set,
        "unknown": plan.unknown,
        "unreadable": plan.unreadable,
        "alternates": plan.alternates,
        "alternateKeys": plan.alternate_keys,
        "toolKeys": set.tool_keys,
        "tools": tool_keys,
        "unknownTools": unknown_tools,
        "windows": set.windows,
    }))
}

/// A `.kys` opened like a document (File › Open, a drop): its bytes are text.
pub fn open_bytes(app: &mut PhotocraftApp, name: &str, bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() > MAX_BYTES {
        return Err(format!("{name}: not a keyboard shortcut set (.kys): larger than 4 MiB"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| format!("{name}: not a keyboard shortcut set (.kys): not UTF-8"))?;
    import_text(app, name, text)
}

/// The dialog's Import Shortcuts… button: ask for the file, then fill the dialog.
pub fn import_dialog(app: &mut PhotocraftApp) -> Result<Value, String> {
    app.pick_file_bytes(|app, name, bytes| open_bytes(app, &name, &bytes))
}

/// Preferences › `dialogs` key that records the first-launch offer (its value names the source
/// and whether it was imported), so it is made once.
pub const IMPORTED_PREF: &str = "photoshopShortcuts";

/// A set found at first launch, waiting for the user's answer ([`take_offer_answer`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    /// Where the set was found (Photoshop's live `Keyboard Shortcuts.psp`).
    pub source: String,
    /// The set's name, else its file name.
    pub set: String,
    /// Shortcuts the import would change.
    pub changes: usize,
    #[serde(skip)]
    text: String,
}

fn remember(app: &mut PhotocraftApp, source: &str, imported: bool) {
    crate::host_settings::remember(app, IMPORTED_PREF, &json!({"source": source, "imported": imported}));
}

fn not_imported(app: &mut PhotocraftApp, e: &str) {
    app.ui.status = crate::i18n::fmt(tl!("The keyboard shortcut set was not imported: {error}"), &[("error", e)]);
    app.ui.status_error = true;
}

/// First launch on a machine with Photoshop: its live set ([`crate::Services::photoshop_shortcuts`])
/// is offered once, as a library [`ImportOffer`] queued at [`StartupStage::Offer`]. Someone who
/// already changed a shortcut keeps their own set and is not asked; a set that changes nothing
/// (Photoshop's defaults are ours too) is not offered either ([`ImportOffer::prepare`]). The
/// preference remembers each outcome, so a later launch never asks again.
pub fn offer_import(app: &mut PhotocraftApp) {
    use craft::keymap::ui::NoOffer;
    if crate::host_settings::recall_ref(app, IMPORTED_PREF).is_some() {
        return;
    }
    let Some((source, text)) = app.services.photoshop_shortcuts.as_mut().and_then(|f| f()) else { return };
    if let Err(e) = parse(&text) {
        remember(app, &source, false);
        return not_imported(app, &e);
    }
    let Ok(set) = craft::keymap::import::kys::read(&text) else { return remember(app, &source, false) };
    let km = app.keymap();
    let record = None;
    match ImportOffer::prepare(&source, &set, km.keymap(), &ImportOptions::default(), record) {
        Ok(offer) => {
            app.ui.kys_offer = Some(Offer { source, set: offer.set_name.clone(), changes: offer.changes, text });
            app.ui.host.enqueue(StartupStage::Offer, offer);
        }
        Err(NoOffer::Answered) => {}
        Err(_) => remember(app, &source, false),
    }
}

/// The offer's answer: Import applies the set and says so; either answer is remembered.
pub fn answer_offer(app: &mut PhotocraftApp, import: bool) {
    let Some(offer) = app.ui.kys_offer.take() else { return };
    remember(app, &offer.source, import);
    if !import {
        return;
    }
    let applied = import_text(app, &offer.source, &offer.text).and_then(|r| {
        let id = r["dialog"].as_u64().ok_or("no dialog")?;
        crate::dialogs::confirm(app, id)
    });
    if let Err(e) = applied {
        return not_imported(app, &e);
    }
    let changed = app.session.prefs().shortcuts.len();
    let id = crate::notices::post(
        app,
        "Keyboard shortcuts imported",
        vec!["Your keyboard shortcut set {set} is in use here: {n} shortcuts differ from the defaults. Edit › Keyboard Shortcuts shows them; Reset All to Defaults undoes this.".to_owned()],
        false,
        None,
    );
    if let Some(n) = app.ui.notices.iter_mut().find(|n| n.id == id) {
        n.args = vec![("set".to_owned(), offer.set), ("n".to_owned(), changed.to_string())];
    }
}

/// The library prompt's answer: Import and Don't Import (Esc) are recorded; a host close is not,
/// so the next launch asks again.
pub fn take_offer_answer(app: &mut PhotocraftApp) {
    match app.ui.host.take::<ImportOffer>() {
        Some(DialogResult::Ok(answer)) => answer_offer(app, answer == OfferAnswer::Import),
        Some(_) => app.ui.kys_offer = None,
        None => {}
    }
}

#[cfg(test)]
mod tests;
