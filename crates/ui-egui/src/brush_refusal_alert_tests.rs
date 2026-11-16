//! The refusal table: which press gets which alert (#11898, #12017, #12536, #12552, #12952, #13030),
//! which keeps today's status line, and which still paints. The alert is opened once through
//! `dialogs::alerts::alert`, so a second press while it is open adds none.

use egui::Modifiers;
use serde_json::json;

use super::*;
use crate::canvas::{ToolEvent, tool_event};

/// A 64 × 64 document with a painted layer active, black foreground, `tool`.
pub(in crate::tools) fn app(tool: Tool) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 64, "height": 64})).unwrap();
    app.run("layer.new.layer", json!({})).unwrap();
    app.run("edit.fill", json!({"color": "#dc1e1e"})).unwrap();
    app.run("tools.setColors", json!({"foreground": "#000000"})).unwrap();
    app.ui.tool = tool;
    app.sync_views();
    app
}

pub(super) fn active(app: &PhotocraftApp) -> u64 {
    app.session.active().unwrap().active_layer.unwrap().0
}

pub(in crate::tools) fn steps(app: &PhotocraftApp) -> usize {
    app.session.active().unwrap().history.past_len()
}

/// A press, a short drag and the release with the active tool.
pub(in crate::tools) fn stroke(app: &mut PhotocraftApp) {
    tool_event(app, ToolEvent::Down { x: 20.0, y: 32.0, pressure: 1.0 }, Modifiers::NONE);
    tool_event(app, ToolEvent::Move { x: 44.0, y: 32.0, pressure: 1.0 }, Modifiers::NONE);
    tool_event(app, ToolEvent::Up { x: 44.0, y: 32.0 }, Modifiers::NONE);
}

/// The messages of the open framework alerts.
pub(in crate::tools) fn alerts(app: &PhotocraftApp) -> Vec<String> {
    app.framework.dialogs.open_dialogs().iter().filter_map(|d| d.fields.as_ref()?.get("message")?.as_str().map(str::to_string)).collect()
}

pub(in crate::tools) fn refused(app: &mut PhotocraftApp, want: &str) {
    let before = steps(app);
    stroke(app);
    assert_eq!(alerts(app), [want], "the alert, once");
    assert_eq!(steps(app), before, "nothing painted, no History step");
    let id = app.framework.dialogs.open_dialogs().first().map(|d| d.id).unwrap();
    let ctx = app.ctx().clone();
    crate::dialogs::confirm(app, &ctx, id).unwrap();
    assert!(alerts(app).is_empty(), "OK closes the alert");
}

/// What a press ends in.
enum Expect {
    /// An alert from a layer template, worded with `tool_name`.
    Layer(&'static str),
    /// An alert from a colour-mode template, worded with `mode_tool_name`.
    Mode(&'static str),
    /// An alert with exactly this text.
    Text(&'static str),
    /// One History step and no alert.
    Paints,
    /// No alert and today's hidden-target status line.
    Status,
    /// No alert.
    Silent,
}

fn hide(app: &mut PhotocraftApp) {
    let id = active(app);
    app.run("layer.setProps", json!({"layer": id, "visible": false})).unwrap();
}
fn lock_all(app: &mut PhotocraftApp) {
    app.run("layer.lockLayers", json!({"all": true})).unwrap();
}
fn lock_pixels(app: &mut PhotocraftApp) {
    app.run("layer.lockLayers", json!({"pixels": true})).unwrap();
}
fn lock_transparency(app: &mut PhotocraftApp) {
    app.run("layer.lockLayers", json!({"transparency": true})).unwrap();
}
fn group(app: &mut PhotocraftApp) {
    app.run("layer.new.group", json!({})).unwrap();
    assert!(app.session.active().unwrap().doc.layer(photocraft_doc::LayerId(active(app))).unwrap().is_group());
}
fn group_mask(app: &mut PhotocraftApp) {
    app.run("layer.new.group", json!({})).unwrap();
    app.run("layer.layerMask.revealAll", json!({})).unwrap();
    assert_eq!(crate::canvas::paint_target(app), json!("mask"));
}
fn bitmap(app: &mut PhotocraftApp) {
    app.run("image.mode.grayscale", json!({})).unwrap();
    app.run("image.mode.bitmap", json!({})).unwrap();
}
fn classic(app: &mut PhotocraftApp) {
    app.ui.tool_options.gradient_classic = true;
}
fn classic_hidden(app: &mut PhotocraftApp) {
    classic(app);
    hide(app);
}
fn classic_locked(app: &mut PhotocraftApp) {
    classic(app);
    lock_all(app);
}
fn classic_group(app: &mut PhotocraftApp) {
    classic(app);
    group(app);
}
fn classic_transparency(app: &mut PhotocraftApp) {
    classic(app);
    lock_transparency(app);
}

#[test]
fn a_refused_press_alerts_or_reports_by_case() {
    use Expect::*;
    type Setup = fn(&mut PhotocraftApp);
    let rows: [(&str, Tool, Setup, Expect); 31] = [
        ("brush hidden", Tool::Brush, hide, Layer(HIDDEN)),
        ("brush lock all", Tool::Brush, lock_all, Layer(LOCKED)),
        ("brush group", Tool::Brush, group, Layer(GROUP)),
        ("brush group mask", Tool::Brush, group_mask, Paints),
        ("brush lock transparency", Tool::Brush, lock_transparency, Paints),
        ("brush lock pixels keeps today", Tool::Brush, lock_pixels, Silent),
        ("pencil hidden", Tool::Pencil, hide, Status),
        ("sharpen hidden", Tool::Sharpen, hide, Layer(HIDDEN)),
        ("sharpen lock all", Tool::Sharpen, lock_all, Layer(LOCKED)),
        ("sharpen lock pixels", Tool::Sharpen, lock_pixels, Layer(LOCKED)),
        ("sharpen group", Tool::Sharpen, group, Layer(GROUP)),
        ("sharpen lock transparency", Tool::Sharpen, lock_transparency, Paints),
        (
            "sharpen bitmap before layer",
            Tool::Sharpen,
            |a| {
                bitmap(a);
                hide(a);
            },
            Mode(BITMAP),
        ),
        ("dodge keeps today", Tool::Dodge, lock_all, Silent),
        ("sponge hidden", Tool::Sponge, hide, Layer(HIDDEN)),
        ("sponge lock all", Tool::Sponge, lock_all, Layer(LOCKED)),
        ("sponge lock pixels", Tool::Sponge, lock_pixels, Layer(LOCKED)),
        ("sponge group", Tool::Sponge, group, Layer(GROUP)),
        ("paint bucket bitmap", Tool::PaintBucket, bitmap, Text(photocraft_engine::paint_cmds::BITMAP_REFUSAL)),
        ("paint bucket rgb fills", Tool::PaintBucket, |_| {}, Paints),
        ("remove lock all", Tool::Remove, lock_all, Text(REMOVE_LOCKED)),
        ("classic gradient hidden", Tool::Gradient, classic_hidden, Layer(HIDDEN)),
        ("classic gradient lock all", Tool::Gradient, classic_locked, Layer(LOCKED)),
        ("classic gradient group", Tool::Gradient, classic_group, Layer(GROUP)),
        ("classic gradient lock transparency", Tool::Gradient, classic_transparency, Paints),
        ("live gradient hidden", Tool::Gradient, hide, Silent),
        ("brush lock all, again", Tool::Brush, lock_all, Layer(LOCKED)),
        (
            "sponge lock position paints",
            Tool::Sponge,
            |a| {
                a.run("layer.lockLayers", json!({"position": true})).unwrap();
            },
            Paints,
        ),
        ("sharpen hidden, again", Tool::Sharpen, hide, Layer(HIDDEN)),
        ("sponge hidden, again", Tool::Sponge, hide, Layer(HIDDEN)),
        ("brush group, again", Tool::Brush, group, Layer(GROUP)),
    ];
    for (name, tool, setup, expect) in rows {
        let mut app = app(tool);
        setup(&mut app);
        match expect {
            Layer(template) | Mode(template) => {
                let tool_name = if matches!(expect, Layer(_)) { tool_name(&app, tool) } else { mode_tool_name(tool) }.expect(name);
                let want = crate::i18n::fmt(tl!(template), &[("tool", tool_name)]);
                // A second press while the alert is open adds none.
                stroke(&mut app);
                refused(&mut app, &want);
                assert!(app.ui.status.is_empty() || !app.ui.status_error, "{name}: no status-bar error as well");
            }
            Text(text) => {
                stroke(&mut app);
                refused(&mut app, tl!(text));
            }
            Paints => {
                let before = steps(&app);
                stroke(&mut app);
                assert!(alerts(&app).is_empty(), "{name}: no alert");
                assert_eq!(steps(&app), before + 1, "{name}: painted");
            }
            Status => {
                stroke(&mut app);
                assert!(alerts(&app).is_empty(), "{name}: no alert");
                assert_eq!(app.ui.status, photocraft_engine::hidden_target::HIDDEN, "{name}");
                assert_eq!(tool_name(&app, tool), None, "{name}: its wording was not measured");
            }
            Silent => {
                stroke(&mut app);
                assert!(alerts(&app).is_empty(), "{name}: no alert");
            }
        }
    }
}
