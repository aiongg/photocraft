//! #7520: the options bar's Select and Mask… button follows the command `select.selectAndMask`
//! (it maps to `select.refineEdge`): greyed out while nothing is selected, live after Select All.
use crate::PhotocraftApp;
use crate::state::Tool;
use egui::vec2;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use serde_json::json;

fn harness() -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 64, "height": 64})).unwrap();
    app.sync_views();
    app.ui.tool = Tool::RectMarquee;
    let mut h = Harness::builder().with_size(vec2(1400.0, 60.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                crate::panels::options_bar(app, ui);
            }
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::AppTheme::pro());
    h.run_steps(4);
    h
}

#[test]
fn select_and_mask_follows_the_selection() {
    let mut h = harness();
    assert!(h.get_by_label("Select and Mask…").accesskit_node().is_disabled(), "the button is greyed out with no selection");
    h.state_mut().run("select.all", json!({})).unwrap();
    h.run_steps(4);
    assert!(!h.get_by_label("Select and Mask…").accesskit_node().is_disabled(), "the button is live with a selection");
}
