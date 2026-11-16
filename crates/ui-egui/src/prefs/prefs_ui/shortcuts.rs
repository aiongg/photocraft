//! Edit › Keyboard Shortcuts and Menus: the framework's Keyboard Shortcuts dialog over a draft
//! keymap, with PhotoCraft's Menus tab beside it.
//!
//! The session holds everything the dialog edits: the draft keymap, the shortcut sets, the Menus
//! draft and the text of a set waiting to be reviewed. Cancel drops it and the runtime never saw
//! a change; OK writes the preferences once ([`save_keys`], then the menus), and the runtime's
//! keymap follows the new preferences revision (`shortcut_dispatch::overrides`).
//!
//! The Toolbar tab is not here: Edit › Toolbar opens the toolbar drawer.

use std::collections::{BTreeMap, BTreeSet};

use craft::keymap::import::kys;
use craft::keymap::{Builtins, CommandRef, ImportOptions, Keymap, NoneDefined, Overrides, Profiles, Scopes, reserved};
use craft::menu::Row;
use craft::menu::shortcuts::{Area, ConflictMode, DialogState, DialogTab, Format, Options, Outcome, ShortcutWords, ShortcutsDialog, kys_error_text};
use craft::ui::choice::Checkbox;
use craft::ui::contract::{Bind, Entry, Props as _, Show as _};
use craft::ui::pickers::Select;
use craft::ui::runtime::env::Env;
use craft::ui::runtime::windows;
use photocraft_engine::keymap_defs;
use photocraft_engine::prefs::{MenuCustomization, Preferences};
use photocraft_engine::shortcut_sets::DEFAULTS_NAME;
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;

/// The id of the `.kys` format in the dialog's import and export lists.
const KYS_FORMAT: &str = "kys";

/// The colours Edit › Menus gives an item; `none` is no colour.
const COLORS: [&str; 8] = ["none", "red", "orange", "yellow", "green", "blue", "violet", "gray"];

/// The open Keyboard Shortcuts and Menus dialog, at `app.prefs_rt.shortcuts`.
pub(crate) struct ShortcutsSession {
    /// The dialog's state between passes.
    pub(crate) state: DialogState,
    /// The shortcut sets as the dialog edits them.
    pub(crate) profiles: Profiles,
    /// The draft the dialog edits in place: the rows the runtime catalog binds, with the
    /// preferences' bindings. The runtime's own keymap is not this one.
    pub(crate) keymap: Keymap,
    /// The Menus tab's draft.
    pub(crate) menus: MenusDraft,
    /// The text of a set to review on the next pass, taken by that pass alone: a second pass of
    /// the same frame finds nothing to review.
    pub(crate) pending_import: Option<String>,
    /// The user's answers to earlier import questions, for the next import of this session.
    pub(crate) remembered: BTreeMap<String, CommandRef>,
}

/// What the Menus tab edits: the hidden items and the colour of each item, by command id.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MenusDraft {
    pub(crate) hidden: BTreeSet<String>,
    pub(crate) colors: BTreeMap<String, String>,
}

impl MenusDraft {
    fn of(menus: &MenuCustomization) -> Self {
        MenusDraft { hidden: menus.hidden.iter().cloned().collect(), colors: menus.colors.clone() }
    }
}

/// The built-in sets: PhotoCraft's defaults.
fn builtins() -> [(&'static str, Overrides); 1] {
    [(DEFAULTS_NAME, Overrides::new())]
}

/// The saved sets of `prefs` as the dialog's `Profiles`. The defaults are the active set when the
/// preferences name none.
pub(crate) fn profiles_of(prefs: &Preferences) -> Profiles {
    let sets: Vec<Value> = prefs.shortcut_sets.iter().map(|(name, set)| json!({"name": name, "overrides": keymap_defs::set_overrides(set)})).collect();
    serde_json::from_value(json!({"active": prefs.shortcut_set, "sets": sets})).unwrap_or_default()
}

/// The keymap the dialog and the offer edit: the rows the runtime catalog binds, the platform the
/// runtime's keymap is built for, and the preferences' bindings as overrides.
pub(crate) fn draft_keymap(app: &PhotocraftApp) -> Keymap {
    let platform = windows::os_of(app.ctx());
    Keymap::new(crate::shortcut_dispatch::command_rows(app).into_iter().chain(crate::shortcut_dispatch::bridged_rows()).collect(), Scopes::new(), platform)
        .with_reserved(reserved::system(platform))
        .with_overrides(keymap_defs::prefs_overrides(app.session.prefs()))
}

/// How a `.kys` set is read for review and for the offer: rows without a key leave their command
/// alone, and the user's earlier answers come first.
pub(crate) fn import_options(remembered: &BTreeMap<String, CommandRef>) -> ImportOptions {
    ImportOptions { remembered: remembered.clone(), none_defined: NoneDefined::Keep, suggest: true, ..ImportOptions::default() }
}

/// The text of a set file's bytes, or why it is not one.
pub(crate) fn kys_text(name: &str, bytes: &[u8]) -> Result<String, String> {
    std::str::from_utf8(bytes).map(str::to_owned).map_err(|_| format!("{name}: not a keyboard shortcut set (.kys): not UTF-8"))
}

/// Opens the dialog (when it is not open) on tab `tab`: 0 Keyboard Shortcuts, 1 Menus.
pub fn open(app: &mut PhotocraftApp, tab: u64) {
    if app.prefs_rt.shortcuts.is_some() {
        return;
    }
    let prefs = app.session.prefs();
    let session = ShortcutsSession {
        state: if tab == 1 { DialogState::new().with_tab("menus") } else { DialogState::new() },
        profiles: profiles_of(prefs),
        keymap: draft_keymap(app),
        menus: MenusDraft::of(&prefs.menus),
        pending_import: None,
        remembered: BTreeMap::new(),
    };
<<<<<<< HEAD
    app.prefs_rt.shortcuts = Some(session);
=======
    let eff = |overrides: &BTreeMap<String, String>, id: &str, def: &Option<String>| -> Option<String> {
        match overrides.get(id) {
            Some(s) if s.is_empty() => None,
            Some(s) => Some(s.clone()),
            None => def.clone(),
        }
    };
    // A command's further keys as the dialog stands: its edited list, else its defaults.
    let eff_alts = |alts: &BTreeMap<String, Vec<String>>, id: &str, main: Option<&str>| -> Vec<String> {
        let mut p = prefs::Preferences::default();
        if let Some(list) = alts.get(id) {
            p.alternate_shortcuts.insert(id.to_string(), list.clone());
        }
        p.alternates(id, main)
    };
    let mut capture_mods = capture_mods_field(f.get("captureMods"));
    // Key capture for the selected command: its main key, or (Add Shortcut) one more key.
    if capture && tab == 0 && !selected.is_empty() {
        let (next_mods, pressed) = ui.input(|i| capture_key_press(capture_mods, i));
        capture_mods = next_mods;
        if let Some((key, m)) = pressed {
            ui.input_mut(|i| {
                i.consume_key(m, key);
            });
            if key == egui::Key::Escape {
                capture = false;
                conflict = None;
            } else if matches!(key, egui::Key::Backspace | egui::Key::Delete) && m.is_none() {
                if capture_alt {
                    alt_overrides.insert(selected.clone(), Vec::new());
                } else {
                    overrides.insert(selected.clone(), String::new());
                }
                capture = false;
                message = tl!("Shortcut removed.").into();
                message_warning = false;
            } else if let Some(sc) = shortcut_text(key, m).filter(|sc| reserved_shortcut(sc)) {
                message = crate::i18n::fmt(tl!("{shortcut} is reserved by the system and can't be assigned."), &[("shortcut", &crate::shortcuts::pretty(&sc))]);
                message_warning = true;
                capture = false;
            } else if let Some(sc) = shortcut_text(key, m) {
                let clash_ids: Vec<&str> = items
                    .iter()
                    .filter(|(id, _, _, def)| {
                        let cur = eff(&overrides, id, def);
                        *id != selected
                            && (cur.as_deref().and_then(prefs::normalize_shortcut) == Some(sc.clone())
                                || eff_alts(&alt_overrides, id, cur.as_deref()).contains(&sc))
                    })
                    .map(|(id, ..)| id.as_str())
                    .collect();
                let clash: Vec<String> = items
                    .iter()
                    .filter(|(id, ..)| clash_ids.contains(&id.as_str()))
                    .map(|(id, label, path, _)| {
                        let (path, label) = shown(id, label, path);
                        format!("{} › {}", path.join(" › "), label.trim_end_matches('…'))
                    })
                    .collect();
                // What Undo Changes puts back: the command's main key and further keys as they were.
                if let Some(first) = clash_ids.first() {
                    conflict = Some(json!({
                        "shortcut": sc,
                        "with": first,
                        // Every command holding the key: Accept takes it from all of them.
                        "holders": clash_ids,
                        "command": selected,
                        "main": overrides.get(&selected),
                        "alternates": alt_overrides.get(&selected),
                    }));
                } else {
                    conflict = None;
                }
                if capture_alt {
                    let def = items.iter().find(|i| i.0 == selected).and_then(|i| i.3.clone());
                    let cur = eff(&overrides, &selected, &def);
                    let mut keys = eff_alts(&alt_overrides, &selected, cur.as_deref());
                    if cur.as_deref().and_then(prefs::normalize_shortcut) != Some(sc.clone()) && !keys.contains(&sc) && keys.len() < prefs::MAX_ALTERNATES {
                        keys.push(sc.clone());
                    }
                    alt_overrides.insert(selected.clone(), keys);
                } else {
                    overrides.insert(selected.clone(), sc.clone());
                }
                message_warning = !clash.is_empty();
                message = if clash.is_empty() {
                    let template = "{shortcut} assigned.";
                    crate::i18n::fmt(tl!(template), &[("shortcut", &crate::shortcuts::pretty(&sc))])
                } else {
                    let template = "{shortcut} is already in use by {commands} and will be removed from it when you click OK.";
                    crate::i18n::fmt(tl!(template), &[("shortcut", &crate::shortcuts::pretty(&sc)), ("commands", &clash.join(", "))])
                };
                capture = false;
            }
        }
    }
    let needle = filter.trim();
    egui::ScrollArea::vertical().max_height(420.0).id_salt("shortcuts-scroll").show(ui, |ui| {
        let mut last_top = String::new();
        egui::Grid::new("shortcut-grid").num_columns(3).spacing([12.0, 3.0]).striped(true).show(ui, |ui| {
            for (id, label, path, def) in &items {
                if tab == 1 && crate::tool_shortcuts::tool(id).is_some() {
                    continue;
                }
                let cur = eff(&overrides, id, def);
                let alts = eff_alts(&alt_overrides, id, cur.as_deref());
                let (shown_path, shown_label) = shown(id, label, path);
                // Match what is shown and the English name (commands are documented in English).
                let local = format!("{} {}", shown_path.join(" "), shown_label);
                let english = format!("{} {} {} {} {id}", path.join(" "), label, cur.clone().unwrap_or_default(), alts.join(" "));
                if !needle.is_empty()
                    && !crate::i18n::current().contains_ignore_case(&local, needle)
                    && !crate::i18n::Lang::EN.contains_ignore_case(&english, needle)
                {
                    continue;
                }
                if tab == 0 && path.is_empty() && def.is_none() && cur.is_none() && alts.is_empty() {
                    continue;
                }
                let top = shown_path.first().cloned().unwrap_or_else(|| tl!("Other").into());
                if top != last_top {
                    ui.label(RichText::new(&top).font(crate::theme::semibold(12.5)).color(t.text));
                    ui.label("");
                    ui.label("");
                    ui.end_row();
                    last_top = top;
                }
                let name = match shown_path.get(1..) {
                    Some(rest) if !rest.is_empty() => format!("{} › {}", rest.join(" › "), shown_label),
                    _ => shown_label,
                };
                let sel = selected == *id;
                let name_row = ui.selectable_label(sel, RichText::new(format!("   {name}")).color(t.text_dim));
                if name_row.clicked() {
                    selected = id.clone();
                }
                if tab == 0 {
                    let text = if sel && capture && !capture_alt {
                        tl!("Press keys…").to_string()
                    } else {
                        cur.as_deref().map(crate::shortcuts::pretty).unwrap_or_default()
                    };
                    let changed = overrides.contains_key(id) || alt_overrides.contains_key(id);
                    let r = ui.add(
                        egui::Button::new(RichText::new(tl!(&text)).size(12.0).color(if changed { t.accent_text } else { t.text })).min_size(vec2(120.0, 18.0)),
                    );
                    // An unassigned shortcut shows no text: name it by the command it binds.
                    let r = if text.is_empty() { r.labelled_by(name_row.id) } else { r };
                    if r.clicked() {
                        selected = id.clone();
                        capture = true;
                        capture_alt = false;
                        message.clear();
                        message_warning = false;
                    }
                    // The further keys (F3 beside ⌘C), each removable on its own, as Photoshop
                    // lists them as rows of their own (#3665).
                    ui.horizontal(|ui| {
                        for k in &alts {
                            let b = egui::Button::new(RichText::new(format!("{}  ×", crate::shortcuts::pretty(k))).size(11.0).color(t.text)).small();
                            if ui.add(b).on_hover_text(tl!("Delete Shortcut")).clicked() {
                                alt_overrides.insert(id.clone(), alts.iter().filter(|x| *x != k).cloned().collect());
                                selected = id.clone();
                                message = tl!("Shortcut removed.").into();
                                message_warning = false;
                            }
                        }
                        if sel && capture && capture_alt {
                            ui.label(RichText::new(tl!("Press keys…")).size(11.0).color(t.accent_text));
                        }
                        if changed {
                            ui.label(RichText::new(tl!("modified")).color(t.text_faint).size(10.5));
                        }
                    });
                } else {
                    // Items that can't be hidden (File › Close, Save…, Edit › Menus…) have no
                    // visibility control at all, as in the reference application (#14335).
                    if photocraft_engine::prefs::MenuCustomization::hideable(id) {
                        let mut visible = !hidden.contains(id);
                        crate::widgets::checkbox(ui, &mut visible, tl!("Visible"));
                        if visible {
                            hidden.retain(|h| h != id);
                        } else if !hidden.contains(id) {
                            hidden.push(id.clone());
                        }
                    } else {
                        ui.label("");
                    }
                    let mut col = colors.get(id).cloned().unwrap_or_else(|| "none".into());
                    let opts: Vec<(String, &str)> =
                        ["none", "red", "orange", "yellow", "green", "blue", "violet", "gray"].iter().map(|c| (c.to_string(), *c)).collect();
                    crate::widgets::dropdown(ui, &format!("menu-color-{id}"), tl!("Color"), &mut col, &opts, 90.0);
                    if col == "none" {
                        colors.remove(id);
                    } else {
                        colors.insert(id.clone(), col);
                    }
                }
                ui.end_row();
            }
        });
    });
    ui.add_space(6.0);
    if tab == 0 {
        ui.horizontal(|ui| {
            let has_sel = !selected.is_empty();
            if ui.add_enabled(has_sel, egui::Button::new(tl!("Add Shortcut"))).clicked() {
                capture = true;
                capture_alt = true;
                message.clear();
                message_warning = false;
            }
            if ui.add_enabled(has_sel, egui::Button::new(tl!("Use Default"))).clicked() {
                overrides.remove(&selected);
                alt_overrides.remove(&selected);
                message = tl!("Default restored.").into();
                message_warning = false;
            }
            if ui.add_enabled(has_sel, egui::Button::new(tl!("Delete Shortcut"))).clicked() {
                overrides.insert(selected.clone(), String::new());
                message = tl!("Shortcut removed.").into();
                message_warning = false;
            }
            if ui.button(tl!("Reset All to Defaults")).clicked() {
                overrides.clear();
                alt_overrides.clear();
                message = tl!("All shortcuts reset to PhotoCraft defaults.").into();
                message_warning = false;
            }
            // A Photoshop set (`.kys`) fills the overrides; the file arrives after this frame
            // and `kys_import` writes the dialog's fields then (#1163).
            if ui.button(tl!("Import Shortcuts…")).on_hover_text(tl!("Load a keyboard shortcut set saved as .kys")).clicked()
                && let Err(e) = crate::kys_import::import_dialog(app)
            {
                message = e;
                message_warning = false;
            }
        });
    } else if ui.button(tl!("Show All Menu Items")).clicked() {
        hidden.clear();
        colors.clear();
    }
    if !message.is_empty() {
        ui.label(RichText::new(&message).color(if message_warning { t.warning } else { t.text_dim }));
    }
    // The conflict describes the bindings as the capture left them: any later change (Use
    // Default, Delete, ×, Reset All, another command or set) makes it stale, and it goes.
    let state = conflict_state(&overrides, &alt_overrides, &selected, f.get("activeSet"));
    if conflict.as_ref().and_then(|c| c.get("state")).is_some_and(|s| s.as_u64() != Some(state)) {
        conflict = None;
    }
    if tab == 0
        && let Some(c) = conflict.clone()
    {
        let (key, with, command) =
            (c["shortcut"].as_str().unwrap_or_default(), c["with"].as_str().unwrap_or_default(), c["command"].as_str().unwrap_or_default());
        ui.horizontal(|ui| {
            if ui.button(tl!("Accept and Go to Conflict")).on_hover_text(tl!("Take the shortcut from the other command and select it")).clicked() {
                let holders: Vec<String> = serde_json::from_value(c["holders"].clone()).unwrap_or_else(|_| vec![with.to_string()]);
                for holder in &holders {
                    let def = items.iter().find(|i| i.0 == *holder).and_then(|i| i.3.clone());
                    let cur = eff(&overrides, holder, &def);
                    if cur.as_deref().and_then(prefs::normalize_shortcut).as_deref() == Some(key) {
                        overrides.insert(holder.clone(), String::new());
                    }
                    let keys = eff_alts(&alt_overrides, holder, eff(&overrides, holder, &def).as_deref());
                    if keys.iter().any(|k| k == key) {
                        alt_overrides.insert(holder.clone(), keys.into_iter().filter(|k| k != key).collect());
                    }
                }
                selected = with.to_string();
                filter.clear();
                message = tl!("Choose a new shortcut for this command, or leave it without one.").into();
                message_warning = false;
                conflict = None;
            }
            if ui.button(tl!("Undo Changes")).on_hover_text(tl!("Put this command's shortcuts back as they were")).clicked() {
                match c["main"].as_str() {
                    Some(m) => overrides.insert(command.to_string(), m.to_string()),
                    None => overrides.remove(command),
                };
                match serde_json::from_value::<Vec<String>>(c["alternates"].clone()) {
                    Ok(list) => alt_overrides.insert(command.to_string(), list),
                    Err(_) => alt_overrides.remove(command),
                };
                selected = command.to_string();
                message = tl!("Changes undone.").into();
                message_warning = false;
                conflict = None;
            }
        });
    }
    if overrides_ok {
        f.insert("overrides".into(), json!(overrides));
    }
    if alternates_ok {
        f.insert("alternates".into(), json!(alt_overrides));
    }
    f.insert("captureAlt".into(), json!(capture && capture_alt));
    // A conflict just found is stamped with the bindings it describes.
    if let Some(c) = conflict.as_mut().filter(|c| c.get("state").is_none()) {
        c["state"] = json!(conflict_state(&overrides, &alt_overrides, &selected, f.get("activeSet")));
    }
    f.insert("conflict".into(), conflict.unwrap_or(Value::Null));
    f.insert("hidden".into(), json!(hidden));
    f.insert("colors".into(), json!(colors));
    f.insert("selected".into(), json!(selected));
    f.insert("capture".into(), json!(capture));
    if capture {
        f.insert("captureMods".into(), capture_mods_json(capture_mods));
    } else {
        f.remove("captureMods");
    }
    f.insert("message".into(), json!(message));
    f.insert("messageWarning".into(), json!(message_warning));
>>>>>>> upstream/main
}

/// Whether the dialog is open: the runtime's keys step aside for its modal.
pub fn shortcuts_open(app: &PhotocraftApp) -> bool {
    app.prefs_rt.shortcuts.is_some()
}

<<<<<<< HEAD
/// Whether the dialog is recording a chord: the key goes to the recorder and runs nothing, and the
/// native menu bar pauses its key equivalents.
pub fn recording(app: &PhotocraftApp) -> bool {
    app.prefs_rt.shortcuts.as_ref().is_some_and(|session| session.state.is_recording())
}

/// Queues the text of a set for review on the dialog's next pass.
pub(crate) fn queue_import(app: &mut PhotocraftApp, text: String) {
    open(app, 0);
    if let Some(session) = app.prefs_rt.shortcuts.as_mut() {
        session.pending_import = Some(text);
    }
}

/// The dialog's options: Photoshop's Shortcuts For areas, the conflict bar, `.kys` files in and
/// out, and the summary.
fn options() -> Options {
    let kys = Format::new(KYS_FORMAT, "Photoshop Keyboard Shortcuts (.kys)", &["kys", "psp"]);
    Options::default().with(|o| {
        o.title = Some(tl!("Keyboard Shortcuts and Menus").to_string());
        o.areas = Area::photoshop();
        o.conflicts = ConflictMode::Ask;
        o.import_formats = vec![kys];
        o.export_formats = vec![kys];
        o.summary = true;
    })
}

/// The dialog over the session's parts for one pass.
fn dialog<'a>(keymap: &'a mut Keymap, profiles: &'a mut Profiles, builtins: &'a Builtins<'a>, options: &'a Options) -> ShortcutsDialog<'a> {
    ShortcutsDialog::new(keymap).sets(profiles, builtins).options(options)
}

/// One pass of the dialog while it is open.
pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    pass(app, ui, false);
}

/// One pass; with `press_ok` the pass presses OK first. The session is out of `app.prefs_rt`
/// during the pass, so nothing in the pass reads it through [`recording`].
fn pass(app: &mut PhotocraftApp, ui: &mut egui::Ui, press_ok: bool) {
    let Some(mut session) = app.prefs_rt.shortcuts.take() else { return };
    let rows = crate::menus::base_rows(app);
    let options = options();
    let builtins = builtins();
    let ShortcutsSession { state, profiles, keymap, menus, pending_import, remembered } = &mut session;
    if let Some(text) = pending_import.take() {
        review(ui, state, dialog(keymap, profiles, &builtins, &options), &text, remembered);
    }
    if press_ok {
        dialog(keymap, profiles, &builtins, &options).ok(ui, state);
    }
    let mut tabs = [menus_tab(&rows, menus)];
    let output = dialog(keymap, profiles, &builtins, &options).tabs(&mut tabs).show(ui, state);
    drop(tabs);
    match output.outcome {
        Some(Outcome::Ok) => {
            if let Err(e) = save_keys(app, keymap, profiles).and_then(|_| save_menus(app, menus)) {
                app.ui.status = crate::i18n::fmt(tl!("The keyboard shortcuts were not saved: {error}"), &[("error", &e)]);
                app.ui.status_error = true;
            }
        }
        Some(Outcome::Import { .. }) => ask_import(app, state),
        Some(Outcome::Export { name, json, .. }) => save_text(app, state, &format!("{name}.json"), json),
        Some(Outcome::ExportAs { format, name, keymap: exported, .. }) if format == KYS_FORMAT => {
            save_text(app, state, &format!("{name}.kys"), kys::write(&name, &exported));
        }
        Some(Outcome::Summary { name, html, .. }) => save_text(app, state, &format!("{name}.html"), html),
        Some(Outcome::Remembered(answers)) => remembered.extend(answers),
        _ => {}
    }
    if state.is_open() {
        app.prefs_rt.shortcuts = Some(session);
    }
}

/// Opens the review of the set `text` holds (a `.kys` file, or craft-keymap JSON), or shows why it
/// cannot be read.
fn review(ui: &egui::Ui, state: &mut DialogState, dialog: ShortcutsDialog<'_>, text: &str, remembered: &BTreeMap<String, CommandRef>) {
    if text.trim_start().starts_with(['{', '[']) {
        if let Err(e) = dialog.import_json(ui, state, text) {
            state.show_error(e.to_string());
        }
        return;
    }
    match kys::read(text) {
        Ok(set) => dialog.review_import(ui, state, set, import_options(remembered)),
        Err(e) => state.show_error(kys_error_text(&e, &Env::of(ui).words::<ShortcutWords>())),
    }
}

/// Import…: ask for the file; the text is reviewed on the pass after it arrives.
fn ask_import(app: &mut PhotocraftApp, state: &mut DialogState) {
    let picked = app.pick_file_bytes_filtered(&["kys", "psp", "json"], |app, name, bytes| {
        let text = kys_text(&name, &bytes)?;
        queue_import(app, text);
        Ok(json!({"dialog": "shortcuts"}))
    });
    if let Err(e) = picked {
        state.show_error(e);
    }
}

/// Saves `text` through the save dialog under the suggested file name.
fn save_text(app: &mut PhotocraftApp, state: &mut DialogState, suggested: &str, text: String) {
    // A set name can hold characters a file name can't.
    let name: String = suggested.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c }).collect();
    let saved = app.pick_save(&name, move |app, path| {
        app.framework.files().write(std::path::Path::new(&path), text.as_bytes()).map_err(|e| e.to_string())?;
        Ok(json!({"path": path}))
    });
    if let Err(e) = saved {
        state.show_error(e);
    }
}

/// Writes the bindings and sets of the dialog (or of an import) to the preferences in one
/// `edit.keyboardShortcuts` call: the one writer of the shortcut preferences. An action's function
/// key belongs to the action, not to a set, so the preferences' own stay whatever set was chosen.
pub(crate) fn save_keys(app: &mut PhotocraftApp, keymap: &Keymap, profiles: &Profiles) -> Result<Value, String> {
    let (mut set, mut alternates) = keymap_defs::overrides_to_prefs(keymap.overrides());
    let prefs = app.session.prefs();
    let is_action = |id: &String| id.starts_with(photocraft_engine::actions_cmds::PLAY_SHORTCUT_PREFIX);
    set.retain(|id, _| !is_action(id));
    alternates.retain(|id, _| !is_action(id));
    set.extend(prefs.shortcuts.iter().filter(|(id, _)| is_action(id)).map(|(id, key)| (id.clone(), key.clone())));
    alternates.extend(prefs.alternate_shortcuts.iter().filter(|(id, _)| is_action(id)).map(|(id, keys)| (id.clone(), keys.clone())));
    let builtins = builtins();
    let active = profiles.active_name(&builtins);
    // OK saves the dialog's bindings into the set in use.
    let sets: Map<String, Value> = profiles
        .sets()
        .iter()
        .map(|(name, overrides)| {
            let overrides = if name.as_str() == active { keymap.overrides() } else { overrides };
            let (mut shortcuts, mut alternate_shortcuts) = keymap_defs::overrides_to_prefs(overrides);
            shortcuts.retain(|id, _| !is_action(id));
            alternate_shortcuts.retain(|id, _| !is_action(id));
            (name.clone(), json!({"shortcuts": shortcuts, "alternateShortcuts": alternate_shortcuts}))
        })
        .collect();
    let active = if active == DEFAULTS_NAME { "" } else { active };
    let call = json!({"reset": true, "set": set, "alternates": alternates, "sets": sets, "activeSet": active, "allowUnknown": true, "removeConflicts": false});
    app.run("edit.keyboardShortcuts", call).map_err(|e| e.to_string())
}

/// Writes the Menus tab's draft to the preferences.
fn save_menus(app: &mut PhotocraftApp, menus: &MenusDraft) -> Result<Value, String> {
    let call = json!({"values": {"menus": {"hidden": menus.hidden, "colors": menus.colors}}});
    app.run("prefs.set", call).map_err(|e| e.to_string())
}

/// The Menus tab: a row per menu item with a Visible check and a colour.
fn menus_tab<'a>(rows: &'a [Row], draft: &'a mut MenusDraft) -> DialogTab<'a> {
    DialogTab::new("menus", tl!("Menus").to_string(), move |ui| menus_body(ui, rows, &mut *draft))
}

/// The Menus tab's body; true when the draft changed. `rows` are the menu items before the
/// customization, so a hidden item is listed here to be shown again.
fn menus_body(ui: &mut egui::Ui, rows: &[Row], draft: &mut MenusDraft) -> bool {
    let before = draft.clone();
    if ui.button(tl!("Show All Menu Items")).clicked() {
        draft.hidden.clear();
    }
    let items: Vec<&Row> = rows.iter().filter(|row| row.label != "---").collect();
    let colors: Vec<Entry<&'static str>> = COLORS.iter().map(|c| Entry::new(*c, *c)).collect();
    let height = ui.spacing().interact_size.y;
    // PLAN: the framework `Table` draws a value in each cell and cannot hold a Checkbox or a Select, so the rows are laid out by hand.
    egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, height, items.len(), |ui, range| {
        for row in &items[range] {
            let id = row.command.id.as_str();
            ui.push_id(id, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("{} › {}", row.path.join(" › "), row.label));
                    // Edit › Menus, Keyboard Shortcuts, Preferences, Save and Exit always stay (#5096).
                    let hideable = MenuCustomization::hideable(id);
                    let visible = !hideable || !draft.hidden.contains(id);
                    let shown = Checkbox::new(Bind::given(visible)).label(tl!("Visible").to_string()).enabled(hideable).show(ui);
                    match shown.events.value() {
                        Some(&true) => {
                            draft.hidden.remove(id);
                        }
                        Some(&false) => {
                            draft.hidden.insert(id.to_owned());
                        }
                        None => {}
                    }
                    let current = draft.colors.get(id).map_or("none", String::as_str).to_owned();
                    let current = COLORS.iter().copied().find(|c| *c == current).unwrap_or("none");
                    let picked = Select::new(tl!("Color").to_string(), Bind::given(current), &colors).show(ui);
                    if let Some(&color) = picked.events.value() {
                        if color == "none" {
                            draft.colors.remove(id);
                        } else {
                            draft.colors.insert(id.to_owned(), color.to_owned());
                        }
                    }
                });
            });
        }
    });
    *draft != before
}

/// Test hook: adds `binding` to `command` in the open dialog's draft keymap, as the dialog's own edit does.
#[cfg(test)]
pub(crate) fn assign(
    app: &mut PhotocraftApp,
    command: &CommandRef,
    binding: craft::keymap::Binding,
    policy: craft::keymap::Policy,
) -> Result<craft::keymap::Assigned, craft::keymap::AssignError> {
    let session = app.prefs_rt.shortcuts.as_mut().ok_or(craft::keymap::AssignError::UnknownCommand)?;
    session.keymap.assign(command, binding, policy)
}

/// Test hook: makes `binding` the only binding of `command` in the open dialog's draft keymap, as recording over its
/// key in the dialog does.
#[cfg(test)]
pub(crate) fn rebind(
    app: &mut PhotocraftApp,
    command: &CommandRef,
    binding: craft::keymap::Binding,
    policy: craft::keymap::Policy,
) -> Result<craft::keymap::Assigned, craft::keymap::AssignError> {
    let session = app.prefs_rt.shortcuts.as_mut().ok_or(craft::keymap::AssignError::UnknownCommand)?;
    session.keymap.set_bindings(command, vec![binding], policy)
}

/// Test hook: presses OK in the open dialog and handles the outcome, in one pass on `ctx`.
#[cfg(test)]
pub(crate) fn press_ok(app: &mut PhotocraftApp, ctx: &egui::Context) {
    crate::testing::run_ui(ctx, Default::default(), |ui| pass(app, ui, true));
}
=======
/// Edit › Keyboard Shortcuts › Set, as in Photoshop: choose a saved set (its bindings replace
/// the dialog's), save the bindings as a new set or over the one in use, delete it, export it as
/// a `.kys` file or summarize it as an HTML page. Everything but Export and Summarize waits for
/// OK, like the rest of the dialog.
#[allow(clippy::too_many_arguments)]
fn set_row(
    app: &mut PhotocraftApp,
    ui: &mut egui::Ui,
    f: &mut Map<String, Value>,
    overrides: &mut BTreeMap<String, String>,
    alternates: &mut BTreeMap<String, Vec<String>>,
    items: &[(String, String, Vec<String>, Option<String>)],
    message: &mut String,
    warning: &mut bool,
) {
    use photocraft_engine::shortcut_sets::{self, ShortcutSet};
    let mut sets: BTreeMap<String, ShortcutSet> = f.get("sets").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let mut active = f.get("activeSet").and_then(Value::as_str).unwrap_or("").to_string();
    if !active.is_empty() && !sets.contains_key(&active) {
        active.clear();
    }
    // The engine decides what a set holds and when it is modified (not the actions' F-keys).
    let now = ShortcutSet::from_bindings(overrides, alternates);
    let modified = shortcut_sets::differs(&sets, &active, overrides, alternates);
    let defaults = tl!("PhotoCraft Defaults").to_string();
    let shown = |name: &str| -> String {
        let base = if name.is_empty() { defaults.clone() } else { name.to_string() };
        if name == active && modified { crate::i18n::fmt(tl!("{set} (modified)"), &[("set", &base)]) } else { base }
    };
    let names: Vec<(String, String)> = std::iter::once(String::new()).chain(sets.keys().cloned()).map(|n| (n.clone(), shown(&n))).collect();
    let mut new_name: Option<String> = f.get("newSetName").and_then(Value::as_str).map(str::to_string);
    // #14326: a set chosen while this one has unsaved edits waits for Yes / No / Cancel, and
    // after Yes, for the edits to be saved under a new name.
    let mut switch_to: Option<String> = f.get("switchTo").and_then(Value::as_str).map(str::to_string);
    let mut switch_after_save: Option<String> = f.get("switchAfterSave").and_then(Value::as_str).map(str::to_string);
    let set_label = if active.is_empty() { defaults.clone() } else { active.clone() };
    let mut export = false;
    let mut summarize = false;
    ui.horizontal(|ui| {
        ui.label(tl!("Set:"));
        let mut chosen = active.clone();
        let pairs: Vec<(String, &str)> = names.iter().map(|(n, l)| (n.clone(), l.as_str())).collect();
        if crate::widgets::dropdown(ui, "shortcut-set", tl!("Set:"), &mut chosen, &pairs, 220.0) && chosen != active {
            if modified {
                // Unsaved edits: ask first (an action's F-key is no edit, see `differs`). The new
                // question replaces any naming step left open after an earlier Yes.
                switch_to = Some(chosen);
                new_name = None;
                switch_after_save = None;
            } else {
                // As `useSet`: the set's bindings, keeping the actions' F-keys.
                sets.get(&chosen).cloned().unwrap_or_default().apply(overrides, alternates);
                active = chosen;
                message.clear();
                // This replaces a question left open for another set, and a naming step left open
                // after Yes with its pending switch.
                switch_to = None;
                new_name = None;
                switch_after_save = None;
            }
        }
        if ui.button(tl!("New Set…")).on_hover_text(tl!("Save these shortcuts as a new set")).clicked() {
            new_name = Some(String::new());
            // A plain new set: no switch follows it.
            switch_after_save = None;
        }
        if ui.add_enabled(!active.is_empty() && modified, egui::Button::new(tl!("Save"))).on_hover_text(tl!("Save the changes to this set")).clicked() {
            sets.insert(active.clone(), now.clone());
            *message = tl!("Set saved.").into();
            *warning = false;
        }
        if ui.add_enabled(!active.is_empty(), egui::Button::new(tl!("Delete"))).on_hover_text(tl!("Delete this set")).clicked() {
            sets.remove(&active);
            active.clear();
            new_name = None;
            switch_after_save = None;
            *message = tl!("Set deleted.").into();
            *warning = false;
        }
        export = ui.button(tl!("Export…")).on_hover_text(tl!("Save these shortcuts as a .kys file")).clicked();
        summarize = ui.button(tl!("Summarize…")).on_hover_text(tl!("Save a list of these shortcuts as a web page")).clicked();
    });
    if let Some(target) = switch_to.clone() {
        ui.horizontal(|ui| {
            ui.label(tl!(SWITCH_SET_QUESTION));
            if ui.button(tl!("Yes")).clicked() {
                // Save first, under a name asked for as New Set… asks, then switch.
                new_name = Some(crate::i18n::fmt(tl!("{set} Copy"), &[("set", &set_label)]));
                switch_after_save = Some(target.clone());
                switch_to = None;
            }
            if ui.button(tl!("No")).clicked() {
                sets.get(&target).cloned().unwrap_or_default().apply(overrides, alternates);
                active = target.clone();
                switch_to = None;
                message.clear();
                // Replaces a naming step left open after an earlier Yes, and its pending switch.
                new_name = None;
                switch_after_save = None;
            }
            if ui.button(tl!("Cancel")).clicked() {
                switch_to = None;
            }
        });
    }
    if let Some(name) = new_name.as_mut() {
        let mut create = false;
        let mut cancel = false;
        ui.horizontal(|ui| {
            ui.label(tl!("Name:"));
            let r = ui.add(egui::TextEdit::singleline(name).desired_width(220.0).hint_text(tl!("set name")));
            create = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            create |= ui.button(tl!("Create")).clicked();
            cancel = ui.button(tl!("Cancel")).clicked();
        });
        if create {
            match shortcut_sets::set_name(name) {
                Ok(n) if !sets.contains_key(&n) && sets.len() >= shortcut_sets::MAX_SETS => {
                    *message = tl!("There are too many sets: delete one first.").into();
                    *warning = true;
                }
                Ok(n) => {
                    sets.insert(n.clone(), now.clone());
                    active = n;
                    new_name = None;
                    *message = tl!("Set saved.").into();
                    *warning = false;
                    // Yes to the switch question: the edits are saved, so switch now.
                    if let Some(target) = switch_after_save.take() {
                        sets.get(&target).cloned().unwrap_or_default().apply(overrides, alternates);
                        active = target;
                    }
                }
                Err(e) => {
                    *message = e;
                    *warning = true;
                }
            }
        } else if cancel {
            // Not measured: cancelling the name after Yes keeps this set and its edits.
            new_name = None;
            switch_after_save = None;
        }
    }
    if export || summarize {
        let result = if export {
            let entries: Vec<photocraft_engine::kys::Entry> = items
                .iter()
                .map(|(id, label, _, def)| photocraft_engine::kys::Entry {
                    id: id.clone(),
                    label: label.clone(),
                    tool: id.starts_with("tools.select."),
                    keys: dialog_keys(overrides, alternates, id, def.as_deref()),
                })
                .collect();
            let text = photocraft_engine::kys::write(&set_label, &entries);
            save_text(app, &format!("{set_label}.kys"), text)
        } else {
            let mut sections: Vec<shortcut_sets::Section> = Vec::new();
            for (id, label, path, def) in items {
                let section = path.first().cloned().unwrap_or_else(|| tl!("Other").to_string());
                let name = path.iter().skip(1).chain(std::iter::once(label)).cloned().collect::<Vec<_>>().join(" › ");
                let row = (name, dialog_keys(overrides, alternates, id, def.as_deref()));
                match sections.iter_mut().find(|(s, _)| *s == section) {
                    Some((_, rows)) => rows.push(row),
                    None => sections.push((section, vec![row])),
                }
            }
            let html = shortcut_sets::summarize(&set_label, &sections, cfg!(target_os = "macos"));
            save_text(app, &format!("{set_label}.html"), html)
        };
        if let Err(e) = result {
            *message = e;
            *warning = true;
        }
    }
    f.insert("sets".into(), json!(sets));
    f.insert("activeSet".into(), json!(active));
    f.insert("newSetName".into(), new_name.map_or(Value::Null, Value::String));
    f.insert("switchTo".into(), switch_to.map_or(Value::Null, Value::String));
    f.insert("switchAfterSave".into(), switch_after_save.map_or(Value::Null, Value::String));
}

/// The reference application's question when another set is chosen over unsaved edits (#14326).
const SWITCH_SET_QUESTION: &str = "Selecting a different set will result in a loss of all unsaved shortcuts.  Save your changes before switching sets?";

/// Save `text` through the save dialog under the suggested file name.
fn save_text(app: &mut PhotocraftApp, suggested: &str, text: String) -> Result<Value, String> {
    // A set name can hold characters a file name can't.
    let name: String = suggested.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c }).collect();
    app.pick_save(&name, move |app, path| {
        let write = app.services.write.as_mut().ok_or("no writer configured")?;
        write(&path, text.as_bytes())?;
        Ok(json!({"path": path}))
    })
}

#[cfg(test)]
#[path = "shortcut_set_switch_tests.rs"]
mod shortcut_set_switch_tests;
>>>>>>> upstream/main
