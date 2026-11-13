use super::*;

fn assert_pixel_edges(xf: ViewXform, ppp: f32) {
    for [x, y] in [[0.0, 0.0], [1.0, 1.0], [17.0, 29.0], [1500.0, 1000.0]] {
        let p = xf.to_screen(x, y) * ppp;
        assert!((p.x - p.x.round()).abs() < 0.001, "x edge: {p:?}, ppp {ppp}");
        assert!((p.y - p.y.round()).abs() < 0.001, "y edge: {p:?}, ppp {ppp}");
    }
}

/// Tools and overlays that map through `ViewXform::active` land where the canvas draws: the canvas's pixel alignment
/// at 100 % in an odd-sized viewport, with the saved pan untouched.
#[test]
fn active_transform_uses_the_same_alignment_without_changing_saved_pan() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", serde_json::json!({"width": 3000, "height": 2000})).unwrap();
    app.sync_views();
    for ppp in [1.0, 1.25, 1.5, 2.0] {
        app.ppp = ppp;
        app.last_canvas_rect = Rect::from_min_size(pos2(40.0 / ppp, 100.0 / ppp), vec2(523.0 / ppp, 471.0 / ppp));
        app.ui.views[0].center = [1500.0, 1000.0];
        app.ui.views[0].zoom = 1.0;
        let before = app.ui.views[0].center;
        let active = ViewXform::active(&app).unwrap();
        assert_pixel_edges(active, ppp);
        assert_eq!(app.ui.views[0].center, before);
    }
}

/// Default Crop slides the view under its frame with fractional pointer distances: the mapping stays continuous, as
/// the canvas shows it; Classic Crop moves the frame and keeps the alignment.
#[test]
fn crop_image_movement_keeps_its_continuous_camera_mapping() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.ui.tool = Tool::Crop;
    let xf = ViewXform {
        rect: Rect::from_min_size(pos2(40.0, 100.0), vec2(523.0, 471.0)),
        zoom: 1.0,
        center: [1500.0, 1000.0],
        flip: false,
        rotation: 0.0,
        aspect: 1.0,
    };
    assert_eq!(xf.shown(&app, 1.0).center, xf.center);
    app.ui.tool_options.crop_shield.classic_mode = true;
    assert_pixel_edges(xf.shown(&app, 1.0), 1.0);
}
