//! The canvas's snapping data: View › Snap To and the document as the canvas `Snap` (cw.C16).
//!
//! [`snap`] gives the canvas the targets and options; the canvas sets the tolerance and the
//! tools snap through it. The tool host reports the lines that snapped and draws them. The
//! targets are cached because the canvas compares `Snap` by `Arc::ptr_eq`: a new `Arc` each
//! pass would count as a changed canvas input every pass.

<<<<<<< HEAD
use std::sync::Arc;

use craft::behavior::Axis;
use craft::behavior::geometry::{SnapKind, SnapOptions, SnapTargets};
use craft::behavior::kurbo::{Point, Rect, Vec2};
use craft::ui::canvas::Snap;
use craft::ui::style::DataColor;
use egui::Color32;
use photocraft_doc::{DocId, LayerId};
use photocraft_engine::snap::target_rect_of;
=======
use egui::{Color32, Stroke, pos2};
use photocraft_doc::LayerId;
use photocraft_engine::layer_multi_cmds::move_targets;
use photocraft_engine::snap::{SnapKind, SnapLine, SnapOptions, SnapTargets, layer_rect, union};
>>>>>>> upstream/main

use crate::PhotocraftApp;
use crate::state::Tool;

/// What the cached targets were built from.
#[derive(PartialEq)]
struct Key {
    doc: DocId,
    revision: u64,
    options: SnapOptions,
    exclude: Vec<LayerId>,
    /// The grid step's bits: the preferences change it without a document edit.
    grid: Option<[u64; 2]>,
}

/// The last built targets, kept at `PhotocraftApp::snap_cache`.
pub(crate) struct SnapCache {
    key: Key,
    targets: Arc<SnapTargets>,
}

/// Whether a drag now would run as the Move tool: the Move tool is selected, or Cmd is held with a
/// tool that hands its press to the Move stand-in. Read before the press so the targets do not
/// change at it.
fn moves_as_move_tool(app: &PhotocraftApp) -> bool {
    app.ui.tool == Tool::Move || (crate::canvas::tools::takes_command_stand_in(app.ui.tool) && craft::ui::runtime::pointer::frame(&app.ctx()).mods.cmd)
}

/// The selected pixels move with the drag, so the selection is not a place to snap to (#6290).
fn selection_moves(app: &PhotocraftApp) -> bool {
    let pixels = moves_as_move_tool(app) && crate::move_ui::moves_selected_pixels_with(app, Tool::Move);
    let transform = app.ui.transform.as_ref().is_some_and(|t| crate::transform_tool::moves_selection(app, t));
    pixels || transform
}

/// The layers a drag moves or resizes: they are not targets. The selected layers while the Move tool
/// would move them whole, and the type layer being edited (a paragraph-box handle drag does not snap to
/// the text it resizes, #6819).
fn moving_set(app: &PhotocraftApp) -> Vec<LayerId> {
    let mut set = Vec::new();
    if moves_as_move_tool(app)
        && !crate::move_ui::moves_selected_pixels_with(app, Tool::Move)
        && let Some(st) = app.session.active()
    {
        set.extend(st.selected_layers());
    }
    if let Some(edit) = app.ui.text_edit.as_ref() {
        set.push(LayerId(edit.layer));
    }
    set
}

/// View › Snap To and View › Show › Smart Guides.
fn options(app: &PhotocraftApp) -> SnapOptions {
    let st = &app.ui.view.snap_to;
    let e = &app.ui.extras;
    let mut o = SnapOptions::default();
    o.on = e.snap;
    o.guides = st.guides && e.guides && app.ui.view.shows(app.ui.view.show.canvas_guides);
    o.grid = st.grid && e.grid;
    o.layers = st.layers;
    o.document_bounds = st.document_bounds;
    o.selection = !selection_moves(app);
    o.smart_guides = app.ui.view.shows(app.ui.view.show.smart_guides);
    o
}

/// The grid: major spacing over the subdivisions, per axis, counted from the origin.
fn grid(app: &PhotocraftApp, st: &photocraft_engine::DocState) -> (Point, Vec2) {
    let prefs = app.session.prefs();
    let g = &prefs.guides_grid_and_slices;
    let ppi = prefs.units_and_rulers.point_size.per_inch();
    let size = [st.doc.size.width as f64, st.doc.size.height as f64];
    let [x, y] = crate::rulers::grid_major_px(g, st.doc.resolution_dpi.max(1.0) as f64, size, ppi).map(|step| step / g.subdivisions.max(1) as f64);
    (Point::ORIGIN, Vec2::new(x, y))
}

<<<<<<< HEAD
fn build(st: &photocraft_engine::DocState, options: &SnapOptions, exclude: &[LayerId], grid: (Point, Vec2)) -> SnapTargets {
    let doc = &st.doc;
    let bounds = Rect::new(0.0, 0.0, doc.size.width as f64, doc.size.height as f64);
    let guides = doc.guides.vertical.iter().map(|g| (Axis::Y, *g)).chain(doc.guides.horizontal.iter().map(|g| (Axis::X, *g)));
    let mut t = SnapTargets::new().with_document(options, bounds, guides, Some(grid));
    if options.layers {
        for (_, _, l) in doc.walk() {
            if !l.visible || exclude.contains(&l.id) {
                continue;
            }
            if let Some(r) = target_rect_of(doc, l) {
                t.add_rect(Rect::new(r[0], r[1], r[2], r[3]), SnapKind::LayerEdge, SnapKind::LayerCenter);
            }
        }
    }
    if options.selection
        && let Some(sel) = &doc.selection
=======
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
    // What moves, not what is selected: a group carries its layers and linked layers go along.
    union(move_targets(&st.doc, &st.selected_layers()).into_iter().filter_map(|id| tree_rect(&st.doc, id)))
}

/// The bounds of layer `id` as the Move tool moves and snaps it. `snap::layer_rect` skips groups,
/// which hold no pixels of their own: a group is the layers in it, whole even with a selection
/// (it moves them whole, so clipping its pixel layers to the selection would frame less than
/// moves). An artboard is its board, as the snap targets take it, since the whole board moves.
pub(crate) fn tree_rect(doc: &photocraft_doc::Document, id: LayerId) -> Option<[f64; 4]> {
    let l = doc.layer(id)?;
    let b = if let Some(a) = l.artboard() {
        a.rect
    } else if l.is_group() {
        photocraft_engine::transform_cmds::transform_bounds(doc, l)
    } else {
        return layer_rect(doc, id);
    };
    (!b.is_empty()).then(|| [b.x0, b.y0, b.x1, b.y1].map(f64::from))
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
            | Tool::Star
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

/// The nine reference point locations of a Free Transform box: corners, edge midpoints, centre.
fn box_anchors(q: &[[f64; 2]; 4]) -> [[f64; 2]; 9] {
    let mid = |a: [f64; 2], b: [f64; 2]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    [q[0], q[1], q[2], q[3], mid(q[0], q[1]), mid(q[1], q[2]), mid(q[2], q[3]), mid(q[3], q[0]), mid(mid(q[0], q[2]), mid(q[1], q[3]))]
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
    let crop_tol = crate::crop_ui::tolerance(app);
    let straightens = tool == Tool::Crop && app.crop.drag.is_none() && (app.crop.straighten.armed || mods.command);
    let gesture = if let Some(t) = &app.ui.transform {
        use crate::transform_tool::Hit;
        let h = crate::transform_tool::box_hit(app, t, p, crate::transform_tool::handle_tolerance(app));
        let corner = match h {
            Hit::Corner(index) => t.quad.get(index).map(|&point| (Gesture::TransformCorner { index, point }, crate::transform_tool::moving_layers(t))),
            _ => None,
        };
        let pivot = || Some((Gesture::Pivot { anchors: box_anchors(&t.quad) }, crate::transform_tool::moving_layers(t)));
        if t.mode == crate::state::TransformMode::Distort {
            corner
        } else {
            // Classify the press as `transform_tool::pointer` does: a handle or the reference point
            // (dragged, or Alt-pressed anywhere off the handles) snaps itself; only a press inside
            // the box moves it and snaps its bounds. Snapping the box's bounds while the reference
            // point was dragged held the point still and drew lines at edges the box never reached.
            match h {
                Hit::Corner(_) => corner,
                Hit::Edge(_) => Some((Gesture::Point, crate::transform_tool::moving_layers(t))),
                Hit::Pivot => pivot(),
                Hit::Inside | Hit::Outside if mods.alt => pivot(),
                Hit::Inside => Some((Gesture::TransformMove { rect: quad_rect(&t.quad) }, crate::transform_tool::moving_layers(t))),
                Hit::Outside => None,
            }
        }
    } else if tool == Tool::Move && crate::rulers::guide_at(app, p[0], p[1]).is_some() {
        Some((Gesture::Guide, Vec::new()))
    } else if tool == Tool::Move || moves_layer {
        // The targets and everything inside them stay out of the snap targets: a moving group
        // must not snap to its own layers' starting edges. Selected pixels float off the active
        // layer alone, so its linked layers stay put and stay targets.
        let pixels = crate::move_ui::moves_selected_pixels_with(app, tool);
        let exclude = app.session.active().map_or_else(Vec::new, |s| {
            let mut ids = s.selected_layers();
            if !pixels {
                ids.extend(crate::move_ui::with_descendants(&s.doc, &move_targets(&s.doc, &ids)));
            }
            ids
        });
        moving_rect(app, tool).map(|rect| (Gesture::Move { rect }, exclude))
    } else if straightens {
        // A straighten line (`crop_straighten::pointer` takes the press before the frame's drags)
        // snaps its ends as points wherever it starts, never by the offset of a frame or image it
        // doesn't move.
        Some((Gesture::Point, Vec::new()))
    } else if tool == Tool::Crop
        && (crate::crop_mode::classic(app) || app.crop.default_frame)
        && let Some(rect) =
            app.ui.crop_rect.filter(|r| crate::crop_ui::angle(app) == 0.0 && crate::crop_ui::hit(*r, p, crop_tol) == crate::crop_ui::Hit::Inside)
>>>>>>> upstream/main
    {
        let b = sel.content_bounds();
        if !b.is_empty() {
            t.add_rect(Rect::new(b.x0 as f64, b.y0 as f64, b.x1 as f64, b.y1 as f64), SnapKind::Selection, SnapKind::Selection);
        }
    }
    t
}

/// The targets of document `idx` without the layers in `exclude`: the one target builder. The same
/// `Arc` comes back while the document revision, the Snap To options, the grid and `exclude` stay.
pub(crate) fn targets(app: &PhotocraftApp, idx: usize, exclude: &[LayerId]) -> Arc<SnapTargets> {
    let Some(st) = app.session.documents().get(idx) else { return Arc::new(SnapTargets::new()) };
    let options = options(app);
    let grid = grid(app, st);
    let key = Key { doc: st.doc.id, revision: st.revision, options, exclude: exclude.to_vec(), grid: Some([grid.1.x.to_bits(), grid.1.y.to_bits()]) };
    let mut cache = app.snap_cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(c) = cache.as_ref().filter(|c| c.key == key) {
        return c.targets.clone();
    }
    let targets = Arc::new(build(st, &options, exclude, grid));
    *cache = Some(SnapCache { key, targets: targets.clone() });
    targets
}

/// The canvas `Snap` of document `idx`: View › Snap To over its targets, without the layers a drag moves.
pub(crate) fn snap(app: &PhotocraftApp, idx: usize) -> Snap {
    let exclude = if app.session.active_index() == Some(idx) { moving_set(app) } else { Vec::new() };
    Snap::new(targets(app, idx, &exclude), options(app))
}

/// The Smart Guides colour from Preferences › Guides, Grid & Slices; `None` when it does not parse
/// (the guides' own colour).
pub(crate) fn smart_guide_color(app: &PhotocraftApp) -> Option<DataColor> {
    photocraft_engine::prefs::parse_hex(&app.session.prefs().guides_grid_and_slices.smart_guide_color)
        .map(|c| DataColor::new(Color32::from_rgb(c[0], c[1], c[2])))
}

#[cfg(test)]
mod tests;
