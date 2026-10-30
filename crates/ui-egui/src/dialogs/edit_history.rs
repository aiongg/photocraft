//! Undo inside a dialog (#4697): ⌘Z / ⇧⌘Z step through the open dialog's own settings, as in
//! Photoshop, instead of reaching the document's History beneath it (which the dialog previews
//! and is about to change), or doing nothing.
//!
//! A parameter dialog keeps its history in `craft_ui::dialog::Dialog` (`history`); this module
//! is the history of the workspaces without a dialog host. Liquify, Puppet Warp and Camera Raw keep
//! their own histories.

use crate::PhotocraftApp;

pub use craft_ui::dialog::{LocalHistory as EditHistory, Step, take_step};

/// ⌘Z / ⇧⌘Z while a workspace without a dialog host is in front (Filter Gallery, Adaptive Wide
/// Angle, Vanishing Point): steps its own history, and the keys never reach the document's
/// History beneath it. The parameter dialogs step theirs inside `craft_ui::dialog::Dialog`.
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

    #[test]
    fn a_drag_is_one_step_and_a_new_edit_drops_redo() {
        let mut h = EditHistory::new(&0);
        for v in 1..=5 {
            h.settle(&v, true);
        }
        h.settle(&5, false);
        h.settle(&7, false);
        let mut now = 7;
        assert!(h.step(&mut now, Step::Undo));
        assert_eq!(now, 5);
        assert!(h.step(&mut now, Step::Undo));
        assert_eq!(now, 0, "the whole drag was one step");
        assert!(!h.step(&mut now, Step::Undo));
        assert!(h.step(&mut now, Step::Redo));
        assert_eq!(now, 5);
        now = 9;
        assert!(!h.step(&mut now, Step::Redo), "a new edit drops what was undone");
        assert!(h.step(&mut now, Step::Undo));
        assert_eq!(now, 5, "an unsettled edit is undone first");
    }

    #[test]
    fn command_z_steps_a_parameter_dialog_and_leaves_its_private_keys() {
        let ctx = egui::Context::default();
        let mut app = app();
        let steps = past(&app);
        let id = crate::dialogs::open_command_dialog(&mut app, "filter.lensCorrection", "Lens Correction");
        track_dialogs(&mut app, &ctx);
        let field = |app: &PhotocraftApp, k: &str| app.ui.dialogs.iter().find(|d| d.id == id).unwrap().fields.get(k).cloned();
        let vertical = field(&app, "vertical");
        let private: Vec<String> = app.ui.dialog_mut(id).unwrap().fields.keys().filter(|k| k.starts_with("__")).cloned().collect();
        assert!(!private.is_empty());
        app.ui.dialog_mut(id).unwrap().fields.insert("vertical".into(), json!(-50.0));
        track_dialogs(&mut app, &ctx);
        app.ui.dialog_mut(id).unwrap().fields.insert("horizontal".into(), json!(30.0));
        track_dialogs(&mut app, &ctx);
        press_z(&mut app, &ctx, egui::Modifiers::COMMAND);
        assert_ne!(field(&app, "horizontal"), Some(json!(30.0)));
        assert_eq!(field(&app, "vertical"), Some(json!(-50.0)), "one step at a time");
        press_z(&mut app, &ctx, egui::Modifiers::COMMAND);
        assert_eq!(field(&app, "vertical"), vertical);
        assert_eq!(past(&app), steps, "the document beneath is untouched");
        for k in &private {
            assert!(field(&app, k).is_some(), "{k} survives");
        }
        press_z(&mut app, &ctx, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT);
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
