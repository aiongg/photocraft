use super::PhotocraftApp;
use craft::ui::patterns::StatusBar;

// ----------------------------------------------------------------------------- status bar

/// The contents of the shell's status slot: a [`StatusBar`] with the status readout and the
/// message in its leading group, and the render path and background jobs in its trailing group.
pub fn status_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let shared = std::cell::RefCell::new(&mut *app);
    StatusBar::new("status_bar").label(tl!("Status bar")).show(ui, |bar| {
        bar.leading(|ui| {
            let app = &mut **shared.borrow_mut();
            crate::chrome_ui::status_leading(app, ui);
        });
        bar.trailing(|ui| {
            let app = &mut **shared.borrow_mut();
            crate::chrome_ui::render_path_status(app, ui);
            crate::background_redraw::status(app, ui);
            // Background job progress with Cancel, at the far end (#210).
            crate::jobs_ui::status_progress(app, ui);
        });
    });
}
