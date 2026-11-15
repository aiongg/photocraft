//! UI for the Eraser group's colour-aware tools: Magic Eraser (click → `paint.magicEraser`) and
//! Background Eraser (stroke → `paint.backgroundEraser`), with their options-bar members.

use craft::behavior::IconId;
use craft::ui::buttons::ToggleButton;
use craft::ui::choice::{Checkbox, Segmented};
use craft::ui::contract::{Bind, Entry, Props as _, Show as _};
use craft::ui::display::Divider;
use craft::ui::pickers::Select;
use craft::ui::primitives::IconSpec;
use serde_json::{Value, json};

use super::option_cells;
use crate::PhotocraftApp;
use crate::state::Tool;

fn report(app: &mut PhotocraftApp, r: Result<Value, String>) {
    if let Err(e) = r {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

/// Magic Eraser click at document `(x, y)`. Returns false for other tools.
pub fn click(app: &mut PhotocraftApp, tool: Tool, x: f64, y: f64) -> bool {
    if tool != Tool::MagicEraser {
        return false;
    }
    let o = &app.ui.tool_options;
    let p = json!({
        "x": x.floor(),
        "y": y.floor(),
        "tolerance": o.tolerance,
        "antiAlias": o.anti_alias,
        "contiguous": o.contiguous,
        "sampleAllLayers": o.sample_all.magic_eraser,
        "opacity": o.magic_eraser_opacity,
    });
    let r = app.run("paint.magicEraser", p);
    report(app, r);
    true
}

/// Finishes a Background Eraser stroke. Returns false for other tools.
pub fn finish_stroke(app: &mut PhotocraftApp, tool: Tool, points: &[[f64; 3]], line: bool) -> bool {
    if tool != Tool::BackgroundEraser {
        return false;
    }
    let mut p = background_eraser_params(app, points);
    crate::canvas::unsmoothed_line(&mut p, line);
    let r = app.run("paint.backgroundEraser", p);
    report(app, r);
    true
}

/// `paint.backgroundEraser` params for a stroke along `points`: shared by the live preview and
/// the commit, with the jitter seed from the first point (`retouch_ui::stroke_seed`).
pub(crate) fn background_eraser_params(app: &PhotocraftApp, points: &[[f64; 3]]) -> Value {
    let o = &app.ui.tool_options;
    json!({
        "points": points,
        "sampling": o.bg_sampling,
        "limits": o.bg_limits,
        "tolerance": stored_tolerance(o.bg_tolerance),
        "protectForegroundColor": o.bg_protect_fg,
        "seed": crate::retouch_ui::stroke_seed(points),
    })
}

/// The tool a drag with `tool` runs as: the History Brush for an Eraser that erases to history. ⌥
/// at the press reverses the option for that stroke, as in Photoshop.
pub fn drag_tool(app: &PhotocraftApp, tool: Tool, alt: bool) -> Tool {
    if tool == Tool::Eraser && app.ui.tool_options.erase_to_history != alt { Tool::HistoryBrush } else { tool }
}

/// Whether `tool` is the Eraser in Pencil mode: it erases hard, aliased pixels through
/// `paint.pencil` (`"erase": true`) and shows the Pencil's pixel-grid cursor (#2662).
pub fn pencil_mode(app: &PhotocraftApp, tool: Tool) -> bool {
    tool == Tool::Eraser && app.ui.tool_options.eraser_mode == "pencil"
}

/// Whether `tool` is the Eraser in Block mode: it erases a hard square of 16 screen pixels
/// (`16 / zoom` document pixels) through `paint.pencil` (`"block": true`); Size, Hardness,
/// Opacity and Flow don't apply (#2770).
pub fn block_mode(app: &PhotocraftApp, tool: Tool) -> bool {
    tool == Tool::Eraser && app.ui.tool_options.eraser_mode == "block"
}

/// The Block's edge in document pixels at the current zoom (what the engine erases).
pub fn block_size(app: &PhotocraftApp) -> f32 {
    photocraft_engine::brush_cmds::block_size(f64::from(app.point_zoom()))
}

/// A Tolerance as the field stores it: a whole number in 1..=100, the default 50 for a value
/// that is no number. Strokes use it too, so a restored setting erases the same before and after
/// the options bar shows it.
pub(crate) fn stored_tolerance(v: f32) -> f32 {
    if v.is_finite() { v.round().clamp(1.0, 100.0) } else { 50.0 }
}

/// A check box named `label` over `value`.
fn check(ui: &mut egui::Ui, label: &str, value: &mut bool, tip: Option<&str>) {
    let mut checkbox = Checkbox::new(Bind::bound(value)).label(label.to_owned());
    if let Some(tip) = tip {
        checkbox = checkbox.tooltip(tip.to_owned());
    }
    checkbox.show(ui);
}

/// The members of the Eraser group's options bars, added to the host's `Bar` body. Returns false
/// for other tools. The Eraser's own Mode is `PaintBar::eraser_mode`, not a member here.
pub fn options_bar(ui: &mut egui::Ui, app: &mut PhotocraftApp, tool: Tool) -> bool {
    match tool {
        Tool::MagicEraser => magic_eraser_members(ui, app),
        Tool::BackgroundEraser => background_eraser_members(ui, app),
        _ => return false,
    }
    true
}

fn magic_eraser_members(ui: &mut egui::Ui, app: &mut PhotocraftApp) {
    let o = &mut app.ui.tool_options;
    // The field clamps without an alert, as the reference's Magic Eraser Tolerance does.
    let _ = option_cells::number(tl!("Tolerance:"), &mut o.tolerance, 0.0..=255.0).show(ui);
    check(ui, tl!("Anti-alias"), &mut o.anti_alias, None);
    check(ui, tl!("Contiguous"), &mut o.contiguous, None);
    check(ui, tl!("Sample All Layers"), &mut o.sample_all.magic_eraser, None);
    Divider::vertical().show(ui);
    let _ = option_cells::number(tl!("Opacity:"), &mut o.magic_eraser_opacity, 1.0..=100.0).unit("%").show(ui);
}

fn background_eraser_members(ui: &mut egui::Ui, app: &mut PhotocraftApp) {
    let mut alerts = Vec::new();
    {
        let o = &mut app.ui.tool_options;
        let sampling = [
            Entry::new("continuous".to_string(), tl!("Sampling: Continuous")).icon(IconId::new("eyedropper")),
            Entry::new("once".to_string(), tl!("Sampling: Once")).icon(IconId::new("target")),
            Entry::new("backgroundSwatch".to_string(), tl!("Sampling: Background Swatch")).icon(IconId::new("square")),
        ];
        let picked = Segmented::new(tl!("Sampling"), Bind::given(o.bg_sampling.clone()), &sampling).show(ui).value().cloned();
        if let Some(sampling) = picked {
            o.bg_sampling = sampling;
        }
        Divider::vertical().show(ui);

        let limits = [
            Entry::new("discontiguous".to_string(), tl!("Discontiguous")),
            Entry::new("contiguous".to_string(), tl!("Contiguous")),
            Entry::new("findEdges".to_string(), tl!("Find Edges")),
        ];
        let picked = Select::new(tl!("Limits:"), Bind::given(o.bg_limits.clone()), &limits)
            .tooltip(tl!("How far to let the erasing spread").to_owned())
            .show(ui)
            .events
            .committed()
            .cloned();
        if let Some(limits) = picked {
            o.bg_limits = limits;
        }

        // Shown as the stroke reads it; written back only when the user edits it.
        let shown = stored_tolerance(o.bg_tolerance);
        let mut tolerance = shown;
        let (_, alert) = option_cells::number(tl!("Tolerance:"), &mut tolerance, 1.0..=100.0)
            .whole()
            .unit("%")
            .tooltip(tl!("Erase range of similar colors"))
            .show(ui);
        if tolerance != shown {
            o.bg_tolerance = tolerance;
        }
        alerts.extend(alert);
    }

    // Angle and Pressure for Size edit the tool's own brush (the shell swaps one in per tool).
    let before = app.session.tools.brush.clone();
    let mut brush = before.clone();
    let mut angle = brush.angle;
    // PLAN: assumed `NumberCell::show(self, ui) -> (EditEvents<f64>, Option<RangeAlert>)`; its events key the Angle's history step.
    let (angle_events, alert) = option_cells::number(tl!("Angle"), &mut angle, -180.0..=180.0)
        .whole()
        .unit("°")
        .icon(IconSpec::svg(IconId::new("rotate_cw")))
        .tooltip(tl!("Set the brush angle"))
        .show(ui);
    let mut key = angle_events.iter().find_map(|e| crate::history_keys::coalesce("brush", e));
    brush.angle = angle;
    alerts.extend(alert);

    check(ui, tl!("Protect Foreground Color"), &mut app.ui.tool_options.bg_protect_fg, Some(tl!("Do not erase the foreground swatch color")));

    let pressure = ToggleButton::new(tl!("Always use pressure for size"), Bind::given(brush.pressure_size))
        .icon(IconId::new("target"))
        .icon_only()
        .tooltip(tl!("Always use Pressure for Size. When off, Brush Preset controls pressure").to_owned())
        .show(ui);
    if let Some(on) = pressure.value().copied() {
        brush.pressure_size = on;
        key = pressure.events.iter().find_map(|e| crate::history_keys::coalesce("brush", e)).or(key);
    }
    match key {
        Some(key) => crate::brush_panel::commit_keyed(app, &before, &brush, key),
        None => crate::brush_panel::commit(app, &before, &brush),
    }

    for alert in &alerts {
        option_cells::range_alert(app, ui, alert);
    }
}

