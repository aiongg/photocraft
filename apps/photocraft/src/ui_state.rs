//! The saved egui layout (`ui.ron`, written by eframe's persistence) and the window's restore
//! store (`window.json`, craft-window).
//!
//! The window's size, position and state live in `window.json`, beside `ui.ron`
//! ([`open_window_store`]). eframe's own `"window"` entry in `ui.ron` is read once, in either
//! layout it has used (flat in egui 0.36, `normal_geometry` since 0.37), to seed `window.json`;
//! then it is removed, because eframe loads that entry on every start whatever `persist_window`
//! says and applies its maximized and full-screen state after the restore store has placed the
//! window.
//!
//! eframe restores the rest of `ui.ron` (zoom, panel sizes) while it creates the window, before
//! PhotoCraft's code runs, and some values it does not check abort startup: a zoom factor of
//! zero, NaN or 1e30 fails a `dpi` assertion. A truncated or hand-edited file would then crash
//! every launch. [`sanitize`] runs first and removes a file holding such values (the panels open
//! at their defaults; nothing else is lost). A file that isn't RON at all is left alone: eframe
//! already ignores it.

use std::{collections::HashMap, path::Path};

use craft::window::restore::{FILE_NAME, MAIN, Store, legacy};

/// Panel rects and other coordinates beyond this many points or pixels are corrupt.
const MAX_COORD: f64 = 1.0e6;
/// egui's zoom factor is the UI scale over the display scale (75% on a 2x display is 0.375,
/// 300% on a 1x display is 3).
const ZOOM: std::ops::RangeInclusive<f64> = 0.1..=10.0;
/// eframe's key for the window geometry in `ui.ron`.
const WINDOW_KEY: &str = "window";

/// The window-restore store in `config_dir` (in memory without one), seeded from eframe's
/// `"window"` entry in `ui.ron` when it holds no main placement yet. Call before [`sanitize`].
pub fn open_window_store(config_dir: Option<&Path>) -> Store {
    let Some(dir) = config_dir else { return Store::in_memory() };
    let mut store = Store::at(&dir.join(FILE_NAME));
    import_window(&mut store, &dir.join("ui.ron"));
    store
}

/// Moves the `"window"` entry of the eframe file at `ui` into `store` when `store` has no main
/// placement, then removes the entry from the file. The entry stays only when saving the import
/// failed: it is then the only copy. An entry that gives no usable placement, or a store that
/// cannot be written (a newer schema), is dropped anyway, because eframe would re-apply its
/// maximized and full-screen state on every start.
fn import_window(store: &mut Store, ui: &Path) {
    let Ok(text) = std::fs::read_to_string(ui) else { return };
    let Ok(mut kv) = ron::from_str::<HashMap<String, String>>(&text) else { return };
    if !kv.contains_key(WINDOW_KEY) {
        return;
    }
    if store.get(MAIN).is_none()
        && let Some(placement) = legacy::eframe_app_ron(&text)
    {
        store.set(MAIN, &placement);
        if let Err(e) = store.save() {
            log::warn!(
                "could not save the window placement to {}: {e}; keeping it in {}",
                store.path().map_or_else(String::new, |p| p.display().to_string()),
                ui.display()
            );
            return;
        }
    }
    kv.remove(WINDOW_KEY);
    let Ok(rest) = ron::to_string(&kv) else { return };
    if let Err(e) = write_atomic(ui, &rest) {
        log::warn!("could not remove the window entry from {}: {e}", ui.display());
    }
}

/// Writes `text` to a temporary file beside `path` and renames it over `path`, so a crash never
/// leaves half a file for eframe to read. The file keeps its mode.
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut name = path.file_name().map(std::ffi::OsStr::to_os_string).unwrap_or_else(|| "ui.ron".into());
    name.push(format!(".tmp{}", std::process::id()));
    let temp = path.with_file_name(name);
    let write = || -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(text.as_bytes())?;
        if let Ok(meta) = std::fs::metadata(path) {
            file.set_permissions(meta.permissions())?;
        }
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)
    };
    let result = write();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// Remove `path` if it holds state that would crash startup. Never fails: errors are logged and
/// leave the file alone (eframe then ignores what it cannot read).
pub fn sanitize(path: Option<&Path>) {
    let Some(path) = path else { return };
    let Ok(text) = std::fs::read_to_string(path) else { return };
    if let Err(why) = check(&text) {
        log::warn!("ignoring saved layout {}: {why}", path.display());
        if let Err(e) = std::fs::remove_file(path) {
            log::warn!("could not remove {}: {e}", path.display());
        }
    }
}

/// `Err` names the first value that would crash startup.
fn check(text: &str) -> Result<(), String> {
    // eframe stores a map of key → RON string; a file that doesn't parse is ignored by eframe.
    let Ok(kv) = ron::from_str::<HashMap<String, String>>(text) else { return Ok(()) };
    for (key, value) in &kv {
        // eframe also skips an entry that doesn't parse.
        let Ok(v) = ron::from_str::<ron::Value>(value) else { continue };
        floats_sane(&v, 0).map_err(|e| format!("{key}: {e}"))?;
        let zoom = (key == "egui").then(|| field(&v, "options").and_then(|o| field(o, "zoom_factor")).and_then(as_f64)).flatten();
        if let Some(z) = zoom.filter(|z| !ZOOM.contains(z)) {
            return Err(format!("egui: zoom factor {z}"));
        }
    }
    Ok(())
}

/// Every float finite and within [`MAX_COORD`]; depth-limited (the file is untrusted).
fn floats_sane(v: &ron::Value, depth: usize) -> Result<(), String> {
    if depth > 64 {
        return Err("nested too deeply".into());
    }
    match v {
        ron::Value::Number(_) => match as_f64(v) {
            Some(f) if !f.is_finite() || f.abs() > MAX_COORD => Err(format!("value {f}")),
            _ => Ok(()),
        },
        ron::Value::Option(Some(inner)) => floats_sane(inner, depth + 1),
        ron::Value::Seq(items) => items.iter().try_for_each(|i| floats_sane(i, depth + 1)),
        ron::Value::Map(m) => m.iter().try_for_each(|(k, v)| floats_sane(k, depth + 1).and_then(|()| floats_sane(v, depth + 1))),
        _ => Ok(()),
    }
}

/// A float (integers such as egui's hashed ids are not coordinates).
fn as_f64(v: &ron::Value) -> Option<f64> {
    match v {
        ron::Value::Number(ron::value::Number::F32(f)) => Some(f64::from(f.get())),
        ron::Value::Number(ron::value::Number::F64(f)) => Some(f.get()),
        _ => None,
    }
}

fn field<'a>(v: &'a ron::Value, name: &str) -> Option<&'a ron::Value> {
    match v {
        ron::Value::Map(m) => m.get(&ron::Value::String(name.into())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{check, open_window_store, sanitize};
    use craft::window::restore::{FILE_NAME, MAIN, Placement, Store};
    use std::collections::HashMap;

    fn file(window: &str, zoom: &str) -> String {
        let egui = format!("(options:(zoom_factor:{zoom},max_passes:2),data:([(18446744073709551615,(ron:\"(x:1.0)\"))]))");
        format!("{{\"window\":{window:?},\"egui\":{egui:?}}}")
    }

    const WINDOW: &str = "(inner_position_pixels:Some((x:616.0,y:430.0)),fullscreen:false,inner_size_points:Some((x:1100.0,y:760.0)))";

    fn entries(text: &str) -> HashMap<String, String> {
        ron::from_str(text).expect("a RON map of strings")
    }

    #[test]
    fn a_saved_layout_is_kept() {
        assert_eq!(check(&file(WINDOW, "1.0")), Ok(()));
        assert_eq!(check(&file(WINDOW, "0.375")), Ok(()));
    }

    #[test]
    fn values_that_abort_startup_are_rejected() {
        // Each of these aborted the app at launch before the check (dpi / wgpu assertions).
        for zoom in ["0.0", "NaN", "1e30", "-1.0"] {
            assert!(check(&file(WINDOW, zoom)).is_err(), "zoom {zoom}");
        }
        for size in ["(x:NaN,y:760.0)", "(x:1100.0,y:1e30)", "(x:inf,y:1.0)"] {
            let w = WINDOW.replace("(x:1100.0,y:760.0)", size);
            assert!(check(&file(&w, "1.0")).is_err(), "size {size}");
        }
    }

    #[test]
    fn unreadable_files_are_left_to_eframe() {
        // eframe ignores files and entries it can't parse, so they can't crash it.
        for text in ["", "not ron {{{", "{\"window\":\"(((\"}", "[1,2,3]"] {
            assert_eq!(check(text), Ok(()), "{text:?}");
        }
        let deep = format!("{{\"egui\":{:?}}}", "[".repeat(500) + &"]".repeat(500));
        let _ = check(&deep);
    }

    #[test]
    fn sanitize_removes_only_a_bad_file() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good.ron");
        let bad = dir.path().join("bad.ron");
        std::fs::write(&good, file(WINDOW, "1.0")).unwrap();
        std::fs::write(&bad, file(WINDOW, "0.0")).unwrap();
        sanitize(Some(&good));
        sanitize(Some(&bad));
        sanitize(Some(&dir.path().join("missing.ron")));
        sanitize(None);
        assert!(!bad.exists());
        assert_eq!(std::fs::read_to_string(&good).unwrap(), file(WINDOW, "1.0"), "a good file is not rewritten");
    }

    #[test]
    fn eframes_window_entry_moves_to_the_restore_store_once() {
        for (name, fixture, size, maximized) in [
            ("0.14", include_str!("../tests/fixtures/ui-0.14.ron"), [1100.0, 760.0], false),
            ("0.15", include_str!("../tests/fixtures/ui-0.15.ron"), [1440.0, 900.0], true),
            ("0.37", include_str!("../tests/fixtures/ui-0.37.ron"), [1440.0, 900.0], false),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let ui = dir.path().join("ui.ron");
            std::fs::write(&ui, fixture).unwrap();
            let store = open_window_store(Some(dir.path()));
            let placement = store.get(MAIN).unwrap_or_else(|| panic!("{name}: imported"));
            assert_eq!(placement.normal.size, size, "{name}");
            assert_eq!(placement.maximized, maximized, "{name}");
            // The file holds it, and ui.ron keeps everything but the window.
            assert_eq!(Store::at(&dir.path().join(FILE_NAME)).get(MAIN), Some(placement), "{name}");
            let rest = entries(&std::fs::read_to_string(&ui).unwrap());
            assert!(!rest.contains_key("window"), "{name}: eframe would re-apply it");
            assert_eq!(rest.get("egui").map(String::as_str), Some("()"), "{name}");
        }
    }

    #[test]
    fn a_saved_placement_is_not_replaced_by_the_import() {
        let dir = tempfile::tempdir().unwrap();
        let mine = Placement::new([1300.0, 800.0], 1.0);
        let mut store = Store::at(&dir.path().join(FILE_NAME));
        store.set(MAIN, &mine);
        assert!(matches!(store.save(), Ok(true)));
        let ui = dir.path().join("ui.ron");
        std::fs::write(&ui, file(WINDOW, "1.0")).unwrap();
        assert_eq!(open_window_store(Some(dir.path())).get(MAIN), Some(mine));
        let rest = entries(&std::fs::read_to_string(&ui).unwrap());
        assert!(!rest.contains_key("window") && rest.contains_key("egui"));
    }

    #[test]
    fn a_saved_window_reads_back_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let mut first = open_window_store(Some(dir.path()));
        assert_eq!(first.get(MAIN), None);
        let mut placement = Placement::new([1000.0, 700.0], 2.0);
        placement.maximized = true;
        first.set(MAIN, &placement);
        assert!(matches!(first.save(), Ok(true)));
        assert_eq!(open_window_store(Some(dir.path())).get(MAIN), Some(placement));
    }

    #[test]
    fn an_unusable_window_entry_or_store_still_loses_the_key() {
        let window = "(normal_geometry:(outer_position_pixels:None,scale_factor:1.0,inner_size_points:(x:500000.0,y:900.0)),fullscreen:true,maximized:true)";
        let text = format!("{{\"window\":{window:?},\"egui\":\"()\"}}");
        // A size the store refuses.
        let dir = tempfile::tempdir().unwrap();
        let ui = dir.path().join("ui.ron");
        std::fs::write(&ui, &text).unwrap();
        assert_eq!(open_window_store(Some(dir.path())).get(MAIN), None);
        assert!(!entries(&std::fs::read_to_string(&ui).unwrap()).contains_key("window"));
        // A window.json from a newer version, which is never written.
        let dir = tempfile::tempdir().unwrap();
        let ui = dir.path().join("ui.ron");
        std::fs::write(&ui, file(WINDOW, "1.0")).unwrap();
        std::fs::write(dir.path().join(FILE_NAME), r#"{"schema_version":999,"windows":{}}"#).unwrap();
        open_window_store(Some(dir.path()));
        assert!(!entries(&std::fs::read_to_string(&ui).unwrap()).contains_key("window"));
    }

    #[test]
    fn without_a_window_entry_nothing_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let ui = dir.path().join("ui.ron");
        let text = "{\"egui\":\"()\"}";
        std::fs::write(&ui, text).unwrap();
        assert_eq!(open_window_store(Some(dir.path())).get(MAIN), None);
        assert_eq!(std::fs::read_to_string(&ui).unwrap(), text);
        assert!(!dir.path().join(FILE_NAME).exists());
        assert_eq!(open_window_store(None).path(), None);
    }
}
