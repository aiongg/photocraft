//! Shape tools expose the Pen's Make controls when drawing a path (#13348).

use craft::draw::ui::VectorMode;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;

use crate::{PhotocraftApp, state::Tool};

#[test]
fn shape_tools_show_make_controls_only_in_path_mode() {
    for tool in [Tool::Rectangle, Tool::EllipseShape, Tool::Line, Tool::Polygon] {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 200})).unwrap();
        app.ui.tool = tool;
        app.ui.tool_options.shape_mode = VectorMode::Path;
        let mut h = Harness::builder().with_context(app.ctx().clone()).with_size(vec2(1800.0, 400.0)).build_ui_state(
            move |ui, app: &mut PhotocraftApp| {
                ui.horizontal(|ui| super::options_bar(app, ui, tool));
            },
            app,
        );
        h.run_steps(4);
        let x = |label: &str| h.get_by_label(label).rect().min.x;
        let order = [x("Make:"), x("Selection…"), x("Mask"), x("Shape")];
        assert!(order.is_sorted_by(|a, b| a < b), "{tool:?}: {order:?}");
        for label in ["Selection…", "Mask", "Shape"] {
            assert!(h.get_by_label(label).accesskit_node().is_disabled(), "{tool:?}: {label}");
        }
        h.state_mut().run("path.set", json!({"path": {"subpaths": [{"closed": true, "knots": [[20, 20], [120, 20], [120, 120]]}]}})).unwrap();
        h.run_steps(2);
        for label in ["Selection…", "Mask", "Shape"] {
            assert!(!h.get_by_label(label).accesskit_node().is_disabled(), "{tool:?}: {label}");
        }
        h.state_mut().ui.tool_options.shape_mode = VectorMode::Shape;
        h.run_steps(2);
        assert!(h.query_by_label("Make:").is_none(), "{tool:?}: Shape mode");
    }
}
