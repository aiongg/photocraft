use egui::{Pos2, pos2};

use super::*;

/// The rulers over a canvas, as the app lays them out: the rulers first, then the guides they feed.
fn draw_rulers(app: &mut PhotocraftApp, ui: &mut egui::Ui, full: Rect, xf: &ViewXform) {
    let Some(doc) = app.session.active().map(|d| d.doc.clone()) else { return };
    let response = ui.interact(xf.rect, egui::Id::new("test-canvas"), egui::Sense::click_and_drag());
    show_rulers(app, ui, &response, full, xf, &doc, 0, Some(guides_id(ui, 0)));
    let ctx = ui.ctx().clone();
    let mods = ui.input(|i| i.modifiers);
    show_guides(app, ui, &ctx, &response, xf, &doc, 0, false, mods);
}

#[test]
fn ruler_strip_matches_the_library_ruler_size() {
    let ctx = egui::Context::default();
    assert_eq!(Rulers::style(&craft::ui::theme::current(&ctx)).size, RULER);
}

/// Rulers alone in a 400×300 frame at 100 %, centred on a 256×128 document.
fn ruler_harness(app: PhotocraftApp) -> (egui_kittest::Harness<'static, PhotocraftApp>, Rect, ViewXform) {
    let full = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));
    let rect = Rect::from_min_max(full.min + vec2(RULER, RULER), full.max);
    let xf = ViewXform { rect, zoom: 1.0, center: [128.0, 64.0], flip: false, rotation: 0.0, aspect: 1.0 };
    let h = egui_kittest::Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(move |ui, app| draw_rulers(app, ui, full, &xf), app);
    (h, full, xf)
}

fn press(h: &mut egui_kittest::Harness<'static, PhotocraftApp>, pos: Pos2, pressed: bool, modifiers: egui::Modifiers) {
    h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers });
    h.run_steps(2);
}

fn drag_corner_to(h: &mut egui_kittest::Harness<'static, PhotocraftApp>, full: Rect, to: Pos2, modifiers: egui::Modifiers) {
    let corner = Rect::from_min_size(full.min, vec2(RULER, RULER)).center();
    h.event(egui::Event::ModifiersChanged(modifiers));
    h.event(egui::Event::PointerMoved(corner));
    h.run_steps(1);
    press(h, corner, true, modifiers);
    for p in [corner + vec2(10.0, 10.0), to] {
        h.event(egui::Event::PointerMoved(p));
        h.run_steps(2);
    }
    press(h, to, false, modifiers);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run_steps(1);
}

fn origin(h: &egui_kittest::Harness<'static, PhotocraftApp>) -> [f64; 2] {
    h.state().session.active().unwrap().doc.ruler_origin
}

/// #2955: dragging out of the ruler corner sets the zero point (cross hairs while dragging),
/// the rulers then read 0 there, Shift snaps it to a tick, and double-clicking the corner
/// resets it.
#[test]
fn dragging_the_ruler_corner_sets_the_zero_point_and_double_click_resets_it() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 256, "height": 128})).unwrap();
    app.ui.extras.snap = false;
    let (mut h, full, xf) = ruler_harness(app);
    h.run_steps(2);
    let target = xf.to_screen(100.0, 40.0);
    drag_corner_to(&mut h, full, target, egui::Modifiers::NONE);
    let o = origin(&h);
    assert!((o[0] - 100.0).abs() < 0.01 && (o[1] - 40.0).abs() < 0.01, "zero point lands under the pointer: {o:?}");
    // The top ruler's "0" now sits at the new zero point, "100" (a distance, unsigned as in
    // Photoshop) at the image's left edge.
    h.run_steps(2);
    let labels: Vec<(String, f32)> = h
        .output()
        .shapes
        .iter()
        .filter_map(|s| if let egui::Shape::Text(t) = &s.shape { Some((t.galley.text().to_string(), t.pos.x)) } else { None })
        .collect();
    let near = |text: &str, x: f32| labels.iter().any(|(t, lx)| t == text && (lx - (x + 2.0)).abs() < 1.0);
    assert!(near("0", target.x), "a 0 label at the zero point: {labels:?}");
    assert!(near("100", xf.to_screen(0.0, 0.0).x), "100 at the image's left edge: {labels:?}");

    // Mid-drag, cross hairs run through the pointer across the canvas.
    let corner = Rect::from_min_size(full.min, vec2(RULER, RULER)).center();
    let mid = xf.to_screen(150.0, 90.0);
    press(&mut h, corner, true, egui::Modifiers::NONE);
    for p in [corner + vec2(10.0, 10.0), mid] {
        h.event(egui::Event::PointerMoved(p));
        h.run_steps(2);
    }
    let crosshair = h.output().shapes.iter().any(|s| {
        matches!(&s.shape, egui::Shape::Mesh(m) if { let b = m.calc_bounds(); (b.center().x - mid.x).abs() < 1.0 && b.width() <= 2.0 && b.height() > 200.0 })
    });
    assert!(crosshair, "a vertical cross-hair line through the pointer while dragging");
    assert_eq!(origin(&h), o, "the zero point moves on release only");
    press(&mut h, mid, false, egui::Modifiers::NONE);
    assert_eq!(origin(&h), [150.0, 90.0]);
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(origin(&h), o, "setting the zero point is undoable");

    // A drag dropped back on the corner leaves it alone.
    drag_corner_to(&mut h, full, full.min + vec2(4.0, 4.0), egui::Modifiers::NONE);
    assert_eq!(origin(&h), o);

    // Shift: ticks are 10 px apart at 100 %, measured from the current zero point (100, 40).
    drag_corner_to(&mut h, full, xf.to_screen(123.0, 57.0), egui::Modifiers::SHIFT);
    assert_eq!(origin(&h), [120.0, 60.0]);

    for _ in 0..40 {
        h.step();
    }
    let corner = Rect::from_min_size(full.min, vec2(RULER, RULER)).center();
    h.event(egui::Event::PointerMoved(corner));
    h.step();
    for _ in 0..2 {
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos: corner, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            h.step();
        }
    }
    h.run_steps(2);
    assert_eq!(origin(&h), [0.0, 0.0], "double-clicking the corner resets the zero point");
}

/// #2955: the Info panel's X/Y are measured from the rulers' zero point.
#[test]
fn info_panel_position_reads_from_the_zero_point() {
    use egui_kittest::kittest::Queryable;
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 200, "height": 100})).unwrap();
    app.run("view.setRulerOrigin", json!({"x": 20, "y": 5})).unwrap();
    app.hover_doc = Some([50.5, 20.5]);
    let mut h = egui_kittest::Harness::builder()
        .with_size(vec2(260.0, 240.0))
        .build_ui_state(|ui, app| crate::panels::dock_body(app, ui, crate::dock::Group::Navigator, 2), app);
    h.run_steps(2);
    assert!(h.query_by_label("30").is_some() && h.query_by_label("15").is_some(), "X 30, Y 15 from the zero point at (20, 5)");
    assert!(h.query_by_label("50").is_none(), "not the raw image x");
}

/// #5728: dragging a guide shows its coordinate in the Info panel on one axis only.
#[test]
fn info_panel_shows_guide_drag_position_outside_canvas() {
    use egui_kittest::kittest::Queryable;
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 200, "height": 100})).unwrap();
    app.hover_doc = None;
    app.guide_drag = Some(GuideDrag { vertical: true, index: None, pos: 42.0 });
    let mut h = egui_kittest::Harness::builder()
        .with_size(vec2(260.0, 240.0))
        .build_ui_state(|ui, app| crate::panels::dock_body(app, ui, crate::dock::Group::Navigator, 2), app);
    h.run_steps(2);
    assert!(h.query_by_label("42").is_some(), "vertical guide: X readout");
    h.state_mut().guide_drag = Some(GuideDrag { vertical: false, index: None, pos: 17.0 });
    h.run_steps(2);
    assert!(h.query_by_label("17").is_some(), "horizontal guide: Y readout");
}

/// A dragged guide shows its position beside the pointer: X for a vertical guide, Y for a
/// horizontal one, from the zero point, in the ruler unit.
#[test]
fn guide_drag_shows_its_position_from_the_zero_point_in_the_ruler_unit() {
    use egui_kittest::kittest::Queryable;
    let mut h = canvas_harness(false);
    h.state_mut().run("view.setRulerOrigin", json!({"x": 3.0, "y": 10.0})).unwrap();
    h.run_steps(2);
    let press = canvas_at(&h, 200.0, 100.0);
    h.event(egui::Event::PointerMoved(press));
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: press, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    h.event(egui::Event::PointerMoved(canvas_at(&h, 200.0, 140.0)));
    h.run_steps(2);
    assert!(h.query_by_label_contains("Y:").is_some(), "the readout shows beside the pointer");
    assert!(h.query_by_label_contains("130").is_some(), "from the zero point: 140 - 10");
    h.state_mut().run("prefs.set", json!({"path": "unitsAndRulers.rulers", "value": "percent"})).unwrap();
    h.event(egui::Event::PointerMoved(canvas_at(&h, 200.0, 150.0)));
    h.run_steps(2);
    assert!(h.query_all_by_label_contains("%").next().is_some(), "in the ruler unit");
}

type AppHarness = egui_kittest::Harness<'static, PhotocraftApp>;

/// The real canvas on a 400 × 300 document with a horizontal guide at y = 100, at 100 %,
/// with the Move tool and snapping off.
fn canvas_harness(rulers: bool) -> AppHarness {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
    app.run("view.newGuide", json!({"orientation": "horizontal", "position": 100})).unwrap();
    app.sync_views();
    app.ui.extras.rulers = rulers;
    app.ui.extras.snap = false;
    app.ui.tool = crate::state::Tool::Move;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(800.0, 600.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            if !ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h.run_steps(4);
    let v = &mut h.state_mut().ui.views[0];
    (v.zoom, v.center, v.fit_pending) = (1.0, [200.0, 150.0], false);
    h.run_steps(2);
    h
}

/// Screen point of document point `(x, y)` in [`canvas_harness`].
fn canvas_at(h: &AppHarness, x: f32, y: f32) -> Pos2 {
    let app = h.state();
    let v = &app.ui.views[0];
    let rect = content_rect(app, app.last_canvas_rect);
    ViewXform { rect, zoom: v.zoom, center: v.center, flip: false, rotation: 0.0, aspect: 1.0 }.to_screen(x, y)
}

/// Moving a horizontal guide with the Move tool on the real canvas: `Y:` follows the pointer.
#[test]
fn moving_a_guide_on_the_canvas_shows_its_y() {
    use egui_kittest::kittest::Queryable;
    let mut h = canvas_harness(false);
    let at = canvas_at;
    let press = at(&h, 200.0, 100.0);
    h.event(egui::Event::PointerMoved(press));
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: press, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    for y in [110.0, 125.0, 140.0] {
        h.event(egui::Event::PointerMoved(at(&h, 210.0, y)));
        h.run_steps(2);
    }
    assert!(h.state().guide_drag.is_some_and(|d| !d.vertical), "a horizontal guide is being dragged");
    assert!(h.query_by_label("Y: 140 px").is_some(), "Y readout at the guide");
    assert!(h.query_by_label_contains("X:").is_none());
    let up = at(&h, 210.0, 140.0);
    h.event(egui::Event::PointerButton { pos: up, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(2);
    assert_eq!(h.state().session.active().unwrap().doc.guides.horizontal, vec![140.0]);
    assert!(h.query_by_label_contains("Y:").is_none(), "gone after the drop");
}

/// A guide dragged out of the top ruler: in each frame the readout shows where the guide is
/// after that frame's pointer move (the ruler handles the drag after the canvas is drawn), and
/// the frame that drops it shows none.
#[test]
fn ruler_drag_readout_is_current_every_frame() {
    use egui_kittest::kittest::Queryable;
    let mut h = canvas_harness(true);
    let from = pos2(canvas_at(&h, 200.0, 0.0).x, h.state().last_canvas_rect.top() + RULER / 2.0);
    h.event(egui::Event::PointerMoved(from));
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    for y in [40.0, 60.0, 75.0] {
        h.event(egui::Event::PointerMoved(canvas_at(&h, 200.0, y)));
        // egui 0.37 lays an Area's contents out a pass after it is shown; step to the settled
        // frame with the pointer still at `y` (readout included) rather than the first pass.
        h.run_steps(2);
        let pos = h.state().guide_drag.map(|d| d.pos);
        assert_eq!(pos, Some(f64::from(y)), "the drag follows the pointer");
        assert!(h.query_by_label(&format!("Y: {y} px")).is_some(), "readout at {y} in the same frame");
    }
    let up = canvas_at(&h, 200.0, 75.0);
    h.event(egui::Event::PointerButton { pos: up, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(2);
    assert!(h.state().guide_drag.is_none());
    assert!(h.query_by_label_contains("Y:").is_none(), "no readout in the frame that drops the guide");
    assert!(h.state().session.active().unwrap().doc.guides.horizontal.contains(&75.0), "the guide lands at y = 75");
}

#[test]
fn percentage_grid_spacing_uses_both_canvas_dimensions() {
    let mut g = GuidesGridAndSlices { grid_unit: Unit::Percent, gridline_every: 20.0, subdivisions: 2, ..Default::default() };
    assert_eq!(grid_major_px(&g, 72.0, [1280.0, 720.0], 72.0), [256.0, 144.0]);
    // Absolute units remain equal on both axes, irrespective of aspect ratio.
    g.grid_unit = Unit::Pixels;
    g.gridline_every = 100.0;
    assert_eq!(grid_major_px(&g, 72.0, [1280.0, 720.0], 72.0), [100.0, 100.0]);
}

#[test]
fn hidden_or_locked_guides_take_neither_pointer_nor_keys() {
    let mut h = canvas_harness(false);
    let drag = |h: &mut AppHarness| {
        let press = canvas_at(h, 200.0, 100.0);
        h.event(egui::Event::PointerMoved(press));
        h.run_steps(1);
        h.event(egui::Event::PointerButton { pos: press, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
        h.event(egui::Event::PointerMoved(canvas_at(h, 200.0, 130.0)));
        h.run_steps(2);
        let took = h.state().guide_drag.is_some();
        h.event(egui::Event::PointerButton {
            pos: canvas_at(h, 200.0, 130.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(2);
        took
    };
    let hide: [(&str, fn(&mut PhotocraftApp, bool)); 4] = [
        ("Show Guides off", |a, on| a.ui.extras.guides = on),
        ("View › Show › Guides off", |a, on| a.ui.view.show.canvas_guides = on),
        ("View › Extras off", |a, on| a.ui.view.extras = on),
        ("Lock Guides", |a, on| a.ui.extras.lock_guides = !on),
    ];
    for (name, set) in hide {
        set(h.state_mut(), false);
        h.run_steps(2);
        assert!(!drag(&mut h), "{name}: the pointer reaches the layer below");
        set(h.state_mut(), true);
        h.run_steps(2);
        let positions = h.state().session.active().unwrap().doc.guides.horizontal.clone();
        assert_eq!(positions, vec![100.0], "{name}: hiding changes no guide");
    }
    assert!(drag(&mut h), "shown and unlocked: the Move tool takes the guide");
    assert_eq!(h.state().session.active().unwrap().doc.guides.horizontal, vec![130.0]);
}

#[test]
fn unit_menu_lists_every_unit_with_the_current_one_checked() {
    let items = unit_menu_items(Unit::Inches, &UNITS);
    assert_eq!(items.len(), Unit::NAMES.len());
    // Every unit is offered, labelled, and in the preference's order.
    assert_eq!(items[0], ("Pixels".to_string(), "pixels", false));
    assert_eq!(items.iter().find(|(_, name, _)| *name == "cm").unwrap().0, "Centimeters");
    // Exactly the current unit is checked.
    let checked: Vec<_> = items.iter().filter(|(_, _, c)| *c).map(|(_, name, _)| *name).collect();
    assert_eq!(checked, vec!["inches"]);
}

#[test]
fn picking_a_ruler_unit_changes_the_preference() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    assert_eq!(app.session.prefs().units_and_rulers.rulers, Unit::Pixels);
    // The context menu applies its pick exactly as the preference pane does.
    app.run("prefs.set", json!({"path": "unitsAndRulers.rulers", "value": "cm"})).unwrap();
    assert_eq!(app.session.prefs().units_and_rulers.rulers, Unit::Centimeters);
}

/// View › Rulers is remembered: the next launch, and every document created or opened in it,
/// shows the rulers as last set, on or off (#2918).
#[test]
fn rulers_open_as_last_set_after_a_restart() {
    use std::{cell::RefCell, rc::Rc};
    let saved = Rc::new(RefCell::new(None::<String>));
    let launch = |saved: &Rc<RefCell<Option<String>>>| {
        let (load, store) = (saved.clone(), saved.clone());
        PhotocraftApp::new(
            photocraft_engine::Session::new(),
            crate::Services {
                load_prefs: Some(Box::new(move || load.borrow().clone())),
                save_prefs: Some(Box::new(move |text| {
                    *store.borrow_mut() = Some(text.to_string());
                    Ok(())
                })),
                ..Default::default()
            },
        )
    };
    for on in [true, false] {
        let mut app = launch(&saved);
        assert_eq!(crate::menus::invoke(&mut app, &egui::Context::default(), "view.rulers", json!({})).unwrap(), json!(on));
        crate::prefs_ui::save_preferences(&mut app).unwrap();
        let mut next = launch(&saved);
        next.session.execute("file.new", json!({"width": 20, "height": 10})).unwrap();
        next.sync_views();
        assert_eq!(next.ui.extras.rulers, on, "a new document after a restart shows the rulers as last set");
    }
}

/// Screen point of document point `(x, y)` in a harness whose view is at 100 %.
fn at(h: &AppHarness, x: f32, y: f32) -> Pos2 {
    canvas_at(h, x, y)
}

/// Drag the primary button through `path` (screen points), holding `modifiers`.
fn drag_path(h: &mut AppHarness, path: &[Pos2], modifiers: egui::Modifiers) {
    let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers };
    h.event_modifiers(egui::Event::PointerMoved(path[0]), modifiers);
    h.run_steps(1);
    h.event_modifiers(button(path[0], true), modifiers);
    h.run_steps(1);
    for &p in &path[1..] {
        h.event_modifiers(egui::Event::PointerMoved(p), modifiers);
        h.run_steps(2);
    }
    let last = *path.last().unwrap();
    h.event_modifiers(button(last, false), modifiers);
    h.run_steps(2);
}

fn guides_of(h: &AppHarness) -> (Vec<f64>, Vec<f64>) {
    let g = &h.state().session.active().unwrap().doc.guides;
    (g.vertical.clone(), g.horizontal.clone())
}

/// One history step per drag: a guide is made out of a ruler, moved with the Move tool, deleted
/// off the document, and locked guides stay put.
#[test]
fn guide_drags_create_move_and_delete() {
    let mut h = tool_canvas_harness(crate::state::Tool::Move);
    let canvas = h.state().last_canvas_rect;
    let steps = |h: &AppHarness| h.state().session.active().unwrap().history.past_len();
    let left = canvas.min.x + RULER / 2.0;
    let before = steps(&h);
    let (a, b) = (pos2(left, at(&h, 0.0, 100.0).y), at(&h, 50.0, 100.0));
    drag_path(&mut h, &[a, b], egui::Modifiers::NONE);
    assert_eq!(guides_of(&h), (vec![50.0], vec![]));
    assert_eq!(steps(&h), before + 1, "one history step");
    // Off the document (x = 500 > 400): no guide.
    let (a, b) = (pos2(left, at(&h, 0.0, 100.0).y), at(&h, 500.0, 100.0));
    drag_path(&mut h, &[a, b], egui::Modifiers::NONE);
    assert_eq!(guides_of(&h), (vec![50.0], vec![]), "a drag let go off the document makes no guide");
    // Move it with the Move tool.
    let before = steps(&h);
    let (a, b, c) = (at(&h, 50.0, 100.0), at(&h, 80.0, 100.0), at(&h, 120.0, 100.0));
    drag_path(&mut h, &[a, b, c], egui::Modifiers::NONE);
    assert_eq!(guides_of(&h), (vec![120.0], vec![]));
    assert_eq!(steps(&h), before + 1, "one history step for the whole drag");
    // Let go off the document: deleted.
    let (a, b) = (at(&h, 120.0, 100.0), at(&h, 450.0, 100.0));
    drag_path(&mut h, &[a, b], egui::Modifiers::NONE);
    assert_eq!(guides_of(&h), (vec![], vec![]));
    // Locked guides don't move.
    h.state_mut().run("view.newGuide", json!({"orientation": "vertical", "position": 60})).unwrap();
    h.state_mut().ui.extras.lock_guides = true;
    h.run_steps(2);
    let (a, b) = (at(&h, 60.0, 100.0), at(&h, 90.0, 100.0));
    drag_path(&mut h, &[a, b], egui::Modifiers::NONE);
    assert_eq!(guides_of(&h), (vec![60.0], vec![]));
}

/// ⌘ is the Move tool with any tool, so a ⌘-drag over a guide moves the guide, never the layer
/// (#2690); with Lock Guides a ⌘-drag with a marquee moves the layer as before.
#[test]
fn command_drag_over_a_guide_moves_the_guide_with_any_tool() {
    use crate::state::Tool;
    for (tool, locked) in [
        (Tool::Brush, false),
        (Tool::Eraser, false),
        (Tool::Gradient, false),
        (Tool::Frame, false),
        (Tool::Artboard, false),
        (Tool::RectMarquee, false),
        (Tool::Lasso, false),
        (Tool::MagicWand, false),
        (Tool::RectMarquee, true),
    ] {
        let mut h = tool_canvas_harness_with(tool, json!({"width": 400, "height": 300, "background": "transparent"}));
        h.state_mut().ui.extras.lock_guides = locked;
        let layer = h.state().session.active().unwrap().active_layer.unwrap();
        h.state_mut()
            .session
            .edit("paint", |doc, _| {
                doc.layer_mut(layer).unwrap().surface_mut().unwrap().fill_rect(photocraft_geom::Rect::new(10, 10, 30, 30), &[1.0, 0.0, 0.0, 1.0]);
                Ok(())
            })
            .unwrap();
        h.state_mut().run("view.newGuide", json!({"orientation": "vertical", "position": 50})).unwrap();
        h.run_steps(2);
        let v = &mut h.state_mut().ui.views[0];
        (v.zoom, v.center, v.fit_pending) = (1.0, [200.0, 150.0], false);
        h.run_steps(2);
        let (a, b) = (at(&h, 50.0, 40.0), at(&h, 65.0, 40.0));
        let mid = at(&h, 57.0, 40.0);
        drag_path(&mut h, &[a, mid, b], egui::Modifiers::COMMAND);
        let doc = &h.state().session.active().unwrap().doc;
        let moved = doc.layer(layer).unwrap().surface().unwrap().rgba(12, 12)[3] == 0.0;
        if locked {
            assert_eq!((doc.guides.vertical.clone(), moved), (vec![50.0], true), "{tool:?}, guides locked: the layer moves");
        } else {
            assert_eq!((doc.guides.vertical.clone(), moved), (vec![65.0], false), "{tool:?}: the guide moves, the layer stays");
        }
    }
}

/// #5698: as in Photoshop, an Alt-drag out of the top ruler makes a vertical guide and one out
/// of the left ruler a horizontal guide; without Alt each ruler makes its own kind.
#[test]
fn alt_drags_a_guide_of_the_other_orientation_out_of_a_ruler() {
    let full = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0));
    let rect = Rect::from_min_max(full.min + vec2(RULER, RULER), full.max);
    let xf = ViewXform { rect, zoom: 1.0, center: [128.0, 64.0], flip: false, rotation: 0.0, aspect: 1.0 };
    let drag = |from: Pos2, modifiers: egui::Modifiers| {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 256, "height": 128})).unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(400.0, 300.0)).build_ui_state(move |ui, app| draw_rulers(app, ui, full, &xf), app);
        let to = xf.to_screen(37.0, 90.0);
        h.event_modifiers(egui::Event::PointerMoved(from), modifiers);
        h.event_modifiers(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers }, modifiers);
        h.run_steps(1);
        h.event_modifiers(egui::Event::PointerMoved(to), modifiers);
        h.run_steps(1);
        h.event_modifiers(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers }, modifiers);
        h.run_steps(2);
        let g = &h.state().session.active().unwrap().doc.guides;
        (g.vertical.clone(), g.horizontal.clone())
    };
    let (top, left) = (pos2(200.0, RULER / 2.0), pos2(RULER / 2.0, 150.0));
    assert_eq!(drag(top, egui::Modifiers::NONE), (vec![], vec![90.0]), "top ruler: horizontal");
    assert_eq!(drag(left, egui::Modifiers::NONE), (vec![37.0], vec![]), "left ruler: vertical");
    assert_eq!(drag(top, egui::Modifiers::ALT), (vec![37.0], vec![]), "Alt from the top ruler: vertical");
    assert_eq!(drag(left, egui::Modifiers::ALT), (vec![], vec![90.0]), "Alt from the left ruler: horizontal");
}

/// #6286: a guide let go over a ruler is removed, whichever ruler it is. The top ruler measures
/// document x and the left one y, so a horizontal guide released on the left ruler is still
/// inside the document on its own axis, and used to be placed instead of discarded.
#[test]
fn a_guide_dropped_on_either_ruler_is_removed() {
    let new_app = || {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 256, "height": 128})).unwrap();
        app.ui.extras.rulers = true;
        // No snapping, so a guide that is kept lands exactly where it is let go.
        app.ui.extras.snap = false;
        app
    };
    let drag = |from: Pos2, to: Pos2| {
        let (mut h, _, _) = ruler_harness(new_app());
        h.event(egui::Event::PointerMoved(from));
        press(&mut h, from, true, egui::Modifiers::NONE);
        h.event(egui::Event::PointerMoved(to));
        h.run_steps(1);
        press(&mut h, to, false, egui::Modifiers::NONE);
        let g = &h.state().session.active().unwrap().doc.guides;
        (g.vertical.clone(), g.horizontal.clone())
    };
    // #5698: out of the top ruler is a horizontal guide, out of the left one a vertical guide.
    let (top, left) = (pos2(200.0, RULER / 2.0), pos2(RULER / 2.0, 150.0));
    assert_eq!(drag(top, left), (vec![], vec![]), "out of the top ruler, released on the left ruler");
    assert_eq!(drag(left, top), (vec![], vec![]), "out of the left ruler, released on the top ruler");
    // Released on the canvas the guide lands where it was let go, as before.
    assert_eq!(drag(top, pos2(200.0, 150.0)), (vec![], vec![56.0]), "released on the canvas");
    assert_eq!(drag(left, pos2(120.0, 150.0)), (vec![40.0], vec![]), "released on the canvas");
}

/// #6286: the guide is removed when it is let go over a ruler, and moved when it is not. The
/// release can be inside the document on the guide's own axis and still be a ruler drop, so the
/// two are told apart by where the pointer is, not by the position it reads.
#[test]
fn a_guide_let_go_on_a_ruler_is_removed_not_moved() {
    let mut h = tool_canvas_harness(crate::state::Tool::Move);
    let canvas = h.state().last_canvas_rect;
    h.state_mut().run("view.newGuide", json!({"orientation": "horizontal", "position": 150})).unwrap();
    h.run_steps(2);
    let on = |h: &AppHarness| at(h, 200.0, 150.0);
    // Dragged sideways onto the left ruler, a horizontal guide holds its y, which on its own
    // reads as a click: the ruler still removes it. So does the top ruler.
    for ruler in [pos2(canvas.min.x + RULER / 2.0, on(&h).y), pos2(on(&h).x, canvas.min.y + RULER / 2.0)] {
        let p = on(&h);
        let side = pos2(p.x - 12.0, p.y);
        drag_path(&mut h, &[p, side, ruler], egui::Modifiers::NONE);
        assert_eq!(guides_of(&h).1, Vec::<f64>::new(), "let go on a ruler: removed");
        h.state_mut().run("view.newGuide", json!({"orientation": "horizontal", "position": 150})).unwrap();
        h.run_steps(2);
    }
    let p = on(&h);
    let (q, r) = (at(&h, 200.0, 170.0), at(&h, 200.0, 180.0));
    drag_path(&mut h, &[p, q, r], egui::Modifiers::NONE);
    assert_eq!(guides_of(&h).1, vec![180.0], "let go on the canvas: moved");
    // Clicked where it was pressed, on the canvas: selected and kept.
    let p = at(&h, 200.0, 180.0);
    drag_path(&mut h, &[p], egui::Modifiers::NONE);
    assert_eq!(guides_of(&h).1, vec![180.0], "clicked on the canvas: kept");
    assert!(h.state().ui.selected_guide.is_some(), "clicked on the canvas: selected");
}

/// The canvas the guides live on, rulers on, at 100 %.
fn tool_canvas_harness(tool: crate::state::Tool) -> egui_kittest::Harness<'static, PhotocraftApp> {
    tool_canvas_harness_with(tool, json!({"width": 400, "height": 300}))
}

/// [`tool_canvas_harness`] on the document `file.new` makes from `new`.
fn tool_canvas_harness_with(tool: crate::state::Tool, new: serde_json::Value) -> egui_kittest::Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", new).unwrap();
    app.sync_views();
    app.ui.extras.rulers = true;
    app.ui.extras.snap = false;
    app.ui.tool = tool;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(1000.0, 700.0)).build_ui_state(
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
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h.run_steps(4);
    let v = &mut h.state_mut().ui.views[0];
    v.zoom = 1.0;
    v.center = [200.0, 150.0];
    v.fit_pending = false;
    h.run_steps(2);
    h
}

/// #6286: an existing guide dragged onto a ruler with the Move tool is deleted, though the
/// release is inside the document on the guide's own axis: the left ruler measures y, so a
/// horizontal guide let go there still reads a y inside the canvas and used to be moved. Held
/// level, that release reads as a click on the guide, which a ruler drop also outranks.
#[test]
fn a_guide_dragged_onto_a_ruler_is_deleted_through_the_canvas() {
    for below in [50.0f32, 0.0] {
        let mut h = tool_canvas_harness(crate::state::Tool::Move);
        h.state_mut().run("view.newGuide", json!({"orientation": "horizontal", "position": 150})).unwrap();
        h.run_steps(1);
        let at = |h: &egui_kittest::Harness<'static, PhotocraftApp>, x: f32, y: f32| {
            let app = h.state();
            let v = &app.ui.views[0];
            let xf = ViewXform {
                rect: content_rect(app, app.last_canvas_rect),
                zoom: v.zoom,
                center: v.center,
                flip: app.ui.view.flip_horizontal,
                rotation: v.rotation,
                aspect: app.ui.view.display_aspect(),
            };
            xf.to_screen(x, y)
        };
        let on_the_guide = at(&h, 200.0, 150.0);
        // In the left ruler's band, `below` document px under the press: inside the canvas on the
        // guide's own axis, which is where the old code moved the guide instead of removing it.
        let canvas = h.state().last_canvas_rect;
        let onto_the_left_ruler = pos2(canvas.min.x + RULER / 2.0, on_the_guide.y + below);
        // Along the guide, so a held-level release keeps its y, the way a sideways drag does.
        let dragged = pos2(on_the_guide.x - 12.0, on_the_guide.y);
        h.event(egui::Event::PointerMoved(on_the_guide));
        h.run_steps(1);
        h.event(egui::Event::PointerButton { pos: on_the_guide, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
        // Along the guide, which its axis does not count as a move.
        h.event(egui::Event::PointerMoved(dragged));
        h.run_steps(2);
        assert!(h.state().guide_drag.is_none(), "along the guide it is still a click");
        h.event(egui::Event::PointerMoved(onto_the_left_ruler));
        h.run_steps(1);
        h.event(egui::Event::PointerButton {
            pos: onto_the_left_ruler,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(2);
        let g = &h.state().session.active().unwrap().doc.guides;
        assert!(g.horizontal.is_empty(), "{below} px below the press, released on the left ruler: got {:?}", g.horizontal);
        assert!(h.state().guide_drag.is_none(), "the drag is over");
    }
}

/// #3770: a guide Shift-dragged out of a ruler lands on the nearest tick the ruler draws
/// (tenths of the labelled step, in the ruler unit); without Shift it lands where it's dropped.
#[test]
fn shift_dragged_guides_land_on_ruler_ticks() {
    let full = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0));
    let rect = Rect::from_min_max(full.min + vec2(RULER, RULER), full.max);
    let xf = ViewXform { rect, zoom: 1.0, center: [128.0, 64.0], flip: false, rotation: 0.0, aspect: 1.0 };
    let new_app = || {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 256, "height": 128})).unwrap();
        app
    };
    let drag_guide = |modifiers: egui::Modifiers| {
        let builder = egui_kittest::Harness::builder().with_size(vec2(400.0, 300.0));
        let mut h = builder.build_ui_state(move |ui, app| draw_rulers(app, ui, full, &xf), new_app());
        // Out of the left ruler: a vertical guide, dropped at document x = 37.
        let (from, to) = (pos2(RULER / 2.0, 100.0), xf.to_screen(37.0, 64.0));
        h.event_modifiers(egui::Event::PointerMoved(from), modifiers);
        h.event_modifiers(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers }, modifiers);
        h.run_steps(1);
        h.event_modifiers(egui::Event::PointerMoved(to), modifiers);
        h.run_steps(1);
        h.event_modifiers(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers }, modifiers);
        h.run_steps(2);
        h.state().session.active().unwrap().doc.guides.vertical.clone()
    };
    assert_eq!(drag_guide(egui::Modifiers::NONE), vec![37.0]);
    // #5698: Alt turns it into a horizontal guide (at the drop's y, the document's centre).
    assert_eq!(drag_guide(egui::Modifiers::ALT), Vec::<f64>::new(), "no vertical guide with Alt");
    // Pixels at 100%: labels every 100 px, ticks every 10 px.
    assert_eq!(drag_guide(egui::Modifiers::SHIFT), vec![40.0]);

    // Percent of each side: ticks every 2.5% of 256 px across (6.4 px).
    let mut percent = new_app();
    percent.run("prefs.set", json!({"path": "unitsAndRulers.rulers", "value": "percent"})).unwrap();
    let mut h = egui_kittest::Harness::builder().with_size(vec2(400.0, 300.0)).build_ui_state(move |ui, app| draw_rulers(app, ui, full, &xf), percent);
    let (from, to, shift) = (pos2(RULER / 2.0, 100.0), xf.to_screen(37.0, 64.0), egui::Modifiers::SHIFT);
    h.event_modifiers(egui::Event::PointerMoved(from), shift);
    h.event_modifiers(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: shift }, shift);
    h.run_steps(1);
    h.event_modifiers(egui::Event::PointerMoved(to), shift);
    h.run_steps(1);
    h.event_modifiers(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: shift }, shift);
    h.run_steps(2);
    let guides = h.state().session.active().unwrap().doc.guides.vertical.clone();
    assert!(guides.len() == 1 && (guides[0] - 38.4).abs() < 1e-9, "{guides:?}");
}

/// The guide readout prints the ruler unit and at most one decimal for pixels: `X: 103.2 px`.
#[test]
fn ruler_units_format_the_readout_with_the_unit_and_one_decimal() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", serde_json::json!({"width": 200, "height": 100})).unwrap();
    let doc = &app.session.active().unwrap().doc;
    let [x, _] = ruler_units_with(&app, LocaleNumbers::from_tag("en"), doc);
    let decimals = 0..=x.decimals.max(1);
    assert_eq!(x.format(103.2, 0.0, decimals.clone()), "103.2 px");
    assert_eq!(x.format(103.0, 0.0, decimals), "103 px");
}
