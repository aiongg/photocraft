//! The sample document the screenshot harness builds (select the bottom layer, Escape, the Pro theme)
//! keeps the Background layer active, and its Layers row shows it selected.

use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;

use crate::PhotocraftApp;

#[test]
fn background_stays_active_and_its_row_selected_after_the_screenshot_harness_setup() {
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default())
    });
    h.run();
    let ctx = h.ctx.clone();
    let app = h.state_mut();
    app.run("file.new", json!({"width": 16, "height": 16})).unwrap();
    app.run("layer.new.layer", json!({})).unwrap();
    let background = app.session.active().unwrap().doc.layers[0].id;
    app.run("layer.selectBottom", json!({})).unwrap();
    let (req, _rx) = crate::control::ControlRequest::new(
        "ui.set",
        json!({"theme": "pro", "tool": "move", "panels": {"layers": true, "properties": true, "color": true, "dock": true, "options_bar": true, "toolbar": true, "status_bar": true}}),
    );
    crate::control::handle(app, &ctx, &req);
    h.run();
    h.key_press(egui::Key::Escape);
    h.run();
    assert_eq!(h.state().session.active().unwrap().active_layer, Some(background));
    assert_eq!(h.get_by_role_and_label(Role::TreeItem, "Background").accesskit_node().is_selected(), Some(true));
}
