//! The native file panels: the library's `FilePanels`, pumped once per frame on the main thread.

use std::sync::Arc;

use craft::dialogs::service::{ExtensionPolicy, FilePanels, FileService};
use craft::host::Wake;

use crate::PhotocraftApp;

/// The panels the native service queues its picks for.
#[derive(Default)]
pub(crate) struct Panels(Option<FilePanels>);

impl Panels {
    /// Shows the next queued panel and settles the answers. The Replace question is a prompt on
    /// `host`, shown with the app's other dialogs.
    pub(crate) fn pump(&mut self, policy: ExtensionPolicy, host: &mut craft::ui::dialog::DialogHost, parent: Option<&eframe::Frame>) {
        let Some(panels) = self.0.as_mut() else { return };
        panels.set_extension_policy(policy);
        panels.pump_with(host, parent);
    }
}

/// Preferences ▸ File Handling › Lowercase Extension; a name typed without an extension always
/// gets the suggested one.
pub(crate) fn policy(lowercase: bool) -> ExtensionPolicy {
    ExtensionPolicy { lowercase, ..ExtensionPolicy::default() }
}

impl PhotocraftApp {
    /// Installs the native file service: picks queue in it, and `poll_file_dialog` shows them.
    /// `ctx` is woken from the dialog's thread when an answer is in.
    pub fn install_native_files(&mut self, ctx: &egui::Context) {
        let repaint = ctx.clone();
        let (service, panels) = FileService::new(Wake::new(move || repaint.request_repaint()));
        self.services.files = Some(Arc::new(service));
        self.file_panels = Panels(Some(panels));
    }
}
