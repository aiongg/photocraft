//! The canvas's snapping data: View › Snap To and the document as the canvas `Snap` (cw.C16).
//!
//! [`snap`] gives the canvas the targets and options; the canvas sets the tolerance and the
//! tools snap through it. The tool host reports the lines that snapped and draws them. The
//! targets are cached because the canvas compares `Snap` by `Arc::ptr_eq`: a new `Arc` each
//! pass would count as a changed canvas input every pass.

use std::sync::Arc;

use craft::behavior::Axis;
use craft::behavior::geometry::{SnapKind, SnapOptions, SnapTargets};
use craft::behavior::kurbo::{Point, Rect, Vec2};
use craft::ui::canvas::Snap;
use craft::ui::style::DataColor;
use egui::Color32;
use photocraft_doc::{DocId, LayerId};
use photocraft_engine::layer_multi_cmds::move_targets;
use photocraft_engine::snap::{layer_rect, target_rect_of};

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

/// The layers a drag moves or resizes: they are not targets. What the Move tool would move whole
/// (the selected layers, the layers linked to them and everything inside a group, so a group does not
/// snap to its own layers' starting edges), the type layer being edited (a paragraph-box handle
/// drag does not snap to the text it resizes, #6819) and the layers of the open Free Transform.
fn moving_set(app: &PhotocraftApp) -> Vec<LayerId> {
    let mut set = Vec::new();
    if moves_as_move_tool(app)
        && !crate::move_ui::moves_selected_pixels_with(app, Tool::Move)
        && let Some(st) = app.session.active()
    {
        let selected = st.selected_layers();
        set.extend(crate::move_ui::with_descendants(&st.doc, &move_targets(&st.doc, &selected)));
        set.extend(selected);
    }
    if let Some(edit) = app.ui.text_edit.as_ref() {
        set.push(LayerId(edit.layer));
    }
    if let Some(t) = app.ui.transform.as_ref() {
        set.extend(crate::transform_tool::moving_layers(t));
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

/// The bounds of layer `id` as the Move tool moves and snaps it. `snap::layer_rect` skips groups,
/// which hold no pixels of their own: a group is the layers in it, whole even with a selection (it
/// moves them whole, so clipping its pixel layers to the selection would frame less than moves). An
/// artboard is its board, as the snap targets take it, since the whole board moves.
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

/// The Smart Guides colour from Preferences › Guides, Grid & Slices; `None` when it does not parse
/// (the guides' own colour).
pub(crate) fn smart_guide_color(app: &PhotocraftApp) -> Option<DataColor> {
    photocraft_engine::prefs::parse_hex(&app.session.prefs().guides_grid_and_slices.smart_guide_color)
        .map(|c| DataColor::new(Color32::from_rgb(c[0], c[1], c[2])))
}

#[cfg(test)]
mod tests;
