//! #5052: an options-bar caption and its number field form one scrub target, and a drag of either
//! is one journal command.
use crate::PhotocraftApp;
use egui::{Modifiers, Pos2, vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

fn harness() -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.ui.tool = crate::state::Tool::Brush;
    app.session.tools.brush.opacity = 0.5;
    let mut h = Harness::builder().with_size(vec2(1400.0, 60.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                crate::panels::options_bar(app, ui);
            }
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
    h.run_steps(4);
    h
}

fn caption(h: &Harness<'_, PhotocraftApp>) -> egui::Rect {
    // The visible caption, not the number field it names: kittest returns the labelled control
    // for `get_by_label`, whose rect would put the drag inside the numeric editor.
    h.query_all(egui_kittest::kittest::by().label("Opacity:").include_labels())
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::Label)
        .expect("the Opacity caption")
        .rect()
}

fn drag(h: &mut Harness<'_, PhotocraftApp>, at: Pos2, dx: f32) {
    h.hover_at(at);
    h.run();
    h.drag_at(at);
    h.run();
    for i in 1..=4 {
        h.hover_at(at + vec2(dx * i as f32 / 4.0, 0.0));
        h.run();
    }
    h.drop_at(at + vec2(dx, 0.0));
    h.run_steps(2);
}

#[test]
fn dragging_the_opacity_caption_scrubs_it_as_one_command() {
    let mut h = harness();
    let l = caption(&h);
    drag(&mut h, l.center(), 12.0);
    assert!(h.state().session.tools.brush.opacity > 0.5, "the caption scrubs the number");
    assert_eq!(h.state().session.journal.len(), 1, "one command for the whole gesture");
}

#[test]
fn typed_arithmetic_in_an_options_field_applies() {
    let mut h = harness();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Opacity").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text("25+5".into()));
    h.run();
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert!((h.state().session.tools.brush.opacity - 0.3).abs() < 0.0001);
}
