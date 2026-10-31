use super::selection::floating_outline;
use super::view_xform::ViewXform;
use super::{Color32, LayerContent, PhotocraftApp, Pos2, Rect, Tool, Vec2, pos2, vec2};
use craft_ui::canvas::quad::{BoxPart, BoxState};
use craft_ui::canvas::{RotateZone, TransformBox, TransformNames, kurbo::Point};
use craft_ui::theme::ToEgui;

/// View › Show › Layer Edges: the colour of the active layer's outline.
pub(crate) const LAYER_EDGES: Color32 = Color32::from_rgb(0x2d, 0x8c, 0xeb);

/// View › Show › Layer Edges: the active layer's content bounds (cached per tile) where the canvas
/// shows them. During a Move drag that is at the offset the image was drawn with this frame, so
/// the outline moves in lockstep with the layer's pixels (#2413).
pub(crate) fn layer_edges_rect(app: &PhotocraftApp, idx: usize) -> Option<photocraft_geom::Rect> {
    if !app.ui.view.shows(app.ui.view.show.layer_edges) {
        return None;
    }
    let st = app.session.documents().get(idx)?;
    let id = st.active_layer?;
    let b = st.doc.layer(id)?.surface().map(photocraft_compose::bounds::content_bounds)?;
    if b.is_empty() {
        return None;
    }
    let (dx, dy) = crate::move_ui::shown_layer_offset(app, st.doc.id, st.revision, id).unwrap_or((0, 0));
    Some(b.translate(dx, dy))
}

/// The layers Move › Show Transform Controls frames: every selected layer but the Background.
/// A corner radius drag hides the box, as in Photoshop. So does View › Extras (⌘H, #6699): the
/// controls are an Extra, hidden and inert until Extras comes back, while Show Transform Controls
/// keeps its setting.
pub(super) fn transform_controls_layers(app: &PhotocraftApp) -> Vec<&photocraft_doc::Layer> {
    if app.ui.tool != Tool::Move
        || !app.ui.tool_options.move_show_transform
        || !app.ui.view.extras
        || app.ui.transform.is_some()
        || app.drag.is_some()
        || app.ui.corner_radius.is_some()
    {
        return Vec::new();
    }
    let Some(st) = app.session.active() else { return Vec::new() };
    st.selected_layers().into_iter().filter_map(|id| st.doc.layer(id)).filter(|l| !crate::doc_props_ui::is_background(&st.doc, l)).collect()
}

/// Move tool › Show Transform Controls: the boxes Free Transform would open on the selected
/// layers, joined into one.
pub(super) fn transform_controls_rect(app: &PhotocraftApp, xf: &ViewXform) -> Option<Rect> {
    let st = app.session.active()?;
    let layers = transform_controls_layers(app);
    let b = layers.iter().map(|l| photocraft_engine::transform_cmds::transform_frame(&st.doc, l)).filter(|b| !b.is_empty()).reduce(|a, b| a.union(&b))?;
    // Floating selected pixels are framed by their outline, as their ants are (#7538), not by the
    // selection they were cut with; a frame round other selected layers too keeps them all.
    let alone = matches!(layers.as_slice(), [l] if photocraft_engine::float_cmds::floating(st).is_some_and(|f| f.layer == l.id));
    let b = app.session.active_index().filter(|_| alone).and_then(|idx| floating_outline(app, idx)).map_or(b, |o| o.content_bounds());
    let (dx, dy) = crate::move_ui::floating_offset(app).unwrap_or((0, 0));
    let b = b.translate(dx, dy);
    if b.is_empty() {
        return None;
    }
    Some(Rect::from_two_pos(xf.to_screen(b.x0 as f32, b.y0 as f32), xf.to_screen(b.x1 as f32, b.y1 as f32)))
}

/// The part of the Move tool's transform controls (the frame `r` on screen) under screen point
/// `p`: a handle scales, the band `transform.band_inner` to `transform.band_outer` outside a
/// corner rotates, and the interior stays the ordinary Move drag (#2871). The library's box decides,
/// so the pointer and the press use the same reach. No modifiers are passed: ⌥ is the Move tool's
/// duplicate drag, not the reference point.
fn control_part(r: Rect, p: Pos2) -> Option<BoxPart> {
    let corner = |q: Pos2| Point::new(f64::from(q.x), f64::from(q.y));
    let state = BoxState {
        quad: [corner(r.left_top()), corner(r.right_top()), corner(r.right_bottom()), corner(r.left_bottom())],
        // The controls have no reference point: far from every handle.
        pivot: Point::new(1e9, 1e9),
    };
    // Screen points are the frame's own, so the view maps them to themselves.
    let view = crate::transform_tool::point_view(1.0);
    let part = TransformBox::new("photocraft.move-controls", &state, TransformNames::default()).rotate_zone(RotateZone::Band).inside_moves(false).hit(
        &view,
        crate::transform_tool::reach(),
        p,
        egui::Modifiers::NONE,
    )?;
    (part != BoxPart::Pivot).then_some(part)
}

/// Use the same hit areas for pointer-down and hover, so a control never behaves like
/// resize/rotate while showing the ordinary four-arrow Move cursor (#2871).
pub(super) fn transform_controls_cursor(r: Rect, p: Pos2) -> Option<egui::CursorIcon> {
    Some(match control_part(r, p)? {
        BoxPart::Corner(0 | 2) => egui::CursorIcon::ResizeNwSe,
        BoxPart::Corner(_) => egui::CursorIcon::ResizeNeSw,
        BoxPart::Edge(0 | 2) => egui::CursorIcon::ResizeVertical,
        BoxPart::Edge(_) => egui::CursorIcon::ResizeHorizontal,
        // The rotation band: the pointer gives way to the curved rotate arrow (`transform_controls_turn`).
        BoxPart::Outside => egui::CursorIcon::None,
        BoxPart::Inside | BoxPart::Pivot => return None,
    })
}

/// In the rotation band just outside the Move tool's transform controls, the screen direction the
/// curved rotate arrow bends round: into the box along the nearest corner's bisector, as Free
/// Transform draws it (#4658). Before, the band showed a plain alias pointer, so nothing said a
/// drag there rotates.
pub(super) fn transform_controls_turn(r: Rect, p: Pos2) -> Option<Vec2> {
    if control_part(r, p) != Some(BoxPart::Outside) {
        return None;
    }
    let corner = pos2(if p.x < r.center().x { r.left() } else { r.right() }, if p.y < r.center().y { r.top() } else { r.bottom() });
    Some(vec2((r.center().x - corner.x).signum(), (r.center().y - corner.y).signum()).normalized() * 16.0)
}

/// A visible handle starts scaling; the narrow band just outside the box starts rotation.
/// The interior stays the normal Move-tool drag target.
pub(super) fn transform_controls_hit(r: Rect, p: Pos2) -> bool {
    transform_controls_cursor(r, p).is_some()
}

/// Enter the existing Free Transform session when a Move-tool transform control is pressed: on
/// every selected layer, as the frame shows them (#6501).
pub(super) fn begin_transform_controls_at(app: &mut PhotocraftApp, ctx: &egui::Context, xf: &ViewXform, p: Pos2) -> bool {
    let Some(r) = transform_controls_rect(app, xf) else { return false };
    if !transform_controls_hit(r, p) {
        return false;
    }
    match crate::transform_tool::begin(app, ctx) {
        Ok(()) => true,
        Err(e) => {
            app.ui.status = e;
            false
        }
    }
}

/// Move tool › Show Transform Controls: the selected layers' bounding box, with its eight handles,
/// and the outline of every selected shape layer, as in Photoshop.
pub(super) fn draw_transform_controls(app: &mut PhotocraftApp, painter: &egui::Painter, xf: &ViewXform) {
    let Some(r) = transform_controls_rect(app, xf) else { return };
    let accent = craft_ui::theme::current(painter.ctx()).color.accent.solid.to_egui();
    let to_scr = |q: [f64; 2]| xf.to_screen(q[0] as f32, q[1] as f32);
    for l in transform_controls_layers(app) {
        if let LayerContent::Shape(sh) = &l.content {
            crate::vector_ui::draw_outline(painter, &sh.path, &to_scr, accent);
        }
    }
    let tokens = craft_ui::theme::current(painter.ctx());
    let style = craft_ui::canvas::TransformBox::style(&tokens);
    painter.rect_stroke(r, craft_ui::theme::scale::Radius::R0.px(), style.line, egui::StrokeKind::Middle);
    let ppp = painter.ctx().pixels_per_point();
    for p in [r.left_top(), r.center_top(), r.right_top(), r.right_center(), r.right_bottom(), r.center_bottom(), r.left_bottom(), r.left_center()] {
        painter.extend(craft_ui::primitives::Handle::shapes_with(&style.handle, craft_ui::primitives::HandleShape::Square, p, ppp));
    }
}
