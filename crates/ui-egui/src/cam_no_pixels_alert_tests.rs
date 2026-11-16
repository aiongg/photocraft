//! #12192: with no selection, a Content-Aware Move drag that encloses no area (a straight line or a
//! click) warns "No pixels are selected." with "Don't show again", adds no History state and
//! selects nothing. A drag that encloses an area still makes the selection; a drag with a
//! selection keeps its behaviour.

use std::path::PathBuf;
use std::sync::Arc;

use egui::Modifiers;
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::canvas::{ToolEvent, tool_event};
use crate::state::Tool;

/// A 200 x 100 white document with nothing selected and the Content-Aware Move tool.
fn app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 200, "height": 100, "background": "white"})).unwrap();
    app.ui.tool = Tool::ContentAwareMove;
    app
}

/// A drag through `points` (the first is the press), released at the last.
fn drag(app: &mut PhotocraftApp, points: &[[f64; 2]]) {
    let m = Modifiers::NONE;
    let [x, y] = points[0];
    tool_event(app, ToolEvent::Down { x, y, pressure: 1.0 }, m);
    for &[x, y] in &points[1..] {
        tool_event(app, ToolEvent::Move { x, y, pressure: 1.0 }, m);
    }
    let [x, y] = points[points.len() - 1];
    tool_event(app, ToolEvent::Up { x, y }, m);
}

const LINE: &[[f64; 2]] = &[[20.0, 50.0], [60.0, 50.0], [100.0, 50.0], [140.0, 50.0]];
const DIAGONAL: &[[f64; 2]] = &[[20.0, 20.0], [50.0, 35.0], [80.0, 50.0], [110.0, 65.0]];
const AREA: &[[f64; 2]] = &[[20.0, 20.0], [80.0, 20.0], [80.0, 80.0], [20.0, 80.0]];

/// The ids of the open "No pixels are selected." alerts. The message is the only way to tell this alert from others.
fn alerts(app: &PhotocraftApp) -> Vec<craft::ui::dialog::DialogId> {
    app.framework
        .dialogs
        .open_dialogs()
        .iter()
        .filter(|d| d.fields.as_ref().and_then(|f| f.get("message")).and_then(Value::as_str) == Some("No pixels are selected."))
        .map(|d| d.id)
        .collect()
}

/// Whether the document has a selection.
fn selected(app: &PhotocraftApp) -> bool {
    app.session.active().unwrap().doc.selection.is_some()
}

/// The number of History states.
fn steps(app: &PhotocraftApp) -> usize {
    app.session.active().unwrap().history.past_len()
}

/// What a drag does.
#[derive(Debug, PartialEq)]
enum Outcome {
    /// One alert, nothing selected, no History state.
    Warns,
    /// No alert, a selection, one History state.
    Selects,
    /// No alert (a selection existed before).
    Silent,
}

#[test]
fn a_lasso_that_selects_no_pixel_warns_and_changes_nothing() {
    // (name, anti-alias, a selection that exists before, points, outcome)
    let rows: [(&str, bool, bool, &[[f64; 2]], Outcome); 9] = [
        ("line", true, false, LINE, Outcome::Warns),
        ("diagonal", true, false, DIAGONAL, Outcome::Warns),
        ("click", true, false, &[[40.0, 40.0]], Outcome::Warns),
        ("two points", true, false, &[[40.0, 40.0], [90.0, 40.0]], Outcome::Warns),
        // Under half a pixel of area, but anti-aliased it covers part of pixel 10,10.
        ("tiny lasso, anti-alias", true, false, &[[10.0, 10.0], [11.0, 10.0], [10.0, 10.75]], Outcome::Selects),
        ("pixel centres, no anti-alias", false, false, &[[10.0, 20.0], [13.0, 20.0], [13.0, 23.0], [10.0, 23.0]], Outcome::Selects),
        // 21 px² of area, but between the centres of row 20 (20.5) and the row above: no pixel.
        ("thin lasso between centres, no anti-alias", false, false, &[[10.0, 20.1], [150.0, 20.1], [150.0, 20.4]], Outcome::Warns),
        ("area", true, false, AREA, Outcome::Selects),
        ("line outside an existing selection", true, true, LINE, Outcome::Silent),
    ];
    for (name, anti_alias, existing, points, outcome) in rows {
        let mut app = app();
        app.ui.tool_options.anti_alias = anti_alias;
        if existing {
            app.run("select.rect", json!({"x": 150, "y": 10, "width": 30, "height": 30})).unwrap();
        }
        let before = steps(&app);
        drag(&mut app, points);
        match outcome {
            Outcome::Warns => {
                assert_eq!(alerts(&app).len(), 1, "{name}: the alert");
                assert!(!selected(&app), "{name}: nothing selected");
                assert_eq!(steps(&app), before, "{name}: no History state");
            }
            Outcome::Selects => {
                assert!(alerts(&app).is_empty(), "{name}: no alert");
                assert!(selected(&app), "{name}: selected");
                assert_eq!(steps(&app), before + 1, "{name}: one History state");
            }
            Outcome::Silent => assert!(alerts(&app).is_empty(), "{name}: not measured with a selection: no alert"),
        }
    }
}

/// An app whose preferences live in `files`, and the path of its settings file.
fn app_over(files: &Arc<craft::settings::testing::MemoryFiles>) -> (PhotocraftApp, PathBuf) {
    let options = craft::settings::OpenOptions::new(craft::settings::Location::Dir("/preferences".into())).files(files.clone());
    let (store, report) = craft::settings::SettingsStore::open_with(options);
    let services = crate::Services { settings: Some((store, report)), ..Default::default() };
    let path = services.settings.as_ref().and_then(|(store, _)| store.path()).expect("a settings file").to_path_buf();
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
    app.run("file.new", json!({"width": 200, "height": 100, "background": "white"})).unwrap();
    app.ui.tool = Tool::ContentAwareMove;
    (app, path)
}
