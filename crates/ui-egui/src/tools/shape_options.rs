//! Shape toolbar edits share the engine commands used by the Properties panel.

use craft::behavior::{EditEvent, Mixed};
use craft::draw::stroke::StrokeStyle;
use craft::draw::ui::shape::{ShapeAppearance, ShapeBar, ShapeChange, ShapePaint, ShapeToolSettings};
use craft::draw::ui::shape_drag::ShapeKind;
use craft::ui::contract::{Bind, Show as _};
use craft::ui::dialog::{Alert, HostedAlert};
use craft::ui::runtime::env::Env;
use photocraft_doc::{Fill, LayerContent};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::shape_gradient_ui::{self as shape_gradient, ShapeGradients};
use crate::shape_paint_ui::{ChipLibrary, PaintChange, decorate_chip, gradient_fill, paint_of, paint_param, rgba, srgba};
use crate::shape_stroke_ui::StrokeOptions;
use crate::state::Tool;
use crate::vector_ui::enabled_stroke_width;

fn solid(color: [f32; 4]) -> photocraft_doc::Fill {
    photocraft_doc::Fill::Solid(photocraft_doc::Color::rgba(color[0], color[1], color[2], color[3]))
}

/// The shape layer the options bar edits.
struct SelectedShape {
    doc: photocraft_doc::DocId,
    layer: photocraft_doc::LayerId,
    fill: Option<photocraft_doc::Fill>,
    stroke: Option<photocraft_doc::ShapeStroke>,
    live: Option<photocraft_doc::LiveShape>,
}

fn selected_shape(session: &photocraft_engine::Session) -> Option<SelectedShape> {
    let st = session.active()?;
    let layer = st.active_layer?;
    match &st.doc.layer(layer)?.content {
        LayerContent::Shape(sh) => Some(SelectedShape { doc: st.doc.id, layer, fill: sh.fill.clone(), stroke: sh.stroke.clone(), live: sh.live.clone() }),
        _ => None,
    }
}

/// The geometry the bar shows: the tool's, or for Path and Direct Selection the selected live shape's.
/// `None` for a tool that has no shape options.
fn kind_of(tool: Tool, target: Option<&SelectedShape>) -> Option<ShapeKind> {
    Some(match tool {
        Tool::Rectangle => ShapeKind::Rectangle,
        Tool::EllipseShape => ShapeKind::Ellipse,
        Tool::Triangle => ShapeKind::Triangle,
        Tool::Polygon => ShapeKind::Polygon,
        Tool::Star => ShapeKind::Star,
        Tool::Line => ShapeKind::Line,
        Tool::CustomShape => ShapeKind::Custom,
        Tool::PathSelection | Tool::DirectSelection => match target.and_then(|s| s.live.as_ref()) {
            Some(photocraft_doc::LiveShape::Rect { .. }) => ShapeKind::Rectangle,
            Some(photocraft_doc::LiveShape::Ellipse { .. }) => ShapeKind::Ellipse,
            _ => ShapeKind::Custom,
        },
        _ => return None,
    })
}

/// The value an event proposes: a running gesture's, a committed or one-step value, or the value a
/// cancelled gesture restores.
fn proposal(event: &EditEvent<ShapeChange>) -> Option<&ShapeChange> {
    match event {
        EditEvent::Update { value, .. } => Some(value),
        EditEvent::Commit { after, .. } | EditEvent::Set { after, .. } => Some(after),
        EditEvent::Cancel { before: Some(Mixed::Same(value)), .. } => Some(value),
        _ => None,
    }
}

/// The history key part of a change: edits of one field in one gesture coalesce into one step.
fn field_of(change: &ShapeChange) -> &'static str {
    match change {
        ShapeChange::Fill(_) | ShapeChange::FillOn(_) => "fill",
        ShapeChange::Stroke { .. } => "stroke",
        ShapeChange::StrokeWidth(_) => "stroke-w",
        ShapeChange::StrokeStyle { .. } => "stroke-style",
        ShapeChange::Radii(_) | ShapeChange::CornerRadius(_) => "radius",
        ShapeChange::Sides(_) => "sides",
        ShapeChange::Weight(_) => "weight",
        _ => "other",
    }
}

/// Whether a cancelled gesture's restore value is already what the bar shows: no `Update` of the gesture
/// reached the document, so a restore would add an empty history step and a cancelled pick must not run.
fn already_shown(change: &ShapeChange, appearance: &ShapeAppearance, settings: &ShapeToolSettings) -> bool {
    match change {
        ShapeChange::Fill(p) => appearance.fill == *p,
        ShapeChange::Stroke { paint, .. } => appearance.stroke == *paint,
        ShapeChange::StrokeWidth(w) => appearance.stroke_width == *w,
        ShapeChange::StrokeStyle { style, .. } => appearance.stroke_style == *style,
        ShapeChange::CornerRadius(r) => settings.corner_radius == *r,
        ShapeChange::Sides(n) => settings.sides == *n,
        ShapeChange::Weight(w) => settings.line_weight == *w,
        _ => false,
    }
}

/// The paint change a chip's paint stands for; a gradient is [`gradient_fill`] over `gradient`, the chip's
/// [`ChipLibrary::gradient`].
fn paint_change(paint: &craft::color::ui::Paint, gradient: &Fill) -> Option<PaintChange> {
    match paint {
        craft::color::ui::Paint::None => Some(PaintChange::None),
        craft::color::ui::Paint::Solid(c) => Some(PaintChange::Color(rgba(*c))),
        craft::color::ui::Paint::Gradient(preview) => Some(PaintChange::Gradient(gradient_fill(preview, gradient))),
        _ => None,
    }
}

/// Applies a paint change to the tool defaults; false when the engine refused it.
fn apply_tool_paint(app: &mut PhotocraftApp, stroke: bool, change: &PaintChange) -> bool {
    match set_tool_paint(app, stroke, change.clone()) {
        Ok(()) => true,
        Err(e) => {
            app.ui.status = e;
            app.ui.status_error = true;
            false
        }
    }
}

/// The `shape.edit` parameters for a paint change of the fill or the stroke (`width` the stroke width in force).
fn paint_edit(stroke: bool, change: &PaintChange, width: f32) -> Value {
    match (stroke, change) {
        (false, PaintChange::None) => json!({"fill": null}),
        (false, PaintChange::Color(c)) => json!({"fill": c}),
        (false, PaintChange::Gradient(g)) => json!({"fill": paint_param(g)}),
        (true, PaintChange::None) => json!({"stroke": null}),
        (true, PaintChange::Color(c)) => json!({"stroke": {"color": c, "width": enabled_stroke_width(width)}}),
        (true, PaintChange::Gradient(g)) => json!({"stroke": {"color": paint_param(g), "width": enabled_stroke_width(width)}}),
    }
}

/// The Shape tools' options. `paint` shows Fill and Stroke: off in Path mode, where a drag draws
/// only an outline, but the tool's geometry (radius, sides, weight, custom shape) stays.
///
/// A pass applies only its own edits: the values are read from the app at the start of each pass
/// and `ShapeBar` reports an edit only in the pass that read the input, so a second pass in the
/// same frame sees the stored result and writes nothing.
pub(crate) fn options_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui, tool: Tool, paint: bool) {
    // Path mode edits only the next outline: an active shape layer is not the target.
    let target = if paint { selected_shape(&app.session) } else { None };
    let Some(kind) = kind_of(tool, target.as_ref()) else { return };
    // Fill and Stroke edit the selected shape layer too (#3347), and still set the next shape's.
    let (fg, bg) = (app.session.tools.foreground, app.session.tools.background);
    let o = &app.ui.tool_options;
    let (fill, stroke) = match &target {
        Some(s) => (s.fill.clone(), s.stroke.clone()),
        None => {
            let fill = o.shape_fill_gradient.clone().unwrap_or_else(|| solid(fg));
            let stroke = o.shape_stroke_gradient.clone().unwrap_or_else(|| solid(bg));
            (o.shape_fill.then_some(fill), (o.stroke_width > 0.0).then(|| o.shape_stroke.stroke(o.stroke_width, stroke)))
        }
    };
<<<<<<< HEAD
    let width = stroke.as_ref().map_or(0.0, |s| s.width);
    let has_stroke = stroke.is_some();
    let rect_target = matches!(target.as_ref().and_then(|s| s.live.as_ref()), Some(photocraft_doc::LiveShape::Rect { .. }));
    let mut settings = ShapeToolSettings::default();
    settings.fill_on = o.shape_fill;
    settings.corner_radius = match target.as_ref().and_then(|s| s.live.as_ref()) {
        Some(photocraft_doc::LiveShape::Rect { radii, .. }) => f64::from(radii[0]),
        _ => f64::from(o.corner_radius),
    };
    settings.sides = o.polygon_sides;
    settings.line_weight = f64::from(o.line_weight);
    settings.stroke_width = f64::from(o.stroke_width);
    let style = stroke.as_ref().map_or_else(|| o.shape_stroke.clone(), StrokeOptions::from);
    let locked = target.as_ref().is_some_and(|s| {
        app.session.active().is_some_and(|st| {
            let locks = st.doc.effective_locks(s.layer);
            locks.all || locks.pixels
        })
    });
    let appearance = craft::draw::ui::shape::ShapeAppearance::new(paint_of(fill.as_ref()), paint_of(stroke.as_ref().map(|s| &s.paint)), f64::from(width))
        .stroke_style(StrokeStyle::from(&style))
        .locked(locked);
    let ppi = app.session.active().map_or(72.0, |st| f64::from(st.doc.resolution_dpi));
    let sampled = [ShapePaint::Fill, ShapePaint::Stroke].into_iter().find_map(|p| crate::screen_picker::take(app, ("shape-bar", p)).map(|c| (p, c)));
    let recent_colors: Vec<craft::theme::Srgba> =
        crate::paint_picker::recent_colors(&app.session).into_iter().map(|c| srgba([c[0], c[1], c[2], 1.0])).collect();
    let groups = ShapeGradients::groups_of(&app.session.presets.gradients, fg, bg);
    let gradients = ShapeGradients { groups, current: &app.session.presets.gradient, recent: &mut app.ui.recent_gradients, fg, bg };
    let current = [fill.clone(), stroke.as_ref().map(|s| s.paint.clone())];
    let mut library =
        ChipLibrary::new(&recent_colors, &app.session.presets.swatches, &mut app.ui.paint_folders, gradients, &mut app.ui.shape_gradient_state, current);
    let out = ShapeBar::new(kind, &settings, &appearance, ppi)
        .paint(paint)
        .fallbacks(srgba(fg), srgba(bg))
        .sampled(sampled)
        .stroke_open(Bind::bound(&mut app.ui.shape_stroke_open))
        .chips(|which, chip, chip_ui| decorate_chip(chip, which, &mut library, chip_ui))
        .show(ui);
    let (fill_gradient, stroke_gradient) = (library.gradient(ShapePaint::Fill), library.gradient(ShapePaint::Stroke));

=======
    let lib = crate::paint_picker::library(&app.session, ui.ctx());
    let mut gradients =
        GradientChoices { presets: &app.session.presets.gradients, current: &app.session.presets.gradient, recent: &mut app.ui.recent_gradients, fg, bg };
    let mut width = stroke.as_ref().map_or(0.0, |s| s.width);
    let stroke_enabled = stroke.as_ref().is_some_and(|s| s.width > 0.0);
    let style = stroke.as_ref().map_or_else(|| o.shape_stroke.clone(), crate::shape_stroke_ui::StrokeOptions::from);
    let gesture_key = ui.scope_id().with("shape-options-paint-gesture");
    let gesture = paint_gesture(ui, gesture_key);
    let key = |k: &str| target.as_ref().map(|s| format!("shape-options-{:?}-{}-{k}:{}:{gesture}", s.doc, s.layer.0, gesture_key.value()));
    let mut edit: Option<Value> = None;
    let (mut fill_color, mut stroke_color) = (None, None);
    if paint {
        lbl(ui, tl!("Fill:"));
        if crate::widgets::checkbox(ui, &mut has_fill, tl!("Fill")).changed() {
            o.shape_fill = has_fill;
            edit = Some(json!({"fill": if has_fill { o.shape_fill_gradient.as_ref().map_or_else(|| json!(fg), paint_param) } else { Value::Null }}));
        }
        fill_color = swatch(ui, &lib, fill.as_ref(), fg, tl!("Fill color"), &mut gradients);
        lbl(ui, tl!("Stroke:"));
        stroke_color = swatch(ui, &lib, stroke.as_ref().map(|s| &s.paint), bg, tl!("Stroke color"), &mut gradients);
        if crate::widgets::value_field(ui, &mut width, 0.0..=288.0, "px", 58.0).accessible_name(tl!("Stroke:")).changed() {
            o.stroke_width = width;
            // A 0 px "change" (an arrow step clamped at the minimum) must not add a stroke.
            if stroke.is_some() || width > 0.0 {
                let mut p = json!({"width": width});
                if stroke.is_none() {
                    p["color"] = o.shape_stroke_gradient.as_ref().map_or_else(|| json!(bg), paint_param);
                }
                edit = Some(json!({"stroke": p, "coalesce": key("stroke-w")}));
            }
        }
        let preview = target.as_ref().filter(|s| s.stroke.is_some()).map(|s| (s.doc, s.layer));
        if let Some(change) =
            crate::shape_stroke_ui::button(ui, "tool-stroke", &style, &mut app.ui.stroke_editor, true, tool != Tool::EllipseShape, preview, stroke_enabled)
        {
            let mut p = change.options.params();
            if !stroke_enabled {
                p["width"] = json!(3.0);
            }
            if stroke.is_none() {
                p["color"] = o.shape_stroke_gradient.as_ref().map_or_else(|| json!(bg), paint_param);
            }
            edit = Some(json!({"stroke": p, "coalesce": change.coalesce}));
            o.shape_stroke = change.options;
            o.stroke_width = enabled_stroke_width(o.stroke_width);
        }
    }
    let (mut radius_alert, mut sides_alert) = (false, false);
    match tool {
        Tool::Rectangle => {
            crate::widgets::vline(ui, 22.0);
            let label = tl!("Radius:");
            lbl(ui, label);
            // The tool's own radius for the next rectangle (#13338); the active shape is not touched.
            let mut r = o.corner_radius;
            let (resp, out_of_range) = crate::widgets::value_field_alerting(ui, label, &mut r, RADIUS_RANGE, "px", 62.0);
            radius_alert = out_of_range;
            if resp.changed() {
                o.corner_radius = r;
            }
        }
        Tool::Polygon => {
            crate::widgets::vline(ui, 22.0);
            let label = tl!("Sides:");
            lbl(ui, label);
            sides_alert = sides_field(ui, label, &mut o.polygon_sides);
        }
        Tool::Triangle => {
            crate::widgets::vline(ui, 22.0);
            // Only a shape layer has pixel edges to align: greyed in Path mode (and in a pixels mode).
            ui.add_enabled_ui(o.shape_mode == "shape", |ui| crate::widgets::checkbox(ui, &mut o.align_edges, tl!("Align Edges")));
        }
        Tool::Line => {
            crate::widgets::vline(ui, 22.0);
            lbl(ui, tl!("Weight:"));
            crate::widgets::value_field(ui, &mut o.line_weight, 1.0..=1000.0, "px", 58.0);
        }
        Tool::CustomShape => {
            crate::widgets::vline(ui, 22.0);
            crate::preset_panels::shape_picker(app, ui);
        }
        _ if matches!(tool, Tool::PathSelection | Tool::DirectSelection) => {
            if let Some(photocraft_doc::LiveShape::Rect { radii, .. }) = target.as_ref().and_then(|s| s.live.as_ref()) {
                crate::widgets::vline(ui, 22.0);
                lbl(ui, tl!("Radius:"));
                let mut r = radii[0] as f32;
                if crate::widgets::value_field(ui, &mut r, 0.0..=10000.0, "px", 62.0).changed() {
                    o.corner_radius = r;
                    let rf = r as f64;
                    edit = Some(json!({"radii": [rf, rf, rf, rf], "coalesce": key("radius")}));
                }
            }
        }
        _ => {}
    }
    // A swatch picked in a Fill/Stroke popover joins the Recently Used Colors once it was applied.
>>>>>>> upstream/main
    let mut recent: Option<[f32; 3]> = None;
    let mut remembered: Vec<Fill> = Vec::new();
    for event in &out.events {
        let Some(change) = proposal(event) else { continue };
        if matches!(event, EditEvent::Cancel { .. }) && already_shown(change, &appearance, &settings) {
            continue;
        }
        let committed = !matches!(event, EditEvent::Update { .. } | EditEvent::Cancel { .. });
        let mut edit: Option<Value> = None;
        let mut applied = true;
        let mut picked: Option<(bool, PaintChange)> = None;
        match change {
            ShapeChange::Fill(p) => {
                if let Some(pc) = paint_change(p, &fill_gradient) {
                    applied = apply_tool_paint(app, false, &pc);
                    edit = Some(paint_edit(false, &pc, width));
                    picked = Some((false, pc));
                }
            }
            ShapeChange::Stroke { paint: p, .. } => {
                if let Some(pc) = paint_change(p, &stroke_gradient) {
                    applied = apply_tool_paint(app, true, &pc);
                    edit = Some(paint_edit(true, &pc, width));
                    picked = Some((true, pc));
                }
            }
            ShapeChange::FillOn(on) => {
                app.ui.tool_options.shape_fill = *on;
                let o = &app.ui.tool_options;
                edit = Some(json!({"fill": if *on { o.shape_fill_gradient.as_ref().map_or_else(|| json!(fg), paint_param) } else { Value::Null }}));
            }
            ShapeChange::StrokeWidth(w) => {
                let w = *w as f32;
                app.ui.tool_options.stroke_width = w;
                // A 0 px "change" (an arrow step clamped at the minimum) must not add a stroke.
                if has_stroke || w > 0.0 {
                    let mut p = json!({"width": w});
                    if !has_stroke {
                        p["color"] = app.ui.tool_options.shape_stroke_gradient.as_ref().map_or_else(|| json!(bg), paint_param);
                    }
                    edit = Some(json!({"stroke": p}));
                }
            }
            ShapeChange::StrokeStyle { style, width: enabled, .. } => {
                let options = StrokeOptions::from_style(style);
                let mut p = options.params();
                if let Some(w) = enabled {
                    p["width"] = json!(w);
                }
                if !has_stroke {
                    p["color"] = app.ui.tool_options.shape_stroke_gradient.as_ref().map_or_else(|| json!(bg), paint_param);
                }
                app.ui.tool_options.shape_stroke = options;
                app.ui.tool_options.stroke_width = enabled_stroke_width(app.ui.tool_options.stroke_width);
                edit = Some(json!({"stroke": p}));
            }
            ShapeChange::CornerRadius(r) => {
                let r = *r as f32;
                app.ui.tool_options.corner_radius = r;
                if rect_target {
                    let rf = f64::from(r);
                    edit = Some(json!({"radii": [rf, rf, rf, rf]}));
                }
            }
            ShapeChange::Radii(r) => {
                app.ui.tool_options.corner_radius = r[0] as f32;
                if rect_target {
                    edit = Some(json!({"radii": r}));
                }
            }
            ShapeChange::Sides(n) => app.ui.tool_options.polygon_sides = *n,
            ShapeChange::Weight(w) => app.ui.tool_options.line_weight = *w as f32,
            _ => {}
        }
        if let (Some(mut p), Some(s)) = (edit, &target) {
            p["layer"] = json!(s.layer.0);
            let key = crate::history_keys::coalesce(&format!("shape-options-{:?}-{}-{}", s.doc, s.layer.0, field_of(change)), event);
            p["coalesce"] = json!(key);
            if let Err(e) = app.run("shape.edit", p) {
                app.ui.status = e;
                app.ui.status_error = true;
                applied = false;
            }
        }
        // A swatch picked in a Fill/Stroke popover joins the Recently Used Colors once it was applied.
        if applied && committed {
            match picked {
                Some((_, PaintChange::Color(c))) => recent = Some([c[0], c[1], c[2]]),
                Some((_, PaintChange::Gradient(g))) => remembered.push(g),
                _ => {}
            }
        }
    }
    if let Some(rgb) = recent {
        crate::paint_picker::remember(app, rgb);
    }
<<<<<<< HEAD
    for g in &remembered {
        shape_gradient::remember_recent(&mut app.ui.recent_gradients, g);
    }
    if let Some((which, _request)) = out.pick_color {
        // The dialog starts from the layer's own paint, so the chip's request is not needed.
        let (which, fallback) = match which {
            ShapePaint::Fill => (crate::shape_fill_ui::Paint::Fill, fg),
            ShapePaint::Stroke => (crate::shape_fill_ui::Paint::Stroke, bg),
            _ => return,
        };
        crate::shape_fill_ui::open_paint(app, which, target.as_ref().map(|s| s.layer), true, fallback);
    }
    if let Some((which, gesture)) = out.screen_pick {
        crate::screen_picker::hold(app, ui.ctx(), ("shape-bar", which), gesture);
    }
    if let Some(range) = out.out_of_range {
        let title = match kind {
            ShapeKind::Polygon | ShapeKind::Star => tl!("Sides"),
            ShapeKind::Line => tl!("Weight"),
            _ => tl!("Radius"),
        };
        let alert = Alert::range(title.to_owned(), &range, &Env::of(ui));
        // An `Err` is the alert that was not opened because one is already up.
        let _ = app.framework.dialogs.open_unless_open(HostedAlert::new(alert));
    }
    if tool == Tool::CustomShape {
        crate::preset_panels::shape_picker(app, ui);
    }
}

=======
    range_alert(app, radius_alert, RADIUS_RANGE_ALERT);
    range_alert(app, sides_alert, SIDES_RANGE_ALERT);
}

/// The Rectangle tool's corner radius in px, as the reference takes it (#13336).
pub(crate) const RADIUS_RANGE: std::ops::RangeInclusive<f32> = 0.0..=150_000.0;

/// The reference's alert for a Rectangle tool Radius typed past [`RADIUS_RANGE`] (#13336).
pub(crate) const RADIUS_RANGE_ALERT: &str = "A value between 0.000 pixel and 150000.000 Pixels is required. Closest value inserted.";

/// The Polygon tool's Sides: whole numbers 3..100. Returns true once the field is left with a
/// value typed past them, so the caller shows [`SIDES_RANGE_ALERT`] (#13304).
fn sides_field(ui: &mut egui::Ui, label: &str, sides: &mut u32) -> bool {
    let mut s = *sides as f32;
    let (r, out_of_range) = crate::widgets::value_field_whole_alerting(ui, label, &mut s, 3.0..=100.0, "", 48.0);
    if r.changed() {
        *sides = s.round().clamp(3.0, 100.0) as u32;
    }
    out_of_range
}

/// The reference's alert for a Polygon Sides value typed past 3..100 (#13304).
pub(crate) const SIDES_RANGE_ALERT: &str = "An integer between 3 and 100 is required. Closest value inserted.";

/// Shows `alert` ([`RADIUS_RANGE_ALERT`], [`SIDES_RANGE_ALERT`]) as a modal with OK when `owed`,
/// once even if more than one pass asks.
fn range_alert(app: &mut PhotocraftApp, owed: bool, alert: &str) {
    if !owed {
        return;
    }
    let message = tl!(alert).to_owned();
    let shown = app.ui.dialogs.iter().any(|d| d.kind == crate::state::DialogKind::Error && d.fields.get("message").and_then(|m| m.as_str()) == Some(&message));
    if !shown {
        app.ui.open_dialog(crate::state::DialogKind::Error, json!({ "message": message }).as_object().cloned().unwrap_or_default());
    }
}

#[cfg(test)]
#[path = "rectangle_radius_range_tests.rs"]
mod rectangle_radius_range_tests;

#[cfg(test)]
#[path = "rectangle_radius_target_tests.rs"]
mod rectangle_radius_target_tests;

#[cfg(test)]
#[path = "polygon_sides_alert_tests.rs"]
mod polygon_sides_alert_tests;

>>>>>>> upstream/main
/// Tool-bar paint changes affect the next shape; Properties edits the selected shape instead.
pub(crate) fn set_tool_paint(app: &mut PhotocraftApp, stroke: bool, change: PaintChange) -> Result<(), String> {
    // A colour takes the place of a gradient; No Color keeps it for when the paint comes back.
    let o = &mut app.ui.tool_options;
    let gradient = if stroke { &mut o.shape_stroke_gradient } else { &mut o.shape_fill_gradient };
    match &change {
        PaintChange::Gradient(g) => *gradient = Some(g.clone()),
        PaintChange::Color(_) => *gradient = None,
        PaintChange::None => {}
    }
    if let PaintChange::Color(color) = change {
        app.run("tools.setColors", if stroke { json!({"background": color}) } else { json!({"foreground": color}) })?;
    }
    if stroke {
        app.ui.tool_options.stroke_width = if change == PaintChange::None { 0.0 } else { enabled_stroke_width(app.ui.tool_options.stroke_width) };
    } else {
        app.ui.tool_options.shape_fill = change != PaintChange::None;
    }
    Ok(())
}
