//! The desktop's colour scheme and GNOME's window-button layout, from craft-platform's settings
//! portal watcher (Linux and the BSDs; macOS and Windows report the scheme through winit instead).

use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use craft_platform::appearance::{Theme, Watcher};

/// The one desktop watcher: it feeds the colour scheme to the UI ([`Appearance::theme_service`]) and
/// GNOME's `button-layout` to the caption buttons ([`Appearance::button_layout`]), over one
/// connection.
#[derive(Clone)]
pub struct Appearance {
    // A mutex: the watcher owns a channel receiver, so it is not `Sync` itself.
    watcher: Arc<Mutex<Watcher>>,
    wake: Arc<OnceLock<egui::Context>>,
}

impl Appearance {
    /// Starts watching and waits up to 250 ms for the first answer, so a fast desktop answers
    /// before the first frame. `None` where the target has no watcher.
    pub fn start() -> Option<Self> {
        let wake: Arc<OnceLock<egui::Context>> = Arc::new(OnceLock::new());
        let woken = Arc::clone(&wake);
        let watcher = Watcher::start(move || {
            if let Some(ctx) = woken.get() {
                ctx.request_repaint();
            }
        })?;
        watcher.wait_first(std::time::Duration::from_millis(250));
        Some(Self { watcher: Arc::new(Mutex::new(watcher)), wake })
    }

    /// The colour scheme the desktop reports, as `Services::system_theme` reads it. Each call also
    /// hands the native window the observed scheme, so its frame follows the desktop.
    pub fn theme_service(&self) -> photocraft_ui_egui::SystemThemeFn {
        let this = self.clone();
        Box::new(move |ctx| {
            let _ = this.wake.set(ctx.clone());
            let observed = this.watcher.lock().unwrap_or_else(PoisonError::into_inner).current();
            craft_platform::appearance::sync_native_window_theme(ctx, observed);
            observed.map(|t| match t {
                Theme::Dark => egui::Theme::Dark,
                Theme::Light => egui::Theme::Light,
            })
        })
    }

    /// GNOME's `button-layout` as the watcher last heard it.
    pub fn button_layout(&self) -> Option<String> {
        self.watcher.lock().unwrap_or_else(PoisonError::into_inner).button_layout()
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn portal_proxy() -> Option<zbus::blocking::Proxy<'static>> {
    let conn = zbus::blocking::connection::Builder::session().ok()?.method_timeout(std::time::Duration::from_secs(2)).build().ok()?;
    zbus::blocking::Proxy::new(&conn, "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.Settings").ok()
}
