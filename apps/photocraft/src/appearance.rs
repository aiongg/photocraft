//! Connection to the desktop settings portal, shared with the caption-button layout reader.

#[cfg(target_os = "linux")]
pub(crate) fn portal_proxy() -> Option<zbus::blocking::Proxy<'static>> {
    let conn = zbus::blocking::connection::Builder::session().ok()?.method_timeout(std::time::Duration::from_secs(2)).build().ok()?;
    zbus::blocking::Proxy::new(&conn, "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.Settings").ok()
}

/// The app inside the eframe window. On Linux and the BSDs winit cannot read the desktop
/// appearance reliably (Wayland compositors send none), so the settings portal's answer replaces
/// winit's in every frame's input; a portal without an answer clears it.
#[cfg(target_os = "linux")]
pub struct Shell {
    app: photocraft_ui_egui::PhotocraftApp,
    watcher: Option<craft_platform::appearance::Watcher>,
}

#[cfg(target_os = "linux")]
impl Shell {
    pub fn new(app: photocraft_ui_egui::PhotocraftApp, ctx: &egui::Context) -> Self {
        let wake = ctx.clone();
        Self { app, watcher: craft_platform::appearance::Watcher::start(move || wake.request_repaint()) }
    }
}

#[cfg(target_os = "linux")]
impl eframe::App for Shell {
    fn on_exit(&mut self) {
        eframe::App::on_exit(&mut self.app);
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        eframe::App::logic(&mut self.app, ctx, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        eframe::App::ui(&mut self.app, ui, frame);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Some(watcher) = &self.watcher {
            let observed = watcher.current();
            raw_input.system_theme = observed.map(|theme| match theme {
                craft_platform::appearance::Theme::Dark => egui::Theme::Dark,
                craft_platform::appearance::Theme::Light => egui::Theme::Light,
            });
            craft_platform::appearance::sync_native_window_theme(ctx, observed);
        }
        eframe::App::raw_input_hook(&mut self.app, ctx, raw_input);
    }
}
