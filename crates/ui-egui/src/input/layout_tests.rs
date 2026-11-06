//! Keyboard-layout symbol keys (German ö/#, ⇧+ / ⇧-, the bracket-position fallback) through the
//! dispatcher's layout handling (`craft::keymap::Layout::adjust`), #3102, #3109.

use crate::{PhotocraftApp, state::Tool};
use egui::Modifiers;
use egui::{Event, Key};
use serde_json::json;

fn app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 32, "height": 32})).unwrap();
    app.ui.tool = Tool::Brush;
    app
}

/// One key press the way a layout reports it: logical `key`, physical position, typed text.
fn press(app: &mut PhotocraftApp, ctx: &egui::Context, key: Key, physical: Key, text: &str) {
    let modifiers = Modifiers::NONE;
    let press = Event::Key { key, physical_key: Some(physical), pressed: true, repeat: false, modifiers };
    let raw = egui::RawInput { events: vec![press, Event::Text(text.into())], ..Default::default() };
    ctx.begin_pass(raw);
    crate::shortcuts::handle(app, ctx);
    ctx.end_pass().textures_delta.clear();
}

/// A layout that types `+` on the US `]` key (Spanish, Italian...) and has no table row:
/// that key is `]` through the fallback, and the US `]` and a Dvorak `[` keep working.
#[test]
fn bracket_positions_are_the_brush_keys_on_other_layouts() {
    let (mut a, ctx) = (app(), egui::Context::default());
    let size = a.session.tools.brush.size;
    press(&mut a, &ctx, Key::Plus, Key::CloseBracket, "+");
    assert!(a.session.tools.brush.size > size, "the `+` key at the `]` position is ]");
    // AZERTY `^` is a dead key at the `[` position: it arrives as the physical key.
    press(&mut a, &ctx, Key::OpenBracket, Key::OpenBracket, "^");
    assert_eq!(a.session.tools.brush.size, size, "the key at the `[` position is [");
}

#[test]
fn real_brackets_switch_the_fallback_off() {
    // Dvorak: `[` is typed on the US `-` key, the US `]` position types `=`.
    let (mut a, ctx) = (app(), egui::Context::default());
    press(&mut a, &ctx, Key::OpenBracket, Key::Minus, "[");
    let size = a.session.tools.brush.size;
    press(&mut a, &ctx, Key::Equals, Key::CloseBracket, "=");
    assert_eq!(a.session.tools.brush.size, size, "= is not stolen once brackets exist");
}

#[test]
fn german_keys_do_not_also_use_the_fallback() {
    let (mut a, ctx) = (app(), egui::Context::default());
    press(&mut a, &ctx, Key::Semicolon, Key::Semicolon, "ö");
    let size = a.session.tools.brush.size;
    press(&mut a, &ctx, Key::Plus, Key::CloseBracket, "+");
    assert_eq!(a.session.tools.brush.size, size, "German + is not a brush key");
}
