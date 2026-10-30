use super::*;

fn frame(app: &mut PhotocraftApp, ctx: &egui::Context, time: f64, events: Vec<egui::Event>) {
    let mut out = ctx.run_ui(
        egui::RawInput { time: Some(time), events, screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1200.0, 1800.0))), ..Default::default() },
        |ui| toolbar(app, ui),
    );
    out.textures_delta.clear();
}

/// #4444: click and hold on Change Screen Mode opens its list like the tool flyouts, without
/// cycling the mode on release; a plain click still cycles, and right-click still opens it.
#[test]
fn long_press_screen_mode_opens_the_modes_without_cycling() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    let ctx = egui::Context::default();
    PhotocraftApp::setup_context(&ctx, crate::theme::ThemeKind::ALL[0]);
    frame(&mut app, &ctx, 0.0, vec![]);
    frame(&mut app, &ctx, 0.1, vec![]);
    let bx = if Tokens::get(&ctx).pro { 30.0 } else { 36.0 };
    // The toolbar's last button: Change Screen Mode, after Quick Mask.
    let at = ctx
        .viewport(|v| {
            v.prev_pass
                .widgets
                .layers()
                .flat_map(|(_, w)| w.iter())
                .filter(|w| w.rect.size() == egui::Vec2::splat(bx) && w.sense.senses_click())
                .map(|w| w.rect)
                .max_by(|a, b| a.top().total_cmp(&b.top()).then(a.left().total_cmp(&b.left())))
        })
        .unwrap()
        .center();
    let standard = |app: &PhotocraftApp| crate::view_cmds::checked(app, "view.screenMode.standard") == Some(true);
    let menu_open = |ctx: &egui::Context| egui::Popup::is_any_open(ctx);
    let pointer = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
    assert!(standard(&app));
    // Hold, then release: the list opens and stays open, and the mode is unchanged.
    frame(&mut app, &ctx, 1.0, vec![egui::Event::PointerMoved(at), pointer(at, true)]);
    frame(&mut app, &ctx, 1.2, vec![]);
    assert!(!menu_open(&ctx), "not before the hold time");
    frame(&mut app, &ctx, 1.4, vec![]);
    assert!(menu_open(&ctx), "open while still held");
    frame(&mut app, &ctx, 1.5, vec![pointer(at, false)]);
    frame(&mut app, &ctx, 1.55, vec![]);
    assert!(menu_open(&ctx), "the release keeps it open");
    assert!(standard(&app), "the release doesn't cycle the mode");
    // Escape closes it; a plain click cycles the mode and opens nothing.
    frame(
        &mut app,
        &ctx,
        2.0,
        vec![egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }],
    );
    frame(&mut app, &ctx, 2.05, vec![]);
    assert!(!menu_open(&ctx));
    frame(&mut app, &ctx, 3.0, vec![egui::Event::PointerMoved(at), pointer(at, true)]);
    frame(&mut app, &ctx, 3.1, vec![pointer(at, false)]);
    frame(&mut app, &ctx, 3.15, vec![]);
    assert!(!standard(&app), "a click cycles");
    assert!(!menu_open(&ctx));
    // Right-click opens it too.
    let secondary = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Secondary, pressed, modifiers: Default::default() };
    frame(&mut app, &ctx, 4.0, vec![secondary(true)]);
    frame(&mut app, &ctx, 4.05, vec![secondary(false)]);
    frame(&mut app, &ctx, 4.1, vec![]);
    assert!(menu_open(&ctx));
}

#[test]
fn red_eye_is_in_the_j_flyout() {
    let j = TOOL_SECTIONS.iter().flat_map(|section| section.iter()).find(|slot| slot.contains(&Tool::SpotHealing)).expect("J group");
    assert!(j.contains(&Tool::RedEye), "{j:?}");
    assert_eq!(j.last(), Some(&Tool::RedEye));
    // Remove leads the flyout, so the slot shows it until another J tool is picked.
    assert_eq!(j.first(), Some(&Tool::Remove));
}

#[test]
fn art_history_is_in_the_y_flyout() {
    let y = TOOL_SECTIONS.iter().flat_map(|section| section.iter()).find(|slot| slot.contains(&Tool::HistoryBrush)).expect("Y group");
    assert_eq!(*y, [Tool::HistoryBrush, Tool::ArtHistoryBrush]);
}

#[test]
fn rotate_view_is_in_the_hand_flyout() {
    let hand = TOOL_SECTIONS.iter().flat_map(|section| section.iter()).find(|slot| slot.contains(&Tool::Hand)).expect("Hand group");
    assert_eq!(*hand, [Tool::Hand, Tool::RotateView]);
    assert_eq!(Tool::RotateView.key(), 'R');
}

/// Holding a slot's button selects the tool it shows and opens its flyout, whose rows pick the
/// slot's other tools (#4453). The gestures are craft-ui's; this checks PhotoCraft's slots.
#[test]
fn a_held_slot_selects_its_shown_tool_and_its_flyout_picks_another() {
    use egui_kittest::kittest::Queryable;
    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(400.0, 1800.0))
        .build_ui_state(|ui, app: &mut PhotocraftApp| toolbar(app, ui), PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default()));
    PhotocraftApp::setup_context(&harness.ctx, crate::theme::ThemeKind::ALL[0]);
    harness.run_steps(3);
    for (shown, other) in [
        (Tool::Type, Tool::VerticalType),
        (Tool::CloneStamp, Tool::PatternStamp),
        (Tool::Brush, Tool::ColorReplacement),
        (Tool::HistoryBrush, Tool::ArtHistoryBrush),
    ] {
        let at = harness.get_by_label(tl!(shown.label())).rect().center();
        let primary = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        harness.event(egui::Event::PointerMoved(at));
        harness.event(primary(true));
        harness.run_steps(3);
        harness.event(primary(false));
        harness.run_steps(2);
        assert_eq!(harness.state().ui.tool, shown, "holding {shown:?} selects it");
        harness.get_by_label(tl!(other.label())).click();
        harness.run_steps(3);
        assert_eq!(harness.state().ui.tool, other, "{shown:?}'s flyout picks {other:?}");
    }
}
