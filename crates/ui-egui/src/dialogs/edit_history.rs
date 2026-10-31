//! Undo inside a dialog (#4697): ⌘Z / ⇧⌘Z step through the open dialog's own settings, as in
//! Photoshop, instead of reaching the document's History beneath it (which the dialog previews
//! and is about to change), or doing nothing.
//!
//! A parameter dialog keeps its history in `craft::ui::dialog::Dialog` (`history`); this module
//! is the history of the workspaces without a dialog host. Liquify, Puppet Warp and Camera Raw keep
//! their own histories.

use crate::PhotocraftApp;

pub use craft::ui::dialog::{LocalHistory as EditHistory, Step, take_step};

/// ⌘Z / ⇧⌘Z while a workspace without a dialog host is in front (Filter Gallery, Adaptive Wide
/// Angle, Vanishing Point): steps its own history, and the keys never reach the document's
/// History beneath it. The parameter dialogs step theirs inside `craft::ui::dialog::Dialog`.
pub fn keys(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if ctx.text_edit_focused() || !app.ui.dialogs.is_empty() {
        return;
    }
    if let Some(d) = app.distort.gallery.as_mut() {
        if let Some(step) = take_step(ctx) {
            d.step(step);
        }
    } else if let Some(d) = app.wide_angle.as_mut() {
        if let Some(step) = take_step(ctx) {
            d.step(step);
        }
    } else if let Some(s) = app.distort.vanishing.as_mut()
        && let Some(step) = take_step(ctx)
    {
        s.step(step);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use serde_json::json;

    use super::*;

    /// One frame of the shell's key handling with ⌘Z (plus `modifiers`) pressed.
    pub(crate) fn press_z(app: &mut PhotocraftApp, ctx: &egui::Context, modifiers: egui::Modifiers) {
        let raw = egui::RawInput {
            events: vec![egui::Event::Key { key: egui::Key::Z, physical_key: None, pressed: true, repeat: false, modifiers }],
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| crate::shortcuts::handle(app, ui.ctx()));
        out.textures_delta.clear();
    }

    fn app() -> PhotocraftApp {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": 120, "height": 80})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        app.run("edit.fill", json!({"color": "#808080"})).unwrap();
        app
    }

    fn past(app: &PhotocraftApp) -> usize {
        app.session.active().unwrap().history.past_len()
    }

    /// One frame of the shell: the key handling, then the dialogs.
    fn frame(app: &mut PhotocraftApp, ctx: &egui::Context, events: Vec<egui::Event>) {
        let raw = egui::RawInput { events, ..Default::default() };
        let mut out = ctx.run_ui(raw, |ui| {
            crate::shortcuts::handle(app, ui.ctx());
            crate::dialogs::show(app, ui.ctx());
        });
        out.textures_delta.clear();
    }

    fn key_z(modifiers: egui::Modifiers) -> Vec<egui::Event> {
        vec![egui::Event::Key { key: egui::Key::Z, physical_key: None, pressed: true, repeat: false, modifiers }]
    }

    #[test]
    fn command_z_steps_a_parameter_dialog_and_leaves_its_private_keys() {
        let ctx = egui::Context::default();
        PhotocraftApp::setup_context(&ctx, crate::theme::ThemeKind::ALL[0]);
        let mut app = app();
        for _ in 0..2 {
            frame(&mut app, &ctx, vec![]);
        }
        let steps = past(&app);
        let id = crate::dialogs::open_command_dialog(&mut app, "filter.lensCorrection", "Lens Correction");
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![]);
        }
        let field = |app: &PhotocraftApp, k: &str| app.ui.dialogs.iter().find(|d| d.id == id).unwrap().fields.get(k).cloned();
        let vertical = field(&app, "vertical");
        let private: Vec<String> = app.ui.dialog_mut(id).unwrap().fields.keys().filter(|k| k.starts_with("__")).cloned().collect();
        assert!(!private.is_empty());
        app.ui.dialog_mut(id).unwrap().fields.insert("vertical".into(), json!(-50.0));
        frame(&mut app, &ctx, vec![]);
        app.ui.dialog_mut(id).unwrap().fields.insert("horizontal".into(), json!(30.0));
        frame(&mut app, &ctx, vec![]);
        // The filter dialog opens with its first number field focused, which keeps ⌘Z for typing.
        ctx.memory_mut(|m| {
            if let Some(focused) = m.focused() {
                m.surrender_focus(focused);
            }
        });
        frame(&mut app, &ctx, key_z(egui::Modifiers::COMMAND));
        assert_ne!(field(&app, "horizontal"), Some(json!(30.0)));
        assert_eq!(field(&app, "vertical"), Some(json!(-50.0)), "one step at a time");
        frame(&mut app, &ctx, key_z(egui::Modifiers::COMMAND));
        assert_eq!(field(&app, "vertical"), vertical);
        assert_eq!(past(&app), steps, "the document beneath is untouched");
        for k in &private {
            assert!(field(&app, k).is_some(), "{k} survives");
        }
        frame(&mut app, &ctx, key_z(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT));
        assert_eq!(field(&app, "vertical"), Some(json!(-50.0)));
    }

    #[test]
    fn command_z_steps_the_filter_gallery_stack() {
        let ctx = egui::Context::default();
        let mut app = app();
        let steps = past(&app);
        crate::distort_ui::menu(&mut app, &ctx, "filter.filterGallery", &json!({})).unwrap().unwrap();
        let before = app.distort.gallery.as_ref().unwrap().effects.clone();
        crate::distort_ui::menu(&mut app, &ctx, "filter.filterGallery", &json!({"ui": {"add": true}})).unwrap().unwrap();
        assert_eq!(app.distort.gallery.as_ref().unwrap().effects.len(), before.len() + 1);
        press_z(&mut app, &ctx, egui::Modifiers::COMMAND);
        let d = app.distort.gallery.as_ref().unwrap();
        assert_eq!(d.effects, before);
        assert!(d.selected < d.effects.len());
        assert_eq!(past(&app), steps);
    }

    #[test]
    fn command_z_steps_vanishing_point_planes() {
        let ctx = egui::Context::default();
        let mut app = app();
        let steps = past(&app);
        crate::distort_ui::menu(&mut app, &ctx, "filter.vanishingPoint", &json!({})).unwrap().unwrap();
        let plane = json!([{"corners": [[30, 20], [90, 20], [110, 70], [10, 70]]}]);
        crate::distort_ui::menu(&mut app, &ctx, "filter.vanishingPoint", &json!({"ui": {"planes": plane}})).unwrap().unwrap();
        press_z(&mut app, &ctx, egui::Modifiers::COMMAND);
        let s = app.distort.vanishing.as_ref().unwrap();
        assert!(s.planes.is_empty());
        assert_eq!(s.selected, None, "no selection left on a plane that is gone");
        assert_eq!(past(&app), steps);
        press_z(&mut app, &ctx, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT);
        assert_eq!(app.distort.vanishing.as_ref().unwrap().planes.len(), 1);
    }
}
