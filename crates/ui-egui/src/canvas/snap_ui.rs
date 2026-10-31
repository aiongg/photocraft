//! Snapping in the shell: builds [`photocraft_engine::snap`] targets from View › Snap To and the
//! document, snaps tool gestures (Move tool, marquees, crop, shapes, pen anchors, Free Transform
//! handles and guides), and draws smart guides (magenta alignment lines) while layers move.
//!
//! Snapping is on when View › Snap (⇧⌘;) is; holding Ctrl while dragging turns it off, except when
//! Ctrl is the corner-distortion modifier. Smart guides follow View › Show › Smart Guides and also snap the Move
//! tool to other layers' edges and centres. The threshold is 8 screen pixels at the current zoom.

use craft::draw::ui::CropBox;
use craft::ui::canvas::kurbo::Vec2;
use craft::ui::canvas::quad::BoxPart;
use egui::Color32;
use photocraft_doc::LayerId;
use photocraft_engine::snap::{SnapKind, SnapLine, SnapOptions, SnapTargets, layer_rect, union};

use crate::PhotocraftApp;
use crate::canvas::{ToolEvent, ViewXform};
use crate::state::Tool;

/// Snap distance in screen pixels.
pub const SNAP_PX: f64 = 8.0;

/// What the current drag snaps.
#[derive(Clone, Debug, PartialEq)]
pub enum Gesture {
    /// A point (marquee corner, crop or shape corner, pen anchor, transform handle).
    Point,
    /// A transform corner: distortion snaps the corner itself, keeping the handle grab offset.
    TransformCorner { index: usize, point: [f64; 2] },
    /// Layers being moved: their bounds at the start of the drag.
    Move { rect: [f64; 4] },
    /// Free Transform box dragged from inside: its bounds at the start of the drag.
    TransformMove { rect: [f64; 4] },
    /// A selection outline (or its floating piece) dragged with a selection tool: its bounds at
    /// the start of the drag.
    SelectionMove { rect: [f64; 4] },
    /// A marquee, Object Selection box or shape being drawn, repositioned with Space held
    /// (`hold_keys`): its bounds when Space went down.
    DrawMove { rect: [f64; 4] },
    /// The Crop tool's drag: the library's frame asks [`CropSnap`] for the vector that snaps the
    /// part it drags, so the pointer itself is left as it is.
    Crop,
}

/// Snapping state of the drag in progress.
#[derive(Clone, Debug)]
pub struct ActiveSnap {
    pub gesture: Gesture,
    pub start: [f64; 2],
    pub targets: SnapTargets,
    /// Layer alignments only (smart guides), when View › Snap and Smart Guides are on.
    pub smart: SnapTargets,
    pub disabled: bool,
}

/// Snap To options from the View menu and the Guides/Grid preferences.
pub fn options(app: &PhotocraftApp) -> SnapOptions {
    let st = &app.ui.view.snap_to;
    let e = &app.ui.extras;
    let grid_step = app.session.active().map_or([18.0; 2], |d| {
        let g = &app.session.prefs().guides_grid_and_slices;
        let ppi = app.session.prefs().units_and_rulers.point_size.per_inch();
        crate::rulers::grid_major_px(g, d.doc.resolution_dpi.max(1.0) as f64, [d.doc.size.width as f64, d.doc.size.height as f64], ppi)
            .map(|step| step / g.subdivisions.max(1) as f64)
    });
    SnapOptions {
        guides: st.guides && e.guides && app.ui.view.shows(app.ui.view.show.canvas_guides),
        grid: st.grid && e.grid,
        layers: st.layers,
        document: st.document_bounds,
        selection: true,
        grid_step,
    }
}

/// Snap tolerance in document pixels at the current zoom.
pub fn tolerance(app: &PhotocraftApp) -> f64 {
    SNAP_PX / app.point_zoom().max(0.01) as f64
}

fn smart_on(app: &PhotocraftApp) -> bool {
    app.ui.view.shows(app.ui.view.show.smart_guides)
}

fn snap_on(app: &PhotocraftApp) -> bool {
    app.ui.extras.snap
}

/// Targets of the active document (smart = layer alignments only).
fn build(app: &PhotocraftApp, exclude: &[LayerId], smart: bool) -> SnapTargets {
    let Some(st) = app.session.active() else { return SnapTargets::default() };
    let opts =
        if smart { SnapOptions { guides: false, grid: false, layers: true, document: true, selection: false, grid_step: [0.0; 2] } } else { options(app) };
    let t = SnapTargets::from_document(&st.doc, &opts, exclude);
    if smart { t.filtered(SnapKind::is_smart) } else { t }
}

/// What a move with `tool` in effect drags: the selection's bounds where its pixels float
/// (`move_ui`), else the union of the selected layers' bounds.
fn moving_rect(app: &PhotocraftApp, tool: Tool) -> Option<[f64; 4]> {
    let st = app.session.active()?;
    if crate::move_ui::moves_selected_pixels_with(app, tool) {
        return selection_rect(app);
    }
    union(st.selected_layers().into_iter().filter_map(|id| layer_rect(&st.doc, id)))
}

/// The bounds of the pixels a selection moves, where they are shown: its floating piece's outline
/// at its offset (#7538), else (nothing cut yet, or a mask or empty piece) the selection's bounds.
fn selection_rect(app: &PhotocraftApp) -> Option<[f64; 4]> {
    let st = app.session.active()?;
    let sel = st.doc.selection.as_ref()?;
    let r = match photocraft_engine::float_cmds::floating(st) {
        Some(f) => f.parts.outline().unwrap_or(sel).content_bounds().translate(f.offset.0, f.offset.1),
        None => sel.content_bounds(),
    };
    (!r.is_empty()).then(|| [r.x0, r.y0, r.x1, r.y1].map(f64::from))
}

/// A press cut the selected pixels after snapping began (`filter_event` runs before the tools):
/// the move now snaps the cut piece's outline, not the selection it was cut with (#7538).
pub(crate) fn follow_cut(app: &mut PhotocraftApp) {
    let Some(rect) = selection_rect(app) else { return };
    if let Some(snap) = app.prefs_rt.snap.as_mut() {
        match &mut snap.gesture {
            Gesture::Move { rect: r } | Gesture::SelectionMove { rect: r } => *r = rect,
            _ => {}
        }
    }
}

fn is_point_tool(t: Tool) -> bool {
    matches!(
        t,
        Tool::RectMarquee
            | Tool::EllipseMarquee
            | Tool::SingleRowMarquee
            | Tool::SingleColumnMarquee
            | Tool::Crop
            | Tool::ObjectSelection
            | Tool::Pen
            | Tool::CurvaturePen
            | Tool::Rectangle
            | Tool::EllipseShape
            | Tool::Triangle
            | Tool::Polygon
            | Tool::Line
            | Tool::Type
            | Tool::VerticalType
            | Tool::HorizontalTypeMask
            | Tool::VerticalTypeMask
    )
}

fn quad_rect(q: &[[f64; 2]; 4]) -> [f64; 4] {
    let xs = q.iter().map(|p| p[0]);
    let ys = q.iter().map(|p| p[1]);
    [xs.clone().fold(f64::MAX, f64::min), ys.clone().fold(f64::MAX, f64::min), xs.fold(f64::MIN, f64::max), ys.fold(f64::MIN, f64::max)]
}

/// Is `p` inside the quad (even-odd)?
fn in_quad(q: &[[f64; 2]; 4], p: [f64; 2]) -> bool {
    let mut inside = false;
    for i in 0..4 {
        let (a, b) = (q[i], q[(i + 3) % 4]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
    }
    inside
}

/// Near a Free Transform corner or edge handle (within its grab radius)?
fn near_handle(q: &[[f64; 2]; 4], p: [f64; 2], tol: f64) -> bool {
    (0..4).any(|i| {
        let (a, b) = (q[i], q[(i + 1) % 4]);
        let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        [a, mid].iter().any(|h| (h[0] - p[0]).hypot(h[1] - p[1]) <= tol)
    })
}

/// Photoshop: holding Ctrl (Windows) / Control (macOS) *during* a drag temporarily turns snapping
/// off. Only the held state while dragging counts: Ctrl at the press belongs to the tool (Ctrl-click
/// auto-selects with the Move tool), and ⌘ on the Mac never overrides snapping.
fn override_held(mods: egui::Modifiers) -> bool {
    mods.ctrl && !mods.mac_cmd
}

fn corner_distorts(app: &PhotocraftApp, gesture: &Gesture, mods: egui::Modifiers) -> bool {
    matches!(gesture, Gesture::TransformCorner { index, .. } if crate::transform_tool::corner_distorts(app, *index, mods))
}

fn override_for_gesture(app: &PhotocraftApp, mods: egui::Modifiers) -> bool {
    override_held(mods) && !(mods.command && app.prefs_rt.snap.as_ref().is_some_and(|s| corner_distorts(app, &s.gesture, mods)))
}

/// The Move tool's press at `p` picked other layers (Auto-Select): the drag moves those, so its
/// box, what it snaps and what it leaves out of the targets are theirs, not the ones selected
/// before (a group or an empty layer selected before had no box at all).
pub(crate) fn retarget_move(app: &mut PhotocraftApp, p: [f64; 2], mods: egui::Modifiers) {
    begin(app, p, mods);
}

/// Start snapping for a drag beginning at `p` with `mods` held (called on pointer down).
fn begin(app: &mut PhotocraftApp, p: [f64; 2], mods: egui::Modifiers) {
    app.prefs_rt.snap = None;
    app.prefs_rt.snap_lines.clear();
    // Puppet Warp's pins and Perspective Warp's quads go where they are dragged, as in Photoshop:
    // the press is theirs, not the tool's (with the Move tool it took a layer move's Smart Guides).
    if app.session.active().is_none() || app.distort.active() {
        return;
    }
    // The tool the press acts as: ⌘ makes a painting tool the Move tool, and a selection tool
    // outside the selection moves the layer as the Move tool does (#2681).
    let tool = crate::canvas::event_tool(app, mods);
    let moves_layer = crate::canvas::command_moves_layer(app, tool, p, mods);
    let gesture = if let Some(t) = &app.ui.transform {
        let corner = match crate::transform_tool::hit(t, p, crate::transform_tool::handle_tolerance(app)) {
            crate::transform_tool::Hit::Corner(index) => {
                t.quad.get(index).map(|&point| (Gesture::TransformCorner { index, point }, crate::transform_tool::moving_layers(t)))
            }
            _ => None,
        };
        if t.mode == crate::state::TransformMode::Distort {
            corner
        } else if near_handle(&t.quad, p, crate::transform_tool::handle_tolerance(app)) {
            corner.or_else(|| Some((Gesture::Point, crate::transform_tool::moving_layers(t))))
        } else if in_quad(&t.quad, p) {
            Some((Gesture::TransformMove { rect: quad_rect(&t.quad) }, crate::transform_tool::moving_layers(t)))
        } else {
            None
        }
    } else if tool == Tool::Move || moves_layer {
        let exclude = app.session.active().map(|s| s.selected_layers()).unwrap_or_default();
        moving_rect(app, tool).map(|rect| (Gesture::Move { rect }, exclude))
    } else if tool == Tool::Crop {
        Some((Gesture::Crop, Vec::new()))
    } else if let Some(cut) = crate::canvas::selection_drag_kind(app, tool, p, mods) {
        // A marquee or lasso inside the selection moves the outline (⌘: cuts its pixels into a
        // floating piece): its bounds snap, as the Move tool's do, not the pointer. A cut piece
        // doesn't snap to its own layer.
        let exclude = if cut { app.session.active().map(|s| s.selected_layers()).unwrap_or_default() } else { Vec::new() };
        selection_rect(app).map(|rect| (Gesture::SelectionMove { rect }, exclude))
    } else if tool.is_type()
        && let Some(layer) = app.ui.text_edit.as_ref().map(|e| LayerId(e.layer))
    {
        // Editing type: a paragraph-box handle drag snaps to guides and other layers, not to the
        // text being resized; a press in the text places the caret where it is (#6819).
        crate::type_tool::box_handle_at(app, layer, p[0], p[1]).map(|_| (Gesture::Point, vec![layer]))
    } else if is_point_tool(tool) {
        Some((Gesture::Point, Vec::new()))
    } else {
        None
    };
    let Some((gesture, exclude)) = gesture else { return };
    let mut targets = if snap_on(app) { build(app, &exclude, false) } else { SnapTargets::default() };
    if app.ui.transform.as_ref().is_some_and(|t| t.mode == crate::state::TransformMode::Distort) {
        // Explicit Distort fits corners to guides; preserve free placement near layer edges.
        targets = targets.filtered(|k| k == SnapKind::Guide);
    }
    if matches!(gesture, Gesture::SelectionMove { .. }) || (matches!(gesture, Gesture::Move { .. }) && crate::move_ui::moves_selected_pixels_with(app, tool)) {
        // A moving selection never snaps back to where it started.
        targets = targets.filtered(|k| k != SnapKind::Selection);
    }
    let smart = if snap_on(app) && matches!(gesture, Gesture::Move { .. } | Gesture::Crop) && smart_on(app) {
        build(app, &exclude, true)
    } else {
        SnapTargets::default()
    };
    app.prefs_rt.snap = Some(ActiveSnap { gesture, start: p, targets, smart, disabled: false });
}

/// Round to whole pixels when Preferences › Tools asks vector tools and transforms to snap to
/// the pixel grid.
fn pixel_round(app: &PhotocraftApp, p: [f64; 2]) -> [f64; 2] {
    if app.session.prefs().tools.snap_vector_tools_and_transforms_to_pixel_grid
        && matches!(app.ui.tool, Tool::Pen | Tool::CurvaturePen | Tool::Rectangle | Tool::EllipseShape | Tool::Triangle | Tool::Polygon | Tool::Line)
    {
        [p[0].round(), p[1].round()]
    } else {
        p
    }
}

/// Snap one pointer position for the active gesture; records the alignment lines.
fn apply(app: &mut PhotocraftApp, p: [f64; 2], mods: egui::Modifiers) -> [f64; 2] {
    let tol = tolerance(app);
    let Some(snap) = app.prefs_rt.snap.as_ref() else { return p };
    if !snap_on(app) || snap.disabled {
        app.prefs_rt.snap_lines.clear();
        return p;
    }
    let (out, lines) = match &snap.gesture {
        Gesture::Point => snap.targets.snap_point(p, tol),
        Gesture::TransformCorner { point, .. } if corner_distorts(app, &snap.gesture, mods) => {
            let proposed = [point[0] + p[0] - snap.start[0], point[1] + p[1] - snap.start[1]];
            let (corner, lines) = snap.targets.snap_point(proposed, tol);
            ([p[0] + corner[0] - proposed[0], p[1] + corner[1] - proposed[1]], lines)
        }
        Gesture::TransformCorner { .. } => snap.targets.snap_point(p, tol),
        Gesture::Move { rect } | Gesture::TransformMove { rect } | Gesture::SelectionMove { rect } | Gesture::DrawMove { rect } => {
            let d = [p[0] - snap.start[0], p[1] - snap.start[1]];
            let moved = [rect[0] + d[0], rect[1] + d[1], rect[2] + d[0], rect[3] + d[1]];
            let set = if snap.targets.is_empty() { &snap.smart } else { &snap.targets };
            let (adj, lines) = set.snap_rect(moved, tol);
            ([p[0] + adj[0], p[1] + adj[1]], lines)
        }
        // The crop frame snaps itself through `crop_snap`; its lines stay as it last set them.
        Gesture::Crop => (p, app.prefs_rt.snap_lines.clone()),
    };
    app.prefs_rt.snap_lines = lines;
    out
}

/// A corner stopped by the transform's convexity constraint is not aligned to the proposed
/// target. Keep only the lines the final corner actually reaches, after the transform applies.
pub(crate) fn retain_transform_lines(app: &mut PhotocraftApp, mods: egui::Modifiers) {
    let Some(snap) = app.prefs_rt.snap.as_ref() else { return };
    let Gesture::TransformCorner { index, .. } = snap.gesture else { return };
    if !corner_distorts(app, &snap.gesture, mods) {
        return;
    }
    let Some(point) = app.ui.transform.as_ref().and_then(|t| t.quad.get(index)).copied() else { return };
    app.prefs_rt.snap_lines.retain(|line| (if line.vertical { point[0] } else { point[1] } - line.pos).abs() < 1e-6);
}

/// What the Crop tool's drag snaps to, taken when it starts ([`crop_snap`]).
#[derive(Clone, Debug)]
pub(crate) struct CropSnap {
    targets: SnapTargets,
    smart: SnapTargets,
    tol: f64,
    /// The default mode moves the image under the box.
    image: bool,
    line: LineSnap,
}

/// How a Straighten line's end points snap, decided at the press from the frame state, as the
/// pointer's snapping of the Crop tool is: inside the default frame the point snaps; inside an
/// edited Classic frame the frame's rect, moved as far as the pointer, snaps and shifts the
/// pointer; anywhere else nothing snaps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum LineSnap {
    #[default]
    Off,
    Point,
    Move {
        rect: [f64; 4],
        start: [f64; 2],
    },
}

/// The [`LineSnap`] of a Straighten press at `p`; `inside` says the press is inside the frame.
pub(crate) fn line_snap(app: &PhotocraftApp, p: [f64; 2], inside: bool) -> LineSnap {
    let rect = app.ui.crop_rect.filter(|_| inside && crate::crop_ui::angle(app) == 0.0);
    match rect {
        Some(_) if app.crop.default_frame => LineSnap::Point,
        Some(rect) if crate::crop_mode::classic(app) => LineSnap::Move { rect, start: p },
        _ => LineSnap::Off,
    }
}

/// The snapping of the Crop drag in progress: None when View › Snap is off, Ctrl turned it off for
/// this gesture, or the drag isn't a crop's.
pub(crate) fn crop_snap(app: &PhotocraftApp) -> Option<CropSnap> {
    let snap = app.prefs_rt.snap.as_ref().filter(|s| s.gesture == Gesture::Crop && !s.disabled)?;
    snap_on(app).then(|| CropSnap {
        targets: snap.targets.clone(),
        smart: snap.smart.clone(),
        tol: tolerance(app),
        image: crate::crop_mode::moves_image(app),
        line: app.crop.line_snap,
    })
}

impl CropSnap {
    /// The document vector that moves a Straighten end point `q` (what the pointer gives now) onto
    /// its snap target, and the alignment lines it reaches.
    pub(crate) fn line_vector(&self, q: [f64; 2]) -> (Vec2, Vec<SnapLine>) {
        match self.line {
            LineSnap::Off => (Vec2::ZERO, Vec::new()),
            LineSnap::Point => {
                let (snapped, lines) = self.targets.snap_point(q, self.tol);
                (Vec2::new(snapped[0] - q[0], snapped[1] - q[1]), lines)
            }
            LineSnap::Move { rect, start } => {
                let d = [q[0] - start[0], q[1] - start[1]];
                let moved = [rect[0] + d[0], rect[1] + d[1], rect[2] + d[0], rect[3] + d[1]];
                let set = if self.targets.is_empty() { &self.smart } else { &self.targets };
                let (adj, lines) = set.snap_rect(moved, self.tol);
                (Vec2::new(adj[0], adj[1]), lines)
            }
        }
    }

    /// The document vector that moves the part `part` of `frame` (what the pointer gives now) onto
    /// its snap target, and the alignment lines it reaches. The frame's edges and centre snap when
    /// it moves (or the image moves under it), so content can be centred in it (#8580); a handle or
    /// the corner being drawn snaps itself, not the pointer, which grabbed it up to the grab radius
    /// away (#1919). A turned frame doesn't snap as a whole: its edges line up with nothing.
    pub(crate) fn vector(&self, part: BoxPart, frame: &CropBox) -> (Vec2, Vec<SnapLine>) {
        let none = (Vec2::ZERO, Vec::new());
        if part == BoxPart::Inside {
            if frame.angle != 0.0 {
                return none;
            }
            let r = frame.rect.abs();
            let rect = [r.x0, r.y0, r.x1, r.y1];
            // The image moving under the box: smart guides still snap when nothing in View › Snap
            // To is within reach (the document's bounds alone keep `targets` from ever being empty).
            // A classic-mode frame move uses them only when there are no targets, until craft-ui
            // row 101.
            let sets: &[&SnapTargets] = if self.image {
                &[&self.targets, &self.smart]
            } else if self.targets.is_empty() {
                &[&self.smart]
            } else {
                &[&self.targets]
            };
            for set in sets {
                let (adj, lines) = set.snap_rect(rect, self.tol);
                if !lines.is_empty() {
                    return (Vec2::new(adj[0], adj[1]), lines);
                }
            }
            return none;
        }
        let Some(handle) = frame.part_point(part) else { return none };
        let (snapped, mut lines) = self.targets.snap_point([handle.x, handle.y], self.tol);
        let deg = frame.angle;
        let mut d = crate::crop_ui::turn([snapped[0] - handle.x, snapped[1] - handle.y], [0.0, 0.0], -deg);
        // An edge handle controls only its own axis, not the midpoint along the other one.
        let (hx, hy) = crate::crop_ui::part_handle(part).unwrap_or((0, 0));
        if hx == 0 {
            d[0] = 0.0;
        }
        if hy == 0 {
            d[1] = 0.0;
        }
        if deg == 0.0 {
            lines.retain(|line| if line.vertical { hx != 0 } else { hy != 0 });
        }
        if lines.is_empty() {
            return none;
        }
        let d = crate::crop_ui::turn(d, [0.0, 0.0], deg);
        (Vec2::new(d[0], d[1]), lines)
    }
}

/// The bounds `[x0, y0, x1, y1]` of what drag `d` draws so far, for the tools whose drag Space
/// repositions and that snap (`None` for the rest, or a shape still too small to draw).
fn drawn_rect(app: &PhotocraftApp, d: &crate::canvas::Drag) -> Option<[f64; 4]> {
    let last = d.points.last().map_or(d.start, |p| [p[0], p[1]]);
    let (a, b) = match d.tool {
        Tool::RectMarquee | Tool::EllipseMarquee => crate::canvas::marquee_corners(&app.ui.tool_options, d, last),
        Tool::ObjectSelection => (d.start, last),
        t if crate::vector_ui::is_shape_tool(t) => return crate::vector_ui::shape_bounds(&app.ui.tool_options, t, d.start, last, d.live),
        _ => return None,
    };
    Some([a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])])
}

/// Space held while a marquee, Object Selection box or shape is drawn moves the whole of it
/// (`hold_keys`): its edges and centre snap, as a moved selection's do, not just the corner under
/// the pointer. Releasing Space goes back to snapping that corner.
fn follow_reposition(app: &mut PhotocraftApp) {
    let Some(d) = app.drag.as_ref() else { return };
    let Some(snap) = app.prefs_rt.snap.as_ref() else { return };
    let gesture = match (&snap.gesture, d.reposition) {
        (Gesture::Point, true) => match drawn_rect(app, d) {
            Some(rect) => Gesture::DrawMove { rect },
            None => return,
        },
        (Gesture::DrawMove { .. }, false) => Gesture::Point,
        _ => return,
    };
    let last = d.points.last().map_or(d.start, |p| [p[0], p[1]]);
    if let Some(snap) = app.prefs_rt.snap.as_mut() {
        snap.gesture = gesture;
        snap.start = last;
    }
}

/// Snap a tool event's coordinates (called by `canvas::tool_event` before the tools see it).
pub fn filter_event(app: &mut PhotocraftApp, ev: ToolEvent, mods: egui::Modifiers) -> ToolEvent {
    match ev {
        ToolEvent::Down { x, y, pressure } => {
            begin(app, [x, y], mods);
            // Moves snap their delta, which is zero at the start: keep the press point.
            let p = match app.prefs_rt.snap.as_ref().map(|s| &s.gesture) {
                Some(Gesture::Point) => {
                    let q = apply(app, [x, y], mods);
                    pixel_round(app, q)
                }
                _ => [x, y],
            };
            ToolEvent::Down { x: p[0], y: p[1], pressure }
        }
        ToolEvent::Move { x, y, pressure } => {
            if app.prefs_rt.snap.is_none() {
                return ev;
            }
            follow_reposition(app);
            let disabled = override_for_gesture(app, mods);
            if let Some(s) = app.prefs_rt.snap.as_mut() {
                s.disabled = disabled;
            }
            let q = apply(app, [x, y], mods);
            let p = pixel_round(app, q);
            ToolEvent::Move { x: p[0], y: p[1], pressure }
        }
        ToolEvent::Up { x, y } => {
            if app.prefs_rt.snap.is_none() {
                return ev;
            }
            follow_reposition(app);
            let disabled = override_for_gesture(app, mods);
            if let Some(s) = app.prefs_rt.snap.as_mut() {
                s.disabled = disabled;
            }
            let q = apply(app, [x, y], mods);
            let p = pixel_round(app, q);
            app.prefs_rt.snap = None;
            app.prefs_rt.snap_lines.clear();
            ToolEvent::Up { x: p[0], y: p[1] }
        }
    }
}

/// Snap a guide being dragged out of a ruler (`vertical` guides snap their x).
pub fn snap_guide(app: &mut PhotocraftApp, vertical: bool, pos: f64) -> f64 {
    if !snap_on(app) {
        return pos;
    }
    let Some(st) = app.session.active() else { return pos };
    // The Snap To options (grid, layers, document bounds, grid step) change the targets too.
    let key = (st.doc.id, st.revision, options(app));
    if app.prefs_rt.guide_targets.as_ref().is_none_or(|(k, _)| *k != key) {
        let t = build(app, &[], false).filtered(|k| k != SnapKind::Guide);
        app.prefs_rt.guide_targets = Some((key, t));
    }
    let tol = tolerance(app);
    let t = app.prefs_rt.guide_targets.as_ref().map(|(_, t)| t);
    t.and_then(|t| t.nearest(vertical, pos, tol)).map_or(pos, |t| t.pos)
}

fn hex_color(s: &str, fallback: Color32) -> Color32 {
    photocraft_engine::prefs::parse_hex(s).map_or(fallback, |c| Color32::from_rgb(c[0], c[1], c[2]))
}

/// The Smart Guides colour from Preferences › Guides, Grid & Slices (magenta by default).
pub(crate) fn smart_color(app: &PhotocraftApp) -> Color32 {
    hex_color(&app.session.prefs().guides_grid_and_slices.smart_guide_color, Color32::from_rgb(255, 0, 255))
}

/// Draw smart guides (and the moving bounds) for the drag in progress.
pub fn draw(app: &PhotocraftApp, ui: &egui::Ui, xf: &ViewXform) {
    use craft::ui::canvas::{SmartGuides, SmartLine, kurbo::Point};
    let Some(snap) = &app.prefs_rt.snap else { return };
    let lines: Vec<SmartLine> = if smart_on(app) {
        app.prefs_rt
            .snap_lines
            .iter()
            .filter(|l| l.kind.is_smart())
            .map(|l| {
                let (a, b) = if l.vertical { ([l.pos, l.from], [l.pos, l.to]) } else { ([l.from, l.pos], [l.to, l.pos]) };
                SmartLine { a: Point::new(a[0], a[1]), b: Point::new(b[0], b[1]), extension: false }
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut guides = SmartGuides::new(&lines, &[]).color(smart_color(app));
    // The moved layers' bounds follow the Move tool (Free Transform draws its own box).
    if let (Gesture::Move { rect }, Some(d)) =
        (&snap.gesture, app.drag.as_ref().and_then(|d| d.points.last()).map(|p| [p[0] - snap.start[0], p[1] - snap.start[1]]))
    {
        let r = [rect[0] + d[0], rect[1] + d[1], rect[2] + d[0], rect[3] + d[1]];
        guides = guides.bounds(craft::ui::canvas::kurbo::Rect::new(r[0], r[1], r[2], r[3]));
    }
    guides.show(ui, &xf.canvas_view());
}

#[cfg(test)]
mod tests;
