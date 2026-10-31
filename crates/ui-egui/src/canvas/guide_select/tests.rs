use egui::{Key, Modifiers, PointerButton, Pos2, vec2};

type AppHarness = Harness<'static, PhotocraftApp>;
use egui_kittest::Harness;
use serde_json::json;

use super::*;
use crate::canvas_tool_menu::{GUIDE_CLEAR, GUIDE_EDIT};
use crate::state::Tool;

/// A 200 × 100 document with the Move tool, a vertical guide at 50 and a magenta horizontal one
/// at 30.
fn app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 200, "height": 100})).unwrap();
    app.sync_views();
    app.ui.tool = Tool::Move;
    app.run("view.newGuide", json!({"orientation": "vertical", "position": 50})).unwrap();
    app.run("view.newGuide", json!({"orientation": "horizontal", "position": 30, "color": "magenta"})).unwrap();
    app
}

/// The canvas of `app` at 100 %, document centred.
fn canvas(app: PhotocraftApp) -> AppHarness {
    let mut h = harness(app);
    let v = &mut h.state_mut().ui.views[0];
    (v.zoom, v.center, v.fit_pending) = (1.0, [100.0, 50.0], false);
    h.run_steps(2);
    h
}

/// Screen point of document point `(x, y)` in [`canvas`].
fn at(h: &AppHarness, x: f32, y: f32) -> Pos2 {
    let app = h.state();
    let v = &app.ui.views[0];
    crate::canvas::ViewXform { rect: app.last_canvas_rect, zoom: v.zoom, center: v.center, flip: false, rotation: 0.0, aspect: 1.0 }.to_screen(x, y)
}

/// Press at the first of `path` (screen points), move through the rest and let go at the last.
fn drag_path(h: &mut AppHarness, path: &[Pos2], modifiers: Modifiers) {
    let button = |pos, pressed| egui::Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers };
    h.event_modifiers(egui::Event::PointerMoved(path[0]), modifiers);
    h.run_steps(1);
    h.event_modifiers(button(path[0], true), modifiers);
    h.run_steps(1);
    for &p in &path[1..] {
        h.event_modifiers(egui::Event::PointerMoved(p), modifiers);
        h.run_steps(2);
    }
    h.event_modifiers(button(*path.last().unwrap(), false), modifiers);
    h.run_steps(2);
}

fn click(h: &mut AppHarness, x: f32, y: f32) {
    let p = at(h, x, y);
    drag_path(h, &[p], Modifiers::NONE);
}

fn guides(app: &PhotocraftApp) -> (Vec<f64>, Vec<f64>) {
    let g = &app.session.active().unwrap().doc.guides;
    (g.vertical.clone(), g.horizontal.clone())
}

fn past(app: &PhotocraftApp) -> usize {
    app.session.active().unwrap().history.past_len()
}

#[test]
fn a_click_selects_a_guide_without_moving_it_and_a_press_elsewhere_deselects() {
    let mut h = canvas(app());
    let steps = past(h.state());
    click(&mut h, 50.0, 70.0);
    assert_eq!(selected(h.state()), Some((true, 0)));
    assert_eq!((guides(h.state()), past(h.state())), ((vec![50.0], vec![30.0]), steps), "a click is not a move");
    // One guide at a time: clicking another selects it instead.
    click(&mut h, 120.0, 30.0);
    assert_eq!(selected(h.state()), Some((false, 0)));
    click(&mut h, 120.0, 70.0);
    assert_eq!(selected(h.state()), None);
    // A press with another tool deselects too.
    click(&mut h, 50.0, 70.0);
    h.state_mut().ui.tool = Tool::RectMarquee;
    h.run_steps(2);
    click(&mut h, 150.0, 80.0);
    assert_eq!(selected(h.state()), None);
}

#[test]
fn a_moved_guide_stays_selected_and_dragging_one_off_the_canvas_deletes_it() {
    let mut h = canvas(app());
    let (a, b, c) = (at(&h, 50.0, 70.0), at(&h, 65.0, 70.0), at(&h, 80.0, 70.0));
    drag_path(&mut h, &[a, b, c], Modifiers::NONE);
    assert_eq!(guides(h.state()).0, vec![80.0]);
    assert_eq!(selected(h.state()), Some((true, 0)));
    let (a, b, c) = (at(&h, 80.0, 70.0), at(&h, 40.0, 70.0), at(&h, -20.0, 70.0));
    drag_path(&mut h, &[a, b, c], Modifiers::NONE);
    assert!(guides(h.state()).0.is_empty());
    assert_eq!(selected(h.state()), None);
}

#[test]
fn the_selection_lapses_when_its_guide_goes_or_guides_are_hidden_or_locked() {
    let mut h = canvas(app());
    click(&mut h, 50.0, 70.0);
    let app = h.state_mut();
    app.ui.extras.lock_guides = true;
    assert_eq!(selected(app), None, "locked guides can't be selected");
    app.ui.extras.lock_guides = false;
    app.ui.extras.guides = false;
    assert_eq!(selected(app), None, "nor hidden ones");
    app.ui.extras.guides = true;
    assert_eq!(selected(app), Some((true, 0)));
    app.run("view.clearGuides", json!({})).unwrap();
    assert_eq!(selected(app), None);
    app.session.undo();
    assert_eq!(selected(app), Some((true, 0)), "undo brings the guide back, still selected");
    // Another document's guide at the same place isn't the selected one.
    app.run("file.new", json!({"width": 200, "height": 100})).unwrap();
    app.run("view.newGuide", json!({"orientation": "vertical", "position": 50})).unwrap();
    assert_eq!(selected(app), None);
}

#[test]
fn delete_removes_the_selected_guide_and_leaves_the_pixels() {
    let mut app = app();
    app.run("select.all", json!({})).unwrap();
    let mut h = canvas(app);
    click(&mut h, 120.0, 30.0);
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert_eq!(guides(h.state()), (vec![50.0], vec![]));
    assert_eq!(selected(h.state()), Some((true, 0)), "the guides keep one selected for the keys: the next");
    click(&mut h, 50.0, 70.0);
    h.key_press(Key::Backspace);
    h.run_steps(2);
    assert_eq!(guides(h.state()), (vec![], vec![]));
    assert_eq!(selected(h.state()), None);
    let st = h.state().session.active().unwrap();
    assert_eq!(st.history.undo_label(), Some("Delete Guide"), "Edit › Clear didn't run");
    assert!(st.doc.selection.is_some());
    // One undoable step per guide.
    h.state_mut().session.undo();
    assert_eq!(guides(h.state()), (vec![50.0], vec![]));
}

fn harness(app: PhotocraftApp) -> Harness<'static, PhotocraftApp> {
    let mut h = Harness::builder().with_size(vec2(1000.0, 700.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            let ctx = ui.ctx().clone();
            if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            crate::shortcuts::handle(app, &ctx);
            egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::default());
    h.run_steps(4);
    h
}

fn right_click(h: &mut Harness<'static, PhotocraftApp>, p: Pos2) {
    h.event(egui::Event::PointerMoved(p));
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: p, button: PointerButton::Secondary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: p, button: PointerButton::Secondary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

/// The vertical guide through the middle of the document, on screen: the view centres the image.
fn guide_on_screen(h: &Harness<'static, PhotocraftApp>) -> Pos2 {
    h.state().last_canvas_rect.center() + vec2(0.0, 20.0)
}

fn centred_guide_app() -> PhotocraftApp {
    let mut app = app();
    app.run("view.clearGuides", json!({})).unwrap();
    app.run("view.newGuide", json!({"orientation": "vertical", "position": 100})).unwrap();
    app
}

#[test]
fn right_click_on_a_guide_selects_it_and_opens_photoshops_guide_menu() {
    let mut h = harness(centred_guide_app());
    let at = guide_on_screen(&h);
    right_click(&mut h, at);
    let menu = h.state().ui.canvas_tool_menu.clone().expect("the guide menu");
    assert!(menu.guide);
    assert_eq!(selected(h.state()), Some((true, 0)));
    let labels: Vec<Option<&str>> = crate::canvas_tool_menu::rows(&menu).iter().map(|r| r.map(|(label, _)| label)).collect();
    assert_eq!(
        labels,
        [
            Some("Edit Selected Guides"),
            None,
            Some("Lock Guides"),
            Some("Clear Guides"),
            Some("Clear Canvas Guides"),
            Some("Clear Selected Guides"),
            Some("Clear Selected Artboard Guides"),
        ]
    );
    let app = h.state();
    for id in [GUIDE_EDIT, GUIDE_CLEAR, "view.lockGuides", "view.clearGuides", "view.clearCanvasGuides"] {
        assert!(crate::canvas_tool_menu::available(app, &menu, id), "{id}");
    }
    assert!(!crate::canvas_tool_menu::available(app, &menu, "view.clearSelectedArtboardGuides"), "no artboard");
    assert!(!crate::canvas_tool_menu::available(app, &menu, "edit.freeTransform"), "not a guide row");
    // Off a guide, the Move tool's right-click is still the layer picker's.
    h.key_press(Key::Escape);
    h.run_steps(2);
    let off = h.state().last_canvas_rect.center() + vec2(-60.0, 20.0);
    right_click(&mut h, off);
    assert!(h.state().ui.canvas_tool_menu.is_none());
}

#[test]
fn guide_menu_rows_clear_the_selected_guide_and_lock_guides() {
    let mut h = harness(centred_guide_app());
    h.state_mut().run("view.newGuide", json!({"orientation": "vertical", "position": 20})).unwrap();
    let at = guide_on_screen(&h);
    right_click(&mut h, at);
    let ctx = h.ctx.clone();
    crate::canvas_tool_menu::choose(h.state_mut(), &ctx, GUIDE_CLEAR);
    assert_eq!(guides(h.state()).0, vec![20.0], "only the right-clicked guide goes");
    h.state_mut().run("view.newGuide", json!({"orientation": "vertical", "position": 100})).unwrap();
    h.run_steps(2);
    let at = guide_on_screen(&h);
    right_click(&mut h, at);
    crate::canvas_tool_menu::choose(h.state_mut(), &ctx, "view.lockGuides");
    assert!(h.state().ui.extras.lock_guides);
    assert_eq!(selected(h.state()), None, "locked guides aren't selected");
}

#[test]
fn edit_selected_guides_opens_new_guide_on_the_guide_and_applies_the_edit() {
    let mut h = canvas(app());
    h.state_mut().run("prefs.set", json!({"path": "unitsAndRulers.rulers", "value": "percent"})).unwrap();
    click(&mut h, 120.0, 30.0);
    let app = h.state_mut();
    let id = crate::new_guide_ui::open_edit(app).unwrap();
    let f = app.ui.dialogs.iter().find(|d| d.id == id).unwrap().fields.clone();
    assert_eq!((f["orientation"].as_str(), f["position"].as_str(), f["color"].as_str()), (Some("horizontal"), Some("30 %"), Some("magenta")));
    assert_eq!(f["__label"], "Edit Selected Guides");
    // Turn it vertical at 30 % of the width; the colour isn't touched, so it keeps its own.
    let mut edited = f.clone();
    edited.insert("orientation".into(), json!("vertical"));
    edited.insert("position".into(), json!("30"));
    app.ui.dialogs.retain(|d| d.id != id);
    crate::new_guide_ui::confirm(app, &edited).unwrap();
    assert_eq!(guides(app), (vec![50.0, 60.0], vec![]));
    let g = &app.session.active().unwrap().doc.guides;
    assert_eq!((g.color(true, 60.0), g.color(false, 30.0)), (Some([0xff, 0x00, 0xff]), None));
    assert_eq!(selected(&app), Some((true, 1)), "the edited guide stays selected");
}

#[test]
fn editing_an_uncoloured_guide_keeps_the_preference_colour_unless_one_is_chosen() {
    let mut h = canvas(app());
    click(&mut h, 50.0, 70.0);
    let app = h.state_mut();
    let id = crate::new_guide_ui::open_edit(app).unwrap();
    let f = app.ui.dialogs.iter().find(|d| d.id == id).unwrap().fields.clone();
    // The default preference colour is Cyan's, so a guide dragged from a ruler shows as Cyan.
    assert_eq!(f["color"].as_str(), Some("cyan"));
    let params = crate::new_guide_ui::params(&app, &f);
    assert!(params.get("color").is_none(), "{params}");
    let mut yellow = f.clone();
    yellow.insert("color".into(), json!("yellow"));
    assert_eq!(crate::new_guide_ui::params(&app, &yellow)["color"], "yellow");
    // A refused position keeps the dialog open with the message and the guide untouched.
    let mut bad = f.clone();
    bad.insert("position".into(), json!("far"));
    app.ui.dialogs.retain(|d| d.id != id);
    assert!(crate::new_guide_ui::confirm(app, &bad).is_err());
    assert_eq!(guides(&app).0, vec![50.0]);
    assert!(app.ui.dialogs.last().is_some_and(|d| d.fields.contains_key("__error")));
    // A preference colour that isn't a named one shows as Custom….
    app.ui.dialogs.clear();
    app.run("prefs.set", json!({"path": "guidesGridAndSlices.guideColor", "value": "#123456"})).unwrap();
    let id = crate::new_guide_ui::open_edit(app).unwrap();
    let f = &app.ui.dialogs.iter().find(|d| d.id == id).unwrap().fields;
    assert_eq!((f["color"].as_str(), f["customColor"].as_str()), (Some("custom"), Some("#123456")));
}

#[test]
fn edit_selected_guides_needs_a_selected_guide() {
    let mut app = app();
    assert!(crate::new_guide_ui::open_edit(&mut app).is_err());
    assert!(app.ui.dialogs.is_empty());
}

/// ⌥-click turns a guide through the point clicked, keeping its colour, as one undoable step, and
/// leaves it selected; a locked guide can't be turned.
#[test]
fn alt_click_turns_a_guide_through_the_point_clicked() {
    let mut h = canvas(app());
    let alt = Modifiers::ALT;
    let p = at(&h, 120.0, 30.0);
    drag_path(&mut h, &[p], alt);
    assert_eq!(guides(h.state()), (vec![50.0, 120.0], vec![]), "the magenta horizontal guide is now vertical at x = 120");
    let g = &h.state().session.active().unwrap().doc.guides;
    assert_eq!(g.color(true, 120.0), Some([0xff, 0x00, 0xff]));
    assert_eq!(selected(h.state()), Some((true, 1)));
    assert_eq!(h.state().session.active().unwrap().history.undo_label(), Some("Edit Guide"));
    h.state_mut().session.undo();
    assert_eq!(guides(h.state()), (vec![50.0], vec![30.0]));
    h.state_mut().ui.extras.lock_guides = true;
    h.run_steps(2);
    let p = at(&h, 50.0, 70.0);
    drag_path(&mut h, &[p], alt);
    assert_eq!(guides(h.state()), (vec![50.0], vec![30.0]), "locked guides stay put");
}

/// Double-clicking a guide opens Preferences › Guides, Grid & Slices, as in Photoshop.
#[test]
fn double_click_on_a_guide_opens_the_guide_preferences() {
    let mut h = harness(centred_guide_app());
    let at = guide_on_screen(&h);
    h.hover_at(at);
    for _ in 0..40 {
        h.step();
    }
    for _ in 0..2 {
        h.event(egui::Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        h.step();
        h.event(egui::Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        h.step();
    }
    h.run_steps(3);
    let dialog = h.state().ui.dialogs.last().map(|d| serde_json::to_string(&d.fields).unwrap_or_default()).unwrap_or_default();
    assert!(dialog.contains("guidesGridAndSlices"), "{dialog}");
    assert_eq!(guides(h.state()).0, vec![100.0], "the guide didn't move");
}

#[test]
fn delete_from_anywhere_removes_the_selected_guide_and_clears_the_selection() {
    let mut app = app();
    app.run("select.all", json!({})).unwrap();
    let mut h = canvas(app);
    click(&mut h, 120.0, 30.0);
    // The pointer leaves the canvas and the guides lose keyboard focus; the global Delete route
    // still reaches the selected guide.
    h.event(egui::Event::PointerMoved(Pos2::ZERO));
    h.ctx.memory_mut(|m| m.surrender_focus(m.focused().unwrap_or_else(|| egui::Id::new("none"))));
    h.run_steps(1);
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert_eq!(guides(h.state()), (vec![50.0], vec![]));
    assert_eq!(selected(h.state()), None, "a request from outside the guides clears the selection");
    let st = h.state().session.active().unwrap();
    assert_eq!(st.history.undo_label(), Some("Delete Guide"), "Edit › Clear didn't run");
    assert!(st.doc.selection.is_some());
}

#[test]
fn delete_in_a_text_field_leaves_the_selected_guide() {
    let mut text = String::from("abc");
    let mut h = Harness::builder().with_size(vec2(1000.0, 700.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            let ctx = ui.ctx().clone();
            if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            crate::shortcuts::handle(app, &ctx);
            egui::Panel::top("field").show(ui, |ui| {
                ui.add(egui::TextEdit::singleline(&mut text).id(egui::Id::new("guide-test-field")));
            });
            egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
        },
        app(),
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::default());
    h.run_steps(4);
    let v = &mut h.state_mut().ui.views[0];
    (v.zoom, v.center, v.fit_pending) = (1.0, [100.0, 50.0], false);
    h.run_steps(2);
    click(&mut h, 120.0, 30.0);
    assert_eq!(selected(h.state()), Some((false, 0)));
    h.ctx.memory_mut(|m| m.request_focus(egui::Id::new("guide-test-field")));
    h.run_steps(2);
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert_eq!(guides(h.state()), (vec![50.0], vec![30.0]), "the text field keeps Delete");
    assert_eq!(selected(h.state()), Some((false, 0)));
}

#[test]
fn arrows_move_the_layer_when_the_guides_do_not_have_keyboard_focus() {
    let mut app = app();
    app.run("layer.new.layer", json!({})).unwrap();
    app.run("select.rect", json!({"x": 40, "y": 40, "width": 20, "height": 20})).unwrap();
    app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
    app.run("select.deselect", json!({})).unwrap();
    let mut h = canvas(app);
    let x0 = |h: &AppHarness| {
        let st = h.state().session.active().unwrap();
        st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap().content_bounds().x0
    };
    let start = x0(&h);
    h.key_press(Key::ArrowRight);
    h.run_steps(2);
    assert_eq!(x0(&h), start + 1, "the Move tool's nudge moves the layer");
    assert_eq!(guides(h.state()), (vec![50.0], vec![30.0]), "and the guides stay");
}
