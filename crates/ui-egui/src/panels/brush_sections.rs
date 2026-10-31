//! The Brush Settings panel's section bodies (Photoshop's right-hand side): Brush Tip Shape with
//! its tip grid and angle/roundness widget, Shape Dynamics, Scattering, Texture, Dual Brush, Color
//! Dynamics, Transfer, Brush Pose, Build-up and Smoothing. Each edits a working copy of the brush;
//! the panel turns the difference into one `tools.setBrush` call.

use craft::ui::theme::StrokeExt;
use craft::ui::theme::ToEgui;
use egui::{Color32, RichText, Sense, Stroke, pos2, vec2};
use photocraft_engine::BrushSettings;
use photocraft_engine::paint::{self, BrushPreset, Control, Dynamic, MAX_BRUSH_SIZE, MaskMode, Pattern, PatternStyle, TipShape};

use crate::brush_preview;
use crate::state::Tool;
use crate::theme;
use crate::widgets;

pub const CONTROLS: [(Control, &str); 8] = [
    (Control::Off, "Off"),
    (Control::Fade, "Fade"),
    (Control::PenPressure, "Pen Pressure"),
    (Control::PenTilt, "Pen Tilt"),
    (Control::StylusWheel, "Stylus Wheel"),
    (Control::Rotation, "Rotation"),
    (Control::InitialDirection, "Initial Direction"),
    (Control::Direction, "Direction"),
];

pub const MASK_MODES: [(MaskMode, &str); 10] = [
    (MaskMode::Multiply, "Multiply"),
    (MaskMode::Subtract, "Subtract"),
    (MaskMode::Darken, "Darken"),
    (MaskMode::Overlay, "Overlay"),
    (MaskMode::ColorDodge, "Color Dodge"),
    (MaskMode::ColorBurn, "Color Burn"),
    (MaskMode::LinearBurn, "Linear Burn"),
    (MaskMode::HardMix, "Hard Mix"),
    (MaskMode::LinearHeight, "Linear Height"),
    (MaskMode::Height, "Height"),
];

const PATTERNS: [(PatternStyle, &str); 4] =
    [(PatternStyle::Paper, "Paper"), (PatternStyle::Canvas, "Canvas"), (PatternStyle::Noise, "Noise"), (PatternStyle::Dots, "Dots")];

/// The library's labelled slider row over `*v`; returns whether it changed. A cancelled drag
/// (Esc) restores the value the drag began with.
fn row(ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>, unit: &str) -> bool {
    use craft::ui::composites::SliderRow;
    use craft::ui::edit::EditEvent;
    use craft::ui::localized::{LocaleNumbers, Unit};
    let numbers = LocaleNumbers::of(ui);
    // Rows repeat labels ("Minimum" under each dynamic), so the row's own position names it.
    let salt = ui.next_auto_id();
    let before = salt.with("before");
    let (lo, hi) = (f64::from(*range.start()), f64::from(*range.end()));
    let response = SliderRow::new(salt, tl!(label), f64::from(*v), lo..=hi, &numbers).unit(Unit::new(&format!("{{n}}{unit}"))).step(1.0).show(ui);
    let mut changed = false;
    for event in response.edits() {
        match event {
            EditEvent::Begin { value, .. } => {
                let start = *v;
                ui.data_mut(|d| d.insert_temp(before, start));
                if let Some(value) = value {
                    *v = value.round().clamp(lo, hi) as f32;
                    changed = true;
                }
            }
            EditEvent::Update { value, .. } | EditEvent::Set { value, .. } | EditEvent::Commit { value, changed: true, .. } => {
                *v = value.round().clamp(lo, hi) as f32;
                changed = true;
            }
            EditEvent::Cancel { .. } => {
                if let Some(old) = ui.data(|d| d.get_temp::<f32>(before)) {
                    *v = old;
                    changed = true;
                }
            }
            _ => {}
        }
    }
    changed
}

/// Percent slider over a 0..`max` fraction.
fn pct(ui: &mut egui::Ui, label: &str, v: &mut f32, max: f32) -> bool {
    signed_pct(ui, label, v, 0.0, max)
}

/// Percent slider over a `min..max` fraction.
fn signed_pct(ui: &mut egui::Ui, label: &str, v: &mut f32, min: f32, max: f32) -> bool {
    let mut p = (*v * 100.0).round();
    let changed = row(ui, label, &mut p, min * 100.0..=max * 100.0, "%");
    if changed {
        *v = (p.round() / 100.0).clamp(min, max);
    }
    changed
}

fn num(ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>, suffix: &str) -> bool {
    row(ui, label, v, range, suffix)
}

fn dim(ui: &mut egui::Ui, s: &str) {
    let t = craft::ui::theme::current(ui.ctx());
    ui.label(RichText::new(s).color(t.color.text.muted.to_egui()));
}

/// Whether a dynamic's control has a 0..1 input its own response curve shapes (not Off, not the
/// direction controls, not Angle Jitter, whose controls set an angle).
pub(crate) fn takes_curve(control: Control, angle: bool) -> bool {
    !angle && matches!(control, Control::Fade | Control::PenPressure | Control::PenTilt | Control::StylusWheel | Control::Rotation)
}

/// egui memory key of the open/closed state of a dynamic's curve editor.
pub(crate) fn curve_open_id(id: &str) -> egui::Id {
    egui::Id::unique(("brush-curve-open", id))
}

/// `Control: [Off ▾] [25] [~]`: the controller pop-up, Fade's step count and the button that
/// shows this dynamic's own response curve (lit while the curve isn't linear). `angle` offers the
/// direction controls (Photoshop only has them for Angle Jitter).
fn control_row(ui: &mut egui::Ui, id: &str, d: &mut Dynamic, angle: bool) {
    let open_id = curve_open_id(id);
    let mut open: bool = ui.data(|m| m.get_temp(open_id)).unwrap_or(false);
    ui.horizontal(|ui| {
        dim(ui, tl!("Control:"));
        let n = if angle || matches!(d.control, Control::InitialDirection | Control::Direction) { 8 } else { 6 };
        widgets::dropdown(ui, id, tl!("Control:"), &mut d.control, &CONTROLS[..n], 130.0);
        if d.control == Control::Fade {
            let mut steps = d.fade_steps as f32;
            if widgets::value_field(ui, &mut steps, 1.0..=9999.0, "", 56.0).accessible_name(tl!("Fade steps")).on_hover_text(tl!("Fade steps")).changed() {
                d.fade_steps = steps.round().clamp(1.0, 9999.0) as u32;
            }
        }
        if takes_curve(d.control, angle) && crate::icons::button(ui, "spline", 22.0, open || !d.curve.is_linear(), tl!("Response curve")).clicked() {
            open = !open;
        }
    });
    if open && takes_curve(d.control, angle) {
        let mut points = d.curve.points();
        if crate::pressure_curve_ui::edit_points(ui, egui::Id::unique(("brush-curve-rect", id)), &mut points) {
            d.curve = paint::ResponseCurve::new(&points);
        }
    }
    ui.data_mut(|m| m.insert_temp(open_id, open));
    ui.add_space(craft::ui::theme::scale::Space::S2.px());
}

/// A jitter slider, its Control row and (optionally) the Minimum slider: one Photoshop block.
fn dynamic(ui: &mut egui::Ui, id: &str, label: &str, d: &mut Dynamic, max: f32, minimum: Option<&str>, angle: bool) {
    pct(ui, label, &mut d.jitter, max);
    control_row(ui, id, d, angle);
    if let Some(m) = minimum {
        minimum_row(ui, m, d);
    }
    ui.add_space(craft::ui::theme::scale::Space::S6.px());
}

/// Minimum belongs to the control. Off disables editing without discarding its saved value.
fn minimum_row(ui: &mut egui::Ui, label: &str, d: &mut Dynamic) {
    ui.add_enabled_ui(d.control != Control::Off, |ui| pct(ui, label, &mut d.minimum, 1.0));
}

/// Size: a value field over a logarithmic slider (small sizes get most of the travel).
/// `restore` adds Photoshop's "Restore original size" button (sampled tips).
fn size_row(ui: &mut egui::Ui, label: &str, size: &mut f32, max: f32, restore: Option<f32>) {
    let t = craft::ui::theme::current(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(RichText::new(tl!(label)).color(t.color.text.muted.to_egui()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut s = *size;
            if widgets::value_field(ui, &mut s, 1.0..=max, "px", 74.0).accessible_name(tl!(label)).changed() {
                *size = s.round().clamp(1.0, max);
            }
            if let Some(orig) = restore
                && crate::icons::button(ui, "undo-2", 22.0, false, tl!("Restore original size")).clicked()
            {
                *size = orig.clamp(1.0, max);
            }
        });
    });
    let mut lv = size.max(1.0).ln();
    if widgets::slider(ui, &mut lv, 0.0..=max.ln(), None).changed() {
        *size = lv.exp().round().clamp(1.0, max);
    }
    ui.add_space(craft::ui::theme::scale::Space::S4.px());
}

/// Which part of the angle/roundness widget a drag grabbed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EllipseGrab {
    Angle,
    Roundness,
}

/// Outer radius of the angle/roundness widget's ellipse.
const ELLIPSE_R: f32 = 34.0;

/// Screen positions (relative to the widget centre) of the two roundness handles.
pub fn roundness_handles(angle: f32, roundness: f32) -> [egui::Vec2; 2] {
    let (s, c) = angle.to_radians().sin_cos();
    // The minor axis is the major axis turned 90° counter-clockwise; screen y points down.
    let minor = vec2(-s, -c) * ELLIPSE_R * roundness.clamp(0.0, 1.0);
    [minor, -minor]
}

/// What a press at `rel` (relative to the centre) grabs: a roundness handle within 7 px, else the angle.
pub fn ellipse_grab(rel: egui::Vec2, angle: f32, roundness: f32) -> EllipseGrab {
    if roundness_handles(angle, roundness).iter().any(|h| (*h - rel).length() <= 7.0) { EllipseGrab::Roundness } else { EllipseGrab::Angle }
}

/// The new (angle, roundness) for a drag to `rel`: the angle points at the pointer (degrees,
/// counter-clockwise, -180..180); the roundness is the pointer's distance along the minor axis.
pub fn ellipse_drag(grab: EllipseGrab, rel: egui::Vec2, angle: f32, roundness: f32) -> (f32, f32) {
    match grab {
        EllipseGrab::Angle if rel.length() > 1.0 => ((-rel.y).atan2(rel.x).to_degrees().round(), roundness),
        EllipseGrab::Angle => (angle, roundness),
        EllipseGrab::Roundness => {
            let (s, c) = angle.to_radians().sin_cos();
            let along = (rel.x * -s + rel.y * -c).abs();
            (angle, (along / ELLIPSE_R).clamp(0.01, 1.0))
        }
    }
}

/// Photoshop's angle/roundness widget: drag the arrow to rotate, the dots to squash.
fn ellipse_widget(ui: &mut egui::Ui, angle: &mut f32, roundness: &mut f32) -> bool {
    let t = craft::ui::theme::current(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(84.0, 84.0), Sense::click_and_drag());
    let c = rect.center();
    let grab_id = resp.id.with("grab");
    let mut changed = false;
    if resp.drag_started()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let g = ellipse_grab(p - c, *angle, *roundness);
        ui.data_mut(|d| d.insert_temp(grab_id, g == EllipseGrab::Roundness));
    }
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let grab = if resp.clicked() {
            ellipse_grab(p - c, *angle, *roundness)
        } else if ui.data(|d| d.get_temp::<bool>(grab_id)).unwrap_or(false) {
            EllipseGrab::Roundness
        } else {
            EllipseGrab::Angle
        };
        let (a, r) = ellipse_drag(grab, p - c, *angle, *roundness);
        if a != *angle || (r - *roundness).abs() > 1e-4 {
            *angle = a;
            *roundness = r;
            changed = true;
        }
    }
    let p = ui.painter();
    p.circle_filled(c, 40.0, t.color.surface.sunken.to_egui());
    p.circle_stroke(c, 40.0, craft::ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.color.line.control_edge.to_egui()));
    p.line_segment([c - vec2(40.0, 0.0), c + vec2(40.0, 0.0)], craft::ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.color.line.divider.to_egui()));
    p.line_segment([c - vec2(0.0, 40.0), c + vec2(0.0, 40.0)], craft::ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.color.line.divider.to_egui()));
    let (s, co) = angle.to_radians().sin_cos();
    let major = vec2(co, -s);
    let minor = vec2(-s, -co);
    let rr = roundness.clamp(0.01, 1.0);
    let pts: Vec<egui::Pos2> = (0..48)
        .map(|i| {
            let a = i as f32 / 48.0 * std::f32::consts::TAU;
            c + major * a.cos() * ELLIPSE_R + minor * a.sin() * ELLIPSE_R * rr
        })
        .collect();
    p.add(egui::Shape::convex_polygon(
        pts,
        t.color.text.normal.to_egui().gamma_multiply(0.18),
        craft::ui::theme::scale::Stroke::W1_5.egui(ui.ctx(), t.color.text.normal.to_egui()),
    ));
    // Arrow along the major axis.
    let tip = c + major * (ELLIPSE_R + 5.0);
    p.line_segment([c, tip], craft::ui::theme::scale::Stroke::W1_5.egui(ui.ctx(), t.color.accent.solid.to_egui()));
    let back = tip - major * 7.0;
    p.add(egui::Shape::convex_polygon(vec![tip, back + minor * 4.0, back - minor * 4.0], t.color.accent.solid.to_egui(), Stroke::NONE));
    for h in roundness_handles(*angle, *roundness) {
        p.circle_filled(c + h, 4.0, Color32::WHITE);
        p.circle_stroke(c + h, 4.0, craft::ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.color.accent.solid.to_egui()));
    }
    let _ = resp.on_hover_text(tl!("Drag the arrow to set the angle, the dots to set the roundness"));
    changed
}

/// The tip a preset contributes to a tip grid: (shape, size, hardness, angle, roundness, spacing).
pub fn preset_tip(p: &BrushPreset) -> (&TipShape, f32, f32, f32, f32, f32) {
    let b = &p.brush;
    (&b.tip, b.size, b.hardness, b.angle, b.roundness, b.spacing)
}

/// Thumbnails show relative size: small tips are drawn smaller (down to 30 %), 40 px and up fill the cell.
pub fn thumb_scale(size: f32) -> f32 {
    (size / 40.0).clamp(0.3, 1.0)
}

/// Photoshop's scrolling grid of preset tips with their sizes; `highlight` is the selected
/// preset (by identity: look-alike duplicates must stay distinct, so rows are keyed by index).
/// Returns the clicked index.
fn tip_grid(ui: &mut egui::Ui, id: &str, presets: &[BrushPreset], highlight: Option<usize>) -> Option<usize> {
    use craft::ui::presets::{PresetBrowser, PresetEvent, PresetRow, PresetState, PresetView};
    const CELL: f32 = 44.0;
    let rows: Vec<PresetRow<usize>> =
        presets.iter().enumerate().map(|(i, p)| PresetRow::item(i, p.name.as_str(), 0).caption(format!("{}", p.brush.size.round() as i64))).collect();
    let mut state = PresetState::<usize>::new();
    state.thumb = CELL;
    let out = PresetBrowser::new(id, tl!("Brush tips"), &mut state, &rows)
        .selected(highlight.as_ref())
        .thumb_range(CELL..=CELL)
        .view_lock(PresetView::Grid)
        .max_height(104.0)
        .show(ui, |painter, rect, info| {
            let Some(p) = presets.get(*info.key) else { return };
            let t = craft::ui::theme::current(painter.ctx());
            let (tip, size, hard, angle, round, _) = preset_tip(p);
            let tex = brush_preview::tip_texture(painter.ctx(), &format!("{id}:{}", p.name), tip, (hard, angle, round), 34, t.color.text.normal.to_egui());
            let ir = egui::Rect::from_center_size(rect.center(), rect.size() * thumb_scale(size));
            painter.image(tex.id(), ir, egui::Rect::from_min_max(egui::Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
        });
    ui.add_space(craft::ui::theme::scale::Space::S6.px());
    out.events.iter().find_map(|e| match e {
        PresetEvent::Select { key, .. } => Some(*key),
        _ => None,
    })
}

/// The Brush Tip Shape section: the tip grid and the tip's own controls. Picking a tip copies the
/// tip fields only — the dynamics sections stay as they are (Photoshop). The clicked preset's name
/// is returned so the caller can mark it current with `brush.presets.setCurrent` (by identity:
/// look-alike duplicates stay distinct; the engine can't infer it from the brush). The tip fields
/// are also copied into the working brush so this frame's controls start from them.
fn tip_shape(ui: &mut egui::Ui, b: &mut BrushSettings, presets: &[BrushPreset], current: Option<&str>) -> Option<String> {
    let highlight = current.and_then(|n| presets.iter().position(|p| p.name.eq_ignore_ascii_case(n)));
    if let Some(p) = tip_grid(ui, "brush-tip-grid", presets, highlight).and_then(|i| presets.get(i)) {
        let (pt, ps, ph, pa, pr, psp) = preset_tip(p);
        b.tip = pt.clone();
        b.size = ps;
        b.hardness = ph;
        b.angle = pa;
        b.roundness = pr;
        b.spacing = psp;
        return Some(p.name.clone());
    }
    let orig = b.tip.bitmap_size().map(|(w, h)| w.max(h) as f32);
    size_row(ui, tl!("Size"), &mut b.size, MAX_BRUSH_SIZE, orig);
    ui.horizontal(|ui| {
        widgets::checkbox(ui, &mut b.flip_x, tl!("Flip X"));
        ui.add_space(craft::ui::theme::scale::Space::S8.px());
        widgets::checkbox(ui, &mut b.flip_y, tl!("Flip Y"));
    });
    ui.add_space(craft::ui::theme::scale::Space::S4.px());
    ui.horizontal(|ui| {
        ellipse_widget(ui, &mut b.angle, &mut b.roundness);
        ui.add_space(craft::ui::theme::scale::Space::S8.px());
        ui.vertical(|ui| {
            ui.add_space(craft::ui::theme::scale::Space::S12.px());
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(vec2(72.0, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| dim(ui, tl!("Angle:")));
                let mut a = b.angle;
                if widgets::value_field(ui, &mut a, -180.0..=180.0, "°", 70.0).accessible_name(tl!("Angle:")).changed() {
                    b.angle = a.round().clamp(-180.0, 180.0);
                }
            });
            ui.add_space(craft::ui::theme::scale::Space::S6.px());
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(vec2(72.0, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| dim(ui, tl!("Roundness:")));
                let mut r = (b.roundness * 100.0).round();
                if widgets::value_field(ui, &mut r, 1.0..=100.0, "%", 70.0).accessible_name(tl!("Roundness:")).changed() {
                    b.roundness = (r.round() / 100.0).clamp(0.01, 1.0);
                }
            });
        });
    });
    ui.add_space(craft::ui::theme::scale::Space::S6.px());
    ui.add_enabled_ui(matches!(b.tip, TipShape::Round), |ui| {
        pct(ui, tl!("Hardness"), &mut b.hardness, 1.0);
    });
    // Unchecked, the pointer's speed sets the spacing (Photoshop).
    widgets::checkbox(ui, &mut b.spacing_enabled, tl!("Spacing")).on_hover_text(tl!("Off: the speed of the pointer sets the spacing"));
    ui.add_space(craft::ui::theme::scale::Space::S2.px());
    let on = b.spacing_enabled;
    ui.add_enabled_ui(on, |ui| signed_pct(ui, tl!("Spacing"), &mut b.spacing, 0.01, 10.0));
    None
}

fn shape_dynamics(ui: &mut egui::Ui, b: &mut BrushSettings) {
    let sd = &mut b.shape_dynamics;
    pct(ui, tl!("Size Jitter"), &mut sd.size.jitter, 1.0);
    control_row(ui, "brush-size-ctl", &mut sd.size, false);
    minimum_row(ui, tl!("Minimum Diameter"), &mut sd.size);
    // Tilt Scale applies with the size control on Pen Tilt (greyed out otherwise, like Photoshop).
    let tilt = sd.size.control == Control::PenTilt;
    ui.add_enabled_ui(tilt, |ui| pct(ui, tl!("Tilt Scale"), &mut sd.tilt_scale, 2.0));
    ui.add_space(craft::ui::theme::scale::Space::S6.px());
    dynamic(ui, "brush-angle-ctl", tl!("Angle Jitter"), &mut sd.angle, 1.0, None, true);
    dynamic(ui, "brush-round-ctl", tl!("Roundness Jitter"), &mut sd.roundness, 1.0, Some(tl!("Minimum Roundness")), false);
    ui.horizontal(|ui| {
        widgets::checkbox(ui, &mut sd.flip_x_jitter, tl!("Flip X Jitter"));
        ui.add_space(craft::ui::theme::scale::Space::S8.px());
        widgets::checkbox(ui, &mut sd.flip_y_jitter, tl!("Flip Y Jitter"));
    });
    ui.add_space(craft::ui::theme::scale::Space::S2.px());
    widgets::checkbox(ui, &mut sd.brush_projection, tl!("Brush Projection")).on_hover_text(tl!("Apply the pen's tilt and rotation to the tip shape"));
}

fn scattering(ui: &mut egui::Ui, b: &mut BrushSettings) {
    let sc = &mut b.scattering;
    pct(ui, tl!("Scatter"), &mut sc.scatter.jitter, 10.0);
    widgets::checkbox(ui, &mut sc.both_axes, tl!("Both Axes"));
    ui.add_space(craft::ui::theme::scale::Space::S2.px());
    control_row(ui, "brush-scatter-ctl", &mut sc.scatter, false);
    ui.add_space(craft::ui::theme::scale::Space::S6.px());
    let mut c = sc.count as f32;
    if num(ui, tl!("Count"), &mut c, 1.0..=16.0, "") {
        sc.count = c as u32;
    }
    dynamic(ui, "brush-count-ctl", tl!("Count Jitter"), &mut sc.count_jitter, 1.0, None, false);
}

/// The pattern swatch: the (adjusted) texture as the stroke sees it.
fn pattern_swatch(ui: &mut egui::Ui, tx: &paint::Texture) {
    let t = craft::ui::theme::current(ui.ctx());
    let n = 56u32;
    let (rect, _) = ui.allocate_exact_size(vec2(n as f32, n as f32), Sense::hover());
    let sig = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_vec(&(
            match &tx.pattern {
                Pattern::Procedural { style, size, seed } => (*style as u32, *size, *seed, 0),
                // A bitmap tile (a stored one's preview until it is loaded, #1843).
                p => p.bitmap().map_or((0, 0, 0, 0), |g| {
                    (g.width, g.height, g.data.len() as u32, g.data.iter().step_by((g.data.len() / 512).max(1)).map(|v| *v as u32).sum::<u32>())
                }),
            },
            tx.invert,
            tx.brightness,
            tx.contrast,
        ))
        .unwrap_or_default()
        .hash(&mut h);
        h.finish()
    };
    let tex = brush_preview::with_cache(ui.ctx(), |c| {
        c.get("texture-swatch", sig, || {
            let img = match &tx.pattern {
                Pattern::Procedural { style, size, seed } => paint::procedural::pattern(*style, (*size).clamp(4, 1024), *seed),
                p => match p.bitmap() {
                    Some(g) if g.is_valid() => paint::tile::PatternImage { width: g.width as usize, height: g.height as usize, data: g.to_f32() },
                    _ => paint::tile::PatternImage { width: 1, height: 1, data: vec![0.5] },
                },
            };
            let px: Vec<u8> = (0..n * n)
                .flat_map(|i| {
                    let x = tx.level(img.sample_wrap((i % n) as f32, (i / n) as f32));
                    let g = (x * 255.0).round() as u8;
                    [g, g, g, 255]
                })
                .collect();
            let img = egui::ColorImage::from_rgba_unmultiplied([n as usize, n as usize], &px);
            ui.ctx().load_texture("texture-swatch", img, egui::TextureOptions::LINEAR)
        })
    });
    ui.painter().image(tex.id(), rect, egui::Rect::from_min_max(egui::Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
    ui.painter().rect_stroke(
        rect,
        craft::ui::theme::scale::Radius::R0.px(),
        craft::ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.color.line.control_edge.to_egui()),
        egui::StrokeKind::Inside,
    );
}

fn texture(ui: &mut egui::Ui, b: &mut BrushSettings) {
    let tx = &mut b.texture;
    ui.horizontal(|ui| {
        pattern_swatch(ui, tx);
        ui.add_space(craft::ui::theme::scale::Space::S8.px());
        ui.vertical(|ui| {
            let mut style = match &tx.pattern {
                Pattern::Procedural { style, .. } => Some(*style),
                Pattern::Tile(_) | Pattern::Stored(_) => None,
            };
            let mut opts: Vec<(Option<PatternStyle>, &str)> = PATTERNS.iter().map(|(s, l)| (Some(*s), *l)).collect();
            if style.is_none() {
                opts.push((None, tl!("Custom")));
            }
            if widgets::dropdown(ui, "brush-pattern", tl!("Pattern"), &mut style, &opts, 110.0)
                && let Some(s) = style
            {
                let size = match &tx.pattern {
                    Pattern::Procedural { size, .. } => *size,
                    Pattern::Tile(_) | Pattern::Stored(_) => 128,
                };
                tx.pattern = Pattern::Procedural { style: s, size, seed: 1 };
            }
            ui.add_space(craft::ui::theme::scale::Space::S4.px());
            widgets::checkbox(ui, &mut tx.invert, tl!("Invert"));
        });
    });
    ui.add_space(craft::ui::theme::scale::Space::S6.px());
    if let Pattern::Procedural { size, .. } = &mut tx.pattern {
        let mut s = *size as f32;
        if num(ui, tl!("Pattern Size"), &mut s, 16.0..=1024.0, "px") {
            *size = s as u32;
        }
    }
    pct(ui, tl!("Scale"), &mut tx.scale, 10.0);
    let mut br = (tx.brightness * 150.0).round();
    if num(ui, tl!("Brightness"), &mut br, -150.0..=150.0, "") {
        tx.brightness = br / 150.0;
    }
    // Photoshop's -50..100 contrast onto the model's -1..1.
    let mut ct = if tx.contrast < 0.0 { tx.contrast * 50.0 } else { tx.contrast * 100.0 }.round();
    if num(ui, tl!("Contrast"), &mut ct, -50.0..=100.0, "") {
        tx.contrast = if ct < 0.0 { ct / 50.0 } else { ct / 100.0 };
    }
    widgets::checkbox(ui, &mut tx.each_tip, tl!("Texture Each Tip"));
    ui.add_space(craft::ui::theme::scale::Space::S4.px());
    ui.horizontal(|ui| {
        dim(ui, tl!("Mode:"));
        widgets::dropdown(ui, "brush-texture-mode", tl!("Mode"), &mut tx.mode, &MASK_MODES, 130.0);
    });
    ui.add_space(craft::ui::theme::scale::Space::S4.px());
    pct(ui, tl!("Depth"), &mut tx.depth, 1.0);
    let each = tx.each_tip;
    ui.add_enabled_ui(each, |ui| {
        pct(ui, tl!("Minimum Depth"), &mut tx.depth_jitter.minimum, 1.0);
        pct(ui, tl!("Depth Jitter"), &mut tx.depth_jitter.jitter, 1.0);
        control_row(ui, "brush-depth-ctl", &mut tx.depth_jitter, false);
    });
}

fn dual_brush(ui: &mut egui::Ui, b: &mut BrushSettings, presets: &[BrushPreset]) {
    let d = &mut b.dual_brush;
    ui.horizontal(|ui| {
        dim(ui, tl!("Mode:"));
        widgets::dropdown(ui, "brush-dual-mode", tl!("Mode"), &mut d.mode, &MASK_MODES, 120.0);
        ui.add_space(craft::ui::theme::scale::Space::S8.px());
        widgets::checkbox(ui, &mut d.flip, tl!("Flip"));
    });
    ui.add_space(craft::ui::theme::scale::Space::S4.px());
    // The dual tip is a sub-setting, not a selection: no persistent highlight (a look-alike match
    // would highlight an arbitrary duplicate).
    if let Some(p) = tip_grid(ui, "brush-dual-grid", presets, None).and_then(|i| presets.get(i)) {
        let (pt, ps, ph, pa, pr, psp) = preset_tip(p);
        d.tip = pt.clone();
        d.size = ps;
        d.hardness = ph;
        d.angle = pa;
        d.roundness = pr;
        d.spacing = psp;
    }
    let orig = d.tip.bitmap_size().map(|(w, h)| w.max(h) as f32);
    size_row(ui, tl!("Size"), &mut d.size, 2500.0, orig);
    signed_pct(ui, tl!("Spacing"), &mut d.spacing, 0.01, 10.0);
    pct(ui, tl!("Scatter"), &mut d.scatter, 10.0);
    widgets::checkbox(ui, &mut d.both_axes, tl!("Both Axes"));
    ui.add_space(craft::ui::theme::scale::Space::S4.px());
    let mut c = d.count as f32;
    if num(ui, tl!("Count"), &mut c, 1.0..=16.0, "") {
        d.count = c as u32;
    }
}

fn color_dynamics(ui: &mut egui::Ui, b: &mut BrushSettings) {
    let c = &mut b.color_dynamics;
    widgets::checkbox(ui, &mut c.per_tip, tl!("Apply Per Tip"));
    ui.add_space(craft::ui::theme::scale::Space::S6.px());
    dynamic(ui, "brush-fgbg-ctl", tl!("Foreground/Background Jitter"), &mut c.fg_bg, 1.0, None, false);
    pct(ui, tl!("Hue Jitter"), &mut c.hue_jitter, 1.0);
    pct(ui, tl!("Saturation Jitter"), &mut c.saturation_jitter, 1.0);
    pct(ui, tl!("Brightness Jitter"), &mut c.brightness_jitter, 1.0);
    signed_pct(ui, tl!("Purity"), &mut c.purity, -1.0, 1.0);
}

fn transfer(ui: &mut egui::Ui, b: &mut BrushSettings, tool: Tool) {
    let tr = &mut b.transfer;
    dynamic(ui, "brush-opacity-ctl", tl!("Opacity Jitter"), &mut tr.opacity, 1.0, Some(tl!("Minimum")), false);
    dynamic(ui, "brush-flow-ctl", tl!("Flow Jitter"), &mut tr.flow, 1.0, Some(tl!("Minimum")), false);
    // Wetness and mix only affect the Mixer Brush. Keep their settings visible and saved
    // for other tools, but disable the whole blocks, including controllers and minimums.
    ui.add_enabled_ui(tool == Tool::MixerBrush, |ui| {
        dynamic(ui, "brush-wet-ctl", tl!("Wetness Jitter"), &mut tr.wetness, 1.0, Some(tl!("Minimum")), false);
        dynamic(ui, "brush-mix-ctl", tl!("Mix Jitter"), &mut tr.mix, 1.0, Some(tl!("Minimum")), false);
    });
}

/// The Mixer Brush options the brush carries (Photoshop shows them in the Mixer Brush's options
/// bar; `paint.mixerBrush` uses them unless its params override them).
fn mixer_options(ui: &mut egui::Ui, b: &mut BrushSettings) {
    let t = craft::ui::theme::current(ui.ctx());
    ui.add_space(t.space.gap.stack);
    widgets::hairline(ui);
    ui.add_space(t.space.gap.stack);
    ui.label(RichText::new(tl!("Mixer Brush")).font(theme::semibold(11.5)).color(t.color.text.muted.to_egui()));
    ui.add_space(craft::ui::theme::scale::Space::S2.px());
    let m = &mut b.mixer;
    pct(ui, tl!("Wet"), &mut m.wet, 1.0);
    pct(ui, tl!("Load"), &mut m.load, 1.0);
    pct(ui, tl!("Mix"), &mut m.mix, 1.0);
    pct(ui, tl!("Flow"), &mut m.flow, 1.0);
    widgets::checkbox(ui, &mut m.sample_all_layers, "Sample All Layers");
    widgets::checkbox(ui, &mut m.load_after_stroke, tl!("Load the brush after each stroke"));
    widgets::checkbox(ui, &mut m.clean_after_stroke, tl!("Clean the brush after each stroke"));
}

/// Tilt is stored in degrees (±90, W3C Pointer Events); Photoshop shows it as ±100 %.
fn tilt(ui: &mut egui::Ui, label: &str, deg: &mut f32) {
    let mut v = (*deg / 90.0).clamp(-1.0, 1.0);
    if signed_pct(ui, label, &mut v, -1.0, 1.0) {
        *deg = v * 90.0;
    }
}

fn pose(ui: &mut egui::Ui, b: &mut BrushSettings) {
    let p = &mut b.pose;
    tilt(ui, tl!("Tilt X"), &mut p.tilt_x);
    widgets::checkbox(ui, &mut p.override_tilt_x, tl!("Override Tilt X"));
    ui.add_space(craft::ui::theme::scale::Space::S8.px());
    tilt(ui, tl!("Tilt Y"), &mut p.tilt_y);
    widgets::checkbox(ui, &mut p.override_tilt_y, tl!("Override Tilt Y"));
    ui.add_space(craft::ui::theme::scale::Space::S8.px());
    num(ui, tl!("Rotation"), &mut p.rotation, 0.0..=360.0, "°");
    widgets::checkbox(ui, &mut p.override_rotation, tl!("Override Rotation"));
    ui.add_space(craft::ui::theme::scale::Space::S8.px());
    pct(ui, tl!("Pressure"), &mut p.pressure, 1.0);
    widgets::checkbox(ui, &mut p.override_pressure, tl!("Override Pressure"));
}

fn smoothing(ui: &mut egui::Ui, b: &mut BrushSettings) {
    let s = &mut b.smoothing;
    pct(ui, tl!("Smoothing"), &mut s.amount, 1.0);
    widgets::checkbox(ui, &mut s.pulled_string, tl!("Pulled String Mode"));
    widgets::checkbox(ui, &mut s.catch_up, tl!("Stroke Catch-up"));
    widgets::checkbox(ui, &mut s.catch_up_on_end, tl!("Catch-up on Stroke End"));
    widgets::checkbox(ui, &mut s.adjust_for_zoom, tl!("Adjust for Zoom"));
}

/// Controls for section `i` (see [`crate::brush_panel::SECTIONS`]). `presets` feed the tip grids.
/// Returns the name of a preset whose tip was picked in the Brush Tip Shape grid, for
/// `brush.presets.setCurrent`.
/// The active `tool` determines which Transfer controls apply.
pub fn section_body(ui: &mut egui::Ui, b: &mut BrushSettings, i: usize, presets: &[BrushPreset], current: Option<&str>, tool: Tool) -> Option<String> {
    let t = craft::ui::theme::current(ui.ctx());
    let note = |ui: &mut egui::Ui, s: &str| {
        ui.label(RichText::new(s).color(t.color.text.faint.to_egui()).font(theme::medium(11.5)));
    };
    if i == 0 {
        return tip_shape(ui, b, presets, current);
    }
    match i {
        1 => shape_dynamics(ui, b),
        2 => scattering(ui, b),
        3 => texture(ui, b),
        4 => dual_brush(ui, b, presets),
        5 => color_dynamics(ui, b),
        6 => {
            transfer(ui, b, tool);
            mixer_options(ui, b);
        }
        7 => pose(ui, b),
        8 => note(ui, "Adds extra randomness to the soft edges of the brush tip. No options: turn it on in the list."),
        9 => note(ui, "Builds up paint along the edges of the stroke for a watercolour look. No options: turn it on in the list."),
        10 => {
            note(ui, tl!("Airbrush: keeps depositing paint while the pointer rests."));
            ui.add_space(craft::ui::theme::scale::Space::S6.px());
            num(ui, tl!("Rate"), &mut b.build_up_rate, 1.0..=200.0, "/s");
        }
        11 => smoothing(ui, b),
        _ => note(ui, "Keeps the current pattern and scale when you switch to another textured brush preset."),
    }
    None
}

#[cfg(test)]
mod tests;
