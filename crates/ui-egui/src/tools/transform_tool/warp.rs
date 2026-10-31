use super::begin::warps_type;
use super::pointer::point_view;
use super::{BezierMesh, CursorIcon, PhotocraftApp, SplitTool, ToolEvent, TransformPreview, ViewXform, Warp, WarpDrag, WarpStyle, json};
use craft::ui::canvas::kurbo::{PathEl, Point, Rect};
use craft::imaging::ui::{WarpEdit, WarpGrid, WarpMesh, WarpNames, WarpSplit, WarpStyle as WarpGridStyle, WarpTarget};
use egui::{Pos2, pos2};
use std::sync::OnceLock;

/// An anchor or a handle on a section line. The inner control points of a patch are not shown.
pub(super) fn on_section_edge(i: usize, j: usize) -> bool {
    i.is_multiple_of(3) || j.is_multiple_of(3)
}

/// A 1×1 mesh (Grid › Default) draws rule-of-thirds guides. Denser presets are real patch
/// boundaries only — a 3×3 is three cells, not a third-line lattice inside every cell.
pub(super) fn single_patch(mesh: &BezierMesh) -> bool {
    mesh.us.len() == 2 && mesh.vs.len() == 2
}

/// The surface parameters under `p` when it is inside the warped mesh, else None.
pub(super) fn surface_at(mesh: &BezierMesh, p: [f64; 2]) -> Option<(f64, f64)> {
    let (s, t) = mesh.param_at(p);
    let q = mesh.eval(s, t);
    ((q[0] - p[0]).hypot(q[1] - p[1]) < 1.0).then_some((s, t))
}

/// The library's sizes for the warp grid's points (`transform.hit`): the same in every theme.
fn warp_reach() -> &'static WarpGridStyle {
    static STYLE: OnceLock<WarpGridStyle> = OnceLock::new();
    STYLE.get_or_init(|| WarpGrid::style(&craft::ui::theme::current(&egui::Context::default())))
}

fn split_of(tool: SplitTool) -> WarpSplit {
    match tool {
        SplitTool::Cross => WarpSplit::Cross,
        SplitTool::Vertical => WarpSplit::Vertical,
        SplitTool::Horizontal => WarpSplit::Horizontal,
    }
}

fn tool_of(split: WarpSplit) -> SplitTool {
    match split {
        WarpSplit::Cross => SplitTool::Cross,
        WarpSplit::Vertical => SplitTool::Vertical,
        WarpSplit::Horizontal => SplitTool::Horizontal,
    }
}

/// Runs `f` with the library's grid over warp `w`: its anchors and handles, the surface under a
/// point, and the split the armed button (or ⌥ and ⌘) makes. `None` for a mesh the library
/// refuses.
pub(super) fn with_grid<R>(w: &Warp, armed: Option<SplitTool>, mods: egui::Modifiers, f: impl FnOnce(&WarpGrid, &WarpMesh) -> R) -> Option<R> {
    let m = w.to_mesh(1, 1);
    let points = m.points.iter().map(|p| Point::new(p[0], p[1])).collect();
    let lib = WarpMesh::new(m.us.clone(), m.vs.clone(), points)?;
    let b = w.bounds;
    let names = WarpNames::new("Warp", |_, _| String::new(), |_, _, _| String::new());
    let grid = WarpGrid::new("photocraft.warp", &lib, Rect::new(b[0], b[1], b[2], b[3]), names)
        .preset(w.style != WarpStyle::Custom)
        .surface(|p| surface_at(&m, [p.x, p.y]))
        .split(armed.map(split_of))
        .modifiers(mods);
    Some(f(&grid, &lib))
}

/// The cursor for hovering document point `p` over a warp: the arrow across the split a button or
/// ⌥ / ⌘ would make there, else a crosshair over a point and a hand over the surface.
pub(super) fn warp_cursor(app: &PhotocraftApp, w: &Warp, xf: &ViewXform, p: [f64; 2], mods: egui::Modifiers) -> CursorIcon {
    if warps_type(app) {
        return CursorIcon::Default;
    }
    let (view, at) = (xf.canvas_view(), xf.to_screen(p[0] as f32, p[1] as f32));
    let armed = app.transform_preview.as_ref().and_then(|pv| pv.split_tool);
    if armed.is_some() || mods.alt || mods.command {
        return with_grid(w, armed, mods, |g, _| g.split_hit(&view, None, at, mods).map(|s| s.cursor())).flatten().unwrap_or(CursorIcon::Default);
    }
    if w.style != WarpStyle::Custom {
        return CursorIcon::Crosshair;
    }
    match with_grid(w, None, mods, |g, _| g.hit(&view, warp_reach(), at, mods)).flatten() {
        Some(WarpTarget::Point(_)) => CursorIcon::Crosshair,
        Some(WarpTarget::Surface(..)) => CursorIcon::Grab,
        None => CursorIcon::Default,
    }
}

/// The lines the split at document point `p` would add, in screen points, for the guide that
/// follows the pointer; empty where no split is armed or the pointer is off the surface.
pub(super) fn split_guide(w: &Warp, pv: &TransformPreview, xf: &ViewXform, p: [f64; 2], mods: egui::Modifiers) -> Vec<Vec<Pos2>> {
    let (view, at) = (xf.canvas_view(), xf.to_screen(p[0] as f32, p[1] as f32));
    let lines =
        with_grid(w, pv.split_tool, mods, |g, lib| g.split_hit(&view, pv.split_press.as_ref(), at, mods).map(|s| s.lines(lib))).flatten().unwrap_or_default();
    let mut out: Vec<Vec<Pos2>> = Vec::new();
    for path in lines {
        craft::ui::canvas::kurbo::flatten(path, 0.25 / f64::from(xf.zoom.max(1e-3)), |el| match el {
            PathEl::MoveTo(q) => out.push(vec![xf.to_screen(q.x as f32, q.y as f32)]),
            PathEl::LineTo(q) => {
                if let Some(line) = out.last_mut() {
                    line.push(xf.to_screen(q.x as f32, q.y as f32));
                }
            }
            _ => {}
        });
    }
    out.retain(|l| l.len() > 1);
    out
}

/// Armed split tool, or Option held: the guide tracks the pointer and the split lands on release.
/// Returns false when this event is an ordinary control-point drag.
fn split_gesture(app: &mut PhotocraftApp, ev: ToolEvent, zoom: f64, mods: egui::Modifiers) -> bool {
    let armed = app.transform_preview.as_ref().and_then(|p| p.split_tool);
    let placing = app.transform_preview.as_ref().is_some_and(|p| p.split_placing);
    let (view, canvas) = (point_view(zoom), |x: f64, y: f64| pos2((x * zoom) as f32, (y * zoom) as f32));
    match ev {
        ToolEvent::Down { x, y, .. } if armed.is_some() || mods.alt || mods.command => {
            let press = app
                .ui
                .transform
                .as_ref()
                .and_then(|t| t.warp.as_ref())
                .and_then(|w| with_grid(w, armed, mods, |g, _| g.split_press(&view, canvas(x, y), mods)).flatten());
            if let Some(pv) = app.transform_preview.as_mut() {
                pv.split_placing = true;
                pv.split_pointer = Some([x, y]);
                pv.split_press = press;
            }
            true
        }
        ToolEvent::Move { x, y, .. } if placing => {
            if let Some(pv) = app.transform_preview.as_mut() {
                pv.split_pointer = Some([x, y]);
            }
            true
        }
        ToolEvent::Up { x, y } if placing => {
            let press = app.transform_preview.as_ref().and_then(|p| p.split_press);
            if let Some(pv) = app.transform_preview.as_mut() {
                pv.split_placing = false;
                pv.split_pointer = None;
                pv.split_press = None;
            }
            let edit = press
                .zip(app.ui.transform.as_ref().and_then(|t| t.warp.as_ref()))
                .and_then(|(press, w)| with_grid(w, armed, mods, |g, _| g.split_release(&view, &press, canvas(x, y))).flatten());
            if let Some(WarpEdit::Split { split: kind, .. }) = edit {
                // On an existing line, or a click the drag path also delivers, this is a no-op.
                let _ = split(app, tool_of(kind).command(), Some([x, y]));
            }
            true
        }
        _ => false,
    }
}

/// Drags the warp grid. A press on an anchor or handle moves that point (an anchor carries its
/// handles along); a press anywhere else inside the grid pulls the surface under the pointer.
/// Preset warps turn into a custom mesh when the press grabs something. An armed split tool, or
/// Ctrl / Option held, places a split where the pointer is released instead.
pub(super) fn warp_pointer(app: &mut PhotocraftApp, ev: ToolEvent, zoom: f64, mods: egui::Modifiers) {
    // Type takes only the warp styles: nothing on the canvas to drag or split (#3004).
    if warps_type(app) {
        return;
    }
    if split_gesture(app, ev, zoom, mods) {
        return;
    }
    let (Some(t), Some(pv)) = (app.ui.transform.as_mut(), app.transform_preview.as_mut()) else { return };
    let Some(w) = t.warp.as_mut() else { return };
    let (view, style) = (point_view(zoom), warp_reach());
    match ev {
        ToolEvent::Down { x, y, .. } => {
            // A preset warp becomes a custom mesh only when the press grabs one of its points.
            let custom = if w.mesh.is_none() || w.style != WarpStyle::Custom { Warp::custom(w.to_mesh(1, 1), w.bounds) } else { w.clone() };
            let pts0 = custom.mesh.as_ref().map(|m| m.points.clone()).unwrap_or_default();
            let at = pos2((x * zoom) as f32, (y * zoom) as f32);
            pv.warp_drag = with_grid(&custom, None, mods, |g, _| g.press(&view, style, at, mods)).flatten().map(|press| match press.target() {
                WarpTarget::Point(_) => WarpDrag::Point { press, pts0 },
                WarpTarget::Surface(..) => WarpDrag::Surface { press, pts0 },
            });
            if pv.warp_drag.is_some() {
                *w = custom;
            }
        }
        ToolEvent::Move { x, y, .. } | ToolEvent::Up { x, y } => {
            let at = pos2((x * zoom) as f32, (y * zoom) as f32);
            let edit = match &mut pv.warp_drag {
                Some(WarpDrag::Point { press, .. } | WarpDrag::Surface { press, .. }) => {
                    with_grid(w, None, mods, |g, _| g.track(&view, press, at).edit).flatten()
                }
                None => None,
            };
            match (&pv.warp_drag, edit, w.mesh.as_mut()) {
                (Some(WarpDrag::Point { pts0, .. }), Some(WarpEdit::Point { index, by }), Some(m)) => m.drag_point(pts0, index, [by.x, by.y]),
                (Some(WarpDrag::Surface { pts0, .. }), Some(WarpEdit::Surface { s, t, by }), Some(m)) => m.pull(pts0, s, t, [by.x, by.y]),
                _ => {}
            }
            if matches!(ev, ToolEvent::Up { .. }) {
                pv.warp_drag = None;
            }
        }
    }
}

/// Runs a warp-edit command against the mesh in the open Warp session and writes the mesh back.
pub fn edit_session_warp(app: &mut PhotocraftApp, id: &str, params: &serde_json::Value) -> Result<(), String> {
    let Some(w) = app.ui.transform.as_ref().and_then(|t| t.warp.clone()) else { return Err("not warping".into()) };
    if warps_type(app) {
        return Err("Warp on type takes only the warp styles".into());
    }
    let mut p = if params.is_object() { params.clone() } else { json!({}) };
    if p.get("warp").is_none() {
        p["warp"] = serde_json::to_value(&w).map_err(|e| e.to_string())?;
    }
    let r = app.session.execute(id, p).map_err(|e| e.to_string())?;
    let raw = r.get("warp").cloned().ok_or_else(|| "warp command returned no mesh".to_string())?;
    let nw: Warp = serde_json::from_value(raw).map_err(|e| e.to_string())?;
    // A warp-edit command can replace the mesh while pointer input is split across control
    // requests. Any drag index captured from the old mesh is invalid once that happens.
    if let Some(pv) = app.transform_preview.as_mut() {
        pv.warp_drag = None;
    }
    if let Some(t) = app.ui.transform.as_mut() {
        t.warp = Some(nw);
    }
    Ok(())
}

/// Applies a split-warp command to the mesh being edited (at the box centre unless `at`).
pub fn split(app: &mut PhotocraftApp, id: &str, at: Option<[f64; 2]>) -> Result<(), String> {
    let mut p = json!({});
    if let Some(a) = at {
        p["at"] = json!(a);
    }
    edit_session_warp(app, id, &p)
}
