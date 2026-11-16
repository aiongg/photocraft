use std::time::Instant;

use craft::platform::autosave::{AutosaveDoc, interval_for_minutes};

use super::appearance::{canvas_style, sync_appearance, sync_display_scale, sync_tooltips};
use super::startup::{load, presets_store, recovery, sync_rulers};
use super::{PhotocraftApp, prefs};

/// The notice a failed preferences write posts; a later successful write removes it.
const SAVE_FAILED_TITLE: &str = "Preferences can't be saved";

/// Per-frame upkeep: theme sync, persistence, autosave and the history log. `now` is the
/// caller's monotonic clock (logic also runs on hidden ticks, where egui's input time is that of
/// the last UI pass).
///
/// Nothing here reads the frame's input, so a second call in one frame does nothing new: the
/// preferences revision is already handed over, the store and the autosave timer have nothing
/// due at the same `now`, and the autosave results were drained by the first call.
pub fn tick(app: &mut PhotocraftApp, ctx: &egui::Context, now: Instant) {
    if !app.prefs_rt.loaded {
        load(app);
    }
    recovery(app);
    sync_rulers(app);
    sync_display_scale(app, ctx);
    sync_appearance(app, ctx);
    crate::theme::set_ui_font_size(ctx, app.session.prefs().interface.ui_font_size);
    presets_store(app);
    sync_tooltips(app, ctx);
    app.sync_recent();
<<<<<<< HEAD
    let save_at = store_preferences(app, now);
=======
    super::agents::sync_agent_link(app, ctx);
    // Monotonic, not `ctx.input(|i| i.time)`: logic also runs on hidden ticks, where egui's input
    // time is that of the last UI pass.
    if let Some(wait) = persist(app, crate::gpu_canvas::now_ms() / 1000.0) {
        ctx.request_repaint_after(std::time::Duration::try_from_secs_f64(wait).unwrap_or_default());
    }
>>>>>>> upstream/main
    let style = canvas_style(app);
    if let Some(gpu) = app.gpu.as_ref()
        && app.prefs_rt.gpu_style != Some(style)
    {
        gpu.set_style(style);
        app.prefs_rt.gpu_style = Some(style);
    }
    autosave(app, now);
    if let Some(at) = [save_at, app.autosave.deadline()].into_iter().flatten().min() {
        ctx.request_repaint_after(at.saturating_duration_since(now));
    }
    #[cfg(not(target_arch = "wasm32"))]
    link_updates(app, ctx);
    history_log(app);
}

/// Hands a changed preferences revision to the settings store (`now`: the store's change time;
/// `None` reads the clock).
#[cfg(not(target_arch = "wasm32"))]
fn hand_over(app: &mut PhotocraftApp, now: Option<Instant>) {
    let rev = app.session.prefs.rev();
    if rev == app.prefs_rt.saved_rev || app.services.settings.is_none() {
        return;
    }
    let saved = app.session.saved_prefs();
    if let Some((store, _)) = app.services.settings.as_mut() {
        match now {
            Some(now) => store.update_at(now, |p| *p = saved),
            None => store.update(|p| *p = saved),
        };
    }
    app.prefs_rt.saved_rev = rev;
}

<<<<<<< HEAD
/// Saves the preferences through the store: hands over a change, ticks the store (its debounce and
/// retry backoff), and reports a failed write once and its recovery. Returns when to tick again.
#[cfg(not(target_arch = "wasm32"))]
fn store_preferences(app: &mut PhotocraftApp, now: Instant) -> Option<Instant> {
    hand_over(app, Some(now));
    let (store, _) = app.services.settings.as_mut()?;
    let result = store.tick(now);
    // Another instance changed the file and the store merged it: show its values here too.
    let merged = store.take_merged_change().then(|| store.get().clone());
    match result.status {
        craft::settings::SaveStatus::Failed(e) => save_failed(app, &e.to_string()),
        craft::settings::SaveStatus::Saved => saved(app),
        craft::settings::SaveStatus::Idle | craft::settings::SaveStatus::Pending => {}
    }
    if let Some(merged) = merged {
        app.session.set_saved_prefs(merged);
        app.prefs_rt.saved_rev = app.session.prefs.rev();
    }
    result.next_deadline
}

/// The web build saves through `Services::save_prefs` (browser storage) until it opens its own
/// settings store (FW-264): the whole document on every change, no retry.
#[cfg(target_arch = "wasm32")]
fn store_preferences(app: &mut PhotocraftApp, _now: Instant) -> Option<Instant> {
=======
/// Save changed preferences. A failed write stays pending and is retried with a backoff (a newer
/// change retries sooner); the status bar reports the first failure and a notice a persistent
/// one. Returns the seconds until the next retry while one is pending.
pub(super) fn persist(app: &mut PhotocraftApp, now: f64) -> Option<f64> {
    finish_written_action_shortcuts(app);
>>>>>>> upstream/main
    let rev = app.session.prefs.rev();
    if rev == app.prefs_rt.saved_rev {
        return None;
    }
<<<<<<< HEAD
    let text = app.session.prefs_to_json();
    let Some(save) = app.services.save_prefs.as_mut() else { return None };
    match save(&text) {
        Ok(()) => {
            app.prefs_rt.saved_rev = rev;
            saved(app);
=======
    let r = &app.prefs_rt.save_retry;
    if r.failures > 0 {
        let due = if rev == r.tried_rev { r.at } else { r.at.min(r.tried_at + SAVE_RETRY_S) };
        if now < due {
            return Some(due - now);
        }
    }
    let ours = app.session.prefs_value();
    match save_prepared_preferences(app, ours, true) {
        Ok((ours, deferred)) => {
            if !deferred {
                app.prefs_rt.saved_rev = rev;
            }
            app.prefs_rt.saved_value = Some(ours);
            let r = std::mem::take(&mut app.prefs_rt.save_retry);
            if r.failures > 0 {
                let message = "Preferences saved";
                app.ui.status = tl!(message).into();
                app.ui.status_error = false;
                if let Some(id) = r.notice {
                    app.ui.notices.retain(|n| n.id != id);
                }
            }
            None
        }
        Err(e) => {
            let r = &mut app.prefs_rt.save_retry;
            r.failures = r.failures.saturating_add(1);
            (r.tried_at, r.tried_rev) = (now, rev);
            let delay = (SAVE_RETRY_S * 2f64.powi(r.failures.min(16) as i32 - 1)).min(SAVE_RETRY_MAX_S);
            r.at = now + delay;
            let failures = r.failures;
            if failures == 1 {
                let template = "Couldn't save preferences: {error}. Retrying…";
                app.ui.status = crate::i18n::fmt(tl!(template), &[("error", &e)]);
                app.ui.status_error = true;
            } else if failures == SAVE_NOTICE_AFTER {
                let lines = vec![e, "PhotoCraft keeps retrying; until a save succeeds, preference changes are lost when it closes.".into()];
                let id = crate::notices::post(app, "Preferences can't be saved", lines, true, None);
                app.prefs_rt.save_retry.notice = Some(id);
            }
            Some(delay)
>>>>>>> upstream/main
        }
        Err(e) => save_failed(app, &e),
    }
    None
}

<<<<<<< HEAD
/// A preferences write failed; the store retries it. The first failure in a row sets the status
/// line and posts the notice; the notice being shown keeps later failures quiet.
fn save_failed(app: &mut PhotocraftApp, error: &str) {
    if crate::notices::shown_titled(app, SAVE_FAILED_TITLE) {
=======
fn finish_written_action_shortcuts(app: &mut PhotocraftApp) {
    if app.ui.actions.shortcut_writes_pending() && app.session.action_definitions_written() {
        crate::actions::finish_deferred_shortcut_conflicts(app, true);
    }
}

/// Hold Action shortcut changes whose definitions have not been written. Keep saving
/// other preferences against the same three-way merge base.
fn persistable_preferences(app: &PhotocraftApp, mut ours: Value) -> Result<(Value, bool), String> {
    let browser_pending = app.services.actions_write_pending.as_ref().is_some_and(|pending| pending());
    let native_pending = app.ui.actions.shortcut_writes_pending();
    let is_held = |key: &str| (browser_pending && photocraft_engine::actions_cmds::is_action_key(key)) || app.ui.actions.shortcut_write_pending(key);
    let base = app.prefs_rt.saved_value.as_ref();
    let mut deferred = false;
    for section in ["shortcuts", "alternateShortcuts"] {
        let bindings = ours.get_mut(section).and_then(Value::as_object_mut).ok_or_else(|| format!("serialized preferences {section} must be an object"))?;
        if !browser_pending && !native_pending {
            continue;
        }
        let saved = base.and_then(|value| value.get(section)).and_then(Value::as_object);
        bindings.retain(|key, _| {
            let keep = !is_held(key) || saved.is_some_and(|map| map.contains_key(key));
            deferred |= !keep;
            keep
        });
        for (key, value) in saved.into_iter().flatten().filter(|(key, _)| is_held(key)) {
            if bindings.get(key) != Some(value) {
                deferred = true;
                bindings.insert(key.clone(), value.clone());
            }
        }
    }
    Ok((ours, deferred))
}

fn save_prepared_preferences(app: &mut PhotocraftApp, ours: Value, skip_unchanged: bool) -> Result<(Value, bool), String> {
    let (ours, deferred) = persistable_preferences(app, ours)?;
    // A change that was undone (or set a value to what it already was) leaves nothing to write.
    if !skip_unchanged || app.prefs_rt.saved_value.as_ref() != Some(&ours) {
        write_prefs(app, &ours)?;
    }
    Ok((ours, deferred))
}

/// Write `ours` over the stored preferences, keeping what another instance (a second browser
/// tab, another app window) changed there since this one last loaded or saved them.
fn write_prefs(app: &mut PhotocraftApp, ours: &Value) -> Result<(), String> {
    let Some(save) = app.services.save_prefs.as_mut() else { return Ok(()) };
    let stored = app.services.load_prefs.as_mut().and_then(|load| load()).and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let text = match (stored, &app.prefs_rt.saved_value) {
        (Some(mut theirs @ Value::Object(_)), Some(base)) => {
            let mut base = without_stale_migration(base, &theirs);
            // Older files have no toolColors object. Diff the first saved swatches against
            // their actual defaults so an unchanged field cannot overwrite another window's edit.
            if matches!(base.get("toolColors"), None | Some(Value::Null))
                && ours.get("toolColors").is_some_and(Value::is_object)
                && let Some(fields) = base.as_object_mut()
            {
                fields.insert("toolColors".into(), crate::tool_memory::default_colors());
            }
            merge_changes(&base, ours, &mut theirs);
            serde_json::to_string_pretty(&theirs)
        }
        _ => serde_json::to_string_pretty(ours),
    }
    .map_err(|e| e.to_string())?;
    save(&text)
}

/// The save base without the shift tool switch migration (#5647) once storage already holds a
/// version 2 file: another instance wrote it, so its Use Shift Key for Tool Switch is a choice,
/// not the old default this instance migrated. The base then holds the migrated value (on) instead
/// of the old file's off, so only a change made here since loading is written over that choice.
fn without_stale_migration(base: &Value, theirs: &Value) -> Value {
    let mut base = base.clone();
    let new_file = |v: &Value| v.get("version").and_then(Value::as_u64).is_some_and(|v| v >= 2);
    if new_file(theirs) && !new_file(&base) {
        let key = "useShiftKeyForToolSwitch";
        if let Some(tools) = base.get_mut("tools").and_then(Value::as_object_mut)
            && tools.get(key) == Some(&Value::Bool(false))
        {
            tools.insert(key.into(), Value::Bool(true));
        }
    }
    base
}

/// Three-way merge of preference trees: whatever `ours` changed since `base` goes over `theirs`
/// (what storage holds now) and the rest of `theirs` is kept. Objects merge key by key; any
/// other value, arrays included, is replaced whole.
pub(super) fn merge_changes(base: &Value, ours: &Value, theirs: &mut Value) {
    if ours == base {
>>>>>>> upstream/main
        return;
    }
    let template = "Couldn't save preferences: {error}. Retrying…";
    app.ui.status = crate::i18n::fmt(tl!(template), &[("error", error)]);
    app.ui.status_error = true;
    let lines = vec![error.to_string(), "PhotoCraft keeps retrying; until a save succeeds, preference changes are lost when it closes.".into()];
    crate::notices::post(app, SAVE_FAILED_TITLE, lines, craft::theme::variant::Tone::Bad, None);
}

/// A preferences write succeeded: after a failure, say so and take the notice down.
fn saved(app: &mut PhotocraftApp) {
    if !crate::notices::shown_titled(app, SAVE_FAILED_TITLE) {
        return;
    }
    crate::notices::remove_titled(app, SAVE_FAILED_TITLE);
    let message = "Preferences saved";
    app.ui.status = tl!(message).into();
    app.ui.status_error = false;
}

/// Write the preferences now, for choices that need the result immediately (recovery, "don't
/// show again"). The change stays pending in the store, and is retried, when the write fails.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn save_preferences(app: &mut PhotocraftApp) -> Result<(), String> {
    hand_over(app, None);
    let Some((store, _)) = app.services.settings.as_mut() else { return Ok(()) };
    store.flush().map_err(|e| e.to_string())?;
    saved(app);
    Ok(())
}

/// Write the preferences now (the web build's storage).
#[cfg(target_arch = "wasm32")]
pub(crate) fn save_preferences(app: &mut PhotocraftApp) -> Result<(), String> {
    finish_written_action_shortcuts(app);
    let rev = app.session.prefs.rev();
<<<<<<< HEAD
    let text = app.session.prefs_to_json();
    let Some(save) = app.services.save_prefs.as_mut() else { return Ok(()) };
    save(&text)?;
    app.prefs_rt.saved_rev = rev;
    saved(app);
=======
    let ours = app.session.prefs_value();
    let (ours, deferred) = save_prepared_preferences(app, ours, false)?;
    if !deferred {
        app.prefs_rt.saved_rev = rev;
    }
    app.prefs_rt.saved_value = Some(ours);
    if let Some(id) = std::mem::take(&mut app.prefs_rt.save_retry).notice {
        app.ui.notices.retain(|n| n.id != id);
    }
>>>>>>> upstream/main
    Ok(())
}

/// Linked smart objects follow their files while the document is open, as in Photoshop: every two
/// seconds, and as soon as the window comes back to the front (after saving in another app), stamp
/// the active document's linked files on a worker thread and re-render the layers of the ones that
/// changed ([`photocraft_engine::link_watch`]).
#[cfg(not(target_arch = "wasm32"))]
fn link_updates(app: &mut PhotocraftApp, ctx: &egui::Context) {
    match app.session.poll_link_scan() {
        Some(Ok(r)) => {
            if let Some(n) = r["updated"].as_array().map(Vec::len).filter(|n| *n > 0) {
                app.ui.status = crate::i18n::trn(
                    crate::i18n::current(),
                    n as u64,
                    "Updated {n} linked smart object changed on disk",
                    "Updated {n} linked smart objects changed on disk",
                );
            }
        }
        Some(Err(e)) => app.ui.status = e.to_string(),
        None => {}
    }
    let (last_key, focus_key) = (egui::Id::unique("linkWatch.lastScan"), egui::Id::unique("linkWatch.focused"));
    let now = crate::gpu_canvas::now_ms() / 1000.0;
    let focused = ctx.input(|i| i.focused);
    let was_focused: bool = ctx.data(|d| d.get_temp(focus_key)).unwrap_or(focused);
    ctx.data_mut(|d| d.insert_temp(focus_key, focused));
    let has_links = app.session.active().is_some_and(|d| !photocraft_engine::link_watch::linked_files(&d.doc).is_empty());
    if !has_links {
        return;
    }
    let last: f64 = ctx.data(|d| d.get_temp(last_key)).unwrap_or(f64::NEG_INFINITY);
    if now - last >= 2.0 || (focused && !was_focused) {
        ctx.data_mut(|d| d.insert_temp(last_key, now));
        app.session.start_link_scan();
    }
    let wait = if app.session.link_scan.is_some() { 100 } else { 2000 };
    ctx.request_repaint_after(std::time::Duration::from_millis(wait));
}

/// Background autosave of documents with unsaved changes every N minutes (File Handling). The
/// timer (`app.autosave`) decides which documents are due and which recovery copies to discard;
/// the write goes through `Services::autosave`.
///
/// A saved or closed document needs no `Autosave::forget` here: the timer's next `due` finds it
/// clean or gone and lists it in `discard` (craft-platform autosave/timer.rs:124-132).
fn autosave(app: &mut PhotocraftApp, now: Instant) {
    if app.services.autosave.is_none() {
        return;
    }
    let fh = &app.session.prefs().file_handling;
    let interval = fh.autosave.then(|| interval_for_minutes(prefs::nearest_autosave_minutes(fh.autosave_minutes))).flatten();
    app.autosave.set_interval(interval);
    // A successful queue operation is not a successful disk write. Poll each frame, including
    // frames before the next autosave interval, so failures are visible.
    if let Some(results) = app.services.autosave_results.as_mut() {
        for (id, revision, result) in results() {
            if let Some(failed) = app.autosave.finished(id, revision, result) {
                autosave_failed(app, failed);
            }
        }
    }
    let docs: Vec<AutosaveDoc> = app.session.documents().iter().map(|d| AutosaveDoc { id: d.doc.id.0, revision: d.revision, dirty: d.is_dirty() }).collect();
    let due = app.autosave.due(now, docs);
    if let Some(discard) = app.services.discard_autosave.as_mut() {
        for id in &due.discard {
            discard(*id);
        }
    }
    let jobs: Vec<_> = due
        .save
        .iter()
        .filter_map(|id| app.session.documents().iter().find(|d| d.doc.id.0 == *id))
        .map(|d| (d.doc.clone(), d.revision, d.path.clone()))
        .collect();
    let reports_later = app.services.autosave_results.is_some();
    for (doc, revision, path) in jobs {
        let Some(save) = app.services.autosave.as_mut() else { return };
        let queued = save(&doc, revision, path.as_deref());
        // Synchronous or mocked services report completion on return; a failure to queue is the
        // write's failure either way.
        if (queued.is_err() || !reports_later)
            && let Some(failed) = app.autosave.finished(doc.id.0, revision, queued)
        {
            autosave_failed(app, failed);
        }
    }
}

/// The status line of a failed autosave (`Autosave::finished`'s text); the next interval retries.
fn autosave_failed(app: &mut PhotocraftApp, text: String) {
    app.ui.status = text;
    app.ui.status_error = true;
}

/// Makes the next tick autosave every dirty document that has none in flight (tests, `prefs`
/// changes).
pub fn autosave_now(app: &mut PhotocraftApp) {
    app.autosave.autosave_now();
}

/// History Log preference: append executed commands to a text file.
fn history_log(app: &mut PhotocraftApp) {
    let n = app.session.journal.len();
    if n <= app.prefs_rt.log_len {
        app.prefs_rt.log_len = n;
        return;
    }
    let hl = app.session.prefs().history_log.clone();
    let start = app.prefs_rt.log_len;
    app.prefs_rt.log_len = n;
    if !hl.enabled || hl.file_path.is_empty() || hl.destination == prefs::LogDestination::Metadata {
        return;
    }
    let Some(append) = app.services.append_text.as_mut() else { return };
    let mut text = String::new();
    for (id, params) in &app.session.journal[start..] {
        let label = photocraft_engine::commands::find(id).map_or(id.as_str(), |c| c.label).trim_end_matches('…');
        match hl.detail {
            prefs::LogDetail::SessionsOnly => {
                if matches!(id.as_str(), "file.new" | "file.open" | "file.close" | "file.openAs") {
                    text.push_str(&format!("{label}\n"));
                }
            }
            prefs::LogDetail::Concise => text.push_str(&format!("{label}\n")),
            prefs::LogDetail::Detailed => text.push_str(&format!("{label}  {id} {params}\n")),
        }
    }
    if !text.is_empty() {
        let _ = append(&hl.file_path, &text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn malformed_shortcut_maps_are_not_written() {
        let services = crate::Services {
            save_prefs: Some(Box::new(|_| panic!("malformed preferences reached storage"))),
            actions_write_pending: Some(Box::new(|| true)),
            ..Default::default()
        };
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
        for skip_unchanged in [true, false] {
            for malformed in [json!({"shortcuts": []}), json!({"shortcuts": {}, "alternateShortcuts": null})] {
                let error = save_prepared_preferences(&mut app, malformed, skip_unchanged).unwrap_err();
                assert!(error.starts_with("serialized preferences "), "{error}");
            }
        }
    }
}
