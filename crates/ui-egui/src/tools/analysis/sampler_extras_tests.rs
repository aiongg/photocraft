//! #11741: View › Extras (⌘H) hides the Color Sampler markers on the canvas, as in Photoshop. The
//! points stay in the document and the Info panel keeps reading them; Extras on again draws the
//! markers back at the same places with the same numbers.

use super::*;

/// A 200 × 100 document with two samplers and the Color Sampler tool.
fn app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 200, "height": 100})).unwrap();
    app.ui.tool = Tool::ColorSampler;
    app.run("view.colorSamplers.add", json!({"x": 50, "y": 50})).unwrap();
    app.run("view.colorSamplers.add", json!({"x": 120, "y": 30})).unwrap();
    app
}

/// Every shape the canvas body paints, and the texts with where they are, in one pass over a 200 x 100 canvas.
fn painted(app: &mut PhotocraftApp) -> (usize, Vec<(String, egui::Pos2)>) {
    let ctx = app.ctx().clone();
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 100.0));
    let mut o = crate::testing::run_ui(&ctx, Default::default(), |ui| super::canvas::body(app, ui, rect));
    o.textures_delta.clear();
    let numbers = o
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some((t.galley.text().to_owned(), t.pos)),
            _ => None,
        })
        .collect();
    (o.shapes.len(), numbers)
}

fn toggle_extras(app: &mut PhotocraftApp) {
    let ctx = app.ctx().clone();
    crate::menus::invoke(app, &ctx, "view.extras", json!({})).unwrap();
}

/// The Info panel's sampler readouts, read afresh rather than from the per-revision cache.
fn readouts(app: &mut PhotocraftApp) -> Vec<Value> {
    app.info_samplers = None;
    crate::panels::info::sampler_readouts(app).to_vec()
}

#[test]
fn extras_off_hides_the_sampler_markers_and_keeps_the_points_and_readouts() {
    let mut app = app();
    let (shown, numbers) = painted(&mut app);
    assert!(shown > 0, "the markers are drawn with Extras on");
    let points = app.session.active().unwrap().color_samplers.clone();
    let info = readouts(&mut app);
    assert_eq!(info.len(), 2, "the Info panel lists both samplers: {info:?}");

    toggle_extras(&mut app);
    assert!(!app.ui.view.extras);
    assert_eq!(painted(&mut app).0, 0, "no marker is drawn with Extras off");
    assert_eq!(app.session.active().unwrap().color_samplers, points, "the samplers stay in the document");
    assert_eq!(readouts(&mut app), info, "the Info panel readouts are unchanged");

    toggle_extras(&mut app);
    assert!(app.ui.view.extras);
    assert_eq!(painted(&mut app), (shown, numbers), "Extras on again draws the same markers at the same places");
}
