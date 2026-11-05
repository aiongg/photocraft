//! File picks that never block the event loop (#673, #574), over the library's file service
//! ([`craft::host::Files`]).
//!
//! A command that needs a file asks with [`PhotocraftApp::ask_file`] (or one of its wrappers) and
//! hands over what to do with the answer. The request becomes a library ticket
//! ([`Files::pick`]); [`PhotocraftApp::poll_file_dialog`], every frame, shows the queued panel on
//! native builds (`FilePanels::pump_with`, with the window as its parent, so it is modal to the window
//! as Photoshop's are) and continues the action once `Files::take_answer` has the ticket's answer.
//! The window keeps drawing and answering the compositor meanwhile. A dialog run to completion
//! inside the frame froze the window (Linux compositors offered to kill it, #574) and, on macOS,
//! ran AppKit's modal loop inside winit's event handler, which aborts the app when another event
//! arrives (#673).
//!
//! The service remembers the folder of the last answer per purpose, completes a typed save name
//! with the suggested extension, lowercases the extension (Preferences ▸ File Handling › Lowercase
//! Extension) and asks before replacing a file the panel did not ask about.
//!
//! One dialog at a time: asking while one is open is refused.

pub mod filters;
#[cfg(not(target_arch = "wasm32"))]
mod native;
pub mod scripted;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::Arc;

use craft::host::{Files, PickId, PickRequest, Picked};
use photocraft_doc::DocId;
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::file_open::{display_name, path_text};

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::Panels;
/// The native panels exist only where the platform has them; the web's service answers from the
/// browser.
#[cfg(target_arch = "wasm32")]
#[derive(Default)]
pub(crate) struct Panels;

/// What to do with the answer.
type Then = Box<dyn FnOnce(&mut PhotocraftApp, Picked) -> Result<Value, String>>;

/// The file dialog in progress.
pub(crate) struct Pending {
    id: PickId,
    then: Then,
    /// The dialog (by its id) whose field the answer fills: closing it forgets the pick.
    owner: Option<u64>,
}

/// The error of a cancelled dialog: not reported, since backing out is the user's choice.
pub(crate) const CANCELLED: &str = "cancelled";
const UNEXPECTED: &str = "the file dialog gave an unexpected answer";

/// The service every library component of this context reaches through its Host.
#[derive(Clone)]
struct SharedFiles(Arc<dyn Files>);

fn shared_id() -> egui::Id {
    egui::Id::new("photocraft-host-files")
}

/// The file service installed for `ctx`, if the app has one.
pub(crate) fn files_of(ctx: &egui::Context) -> Option<Arc<dyn Files>> {
    ctx.data(|d| d.get_temp::<SharedFiles>(shared_id())).map(|s| s.0)
}

/// Makes `files` the service of `ctx`'s Host, so library components (their Browse buttons, the
/// swatch and LUT pickers) and the app's own picks meet in one queue.
fn publish(ctx: &egui::Context, files: &Arc<dyn Files>) {
    if files_of(ctx).is_some_and(|f| Arc::ptr_eq(&f, files)) {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(shared_id(), SharedFiles(Arc::clone(files))));
    crate::i18n::reinstall_host(ctx);
}

impl PhotocraftApp {
    /// Whether a file dialog is open (or about to be shown).
    pub fn file_dialog_open(&self) -> bool {
        self.file_dialog.is_some()
    }

    /// Queue a pick; `then` continues with the user's choice on a later frame (see the module
    /// docs). Returns at once with `{"fileDialog": "open" | "folder" | "save"}`; `Err` without a
    /// file service ("cancelled") or while a dialog is open.
    pub(crate) fn ask_file(&mut self, request: PickRequest, then: impl FnOnce(&mut Self, Picked) -> Result<Value, String> + 'static) -> Result<Value, String> {
        let Some(files) = self.services.files.clone() else { return Err(CANCELLED.into()) };
        if self.file_dialog.is_some() {
            return Err("a file dialog is already open".into());
        }
        let kind = match request.kind {
            craft::host::PickKind::Open | craft::host::PickKind::OpenMany => "open",
            craft::host::PickKind::Folder => "folder",
            craft::host::PickKind::Save => "save",
        };
        let then: Then = Box::new(move |app, picked| match picked {
            Picked::Cancelled => Err(CANCELLED.into()),
            picked => then(app, picked),
        });
        let id = files.pick(request);
        self.file_dialog = Some(Pending { id, then, owner: None });
        Ok(json!({ "fileDialog": kind }))
    }

    /// Ask the user to choose a folder, starting in the one the last such dialog answered with.
    /// `then` receives the selected path when the dialog is answered.
    pub(crate) fn ask_folder(&mut self, then: impl FnOnce(&mut Self, String) -> Result<Value, String> + 'static) -> Result<Value, String> {
        self.ask_file(PickRequest::folder("folder"), move |app, picked| then(app, first_path(&picked, false)?))
    }

    /// Ask where to save: `then` gets the chosen path. The service completes a name typed without
    /// an extension with the suggested file's (#5039, as Photoshop's Save As appends the extension
    /// of the chosen file type), lowercases the extension when Preferences ▸ File Handling ›
    /// Lowercase Extension is on, and asks whether to replace a file it named that way.
    ///
    /// A suggested path with a folder (Save As, the document's own path) starts in that folder. A
    /// bare file name (export dialogs, Save a Copy) starts in the folder the document's last
    /// dialog answered with (#1826, #1866), else beside the document.
    pub(crate) fn pick_save(&mut self, suggested: &str, then: impl FnOnce(&mut Self, String) -> Result<Value, String> + 'static) -> Result<Value, String> {
        let suggested = filters::netpbm_as_pnm(suggested);
        let path = Path::new(&suggested);
        let doc = self.session.active().map(|d| d.doc.id);
        let purpose = doc.map_or_else(|| "save".to_owned(), |DocId(id)| format!("save.{id}"));
        // An explicit directory wins over the remembered folder, so a bare name gives it only when
        // the document has no remembered folder yet.
        let directory = path.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf).or_else(|| {
            let remembered = self.services.files.as_ref().and_then(|f| f.last_folder(&purpose));
            let beside = self.session.active().and_then(|d| d.path.as_deref()).and_then(|p| Path::new(p).parent()).map(Path::to_path_buf);
            if remembered.is_some() { None } else { beside }
        });
        let mut request = PickRequest::save(purpose);
        request.directory = directory;
        request.file_name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            request = request.suggested_extension(ext);
        }
        for (name, extensions) in filters::save_filters(&suggested) {
            request = request.filter(name, &extensions.iter().map(String::as_str).collect::<Vec<_>>());
        }
        self.ask_file(request, move |app, picked| then(app, first_path(&picked, true)?))
    }

    /// Ask for a file a command reads (a script, notes, a placed image, presets): `then` gets its
    /// name (the full path on the desktop) and bytes. A file that can't be read fails with
    /// "<file name>: <why>".
    pub(crate) fn pick_file_bytes(&mut self, then: impl FnOnce(&mut Self, String, Vec<u8>) -> Result<Value, String> + 'static) -> Result<Value, String> {
        self.pick_file_bytes_filtered(&[], then)
    }

    /// Same picker, with an extension filter (desktop and web).
    pub(crate) fn pick_file_bytes_filtered(
        &mut self,
        extensions: &[&str],
        then: impl FnOnce(&mut Self, String, Vec<u8>) -> Result<Value, String> + 'static,
    ) -> Result<Value, String> {
        let request = self.open_request(false, extensions);
        self.ask_file(request, move |app, picked| {
            let (name, bytes) = read_picked(picked)?;
            then(app, name, bytes)
        })
    }

    /// [`Self::pick_file_bytes`] starting in `directory` (Relink to File: the missing file's folder).
    pub(crate) fn pick_file_bytes_in(
        &mut self,
        directory: Option<std::path::PathBuf>,
        then: impl FnOnce(&mut Self, String, Vec<u8>) -> Result<Value, String> + 'static,
    ) -> Result<Value, String> {
        let mut request = self.open_request(false, &[]);
        request.directory = directory;
        self.ask_file(request, move |app, picked| {
            let (name, bytes) = read_picked(picked)?;
            then(app, name, bytes)
        })
    }

    /// An Open request for `extensions` (every format File › Open reads when empty), starting in
    /// the folder the service remembers for the purpose.
    pub(crate) fn open_request(&self, multiple: bool, extensions: &[&str]) -> PickRequest {
        let purpose = if extensions.is_empty() { "open".to_owned() } else { format!("open.{}", extensions.join(".")) };
        let mut request = if multiple { PickRequest::open_many(purpose) } else { PickRequest::open(purpose) };
        if extensions.is_empty() {
            for (name, extensions) in filters::open_filters() {
                request = request.filter(name, &extensions.iter().map(String::as_str).collect::<Vec<_>>());
            }
            request
        } else {
            // A command-specific filter must not inherit the image formats used by File › Open.
            request.filter("Supported Files", extensions)
        }
    }

    /// File › Open: opens every chosen file, reporting each failure (see [`Self::open_paths`]).
    pub fn open_dialog_file(&mut self) -> Result<Value, String> {
        let request = self.open_request(true, &[]);
        self.ask_file(request, |app, picked| {
            match picked {
                Picked::Paths(paths) => {
                    let paths = paths.iter().map(|p| path_text(p, false)).collect::<Result<Vec<_>, _>>()?;
                    app.open_paths(&paths);
                }
                Picked::Contents(files) => app.open_contents(files),
                Picked::Cancelled | Picked::Unavailable(_) => return Err(CANCELLED.into()),
            }
            Ok(Value::Null)
        })
    }

    /// Opens each picked file's contents as its own document, in name order (like
    /// [`Self::open_paths`]), reporting each failure.
    fn open_contents(&mut self, mut files: Vec<(String, Vec<u8>)>) {
        files.sort_by(|a, b| crate::file_open::natural_cmp(&a.0, &b.0));
        for (name, bytes) in files {
            if let Err(e) = self.open_bytes(&name, &bytes) {
                self.open_failed(&name, &e);
            }
        }
    }

    /// Run `next` on the result of the open dialog's action once it is answered; its result
    /// replaces the action's. False when no dialog is open.
    pub(crate) fn after_file_dialog(&mut self, next: impl FnOnce(&mut Self, Result<Value, String>) -> Result<Value, String> + 'static) -> bool {
        let Some(p) = self.file_dialog.as_mut() else { return false };
        let then = std::mem::replace(&mut p.then, Box::new(|_, _| Ok(Value::Null)));
        p.then = Box::new(move |app, picked| {
            let r = then(app, picked);
            next(app, r)
        });
        true
    }

    /// The document `id` becomes active again: an answer applies to the document it was asked
    /// for, even when another one was activated while the dialog was open. `Err` once it's closed.
    pub(crate) fn refocus(&mut self, id: DocId) -> Result<(), String> {
        let i = self.session.documents().iter().position(|d| d.doc.id == id).ok_or("no document")?;
        self.session.set_active(i);
        Ok(())
    }

    /// Complete a deferred operation on its requesting document without stealing the selected
    /// tab. Restore by ID as script events can close or reorder documents during a save.
    pub(crate) fn with_document<T>(&mut self, id: DocId, run: impl FnOnce(&mut Self) -> Result<T, String>) -> Result<T, String> {
        let active = self.session.active().map(|d| d.doc.id);
        self.refocus(id)?;
        let result = run(self);
        if let Some(active) = active {
            let _ = self.refocus(active);
        }
        result
    }

    /// The active document's id, for [`Self::refocus`].
    pub(crate) fn active_doc_id(&self) -> Result<DocId, String> {
        Ok(self.session.active().ok_or("no document")?.doc.id)
    }

    /// Marks the dialog in progress as filling a field of dialog `owner`.
    pub(crate) fn file_dialog_for(&mut self, owner: u64) {
        if let Some(p) = self.file_dialog.as_mut() {
            p.owner = Some(owner);
        }
    }

    /// Dialog `owner` closes: a pick it has outstanding is dropped without running its action, and
    /// the service forgets the ticket, so a late answer blocks nothing.
    pub(crate) fn forget_file_dialog_of(&mut self, owner: u64) {
        if self.file_dialog.as_ref().is_some_and(|p| p.owner == Some(owner))
            && let (Some(p), Some(files)) = (self.file_dialog.take(), self.services.files.as_ref())
        {
            files.forget(p.id);
        }
    }

    /// Called every frame: shows a queued panel with `parent` (the window) as its parent, and
    /// continues the action that asked once the answer is in. Its errors go to the status bar.
    pub fn poll_file_dialog(&mut self, ctx: &egui::Context, parent: Option<&eframe::Frame>) {
        let Some(files) = self.services.files.clone() else { return };
        publish(ctx, &files);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let policy = native::policy(self.session.prefs().file_handling.lowercase_extension);
            self.file_panels.pump(policy, &mut self.ui.host, parent);
            crate::standard_prompts::adopt_file_question(self);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = parent;
        let Some(id) = self.file_dialog.as_ref().map(|p| p.id) else { return };
        let Some(picked) = files.take_answer(id) else { return };
        let Some(Pending { then, .. }) = self.file_dialog.take() else { return };
        // No dialog could open: the action ends as for a Cancel (so what chained on it still
        // cleans up), and the status says why.
        let r = match picked {
            Picked::Unavailable(cause) => {
                let _ = then(self, Picked::Cancelled);
                Err(unavailable_message(ctx, &cause))
            }
            picked => then(self, picked),
        };
        if let Err(e) = &r
            && e != CANCELLED
        {
            self.ui.status = e.clone();
            self.ui.status_error = true;
        }
    }
}

/// The status sentence for a pick no dialog could answer.
#[cfg(not(target_arch = "wasm32"))]
fn unavailable_message(ctx: &egui::Context, cause: &str) -> String {
    <craft::dialogs::ui::UnavailableLabels as craft::ui::localized::Labels>::of_ctx(ctx).message(cause)
}

/// The browser's service never answers `Unavailable`; the cause is the sentence.
#[cfg(target_arch = "wasm32")]
fn unavailable_message(_: &egui::Context, cause: &str) -> String {
    cause.to_owned()
}

/// The first chosen path as text; a name that isn't valid Unicode is an error (#6601).
pub(crate) fn first_path(picked: &Picked, saving: bool) -> Result<String, String> {
    let path = picked.first_path().ok_or(UNEXPECTED)?;
    path_text(&path, saving)
}

/// The name and bytes of the single file an open dialog answered with.
fn read_picked(picked: Picked) -> Result<(String, Vec<u8>), String> {
    match picked {
        Picked::Contents(files) => files.into_iter().next().ok_or_else(|| CANCELLED.into()),
        Picked::Paths(paths) => {
            let path = paths.into_iter().next().ok_or(CANCELLED)?;
            let name = path_text(&path, false)?;
            let bytes = photocraft_format::read_file(&path).map_err(|e| format!("{}: {e}", display_name(&name)))?;
            Ok((name, bytes))
        }
        Picked::Cancelled => Err(CANCELLED.into()),
        Picked::Unavailable(_) => Err(UNEXPECTED.into()),
    }
}
