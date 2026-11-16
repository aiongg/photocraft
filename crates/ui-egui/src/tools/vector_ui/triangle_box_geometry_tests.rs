//! #13350: the Triangle tool's drag fills its box, apex at the top centre, in either direction and
//! from the centre with Alt, and Create Triangle sends the same triangle. Shift keeps the
//! equilateral triangle of the constrained square's circle.

use egui::Modifiers;
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::canvas::{ToolEvent, tool_event};
use crate::state::Tool;

fn app(w: u32, h: u32) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": w, "height": h})).unwrap();
    app.sync_views();
    app.ui.tool = Tool::Triangle;
    app
}

/// A canvas drag from `from` to `to` with `mods` held from the first move, as the canvas sends it.
fn drag(app: &mut PhotocraftApp, from: [f64; 2], to: [f64; 2], mods: Modifiers) {
    tool_event(app, ToolEvent::Down { x: from[0], y: from[1], pressure: 1.0 }, Modifiers::NONE);
    tool_event(app, ToolEvent::Move { x: (from[0] + to[0]) / 2.0, y: (from[1] + to[1]) / 2.0, pressure: 1.0 }, mods);
    tool_event(app, ToolEvent::Move { x: to[0], y: to[1], pressure: 1.0 }, mods);
    tool_event(app, ToolEvent::Up { x: to[0], y: to[1] }, mods);
}

fn steps(app: &PhotocraftApp) -> usize {
    app.session.active().unwrap().history.entries().len()
}

/// `shape.info` of the active layer.
fn info(app: &mut PhotocraftApp) -> Value {
    let id = app.session.active().unwrap().active_layer.unwrap().0;
    app.session.execute("shape.info", json!({"layer": id})).unwrap()
}

fn anchors(v: &Value) -> Vec<(f64, f64)> {
    let knots = v["path"]["subpaths"][0]["knots"].as_array().unwrap();
    knots.iter().map(|k| (k["anchor"][0].as_f64().unwrap(), k["anchor"][1].as_f64().unwrap())).collect()
}

fn close(got: &[(f64, f64)], want: &[(f64, f64)], tol: f64) {
    assert_eq!(got.len(), want.len(), "{got:?}");
    for (g, w) in got.iter().zip(want) {
        assert!((g.0 - w.0).abs() <= tol && (g.1 - w.1).abs() <= tol, "{got:?} vs {want:?}");
    }
}

#[test]
fn a_drag_fills_its_box_in_either_direction() {
    for (from, to) in [([400.0, 300.0], [600.0, 400.0]), ([600.0, 400.0], [400.0, 300.0])] {
        let mut app = app(800, 600);
        let n = steps(&app);
        drag(&mut app, from, to, Modifiers::NONE);
        let v = info(&mut app);
        assert_eq!(anchors(&v), [(500.0, 300.0), (600.0, 400.0), (400.0, 400.0)]);
        assert_eq!(v["live"]["kind"], "triangle");
        assert_eq!(steps(&app), n + 1);
    }
}

#[test]
fn alt_after_the_start_draws_from_the_centre() {
    let mut app = app(800, 600);
    drag(&mut app, [400.0, 300.0], [500.0, 350.0], Modifiers::ALT);
    assert_eq!(anchors(&info(&mut app)), [(400.0, 250.0), (500.0, 350.0), (300.0, 350.0)]);
}

#[test]
fn the_dialog_and_the_drag_send_the_same_triangle() {
    let mut dialog = app(800, 600);
    let id = crate::dialogs::shape_dialog::open(&mut dialog, craft::draw::ui::shape_drag::ShapeKind::Triangle, craft::behavior::kurbo::Point::new(400.0, 300.0)).unwrap();
    crate::dialogs::testing::confirm(&mut dialog, id).unwrap();
    let mut dragged = app(800, 600);
    drag(&mut dragged, [400.0, 300.0], [500.0, 400.0], Modifiers::NONE);
    let (a, b) = (info(&mut dialog), info(&mut dragged));
    assert_eq!(a["live"], b["live"]);
    assert_eq!(anchors(&a), [(450.0, 300.0), (500.0, 400.0), (400.0, 400.0)]);
}

#[test]
fn shift_still_draws_the_equilateral_triangle_of_the_squares_circle() {
    for mods in [Modifiers::SHIFT, Modifiers::SHIFT | Modifiers::ALT] {
        let mut app = app(800, 600);
        let (start, end) = ([400.0, 300.0], [600.0, 400.0]);
        // The square of the drag's shorter side (100), drawn from the press, or from the centre with Alt.
        let (x, y, m) = if mods.alt { (300.0, 200.0, 200.0) } else { (400.0, 300.0, 100.0) };
        drag(&mut app, start, end, mods);
        let v = info(&mut app);
        assert_eq!(v["live"]["kind"], "triangle");
        let s60 = 60f64.to_radians().sin();
        let want = [(x + m / 2.0, y), (x + m / 2.0 + m / 2.0 * s60, y + 0.75 * m), (x + m / 2.0 - m / 2.0 * s60, y + 0.75 * m)];
        let got = anchors(&v);
        close(&got, &want, 1e-9);
        let d = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1);
        let sides = [d(got[0], got[1]), d(got[1], got[2]), d(got[2], got[0])];
        assert!((sides[0] - sides[1]).abs() < 1e-9 && (sides[1] - sides[2]).abs() < 1e-9, "{sides:?}");
    }
}

#[test]
fn the_corner_widget_rounds_a_live_triangle() {
    let mut app = app(200, 200);
    app.run("shape.create", json!({"kind": "triangle", "rect": [40, 40, 120, 120], "cornerRadius": 20})).unwrap();
    app.ui.tool = Tool::PathSelection;
    let n = steps(&app);
    let cy = 40.0 + 20.0 * 5f64.sqrt();
    let to = cy + 4.0 * 5f64.sqrt();
    tool_event(&mut app, ToolEvent::Down { x: 100.0, y: cy, pressure: 1.0 }, Modifiers::NONE);
    tool_event(&mut app, ToolEvent::Move { x: 100.0, y: to, pressure: 1.0 }, Modifiers::NONE);
    tool_event(&mut app, ToolEvent::Up { x: 100.0, y: to }, Modifiers::NONE);
    let v = info(&mut app);
    assert_eq!(v["live"]["kind"], "triangle");
    assert_eq!(v["live"]["rect"], json!([40.0, 40.0, 120.0, 120.0]));
    assert!((v["live"]["corner_radius"].as_f64().unwrap() - 24.0).abs() < 1e-9, "{}", v["live"]);
    assert_eq!(anchors(&v).len(), 6);
    assert_eq!(steps(&app), n + 1);
    assert!(app.session.undo());
    assert_eq!(info(&mut app)["live"]["corner_radius"].as_f64(), Some(20.0));
}
