//! Camera Raw through the real app shell and its existing agent entry point.
use photocraft_ui_egui::{PhotocraftApp, camera_raw_ui, control, dialogs};
use serde_json::{Value, json};

/// One pass of the hosted dialogs, as the app's frame runs them.
fn frame(app: &mut PhotocraftApp, ctx: &egui::Context) {
    ctx.run_ui(egui::RawInput::default(), |ui| dialogs::run(app, ui)).textures_delta.clear();
}

/// Runs frames until the develop of the settings in force has landed, and returns the dialog's
/// description as the agent entry point reports it. The first pass after a change starts its develop, and the
/// queue runs it when a request finishes it, as the agent entry point's `waitPreview` does.
fn settled(app: &mut PhotocraftApp, ctx: &egui::Context) -> Value {
    frame(app, ctx);
    for _ in 0..2000 {
        craft::ui::runtime::pending::finish_jobs(ctx, std::time::Duration::from_secs(30));
        frame(app, ctx);
        let described = control::inspect(app, ctx)["cameraRaw"].clone();
        if described["previewPending"] == false && !described["histogram"].is_null() {
            return described;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the Camera Raw preview never finished");
}

/// A control request, then the develop it asks for.
fn request(app: &mut PhotocraftApp, ctx: &egui::Context, params: Value) -> Value {
    let reply = camera_raw_ui::menu(app, ctx, "filter.cameraRaw", &params).unwrap().unwrap();
    if control::inspect(app, ctx)["cameraRaw"].is_null() {
        return reply;
    }
    settled(app, ctx)
}

#[test]
fn histogram_matches_visible_preview_at_all_supported_depths_and_cancel_commit_are_reversible() {
    for mode in ["rgb", "grayscale"] {
        for depth in [8, 16, 32] {
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
            let ctx = app.ctx().clone();
            app.run("file.new", json!({"width": 16, "height": 8, "mode": mode, "depth": depth})).unwrap();
            app.run("layer.new.layer", json!({})).unwrap();
            app.run("edit.fill", json!({"color": "#808080"})).unwrap();
            let original = app.session.active().unwrap().doc.clone();
            let steps = app.session.active().unwrap().history.past_len();
            let initial = request(&mut app, &ctx, json!({}));
            assert_eq!(initial["histogram"]["samples"], 128);
            let changed = request(&mut app, &ctx, json!({"ui": {"set": {"exposure": 1.0}}}));
            assert_ne!(changed["histogram"]["red"], initial["histogram"]["red"]);
            assert_eq!(app.session.active().unwrap().history.past_len(), steps);
            request(&mut app, &ctx, json!({"ui": {"cancel": true}}));
            assert!(std::sync::Arc::ptr_eq(&app.session.active().unwrap().doc, &original), "Cancel leaves the document alone");
            request(&mut app, &ctx, json!({"ui": {"set": {"exposure": 1.0}, "commit": true}}));
            assert_eq!(app.session.active().unwrap().history.past_len(), steps + 1, "OK adds one step");
            app.run("edit.undo", json!({})).unwrap();
            assert!(std::sync::Arc::ptr_eq(&app.session.active().unwrap().doc, &original));
            app.run("edit.redo", json!({})).unwrap();
            let committed = request(&mut app, &ctx, json!({}));
            assert_eq!(committed["histogram"]["red"], changed["histogram"]["red"], "{mode}/{depth}: graph and committed pixels disagree");
        }
    }
}
