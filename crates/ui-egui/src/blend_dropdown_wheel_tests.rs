//! #1747: the scroll wheel over the Layers panel's Blend Mode dropdown steps through the modes,
//! as in Photoshop: down picks the next mode, up the previous one, one history step per notch.

use egui::accesskit::Role;
use egui::{Pos2, Rect, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use photocraft_doc::BlendMode;
use serde_json::json;

use crate::PhotocraftApp;

/// The app with a new document after `setup` (its last layer is the active one).
fn harness(setup: impl FnOnce(&mut PhotocraftApp) + 'static) -> Harness<'static, PhotocraftApp> {
    let mut h = Harness::builder().with_size(vec2(1440.0, 900.0)).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 64, "height": 48})).unwrap();
        setup(&mut app);
        app
    });
    h.run_steps(8);
    h
}

/// A filled pixel layer above the Background, active.
fn pixel_layer(app: &mut PhotocraftApp) {
    app.run("layer.new.layer", json!({})).unwrap();
    app.run("edit.fill", json!({"color": "#cc3366"})).unwrap();
}

fn blend(h: &Harness<'_, PhotocraftApp>) -> BlendMode {
    let st = h.state().session.active().unwrap();
    st.doc.layer(st.active_layer.unwrap()).unwrap().blend
}

fn steps(h: &Harness<'_, PhotocraftApp>) -> usize {
    h.state().session.active().unwrap().history.past_len()
}

/// The Layers panel's dropdown: the lowest one (the options bar and the title bar have some too).
fn dropdown(h: &Harness<'_, PhotocraftApp>) -> Rect {
    h.query_all_by_role(Role::ComboBox).map(|n| n.rect()).max_by(|a, b| a.top().total_cmp(&b.top())).unwrap()
}

/// `lines` wheel lines at `at` in one event: negative is down.
fn wheel(h: &mut Harness<'_, PhotocraftApp>, at: Pos2, lines: f32) {
    h.hover_at(at);
    h.run_steps(1);
    h.event(egui::Event::MouseWheel {
        source: egui::MouseWheelSource::Unknown,
        unit: egui::MouseWheelUnit::Line,
        delta: vec2(0.0, lines),
        phase: egui::TouchPhase::Move,
        modifiers: Default::default(),
    });
    h.run_steps(3);
}

#[test]
fn the_wheel_stops_at_the_first_and_last_blend_mode() {
    let mut h = harness(|app| {
        pixel_layer(app);
        let id = app.session.active().unwrap().active_layer.unwrap().0;
        app.run("layer.setProps", json!({"layer": id, "blend": "Luminosity"})).unwrap();
    });
    let at = dropdown(&h).center();
    let undo = steps(&h);
    wheel(&mut h, at, -1.0);
    assert_eq!((blend(&h), steps(&h)), (BlendMode::Luminosity, undo), "down on the last mode");
    let id = h.state().session.active().unwrap().active_layer.unwrap().0;
    h.state_mut().run("layer.setProps", json!({"layer": id, "blend": "Normal"})).unwrap();
    h.run_steps(2);
    let undo = steps(&h);
    wheel(&mut h, at, 1.0);
    assert_eq!((blend(&h), steps(&h)), (BlendMode::Normal, undo), "up on the first mode");
}

#[test]
fn the_wheel_leaves_the_greyed_background_blend_mode_alone() {
    let mut h = harness(|_| {});
    let st = h.state().session.active().unwrap();
    assert!(crate::doc_props_ui::is_background(&st.doc, st.doc.layer(st.active_layer.unwrap()).unwrap()));
    let at = dropdown(&h).center();
    let undo = steps(&h);
    wheel(&mut h, at, -1.0);
    wheel(&mut h, at, -1.0);
    assert_eq!((blend(&h), steps(&h)), (BlendMode::Normal, undo));
}

/// The open list's rect (the one foreground area: no other popup is open).
fn open_list(h: &Harness<'_, PhotocraftApp>) -> Option<Rect> {
    h.ctx.memory(|m| m.layer_ids().filter(|l| l.order == egui::Order::Foreground).find_map(|l| m.area_rect(l.id)))
}

/// A plain click on the Layers panel's Blend Mode box, released where it was pressed (after one
/// frame, or after a few as a person's click takes), opens the list beside the box and chooses
/// nothing (#5495 review): the full-height list doesn't fit below the box low in the panel, and
/// it must not open over the box, where the release would pick the row under the pointer.
#[test]
fn a_click_on_the_blend_mode_box_opens_the_list_beside_it_and_changes_nothing() {
    for held in [6, 1] {
        let mut h = harness(pixel_layer);
        let undo = steps(&h);
        let button = dropdown(&h);
        let at = button.center();
        h.hover_at(at);
        h.run_steps(1);
        for (pressed, frames) in [(true, held), (false, 4)] {
            h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() });
            h.run_steps(frames);
        }
        assert_eq!((blend(&h), steps(&h)), (BlendMode::Normal, undo), "held {held}: the click chose nothing");
        let Some(list) = open_list(&h) else { panic!("held {held}: the list is open") };
        assert!(!list.intersects(button.shrink(1.0)), "held {held}: the list {list:?} leaves the box {button:?} free");
        assert!(list.top() >= 0.0 && list.bottom() <= 900.0, "held {held}: the list {list:?} is in the window");
    }
}
