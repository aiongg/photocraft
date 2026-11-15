//! Stroke popups own plain keys before the application's tool shortcuts run.
use craft::draw::ui::shape::VectorMode;
use egui::{Key, Modifiers, vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use serde_json::json;

use crate::{PhotocraftApp, state::Tool};

fn app(tool: Tool) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 80, "height": 60})).unwrap();
    app.ui.tool = tool;
    app.ui.tool_options.vector_mode = VectorMode::Shape;
    app.ui.tool_options.stroke_width = 3.0;
    app.sync_views();
    app
}

/// The Properties body of the active shape layer, after the application's shortcuts as in
/// `PhotocraftApp::update`.
fn harness(app: PhotocraftApp) -> Harness<'static, PhotocraftApp> {
    let ctx = app.ctx().clone();
    let mut h = Harness::builder().with_size(vec2(1000.0, 750.0)).with_context(ctx).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            let _language = crate::i18n::language_scope(crate::i18n::Lang::EN);
            if !ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            crate::shortcuts::handle(app, ui.ctx());
            let layer = app.session.active().unwrap().active_layer.unwrap();
            ui.set_max_width(320.0);
            crate::vector_ui::shape_properties(app, ui, layer);
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::AppTheme::pro_medium());
    h.run_steps(4);
    h
}

#[test]
fn undo_shortcut_remains_available_and_invalidates_the_open_stroke_editor() {
    let mut app = app(Tool::Rectangle);
    app.run("shape.create", json!({"rect": [10, 10, 40, 30], "stroke": {"width": 4, "color": "#dd2244"}})).unwrap();
    let original = app.session.active().unwrap().doc.clone();
    let history = app.session.active().unwrap().history.past_len();
    let mut h = harness(app);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Stroke style").click();
    h.run_steps(3);
    assert!(h.state().ui.shape_stroke_open);
    h.get_by_role_and_label(egui::accesskit::Role::RadioButton, "Dotted").click();
    h.run_steps(3);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().session.active().unwrap().doc, original);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
    // The editor binds the layer, so it shows the undone style.
    assert!(h.state().ui.shape_stroke_open);
    let dotted = h.get_by_role_and_label(egui::accesskit::Role::RadioButton, "Dotted");
    assert_ne!(dotted.accesskit_node().toggled(), Some(egui::accesskit::Toggled::True), "the editor shows the undone style");
}
