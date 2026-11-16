//! #11987: the ruler stays drawn on the canvas whichever tool is active, and View › Extras (⌘H)
//! hides and shows it, also with the Ruler tool active. The ruler itself never changes.

use serde_json::json;

use crate::PhotocraftApp;
use crate::state::Tool;
use photocraft_doc::Ruler;

/// A 200 x 100 document with a ruler from 20,20 to 180,80 and its protractor arm to 20,90.
fn app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 200, "height": 100})).unwrap();
    app.run("image.analysis.rulerTool", json!({"start": [20, 20], "end": [180, 80], "protractor": [20, 90]})).unwrap();
    app
}

fn ruler(app: &PhotocraftApp) -> Option<Ruler> {
    app.session.active().unwrap().doc.measurement.ruler
}

/// The shapes one pass of the canvas body paints over a 200 x 100 canvas: nothing without a visible line.
fn painted(app: &mut PhotocraftApp) -> usize {
    let ctx = app.ctx().clone();
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 100.0));
    crate::testing::run_ui(&ctx, Default::default(), |ui| crate::analysis::canvas::body(app, ui, rect)).shapes.len()
}

fn extras(app: &mut PhotocraftApp) {
    let ctx = app.ctx().clone();
    crate::menus::invoke(app, &ctx, "view.extras", json!({})).unwrap();
}

#[test]
fn the_ruler_stays_drawn_under_other_tools() {
    let mut app = app();
    let before = ruler(&app);
    for tool in [Tool::Ruler, Tool::Brush, Tool::Move, Tool::RectMarquee] {
        app.ui.tool = tool;
        assert!(painted(&mut app) > 0, "{tool:?}: the line and the protractor arm are drawn");
        assert_eq!(ruler(&app), before, "{tool:?}: the ruler is unchanged");
    }
}

#[test]
fn view_extras_hides_and_shows_it_with_any_tool() {
    for tool in [Tool::Ruler, Tool::Brush] {
        let mut app = app();
        app.ui.tool = tool;
        let before = ruler(&app);
        assert!(app.ui.view.extras);
        extras(&mut app);
        assert!(!app.ui.view.extras);
        assert_eq!(painted(&mut app), 0, "{tool:?}: Extras off hides it");
        assert_eq!(ruler(&app), before, "{tool:?}: the measurement stays");
        extras(&mut app);
        assert!(painted(&mut app) > 0, "{tool:?}: Extras on shows it again");
        assert_eq!(ruler(&app), before);
    }
}

#[test]
fn a_new_document_shows_no_ruler_and_the_first_one_shows_its_own_again() {
    let mut app = app();
    app.ui.tool = Tool::Brush;
    app.run("file.new", json!({"width": 200, "height": 100})).unwrap();
    assert_eq!(painted(&mut app), 0, "the new document has none");
    app.run("document.activate", json!({"document": 0})).unwrap();
    assert!(painted(&mut app) > 0, "the first document's ruler is back");
}
