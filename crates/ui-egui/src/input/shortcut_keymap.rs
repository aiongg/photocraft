//! PhotoCraft's shortcut catalog as a `craft::keymap::Keymap`: the source of truth for what key
//! a command has. The catalog lists every bindable id (the shell's and the engine's commands, the
//! Photoshop menu catalogue, toolbar tools, held temporary tools and recorded actions) with its
//! default keys. The user's Edit › Keyboard Shortcuts assignments (`Preferences::shortcuts` for
//! the main key, `alternate_shortcuts` for further keys) are its overrides. A command's first
//! binding is its main key (the one its menu shows), the rest are its alternates (#3665).
//!
//! The Keymap is rebuilt when the preferences or the recorded actions change
//! ([`PhotocraftApp::keymap`]). Dialog-local keys are separate scopes in [`crate::dialog_keys`].
//! The engine's `edit.keyboardShortcuts` (headless) and its `.kys` matcher read `Preferences`
//! directly: the engine has no UI dependencies.

use std::collections::{BTreeMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use craft::keymap::{Bindable, Binding, Chord, CommandRef, Keymap, Kind, Overrides, Platform, Scope, Scopes};

use crate::PhotocraftApp;

/// Chord text in the library's canonical spelling. Saved files spell Cmd+Plus `Cmd+` as well as
/// `Cmd++`.
fn chord_text(s: &str) -> Option<String> {
    let chord = Chord::parse(s).or_else(|e| if s.ends_with('+') && !s.ends_with("++") { Chord::parse(&format!("{s}+")) } else { Err(e) });
    chord.ok().map(|c| c.to_string())
}

/// The key a registry lists for `id`: the shell command's, else the engine's, else the menu
/// catalogue's, else a held tool's, else a toolbar tool's.
fn registry_default(id: &str) -> Option<&'static str> {
    crate::menus::UI_COMMANDS
        .iter()
        .find(|c| c.0 == id)
        .and_then(|c| c.3)
        .or_else(|| photocraft_engine::commands::find(id).and_then(|c| c.shortcut))
        .or_else(|| crate::menu_catalog::CATALOG.iter().find(|c| c.3 == id).and_then(|c| c.2))
        .or_else(|| photocraft_engine::prefs::TEMPORARY_TOOLS.iter().find(|t| t.0 == id).map(|t| t.2))
}

/// The default main key as the registries spell it.
fn default_main_text(id: &str) -> Option<String> {
    registry_default(id).map(str::to_string).or_else(|| crate::tool_shortcuts::tool(id).and_then(crate::tool_shortcuts::default))
}

fn bindable(id: &str, label: &str, menu: &[&str], tool: Option<char>) -> Bindable {
    let mut b = Bindable::new(CommandRef::new(id), label, Scope::app());
    b = b.with_path(menu.iter().map(|s| s.to_string()).chain(std::iter::once(label.to_string())).collect());
    if let Some(key) = tool {
        b = b.with_kind(Kind::Tool { group: (key != '\0').then(|| key.to_string()) });
    }
    let mut texts: Vec<String> = default_main_text(id).into_iter().collect();
    texts.extend(photocraft_engine::prefs::default_alternates(id).map(str::to_string));
    for text in texts {
        if let Some(binding) = chord_text(&text).and_then(|t| Binding::parse(&t).ok()) {
            b = b.with_default(binding);
        }
    }
    b
}

/// What the catalog lists, in the order the dispatch table prefers (the shell's commands, the
/// engine's, the menu catalogue's, the tools, the held tools).
fn registry() -> &'static [Bindable] {
    static REGISTRY: OnceLock<Vec<Bindable>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut out: Vec<Bindable> = Vec::new();
        out.extend(crate::menus::UI_COMMANDS.iter().map(|(id, label, menu, _)| bindable(id, label, menu, None)));
        out.extend(photocraft_engine::command_specs().iter().map(|c| bindable(c.id, c.label, c.menu, None)));
        out.extend(crate::menu_catalog::CATALOG.iter().filter(|c| c.1 != "---").map(|(menu, label, _, id)| bindable(id, label, menu, None)));
        out.extend(crate::state::Tool::ALL.into_iter().map(|t| bindable(&crate::tool_shortcuts::id(t), t.label(), &["Tools"], Some(t.key()))));
        out.extend(photocraft_engine::prefs::TEMPORARY_TOOLS.iter().map(|(id, label, _)| bindable(id, label, &["Tools", "Temporary"], None)));
        out
    })
}

/// The catalog with only its default keys: what a command has before the user changes anything.
fn defaults() -> &'static ShortcutKeymap {
    static DEFAULTS: OnceLock<ShortcutKeymap> = OnceLock::new();
    DEFAULTS.get_or_init(|| ShortcutKeymap::new(&[], &BTreeMap::new(), &BTreeMap::new()))
}

/// A command's default main key (the registry's, or a toolbar tool's letter).
pub fn default_main(id: &str) -> Option<String> {
    defaults().main(id)
}

/// The keymap and what the Keymap's single list per command cannot say.
pub struct ShortcutKeymap {
    keymap: Keymap,
    /// Commands whose main key the user set or removed.
    main_overridden: HashSet<String>,
    /// Commands whose main key the user removed while further keys remain: their list starts
    /// with an alternate.
    main_cleared: HashSet<String>,
    alternates_overridden: HashSet<String>,
    /// Commands whose keys the dispatch table takes (see [`ShortcutKeymap::fires`]), worked out
    /// on first use: a draft in the Keyboard Shortcuts dialog never needs it.
    fires: OnceLock<HashSet<String>>,
    user_main: HashSet<String>,
}

impl ShortcutKeymap {
    /// The catalog plus the recorded actions (`action_ids`, bindable by the user and unbound by
    /// default), with `shortcuts` and `alternates` as the user's overrides.
    pub fn new(action_ids: &[String], shortcuts: &BTreeMap<String, String>, alternates: &BTreeMap<String, Vec<String>>) -> ShortcutKeymap {
        let mut catalog: Vec<Bindable> = action_ids.iter().map(|id| bindable(id, id, &[], None)).collect();
        catalog.extend(registry().iter().cloned());
        let known: HashSet<&str> = catalog.iter().map(|b| b.command.id.as_str()).collect();
        // Ids the registry does not list but the user has keys for (set with `allowUnknown`).
        let extra: Vec<Bindable> = shortcuts
            .keys()
            .chain(alternates.keys())
            .filter(|id| !known.contains(id.as_str()) && !id.starts_with(crate::actions::SHORTCUT_PREFIX))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|id| bindable(id, id, &[], None))
            .collect();
        catalog.extend(extra);
        let defaults: Keymap = Keymap::new(catalog, Scopes::new(), Platform::current());

        let mut overrides = Overrides::new();
        let (mut main_overridden, mut main_cleared, mut alternates_overridden) = (HashSet::new(), HashSet::new(), HashSet::new());
        let mut prefs = photocraft_engine::prefs::Preferences::default();
        prefs.shortcuts.clone_from(shortcuts);
        prefs.alternate_shortcuts.clone_from(alternates);
        let ids: std::collections::BTreeSet<&String> = shortcuts.keys().chain(alternates.keys()).collect();
        for id in ids {
            let main = prefs.shortcut(id, default_main_text(id).as_deref()).map(str::to_string);
            let rest = prefs.alternates(id, main.as_deref());
            if shortcuts.contains_key(id) {
                main_overridden.insert(id.clone());
                if main.is_none() && !rest.is_empty() {
                    main_cleared.insert(id.clone());
                }
            }
            if alternates.contains_key(id) {
                alternates_overridden.insert(id.clone());
            }
            let texts: Vec<String> = main.iter().chain(rest.iter()).filter_map(|s| chord_text(s)).collect();
            overrides.insert(id.clone(), texts);
        }
        // Photoshop's Window › <panel> item stands in for the shell command it runs
        // (`menus::panel_alias`), so an assignment to that item replaces the command's default.
        for key in shortcuts.keys() {
            if let Some(target) = crate::menus::panel_alias(key)
                && !shortcuts.contains_key(target)
                && crate::menu_catalog::CATALOG.iter().any(|c| c.3 == key.as_str() && c.2.is_some() && c.2 == registry_default(target))
            {
                overrides.insert(target, Vec::new());
            }
        }
        let keymap = defaults.with_overrides(overrides);

        let user_main = shortcuts.iter().filter(|(_, s)| !s.is_empty()).map(|(id, _)| id.clone()).collect();
        ShortcutKeymap { keymap, main_overridden, main_cleared, alternates_overridden, fires: OnceLock::new(), user_main }
    }

    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    /// Does the catalog list `id`?
    pub fn knows(&self, id: &str) -> bool {
        self.keymap.bindable(&CommandRef::new(id)).is_some()
    }

    /// Every bindable id, in the order the dispatch table prefers.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.keymap.catalog().iter().map(|b| b.command.id.as_str())
    }

    fn texts(&self, id: &str) -> Vec<String> {
        self.keymap.bindings(&CommandRef::new(id)).iter().map(|b| b.trigger.to_string()).collect()
    }

    /// The key in effect for a command, in the library's canonical text; `None` when it has none.
    pub fn main(&self, id: &str) -> Option<String> {
        if self.main_cleared.contains(id) { None } else { self.texts(id).into_iter().next() }
    }

    /// A command's further keys in effect.
    pub fn alternates(&self, id: &str) -> Vec<String> {
        let all = self.texts(id);
        let skip = usize::from(!self.main_cleared.contains(id));
        all.into_iter().skip(skip).collect()
    }

    /// Did the user set or remove this command's main key?
    pub fn main_overridden(&self, id: &str) -> bool {
        self.main_overridden.contains(id)
    }

    /// Did the user give this command its own list of further keys?
    pub fn alternates_overridden(&self, id: &str) -> bool {
        self.alternates_overridden.contains(id)
    }

    /// Does the key dispatch run this command? Shell and engine commands and recorded actions
    /// do. A menu item of Photoshop's tree does when it is implemented and has a default key, or
    /// the user bound it. A toolbar tool does only when the user bound it: unchanged tools keep
    /// Photoshop's shared keys and Shift cycling. Held tools never do ([`crate::hold_keys`]).
    pub fn fires(&self, id: &str) -> bool {
        self.fires.get_or_init(|| self.dispatched()).contains(id)
    }

    fn dispatched(&self) -> HashSet<String> {
        static UI: OnceLock<HashSet<&'static str>> = OnceLock::new();
        static MENU: OnceLock<HashSet<&'static str>> = OnceLock::new();
        let ui = UI.get_or_init(|| crate::menus::UI_COMMANDS.iter().map(|c| c.0).collect());
        let menu = MENU.get_or_init(|| crate::menu_catalog::CATALOG.iter().map(|c| c.3).collect());
        let user_main = |id: &str| self.user_main.contains(id);
        self.ids()
            .filter(|id| {
                if crate::hold_keys::is_temporary(id) {
                    false
                } else if crate::tool_shortcuts::tool(id).is_some() {
                    user_main(id)
                } else if ui.contains(id) || photocraft_engine::commands::find(id).is_some() {
                    true
                } else if menu.contains(id) {
                    (registry_default(id).is_some() && crate::menus::is_live(id)) || user_main(id)
                } else {
                    // A recorded action, or an id the user bound.
                    true
                }
            })
            .map(str::to_string)
            .collect()
    }
}

/// The keymap for a draft of the assignments (the Keyboard Shortcuts dialog before OK), listing
/// the dialog's `ids` too. The last draft is kept, so the dialog's many lookups in a frame build
/// it once.
pub fn draft(ids: &[String], shortcuts: &BTreeMap<String, String>, alternates: &BTreeMap<String, Vec<String>>) -> Arc<ShortcutKeymap> {
    type Slot = Option<(Vec<String>, BTreeMap<String, String>, BTreeMap<String, Vec<String>>, Arc<ShortcutKeymap>)>;
    thread_local! {
        static LAST: std::cell::RefCell<Slot> = const { std::cell::RefCell::new(None) };
    }
    LAST.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some((i, s, a, km)) = slot.as_ref()
            && i == ids
            && s == shortcuts
            && a == alternates
        {
            return km.clone();
        }
        let km = Arc::new(ShortcutKeymap::new(ids, shortcuts, alternates));
        *slot = Some((ids.to_vec(), shortcuts.clone(), alternates.clone(), km.clone()));
        km
    })
}

/// The keymap of one app, rebuilt when the preferences or the recorded actions change.
#[derive(Default)]
pub struct Cache(Mutex<Option<(u64, Arc<ShortcutKeymap>)>>);

impl Cache {
    fn get(&self, app: &PhotocraftApp) -> Arc<ShortcutKeymap> {
        let mut h = DefaultHasher::new();
        app.session.prefs.rev().hash(&mut h);
        app.session.actions.list.iter().for_each(|a| a.name.hash(&mut h));
        let key = h.finish();
        let mut slot = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((k, km)) = slot.as_ref()
            && *k == key
        {
            return km.clone();
        }
        let prefs = app.session.prefs();
        let actions: Vec<String> = app.session.actions.list.iter().map(|a| crate::actions::shortcut_id(&a.name)).collect();
        let km = Arc::new(ShortcutKeymap::new(&actions, &prefs.shortcuts, &prefs.alternate_shortcuts));
        *slot = Some((key, km.clone()));
        km
    }
}

impl PhotocraftApp {
    /// The shortcut catalog with the user's assignments.
    pub fn keymap(&self) -> Arc<ShortcutKeymap> {
        self.keymap.get(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn app() -> PhotocraftApp {
        PhotocraftApp::new(photocraft_engine::Session::new(), Default::default())
    }

    #[test]
    fn every_registry_default_is_in_the_catalog() {
        let km = defaults();
        for (id, _, _, sc) in crate::menus::UI_COMMANDS.iter().chain(photocraft_engine::shell_commands::SHELL_COMMANDS.iter()) {
            if let Some(sc) = sc {
                assert!(chord_text(sc).is_some(), "{id}: {sc} is not a chord");
                assert!(km.main(id).is_some(), "{id}: default {sc} missing from the catalog");
            }
        }
        for c in photocraft_engine::command_specs() {
            if let Some(sc) = c.shortcut {
                assert!(km.main(c.id).is_some(), "{}: default {sc} missing from the catalog", c.id);
            }
        }
        for (menu, label, sc, id) in crate::menu_catalog::CATALOG {
            if sc.is_some() && *label != "---" {
                assert!(km.main(id).is_some(), "{menu:?} {id}: default {sc:?} missing from the catalog");
            }
        }
        assert_eq!(km.main("edit.copy").as_deref(), Some("Cmd+C"));
        assert_eq!(km.alternates("edit.copy"), ["F3"]);
        assert_eq!(default_main("edit.copy").as_deref(), Some("Cmd+C"));
    }

    #[test]
    fn the_keymap_follows_the_preferences() {
        let mut app = app();
        assert_eq!(app.keymap().main("edit.copy").as_deref(), Some("Cmd+C"));
        app.run("prefs.set", json!({"path": "shortcuts.edit.copy", "value": "Cmd+Alt+C"})).unwrap();
        let km = app.keymap();
        assert_eq!(km.main("edit.copy").as_deref(), Some("Cmd+Alt+C"));
        assert_eq!(km.alternates("edit.copy"), ["F3"], "the default further key stays");
        assert!(km.main_overridden("edit.copy"));
        app.run("prefs.set", json!({"path": "shortcuts.edit.copy", "value": ""})).unwrap();
        let km = app.keymap();
        assert_eq!((km.main("edit.copy"), km.alternates("edit.copy")), (None, vec!["F3".to_string()]), "a removed main key leaves the further key");
        app.run("prefs.reset", json!({"path": "shortcuts.edit.copy"})).unwrap();
        assert_eq!(app.keymap().main("edit.copy").as_deref(), Some("Cmd+C"));
    }

    #[test]
    fn dispatch_takes_tools_only_when_the_user_bound_them() {
        let mut app = app();
        let km = app.keymap();
        assert!(km.fires("edit.copy") && !km.fires("tools.select.pencil") && !km.fires("tools.temporary.hand"));
        app.run("prefs.set", json!({"path": "shortcuts.tools.select.pencil", "value": "F6"})).unwrap();
        assert!(app.keymap().fires("tools.select.pencil"));
    }
}
