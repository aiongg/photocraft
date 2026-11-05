//! The library Host's settings store, backed by PhotoCraft's preferences.
//!
//! craft-settings' `Remembered` keeps each value under a scope of the `remembered` section. The
//! scopes are `prefs.dialogs` keys: a scope `layer.delete.confirmation` is
//! `prefs.dialogs["layer.delete.confirmation"]`, so stored values, "Don't show again" flags and
//! automation's `prefs.get` / `prefs.set` of `prefs.dialogs[...]` all see the same data. A
//! `StandardPrompt`'s scope `prompt.<key>.skip` is the `skip` field of `prefs.dialogs[<key>]`.
//!
//! The Host is shared (`Arc`) and the preferences are the session's, so the store keeps a copy of
//! `prefs.dialogs` that [`sync`] exchanges with the preferences at the points where dialogs run.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use craft::host::SectionStore;
use serde_json::{Map, Value};

use crate::PhotocraftApp;

const SECTION: &str = "remembered";
const PROMPT: &str = "prompt.";
const SKIP: &str = "skip";

#[derive(Default)]
struct Mirror {
    dialogs: BTreeMap<String, Value>,
    /// The other sections, `prefs.sections`.
    sections: BTreeMap<String, Value>,
    /// The preferences revision `dialogs` was copied from or written to.
    rev: Option<u64>,
    /// The Host wrote since the last [`sync`].
    dirty: bool,
    /// The `prefs.dialogs` scopes and the sections the Host changed since the last [`sync`]. Only
    /// these go into the preferences, so a preference edit of another scope in between is kept.
    written: BTreeSet<String>,
    written_sections: BTreeSet<String>,
}

/// The store the Host reads and writes: the `remembered` section over `prefs.dialogs`, every other
/// section as a table of `prefs.sections`.
#[derive(Default)]
pub struct PrefsSections {
    mirror: Mutex<Mirror>,
}

impl PrefsSections {
    fn mirror(&self) -> std::sync::MutexGuard<'_, Mirror> {
        self.mirror.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The store of `ctx`, made on first use and shared by every Host installed on it.
pub fn of(ctx: &egui::Context) -> Arc<PrefsSections> {
    let id = egui::Id::new("photocraft-host-settings");
    if let Some(store) = ctx.data(|d| d.get_temp::<Arc<PrefsSections>>(id)) {
        return store;
    }
    let store = Arc::new(PrefsSections::default());
    ctx.data_mut(|d| d.insert_temp(id, Arc::clone(&store)));
    store
}

/// Exchanges `prefs.dialogs` with the Host's store: what the Host wrote goes into the
/// preferences, otherwise a preference change since the last call is copied into the store.
pub fn sync(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let store = of(ctx);
    exchange(app, &mut store.mirror());
}

fn exchange(app: &mut PhotocraftApp, mirror: &mut Mirror) {
    let rev = app.session.prefs.rev();
    if mirror.dirty {
        let mut dialogs = app.session.prefs().dialogs.clone();
        let mut sections = app.session.prefs().sections.clone();
        for scope in std::mem::take(&mut mirror.written) {
            match mirror.dialogs.get(&scope) {
                Some(value) => dialogs.insert(scope, value.clone()),
                None => dialogs.remove(&scope),
            };
        }
        for key in std::mem::take(&mut mirror.written_sections) {
            match mirror.sections.get(&key) {
                Some(value) => sections.insert(key, value.clone()),
                None => sections.remove(&key),
            };
        }
        if app.session.prefs().dialogs != dialogs || app.session.prefs().sections != sections {
            let (d, s) = (dialogs.clone(), sections.clone());
            app.session.prefs.edit(|p| {
                p.dialogs = d;
                p.sections = s;
            });
        }
        (mirror.dialogs, mirror.sections) = (dialogs, sections);
        mirror.dirty = false;
        mirror.rev = Some(app.session.prefs.rev());
    } else if mirror.rev != Some(rev) {
        mirror.dialogs = app.session.prefs().dialogs.clone();
        mirror.sections = app.session.prefs().sections.clone();
        mirror.rev = Some(rev);
    }
}

const APP: craft::host::AppInfo = craft::host::AppInfo { id: "photocraft", name: "PhotoCraft", version: env!("CARGO_PKG_VERSION") };

/// Runs `f` with a Host over the preferences as they are now, for the app code that has no egui
/// context (a dialog's seed or its OK). What `f` writes is in the preferences when it returns.
pub fn with_host<R>(app: &mut PhotocraftApp, f: impl FnOnce(&craft::host::Host) -> R) -> R {
    let store = Arc::new(PrefsSections::default());
    exchange(app, &mut store.mirror());
    let host = craft::host::Host::builder(APP).settings(store.clone()).build();
    let result = f(&host);
    exchange(app, &mut store.mirror());
    result
}

/// The value remembered under `scope` (`prefs.dialogs[scope]`), through the Host.
pub fn recall(app: &mut PhotocraftApp, scope: &str) -> Option<Value> {
    with_host(app, |host| craft::settings::Remembered::<Value>::new(scope).recall(host))
}

/// [`recall`] for code that only has the app by reference.
pub fn recall_ref(app: &PhotocraftApp, scope: &str) -> Option<Value> {
    let store = Arc::new(PrefsSections::default());
    {
        let mut mirror = store.mirror();
        mirror.dialogs = app.session.prefs().dialogs.clone();
        mirror.sections = app.session.prefs().sections.clone();
    }
    let host = craft::host::Host::builder(APP).settings(store).build();
    craft::settings::Remembered::<Value>::new(scope).recall(&host)
}

/// Remembers `value` under `scope` through the Host. Nulls are left out: the settings tables
/// cannot hold them.
pub fn remember(app: &mut PhotocraftApp, scope: &str, value: &Value) {
    let value = without_nulls(value);
    with_host(app, |host| craft::settings::Remembered::<Value>::new(scope).store(host, &value));
}

fn without_nulls(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(map.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), without_nulls(v))).collect()),
        Value::Array(items) => Value::Array(items.iter().map(without_nulls).collect()),
        other => other.clone(),
    }
}

impl SectionStore for PrefsSections {
    fn read(&self, key: &str) -> Option<toml::Table> {
        if key != SECTION {
            return match self.mirror().sections.get(key).map(to_toml) {
                Some(Some(toml::Value::Table(table))) => Some(table),
                _ => None,
            };
        }
        let mirror = self.mirror();
        let mut table = toml::Table::new();
        for (scope, value) in &mirror.dialogs {
            let (rest, skip) = split_skip(value);
            if let Some(skip) = skip {
                table.insert(format!("{PROMPT}{scope}.{SKIP}"), toml::Value::Boolean(skip));
            }
            if let Some(value) = rest.as_ref().and_then(to_toml) {
                table.insert(scope.clone(), value);
            }
        }
        Some(table)
    }

    fn write(&self, key: &str, table: toml::Table) {
        let mut mirror = self.mirror();
        if key != SECTION {
            let json = to_json(&toml::Value::Table(table));
            if mirror.sections.get(key) != Some(&json) {
                mirror.sections.insert(key.to_owned(), json);
                mirror.written_sections.insert(key.to_owned());
                mirror.dirty = true;
            }
            return;
        }
        let mut next = BTreeMap::new();
        // The prompts' `skip` flags, by the key of the dialog they belong to.
        let skips: Vec<(&str, bool)> =
            table.iter().filter_map(|(scope, v)| Some((scope.strip_prefix(PROMPT)?.strip_suffix(&format!(".{SKIP}"))?, v.as_bool()?))).collect();
        for (scope, value) in &table {
            if scope.starts_with(PROMPT) && scope.ends_with(&format!(".{SKIP}")) {
                continue;
            }
            let (kept, _) = mirror.dialogs.get(scope).map(split_skip).unwrap_or((None, None));
            // A value that did not change keeps its stored JSON, which TOML cannot always hold (null).
            let json = match kept {
                Some(kept) if to_toml(&kept).as_ref() == Some(value) => kept,
                _ => to_json(value),
            };
            next.insert(scope.clone(), json);
        }
        for (dialog, skip) in skips {
            let entry = next.entry(dialog.to_owned()).or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(object) = entry {
                object.insert(SKIP.into(), Value::Bool(skip));
            }
        }
        if next != mirror.dialogs {
            let changed: Vec<String> = next
                .iter()
                .filter(|(scope, value)| mirror.dialogs.get(*scope) != Some(*value))
                .map(|(scope, _)| scope.clone())
                .chain(mirror.dialogs.keys().filter(|scope| !next.contains_key(*scope)).cloned())
                .collect();
            mirror.written.extend(changed);
            mirror.dialogs = next;
            mirror.dirty = true;
        }
    }
}

/// A `prefs.dialogs` value without its boolean `skip` field, and that field. An object that held
/// only `skip` has no rest.
fn split_skip(value: &Value) -> (Option<Value>, Option<bool>) {
    let Value::Object(object) = value else { return (Some(value.clone()), None) };
    let Some(skip) = object.get(SKIP).and_then(Value::as_bool) else { return (Some(value.clone()), None) };
    let mut rest = object.clone();
    rest.remove(SKIP);
    ((!rest.is_empty()).then_some(Value::Object(rest)), Some(skip))
}

fn to_toml(value: &Value) -> Option<toml::Value> {
    Some(match value {
        Value::Null => return None,
        Value::Bool(b) => toml::Value::Boolean(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => toml::Value::Integer(i),
            None => toml::Value::Float(n.as_f64().filter(|f| f.is_finite())?),
        },
        Value::String(s) => toml::Value::String(s.clone()),
        Value::Array(items) => toml::Value::Array(items.iter().filter_map(to_toml).collect()),
        Value::Object(map) => toml::Value::Table(map.iter().filter_map(|(k, v)| Some((k.clone(), to_toml(v)?))).collect()),
    })
}

fn to_json(value: &toml::Value) -> Value {
    match value {
        toml::Value::Boolean(b) => Value::Bool(*b),
        toml::Value::Integer(i) => Value::from(*i),
        toml::Value::Float(f) => serde_json::Number::from_f64(*f).map_or(Value::Null, Value::Number),
        toml::Value::String(s) => Value::String(s.clone()),
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(items) => Value::Array(items.iter().map(to_json).collect()),
        toml::Value::Table(table) => Value::Object(table.iter().map(|(k, v)| (k.clone(), to_json(v))).collect()),
    }
}

#[cfg(test)]
mod tests {
    use craft::host::{AppInfo, Host};
    use craft::settings::Remembered;
    use serde_json::json;

    use super::*;

    fn app_and_host() -> (PhotocraftApp, egui::Context, Host) {
        let ctx = egui::Context::default();
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.session.prefs.edit(|p| {
            p.dialogs.insert("filter.blur.gaussianBlur".into(), json!({"radius": 9.0, "mode": "x"}));
            p.dialogs.insert("layer.delete.confirmation".into(), json!({"skip": true}));
        });
        sync(&mut app, &ctx);
        let host = Host::builder(AppInfo { id: "photocraft", name: "PhotoCraft", version: "0" }).settings(of(&ctx)).build();
        (app, ctx, host)
    }

    #[test]
    fn remembered_values_are_the_prefs_dialogs_entries() {
        let (mut app, ctx, host) = app_and_host();
        let blur = Remembered::<Map<String, Value>>::new("filter.blur.gaussianBlur");
        assert_eq!(blur.recall(&host).unwrap()["radius"], 9.0);
        assert_eq!(Remembered::<bool>::new("prompt.layer.delete.confirmation.skip").recall(&host), Some(true));
        blur.store(&host, json!({"radius": 2.5}).as_object().unwrap());
        Remembered::<bool>::new("prompt.zoom.warn.skip").store(&host, &true);
        sync(&mut app, &ctx);
        let dialogs = &app.session.prefs().dialogs;
        assert_eq!(dialogs["filter.blur.gaussianBlur"], json!({"radius": 2.5}));
        assert_eq!(dialogs["zoom.warn"], json!({"skip": true}));
        assert_eq!(dialogs["layer.delete.confirmation"], json!({"skip": true}), "untouched entries stay as stored");
        Remembered::<bool>::new("prompt.layer.delete.confirmation.skip").forget(&host);
        sync(&mut app, &ctx);
        assert!(!app.session.prefs().dialogs.contains_key("layer.delete.confirmation"));
    }

    #[test]
    fn every_section_is_kept_in_the_preferences() {
        #[derive(serde::Serialize, serde::Deserialize, Default, PartialEq, Debug)]
        struct Panel {
            width: u32,
        }
        impl craft::host::Section for Panel {
            const KEY: &'static str = "fonts_panel";
        }
        let (mut app, ctx, host) = app_and_host();
        host.set_setting(&Panel { width: 240 });
        sync(&mut app, &ctx);
        assert_eq!(app.session.prefs().sections["fonts_panel"], json!({"width": 240}));
        app.session.prefs.edit(|p| p.sections.insert("fonts_panel".into(), json!({"width": 300})));
        sync(&mut app, &ctx);
        assert_eq!(host.setting::<Panel>(), Panel { width: 300 });
    }

    #[test]
    fn a_preference_edit_and_a_host_store_between_syncs_both_survive() {
        let (mut app, ctx, host) = app_and_host();
        app.session.prefs.edit(|p| p.dialogs.insert("filter.blur.gaussianBlur".into(), json!({"radius": 4})));
        Remembered::<bool>::new("prompt.zoom.warn.skip").store(&host, &true);
        sync(&mut app, &ctx);
        let dialogs = &app.session.prefs().dialogs;
        assert_eq!(dialogs["filter.blur.gaussianBlur"], json!({"radius": 4}));
        assert_eq!(dialogs["zoom.warn"], json!({"skip": true}));
        assert_eq!(dialogs["layer.delete.confirmation"], json!({"skip": true}));
    }

    #[test]
    fn a_preference_change_reaches_the_host() {
        let (mut app, ctx, host) = app_and_host();
        app.session.prefs.edit(|p| p.dialogs.insert("filter.blur.gaussianBlur".into(), json!({"radius": 4})));
        sync(&mut app, &ctx);
        let blur = Remembered::<Map<String, Value>>::new("filter.blur.gaussianBlur");
        assert_eq!(blur.recall(&host).unwrap()["radius"], 4);
    }
}
