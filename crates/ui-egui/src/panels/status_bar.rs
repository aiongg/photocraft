use super::PhotocraftApp;
use craft_ui::patterns::StatusBar;

// ----------------------------------------------------------------------------- status bar

/// The contents of the shell's status slot: a [`StatusBar`] with the zoom field, the document
/// info and the message in its leading group, and the render path, background jobs and Fit in its
/// trailing group.
pub fn status_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let pro = crate::theme::Tokens::get(ui.ctx()).pro;
    // The bar mirrors itself from the library's direction, which the title bar normally sets.
    super::title_bar::sync_library_direction(ui.ctx());
    let shared = std::cell::RefCell::new(&mut *app);
    StatusBar::new("status_bar").label(tl!("Status bar")).show(ui, |bar| {
        bar.leading(|ui| {
            let app = &mut **shared.borrow_mut();
            if pro {
                crate::chrome_ui::status_bar_pro(app, ui);
                return;
            }
            if let (Some(st), Some(i)) = (app.session.active(), app.session.active_index()) {
                let (w, h, mode, bits, layers) =
                    (st.doc.size.width, st.doc.size.height, crate::canvas::mode_label(&st.doc), st.doc.depth.bits(), st.doc.layer_count());
                crate::chrome_ui::zoom_field(app, ui, i);
                StatusBar::info(ui, &crate::i18n::fmt(tl!("{mode} Color · {bits} bit"), &[("mode", mode), ("bits", &bits.to_string())]));
                StatusBar::separator(ui);
                StatusBar::info(ui, &format!("{w} × {h} px"));
                StatusBar::separator(ui);
                StatusBar::info(ui, &crate::i18n::trn(crate::i18n::current(), layers as u64, "{n} layer", "{n} layers"));
            } else {
                StatusBar::info(ui, tl!("No document"));
            }
            crate::chrome_ui::status_message(app, ui);
        });
        bar.trailing(|ui| {
            let app = &mut **shared.borrow_mut();
            crate::chrome_ui::render_path_status(app, ui);
            crate::background_redraw::status(app, ui);
            if pro {
                // Background job progress with Cancel, at the far end (#210).
                crate::jobs_ui::status_progress(app, ui);
                return;
            }
            if app.session.has_jobs() {
                crate::jobs_ui::status_progress(app, ui);
                ui.add_space(craft_ui::theme::scale::Space::S6.px());
            }
            if app.session.active().is_some() && ui.add(craft_ui::controls::Button::new(tl!("Fit")).size(craft_ui::controls::ControlSize::Sm)).clicked() {
                if let Some(i) = app.session.active_index() {
                    app.ui.views[i].fit_pending = true;
                }
            }
        });
    });
}
