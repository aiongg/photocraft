//! #11910: with the Background Eraser, a canvas right-click opens the Brush Preset picker's compact
//! form (no presets, search or gear) with the Tolerance control; other tools get the full picker. The
//! two pen controls store apart: Tolerance with the tool, "Size:" with its brush.

use crate::app::framework::BuildApp;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;

use super::*;
use crate::PhotocraftApp;
use crate::state::Tool;

fn app(tool: Tool) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
    app.run("layer.new.layer", json!({})).unwrap();
    app.run("edit.fill", json!({"color": "#dc1e1e"})).unwrap();
    app.ui.tool = tool;
    app.sync_views();
    app
}

/// The canvas body; the first frame installs fonts and the theme.
fn harness(app: PhotocraftApp) -> Harness<'static, PhotocraftApp> {
    crate::theme::install_fonts(app.ctx());
    crate::theme::apply(app.ctx(), &crate::theme::AppTheme::pro());
    let mut h = Harness::builder().with_context(app.ctx().clone()).with_size(egui::vec2(1200.0, 800.0)).with_step_dt(1.0 / 60.0).build_app(
        move |ui, app: &mut PhotocraftApp| {
            crate::paint_mouse::sync_tool_brush(app);
            egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
        },
        app,
    );
    h.run_steps(4);
    h
}

/// A secondary press and release at the canvas centre.
fn right_click(h: &mut Harness<'static, PhotocraftApp>) {
    let c = h.state().last_canvas_rect.center();
    h.event(egui::Event::PointerMoved(c));
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE });
        h.run_steps(2);
    }
    h.run_steps(2);
}

fn select<'h>(h: &'h Harness<'static, PhotocraftApp>, label: &str) -> egui_kittest::Node<'h> {
    h.get_all_by_role(Role::ComboBox).find(|n| n.accesskit_node().label().as_deref() == Some(label)).unwrap_or_else(|| panic!("the {label} select"))
}

#[test]
fn a_right_click_opens_the_compact_picker_for_the_background_eraser_and_the_full_picker_for_other_tools() {
    for (tool, compact) in [(Tool::BackgroundEraser, true), (Tool::Brush, false)] {
        let mut h = harness(app(tool));
        let steps = h.state().session.active().unwrap().history.past_len();
        right_click(&mut h);
        assert_eq!(h.query_all_by_label("Tolerance:").next().is_some(), compact, "{tool:?}: the Tolerance control");
        assert_eq!(h.query_all_by_label("Search Brushes").next().is_none(), compact, "{tool:?}: presets and search only in the full picker");
        assert_eq!(h.state().session.active().unwrap().history.past_len(), steps, "{tool:?}: the right-click erases nothing");
    }
}

#[test]
fn tolerance_is_stored_with_the_tool_and_size_with_its_brush() {
    let mut h = harness(app(Tool::BackgroundEraser));
    right_click(&mut h);
    select(&h, "Tolerance:").click();
    h.run_steps(2);
    h.get_all_by_label("Off").last().expect("the Off entry").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.tool_options.bg_tolerance_control, OFF);
    select(&h, "Size:").click();
    h.run_steps(2);
    h.get_all_by_label("Pen Pressure").last().expect("the Pen Pressure entry").click();
    h.run_steps(3);
    assert!(h.state().session.tools.brush.pressure_size, "Size: Pen Pressure is the brush's pressure for size");
    let mut b = BrushSettings::default();
    for c in [STYLUS_WHEEL, PEN_PRESSURE, OFF] {
        set_size_control(&mut b, c);
        assert_eq!(size_control(&b), c);
    }
}

