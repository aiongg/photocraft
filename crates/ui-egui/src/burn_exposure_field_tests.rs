//! #12669: the Burn and Dodge bar's Exposure is a whole-number 1..100 field with the reference
//! application's "Closest value inserted" alert for a value typed past the range, and a slider
//! arrow from 1 to 100. A typed fraction in range is rounded; a value that is no number keeps the
//! previous one silently. The field's draft handling (text over an out-of-range entry, the input
//! limit, text and the leaving key in one pass) is the framework number field's.

use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;

use crate::PhotocraftApp;
use crate::state::Tool;

fn bar(tool: Tool) -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 80, "height": 80})).unwrap();
    app.ui.tool = tool;
    crate::theme::install_fonts(app.ctx());
    crate::theme::apply(app.ctx(), &crate::theme::AppTheme::pro());
    let mut h = Harness::builder().with_context(app.ctx().clone()).with_size(vec2(1800.0, 300.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            crate::panels::options_bar(app, ui);
            // The alert a field raises is a hosted dialog, drawn by the dialog host.
            crate::dialogs::run(app, ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn exposure_field(h: &Harness<'static, PhotocraftApp>) -> egui::Rect {
    h.query_all_by_role(Role::SpinButton).find(|n| n.accesskit_node().label().as_deref() == Some("Exposure:")).expect("the Exposure field").rect()
}

fn type_within(h: &mut Harness<'static, PhotocraftApp>, text: &str) {
    let at = exposure_field(h).center();
    h.hover_at(at);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text(text.into()));
    h.run_steps(1);
}

fn type_exposure(h: &mut Harness<'static, PhotocraftApp>, text: &str) {
    type_within(h, text);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
}

fn exposure(h: &Harness<'static, PhotocraftApp>) -> f32 {
    h.state().ui.tool_options.exposure
}

/// The messages of the open framework alerts.
fn alerts(h: &Harness<'static, PhotocraftApp>) -> Vec<String> {
    h.state().framework.dialogs.open_dialogs().iter().filter_map(|d| d.fields.as_ref()?.get("message")?.as_str().map(str::to_string)).collect()
}

/// The one alert says the value was changed to the closest one in range.
fn is_range_alert(shown: &[String]) -> bool {
    shown.len() == 1 && shown[0].contains("Closest value inserted")
}

fn ok(h: &mut Harness<'static, PhotocraftApp>) {
    h.get_by_label("OK").click();
    h.run_steps(4);
}

fn click_at(h: &mut Harness<'static, PhotocraftApp>, p: egui::Pos2) {
    h.hover_at(p);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
    h.run_steps(3);
}

#[test]
fn the_bar_shows_exposure_between_1_and_100() {
    for tool in [Tool::Burn, Tool::Dodge] {
        let h = bar(tool);
        let field = h.query_all_by_role(Role::SpinButton).find(|n| n.accesskit_node().label().as_deref() == Some("Exposure:")).expect("the Exposure field");
        assert_eq!(field.accesskit_node().numeric_value(), Some(50.0), "{tool:?}: the default exposure");
    }
}

#[test]
fn exposure_typed_past_the_range_takes_the_closest_value_and_alerts() {
    for (typed, want) in [("0", 1.0), ("-5", 1.0), ("150", 100.0)] {
        let mut h = bar(Tool::Burn);
        type_exposure(&mut h, typed);
        assert_eq!(exposure(&h), want, "{typed}");
        assert!(is_range_alert(&alerts(&h)), "{typed}: one alert, got {:?}", alerts(&h));
        ok(&mut h);
        assert!(alerts(&h).is_empty(), "{typed}: OK closes the alert");
        assert_eq!(exposure(&h), want, "{typed}: the closest value is kept");
    }
}

#[test]
fn exposure_in_range_is_a_whole_number_without_an_alert() {
    for (typed, want) in [("33.7", 34.0), ("1", 1.0), ("100", 100.0), ("72", 72.0)] {
        let mut h = bar(Tool::Burn);
        type_exposure(&mut h, typed);
        assert_eq!(exposure(&h), want, "{typed}");
        assert!(alerts(&h).is_empty(), "{typed} is in range");
    }
}

#[test]
fn exposure_that_is_no_number_keeps_the_value_without_an_alert() {
    let mut h = bar(Tool::Burn);
    type_exposure(&mut h, "abc");
    assert_eq!(exposure(&h), 50.0, "the previous value stays");
    assert!(alerts(&h).is_empty(), "no number, no alert");
}

#[test]
fn the_arrow_opens_a_slider_from_1_to_100() {
    let mut h = bar(Tool::Burn);
    assert!(h.query_all_by_role(Role::Slider).next().is_none(), "the slider stays closed");
    h.get_by_role_and_label(Role::Button, "Exposure").click();
    h.run_steps(4);
    let slider = h.get_by_role_and_label(Role::Slider, "Exposure").rect();
    click_at(&mut h, egui::pos2(slider.right() - 3.0, slider.center().y));
    assert_eq!(exposure(&h), 100.0, "the slider's far end");
    click_at(&mut h, egui::pos2(slider.left() + 3.0, slider.center().y));
    assert_eq!(exposure(&h), 1.0, "the slider's near end");
    click_at(&mut h, slider.center());
    assert_eq!(exposure(&h).fract(), 0.0, "the slider gives whole numbers");
    assert!((2.0..=99.0).contains(&exposure(&h)), "between the ends: {}", exposure(&h));
    assert!(alerts(&h).is_empty(), "the slider does not alert");
}

#[test]
fn the_dodge_tool_shares_the_field() {
    let mut h = bar(Tool::Dodge);
    type_exposure(&mut h, "150");
    assert_eq!(exposure(&h), 100.0);
    assert!(is_range_alert(&alerts(&h)), "the Dodge bar alerts the same way: {:?}", alerts(&h));
}
