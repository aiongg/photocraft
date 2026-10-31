//! Non-blocking notices: a small stack of cards in the lower-right corner of the window for things
//! the user should see but that must not interrupt work (import/export warnings such as "adjustment
//! flattened", files that couldn't be opened). Notices live in [`UiState`](crate::UiState) so the
//! control channel can read them (`ui.inspect`) and clear them (`ui.set`).

use std::time::Duration;

use craft::ui::composites::{Toast, ToastEnd, ToastStack, Tone};
use serde::{Deserialize, Serialize};

use crate::PhotocraftApp;

const WAYLAND_FILE_DROP_DISMISSED: &str = "ui.waylandFileDropGuidanceDismissed";
const CLIPBOARD_UNREADABLE: &str = "Pasting from other apps is unavailable";
/// At most this many notices are kept; older ones drop off.
pub const MAX_NOTICES: usize = 3;
/// Height of the status bar the stack stays above.
const STATUS_BAR_HEIGHT: f32 = 24.0;
/// A full canvas refresh on the CPU compositor slower than this (ms) gets a notice.
const SLOW_CPU_REFRESH_MS: f64 = 1000.0;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub lines: Vec<String>,
    /// An error (warning colour) rather than an informational notice.
    #[serde(default)]
    pub error: bool,
    /// Preference key to set when this notice is dismissed.
    #[serde(default)]
    pub dismiss_pref: Option<String>,
    /// `{name}` values filled into the title and lines after they are translated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<(String, String)>,
}

impl Notice {
    /// `s` (the title or a line) in the current language, with [`Notice::args`] filled in.
    pub fn text(&self, s: &str) -> String {
        let args: Vec<(&str, &str)> = self.args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        crate::i18n::fmt(crate::i18n::t(s), &args)
    }
}

/// Show a notice (newest last); returns its id.
pub fn post(app: &mut PhotocraftApp, title: impl Into<String>, lines: Vec<String>, error: bool, dismiss_pref: Option<&str>) -> u64 {
    let id = app.ui.alloc_id();
    app.ui.notices.push(Notice { id, title: title.into(), lines, error, dismiss_pref: dismiss_pref.map(str::to_owned), args: Vec::new() });
    cap_notices(app);
    id
}

fn cap_notices(app: &mut PhotocraftApp) {
    while app.ui.notices.len() > MAX_NOTICES {
        let oldest_temporary = app.ui.notices.iter().position(|notice| notice.dismiss_pref.is_none()).unwrap_or(0);
        app.ui.notices.remove(oldest_temporary);
    }
}

/// How to paste in hints: Edit › Paste's effective shortcut (`Ctrl+V`), else the menu path.
pub(crate) fn paste_hint(app: &PhotocraftApp) -> String {
    crate::shortcuts::shortcut_label(app, "edit.paste").unwrap_or_else(|| tl!("Edit › Paste").to_string())
}

/// Show the Wayland-specific fallback guidance unless the user dismissed it in preferences: winit
/// 0.30 delivers no file drops on Wayland, so point at File › Open, pasting a copied file, and the
/// command that starts this install under XWayland (where drops work), when there is one.
pub fn wayland_file_drop_guidance(app: &mut PhotocraftApp) {
    if !app.services.is_wayland
        || app.session.prefs().dialogs.get(WAYLAND_FILE_DROP_DISMISSED).and_then(serde_json::Value::as_bool) == Some(true)
        || app.ui.notices.iter().any(|notice| notice.dismiss_pref.as_deref() == Some(WAYLAND_FILE_DROP_DISMISSED))
    {
        return;
    }
    // English templates, translated and filled in when drawn, so the notice follows a language change.
    let mut lines = vec![
        "Native file drag-and-drop is not supported on Wayland yet. Use File › Open, or copy the image in your file manager and paste it with {paste}."
            .to_owned(),
    ];
    let mut args = vec![("paste".to_owned(), paste_hint(app))];
    if let Some(command) = &app.services.xwayland_command {
        lines.push("To drop files, start PhotoCraft under XWayland: `{command}`".to_owned());
        args.push(("command".to_owned(), command.clone()));
        lines.push("Or set Preferences › Performance › Linux display server to X11: PhotoCraft then always starts under XWayland.".to_owned());
    }
    let id = post(app, "Native file drag-and-drop is unavailable", lines, false, Some(WAYLAND_FILE_DROP_DISMISSED));
    if let Some(notice) = app.ui.notices.iter_mut().find(|notice| notice.id == id) {
        notice.args = args;
    }
}

/// A paste found no clipboard to read (#5031). In a Flatpak on Wayland the sandbox hides the
/// data-control protocol that other apps' images are read through, and there is no X server to
/// fall back to, so the clipboard may hold an image all the same. Say so, and give the command
/// that starts this install under XWayland, where the X11 clipboard works, when there is one.
pub fn clipboard_unreadable(app: &mut PhotocraftApp) {
    app.ui.status = tl!("Nothing to paste: PhotoCraft can't read the system clipboard here").into();
    app.ui.status_error = false;
    if app.ui.notices.iter().any(|notice| notice.title == CLIPBOARD_UNREADABLE) {
        return;
    }
    // English templates, translated when drawn, as in `wayland_file_drop_guidance`.
    let mut lines =
        vec!["PhotoCraft can't reach the system clipboard in this session. On Wayland, a Flatpak sandbox hides the protocol that images from other apps are read through.".to_owned()];
    let mut args = Vec::new();
    if let Some(command) = &app.services.xwayland_command {
        lines.push("To paste from other apps, start PhotoCraft under XWayland: `{command}`".to_owned());
        args.push(("command".to_owned(), command.clone()));
    }
    let id = post(app, CLIPBOARD_UNREADABLE, lines, false, None);
    if let Some(notice) = app.ui.notices.iter_mut().find(|notice| notice.id == id) {
        notice.args = args;
    }
}

/// A full refresh of document `doc` fell back to the CPU compositor (`reason`, the GPU's) and took
/// `ms`. The canvas doesn't respond while it runs, so once per document say why it was slow and
/// which edits will do it again (#1015).
pub fn slow_cpu_refresh(app: &mut PhotocraftApp, doc: u64, ms: f64, reason: Option<&crate::render_path::CpuReason>) {
    if ms.is_nan() || ms < SLOW_CPU_REFRESH_MS || app.ui.slow_refresh_noticed.contains(&doc) {
        return;
    }
    app.ui.slow_refresh_noticed.push(doc);
    let seconds = format!("{:.1}", ms / 1000.0);
    let mut lines = vec![
        crate::i18n::fmt(tl!("This redraw took {seconds} s on the CPU."), &[("seconds", &seconds)]),
        tl!("Edits that change the whole document, such as a layer's blend mode, opacity or visibility, redraw all of it.").into(),
    ];
    if let Some(&crate::render_path::CpuReason::OverBudget { needed, budget }) = reason {
        if needed > budget {
            lines.push(crate::i18n::fmt(
                tl!("Its layers need {needed} of GPU memory; the budget leaves them {budget}."),
                &[("needed", &crate::chrome_ui::fmt_bytes(needed)), ("budget", &crate::chrome_ui::fmt_bytes(budget))],
            ));
        }
        lines.push(
            tl!("Set GPU Memory in Preferences › Performance to what your graphics card can hold, at most 3/8 of its memory. Automatic follows Memory Usage, within a quarter of this computer's memory and 3/8 of the card's, and doesn't count the document against a card whose memory is known. The status bar shows GPU Memory Full while they don't fit.")
                .into(),
        );
    } else if let Some(r) = reason {
        lines.push(crate::i18n::fmt(tl!("The GPU compositor wasn't used: {reason}"), &[("reason", &r.to_string())]));
    }
    post(app, tl!("Large document: redrawing on the CPU"), lines, false, None);
}

fn dismiss(app: &mut PhotocraftApp, id: u64) {
    let dismiss_pref = app.ui.notices.iter().find(|notice| notice.id == id).and_then(|notice| notice.dismiss_pref.clone());
    app.ui.notices.retain(|notice| notice.id != id);
    if let Some(key) = dismiss_pref {
        app.session.prefs.edit(|prefs| {
            prefs.dialogs.insert(key, serde_json::Value::Bool(true));
        });
    }
}

/// Report import/export `warnings` for the file operation `what` (e.g. "Opened a.psd"): the status
/// bar says how many there were, and a notice lists them. Nothing happens when there are none.
pub fn io_warnings(app: &mut PhotocraftApp, what: &str, warnings: &[String]) {
    let Some(first) = warnings.first() else { return };
    let n = warnings.len();
    let title = crate::i18n::fmt(&crate::i18n::trn(crate::i18n::current(), n as u64, "{what} with {n} warning", "{what} with {n} warnings"), &[("what", what)]);
    app.ui.status = if n == 1 { format!("{what}: {first}") } else { format!("{title}: {first} …") };
    app.ui.status_error = true;
    post(app, title, warnings.to_vec(), false, None);
}

/// Report a failed file operation: the status bar shows it as an error and a notice keeps it on
/// screen until dismissed.
pub fn error(app: &mut PhotocraftApp, message: String) {
    app.ui.status = message.clone();
    app.ui.status_error = true;
    post(app, message, Vec::new(), true, None);
}

/// Draw the notices as a toast stack above the status bar; each has a close button. When
/// Interface › Notification › Auto Hide Notices is on, a notice disappears once its delay is up
/// unless the pointer rests on the stack (#2022).
pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if app.ui.notices.is_empty() {
        return;
    }
    let (auto_hide, duration) = {
        let i = &app.session.prefs().interface;
        (i.notification_auto_hide, u64::from(i.notification_duration_seconds.max(1)))
    };
    // Clear the status bar (~24 px); the stack leaves its own inset above that.
    let mut body = ctx.content_rect();
    body.max.y -= STATUS_BAR_HEIGHT;
    let toasts: Vec<Toast> = app
        .ui
        .notices
        .iter()
        .map(|n| {
            Toast::new(n.id, n.text(&n.title))
                .tone(if n.error { Tone::Warn } else { Tone::Neutral })
                .lines(n.lines.iter().map(|line| n.text(line)).collect::<Vec<_>>())
                .sticky(n.dismiss_pref.is_some())
        })
        .collect();
    let ended = ToastStack::new("photocraft-notices")
        .auto_hide(auto_hide.then(|| Duration::from_secs(duration)))
        .dismiss_label(tl!("Dismiss"))
        .more_label(|n| crate::i18n::fmt(tl!("…and {n} more"), &[("n", &n.to_string())]))
        .show(ctx, body, toasts);
    for (id, end) in ended {
        match end {
            ToastEnd::Closed => dismiss(app, id),
            ToastEnd::Expired | ToastEnd::Dropped => app.ui.notices.retain(|n| n.id != id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PhotocraftApp, Services};
    use photocraft_engine::Session;
    use std::sync::{Arc, Mutex};

    #[test]
    fn wayland_guidance_follows_language_changes_after_startup() {
        let command = Some("WAYLAND_DISPLAY= photocraft".to_string());
        let app = PhotocraftApp::new(Session::new(), Services { is_wayland: true, xwayland_command: command, ..Default::default() });
        let notice = &app.ui.notices[0];
        // the notice keeps its English source text and is translated when drawn (`i18n::t` with
        // the current language)
        assert_eq!(notice.title, "Native file drag-and-drop is unavailable");
    }

    /// #5031: a paste that can't reach the clipboard (a Flatpak on Wayland) says why and how to
    /// paste anyway, once while shown; a clipboard that simply holds no image stays a quiet no-op.
    #[test]
    fn unreadable_clipboard_paste_explains_the_sandbox() {
        let command = "flatpak run --nosocket=wayland --socket=x11 ai.storyteller.photocraft";
        let services = |unreadable: bool| Services {
            clipboard_get_image: Some(Box::new(move || if unreadable { Err("no clipboard backend".into()) } else { Ok(None) })),
            clipboard_unreadable: Some(Box::new(move || unreadable)),
            xwayland_command: Some(command.to_string()),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut app = PhotocraftApp::new(Session::new(), services(true));
        for _ in 0..2 {
            let r = crate::menus::invoke(&mut app, &ctx, "edit.paste", serde_json::json!({})).unwrap();
            assert_eq!(r["pasted"], serde_json::json!(false));
        }
        assert_eq!(app.ui.status, "Nothing to paste: PhotoCraft can't read the system clipboard here");
        let shown: Vec<_> = app.ui.notices.iter().filter(|notice| notice.title == CLIPBOARD_UNREADABLE).collect();
        assert_eq!(shown.len(), 1, "a second paste doesn't stack another notice");
        let guidance = english(shown[0]);
        assert!(guidance.contains(&format!("under XWayland: `{command}`")), "{guidance}");

        let mut empty = PhotocraftApp::new(Session::new(), services(false));
        crate::menus::invoke(&mut empty, &ctx, "edit.paste", serde_json::json!({})).unwrap();
        assert_eq!(empty.ui.status, "Nothing to paste: the clipboard holds no image");
        assert!(empty.ui.notices.iter().all(|notice| notice.title != CLIPBOARD_UNREADABLE));
    }

    /// A notice's lines in English with their arguments filled in.
    fn english(notice: &Notice) -> String {
        let args: Vec<(&str, &str)> = notice.args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        notice.lines.iter().map(|line| crate::i18n::fmt(line, &args)).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn wayland_guidance_is_only_shown_in_wayland_sessions() {
        let command = Some("WAYLAND_DISPLAY= '/apps/Photo Craft.AppImage'".to_string());
        let mut app = PhotocraftApp::new(Session::new(), Services { is_wayland: true, xwayland_command: command, ..Default::default() });
        assert_eq!(app.ui.notices.len(), 1);
        assert_eq!(app.ui.notices[0].title, "Native file drag-and-drop is unavailable");
        let guidance = english(&app.ui.notices[0]);
        assert!(guidance.contains("not supported on Wayland yet"));
        assert!(guidance.contains("File › Open"));
        // Pasting a copied file works on Wayland (#338), so the notice offers it.
        let paste = paste_hint(&app);
        assert!(guidance.contains(&format!("paste it with {paste}")), "{guidance}");
        // The relaunch command is the one for this install (an AppImage here), not a guess.
        assert!(guidance.contains("under XWayland: `WAYLAND_DISPLAY= '/apps/Photo Craft.AppImage'`"), "{guidance}");
        assert!(guidance.contains("Linux display server to X11"), "{guidance}");
        assert_eq!(app.ui.notices[0].dismiss_pref.as_deref(), Some(WAYLAND_FILE_DROP_DISMISSED));
        for i in 0..MAX_NOTICES {
            post(&mut app, format!("Transient {i}"), Vec::new(), false, None);
        }
        assert_eq!(app.ui.notices.len(), MAX_NOTICES);
        assert!(app.ui.notices.iter().any(|notice| notice.dismiss_pref.is_some()));

        let app = PhotocraftApp::new(Session::new(), Services::default());
        assert!(app.ui.notices.is_empty());
    }

    #[test]
    fn wayland_guidance_without_an_x_server_does_not_suggest_xwayland() {
        let app = PhotocraftApp::new(Session::new(), Services { is_wayland: true, ..Default::default() });
        let guidance = english(&app.ui.notices[0]);
        assert!(guidance.contains("File › Open"));
        assert!(!guidance.contains("XWayland"), "{guidance}");
    }

    #[test]
    fn paste_hint_follows_the_shortcut_and_falls_back_to_the_menu() {
        let mut app = PhotocraftApp::new(Session::new(), Services::default());
        assert_eq!(paste_hint(&app), crate::shortcuts::shortcut_label(&app, "edit.paste").unwrap());
        app.session.prefs.edit(|prefs| {
            prefs.shortcuts.insert("edit.paste".into(), String::new());
        });
        assert_eq!(paste_hint(&app), "Edit › Paste");
    }

    #[test]
    fn dismissing_wayland_guidance_persists_across_launches() {
        let saved = Arc::new(Mutex::new(None::<String>));
        let load_store = saved.clone();
        let save_store = saved.clone();
        let services = Services {
            is_wayland: true,
            load_prefs: Some(Box::new(move || load_store.lock().unwrap_or_else(|e| e.into_inner()).clone())),
            save_prefs: Some(Box::new(move |text| {
                *save_store.lock().unwrap_or_else(|e| e.into_inner()) = Some(text.to_owned());
                Ok(())
            })),
            ..Default::default()
        };
        let mut app = PhotocraftApp::new(Session::new(), services);
        let id = app.ui.notices[0].id;
        dismiss(&mut app, id);
        crate::prefs_ui::tick(&mut app, &egui::Context::default());

        let services =
            Services { is_wayland: true, load_prefs: Some(Box::new(move || saved.lock().unwrap_or_else(|e| e.into_inner()).clone())), ..Default::default() };
        let app = PhotocraftApp::new(Session::new(), services);
        assert!(app.ui.notices.is_empty());
    }

    #[test]
    fn a_slow_cpu_refresh_is_explained_once_per_document() {
        let mut app = PhotocraftApp::new(Session::new(), Services::default());
        let over = crate::render_path::CpuReason::OverBudget { needed: 3600 << 20, budget: 3400 << 20 };
        slow_cpu_refresh(&mut app, 1, 400.0, Some(&over));
        slow_cpu_refresh(&mut app, 1, f64::NAN, Some(&over));
        assert!(app.ui.notices.is_empty(), "a quick refresh needs no explanation");

        // The layers of document 1 need 3.6 GB and may hold 3.4 GB: the notice says so.
        slow_cpu_refresh(&mut app, 1, 55_000.0, Some(&over));
        assert_eq!(app.ui.notices.len(), 1);
        let text = app.ui.notices[0].lines.join(" ");
        assert!(text.contains("55.0 s"), "{text}");
        assert!(text.contains("blend mode, opacity or visibility"), "{text}");
        assert!(text.contains("need 3.5G of GPU memory; the budget leaves them 3.3G"), "{text}");
        assert!(text.contains("GPU Memory in Preferences › Performance"), "{text}");
        assert!(!app.ui.notices[0].error);

        slow_cpu_refresh(&mut app, 1, 55_000.0, Some(&over));
        assert_eq!(app.ui.notices.len(), 1, "once per document");

        slow_cpu_refresh(&mut app, 2, 3_000.0, Some(&crate::render_path::CpuReason::Unsupported("Blend If on `Overlay` (composited on the CPU)".into())));
        assert_eq!(app.ui.notices.len(), 2);
        let text = app.ui.notices[1].lines.join(" ");
        assert!(text.contains("Blend If on `Overlay`"), "{text}");
        assert!(!text.contains("Memory usage"), "the budget hint is only for the budget fallback: {text}");
    }

    #[test]
    fn import_warning_punctuation_is_localized_without_changing_english() {
        let mut app = PhotocraftApp::new(Session::new(), Services::default());
        for lang in [crate::i18n::Lang::EN, crate::i18n::Lang::from_code("cs").unwrap()] {
            crate::i18n::with_language(lang, || {
                let what = crate::i18n::fmt(tl!("Opened {name}"), &[("name", "x.psd")]);
                let warnings = vec!["first warning".into(), "second warning".into(), "third warning".into()];
                io_warnings(&mut app, &what, &warnings);
                if lang == crate::i18n::Lang::EN {
                    assert_eq!(app.ui.status, "Opened x.psd with 3 warnings: first warning …");
                }
                let title = crate::i18n::fmt(&crate::i18n::trn(lang, 3, "{what} with {n} warning", "{what} with {n} warnings"), &[("what", &what)]);
                assert_eq!(app.ui.status, format!("{title}: first warning …"));
                assert_eq!(app.ui.notices.last().unwrap().lines, warnings);
                io_warnings(&mut app, &what, &warnings[..1]);
                assert_eq!(app.ui.status, format!("{what}: first warning"));
            });
        }
    }

    #[test]
    fn dismissing_a_notice_writes_its_own_preference_key() {
        let mut app = PhotocraftApp::new(Session::new(), Services::default());
        post(&mut app, "Dismissible", Vec::new(), false, Some("ui.testNoticeDismissed"));
        let id = app.ui.notices[0].id;
        dismiss(&mut app, id);
        assert_eq!(app.session.prefs().dialogs.get("ui.testNoticeDismissed").and_then(serde_json::Value::as_bool), Some(true));
    }

    /// #2022 review: the frame that drops a notice has already painted it, so it must ask for
    /// another frame at once; otherwise the card lingers until unrelated input.
    #[test]
    fn expiring_a_notice_requests_a_frame_to_clear_it() {
        let mut app = PhotocraftApp::new(Session::new(), Services::default());
        post(&mut app, "Transient", Vec::new(), false, None);
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(Default::default(), |ui| show(&mut app, ui.ctx()));
        out.textures_delta.clear();
        assert_eq!(app.ui.notices.len(), 1);
        // Past the 6 s default: the notice is dropped and a repaint is requested in the same frame.
        let input = egui::RawInput { time: Some(10.0), ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| show(&mut app, ui.ctx()));
        out.textures_delta.clear();
        assert!(app.ui.notices.is_empty());
        assert!(ctx.has_requested_repaint(), "the frame that drops a notice asks for another");
    }
}
