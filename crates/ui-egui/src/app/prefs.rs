use super::{PhotocraftApp, dock, panel_docking, single_line_drag, text_field_menu, theme, tool_cursor, wheel_nav};

impl PhotocraftApp {
    /// Records a picked type family for the font menu's Recent group, capped by
    /// Preferences ▸ Type ▸ Recent Fonts (0 keeps no list), most recent first, de-duplicated.
    pub fn note_recent_font(&mut self, family: &str) {
        let cap = self.session.prefs().type_.recent_fonts.min(50) as usize;
        let r = &mut self.ui.recent_fonts;
        r.retain(|f| f != family);
        if cap == 0 {
            r.clear();
            return;
        }
        r.insert(0, family.to_string());
        r.truncate(cap);
    }

    /// Preferences ▸ Type ▸ Recent Fonts: a smaller cap (or 0) applies to the existing list at
    /// once, not only after the next pick.
    pub fn sync_recent_fonts(&mut self) {
        self.ui.recent_fonts.truncate(self.session.prefs().type_.recent_fonts.min(50) as usize);
    }
}

impl PhotocraftApp {
    /// Shows `theme` and saves it as the theme of its mode, which becomes the appearance mode.
    pub fn set_theme(&mut self, ctx: &egui::Context, theme: theme::AppTheme) {
        let slot = if theme.is_dark() { "interface.darkTheme" } else { "interface.lightTheme" };
        let mode = if theme.is_dark() { "dark" } else { "light" };
        let values = serde_json::json!({"values": {slot: theme.id().as_str(), "interface.appearanceMode": mode}});
        if let Err(e) = self.run("prefs.set", values) {
            self.ui.status = e;
        } else {
            self.apply_theme(ctx, theme);
        }
    }

    pub(crate) fn apply_theme(&mut self, ctx: &egui::Context, theme: theme::AppTheme) {
        // Color and Swatches trade indices between Pro and Studio layouts.
        if let Some(selected) = dock::Group::Color.tabs(self.ui.theme.is_pro()).get(self.ui.dock_tabs.color)
            && let Some(tab) = dock::Group::Color.tabs(theme.is_pro()).iter().position(|name| name == selected)
        {
            self.ui.dock_tabs.color = tab;
        }
        self.ui.theme = theme;
        panel_docking::sync_visibility(self);
        theme::apply(ctx, &self.ui.theme);
        self.checker = None;
    }

    /// Follows the theme selection the suite's folder holds, shared with the other Craft apps, in
    /// place of the in-memory selection the app starts with. The desktop app calls it at startup.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_suite(&mut self, ctx: &egui::Context) {
        self.framework.open_suite(ctx);
    }
}

impl PhotocraftApp {
    /// Install fonts, image loaders and the theme. Call from the app creator (and again for a
    /// replacement Context) before the first `run_ui` so the very first frame renders and the
    /// Type reset begin/end pass markers exist; otherwise `logic` does it and the first frame is
    /// skipped, with the begin-pass marker necessarily starting on the following pass.
    pub fn setup_context(ctx: &egui::Context, theme: theme::AppTheme) {
        Self::install_type_reset_hooks(ctx);
        ctx.add_plugin(tool_cursor::CursorLifecycle);
        ctx.add_plugin(single_line_drag::SingleLineDrag::default());
        theme::install_fonts(ctx);
        egui_extras::install_image_loaders(ctx);
        theme::apply(ctx, &theme);
        // Right-click in any text field: its Cut / Copy / Paste menu (text_field_menu.rs).
        ctx.add_plugin(text_field_menu::TextFieldMenu);
        // egui's own ⌘+ / ⌘- / ⌘0 scale the whole interface; PhotoCraft zooms the canvas instead
        // (shortcuts.rs), like Photoshop.
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        // ⌘/Ctrl + wheel scrolls sideways, as in Photoshop; only a pinch zooms (wheel_nav.rs).
        wheel_nav::configure(ctx);
    }
}
