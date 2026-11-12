//! #1318: the Layers panel's Blend Mode dropdown takes the arrow keys once it is open, ahead of
//! the tool shortcuts (the Move tool's arrow-key nudge).

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::PhotocraftApp;

/// #9065 (CodeRabbit on #9314): with the Layers panel's options menu open, a command shortcut
/// closes the menu and runs, as Photoshop's panel flyout does (⌘1 there closes it and zooms to
/// 100%).
#[test]
fn a_command_shortcut_closes_the_open_panel_menu_and_runs() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).with_max_steps(64).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 3000, "height": 2000})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        app
    });
    h.run_steps(6);
    assert_ne!(h.state().ui.views[0].zoom, 1.0, "the document starts fitted, not at 100%");
    let group = crate::panel_docking::probe::group_rect(h.state(), &h.ctx, "layers").expect("layers group");
    h.query_all_by_label("Panel menu").find(|n| group.contains(n.rect().center())).expect("the Layers panel menu").click();
    h.run_steps(2);
    assert!(h.query_by_label("Collapse All Groups").is_some(), "the Layers panel menu is open");
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num1);
    h.run_steps(3);
    assert_eq!(h.state().ui.views[0].zoom, 1.0, "⌘1 (100%) runs with the menu open");
    assert!(h.query_by_label("Collapse All Groups").is_none(), "and closes the menu");
}

/// #9065 (CodeRabbit on #9314): Layer Style's contour picker keeps its open state in the
/// dialog's fields rather than egui's popup memory; a command shortcut closes it too and runs
/// (⌘1 zooms to 100%, which Layer Style lets through).
#[test]
fn a_command_shortcut_closes_the_open_contour_picker_and_runs() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).with_max_steps(64).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 3000, "height": 2000})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        app
    });
    h.run_steps(6);
    assert_ne!(h.state().ui.views[0].zoom, 1.0, "the document starts fitted, not at 100%");
    crate::layer_style::open(h.state_mut(), Some("dropShadow")).expect("Layer Style opens");
    h.run_steps(4);
    let picker = "contourPicker";
    let d = h.state_mut().ui.dialogs.last_mut().unwrap();
    let fx = d.fields["selected"].clone();
    d.fields.insert(picker.into(), json!({"effect": fx, "key": "contour"}));
    h.run_steps(3);
    assert!(h.query_by_label("Load Contours…").is_some(), "the contour picker is open");
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num1);
    h.run_steps(3);
    assert_eq!(h.state().ui.views[0].zoom, 1.0, "⌘1 (100%) runs with the picker open");
    assert!(h.query_by_label("Load Contours…").is_none(), "and closes the picker");
    assert!(!h.state().ui.dialogs.last().unwrap().fields.contains_key(picker), "and forgets it was open");
}
