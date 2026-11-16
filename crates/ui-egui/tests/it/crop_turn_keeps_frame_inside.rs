//! Crop tool, default mode: turning the image keeps the frame inside the turned image (#10869).
//! The turn rule itself is the framework's (`CropBox::kept_inside`); this file keeps the app's
//! exceptions.
//!
//! Measured in the reference application on an 800 × 500 document, turning by a drag outside the
//! frame with Auto Center Preview off. Not measured, so they keep their behaviour: with Fill set to
//! Content-Aware Fill, with a frame already reaching past the canvas at the press, and in Classic
//! Mode (where the frame turns over the image) the frame keeps its size and place.
//!
//! Driven as an agent does, through `ui.pointer` (document px as the view showed them at the
//! press), in an egui_kittest harness so the canvas takes each call's input on the frames after it.

use craft::draw::ui::crop::CropFill;
use photocraft_ui_egui::PhotocraftApp;
use photocraft_ui_egui::control::{ControlRequest, Reply, handle};
use photocraft_ui_egui::crop_ui::frame;
use serde_json::{Value, json};

type Harness = egui_kittest::Harness<'static, PhotocraftApp>;

/// The document's size.
const W: f64 = 800.0;
const H: f64 = 500.0;
/// The whole canvas, the default frame.
const CANVAS: [f64; 4] = [0.0, 0.0, W, H];

/// An 800 × 500 white document, the Crop tool in the default mode with Auto Center Preview off,
/// and `frame_rect` as the pending crop frame (the window is wide enough that the canvas, beside the docks, shows 500 px
/// right of the frame's centre, where `turn` presses): the default frame when it is the whole canvas, else
/// drawn from its bottom-right corner to its top-left one (a press on the default frame's corner
/// handle would resize it).
fn app(frame_rect: [f64; 4]) -> Harness {
    let mut h = Harness::builder().with_size(egui::vec2(1800.0, 900.0)).with_max_steps(64).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::with_context(cc.egui_ctx.clone(), photocraft_engine::Session::new(), Default::default());
        app.session.prefs.edit(|p| p.interface.language = "en".into());
        app.run("file.new", json!({"width": W, "height": H, "background": "white"})).expect("new");
        app.ui.extras.snap = false;
        app.ui.view.show.smart_guides = false;
        app.ui.tool_options.crop.shield.auto_center_preview = false;
        app
    });
    h.run_steps(4);
    control(&mut h, "ui.set", json!({"tool": "crop", "zoom": 1.0, "center": [W / 2.0, H / 2.0]}));
    assert!(!h.state().ui.tool_options.crop.shield.classic_mode, "Photoshop's default mode");
    if frame_rect != CANVAS {
        let events = json!([
            {"kind": "down", "x": frame_rect[2], "y": frame_rect[3]},
            {"kind": "move", "x": frame_rect[0], "y": frame_rect[1]},
            {"kind": "up", "x": frame_rect[0], "y": frame_rect[1]},
        ]);
        control(&mut h, "ui.pointer", json!({"events": events}));
    }
    assert_eq!(rect(&h), Some(frame_rect));
    h
}

/// Runs one control-channel request, which must succeed, then frames so the canvas takes any
/// queued input.
fn control(h: &mut Harness, method: &str, params: Value) {
    let ctx = h.ctx.clone();
    let (req, _rx) = ControlRequest::new(method, params.clone());
    match handle(h.state_mut(), &ctx, &req) {
        Reply::Done(v) => assert_eq!(v["ok"], true, "{method} {params}: {v}"),
        Reply::AfterInput => {}
        _ => panic!("{method} {params}: not done"),
    }
    h.run_steps(6);
}

/// The point 500 px right of the frame's centre turned `deg` degrees clockwise about it: a press
/// there is outside every frame below, and moving to it from the unturned point turns the image
/// `deg` degrees clockwise.
fn around(frame_rect: [f64; 4], deg: f64) -> [f64; 2] {
    let c = [(frame_rect[0] + frame_rect[2]) / 2.0, (frame_rect[1] + frame_rect[3]) / 2.0];
    let (s, k) = deg.to_radians().sin_cos();
    [c[0] + 500.0 * k, c[1] + 500.0 * s]
}

/// A turn of the image: a press outside `frame_rect`, a move to `deg`, a release there (one
/// `ui.pointer` call each).
fn turn(h: &mut Harness, frame_rect: [f64; 4], deg: f64) {
    let p = around(frame_rect, 0.0);
    control(h, "ui.pointer", json!({"events": [{"kind": "down", "x": p[0], "y": p[1]}]}));
    let p = around(frame_rect, deg);
    control(h, "ui.pointer", json!({"events": [{"kind": "move", "x": p[0], "y": p[1]}]}));
    control(h, "ui.pointer", json!({"events": [{"kind": "up", "x": p[0], "y": p[1]}]}));
}

/// The pending frame's rect as `[x0, y0, x1, y1]`.
fn rect(h: &Harness) -> Option<[f64; 4]> {
    frame(h.state()).map(|f| [f.rect.x0, f.rect.y0, f.rect.x1, f.rect.y1])
}

/// The pending frame's turn in degrees.
fn angle(h: &Harness) -> f64 {
    frame(h.state()).expect("a frame").angle
}

#[test]
fn content_aware_fill_a_frame_past_the_canvas_and_classic_mode_keep_the_frame() {
    let past = [-50.0, 100.0, 300.0, 300.0];
    for (what, start) in [("content-aware", CANVAS), ("past the canvas", past), ("classic", CANVAS)] {
        let mut h = app(start);
        h.state_mut().ui.tool_options.crop.fill = if what == "content-aware" { CropFill::ContentAware } else { CropFill::Background };
        h.state_mut().ui.tool_options.crop.shield.classic_mode = what == "classic";
        turn(&mut h, start, 6.3);
        assert!(angle(&h).abs() > 6.0, "{what}: turned, {}", angle(&h));
        assert_eq!(rect(&h), Some(start), "{what}");
    }
}
