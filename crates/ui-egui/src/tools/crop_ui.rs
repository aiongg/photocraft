//! Crop tool gestures. In Photoshop's "Classic Mode" (Use Classic Mode, P) the frame moves and
//! turns over a fixed image, as described below; in its default mode (`crop_mode`) the box stays
//! upright on screen and the same gestures inside and outside it move and turn the image behind
//! it instead (the view turns with the frame, so hit-testing in document space is unchanged):
//!
//! - with no frame, or inside the untouched default frame, a drag draws a new frame; ⇧ makes it
//!   square, ⌥ draws it from the centre, and holding Space while drawing repositions the frame
//!   being drawn;
//! - inside the frame, a drag moves it;
//! - on an edge or corner, a drag resizes it along the frame's own (possibly turned) axes; ⇧ keeps
//!   the frame's aspect ratio, ⌥ resizes about the centre, and an options-bar ratio preset always
//!   holds;
//! - outside the frame (off its handles) the pointer is the rotate cursor and a drag turns the
//!   frame about its centre, ⇧ in 15° steps, with the angle beside the pointer (#1792). As in
//!   Photoshop, a new frame is drawn after the pending one is committed or cancelled;
//! - while a frame is drawn or resized, its W × H (in px, as ↵ would crop it) shows beside the
//!   pointer, like the marquee's readout ([`sizing`]);
//! - with a pending frame and no gesture, the arrow keys nudge it 1 document px (⇧ 10 px) instead
//!   of the selection or layer, and X swaps it between portrait and landscape about its centre
//!   (keeping its angle, and swapping an options-bar ratio with it) instead of the colours
//!   ([`keys`], #1919). In the default mode the arrows move the image under the box instead.
//!
//! ↵ commits (`image.crop`, see `canvas::commit_crop`, with the turn as its `angle`: the document
//! is rotated so the frame is upright, then cropped to it) and Esc cancels. The pending frame lives
//! in `UiState::crop_rect` (the frame before its turn) and `UiState::crop_angle` (degrees,
//! clockwise on screen), so the control channel reads both. Both modes commit the same crop.
//!
//! As in Photoshop, from the first press until the crop is committed or cancelled the canvas also
//! shows the layers' pixels past its edges (kept by a crop with Delete Cropped Pixels off, or moved
//! out), with transparency out to the frame, under the shield ([`shows_beyond_canvas`]).

use std::sync::OnceLock;

use craft::draw::ui::{CropBox, CropFrame, CropNames, CropPress, CropPressKind, CropRelease, CropStyle};
use craft::ui::canvas::quad::BoxPart;
use craft::ui::canvas::{CanvasView, kurbo::Point, kurbo::Rect as KRect};
use craft::ui::theme::StrokeExt;
use craft::ui::theme::ToEgui;
use egui::{CursorIcon, Modifiers};

use photocraft_engine::snap::SnapLine;

use crate::PhotocraftApp;
use crate::canvas::{ToolEvent, ViewXform};
use crate::state::Tool;

/// Crop tool pointer state (not serialized: it only exists during a gesture).
#[derive(Clone, Debug, Default)]
pub struct CropState {
    pub drag: Option<CropDrag>,
    /// Space is held: drawing a new frame moves it instead of sizing it.
    pub space: bool,
    /// `UiState::crop_rect` is the untouched default frame ([`ensure_frame`]): a drag inside it
    /// draws a new frame (as in Photoshop) instead of moving it, and committing it does nothing.
    pub default_frame: bool,
    /// The document (index and size) the default frame was made for.
    frame_for: Option<(usize, u32, u32)>,
    /// The frame is being edited (pressed since it was made): the canvas shows what lies past it.
    pub editing: bool,
    /// The document the pending frame (or gesture) belongs to ([`cancel_stale`], #1918).
    doc: Option<photocraft_doc::DocId>,
    /// The options-bar Straighten button and ⌘-drag (`crop_straighten`).
    pub straighten: crate::crop_straighten::Straighten,
    /// The box's own Undo/Redo while a crop is pending (Photoshop keeps Edit › Undo live during a
    /// crop, so it must step the box, not the document — #3145): frame snapshots.
    undo: Vec<CropStep>,
    redo: Vec<CropStep>,
    /// The frame at the current gesture's press; pushed to `undo` on release when the box changed.
    press_step: Option<CropStep>,
    /// What the last pointer position snapped to: the release has no snapping of its own (the
    /// canvas ends it before the tools see the release), so it ends the drag where the last move did.
    snap: Option<crate::snap_ui::CropSnap>,
    /// How the Straighten line in progress snaps, decided at its press from the frame state.
    pub(crate) line_snap: crate::snap_ui::LineSnap,
}

/// One frame state of the crop box's own Undo/Redo.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CropStep {
    rect: Option<[f64; 4]>,
    angle: f64,
    default_frame: bool,
}

/// A crop gesture in progress: the library frame's press (its [`CropPress::kind`] is what the press picked).
#[derive(Clone, Copy, Debug)]
pub struct CropDrag {
    pub press: CropPress,
    /// The view's camera at the press: `ui.pointer` points are read in it (`crop_mode::control_point`).
    pub view: Option<crate::crop_mode::PressView>,
}

/// The centre of `[x0, y0, x1, y1]`.
pub fn center(r: [f64; 4]) -> [f64; 2] {
    [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
}

/// `p` turned `deg` degrees clockwise on screen (y down) about `c`.
pub fn turn(p: [f64; 2], c: [f64; 2], deg: f64) -> [f64; 2] {
    let (s, k) = deg.to_radians().sin_cos();
    let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
    [c[0] + k * dx - s * dy, c[1] + s * dx + k * dy]
}

/// The corners of frame `r` turned `deg` degrees about its centre: top-left, top-right,
/// bottom-right, bottom-left (of the upright frame).
pub fn corners(r: [f64; 4], deg: f64) -> [[f64; 2]; 4] {
    let c = center(r);
    [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]].map(|p| turn(p, c, deg))
}

/// The axis-aligned bounds `[x0, y0, x1, y1]` of frame `r` turned `deg` degrees.
pub fn turned_bounds(r: [f64; 4], deg: f64) -> [f64; 4] {
    if deg == 0.0 {
        return r;
    }
    corners(r, deg)
        .iter()
        .fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])])
}

/// An angle in degrees brought into (-180, 180]; 0 for anything not finite.
pub fn normalized_angle(deg: f64) -> f64 {
    if !deg.is_finite() {
        return 0.0;
    }
    let a = deg.rem_euclid(360.0);
    if a > 180.0 { a - 360.0 } else { a }
}

/// The options-bar ratio preset as width / height (W : H in W x H x Resolution mode).
pub(crate) fn preset_ratio(app: &PhotocraftApp) -> Option<f64> {
    if app.ui.tool_options.crop_ratio == crate::crop_size::WHR {
        let dpi = app.session.active().map_or(72.0, |s| f64::from(s.doc.resolution_dpi));
        let pt_per_in = app.session.prefs().units_and_rulers.point_size.per_inch();
        return crate::crop_size::ratio(&app.ui.tool_options, dpi, pt_per_in);
    }
    let size = app.session.active().map_or((1.0, 1.0), |s| (s.doc.size.width as f64, s.doc.size.height as f64));
    crate::chrome_ui::crop_ratio(&app.ui.tool_options.crop_ratio, size.0, size.1).map(|(w, h)| w / h).filter(|k| k.is_finite() && *k > 0.0)
}

/// Largest centred rectangle of `ratio` contained in the current frame; never adds area.
fn fitted_ratio(r: [f64; 4], ratio: f64) -> Option<[f64; 4]> {
    let (w, h) = (r[2] - r[0], r[3] - r[1]);
    if !r.iter().all(|v| v.is_finite()) || !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 || !ratio.is_finite() || ratio <= 0.0 {
        return None;
    }
    if ((w / h) / ratio - 1.0).abs() < 1e-12 {
        return Some(r);
    }
    let c = [r[0] + w / 2.0, r[1] + h / 2.0];
    let (w, h) = if w / h > ratio { (h * ratio, h) } else { (w, w / ratio) };
    let fitted = [c[0] - w / 2.0, c[1] - h / 2.0, c[0] + w / 2.0, c[1] + h / 2.0];
    (fitted.iter().all(|v| v.is_finite()) && fitted[2] > fitted[0] && fitted[3] > fitted[1]).then_some(fitted)
}

/// Apply committed options to the preview now, before a resize or confirmation is needed.
pub(crate) fn apply_ratio(app: &mut PhotocraftApp) {
    if app.ui.tool != Tool::Crop || app.crop.drag.is_some() {
        return;
    }
    frame(app);
    fit_frame(app);
    crate::crop_mode::sync_frame(app);
}

/// [`apply_ratio`] for a ratio being typed: fits from `base`, the frame the edit started with,
/// not from the frame the last keystroke's ratio left (#7752).
pub(crate) fn apply_ratio_from(app: &mut PhotocraftApp, base: [f64; 4]) {
    if app.ui.tool != Tool::Crop || app.crop.drag.is_some() {
        return;
    }
    if let Some(ratio) = preset_ratio(app)
        && let Some(fitted) = fitted_ratio(base, ratio)
        && app.ui.crop_rect != Some(fitted)
    {
        keyed(app, fitted);
    }
    crate::crop_mode::sync_frame(app);
}

fn fit_frame(app: &mut PhotocraftApp) {
    let dpi = app.session.active().map_or(72.0, |s| f64::from(s.doc.resolution_dpi));
    let pt_per_in = app.session.prefs().units_and_rulers.point_size.per_inch();
    let sized = crate::crop_size::target(&app.ui.tool_options, dpi, pt_per_in).is_some();
    if let Some(r) = app.ui.crop_rect
        && let Some(ratio) = preset_ratio(app)
        && let Some(fitted) = fitted_ratio(r, ratio)
        // W x H x Resolution can change output size even when the frame already fits.
        && (fitted != r || sized)
    {
        // #6750: fitting the untouched default frame to a ratio is not an edit: the user only
        // picked the tool, so leaving it drops the frame instead of cropping to it, as with the
        // plain default frame. A W x H x Resolution output size is an explicit instruction and
        // stays a pending crop even on the default frame.
        if fitted != r && !sized && app.crop.default_frame {
            app.ui.crop_rect = Some(fitted);
        } else {
            keyed(app, fitted);
        }
    }
}

/// Space is held (set by the canvas each frame, or by `ui.pointer`'s `space` flag).
pub fn set_space(app: &mut PhotocraftApp, down: bool) {
    app.crop.space = down;
}

/// As in Photoshop, the Crop tool always shows a frame: on picking the tool, and after a crop is
/// cancelled or committed, it frames the selection's bounds, or the whole canvas. In default mode
/// the view then follows the frame being edited (`crop_mode::sync`).
pub fn ensure_frame(app: &mut PhotocraftApp) {
    frame(app);
    crate::crop_mode::sync(app, None);
}

fn frame(app: &mut PhotocraftApp) {
    if app.ui.tool != Tool::Crop {
        app.crop.editing = false;
        crate::crop_straighten::reset(app);
        // Leaving the tool drops an untouched default frame and, as in Photoshop, commits a
        // drawn one (#3368).
        if app.crop.default_frame {
            app.ui.crop_rect = None;
            app.crop.default_frame = false;
        }
        if app.ui.crop_rect.is_some() && !keeps_pending(app) {
            commit_on_leave(app);
        }
        if app.ui.crop_rect.is_none() {
            app.ui.crop_angle = 0.0;
        }
        return;
    }
    // No frame (cancelled or committed): the next one starts upright.
    if app.ui.crop_rect.is_none() && app.crop.drag.is_none() {
        app.ui.crop_angle = 0.0;
    }
    let key = app.session.active_index().zip(app.session.active()).map(|(i, st)| (i, st.doc.size.width, st.doc.size.height));
    // Another document (or a resized one) gets its own default frame.
    if app.crop.default_frame && app.crop.frame_for != key && app.crop.drag.is_none() {
        app.ui.crop_rect = None;
    }
    if app.ui.crop_rect.is_some() || app.crop.drag.is_some() {
        return;
    }
    let Some(st) = app.session.active() else { return };
    let canvas = st.doc.bounds();
    // Computed once per new frame, not per frame drawn.
    let r = st.doc.selection.as_ref().map(|s| s.content_bounds().intersect(&canvas)).filter(|b| !b.is_empty()).unwrap_or(canvas);
    if r.is_empty() {
        return;
    }
    app.ui.crop_rect = Some([f64::from(r.x0), f64::from(r.y0), f64::from(r.x1), f64::from(r.y1)]);
    app.crop.default_frame = true;
    app.crop.frame_for = key;
    app.crop.editing = false;
    app.crop.undo.clear();
    app.crop.redo.clear();
    app.crop.press_step = None;
    claim(app);
    fit_frame(app);
}

/// Another tool is current but the drawn frame stays pending: a gesture on it is still in
/// progress, or the pen's eraser end stands in for the Crop tool until the tip comes back
/// (`stylus`). The held tools (Space, ⌘…, `hold_keys`) never change the current tool.
fn keeps_pending(app: &PhotocraftApp) -> bool {
    app.crop.drag.is_some() || app.stylus.tool_before_eraser == Some(Tool::Crop)
}

/// Commits the drawn frame on leaving the Crop tool as ↵ would with the tool still picked (so the
/// view goes upright too, `crop_mode::upright`), then gives the new tool back. A refused crop
/// keeps its frame: that is cancelled, so the new tool doesn't retry it every frame.
fn commit_on_leave(app: &mut PhotocraftApp) {
    let tool = std::mem::replace(&mut app.ui.tool, Tool::Crop);
    crate::canvas::commit_crop(app);
    if app.ui.crop_rect.is_some() {
        cancel(app);
    }
    app.ui.tool = tool;
}

/// A crop gesture is in progress: Space repositions the frame rather than panning.
pub fn active(app: &PhotocraftApp) -> bool {
    app.ui.tool == Tool::Crop && app.crop.drag.is_some()
}

/// Cancel the pending crop: the frame, any gesture on it and the default-frame state. With the
/// Crop tool still picked, [`ensure_frame`] then makes the active document a new default frame.
pub fn cancel(app: &mut PhotocraftApp) {
    crate::crop_mode::upright(app);
    app.ui.crop_rect = None;
    app.ui.crop_angle = 0.0;
    app.crop.drag = None;
    app.crop.default_frame = false;
    app.crop.frame_for = None;
    app.crop.editing = false;
    clear_history(app);
    crate::crop_straighten::reset(app);
}

/// Drop the box's own Undo/Redo (leaving the tool, cancelling or committing a crop).
pub fn clear_history(app: &mut PhotocraftApp) {
    app.crop.undo.clear();
    app.crop.redo.clear();
    app.crop.press_step = None;
}

/// The frame's current state, as an undo step.
fn step_of(app: &PhotocraftApp) -> CropStep {
    CropStep { rect: app.ui.crop_rect, angle: app.ui.crop_angle, default_frame: app.crop.default_frame }
}

/// Put a frame state back.
fn apply_step(app: &mut PhotocraftApp, s: CropStep) {
    app.ui.crop_rect = s.rect;
    app.ui.crop_angle = s.angle;
    app.crop.default_frame = s.default_frame;
}

/// The pending frame, its box Undo/Redo and the view's turn, kept while `image.crop` runs so a
/// refused commit gives them back ([`restore`]).
pub(crate) struct Kept {
    step: CropStep,
    undo: Vec<CropStep>,
    redo: Vec<CropStep>,
    view: Option<(f32, [f32; 2])>,
}

pub(crate) fn keep(app: &PhotocraftApp) -> Kept {
    let view = app.session.active_index().and_then(|i| app.ui.views.get(i)).map(|v| (v.rotation, v.center));
    Kept { step: step_of(app), undo: app.crop.undo.clone(), redo: app.crop.redo.clone(), view }
}

pub(crate) fn restore(app: &mut PhotocraftApp, k: Kept) {
    apply_step(app, k.step);
    (app.crop.undo, app.crop.redo) = (k.undo, k.redo);
    if let Some((rotation, center)) = k.view
        && let Some(v) = app.session.active_index().and_then(|i| app.ui.views.get_mut(i))
    {
        (v.rotation, v.center) = (rotation, center);
    }
}

/// While the Crop tool has box history, Undo/Redo step the *box*, not the document (Photoshop keeps
/// Edit › Undo live during a crop, #1918/#3145). `Some(enabled)` for those commands, else `None` so
/// the caller falls back to document history. While either box stack has a step, `intercept` takes
/// both commands, so one with nothing left to step greys out rather than offering the document's.
pub fn is_enabled(app: &PhotocraftApp, id: &str) -> Option<bool> {
    if app.ui.tool != Tool::Crop || (app.crop.undo.is_empty() && app.crop.redo.is_empty()) {
        return None;
    }
    match id {
        "edit.undo" => Some(!app.crop.undo.is_empty()),
        "edit.redo" => Some(!app.crop.redo.is_empty()),
        "edit.toggleLastState" => Some(false),
        _ => None,
    }
}

/// Intercept Undo/Redo/Toggle Last State while the Crop tool owns box history, for every caller
/// (menus, shortcuts, the control channel, MCP) — as `transform_tool::intercept` does.
pub fn intercept(app: &mut PhotocraftApp, id: &str) -> Option<Result<serde_json::Value, String>> {
    if app.ui.tool != Tool::Crop || (app.crop.undo.is_empty() && app.crop.redo.is_empty()) {
        return None;
    }
    match id {
        "edit.undo" => Some(Ok(step(app, false))),
        "edit.redo" => Some(Ok(step(app, true))),
        "edit.toggleLastState" => Some(Err("Commit or cancel the crop first".into())),
        _ => None,
    }
}

/// Undo (or redo) one box step.
fn step(app: &mut PhotocraftApp, redo: bool) -> serde_json::Value {
    let s = if redo { app.crop.redo.pop() } else { app.crop.undo.pop() };
    let Some(s) = s else { return serde_json::Value::Null };
    let now = step_of(app);
    apply_step(app, s);
    if redo {
        app.crop.undo.push(now);
    } else {
        app.crop.redo.push(now);
    }
    // Default mode: the box stays put on screen, so the view follows the frame's centre (#7088).
    if let (true, Some(a), Some(b)) = (crate::crop_mode::moves_image(app), now.rect, s.rect) {
        let (a, b) = (center(a), center(b));
        let d = [(b[0] - a[0]) as f32, (b[1] - a[1]) as f32];
        if let Some(v) = app.session.active_index().and_then(|i| app.ui.views.get_mut(i)) {
            let c = [v.center[0] + d[0], v.center[1] + d[1]];
            if c.iter().all(|x| x.is_finite()) {
                v.center = c;
            }
        }
    }
    crate::crop_mode::sync_frame(app);
    crate::crop_mode::auto_center(app);
    serde_json::Value::Null
}

/// The pending frame belongs to the active document.
fn claim(app: &mut PhotocraftApp) {
    app.crop.doc = app.session.active().map(|st| st.doc.id);
}

/// A crop is pending: the Crop tool's frame on the active document has been set (drawn, moved,
/// resized or turned), not just the untouched default frame the tool shows.
pub fn pending(app: &PhotocraftApp) -> bool {
    app.ui.tool == Tool::Crop
        && app.ui.crop_rect.is_some()
        && !app.crop.default_frame
        && (app.crop.doc.is_none() || app.crop.doc == app.session.active().map(|st| st.doc.id))
}

/// Menu commands Photoshop greys out while a crop is pending (#1918), until it is committed or
/// cancelled. Checked item by item in Photoshop 27.11: everything except Close, Save, Save As,
/// Save a Copy; Undo, Redo, Toggle Last State, Search; Image › Crop (which commits the frame); the
/// View menu but Proof Setup, Pixel Aspect Ratio (Correction), 32-bit Preview Options, Flip
/// Horizontal and Screen Mode; the Window menu but Workspace and Adjustments; Help but System Info.
pub fn blocks(app: &PhotocraftApp, id: &str) -> bool {
    pending(app) && !available_while_pending(id)
}

fn available_while_pending(id: &str) -> bool {
    if let Some(v) = id.strip_prefix("view.") {
        let greyed = ["proofSetup.", "pixelAspectRatio", "thirtyTwoBitPreviewOptions", "flipHorizontal", "screenMode."];
        return !greyed.iter().any(|g| v.starts_with(g));
    }
    if let Some(w) = id.strip_prefix("window.") {
        return !(w.starts_with("workspace.") || w == "panel.adjustments");
    }
    if id.starts_with("help.") {
        return id != "help.systemInfo";
    }
    matches!(
        id,
        "file.close"
            | "file.save"
            | "file.saveAs"
            | "file.saveACopy"
            | "file.exit"
            | "edit.undo"
            | "edit.redo"
            | "edit.toggleLastState"
            | "edit.search"
            | "image.crop"
    )
}

/// A pending crop belongs to the document it was drawn on (#1918): when another document becomes
/// active (File › New, Open, a tab switch, closing the document) it is cancelled, so it never
/// shows on or crops another document. (As in Photoshop: switching to another document discards
/// the pending crop, and that document gets the tool's own frame.) Run on every frame and command
/// (`sync_views`) and before a commit.
pub fn cancel_stale(app: &mut PhotocraftApp) {
    let active = app.session.active().map(|st| st.doc.id);
    let pending = app.ui.crop_rect.is_some() || app.crop.drag.is_some();
    // A frame made before any document was claimed (control channel, tests) is the active one's.
    if pending && app.crop.doc.is_some() && app.crop.doc != active {
        cancel(app);
    }
    app.crop.doc = active;
}

/// The frame is being edited, so the canvas shows the pixels past its edges (Photoshop's crop
/// preview): from the first press with the tool until the crop is committed or cancelled.
pub fn shows_beyond_canvas(app: &PhotocraftApp) -> bool {
    app.ui.tool == Tool::Crop && app.ui.crop_rect.is_some() && (app.crop.editing || app.crop.drag.is_some())
}

/// The button went down on the canvas with the Crop tool: the frame is being edited from now on.
/// Returns true when that is new.
pub fn press(app: &mut PhotocraftApp) -> bool {
    let new = app.ui.tool == Tool::Crop && app.ui.crop_rect.is_some() && !app.crop.editing;
    if new {
        app.crop.editing = true;
    }
    new
}

/// The pending frame's angle (degrees), 0 when it isn't a finite number.
pub fn angle(app: &PhotocraftApp) -> f64 {
    normalized_angle(app.ui.crop_angle)
}

/// The part of the frame under document point `p` (None while Straighten is armed).
fn part_at(app: &PhotocraftApp, p: [f64; 2]) -> Option<BoxPart> {
    let b = crop_box(app)?;
    let (view, at) = view_and_screen(app, p)?;
    library_frame(app, Some(&b), Modifiers::NONE, app.crop.straighten.armed).hit(&view, reach(), at)
}

/// A press at document point `p` would turn the frame (Crop tool, outside the frame and its
/// handles) rather than draw, move or resize one.
pub fn turns_at(app: &PhotocraftApp, p: [f64; 2]) -> bool {
    app.ui.tool == Tool::Crop && part_at(app, p) == Some(BoxPart::Outside)
}

/// A double-click at document point `p` commits the crop (Photoshop: double-click inside the box,
/// like ↵): Crop tool, a pending frame, and `p` inside it (not on a handle, not outside).
pub fn commits_at(app: &PhotocraftApp, p: [f64; 2]) -> bool {
    if app.ui.tool != Tool::Crop || app.crop.drag.is_some() {
        return false;
    }
    let (Some(b), Some((view, at))) = (crop_box(app), view_and_screen(app, p)) else { return false };
    library_frame(app, Some(&b), Modifiers::NONE, false).double_click(&view, reach(), at)
}

/// The frame is being turned: the angle for the readout beside the pointer. Classic Mode shows
/// the frame's turn; the default mode the image's (clockwise positive, as Photoshop 25.1 does).
pub fn turning(app: &PhotocraftApp) -> Option<f64> {
    match app.crop.drag {
        Some(d) if d.press.kind() == CropPressKind::Turn && crate::crop_mode::moves_image(app) => Some(crate::crop_mode::image_turn(angle(app))),
        Some(d) if d.press.kind() == CropPressKind::Turn => Some(angle(app)),
        _ => None,
    }
}

/// The size ↵ would crop frame `r` to: its own width and height (before any turn) in whole
/// document px, at least 1; `None` when they aren't finite.
pub fn frame_size(r: [f64; 4]) -> Option<[f64; 2]> {
    let (w, h) = (r[2] - r[0], r[3] - r[1]);
    (w.is_finite() && h.is_finite()).then(|| [w.round().max(1.0), h.round().max(1.0)])
}

/// The Crop tool owns the arrow keys and X: a pending frame and no gesture in progress.
pub fn takes_keys(app: &PhotocraftApp) -> bool {
    app.ui.tool == Tool::Crop && app.crop.drag.is_none() && app.ui.transform.is_none() && app.ui.crop_rect.is_some_and(|r| r.iter().all(|v| v.is_finite()))
}

/// The frame now set by the keys: a real frame (no longer the default), being edited.
fn keyed(app: &mut PhotocraftApp, r: [f64; 4]) {
    if r.iter().all(|v| v.is_finite()) {
        app.ui.crop_rect = Some(r);
        app.crop.default_frame = false;
        app.crop.editing = true;
    }
}

/// Moves the pending frame by (`dx`, `dy`) document px (a turned frame moves on the page, keeping
/// its angle); in default mode the image moves that way on screen under the box instead. Returns
/// false when the Crop tool doesn't take the keys ([`takes_keys`]).
pub fn nudge(app: &mut PhotocraftApp, dx: f64, dy: f64) -> bool {
    let Some(r) = app.ui.crop_rect.filter(|_| takes_keys(app)) else { return false };
    if crate::crop_mode::moves_image(app) {
        if dx.is_finite() && dy.is_finite() {
            app.crop.default_frame = false;
            app.crop.editing = true;
            crate::crop_mode::sync_frame(app);
            let d = crate::crop_mode::screen_to_doc_vec(app, [dx, dy]);
            crate::crop_mode::shift_image(app, d);
        }
        return true;
    }
    if dx.is_finite() && dy.is_finite() {
        keyed(app, [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy]);
    }
    true
}

/// Frame `r` with its width and height swapped about its centre.
pub fn swapped(r: [f64; 4]) -> [f64; 4] {
    let c = center(r);
    let (hw, hh) = ((r[2] - r[0]) / 2.0, (r[3] - r[1]) / 2.0);
    [c[0] - hh, c[1] - hw, c[0] + hh, c[1] + hw]
}

/// The options-bar ratio `key` with its sides swapped (what the bar's ⇄ button does), `None` when
/// no ratio is set; "original" becomes the document's size turned on its side.
pub fn swapped_ratio(key: &str, doc_w: f64, doc_h: f64) -> Option<String> {
    let (w, h) = crate::chrome_ui::crop_ratio(key, doc_w, doc_h).filter(|(w, h)| w.is_finite() && h.is_finite() && *w > 0.0 && *h > 0.0)?;
    Some(format!("{}:{}", crate::crop_size::fmt_ratio(h), crate::crop_size::fmt_ratio(w)))
}

/// X: swaps the pending frame between portrait and landscape about its centre, keeping its angle,
/// and swaps the options-bar ratio with it, as in Photoshop. Returns false when the Crop tool
/// doesn't take the keys ([`takes_keys`]), so X swaps the colours as usual.
pub fn swap_orientation(app: &mut PhotocraftApp) -> bool {
    let Some(r) = app.ui.crop_rect.filter(|_| takes_keys(app)) else { return false };
    let size = app.session.active().map_or((0.0, 0.0), |s| (f64::from(s.doc.size.width), f64::from(s.doc.size.height)));
    // W x H x Resolution swaps W and H instead of a ratio.
    if !crate::crop_size::swap(&mut app.ui.tool_options)
        && let Some(k) = swapped_ratio(&app.ui.tool_options.crop_ratio, size.0, size.1)
    {
        app.ui.tool_options.crop_ratio = k;
    }
    // The frame turns first: fitting the swapped ratio inside the old frame would shrink it.
    keyed(app, swapped(r));
    if preset_ratio(app).is_some() {
        apply_ratio(app);
    } else {
        crate::crop_mode::sync_frame(app);
        crate::crop_mode::auto_center(app);
    }
    true
}

/// Crop tool keys with a pending frame: arrows nudge it 1 px (⇧ 10 px) instead of the selection
/// or layer, and X swaps its orientation instead of the colours. Returns true when a key was used.
pub fn keys(app: &mut PhotocraftApp, ctx: &egui::Context) -> bool {
    if !takes_keys(app) {
        return false;
    }
    use egui::Key;
    let arrows = [(Key::ArrowLeft, -1.0, 0.0), (Key::ArrowRight, 1.0, 0.0), (Key::ArrowUp, 0.0, -1.0), (Key::ArrowDown, 0.0, 1.0)];
    for (key, ux, uy) in arrows {
        // ⇧ first: `consume_key` ignores an extra ⇧.
        for mods in [Modifiers::SHIFT, Modifiers::NONE] {
            if ctx.input_mut(|i| i.consume_key(mods, key)) {
                let k = if mods.shift { 10.0 } else { 1.0 };
                return nudge(app, ux * k, uy * k);
            }
        }
    }
    ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::X)) && swap_orientation(app)
}

/// The library's frame for the pending box `state`: the options bar's ratio, the default-frame and
/// mode flags, Straighten and the host's snapping.
fn library_frame<'a>(app: &PhotocraftApp, state: Option<&'a CropBox>, mods: Modifiers, straighten: bool) -> CropFrame<'a> {
    match state {
        Some(state) => CropFrame::new("crop-tool", state, CropNames::default()),
        None => CropFrame::unframed("crop-tool", CropNames::default()),
    }
    .ratio(preset_ratio(app))
    .default_frame(app.crop.default_frame)
    .straighten(straighten)
    .image_mode(crate::crop_mode::moves_image(app))
    .auto_center(app.ui.tool_options.crop_shield.auto_center_preview)
    .modifiers(mods)
}

/// The library's sizes (`crop.*`): the same in every theme, so hit tests read them without a context.
pub(crate) fn reach() -> &'static CropStyle {
    static STYLE: OnceLock<CropStyle> = OnceLock::new();
    STYLE.get_or_init(|| CropFrame::style(&craft::ui::theme::current(&egui::Context::default())))
}

/// The pending frame as the library's box (None when it isn't a finite frame).
pub(crate) fn crop_box(app: &PhotocraftApp) -> Option<CropBox> {
    let r = app.ui.crop_rect.filter(|r| r.iter().all(|v| v.is_finite()))?;
    Some(CropBox { rect: KRect::new(r[0], r[1], r[2], r[3]), angle: angle(app) })
}

fn apply_box(app: &mut PhotocraftApp, b: &CropBox) {
    app.ui.crop_rect = Some([b.rect.x0, b.rect.y0, b.rect.x1, b.rect.y1]);
    app.ui.crop_angle = b.angle;
}

/// The handle (`hx`, `hy`) of a corner or edge part.
pub(crate) fn part_handle(part: BoxPart) -> Option<(i8, i8)> {
    match part {
        BoxPart::Corner(i) => Some([(-1, -1), (1, -1), (1, 1), (-1, 1)][usize::from(i % 4)]),
        BoxPart::Edge(i) => Some([(0, -1), (1, 0), (0, 1), (-1, 0)][usize::from(i % 4)]),
        _ => None,
    }
}

/// The active view as the library's, and a point of it in screen space.
fn view_and_screen(app: &PhotocraftApp, p: [f64; 2]) -> Option<(CanvasView, egui::Pos2)> {
    let view = ViewXform::library_view(app);
    let at = view.to_screen(Point::new(p[0], p[1]));
    (at.x.is_finite() && at.y.is_finite()).then_some((view, at))
}

/// Writes the library view's camera back to the active document's view when a drag moved it.
fn follow_view(app: &mut PhotocraftApp, before: &CanvasView, view: &CanvasView) {
    if (before.center, before.rotation) == (view.center, view.rotation) {
        return;
    }
    let (center, rotation) = ([view.center.x as f32, view.center.y as f32], view.rotation as f32);
    if let Some(v) = app.session.active_index().and_then(|i| app.ui.views.get_mut(i))
        && center.iter().all(|c| c.is_finite())
        && rotation.is_finite()
    {
        (v.center, v.rotation) = (center, rotation);
    }
}

/// The frame with the host's snapping: what its closure reaches goes to `lines`. The library snaps
/// a Straighten line's end points through it too, as point gestures.
fn snapping<'a>(
    frame: CropFrame<'a>,
    snap: Option<&'a crate::snap_ui::CropSnap>,
    straighten: bool,
    lines: &'a std::cell::RefCell<Vec<SnapLine>>,
) -> CropFrame<'a> {
    match snap {
        Some(snap) => frame.snap(move |part, state, _| {
            let (d, found) = if straighten { snap.line_vector([state.rect.x0, state.rect.y0]) } else { snap.vector(part, state) };
            *lines.borrow_mut() = found;
            d
        }),
        None => frame,
    }
}

/// Pointer input for the Crop tool. Returns true when the event was its.
pub fn pointer(app: &mut PhotocraftApp, ev: ToolEvent, mods: Modifiers) -> bool {
    if app.ui.tool != Tool::Crop {
        app.crop.drag = None;
        return false;
    }
    let p = match ev {
        ToolEvent::Down { x, y, .. } | ToolEvent::Move { x, y, .. } | ToolEvent::Up { x, y } => [x, y],
    };
    if app.session.active().is_none() || !p.iter().all(|v| v.is_finite()) {
        return true;
    }
    match ev {
        ToolEvent::Down { .. } => press_at(app, p, mods),
        ToolEvent::Move { .. } => track_to(app, p, mods, false),
        ToolEvent::Up { .. } => release_at(app, p, mods),
    }
    true
}

/// Starts the gesture a press at `p` picks: resize, move, turn, draw or a Straighten line.
fn press_at(app: &mut PhotocraftApp, p: [f64; 2], mods: Modifiers) {
    // A Straighten line whose release went elsewhere (Space's Hand took it) never continues here.
    let had_drag = app.crop.drag.take().is_some_and(|d| d.press.kind() != CropPressKind::Straighten);
    // A ⌘-drag in the middle of a crop gesture doesn't hijack it.
    let straight = !had_drag && (app.crop.straighten.armed || mods.command);
    if straight {
        // The Straighten line's snapping is decided from the frame as it was before this press
        // touched it, as the pointer's snapping is.
        let inside = crop_box(app)
            .zip(view_and_screen(app, p))
            .is_some_and(|(b, (view, at))| library_frame(app, Some(&b), mods, false).hit(&view, reach(), at) == Some(BoxPart::Inside));
        app.crop.line_snap = crate::snap_ui::line_snap(app, p, inside);
    }
    claim(app);
    app.crop.editing = app.ui.crop_rect.is_some();
    app.crop.press_step = Some(step_of(app));
    // Default mode: the view stays upright for the frame, about the pressed point.
    crate::crop_mode::sync(app, Some(p));
    let Some((view, at)) = view_and_screen(app, p) else { return };
    // No frame yet: the press draws one.
    let b = crop_box(app);
    if b.is_none() {
        app.ui.crop_angle = 0.0;
    }
    let snap = crate::snap_ui::crop_snap(app);
    let lines = std::cell::RefCell::new(Vec::new());
    let frame = snapping(library_frame(app, b.as_ref(), mods, straight).default_frame(app.crop.default_frame), snap.as_ref(), straight, &lines);
    let Some(press) = frame.press(&view, reach(), at) else { return };
    drop(frame);
    if snap.is_some() && press.kind() == CropPressKind::Straighten {
        app.prefs_rt.snap_lines = lines.into_inner();
    }
    app.crop.editing = true;
    app.crop.drag = Some(CropDrag { press, view: crate::crop_mode::press_view(app) });
}

/// The frame follows the pointer at document point `p`.
fn track_to(app: &mut PhotocraftApp, p: [f64; 2], mods: Modifiers, release: bool) {
    let Some(mut drag) = app.crop.drag else { return };
    let now = crate::snap_ui::crop_snap(app);
    if !release {
        app.crop.snap = now.clone();
    }
    let snap = now.or_else(|| app.crop.snap.clone().filter(|_| release));
    let (Some(b), Some((mut view, at))) = (crop_box(app).or_else(|| Some(*drag.press.last())), view_and_screen(app, p)) else { return };
    let before = view;
    let lines = std::cell::RefCell::new(Vec::new());
    let frame = snapping(library_frame(app, Some(&b), mods, false), snap.as_ref(), drag.press.kind() == CropPressKind::Straighten, &lines);
    let track = frame.track(&mut view, &mut drag.press, at, mods, app.crop.space);
    drop(frame);
    if let Some(next) = track.state {
        apply_box(app, &next);
    }
    if snap.is_some() && !release {
        app.prefs_rt.snap_lines = lines.into_inner();
    }
    if crate::crop_mode::moves_image(app) {
        follow_view(app, &before, &view);
    }
    app.crop.drag = Some(drag);
    crate::crop_mode::sync_frame(app);
}

/// Ends the gesture: keeps a valid frame or restores the previous one, and records a box Undo step
/// when it changed.
fn release_at(app: &mut PhotocraftApp, p: [f64; 2], mods: Modifiers) {
    track_to(app, p, mods, true);
    let Some(drag) = app.crop.drag.take() else { return };
    let (Some(b), Some((mut view, at))) = (crop_box(app).or_else(|| Some(*drag.press.last())), view_and_screen(app, p)) else { return };
    let before = view;
    let lines = std::cell::RefCell::new(Vec::new());
    let frame = snapping(library_frame(app, Some(&b), mods, false), app.crop.snap.as_ref(), drag.press.kind() == CropPressKind::Straighten, &lines);
    let release = frame.release(&mut view, reach(), &drag.press, Some(at));
    drop(frame);
    if crate::crop_mode::moves_image(app) {
        follow_view(app, &before, &view);
    }
    let start = app.crop.press_step;
    match release {
        CropRelease::Commit(done) => {
            apply_box(app, &done);
            let held = |s: CropStep| match drag.press.kind() {
                CropPressKind::Turn => done.angle == s.angle,
                CropPressKind::Move if crate::crop_mode::moves_image(app) => app.ui.crop_rect == s.rect,
                _ => false,
            };
            if !start.is_some_and(held) {
                app.crop.default_frame = false;
            }
        }
        CropRelease::Cancel => {
            if let Some(s) = start {
                (app.ui.crop_rect, app.ui.crop_angle) = (s.rect, s.angle);
            }
        }
        CropRelease::Straighten(deg) => crate::crop_straighten::apply(app, deg),
        CropRelease::Nothing => {
            // A press that never moved: clicking on the frame is an edit of it, as before; the
            // turn zones and a held-still image are not.
            if !matches!(drag.press.kind(), CropPressKind::Turn | CropPressKind::Straighten)
                && !(drag.press.kind() == CropPressKind::Move && crate::crop_mode::moves_image(app))
            {
                app.crop.default_frame = false;
            }
        }
    }
    if drag.press.kind() == CropPressKind::Straighten {
        // One line per click of the button.
        app.crop.straighten.armed = false;
    }
    push_press_step(app);
    crate::crop_mode::sync_frame(app);
}

/// On release: the frame state at the press becomes a box Undo step when the gesture changed it.
fn push_press_step(app: &mut PhotocraftApp) {
    if let Some(before) = app.crop.press_step.take()
        && step_of(app) != before
    {
        app.crop.undo.push(before);
        app.crop.redo.clear();
    }
}

/// Cursor over the canvas at document point `p`: resize arrows on the edges, move inside (the
/// crosshair inside the untouched default frame, where a drag draws a new one).
pub fn cursor(app: &PhotocraftApp, p: [f64; 2]) -> Option<CursorIcon> {
    if app.ui.tool != Tool::Crop {
        return None;
    }
    if crate::crop_straighten::mode(app) {
        return Some(CursorIcon::Crosshair);
    }
    let deg = angle(app);
    let part = match app.crop.drag.map(|d| d.press.kind()) {
        Some(CropPressKind::Move) => BoxPart::Inside,
        Some(CropPressKind::Resize(part)) => part,
        Some(CropPressKind::Draw | CropPressKind::Straighten) => return Some(CursorIcon::Crosshair),
        Some(CropPressKind::Turn) => BoxPart::Outside,
        None => part_at(app, p)?,
    };
    Some(match part {
        // The arrow as the handle faces on screen: the frame's turn plus the view's, mirrored when
        // the view is flipped.
        BoxPart::Corner(_) | BoxPart::Edge(_) => {
            let (hx, hy) = part_handle(part)?;
            resize_icon(hx, hy, deg + f64::from(crate::crop_mode::view_rotation(app)), app.ui.view.flip_horizontal)
        }
        // Inside the untouched default frame a drag draws a new one (`pointer`), so nothing moves (#4461).
        BoxPart::Inside if app.crop.default_frame => CursorIcon::Crosshair,
        BoxPart::Inside => CursorIcon::Move,
        // Photoshop's curved two-headed arrow (`draw_turn_cursor`, from `turn_cursor_dir`).
        _ => CursorIcon::None,
    })
}

/// Where the turn cursor's arc bends at document point `p` (pointer outside the frame, or turning
/// it): towards the frame corner of `p`'s quadrant, along the diagonal of the frame's own axes.
/// As in Photoshop that gives four cursors, one per corner; they turn with the frame.
pub fn turn_cursor_dir(app: &PhotocraftApp, p: [f64; 2]) -> Option<[f64; 2]> {
    if app.ui.tool != Tool::Crop || crate::crop_straighten::mode(app) {
        return None;
    }
    let r = app.ui.crop_rect.filter(|r| r.iter().all(|v| v.is_finite()))?;
    let deg = angle(app);
    let turning = match app.crop.drag {
        Some(d) => d.press.kind() == CropPressKind::Turn,
        None => part_at(app, p) == Some(BoxPart::Outside),
    };
    if !(turning && p[0].is_finite() && p[1].is_finite()) {
        return None;
    }
    let c = center(r);
    let local = turn(p, c, -deg);
    let sx = if local[0] < c[0] { 1.0 } else { -1.0 };
    let sy = if local[1] < c[1] { 1.0 } else { -1.0 };
    Some(turn([sx * std::f64::consts::FRAC_1_SQRT_2, sy * std::f64::consts::FRAC_1_SQRT_2], [0.0, 0.0], deg))
}

/// Photoshop's turn cursor (Crop, and Free Transform's rotate zone) at screen point `p`: a short
/// arc with an arrowhead at each end, curving round the frame corner that lies in screen direction
/// `toward` (an original drawing, no proprietary cursor asset).
pub fn draw_turn_cursor(ctx: &egui::Context, p: egui::Pos2, toward: egui::Vec2) {
    let len = toward.length();
    if !len.is_finite() || len < 1e-6 {
        return;
    }
    let u = toward / len;
    let tokens = craft::ui::theme::current(ctx);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("crop-turn-cursor")));
    const R: f32 = 12.0;
    const HALF: f32 = 0.75;
    let c = p + u * R;
    let mid = (-u.y).atan2(-u.x);
    let at = |a: f32| c + egui::vec2(a.cos(), a.sin()) * R;
    let points: Vec<egui::Pos2> = (0..=16).map(|i| at(mid - HALF + 2.0 * HALF * i as f32 / 16.0)).collect();
    painter.add(egui::Shape::line(points.clone(), egui::Stroke::new(3.5, tokens.color.surface.shadow.to_egui())));
    painter.add(egui::Shape::line(points, craft::ui::theme::scale::Stroke::W1_5.egui(ctx, tokens.color.accent.text.to_egui())));
    for (a, sign) in [(mid - HALF, -1.0f32), (mid + HALF, 1.0)] {
        let tip = at(a) + egui::vec2(-a.sin(), a.cos()) * (sign * 2.0);
        let tangent = egui::vec2(-a.sin(), a.cos()) * sign;
        let normal = egui::vec2(a.cos(), a.sin());
        painter.add(egui::Shape::convex_polygon(
            vec![tip + tangent * 3.5, tip - tangent * 3.0 + normal * 3.5, tip - tangent * 3.0 - normal * 3.5],
            tokens.color.accent.text.to_egui(),
            craft::ui::theme::scale::Stroke::W1.egui(ctx, tokens.color.surface.shadow.to_egui()),
        ));
    }
}

/// The resize arrow for handle (`hx`, `hy`) of a frame turned `deg` degrees: the handle's outward
/// direction, turned with the frame and mirrored by a flipped view (`flip`), to the nearest of the
/// four arrows.
fn resize_icon(hx: i8, hy: i8, deg: f64, flip: bool) -> CursorIcon {
    let d = turn([f64::from(hx), f64::from(hy)], [0.0, 0.0], deg);
    // View › Flip Horizontal mirrors the screen after the turn.
    let d = if flip { [-d[0], d[1]] } else { d };
    let a = d[1].atan2(d[0]).rem_euclid(std::f64::consts::PI);
    match ((a / std::f64::consts::FRAC_PI_4).round() as i64).rem_euclid(4) {
        0 => CursorIcon::ResizeHorizontal,
        1 => CursorIcon::ResizeNwSe,
        2 => CursorIcon::ResizeVertical,
        _ => CursorIcon::ResizeNeSw,
    }
}

/// A press in progress for tests of the keys and overlays that a drag in progress leaves alone.
#[cfg(test)]
pub(crate) fn test_drag() -> CropDrag {
    let view = CanvasView::new(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0)), Point::ZERO, 1.0);
    let b = CropBox::upright(KRect::new(0.0, 0.0, 10.0, 10.0));
    let press = CropFrame::new("test", &b, CropNames::default()).press(&view, reach(), egui::pos2(5.0, 5.0)).expect("a finite frame");
    CropDrag { press, view: None }
}

#[cfg(test)]
mod tests;
