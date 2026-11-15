use serde_json::json;

use crate::PhotocraftApp;
use crate::canvas::{ToolEvent, tool_event};
use crate::state::Tool;

fn app(tool: Tool) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 400, "height": 300, "background": "transparent"})).unwrap();
    app.run("shape.create", json!({"kind": "rect", "rect": [0, 0, 200, 300], "fill": "#ff0000"})).unwrap();
    app.run("shape.create", json!({"kind": "rect", "rect": [200, 0, 200, 300], "fill": "#00ff00"})).unwrap();
    app.run("layer.new.layer", json!({})).unwrap();
    app.run("tools.setColors", json!({"foreground": [0.0, 0.0, 1.0, 1.0], "background": [1.0, 1.0, 1.0, 1.0]})).unwrap();
    app.ui.tool = tool;
    app.sync_views();
    app
}

fn steps(app: &PhotocraftApp) -> usize {
    app.session.active().unwrap().history.past_len()
}

/// Every drag pick applies, not only the release: the swatch follows the pointer.
#[test]
fn eyedropper_drag_updates_the_sampled_colour() {
    let mut app = app(Tool::Eyedropper);
    let none = egui::Modifiers::NONE;
    tool_event(&mut app, ToolEvent::Down { x: 100.0, y: 100.0, pressure: 1.0 }, none);
    assert_eq!(app.session.tools.foreground, [1.0, 0.0, 0.0, 1.0], "the press picked the red half");
    tool_event(&mut app, ToolEvent::Move { x: 300.0, y: 100.0, pressure: 1.0 }, none);
    assert_eq!(app.session.tools.foreground, [0.0, 1.0, 0.0, 1.0], "the drag picked the green half before the release");
    tool_event(&mut app, ToolEvent::Up { x: 300.0, y: 100.0 }, none);
    assert_eq!(app.session.tools.foreground, [0.0, 1.0, 0.0, 1.0]);
    // Alt picks the background, and leaves the foreground.
    tool_event(&mut app, ToolEvent::Down { x: 100.0, y: 100.0, pressure: 1.0 }, egui::Modifiers::ALT);
    tool_event(&mut app, ToolEvent::Up { x: 100.0, y: 100.0 }, egui::Modifiers::ALT);
    assert_eq!(app.session.tools.background, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(app.session.tools.foreground, [0.0, 1.0, 0.0, 1.0]);
}

/// ⌥ on a painting tool is the Eyedropper for the whole press, whether ⌥ stays held or is let go
/// before the drag (#7802): it sets the foreground, never the background, and edits nothing. The
/// Mixer Brush's ⌥ loads paint instead of sampling, and paints nothing either.
#[test]
fn alt_press_samples_and_never_paints() {
    let (alt, none) = (egui::Modifiers::ALT, egui::Modifiers::NONE);
    let samples = [Tool::Brush, Tool::Pencil, Tool::Gradient, Tool::PaintBucket, Tool::BackgroundEraser, Tool::ColorReplacement];
    for tool in samples.into_iter().chain([Tool::MixerBrush]) {
        for release_alt in [true, false] {
            let mut app = app(tool);
            let (rev, before) = (app.session.active().unwrap().revision, steps(&app));
            tool_event(&mut app, ToolEvent::Down { x: 100.0, y: 100.0, pressure: 1.0 }, alt);
            if tool != Tool::MixerBrush {
                assert_eq!(app.session.tools.foreground, [1.0, 0.0, 0.0, 1.0], "{tool:?}: the press sampled the red under it");
            }
            let later = if release_alt { none } else { alt };
            tool_event(&mut app, ToolEvent::Move { x: 300.0, y: 100.0, pressure: 1.0 }, later);
            if tool != Tool::MixerBrush {
                assert_eq!(app.session.tools.foreground, [0.0, 1.0, 0.0, 1.0], "{tool:?} release_alt={release_alt}: the drag keeps sampling");
            }
            tool_event(&mut app, ToolEvent::Up { x: 300.0, y: 100.0 }, later);
            assert_eq!(app.session.tools.background, [1.0, 1.0, 1.0, 1.0], "{tool:?}: the background is untouched");
            assert_eq!(app.session.active().unwrap().revision, rev, "{tool:?} release_alt={release_alt}: nothing painted");
            assert_eq!(steps(&app), before, "{tool:?} release_alt={release_alt}: no history step");
        }
    }
    // Ctrl+Alt is the brush-resize drag and the Eraser has no Alt sample: neither sets the foreground.
    for (tool, mods) in [(Tool::Brush, egui::Modifiers { alt: true, ctrl: true, ..Default::default() }), (Tool::Eraser, alt)] {
        let mut app = app(tool);
        tool_event(&mut app, ToolEvent::Down { x: 100.0, y: 100.0, pressure: 1.0 }, mods);
        tool_event(&mut app, ToolEvent::Up { x: 100.0, y: 100.0 }, mods);
        assert_eq!(app.session.tools.foreground, [0.0, 0.0, 1.0, 1.0], "{tool:?} {mods:?}: not a sample");
    }
}
