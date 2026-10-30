//! The window's skeleton: the title bar, options bar, tool rail, status bar and the dock around
//! the document, laid out by craft-ui's `AppShell` with the Canvas preset's slot keys.

use std::cell::RefCell;

use craft_ui::shell::{AppShell, BodyMode, Slot, WindowKey};

use super::{PhotocraftApp, canvas, panels, prefs_ui, theme, titlebar};

impl PhotocraftApp {
    /// The main window's key in the shell.
    pub(crate) fn window_key() -> WindowKey {
        WindowKey::main()
    }

    /// Whether this input holds a Tab press and Tab is bound to Show/Hide All Panels.
    pub(crate) fn tab_runs_toggle_panels(&self, raw: &egui::RawInput) -> bool {
        let tab = |e: &egui::Event| matches!(e, egui::Event::Key { key: egui::Key::Tab, pressed: true, .. });
        raw.events.iter().any(tab)
            && crate::shortcut_dispatch::bindings(self).into_iter().any(|(id, sc)| id == "window.togglePanels" && Some(sc) == crate::shortcuts::parse("Tab"))
    }

    /// Draws the bars, the tool rail and the body. Full Screen Mode runs the shell body-only: the
    /// bars, the rail and the dock show only while Tab (or the screen edge) reveals them, and the
    /// panels' own flags stay as they were.
    pub(crate) fn shell(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let t = theme::Tokens::get(&ctx);
        let panels_on = self.ui.panels.clone();
        let mode = if self.ui.view.hides_chrome() { BodyMode::Only } else { BodyMode::Normal };
        let narrow = self.session.prefs().workspace.narrow_options_bar();
        let options_height = panels::options_bar_height(t.pro, narrow);
        let tools_width = panels::toolbar_width(t.pro, panels_on.toolbar_double);
        let pasteboard = prefs_ui::pasteboard_color(self).unwrap_or(t.canvas);
        if self.custom_titlebar && !panels_on.menu_bar {
            // The undecorated window keeps its edge resizing without the bar.
            titlebar::edge_zones_without_bar(self, ui);
        }
        let app = RefCell::new(&mut *self);
        AppShell::new(Self::window_key())
            .title(Slot::new("title").visible(panels_on.menu_bar).reveal_if(panels_on.menu_bar), |ui| panels::title_bar(&mut app.borrow_mut(), ui))
            .top(Slot::new("options").visible(panels_on.options_bar).reveal().height(options_height), |ui| panels::options_bar(&mut app.borrow_mut(), ui))
            .bottom(Slot::new("status").visible(panels_on.status_bar), |ui| panels::status_bar(&mut app.borrow_mut(), ui))
            .start(Slot::new("tools").visible(panels_on.toolbar).reveal().width(tools_width), |ui| panels::toolbar(&mut app.borrow_mut(), ui))
            .body_mode(mode)
            .backdrop(Some(pasteboard))
            .show(ui, |ui, cx| {
                let mut guard = app.borrow_mut();
                let app = &mut **guard;
                let docking = if mode == BodyMode::Only { cx.dock_visible() } else { panels_on.dock };
                if docking {
                    panels::dock_area(app, ui);
                } else {
                    canvas::document_area(app, ui);
                    if cx.floating_visible() {
                        panels::properties_window(app, &ctx);
                    }
                }
            });
    }
}
