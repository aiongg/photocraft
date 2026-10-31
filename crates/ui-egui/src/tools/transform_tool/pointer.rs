use super::begin::moving_layers;
use super::commit::{commit, committing, moves_type};
use super::drag_modes::handle_indicator;
use super::warp::{warp_cursor, warp_pointer};
use super::{CursorIcon, Homography, LayerContent, PhotocraftApp, ToolEvent, TransformMode, TransformPreview, TransformSession, ViewXform, handle_tolerance};
use craft::draw::ui::{BoxPress, BoxRelease, TransformBox, TransformNames, TransformStyle};
use craft::ui::canvas::quad::{BoxAllow, BoxPart, BoxRules, BoxState, ScaleRule, TransformMode as BoxMode};
use craft::ui::canvas::{CanvasView, kurbo::Point, kurbo::Vec2, resize_cursor};
use egui::{Modifiers, Pos2, pos2};
use std::sync::OnceLock;

/// Which part of the box a document point hits.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Hit {
    Corner(usize),
    Edge(usize),
    Pivot,
    Inside,
    Outside,
}

impl From<BoxPart> for Hit {
    fn from(part: BoxPart) -> Self {
        match part {
            BoxPart::Corner(i) => Hit::Corner(usize::from(i)),
            BoxPart::Edge(i) => Hit::Edge(usize::from(i)),
            BoxPart::Pivot => Hit::Pivot,
            BoxPart::Inside => Hit::Inside,
            BoxPart::Outside => Hit::Outside,
        }
    }
}

impl From<Hit> for BoxPart {
    fn from(hit: Hit) -> Self {
        match hit {
            Hit::Corner(i) => BoxPart::Corner(i as u8),
            Hit::Edge(i) => BoxPart::Edge(i as u8),
            Hit::Pivot => BoxPart::Pivot,
            Hit::Inside => BoxPart::Inside,
            Hit::Outside => BoxPart::Outside,
        }
    }
}

/// The library box's sizes (`transform.*`): the same in every theme, so the pointer reads them
/// without a context.
pub(crate) fn reach() -> &'static TransformStyle {
    static STYLE: OnceLock<TransformStyle> = OnceLock::new();
    STYLE.get_or_init(|| TransformBox::style(&craft::ui::theme::current(&egui::Context::default())))
}

/// The library's canvas points for document points: `zoom` canvas points per document pixel, no
/// turn, flip or stretch (the library's reach is in points, PhotoCraft measures it in document
/// pixels at the zoom).
pub(crate) fn point_view(zoom: f64) -> CanvasView {
    CanvasView::new(egui::Rect::from_center_size(Pos2::ZERO, egui::vec2(1e7, 1e7)), Point::ZERO, zoom)
}

fn canvas_point(p: [f64; 2], zoom: f64) -> Pos2 {
    pos2((p[0] * zoom) as f32, (p[1] * zoom) as f32)
}

pub(super) fn zoom_of(app: &PhotocraftApp) -> f64 {
    f64::from(app.point_zoom().max(0.01))
}

/// The library's box state of the session.
pub(super) fn box_state(t: &TransformSession) -> BoxState {
    let p = |q: [f64; 2]| Point::new(q[0], q[1]);
    BoxState { quad: t.quad.map(p), pivot: p(t.pivot) }
}

pub(super) fn apply_state(t: &mut TransformSession, s: &BoxState) {
    t.quad = s.quad.map(|p| [p.x, p.y]);
    t.pivot = [s.pivot.x, s.pivot.y];
}

pub(super) fn box_mode(mode: TransformMode) -> BoxMode {
    match mode {
        TransformMode::Free => BoxMode::Free,
        TransformMode::Rotate => BoxMode::Rotate,
        TransformMode::Skew => BoxMode::Skew,
        TransformMode::Distort => BoxMode::Distort,
        TransformMode::Perspective => BoxMode::Perspective,
    }
}

/// How drags change the session's box: its Edit › Transform mode, the preferences for legacy
/// scaling and the aspect link, and what the content allows (editable type refuses distort;
/// Content-Aware Scale only scales and moves).
pub(super) fn box_rules(app: &PhotocraftApp, t: &TransformSession) -> BoxRules {
    let legacy = app.session.prefs().general.use_legacy_free_transform;
    let scales_only = t.content_aware.is_some();
    BoxRules {
        mode: box_mode(t.mode),
        scale: if legacy { ScaleRule::Free } else { ScaleRule::Proportional },
        aspect_linked: !app.ui.transform_unlinked,
        allow: BoxAllow { rotate: !scales_only, skew: !scales_only, distort: !(scales_only || moves_type(app, t)) },
        rotate_step: 15.0,
    }
}

/// Preferences › Tools › Snap Vector Tools and Transforms to Pixel Grid (#5751): while a shape
/// layer or a path is moved or scaled, its edges stay on whole pixels, whatever the tool. The box
/// rounds only while still an upright rectangle.
fn snaps_to_pixels(app: &PhotocraftApp, t: &TransformSession) -> bool {
    if !app.session.prefs().tools.snap_vector_tools_and_transforms_to_pixel_grid
        || t.selection
        || t.warp.is_some()
        || t.content_aware.is_some()
        || t.target.is_some()
    {
        return false;
    }
    let shapes =
        |st: &photocraft_engine::DocState| moving_layers(t).iter().all(|id| st.doc.layer(*id).is_some_and(|l| matches!(l.content, LayerContent::Shape(_))));
    t.path.is_some() || app.session.active().is_some_and(shapes)
}

fn library_box<'a>(state: &'a BoxState, rules: BoxRules, pixel_grid: bool) -> TransformBox<'a> {
    TransformBox::new("photocraft.transform", state, TransformNames::default()).rules(rules).pixel_grid(pixel_grid)
}

/// The nearest handle within the library's reach at `zoom`, else inside or outside the box:
/// the geometry alone, whatever the mode refuses.
fn hit_at(t: &TransformSession, p: [f64; 2], zoom: f64) -> Hit {
    let state = box_state(t);
    library_box(&state, BoxRules::default(), false).hit(&point_view(zoom), reach(), canvas_point(p, zoom), Modifiers::NONE).map_or(Hit::Outside, Hit::from)
}

/// [`hit_at`] for a grab radius of `tol` document pixels.
pub(crate) fn hit(t: &TransformSession, p: [f64; 2], tol: f64) -> Hit {
    hit_at(t, p, f64::from(reach().hit) / tol.max(1e-9))
}

/// The drag the Move, Camera and Type transform boxes keep, which they run through [`apply_drag`].
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Gesture {
    pub(crate) hit: Hit,
    pub(crate) start: [f64; 2],
    pub(crate) quad0: [[f64; 2]; 4],
    pub(crate) pivot0: [f64; 2],
}

/// `g`'s drag of a box to document point `p` with `mods` held, by the library's `rules`.
pub(crate) fn apply_drag(s: &mut TransformSession, g: Gesture, p: [f64; 2], mods: Modifiers, rules: &BoxRules) {
    let point = |q: [f64; 2]| Point::new(q[0], q[1]);
    let start = BoxState { quad: g.quad0.map(point), pivot: point(g.pivot0) };
    let next = craft::ui::canvas::quad::drag(&start, &box_state(s), g.hit.into(), point(g.start), point(p), mods, rules);
    apply_state(s, &next);
}

/// Transient drag state of the Free Transform box (kept in egui memory; not part of the serialised
/// session).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Held {
    pub(crate) press: BoxPress,
    pub(crate) quad0: [[f64; 2]; 4],
    pub(crate) pivot0: [f64; 2],
}

impl Held {
    pub(crate) fn hit(&self) -> Hit {
        self.press.part().into()
    }
}

/// Pointer input while transforming. Returns false when no transform is active.
pub fn pointer(app: &mut PhotocraftApp, ev: ToolEvent, mods: egui::Modifiers) -> bool {
    let Some(t) = app.ui.transform.clone() else { return false };
    if committing(app).is_some() {
        return true;
    }
    let zoom = zoom_of(app);
    if t.warp.is_some() {
        warp_pointer(app, ev, zoom, mods);
        return true;
    }
    if app.transform_preview.is_none() {
        return false;
    }
    let (state, rules, rounds) = (box_state(&t), box_rules(app, &t), snaps_to_pixels(app, &t));
    let (view, style) = (point_view(zoom), reach());
    let tool = library_box(&state, rules, rounds);
    match ev {
        ToolEvent::Down { x, y, .. } => {
            let at = canvas_point([x, y], zoom);
            let press = tool.press(&view, style, at, mods);
            // ⌥-click away from the handles moves the reference point there (and drags it): shown
            // at the press, not only once the drag begins.
            if let Some(press) = &press
                && mods.alt
                && press.part() == BoxPart::Pivot
                && tool.hit(&view, style, at, Modifiers { alt: false, ..mods }) != Some(BoxPart::Pivot)
                && let Some(s) = app.ui.transform.as_mut()
            {
                s.pivot = [x, y];
            }
            if let Some(pv) = app.transform_preview.as_mut() {
                pv.gesture = press.map(|press| Held { press, quad0: t.quad, pivot0: t.pivot });
            }
        }
        ToolEvent::Move { x, y, .. } | ToolEvent::Up { x, y } => {
            let up = matches!(ev, ToolEvent::Up { .. });
            let Some(mut g) = app.transform_preview.as_mut().and_then(|pv| if up { pv.gesture.take() } else { pv.gesture }) else {
                crate::snap_ui::retain_transform_lines(app, mods);
                return true;
            };
            let at = canvas_point([x, y], zoom);
            // The library begins the drag on the first move (outside, past its click slop).
            if let Some(next) = tool.track(&view, style, &mut g.press, at, mods).state
                && let Some(s) = app.ui.transform.as_mut()
            {
                apply_state(s, &next);
            }
            if up {
                // Clicking an empty spot commits instead of silently arming rotation (#2851);
                // Option-clicking moves the pivot.
                match tool.release(style, &g.press, Some(at), false) {
                    BoxRelease::Apply => {
                        commit(app);
                        return true;
                    }
                    BoxRelease::Pivot(next) => {
                        if let Some(s) = app.ui.transform.as_mut() {
                            apply_state(s, &next);
                        }
                    }
                    BoxRelease::Commit(_) | BoxRelease::Nothing => {}
                }
            } else if let Some(pv) = app.transform_preview.as_mut() {
                pv.gesture = Some(g);
            }
        }
    }
    crate::snap_ui::retain_transform_lines(app, mods);
    true
}

fn anchor_uv(anchor: usize) -> (f64, f64) {
    match anchor {
        0 => (0.0, 0.0),
        1 => (0.5, 0.0),
        2 => (1.0, 0.0),
        3 => (0.0, 0.5),
        4 => (0.5, 0.5),
        5 => (1.0, 0.5),
        6 => (0.0, 1.0),
        7 => (0.5, 1.0),
        8 => (1.0, 1.0),
        _ => (0.5, 0.5),
    }
}

pub(super) fn point_on_quad(quad: [[f64; 2]; 4], anchor: usize) -> [f64; 2] {
    let (u, v) = anchor_uv(anchor);
    if let Some(h) = Homography::rect_to_quad([0.0, 0.0, 1.0, 1.0], quad) {
        let (x, y) = h.apply(u, v);
        if x.is_finite() && y.is_finite() {
            return [x, y];
        }
    }
    match anchor {
        0 => quad[0],
        1 => [(quad[0][0] + quad[1][0]) / 2.0, (quad[0][1] + quad[1][1]) / 2.0],
        2 => quad[1],
        3 => [(quad[0][0] + quad[3][0]) / 2.0, (quad[0][1] + quad[3][1]) / 2.0],
        4 => [(quad[0][0] + quad[1][0] + quad[2][0] + quad[3][0]) / 4.0, (quad[0][1] + quad[1][1] + quad[2][1] + quad[3][1]) / 4.0],
        5 => [(quad[1][0] + quad[2][0]) / 2.0, (quad[1][1] + quad[2][1]) / 2.0],
        6 => quad[3],
        7 => [(quad[2][0] + quad[3][0]) / 2.0, (quad[2][1] + quad[3][1]) / 2.0],
        8 => quad[2],
        _ => [(quad[0][0] + quad[1][0] + quad[2][0] + quad[3][0]) / 4.0, (quad[0][1] + quad[1][1] + quad[2][1] + quad[3][1]) / 4.0],
    }
}

/// True just outside a corner: rotate handle feedback lives here, not over the entire
/// empty canvas. The actual rotation gesture continues to work farther away.
pub(super) fn near_rotate_corner(t: &TransformSession, p: [f64; 2], tol: f64) -> bool {
    t.warp.is_none()
        && t.mode != TransformMode::Distort
        && {
            let h = hit(t, p, tol);
            h == Hit::Outside || (t.mode == TransformMode::Rotate && matches!(h, Hit::Corner(_) | Hit::Edge(_)))
        }
        && t.quad.iter().any(|q| (p[0] - q[0]).hypot(p[1] - q[1]) <= tol * f64::from(reach().rotate_reach / reach().hit))
}

/// Clockwise degrees in canvas coordinates (positive Y points down).
pub(super) fn rotation_degrees(quad: [[f64; 2]; 4]) -> f64 {
    let dx = quad[1][0] - quad[0][0];
    let dy = quad[1][1] - quad[0][1];
    if dx.is_finite() && dy.is_finite() && dx.hypot(dy) > 1e-9 { dy.atan2(dx).to_degrees() } else { 0.0 }
}

/// Where the rotate cursor's arc bends at document point `p`, or `None` outside the rotate zone:
/// just outside a corner, or anywhere while a rotate drag is on. It points into the box along the
/// bisector of the nearest corner, so each corner gets its own arrow and they turn with the box.
pub(crate) fn rotate_cursor_dir(app: &PhotocraftApp, p: [f64; 2]) -> Option<[f64; 2]> {
    rotate_dir(app.ui.transform.as_ref()?, app.transform_preview.as_ref(), p, handle_tolerance(app))
}

pub(super) fn rotate_dir(t: &TransformSession, pv: Option<&TransformPreview>, p: [f64; 2], tol: f64) -> Option<[f64; 2]> {
    // Content-Aware Scale's box only scales, and Skew doesn't rotate: no rotate arrow.
    if !(p[0].is_finite() && p[1].is_finite())
        || t.warp.is_some()
        || t.content_aware.is_some()
        || matches!(t.mode, TransformMode::Distort | TransformMode::Skew)
    {
        return None;
    }
    // During a drag the arrow shows only while it rotates: not when a handle that scales, skews
    // or distorts strays past a corner (#5405).
    match pv.and_then(|pv| pv.gesture) {
        Some(g) if g.hit() == Hit::Outside || (t.mode == TransformMode::Rotate && matches!(g.hit(), Hit::Corner(_) | Hit::Edge(_))) => {}
        Some(_) => return None,
        None if !near_rotate_corner(t, p, tol) => return None,
        None => {}
    }
    corner_bisector(&t.quad, p)
}

/// The unit direction into the box along the bisector of the corner nearest document point `p`:
/// where the library's turn cursor bends at `p`.
pub(crate) fn corner_bisector(quad: &[[f64; 2]; 4], p: [f64; 2]) -> Option<[f64; 2]> {
    let dist = |q: [f64; 2]| (p[0] - q[0]).hypot(p[1] - q[1]);
    let i = (0..4).min_by(|&a, &b| dist(quad[a]).total_cmp(&dist(quad[b])))?;
    let unit = |to: [f64; 2]| {
        let d = [to[0] - quad[i][0], to[1] - quad[i][1]];
        let l = d[0].hypot(d[1]);
        (l.is_finite() && l > 1e-9).then(|| [d[0] / l, d[1] / l])
    };
    let (a, b) = (unit(quad[(i + 1) % 4])?, unit(quad[(i + 3) % 4])?);
    let s = [a[0] + b[0], a[1] + b[1]];
    let l = s[0].hypot(s[1]);
    (l.is_finite() && l > 1e-9).then(|| [s[0] / l, s[1] / l])
}

/// The handle a drag in progress holds, else the one under document point `p`.
pub(super) fn handle_at(app: &PhotocraftApp, t: &TransformSession, p: [f64; 2], tol: f64) -> Hit {
    app.transform_preview.as_ref().and_then(|pv| pv.gesture.map(|g| g.hit())).unwrap_or_else(|| hit(t, p, tol))
}

/// Cursor for hovering a document point while transforming. `xf` is the canvas's view;
/// `mods` provides modifier keys (Command for Distort on corners / Skew on edges; Alt/Command for quick split in warp).
pub fn cursor(app: &PhotocraftApp, xf: &ViewXform, p: [f64; 2], mods: egui::Modifiers) -> Option<CursorIcon> {
    let t = app.ui.transform.as_ref()?;
    let tol = handle_tolerance(app);
    if let Some(w) = &t.warp {
        return Some(warp_cursor(app, w, xf, p, mods));
    }
    let h = handle_at(app, t, p, tol);
    if !box_rules(app, t).allows(h.into()) || (t.content_aware.is_some() && h == Hit::Outside) {
        return Some(CursorIcon::Default);
    }
    // A ⌘-drag outside the box still rotates, so ⌘ keeps the curved arrow there.
    if rotate_cursor_dir(app, p).is_some() {
        // The canvas draws the curved rotate arrow (`rotate_cursor_dir`), also during the drag.
        return Some(CursorIcon::None);
    }
    // The canvas draws the distort or skew pointer in place of the hidden one
    // (`draw_handle_cursor`); not while the box is being applied, when it draws no handles.
    let indicator = handle_indicator(app, t, h, mods).filter(|_| committing(app).is_none());
    // The box's corners run clockwise from the top-left; edge k joins corners k and k + 1.
    let corner = |k: usize| t.quad[k % 4];
    let edge_mid = |k: usize| {
        let (a, b) = (corner(k), corner(k + 1));
        [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
    };
    // A handle resizes the box away from the opposite handle.
    let resize = |from: [f64; 2], to: [f64; 2]| resize_cursor(&xf.canvas_view(), Vec2::new(to[0] - from[0], to[1] - from[1]));
    Some(match h {
        Hit::Corner(_) | Hit::Edge(_) if indicator.is_some() => CursorIcon::None,
        Hit::Corner(i) => crate::tool_cursor::corner_resize(&t.quad, i, xf),
        Hit::Edge(i) => resize(edge_mid(i + 2), edge_mid(i)),
        Hit::Pivot => CursorIcon::Crosshair,
        Hit::Inside => CursorIcon::Move,
        Hit::Outside => CursorIcon::Alias,
    })
}
