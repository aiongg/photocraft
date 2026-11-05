//! Save / Don't Save / Cancel prompt for actions that would throw away unsaved changes: closing
//! documents, Revert, and leaving the app (File › Exit or the window's close button).
//!
//! The action is parked in [`Prompt`] while the question is up and re-run once every affected
//! document has been answered. A library [`CloseGuard`] steps through the documents, and the
//! question is the library's [`UnsavedChangesDialog`] on the dialog host. Cancel at any point drops
//! the action. Documents are tracked by id, not tab index, so closing one elsewhere while the
//! prompt is up can't retarget it.

use craft::quit::{CloseAnswer, CloseGuard, CloseScope, GuardStep};
use craft::ui::dialog::{DialogResult, UnsavedChangesDialog};
use photocraft_doc::DocId;
use serde_json::{Value, json};

use crate::PhotocraftApp;

const EXIT: &str = "file.exit";

/// An action waiting on the user, and the dirty documents still to ask about.
pub struct Prompt {
    id: String,
    params: Value,
    /// The document the command was aimed at (`document` param, else the active one), if it has one.
    target: Option<DocId>,
    /// Steps through the documents still to ask about, the one asked now first: what each answer leads to.
    guard: CloseGuard<DocId>,
    /// The first document is being saved: the question is closed until the save ends.
    saving: bool,
}

impl Prompt {
    fn new(id: &str, params: Value, target: Option<DocId>, docs: Vec<DocId>) -> Self {
        let scope = match id {
            EXIT => CloseScope::Quit,
            "file.revert" => CloseScope::Revert,
            _ => CloseScope::Close,
        };
        Self { id: id.to_string(), params, target, guard: CloseGuard::new(scope, docs), saving: false }
    }
}

fn index_of(app: &PhotocraftApp, id: DocId) -> Option<usize> {
    app.session.documents().iter().position(|d| d.doc.id == id)
}

/// Which documents a command discards, relative to its target (the `document` param, else the active one).
#[derive(Clone, Copy)]
enum Reach {
    Target,
    AllButTarget,
    All,
}

/// Every command that can throw away unsaved changes. A new one only needs a row here.
const DISCARDING: &[(&str, Reach)] = &[
    ("file.close", Reach::Target),
    ("file.revert", Reach::Target),
    ("file.closeOthers", Reach::AllButTarget),
    ("file.closeAll", Reach::All),
    (EXIT, Reach::All),
];

fn reach(id: &str) -> Option<Reach> {
    DISCARDING.iter().find(|(c, _)| *c == id).map(|(_, r)| *r)
}

/// The command's target document and the unsaved documents it would discard (empty for commands
/// that discard nothing).
fn discarded(app: &PhotocraftApp, id: &str, params: &Value) -> (Option<DocId>, Vec<DocId>) {
    let docs = app.session.documents();
    let target = params.get("document").and_then(Value::as_u64).map(|i| i as usize).or(app.session.active_index());
    let affected: Vec<usize> = match reach(id) {
        Some(Reach::Target) => target.into_iter().collect(),
        Some(Reach::AllButTarget) => (0..docs.len()).filter(|&i| Some(i) != target).collect(),
        Some(Reach::All) => (0..docs.len()).collect(),
        None => Vec::new(),
    };
    let dirty = affected.into_iter().filter_map(|i| docs.get(i)).filter(|d| d.is_dirty()).map(|d| d.doc.id).collect();
    (target.and_then(|i| docs.get(i)).map(|d| d.doc.id), dirty)
}

/// Park `id` behind a prompt if it would discard unsaved work. Returns whether it did.
pub fn intercept(app: &mut PhotocraftApp, id: &str, params: &Value) -> bool {
    let (target, docs) = discarded(app, id, params);
    if docs.is_empty() {
        return false;
    }
    // Once the window's GPU device is lost nothing can be drawn (`gpu_status::check_window`), so
    // the prompt would never show. While saving for recovery keeps failing, quitting (the close
    // button, File › Exit) must still go through rather than leave a blank window that can't be
    // closed.
    if id == EXIT && app.window_lost {
        log::warn!("quitting a window that lost its GPU device before its unsaved work could be saved for recovery");
        return false;
    }
    let prompt = Prompt::new(id, params.clone(), target, docs);
    match &app.discard {
        None => ask(app, prompt),
        // Quitting overrides whatever is pending: it covers every document, so nothing is lost.
        Some(open) if id == EXIT && open.id != EXIT => ask(app, prompt),
        // Repeated quit requests (the X pressed again) keep the prompt, and the answers so far.
        Some(open) if open.id == id => {}
        Some(_) => {
            app.ui.status = tl!("Answer the unsaved-changes prompt first").into();
            app.ui.status_error = true;
        }
    }
    true
}

/// Puts up `prompt`. One over several documents shows the document it asks about first; each
/// later one is shown as it comes up (see [`advance`]).
fn ask(app: &mut PhotocraftApp, prompt: Prompt) {
    if !matches!(reach(&prompt.id), Some(Reach::Target)) {
        show_tab(app, prompt.guard.current().copied());
    }
    app.discard = Some(prompt);
    open_question(app);
}

/// Opens the question about the first document, over any other the host holds.
fn open_question(app: &mut PhotocraftApp) {
    let Some(p) = app.discard.as_ref().filter(|p| !p.saving) else { return };
    let Some(name) = p.guard.current().and_then(|d| index_of(app, *d)).map(|i| app.session.documents()[i].doc.name.clone()) else { return };
    app.ui.host.open(UnsavedChangesDialog::new(p.guard.scope(), name).remaining(p.guard.remaining()));
}

/// Makes `doc` the active tab, if it is still open.
fn show_tab(app: &mut PhotocraftApp, doc: Option<DocId>) {
    if let Some(i) = doc.and_then(|d| index_of(app, d)) {
        app.session.set_active(i);
    }
}

impl PhotocraftApp {
    /// Whether closing now would lose work: a document has unsaved changes and closing wasn't
    /// confirmed. The web build has the browser ask before leaving the page while this holds (#1380).
    pub fn has_unsaved_work(&self) -> bool {
        !self.allow_close && self.session.documents().iter().any(|d| d.is_dirty())
    }
}

/// Called once per frame: holds back a window close request while there is unsaved work. Every
/// way of leaving (File › Exit / Quit, the close button, the OS's quit event, `app.quit`) ends up
/// here as a close request.
pub fn guard_window_close(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !ctx.input(|i| i.viewport().close_requested()) {
        return;
    }
    if !app.allow_close && intercept(app, EXIT, &Value::Null) {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        return;
    }
    // A save or print worker must finish its external side effect before the process exits;
    // `jobs_ui` repeats the close after completion and keeps errors visible.
    if app.saving() || app.printing() {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        app.jobs.close_after_saves = true;
        return;
    }
    // Leaving: through the platform's own quit where there is one (see `Services::quit`), with
    // the window left open for it to close.
    if let Some(quit) = app.services.quit.as_mut() {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        quit();
    }
}

/// Moves on after the first document was answered: saved (`saved`) or discarded. Close All, Close
/// Others and quitting close each document once it is answered, as Photoshop does, and show the
/// next one asked about: otherwise the tab just answered stays up and it looks like it is being
/// asked about again (#2547). Cancel then keeps only the unanswered ones.
fn advance(app: &mut PhotocraftApp, ctx: &egui::Context, saved: bool) {
    let Some(p) = app.discard.as_mut() else { return };
    let Some(answered) = p.guard.current().copied() else { return };
    let one_by_one = !matches!(reach(&p.id), Some(Reach::Target));
    let step = if saved {
        if !p.saving {
            // Saved without the guard having asked for it (the question was re-opened meanwhile).
            p.guard.forget(&answered);
        }
        p.guard.saved()
    } else {
        p.guard.discarded()
    };
    p.saving = false;
    if one_by_one
        && let Some(i) = index_of(app, answered)
        && let Err(e) = crate::menus::invoke_unguarded(app, ctx, "file.close", json!({"document": i}))
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
    follow(app, ctx, step);
}

/// What the guard says comes next.
fn follow(app: &mut PhotocraftApp, ctx: &egui::Context, step: GuardStep<DocId>) {
    let one_by_one = app.discard.as_ref().is_some_and(|p| !matches!(reach(&p.id), Some(Reach::Target)));
    match step {
        GuardStep::Ask(doc) => {
            if one_by_one {
                show_tab(app, Some(doc));
            }
            open_question(app);
        }
        GuardStep::Save(doc) => {
            if one_by_one {
                show_tab(app, Some(doc));
            }
            if let Some(p) = app.discard.as_mut() {
                p.saving = true;
            }
            if save(app, ctx, doc) {
                advance(app, ctx, true);
            }
        }
        GuardStep::Discard(doc) => {
            if one_by_one {
                show_tab(app, Some(doc));
            }
            advance(app, ctx, false);
        }
        GuardStep::Cancelled => app.discard = None,
        GuardStep::Finished => finish(app, ctx),
    }
}

/// Every document is answered: runs the parked action.
fn finish(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let one_by_one = app.discard.as_ref().is_some_and(|p| !matches!(reach(&p.id), Some(Reach::Target)));
    // A save or close hook may have dirtied a document that was clean when the prompt opened (or
    // made a new one): ask about those before the final close, which discards without checking.
    if one_by_one && let Some(p) = app.discard.as_ref() {
        let (id, target) = (p.id.clone(), p.target);
        let rest: Vec<DocId> = app
            .session
            .documents()
            .iter()
            .filter(|d| d.is_dirty() && (matches!(reach(&id), Some(Reach::All)) || Some(d.doc.id) != target))
            .map(|d| d.doc.id)
            .collect();
        if !rest.is_empty() {
            show_tab(app, rest.first().copied());
            if let Some(p) = app.discard.as_mut() {
                *p = Prompt::new(&p.id, p.params.clone(), p.target, rest);
            }
            open_question(app);
            return;
        }
    }
    let Some(Prompt { id, mut params, target, .. }) = app.discard.take() else { return };
    if id == EXIT {
        app.allow_close = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        return;
    }
    if let Some(target) = target {
        // The tab may have moved since the command was issued; aim it at the same document.
        params = match index_of(app, target) {
            Some(i) => json!({"document": i}),
            // Close All may have closed its target above already; it still closes the rest.
            None if matches!(reach(&id), Some(Reach::All)) => json!({}),
            None => return,
        };
    }
    if let Err(e) = crate::menus::invoke_unguarded(app, ctx, &id, params) {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

/// Saves `doc`; true once it is saved. A document without a file to write back to asks where in
/// the save dialog: the prompt then hides until it is answered, and moves on only if it saved.
fn save(app: &mut PhotocraftApp, ctx: &egui::Context, doc: DocId) -> bool {
    let Some(i) = index_of(app, doc) else { return false };
    app.session.set_active(i);
    let saved = crate::menus::invoke_unguarded(app, ctx, "file.save", json!({}));
    if saved.is_ok() && app.file_dialog_open() {
        let ctx = ctx.clone();
        app.after_file_dialog(move |app, saved| {
            let saved = saved.map_err(couldnt_save)?;
            saved_document(app, &ctx, doc);
            Ok(saved)
        });
        return false;
    }
    match saved {
        // A Save Document script may have edited the document after it was written: still dirty.
        // A write the platform can't confirm (a web download) leaves it dirty too, and counts.
        Ok(_) => {
            !app.save_options_open()
                && !app.saving()
                && index_of(app, doc).is_some_and(|i| app.services.write_unconfirmed || !app.session.documents()[i].is_dirty())
        }
        // Backing out of the file dialog is the user's choice, not an error.
        Err(e) if e == crate::file_dialog::CANCELLED => false,
        Err(e) => {
            app.ui.status = couldnt_save(e);
            app.ui.status_error = true;
            false
        }
    }
}

/// A completed save may release the close prompt. Choosing a path only starts a layered TIFF
/// save; TIFF Options calls this after the write, and `jobs_ui` when a background save ends.
/// Copies and failed writes leave the doc dirty; so does any write when it is unconfirmed.
pub(crate) fn saved_document(app: &mut PhotocraftApp, ctx: &egui::Context, doc: DocId) {
    // Still the prompt that asked (quitting may have replaced it meanwhile).
    if !app.save_options_open()
        && app.discard.as_ref().is_some_and(|p| p.guard.current() == Some(&doc))
        && index_of(app, doc).is_some_and(|i| app.services.write_unconfirmed || !app.session.documents()[i].is_dirty())
    {
        advance(app, ctx, true);
    }
}

/// A save failure as reported ("cancelled" stays as it is: it isn't reported).
fn couldnt_save(e: String) -> String {
    if e == crate::file_dialog::CANCELLED { e } else { format!("Couldn't save: {e}") }
}

/// Called every frame after the dialog host drew: takes the question's answer, and brings the
/// question back when a save it started ended without finishing the document.
pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let answered = app.ui.host.take::<UnsavedChangesDialog>();
    if app.discard.is_none() {
        // Dropped from outside while the question was up.
        app.ui.host.cancel::<UnsavedChangesDialog>(ctx);
        app.ui.host.take::<UnsavedChangesDialog>();
        return;
    }
    if let Some(DialogResult::Ok(answer)) = answered {
        apply(app, ctx, answer.answer, answer.apply_to_all);
        return;
    }
    // Save can ask for a path and then TIFF or Targa Options, and then write in the background.
    // Wait for all three; cancellation or a failed save brings us back.
    if app.file_dialog_open() || app.save_options_open() || app.saving() {
        return;
    }
    let Some(&doc) = app.discard.as_ref().and_then(|p| p.guard.current()) else { return };
    if index_of(app, doc).is_none() {
        // Closed by something else while the prompt was up: nothing left to ask about.
        if let Some(p) = app.discard.as_mut() {
            p.guard.forget(&doc);
        }
        advance_forgotten(app, ctx);
    } else if !app.ui.host.is_open::<UnsavedChangesDialog>() {
        // A save that ended without closing the document (cancelled Save As, failed write): the
        // whole close ends and the remaining documents stay open.
        let step = app.discard.as_mut().map(|p| p.guard.save_failed());
        if let Some(step) = step {
            follow(app, ctx, step);
        }
    }
}

/// The first document is gone (already forgotten by the guard): moves on without closing anything.
fn advance_forgotten(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(p) = app.discard.as_mut() else { return };
    p.saving = false;
    let step = p.guard.saved();
    follow(app, ctx, step);
}

/// Acts on the answer about the first document, the one the question asks about.
fn apply(app: &mut PhotocraftApp, ctx: &egui::Context, answer: CloseAnswer, to_all: bool) {
    let Some(p) = app.discard.as_mut() else { return };
    let step = if to_all { p.guard.answer_all(answer) } else { p.guard.answer(answer) };
    follow(app, ctx, step);
}

/// The id `ui.inspect` lists the prompt under; `ui.dialog.confirm` and `ui.dialog.cancel` answer
/// it by this id, so a control client can see and answer it like the window's buttons (#4920).
pub const CONTROL_ID: &str = "discard";

/// The prompt as `ui.inspect` lists it among the dialogs: the command waiting on it, the document
/// it asks about, how many are still to answer, and the choices `ui.dialog.confirm` takes.
pub fn inspect(app: &PhotocraftApp) -> Option<Value> {
    let p = app.discard.as_ref()?;
    let doc = *p.guard.current()?;
    let name = index_of(app, doc).and_then(|i| app.session.documents().get(i)).map(|d| d.doc.name.clone())?;
    let reverts = p.id == "file.revert";
    let (choices, default): (&[&str], _) = if reverts { (&["dontSave", "cancel"], "dontSave") } else { (&["save", "dontSave", "cancel"], "save") };
    Some(json!({
        "id": CONTROL_ID,
        "kind": "unsavedChanges",
        "title": if reverts { "Revert" } else { "Unsaved changes" },
        "fields": {"command": p.id, "document": name, "remaining": p.guard.remaining(), "choices": choices, "default": default},
    }))
}

/// The command the prompt holds back and its params, for the automation policy to see before a
/// control client answers it.
pub fn pending_command(app: &PhotocraftApp) -> Option<(String, Value)> {
    app.discard.as_ref().map(|p| (p.id.clone(), p.params.clone()))
}

/// Answers the prompt for a control client: `save`, `dontSave` or `cancel` (Revert takes
/// `dontSave` or `cancel`), with the Apply to All box set to `apply_to_all`. Like the buttons, Save may first ask for a path in the save dialog.
pub fn answer_control(app: &mut PhotocraftApp, ctx: &egui::Context, choice: &str, apply_to_all: bool) -> Result<Value, String> {
    if app.discard.as_ref().and_then(|p| p.guard.current()).is_none() {
        return Err("no unsaved-changes prompt is open".into());
    }
    if app.file_dialog_open() || app.save_options_open() || app.saving() {
        return Err("the unsaved-changes prompt is waiting for a save to finish".into());
    }
    let reverts = app.discard.as_ref().is_some_and(|p| p.id == "file.revert");
    let answer = match choice {
        "save" if !reverts => CloseAnswer::Save,
        "dontSave" => CloseAnswer::DontSave,
        "cancel" => CloseAnswer::Cancel,
        other => {
            let choices = if reverts { "dontSave or cancel" } else { "save, dontSave or cancel" };
            return Err(format!("unknown choice `{other}`: {choices}"));
        }
    };
    if !app.ui.host.confirm_with::<UnsavedChangesDialog>(ctx, |d| {
        d.set_answer(answer);
        d.set_apply_to_all(apply_to_all);
    }) {
        return Err("no unsaved-changes prompt is open".into());
    }
    let Some(DialogResult::Ok(answered)) = app.ui.host.take::<UnsavedChangesDialog>() else { return Err("the unsaved-changes prompt gave no answer".into()) };
    apply(app, ctx, answered.answer, answered.apply_to_all);
    Ok(json!({"open": app.discard.is_some(), "fileDialog": app.file_dialog_open()}))
}

#[cfg(test)]
mod tests;
