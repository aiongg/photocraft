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

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use craft::host::{MemorySections, SectionStore};
use serde_json::{Map, Value};

use crate::PhotocraftApp;

const SECTION: &str = "remembered";
const PROMPT: &str = "prompt.";
const SKIP: &str = "skip";

#[derive(Default)]
struct Mirror {
    dialogs: BTreeMap<String, Value>,
    /// The preferences revision `dialogs` was copied from or written to.
    rev: Option<u64>,
    /// The Host wrote since the last [`sync`].
    dirty: bool,
}

/// The store the Host reads and writes: the `remembered` section over `prefs.dialogs`, every other
/// section in memory.
#[derive(Default)]
pub struct PrefsSections {
    mirror: Mutex<Mirror>,
    other: MemorySections,
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
    let mut mirror = store.mirror();
    let rev = app.session.prefs.rev();
    if mirror.dirty {
        let dialogs = mirror.dialogs.clone();
        if app.session.prefs().dialogs != dialogs {
            app.session.prefs.edit(|p| p.dialogs = dialogs);
        }
        mirror.dirty = false;
        mirror.rev = Some(app.session.prefs.rev());
    } else if mirror.rev != Some(rev) {
        mirror.dialogs = app.session.prefs().dialogs.clone();
        mirror.rev = Some(rev);
    }
}

impl SectionStore for PrefsSections {
    fn read(&self, key: &str) -> Option<toml::Table> {
        if key != SECTION {
            return self.other.read(key);
        }
        let mirror = self.mirror();
        let mut table = toml::Table::new();
        for (scope, value) in &mirror.dialogs {
            let (rest, skip) = split_skip(value);
            if let Some(skip) = skip {
                table.insert(format!("{PROMPT}{scope}.{SKIP}"), toml::Value::Boolean(skip));
            }
            if let Some(value) = rest.as_ref().and_then(to_toml) {
                table.insert(scope.clone(), wrap(value));
            }
        }
        Some(table)
    }

    fn write(&self, key: &str, table: toml::Table) {
        if key != SECTION {
            return self.other.write(key, table);
        }
        let mut mirror = self.mirror();
        let mut next = BTreeMap::new();
        // The prompts' `skip` flags, by the key of the dialog they belong to.
        let skips: Vec<(&str, bool)> =
            table.iter().filter_map(|(scope, v)| Some((scope.strip_prefix(PROMPT)?.strip_suffix(&format!(".{SKIP}"))?, v.as_bool()?))).collect();
        for (scope, value) in &table {
            if scope.starts_with(PROMPT) && scope.ends_with(&format!(".{SKIP}")) {
                continue;
            }
            let (kept, _) = mirror.dialogs.get(scope).map(split_skip).unwrap_or((None, None));
            let value = &unwrap(value);
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
            mirror.dialogs = next;
            mirror.dirty = true;
        }
    }
}

/// An object scope as the library's `DialogFields` reads it: its map under `values`. PhotoCraft
/// stores the map itself (`prefs.dialogs[command] = {"radius": 2.5}`), so reading wraps it and
/// writing unwraps it. An object that already has a `values` table is left as it is.
fn wrap(value: toml::Value) -> toml::Value {
    match value {
        toml::Value::Table(table) if !matches!(table.get("values"), Some(toml::Value::Table(_))) => {
            toml::Value::Table(toml::Table::from_iter([("values".to_owned(), toml::Value::Table(table))]))
        }
        other => other,
    }
}

/// The inverse of [`wrap`]: a table that holds only `values` (a table) is that table.
fn unwrap(value: &toml::Value) -> toml::Value {
    match value {
        toml::Value::Table(table) if table.len() == 1 => match table.get("values") {
            Some(inner @ toml::Value::Table(_)) => inner.clone(),
            _ => value.clone(),
        },
        other => other.clone(),
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

    /// The shape of the library's `DialogFields`.
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Values {
        values: serde_json::Map<String, Value>,
    }

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
        let blur = Remembered::<Values>::new("filter.blur.gaussianBlur");
        assert_eq!(blur.recall(&host).unwrap().values["radius"], 9.0);
        assert_eq!(Remembered::<bool>::new("prompt.layer.delete.confirmation.skip").recall(&host), Some(true));
        blur.store(&host, &Values { values: json!({"radius": 2.5}).as_object().cloned().unwrap() });
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
    fn a_preference_change_reaches_the_host() {
        let (mut app, ctx, host) = app_and_host();
        app.session.prefs.edit(|p| p.dialogs.insert("filter.blur.gaussianBlur".into(), json!({"radius": 4})));
        sync(&mut app, &ctx);
        let blur = Remembered::<Values>::new("filter.blur.gaussianBlur");
        assert_eq!(blur.recall(&host).unwrap().values["radius"], 4);
    }
}
