use super::*;
use egui::{Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;

use super::tests::{any_dialog, close_dialogs, open_here};

fn app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 32, "height": 32})).unwrap();
    app.ui.tool = Tool::ContentAwareMove;
    app
}

fn harness(mut app: PhotocraftApp) -> Harness<'static, PhotocraftApp> {
    app.sync_views();
    let mut h = Harness::builder().with_size(vec2(1000.0, 700.0)).with_context(app.ctx().clone()).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            let ctx = ui.ctx().clone();
            if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::AppTheme::default());
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

/// Draws a crop frame with the Crop tool as a user does: press, moves and release on the canvas.
fn crop_drag(h: &mut Harness<'static, PhotocraftApp>) {
    let center = h.state().last_canvas_rect.center();
    let (from, to) = (center - vec2(40.0, 40.0), center + vec2(40.0, 40.0));
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
    h.event(egui::Event::PointerMoved(from));
    h.run_steps(1);
    h.event(button(from, true));
    h.run_steps(1);
    for i in 1..=4 {
        h.event(egui::Event::PointerMoved(from + (to - from) * (i as f32 / 4.0)));
        h.run_steps(1);
    }
    h.event(button(to, false));
    h.run_steps(2);
}

#[test]
fn content_aware_move_inside_selection_opens_color_range_and_load_selection_only() {
    let mut app = app();
    app.run("select.rect", json!({"x": 4, "y": 4, "width": 24, "height": 24})).unwrap();
    let mut h = harness(app);
    let center = h.state().last_canvas_rect.center();
    right_click(&mut h, center);

    let menu = h.state().ui.canvas_tool_menu.clone().expect("inside-selection menu opened");
    assert!(menu.has_selection && inside_selection(&menu) && menu.sampler.is_none() && menu.sampler_at.is_none());
    assert_eq!(rows(&menu), &[Some(("Color Range…", "select.colorRange")), Some(("Load Selection…", "select.loadSelection"))]);

    // Color Range… is enabled; Load Selection… is greyed out when no selection channel is saved.
    assert!(entry_enabled(h.state(), &menu, "select.colorRange"));
    assert!(!entry_enabled(h.state(), &menu, "select.loadSelection"));
    assert!(!available(h.state(), &menu, "select.deselect"), "full selection menu rows are not available inside selection");

    // A spot channel does not count as a saved selection channel.
    let ctx = h.ctx.clone();
    h.state_mut().run("channel.newSpot", json!({"name": "Spot 1"})).unwrap();
    assert!(!entry_enabled(h.state(), &menu, "select.loadSelection"));

    // Once a selection channel is saved, Load Selection… becomes enabled and renders cleanly with no samplers.
    h.state_mut().run("select.saveSelection", json!({"name": "Alpha 1"})).unwrap();
    assert!(open_here(h.state_mut(), Tool::ContentAwareMove, [16.0, 16.0]));
    h.run_steps(1);
    let menu = h.state().ui.canvas_tool_menu.clone().unwrap();
    assert!(entry_enabled(h.state(), &menu, "select.loadSelection"));

    // Even with a saved channel, Load Selection… greys out when menus::is_enabled blocks it (e.g. a pending crop).
    h.state_mut().ui.tool = Tool::Crop;
    h.run_steps(2);
    crop_drag(&mut h);
    assert!(crate::crop_ui::pending(h.state()), "the drag draws a pending crop");
    assert!(!entry_enabled(h.state(), &menu, "select.loadSelection"));
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(!crate::crop_ui::pending(h.state()), "Esc drops the pending crop");
    h.state_mut().ui.tool = Tool::ContentAwareMove;
    assert!(entry_enabled(h.state(), &menu, "select.loadSelection"));

    // Choosing Color Range… opens the Color Range dialog.
    choose(h.state_mut(), &ctx, "select.colorRange");
    assert!(any_dialog(h.state()));
    close_dialogs(h.state_mut());

    // Choosing Load Selection… opens the Load Selection dialog.
    assert!(open_here(h.state_mut(), Tool::ContentAwareMove, [16.0, 16.0]));
    choose(h.state_mut(), &ctx, "select.loadSelection");
    assert!(any_dialog(h.state()));
}

#[test]
fn content_aware_move_boundary_modes_and_overlay_guards() {
    let mut empty = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    empty.ui.tool = Tool::ContentAwareMove;
    crate::color_hud::open(&mut empty, [12.0, 12.0], false);
    assert!(!open_here(&mut empty, Tool::ContentAwareMove, [8.0, 8.0]));
    assert!(empty.ui.color_hud.is_none());

    let mut app = app();
    app.run("select.rect", json!({"x": 4, "y": 4, "width": 8, "height": 8})).unwrap();
    // Boundary, negative, and non-finite coordinates.
    for (pt, expected) in [([4.0, 4.0], true), ([11.9, 11.9], true), ([3.9, 4.0], false), ([12.0, 12.0], false), ([-1.0, 4.0], false)] {
        assert!(open_here(&mut app, Tool::ContentAwareMove, pt));
        let m = app.ui.canvas_tool_menu.clone().unwrap();
        assert_eq!(inside_selection(&m), expected, "point {pt:?}");
    }
    assert!(!open_here(&mut app, Tool::ContentAwareMove, [f64::NAN, 4.0]), "a point that is not finite opens no menu");

    // Excluded color modes return false and release a non-held color HUD.
    for mode in [ColorMode::Bitmap, ColorMode::Duotone, ColorMode::Multichannel] {
        std::sync::Arc::make_mut(&mut app.session.active_mut().unwrap().doc).mode = mode;
        crate::color_hud::open(&mut app, [12.0, 12.0], false);
        assert!(!open_here(&mut app, Tool::ContentAwareMove, [6.0, 6.0]));
        assert!(app.ui.color_hud.is_none());
    }
}
