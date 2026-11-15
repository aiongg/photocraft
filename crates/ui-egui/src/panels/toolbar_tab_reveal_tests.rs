use super::*;
use egui_kittest::kittest::Queryable;

const HEIGHT: f32 = 360.0;

/// A harness that draws the toolbar in a window much shorter than its tools, so the rail scrolls.
fn short_toolbar_harness() -> egui_kittest::Harness<'static, PhotocraftApp> {
    let mut ready = false;
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    // The supported shortcut preference clears the canvas Tab binding, so Tab is the focus key.
    app.session.prefs.edit(|prefs| prefs.shortcuts.insert("window.togglePanels".into(), String::new()));
    let mut harness = egui_kittest::Harness::builder().with_context(app.ctx().clone()).with_size(vec2(400.0, HEIGHT)).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            if !ready {
                egui_extras::install_image_loaders(ui.ctx());
                crate::theme::install_fonts(ui.ctx());
                crate::theme::apply(ui.ctx(), &crate::theme::AppTheme::pro());
                ready = true;
                return;
            }
            crate::shortcuts::handle(app, ui.ctx());
            toolbar(app, ui);
        },
        app,
    );
    harness.run_steps(4);
    harness
}

/// From no focus, Tab enters the rail's buttons (one roving group) and the arrow keys walk them: the last tool is
/// reached in a window shorter than the rail, it is scrolled into view, and Space picks it.
#[test]
fn tab_reveals_and_activates_the_last_tool() {
    let mut harness = short_toolbar_harness();
    assert_eq!(harness.state().ui.tool, Tool::Move);
    let mut entered = false;
    for _ in 0..40 {
        harness.key_press(egui::Key::Tab);
        harness.run_steps(3);
        entered = harness.get_by_label_contains("Move Tool").is_focused();
        if entered {
            break;
        }
    }
    assert!(entered, "Tab must reach the rail's buttons");
    // The node's rect is its visible part, so a button the rail clipped is shorter than a whole one.
    let full_height = harness.get_by_label_contains("Move Tool").rect().height();
    // The grid wraps, so Up from the first slot goes backwards from the end of the rail.
    let mut reached = false;
    for _ in 0..10 {
        harness.key_press(egui::Key::ArrowUp);
        harness.run_steps(3);
        reached = harness.get_by_label_contains("Zoom Tool").is_focused();
        if reached {
            break;
        }
    }
    assert!(reached, "the arrow keys must reach the last tool");
    harness.run_steps(20);
    let rect = harness.get_by_label_contains("Zoom Tool").rect();
    assert!(rect.height() >= full_height - 0.5, "the focused tool is clipped: {rect:?} against a full height of {full_height}");
    assert!(rect.top() >= 0.0 && rect.bottom() <= HEIGHT, "the focused tool is outside the window: {rect:?}");
    harness.key_press(egui::Key::Space);
    harness.run_steps(3);
    assert_eq!(harness.state().ui.tool, Tool::Zoom, "Space must select the revealed tool");
}
