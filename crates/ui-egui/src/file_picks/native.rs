//! The native file panels: the library's `FilePanels`, pumped once per frame on the main thread.

use std::sync::Arc;

use craft::dialogs::service::{ExtensionPolicy, FilePanels, FileService, ReplaceWords};
use craft::host::Wake;

use crate::PhotocraftApp;

/// The panels and the extension policy they were last given.
#[derive(Default)]
pub(crate) struct Panels {
    panels: Option<FilePanels>,
    policy: ExtensionPolicy,
}

impl Panels {
    /// Shows the next queued panel and settles the answers. `FilePanels` takes its policy by
    /// value, so a changed preference rebuilds it around the same state.
    pub(crate) fn pump(&mut self, policy: ExtensionPolicy, parent: Option<&eframe::Frame>) {
        let Some(mut panels) = self.panels.take() else { return };
        if policy != self.policy {
            self.policy = policy;
            panels = panels.extension_policy(policy);
        }
        panels.set_replace_words(replace_words());
        panels.pump(parent);
        self.panels = Some(panels);
    }
}

/// Preferences ▸ File Handling › Lowercase Extension; a name typed without an extension always
/// gets the suggested one.
pub(crate) fn policy(lowercase: bool) -> ExtensionPolicy {
    ExtensionPolicy { lowercase, ..ExtensionPolicy::default() }
}

/// The Replace question, in PhotoCraft's words and the current language.
fn replace_words() -> ReplaceWords {
    ReplaceWords {
        title: tl!("Replace").to_owned(),
        question: tl!("“{name}” already exists. Replace it?").to_owned(),
        replace: tl!("Replace").to_owned(),
        cancel: tl!("Cancel").to_owned(),
    }
}

impl PhotocraftApp {
    /// Installs the native file service: picks queue in it, and `poll_file_dialog` shows them.
    /// `ctx` is woken from the dialog's thread when an answer is in.
    pub fn install_native_files(&mut self, ctx: &egui::Context) {
        let repaint = ctx.clone();
        let (service, panels) = FileService::new(Wake::new(move || repaint.request_repaint()));
        self.services.files = Some(Arc::new(service));
        self.file_panels = Panels { panels: Some(panels), policy: ExtensionPolicy::default() };
    }
}
