//! One background check at startup and the library's check dialog for release information.

use photocraft_engine::updates_cmds::Status;
use serde_json::Value;

use crate::PhotocraftApp;

pub(crate) struct Runtime {
    startup_checked: bool,
    notified_version: Option<String>,
    /// The self-update behind the check dialog (desktop).
    #[cfg(not(target_arch = "wasm32"))]
    updater: craft::update::updater::Updater,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            startup_checked: false,
            notified_version: None,
            #[cfg(not(target_arch = "wasm32"))]
            updater: craft::update::updater::Updater::new(&crate::UPDATE_APP, photocraft_engine::build_info::VERSION),
        }
    }
}

/// The release the engine found, as `craft-update` knows it.
#[cfg(not(target_arch = "wasm32"))]
fn release_of(found: &photocraft_engine::updates_cmds::Release) -> craft::update::Release {
    craft::update::Release {
        tag: format!("v{}", found.version),
        version: craft::update::parse_version(&found.version),
        name: format!("PhotoCraft {}", found.version),
        html_url: found.url.clone(),
        ..Default::default()
    }
}

pub(crate) fn check(app: &mut PhotocraftApp, _ctx: &egui::Context, params: Value) -> Result<Value, String> {
    let before = app.session.updates.status.clone();
    let result = app.run("help.checkForUpdates", params);
    // A check that fails to start still shows its failure and Check Again; the command error
    // stands. Only this call's failure: a Failed left from an earlier check doesn't open the
    // window for a call refused for another reason (bad params).
    let failed_to_start = match (&result, &app.session.updates.status) {
        (Err(error), status @ Status::Failed { message }) => *status != before || (!message.is_empty() && error.contains(message.as_str())),
        _ => false,
    };
    if result.is_ok() || failed_to_start {
        app.ui.updates_open = true;
        app.updates_rt.startup_checked = true;
    }
    result
}

pub(crate) fn tick(app: &mut PhotocraftApp, ctx: &egui::Context) {
    // `updates.status` also polls (automation may consume the result before this frame).
    app.session.updates.poll();
    if let Status::Available { release } = &app.session.updates.status
        && app.updates_rt.notified_version.as_ref() != Some(&release.version)
    {
        app.updates_rt.notified_version = Some(release.version.clone());
        app.ui.updates_open = true;
        #[cfg(not(target_arch = "wasm32"))]
        app.updates_rt.updater.offer(release_of(release));
    }
    #[cfg(not(target_arch = "wasm32"))]
    push_state(app, ctx);
    if app.session.updates.check.is_none() {
        return;
    }
    if !app.updates_rt.startup_checked {
        app.updates_rt.startup_checked = true;
        if app.session.prefs().general.automatically_check_updates && matches!(app.session.updates.status, Status::Idle) {
            let _ = app.run("help.checkForUpdates", serde_json::json!({}));
        }
    }
    if matches!(app.session.updates.status, Status::Checking) {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// Opens, closes and feeds the check dialog before the dialog host draws, and starts the part of
/// the finished install that waits for saved work.
#[cfg(not(target_arch = "wasm32"))]
fn push_state(app: &mut PhotocraftApp, ctx: &egui::Context) {
    use craft::update::stack_dialogs::UpdateCheckDialog;
    let rt = &mut app.updates_rt;
    rt.updater.tick(ctx);
    if !app.ui.updates_open {
        if app.ui.host.is_open::<UpdateCheckDialog>() {
            app.ui.host.cancel::<UpdateCheckDialog>(ctx);
        }
        return;
    }
    if !app.ui.host.is_open::<UpdateCheckDialog>() {
        app.ui.host.open(UpdateCheckDialog::new(photocraft_engine::build_info::VERSION));
    }
    let status = match &app.session.updates.status {
        Status::Idle | Status::Available { .. } => craft::update::updater::Status::Idle,
        Status::Checking => craft::update::updater::Status::Checking,
        Status::UpToDate => craft::update::updater::Status::UpToDate,
        Status::Failed { message } => craft::update::updater::Status::Failed(message.clone()),
    };
    let available = match &app.session.updates.status {
        Status::Available { release } => Some(release_of(release)),
        _ => None,
    };
    let (installing, message) = (app.updates_rt.updater.is_installing(), app.updates_rt.updater.status_message());
    if let Some(dialog) = app.ui.host.get_mut::<UpdateCheckDialog>() {
        dialog.set_status(status);
        dialog.set_available(available);
        dialog.set_installing(installing);
        dialog.set_message(message);
    }
}

/// The id `ui.inspect` lists the check dialog under, and `ui.dialog.confirm` and `ui.dialog.cancel`
/// take.
pub const CONTROL_ID: &str = "updates";

/// The check dialog as `ui.inspect` lists it: the engine's status, the release on offer, the
/// install's progress and the choices `ui.dialog.confirm` takes in this state.
pub fn inspect(app: &PhotocraftApp) -> Option<Value> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if !app.ui.host.is_open::<craft::update::stack_dialogs::UpdateCheckDialog>() {
            return None;
        }
        let (state, version, choices): (&str, Option<&str>, &[&str]) = match &app.session.updates.status {
            Status::Idle => ("idle", None, &["checkNow", "close"]),
            Status::Checking => ("checking", None, &["close"]),
            Status::UpToDate => ("upToDate", None, &["checkAgain", "close"]),
            Status::Failed { .. } => ("failed", None, &["checkAgain", "close"]),
            Status::Available { release } => ("available", Some(release.version.as_str()), &["update", "download", "checkAgain", "close"]),
        };
        let message = match &app.session.updates.status {
            Status::Failed { message } => Some(message.clone()),
            _ => app.updates_rt.updater.status_message(),
        };
        Some(serde_json::json!({
            "id": CONTROL_ID,
            "kind": "updates",
            "title": "Check for Updates",
            "fields": {"state": state, "version": version, "installing": app.updates_rt.updater.is_installing(), "message": message, "choices": choices},
        }))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = app;
        None
    }
}

/// Answers the check dialog for a control client with one of the choices [`inspect`] lists, as
/// pressing that button would. A choice the dialog's state doesn't offer is refused.
pub fn answer_control(app: &mut PhotocraftApp, ctx: &egui::Context, choice: &str) -> Result<Value, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use craft::update::stack_dialogs::{CheckChoice, UpdateCheckDialog};
        let answer = match choice {
            "close" => CheckChoice::Close,
            "checkNow" => CheckChoice::CheckNow,
            "checkAgain" => CheckChoice::CheckAgain,
            "update" => CheckChoice::Update,
            "download" => CheckChoice::Download,
            other => return Err(format!("unknown choice `{other}`: close, checkNow, checkAgain, update or download")),
        };
        let id = app.ui.host.top_id().filter(|_| app.ui.host.is_open::<UpdateCheckDialog>()).ok_or("the update dialog is not open")?;
        app.ui.host.get_mut::<UpdateCheckDialog>().ok_or("the update dialog is not open")?.set_choice(answer);
        if !app.ui.host.confirm_by_id::<UpdateCheckDialog>(ctx, id) {
            return Err(format!("the update dialog does not offer `{choice}` now"));
        }
        show(app, ctx);
        Ok(serde_json::json!({"choice": choice}))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (app, ctx, choice);
        Err("the update dialog is not open".into())
    }
}

/// Closes the check dialog for `ui.dialog.cancel`.
pub fn cancel_control(app: &mut PhotocraftApp) -> Result<Value, String> {
    if !app.ui.updates_open {
        return Err("the update dialog is not open".into());
    }
    app.ui.updates_open = false;
    Ok(Value::Null)
}

/// The dialog's answer, after the dialog host drew.
pub(crate) fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use craft::ui::dialog::DialogResult;
        use craft::update::stack_dialogs::{CheckChoice, UpdateCheckDialog};
        if let Some(result) = app.ui.host.take::<UpdateCheckDialog>() {
            match result {
                DialogResult::Ok(CheckChoice::CheckNow | CheckChoice::CheckAgain) => {
                    let _ = check(app, ctx, serde_json::json!({}));
                }
                // The window stays open and shows the progress; a refusal for unsaved work is
                // the updater's status line.
                DialogResult::Ok(CheckChoice::Update) => {
                    let can_start = !app.has_unsaved_work();
                    app.updates_rt.updater.request_install(ctx, can_start);
                }
                DialogResult::Ok(CheckChoice::Download) => {
                    app.ui.updates_open = false;
                    if let Status::Available { release } = &app.session.updates.status {
                        let url = release.url.clone();
                        crate::links::open(app, ctx, &url);
                    }
                }
                // Release Notes is answered inside the dialog.
                _ => app.ui.updates_open = false,
            }
        }
        // The restart helper is scheduled before the app closes, so no update starts over unsaved
        // work: the close waits until the work is saved.
        {
            let can_close = !app.has_unsaved_work();
            if app.updates_rt.updater.take_close(can_close) {
                // The guarded close path asks about unsaved work and waits for saves; the restart
                // helper relaunches once the process exits.
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (app, ctx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Services, menus};
    #[cfg(not(target_arch = "wasm32"))]
    use craft::update::stack_dialogs::{CheckChoice, UpdateCheckDialog};
    use photocraft_engine::{Session, updates_cmds::Release};
    use std::sync::{Arc, Mutex, mpsc};

    type Sender = Arc<Mutex<Option<mpsc::Sender<photocraft_engine::updates_cmds::CheckResult>>>>;

    fn app() -> (PhotocraftApp, Arc<Mutex<u32>>, Sender) {
        let calls = Arc::new(Mutex::new(0));
        let count = calls.clone();
        let sender = Arc::new(Mutex::new(None));
        let tx = sender.clone();
        let app = PhotocraftApp::new(
            Session::new(),
            Services {
                check_updates: Some(Box::new(move || {
                    *count.lock().unwrap() += 1;
                    let (sender, receiver) = mpsc::channel();
                    *tx.lock().unwrap() = Some(sender);
                    Ok(receiver)
                })),
                ..Default::default()
            },
        );
        (app, calls, sender)
    }

    fn at_ctx(app: &mut PhotocraftApp, ctx: &egui::Context) {
        tick(app, ctx);
        app.ui.host.show(ctx);
        show(app, ctx);
    }

    fn at(app: &mut PhotocraftApp, ctx: &egui::Context, time: f64) {
        let mut output = ctx.run_ui(egui::RawInput { time: Some(time), ..Default::default() }, |ui| tick(app, ui.ctx()));
        output.textures_delta.clear();
    }

    #[test]
    fn automatic_check_runs_once_per_launch_even_after_days_or_preference_changes() {
        let (mut app, calls, sender) = app();
        let ctx = egui::Context::default();
        at(&mut app, &ctx, 0.0);
        at(&mut app, &ctx, 1.0);
        at(&mut app, &ctx, 2.0);
        assert_eq!(*calls.lock().unwrap(), 1);
        sender.lock().unwrap().as_ref().unwrap().send(Ok(None)).unwrap();
        at(&mut app, &ctx, 3.0);
        assert!(!app.ui.updates_open, "automatic up-to-date checks stay quiet");
        at(&mut app, &ctx, 86400.0);
        app.session.edit_prefs(|p| p.general.automatically_check_updates = false);
        at(&mut app, &ctx, 86401.0);
        app.session.edit_prefs(|p| p.general.automatically_check_updates = true);
        at(&mut app, &ctx, 172800.0);
        assert_eq!(*calls.lock().unwrap(), 1, "no periodic checks or preference-triggered rechecks");
        let (mut restarted, restarted_calls, _) = self::app();
        at(&mut restarted, &ctx, 0.0);
        assert_eq!(*restarted_calls.lock().unwrap(), 1, "the next launch checks again");
    }

    #[test]
    fn startup_opt_out_and_failed_checks_do_not_retry_automatically() {
        let (mut disabled, calls, _) = app();
        let ctx = egui::Context::default();
        disabled.session.edit_prefs(|p| p.general.automatically_check_updates = false);
        at(&mut disabled, &ctx, 0.0);
        disabled.session.edit_prefs(|p| p.general.automatically_check_updates = true);
        at(&mut disabled, &ctx, 86400.0);
        assert_eq!(*calls.lock().unwrap(), 0, "opt-out is evaluated at startup");

        let (mut failed, calls, sender) = app();
        at(&mut failed, &ctx, 0.0);
        sender.lock().unwrap().as_ref().unwrap().send(Err("offline".into())).unwrap();
        at(&mut failed, &ctx, 1.0);
        at(&mut failed, &ctx, 172800.0);
        assert_eq!(*calls.lock().unwrap(), 1);
        assert!(!failed.ui.updates_open, "an automatic failure stays quiet");
    }

    #[test]
    fn manual_check_works_when_automatic_checks_are_off_and_opens_status() {
        let (mut app, calls, sender) = app();
        let ctx = egui::Context::default();
        app.session.edit_prefs(|p| p.general.automatically_check_updates = false);
        assert!(menus::is_enabled(&app, "help.checkForUpdates"));
        menus::invoke(&mut app, &ctx, "help.checkForUpdates", serde_json::json!({})).unwrap();
        assert!(app.ui.updates_open);
        assert_eq!(*calls.lock().unwrap(), 1);
        sender.lock().unwrap().as_ref().unwrap().send(Err("offline".into())).unwrap();
        at(&mut app, &ctx, 1.0);
        assert!(matches!(app.session.updates.status, Status::Failed { .. }));
        let mut web = PhotocraftApp::new(Session::new(), Services::default());
        assert!(!menus::is_enabled(&web, "help.checkForUpdates"));
        at(&mut web, &ctx, 2.0);
        assert_eq!(web.session.updates.status, Status::Idle);
    }

    #[test]
    fn available_release_notifies_once_even_when_automation_polls_first() {
        let (mut app, _, sender) = app();
        let ctx = egui::Context::default();
        at(&mut app, &ctx, 0.0);
        sender
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .send(Ok(Some(Release { version: "99.0.0".into(), url: "https://github.com/storytold/photocraft/releases/tag/v99.0.0".into() })))
            .unwrap();
        app.run("updates.status", serde_json::json!({})).unwrap();
        at(&mut app, &ctx, 1.0);
        assert!(app.ui.updates_open);
        app.ui.updates_open = false;
        at(&mut app, &ctx, 2.0);
        assert!(!app.ui.updates_open);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn startup_update_prompt_appears_and_can_be_dismissed() {
        let (mut app, _, sender) = app();
        let ctx = egui::Context::default();
        at(&mut app, &ctx, 0.0);
        sender
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .send(Ok(Some(Release { version: "99.0.0".into(), url: "https://github.com/storytold/photocraft/releases/tag/v99.0.0".into() })))
            .unwrap();
        at(&mut app, &ctx, 1.0);
        assert!(app.ui.updates_open);
        assert!(app.ui.host.is_open::<UpdateCheckDialog>(), "the available release opens the check dialog");

        let frame = |app: &mut PhotocraftApp| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                at_ctx(app, ui.ctx());
            });
            output.textures_delta.clear();
        };
        frame(&mut app);
        assert!(app.ui.host.is_open::<UpdateCheckDialog>(), "still open after a frame");
        // Later closes it.
        assert!(app.ui.host.cancel::<UpdateCheckDialog>(&ctx));
        show(&mut app, &ctx);
        assert!(!app.ui.updates_open);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn update_and_restart_on_an_unsupported_install_shows_the_updaters_status() {
        let (mut app, _, sender) = app();
        let ctx = egui::Context::default();
        at(&mut app, &ctx, 0.0);
        app.ui.updates_open = true;
        app.ui.host.open(UpdateCheckDialog::new(photocraft_engine::build_info::VERSION).with_self_updating(true));
        sender
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .send(Ok(Some(Release { version: "99.0.0".into(), url: "https://github.com/storytold/photocraft/releases/tag/v99.0.0".into() })))
            .unwrap();
        at(&mut app, &ctx, 1.0);
        let frame = |app: &mut PhotocraftApp| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| at_ctx(app, ui.ctx()));
            output.textures_delta.clear();
        };
        frame(&mut app);
        answer_control(&mut app, &ctx, "update").unwrap();
        frame(&mut app);
        assert!(app.ui.updates_open, "the dialog stays for the install's status");
        assert!(app.updates_rt.updater.status_message().is_some());
    }

    #[test]
    fn a_manual_check_that_fails_to_start_opens_the_status_window() {
        let mut app =
            PhotocraftApp::new(Session::new(), Services { check_updates: Some(Box::new(|| Err("Could not start worker".into()))), ..Default::default() });
        let ctx = egui::Context::default();
        let error = menus::invoke(&mut app, &ctx, "help.checkForUpdates", serde_json::json!({})).unwrap_err();
        assert!(error.contains("Could not start worker"), "{error}");
        assert!(matches!(app.session.updates.status, Status::Failed { .. }));
        assert!(app.ui.updates_open, "the failure and Check Again are shown");
        // Bad params are a caller error, not a failed check: nothing opens, also while the earlier
        // failure is still the status.
        app.ui.updates_open = false;
        assert!(menus::invoke(&mut app, &ctx, "help.checkForUpdates", serde_json::json!({"x": 1})).is_err());
        assert!(matches!(app.session.updates.status, Status::Failed { .. }), "the earlier failure stays");
        assert!(!app.ui.updates_open, "bad params after a failed check");
        app.session.updates.status = Status::Idle;
        assert!(menus::invoke(&mut app, &ctx, "help.checkForUpdates", serde_json::json!({"x": 1})).is_err());
        assert!(!app.ui.updates_open);
        // The same start failure again opens it again.
        app.session.updates.status = Status::Failed { message: "Could not start worker".into() };
        assert!(menus::invoke(&mut app, &ctx, "help.checkForUpdates", serde_json::json!({})).is_err());
        assert!(app.ui.updates_open, "a repeated start failure");
    }

    #[test]
    fn update_window_is_drivable_and_bad_state_changes_are_atomic() {
        let (mut app, _, _) = app();
        let ctx = egui::Context::default();
        let call = |app: &mut PhotocraftApp, method: &str, params: Value| {
            let (req, _) = crate::control::ControlRequest::new(method, params);
            match crate::control::handle(app, &ctx, &req) {
                crate::control::Outcome::Done(value) => value,
                _ => panic!("expected an immediate reply"),
            }
        };
        assert_eq!(call(&mut app, "ui.set", serde_json::json!({"updatesOpen": true}))["ok"], true);
        let inspected = call(&mut app, "ui.inspect", serde_json::json!({}));
        assert_eq!(inspected["result"]["updatesOpen"], true);
        assert_eq!(inspected["result"]["updates"]["supported"], true);
        assert_eq!(call(&mut app, "ui.set", serde_json::json!({"updatesOpen": false, "maskTarget": "bad"}))["ok"], false);
        assert!(app.ui.updates_open);
        assert_eq!(call(&mut app, "ui.set", serde_json::json!({"updatesOpen": false}))["ok"], true);
        assert!(!app.ui.updates_open);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn the_check_dialog_is_listed_and_answered_through_the_control_routes() {
        let (mut app, _, _) = app();
        let ctx = egui::Context::default();
        let call = |app: &mut PhotocraftApp, method: &str, params: Value| {
            let (req, _) = crate::control::ControlRequest::new(method, params);
            match crate::control::handle(app, &ctx, &req) {
                crate::control::Outcome::Done(value) => value,
                _ => panic!("expected an immediate reply"),
            }
        };
        call(&mut app, "ui.set", serde_json::json!({"updatesOpen": true}));
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| at_ctx(&mut app, ui.ctx()));
        output.textures_delta.clear();
        let inspected = call(&mut app, "ui.inspect", serde_json::json!({}));
        let listed = inspected["result"]["dialogs"].as_array().unwrap().iter().find(|d| d["id"] == CONTROL_ID).cloned().expect("the update dialog is listed");
        assert_eq!(listed["fields"]["choices"], serde_json::json!(["close"]));
        assert_eq!(call(&mut app, "ui.dialog.confirm", serde_json::json!({"dialog": CONTROL_ID, "choice": "update"}))["ok"], false);
        assert_eq!(call(&mut app, "ui.dialog.confirm", serde_json::json!({"dialog": CONTROL_ID, "choice": "close"}))["ok"], true);
        assert!(!app.ui.updates_open);
    }
}
