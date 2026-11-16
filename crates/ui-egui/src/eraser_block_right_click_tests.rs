//! Right-click routing on the canvas with the Eraser: the mode menu, the Brush Preset picker or an erase, by the
//! Eraser's mode, Shift and the right-click preference.

use egui::Modifiers;
use serde_json::json;

use crate::PhotocraftApp;
use crate::canvas::testing;
use crate::canvas_tool_menu::MenuKind;
use crate::state::Tool;

<<<<<<< HEAD
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Menu,
    Picker,
    Erase,
}

fn app(mode: &str, erase_preference: bool) -> PhotocraftApp {
=======
pub(super) fn app(mode: &str) -> PhotocraftApp {
>>>>>>> upstream/main
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    // The erase stroke commits in the click's own pass, so its journal entry is there when the click returns.
    app.background_jobs = false;
    app.run("file.new", json!({"width": 64, "height": 64, "background": "white"})).unwrap();
    app.run("layer.new.layer", json!({})).unwrap();
    app.run("edit.fill", json!({"color": "#dc1e1e"})).unwrap();
    app.ui.tool = Tool::Eraser;
    app.ui.tool_options.eraser_mode = mode.into();
    if erase_preference {
        app.run("prefs.set", json!({"path": "tools.rightClickWithPaintingTools", "value": "erase"})).unwrap();
    }
    app.sync_views();
    app
}

<<<<<<< HEAD
/// A secondary click at the document centre, with Shift held when asked; what the click led to.
fn right_click(mode: &str, shift: bool, erase_preference: bool) -> Outcome {
    let mut app = app(mode, erase_preference);
    let journal = app.session.journal.len();
    let mods = if shift { Modifiers::SHIFT } else { Modifiers::NONE };
    testing::secondary_click(&mut app, [32.0, 32.0], mods);
    if app.ui.canvas_tool_menu.as_ref().is_some_and(|m| matches!(m.kind, MenuKind::BrushMode)) {
        Outcome::Menu
    } else if crate::brush_picker::opened_at(&app).is_some() {
        Outcome::Picker
    } else {
        // An erase is a `paint.stroke` command whose parameters carry `erase: true`.
        let erased = app.session.journal[journal..].iter().any(|(id, p)| id == "paint.stroke" && p["erase"] == json!(true));
        assert!(erased, "a right-click that opened nothing erases");
        Outcome::Erase
=======
pub(super) fn call(app: &mut PhotocraftApp, ctx: &egui::Context, method: &str, params: Value) -> Value {
    let (req, _rx) = ControlRequest::new(method, params);
    match handle(app, ctx, &req) {
        Outcome::Done(v) => v,
        _ => panic!("{method}: expected an immediate reply"),
    }
}

pub(super) fn right_click(app: &mut PhotocraftApp, ctx: &egui::Context, modifiers: Value) {
    call(
        app,
        ctx,
        "ui.pointer",
        json!({"button": "right", "modifiers": modifiers, "events": [{"kind": "down", "x": 20, "y": 20}, {"kind": "up", "x": 20, "y": 20}]}),
    );
}

pub(super) fn steps(app: &PhotocraftApp) -> usize {
    app.session.active().unwrap().history.past_len()
}

#[test]
fn block_mode_right_click_opens_the_mode_menu() {
    let ctx = egui::Context::default();
    let mut app = app("block");
    let n = steps(&app);
    right_click(&mut app, &ctx, json!({}));
    assert!(app.ui.brush_mode_menu.is_some(), "the Brush · Pencil · Block menu");
    assert!(app.ui.brush_picker.is_none(), "not the preset picker");
    let inspect = call(&mut app, &ctx, "ui.inspect", json!({}));
    assert_eq!(inspect["result"]["brushModeMenu"]["choices"], json!(["Brush", "Pencil", "Block"]), "{inspect}");
    // Picking Pencil sets the mode; nothing is erased and no history is added.
    assert_eq!(call(&mut app, &ctx, "ui.context.choose", json!({"id": "Pencil"}))["ok"], true);
    assert_eq!(app.ui.tool_options.eraser_mode, "pencil");
    assert_eq!(steps(&app), n);
    assert!(app.ui.brush_mode_menu.is_none());
}

#[test]
fn brush_and_pencil_mode_keep_the_picker() {
    let ctx = egui::Context::default();
    for mode in ["brush", "pencil"] {
        let mut app = app(mode);
        right_click(&mut app, &ctx, json!({}));
        assert!(app.ui.brush_picker.is_some(), "{mode}: the Brush Preset picker");
        assert!(app.ui.brush_mode_menu.is_none(), "{mode}");
>>>>>>> upstream/main
    }
}

#[test]
fn eraser_right_click_routes_by_mode_shift_and_preference() {
    use Outcome::{Erase, Menu, Picker};
    let table = [
        ("block", false, false, Menu),
        ("brush", false, false, Picker),
        ("pencil", false, false, Picker),
        ("brush", true, false, Menu),
        ("pencil", true, false, Menu),
        ("block", true, false, Menu),
        ("block", false, true, Erase),
    ];
    for (mode, shift, erase_preference, want) in table {
        assert_eq!(right_click(mode, shift, erase_preference), want, "{mode}, shift {shift}, erase preference {erase_preference}");
    }
}
<<<<<<< HEAD
=======

#[test]
fn with_the_erase_preference_block_mode_still_erases() {
    let ctx = egui::Context::default();
    let mut app = app("block");
    app.run("prefs.set", json!({"path": "tools.rightClickWithPaintingTools", "value": "erase"})).unwrap();
    call(&mut app, &ctx, "ui.pointer", json!({"button": "right", "events": [{"kind": "down", "x": 20, "y": 20}]}));
    assert!(app.ui.brush_mode_menu.is_none(), "the preference's erase wins");
    assert!(app.secondary_erase || app.drag.is_some(), "a right-erase started");
}

/// The real mouse: a right-click on the canvas in Block mode opens the menu; Escape closes it and
/// nothing is erased.
#[test]
fn a_real_right_click_opens_the_menu_and_escape_closes_it() {
    let mut h = harness(app("block"));
    let n = steps(h.state());
    let c = h.state().last_canvas_rect.center();
    h.event(egui::Event::PointerMoved(c));
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE });
        h.run_steps(2);
    }
    assert!(h.state().ui.brush_mode_menu.is_some(), "the menu at the pointer");
    assert!(h.state().ui.brush_picker.is_none());
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(h.state().ui.brush_mode_menu.is_none(), "Escape closes it");
    assert_eq!(steps(h.state()), n, "nothing erased");
}

/// The canvas alone, drawn by the real mouse path.
pub(super) fn harness(app: PhotocraftApp) -> Harness<'static, PhotocraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            let ctx = ui.ctx().clone();
            if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
        },
        app,
    );
    crate::PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h.run_steps(4);
    h
}
>>>>>>> upstream/main
