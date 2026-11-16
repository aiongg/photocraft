//! The canvas's tool dispatcher. Every tool is a framework `CanvasTool` built for one pass over the app; the framework's
//! `ToolHost` runs it (press arena, momentary Hand and Zoom, stand-ins, autoscroll, snapping, readouts). This module
//! picks the tool for the pass, wraps it in the press preflight, runs the host and hands what the host reports to the
//! families that keep state between passes.
//!
//! A family tool holds the [`ToolScope`] and its own state, never a `&mut PhotocraftApp`: the scope gives the app to one
//! method at a time (`with_app`), so the host can borrow the app's `Ui` and the tool in the same pass.

// PLAN: the sheet's Files row says "no `mod crop`", but canvas-widgets-w20's decision (cw.P5) makes `canvas/tools/crop.rs`
// the adapter `run` calls (`crop::pass`, `crop::finish`); the `mod` line is declared here.
pub(crate) mod crop;
pub(crate) mod dialog;
pub(crate) mod move_tool;
pub(crate) mod retouch;
pub(crate) mod selection;

use std::cell::RefCell;

use craft::behavior::geometry::SnapLine;
use craft::behavior::kurbo::Size;
use craft::behavior::{CancelReason, Cursor, GesturePhase, PointerButton};
use craft::draw::ui::{CropShown, CropToolPass};
use craft::keymap::Mods;
use craft::ui::canvas::{
    Autoscroll, CanvasOutput, CanvasTool, HandTool, Momentary, NavigationOptions, NoTool, RotateViewTool, StandInKey, ToolAccess, ToolAction, ToolClaims,
    ToolContext, ToolHost, ToolHostResponse, ToolKey, ToolPath, ToolPointer, ZoomTool,
};
use craft::ui::primitives::Draw;

use super::document_display::{self, CanvasHost};
use super::scroll_all_windows::{self, Source};
use crate::PhotocraftApp;
use crate::state::Tool;

/// A family tool built for one pass: it borrows the pass's [`ToolScope`] and its family's state.
pub(crate) type FamilyTool<'a> = Box<dyn CanvasTool + 'a>;

/// A tool borrowed for the host's run.
type DynTool<'x> = &'x mut (dyn CanvasTool + 'x);

/// Every canvas family's state between passes. `run` takes it out of the app for a pass (`std::mem::take`), so a family
/// reads its own state through the tool it is given, and readers outside a pass read the fields through the app.
// PLAN: each family module's `tool(scope, state, host)` / `drain(app, state, response)` path and state type follow cw.P1's
// family shape; the owners' sheets (w18 selection, w21 move, w22 retouch, w25 paint, w27 eyedropper, tools-w22 type,
// tools-w33 vector, tools-w44 gradient, tools-w51 analysis, tools-w66 artboard, tools-w6x frame, slice, perspective crop)
// fix the names.
pub(crate) struct CanvasTools {
    /// The Space, Cmd+Space and Alt+Space Hand and Zoom, held in place of the selected tool.
    pub(crate) momentary: Momentary,
    /// The Hand tool.
    pub(crate) hand: HandTool,
    /// The Zoom tool.
    pub(crate) zoom: ZoomTool,
    /// The Rotate View tool.
    pub(crate) rotate: RotateViewTool,
    /// The selection tools.
    pub(crate) selection: selection::SelectionTools,
    /// The Move tool, also the Command stand-in of the tools that take ⌘ as Move.
    pub(crate) mover: move_tool::MoveTool,
    /// The armed pick of the top dialog.
    pub(crate) dialog: dialog::DialogTools,
    /// Brush, pencil, eraser, bucket and the other painting tools.
    pub(crate) paint: super::paint_tool::PaintState,
    /// The Eyedropper, also the Alt stand-in of the painting tools that sample.
    pub(crate) eyedropper: super::eyedropper_tool::EyedropperState,
    /// Pen, Freeform, Curvature, Path Selection and the shape tools.
    pub(crate) vector: crate::vector_ui::VectorTools,
    /// Type, Vertical Type and the two Type Masks.
    pub(crate) type_tool: crate::type_tool::TypeState,
    /// The live Gradient.
    pub(crate) gradient: crate::gradient_ui::tool::GradientDraw,
    /// Ruler, Color Sampler, Count and Note.
    pub(crate) analysis: crate::analysis::canvas::AnalysisTools,
    /// The Artboard tool.
    pub(crate) artboard: craft::draw::ui::artboard::ArtboardTool,
    /// The Frame tool.
    pub(crate) frame: crate::frame_tool::FrameTool,
    /// The Slice and Slice Select tools.
    pub(crate) slice: craft::draw::ui::slice::SliceTool,
    /// Patch, Content-Aware Move, Red Eye, Object Selection and the Clone Stamp and Healing source pick.
    pub(crate) retouch: retouch::RetouchTools,
    /// The Perspective Crop tool.
    pub(crate) perspective: crate::perspective_crop_ui::PerspectiveCrop,
    /// The Crop tool.
    pub(crate) crop: crate::crop_ui::CropHost,
    /// The document and canvas a running gesture began on: the canvas pass ends it with `tool_changed` once another
    /// document is the one shown.
    gesture_on: Option<(u64, CanvasHost)>,
    /// The app context's frame loop for the canvas test driver (`canvas::testing`).
    #[cfg(test)]
    pub(crate) frames: Option<craft::ui::testing::Frames>,
    /// What the host reported in the main window's primary canvas pass, for the test driver's `tool_event`.
    #[cfg(test)]
    pub(crate) last_response: Option<ToolHostResponse>,
}

impl Default for CanvasTools {
    fn default() -> Self {
        let options = NavigationOptions::default();
        Self {
            momentary: Momentary::new(options.clone()),
            hand: HandTool::new(options.clone()),
            zoom: ZoomTool::new(options),
            rotate: RotateViewTool::default(),
            selection: Default::default(),
            mover: Default::default(),
            dialog: Default::default(),
            paint: Default::default(),
            eyedropper: Default::default(),
            vector: Default::default(),
            type_tool: Default::default(),
            gradient: Default::default(),
            analysis: Default::default(),
            artboard: Default::default(),
            frame: Default::default(),
            slice: Default::default(),
            retouch: Default::default(),
            perspective: Default::default(),
            crop: Default::default(),
            gesture_on: None,
            #[cfg(test)]
            frames: None,
            #[cfg(test)]
            last_response: None,
        }
    }
}

/// The tool families: each has one module that builds its tool and drains what the host reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Family {
    /// Marquees, lassos, Magic Wand and Quick Selection.
    Selection,
    /// The Move tool.
    Move,
    /// The Hand tool.
    Hand,
    /// The Zoom tool.
    Zoom,
    /// The Rotate View tool.
    RotateView,
    /// The Crop tool.
    Crop,
    /// The Perspective Crop tool.
    PerspectiveCrop,
    /// Brushes, erasers, stamps, the toning tools, Gradient-less fills and the healing brushes.
    Paint,
    /// The Eyedropper.
    Eyedropper,
    /// Pen, Freeform, Curvature, anchor tools, Path Selection and the shape tools.
    Vector,
    /// The four Type tools.
    Type,
    /// The Gradient tool.
    Gradient,
    /// Ruler, Color Sampler, Count and Note.
    Analysis,
    /// The Artboard tool.
    Artboard,
    /// The Frame tool.
    Frame,
    /// Slice and Slice Select.
    Slice,
    /// Patch, Content-Aware Move, Red Eye and Object Selection.
    Retouch,
}

/// The family `tool` belongs to. Clone Stamp and Healing are painting tools whose Alt press is the retouch source pick.
pub(crate) fn family(tool: Tool) -> Family {
    match tool {
        Tool::RectMarquee
        | Tool::EllipseMarquee
        | Tool::SingleRowMarquee
        | Tool::SingleColumnMarquee
        | Tool::Lasso
        | Tool::PolygonLasso
        | Tool::MagneticLasso
        | Tool::MagicWand
        | Tool::QuickSelection => Family::Selection,
        Tool::Move => Family::Move,
        Tool::Hand => Family::Hand,
        Tool::Zoom => Family::Zoom,
        Tool::RotateView => Family::RotateView,
        Tool::Crop => Family::Crop,
        Tool::PerspectiveCrop => Family::PerspectiveCrop,
        Tool::Eyedropper => Family::Eyedropper,
        Tool::ColorSampler | Tool::Ruler | Tool::Note | Tool::Count => Family::Analysis,
        Tool::Gradient => Family::Gradient,
        Tool::Type | Tool::VerticalType | Tool::HorizontalTypeMask | Tool::VerticalTypeMask => Family::Type,
        Tool::Artboard => Family::Artboard,
        Tool::Frame => Family::Frame,
        Tool::Slice | Tool::SliceSelect => Family::Slice,
        Tool::Patch | Tool::ContentAwareMove | Tool::RedEye | Tool::ObjectSelection => Family::Retouch,
        Tool::Pen
        | Tool::FreeformPen
        | Tool::CurvaturePen
        | Tool::AddAnchorPoint
        | Tool::DeleteAnchorPoint
        | Tool::ConvertPoint
        | Tool::PathSelection
        | Tool::DirectSelection
        | Tool::Rectangle
        | Tool::EllipseShape
        | Tool::Triangle
        | Tool::Polygon
        | Tool::Star
        | Tool::Line
        | Tool::CustomShape => Family::Vector,
        _ => Family::Paint,
    }
}

/// Whether holding ⌘ (Ctrl off the Mac) over `tool` makes the Move tool take the press: every tool but the ones ⌘ means
/// something else to (Hand, Rotate View, Zoom, the crops, Slice, the path and shape tools, the Type tools). The
/// selection tools read ⌘ per press themselves and hand the press to the stand-in (`Ignored`) where ⌘ moves the layer.
/// `run` installs the Command stand-in by it and the snap targets of the moving set follow it.
pub(crate) fn takes_command_stand_in(tool: Tool) -> bool {
    !tool.is_type()
        && !matches!(family(tool), Family::Move | Family::Hand | Family::Zoom | Family::RotateView | Family::Crop | Family::PerspectiveCrop | Family::Slice)
        && family(tool) != Family::Vector
}

/// The app for one tool method at a time. A tool in the pass holds the scope instead of `&mut PhotocraftApp`, so the
/// canvas's `Ui` and the tool coexist.
pub(crate) struct ToolScope<'a> {
    app: RefCell<&'a mut PhotocraftApp>,
    host: CanvasHost,
    ctx: egui::Context,
}

impl<'a> ToolScope<'a> {
    fn new(app: &'a mut PhotocraftApp, ctx: egui::Context, host: CanvasHost) -> Self {
        Self { app: RefCell::new(app), host, ctx }
    }

    /// A scope for a family tool's unit test, with no canvas pass.
    // PLAN: the contract takes `&PhotocraftApp`, but `with_app` hands out `&mut PhotocraftApp`; the constructor takes `&mut`.
    #[cfg(test)]
    pub(crate) fn for_test(app: &'a mut PhotocraftApp, host: CanvasHost) -> Self {
        let ctx = super::testing::ctx(app);
        Self::new(app, ctx, host)
    }

    /// Runs `f` on the app. A nested call panics: a tool method calls `with_app` once at a time.
    pub(crate) fn with_app<R>(&self, f: impl FnOnce(&mut PhotocraftApp) -> R) -> R {
        f(&mut self.app.borrow_mut())
    }

    /// The canvas this pass runs on.
    pub(crate) fn host(&self) -> CanvasHost {
        self.host
    }

    /// The egui context of the pass.
    pub(crate) fn ctx(&self) -> &egui::Context {
        &self.ctx
    }
}

/// Why a press with `tool` must not start: it would paint on or move a hidden target layer (the engine refuses the
/// command; refusing the press keeps the live stroke from drawing first).
fn hidden_target(app: &PhotocraftApp, tool: Tool) -> Option<&'static str> {
    let cmd = match tool {
        Tool::Move => "layer.translate",
        // The live Gradient makes a new Gradient Fill layer above the target, which Photoshop allows over a hidden
        // layer; only the classic one paints the target itself.
        Tool::Gradient if !app.ui.tool_options.gradient_classic => return None,
        // Every painting command follows the same target (layer, mask or channel) as a stroke.
        t if t.is_brushlike() || matches!(t, Tool::Gradient | Tool::PaintBucket | Tool::MagicEraser) => "paint.stroke",
        _ => return None,
    };
    photocraft_engine::hidden_target::refusal(&app.session, cmd, &serde_json::json!({}))
}

/// The press-time refusals: a hidden target layer, and for the tools with an alert in `brush_refusal` a locked layer or
/// a group's pixels. Those tools say why in an alert; the other tools say a hidden target in the status bar. True when
/// the press is refused.
fn press_refused(app: &mut PhotocraftApp, tool: Tool) -> bool {
    let hidden = hidden_target(app, tool);
    if let Some(message) = crate::brush_refusal::alert(app, tool, hidden.is_some()) {
        let _ = crate::dialogs::alerts::alert(app, message);
        return true;
    }
    if let Some(why) = hidden {
        app.ui.status = why.into();
        app.ui.status_error = true;
        return true;
    }
    false
}

/// A press with `tool` that would paint the pixels of a group (the active layer, with neither its mask, an alpha channel
/// nor the Quick Mask targeted): a group has none, so the press is refused with Photoshop's message. Returns whether it
/// was refused.
fn refuse_group(app: &mut PhotocraftApp, tool: Tool) -> bool {
    let group = crate::rasterize_prompt::paints_pixels(app, tool)
        && super::paint_target(app) == serde_json::json!("pixels")
        && app.session.active().and_then(|st| st.doc.layer(st.active_layer?)).is_some_and(photocraft_doc::Layer::is_group);
    if group {
        let message = crate::i18n::fmt(tl!("Could not use the {tool} because the target layer is a group."), &[("tool", tl!(tool.label()))]);
        crate::dialogs::alerts::alert(app, message);
    }
    group
}

/// The press preflight around the pass's tool (M21): the quick-pick chord and the press refusals come before any tool
/// or stand-in sees the press, and an arrow key the tool did not take nudges as the Move tool does.
struct Preflight<'a, 'x> {
    scope: &'a ToolScope<'a>,
    inner: DynTool<'x>,
    tool: Tool,
    /// A hosted dialog's pick runs the pass: its presses are the dialog's own, so the painting refusals and the quick-pick
    /// chord (which belong to the selected tool) do not apply.
    dialog: bool,
    /// The selection tools' outline is open (a Polygonal or Magnetic Lasso being drawn): an arrow nudges it, not the
    /// layer. Read before the pass takes the families' state out of the app.
    outline_open: bool,
    /// The pass has an Alt stand-in, so a press the preflight hands on (`Ignored`) reaches it.
    alt_stand_in: bool,
}

impl CanvasTool for Preflight<'_, '_> {
    fn each_pass(&mut self, cx: &mut ToolContext<'_>) {
        self.inner.each_pass(cx);
    }

    fn press(&mut self, pointer: &ToolPointer, cx: &mut ToolContext<'_>) -> ToolAction {
        let tool = self.tool;
        let doc = cx.space().map(|space| space.view.to_doc(pointer.pos));
        if pointer.button == PointerButton::Primary && !self.dialog {
            if let Some(doc) = doc {
                if self.scope.with_app(|app| super::quick_pick::press(app, pointer, doc)) {
                    return ToolAction::Done;
                }
                let at = [doc.x, doc.y];
                // The Triangle's ⌥ press that samples is the Eyedropper stand-in's, not a shape (#13364).
                if self.alt_stand_in && self.scope.with_app(|app| super::cursor_policy::shape_alt_samples(app, pointer.mods, at[0], at[1])) {
                    return ToolAction::Ignored;
                }
                if self.refused(pointer, at) {
                    return ToolAction::Done;
                }
            }
        }
        self.inner.press(pointer, cx)
    }

    fn drag(&mut self, pointer: &ToolPointer, cx: &mut ToolContext<'_>) {
        self.inner.drag(pointer, cx);
    }

    fn release(&mut self, pointer: &ToolPointer, cx: &mut ToolContext<'_>) {
        self.inner.release(pointer, cx);
    }

    fn hover(&mut self, pointer: &ToolPointer, cx: &mut ToolContext<'_>) {
        self.inner.hover(pointer, cx);
    }

    fn cancel(&mut self, reason: CancelReason, cx: &mut ToolContext<'_>) {
        self.inner.cancel(reason, cx);
    }

    fn claims(&self) -> ToolClaims {
        self.inner.claims()
    }

    fn key(&mut self, key: &ToolKey, cx: &mut ToolContext<'_>) -> bool {
        if self.inner.key(key, cx) {
            return true;
        }
        let ToolKey::Nudge { direction, mods, .. } = key else { return false };
        if cx.gesture_id().is_some() {
            return false;
        }
        let time = self.scope.ctx().input(|i| i.time);
        let outline_open = self.outline_open;
        self.scope.with_app(|app| crate::move_mods::arrow_key(app, *direction, *mods, time, outline_open))
    }

    fn paint(&self, draw: &Draw<'_>, cx: &ToolContext<'_>) {
        self.inner.paint(draw, cx);
    }

    fn cursor(&self, cx: &ToolContext<'_>) -> Cursor {
        self.inner.cursor(cx)
    }

    fn takes(&self, button: PointerButton, mods: Mods) -> bool {
        (!self.dialog && self.scope.with_app(|app| super::quick_pick::takes(app, mods))) || self.inner.takes(button, mods)
    }

    fn autoscrolls(&self) -> bool {
        self.inner.autoscrolls()
    }

    fn autoscrolls_on_hover(&self) -> bool {
        self.inner.autoscrolls_on_hover()
    }

    fn view_only(&self) -> bool {
        self.inner.view_only()
    }

    fn takes_every_move(&self) -> bool {
        self.inner.takes_every_move()
    }

    fn access(&self, cx: &ToolContext<'_>) -> Option<ToolAccess> {
        self.inner.access(cx)
    }

    fn command_form(&self) -> ToolPath {
        self.inner.command_form()
    }
}

impl Preflight<'_, '_> {
    /// The refusals of a painting press: a hidden or locked target, a layer that must be rasterized first, a group, a
    /// Clone Stamp with no source. Each says why itself; true when the press is refused.
    fn refused(&self, pointer: &ToolPointer, at: [f64; 2]) -> bool {
        let tool = self.tool;
        let pressure = pointer.sample.pressure;
        let mods = pointer.mods;
        self.scope.with_app(|app| {
            // Auto-Select's press picks the layer to move, so the hidden active layer is not its target.
            let picks = tool == Tool::Move && !crate::move_ui::moves_selected_pixels_with(app, tool) && app.ui.tool_options.move_auto_select != mods.cmd;
            (!picks && press_refused(app, tool))
                || crate::rasterize_prompt::intercept(app, tool, at[0], at[1], pressure)
                || refuse_group(app, tool)
                || crate::retouch_ui::clone_stamp_without_source(app, tool)
        })
    }
}

/// What the canvas body's controls reported for the host's pass of the same frame.
#[derive(Default)]
pub(crate) struct BodyShown {
    /// The Crop tool's frame.
    pub(crate) crop: Option<CropShown>,
    /// The snap lines of the open Free Transform's box, which its canvas tool hands to the host.
    pub(crate) transform_snapped: Vec<SnapLine>,
}

/// Borrows a family tool as the host's `dyn`.
fn dynamic<'x>(tool: &'x mut FamilyTool<'_>) -> DynTool<'x> {
    &mut **tool
}

/// Builds the pass's tool and the stand-ins, wraps the tool in the preflight and calls `f` with it and the tool host
/// over the momentary set and the stand-ins, so the pass (`run`) and the cancel (`tool_changed`) pick from the same
/// tools. The tool is, in order: the top dialog's armed pick, the open Free Transform's tool, the painting tool with a
/// symmetry session, else the selected tool's family. The crop pass finishes after `f`, with the app free again.
fn with_rig<R>(
    app: &mut PhotocraftApp,
    ctx: &egui::Context,
    host: CanvasHost,
    tools: &mut CanvasTools,
    stand_ins: bool,
    shown: BodyShown,
    f: impl FnOnce(&mut dyn CanvasTool, ToolHost<'_>) -> R,
) -> R {
    let tool = app.ui.tool;
    let fam = family(tool);
    // Tools that own state the app lends nobody are built before the scope takes the app.
    let crop_run = (fam == Family::Crop).then(|| crate::crop_ui::CropRun::of(app)).flatten();
    let mut crop_tool = crop_run.as_ref().map(|_| std::mem::take(&mut tools.crop.tool));
    // The open Free Transform session moves into the pass and back after it, so its tool borrows the session, not the app.
    let free_transform = app.ui.transform.take();
    let BodyShown { crop: crop_shown, transform_snapped } = shown;
    let mut transform = free_transform.as_ref().map(|session| crate::transform_tool::canvas_tool(session, transform_snapped));
    // The symmetry session moves into the pass the same way.
    let symmetry_transform = if fam == Family::Paint { app.ui.symmetry_transform.take() } else { None };
    let mut symmetry = symmetry_transform.as_ref().and_then(|t| crate::symmetry_ui::session_tool(app, t));
    let mut crop_pass = match (crop_tool.as_mut(), crop_run.as_ref()) {
        (Some(tool), Some(run)) => Some(crop::pass(tool, run, ctx, crop_shown)),
        _ => None,
    };
    let command_wanted = stand_ins && transform.is_none() && takes_command_stand_in(tool);
    let alt_wanted = stand_ins && transform.is_none() && super::cursor_policy::alt_eyedropper(tool);
    let source_wanted = stand_ins && matches!(tool, Tool::CloneStamp | Tool::Healing);

    let CanvasTools {
        momentary,
        hand,
        zoom,
        rotate,
        selection,
        mover,
        dialog,
        paint,
        eyedropper,
        vector,
        type_tool,
        gradient,
        analysis,
        artboard,
        frame,
        slice,
        retouch,
        perspective,
        ..
    } = tools;
    let outline_open = selection.outline_open();
    let result = {
        let scope = ToolScope::new(app, ctx.clone(), host);
        let mut boxed: Option<FamilyTool<'_>> = None;
        let mut command_t: Option<FamilyTool<'_>> = None;
        let mut alt_t: Option<FamilyTool<'_>> = None;
        let mut mover = Some(mover);
        let mut eyedropper = Some(eyedropper);
        let mut retouch = Some(retouch);
        let dialog_picked = dialog::dialog_tool(&scope, dialog, host).is_some_and(|picked| {
            boxed = Some(picked);
            true
        });
        let selected: DynTool<'_> = 'select: {
            if dialog_picked {
                break 'select dynamic(boxed.as_mut().expect("the dialog's tool was just stored"));
            }
            if let Some(t) = transform.as_mut() {
                break 'select t as &mut dyn CanvasTool;
            }
            if let Some(t) = symmetry.as_mut() {
                break 'select t as &mut dyn CanvasTool;
            }
            match fam {
                Family::Hand => hand as &mut dyn CanvasTool,
                Family::Zoom => zoom as &mut dyn CanvasTool,
                Family::RotateView => rotate as &mut dyn CanvasTool,
                Family::Crop => match crop_pass.as_mut() {
                    Some(pass) => pass as &mut dyn CanvasTool,
                    None => dynamic(boxed.insert(Box::new(NoTool))),
                },
                Family::Selection => dynamic(boxed.insert(selection::tool(&scope, selection, host))),
                Family::Move => dynamic(boxed.insert(move_tool::tool(&scope, mover.take().expect("the Move state is free"), host))),
                Family::Paint => dynamic(boxed.insert(super::paint_tool::tool(&scope, paint, host))),
                Family::Eyedropper => {
                    dynamic(boxed.insert(super::eyedropper_tool::tool(&scope, eyedropper.take().expect("the Eyedropper state is free"), host)))
                }
                Family::Vector => dynamic(boxed.insert(crate::vector_ui::host::tool(&scope, vector, host))),
                Family::Type => dynamic(boxed.insert(crate::type_tool::tool(&scope, type_tool, host))),
                Family::Gradient => dynamic(boxed.insert(crate::gradient_ui::tool::tool(&scope, gradient, host))),
                Family::Analysis => dynamic(boxed.insert(crate::analysis::canvas::tool(&scope, analysis, host))),
                Family::Artboard => dynamic(boxed.insert(crate::artboard_tool::tool(&scope, artboard, host))),
                Family::Frame => dynamic(boxed.insert(crate::frame_tool::tool(&scope, frame, host))),
                Family::Slice => dynamic(boxed.insert(crate::slice_ui::tool(&scope, slice, host))),
                Family::Retouch => dynamic(boxed.insert(retouch::tool(&scope, retouch.take().expect("the Retouch state is free"), host))),
                Family::PerspectiveCrop => dynamic(boxed.insert(crate::perspective_crop_ui::tool(&scope, perspective, host))),
            }
        };
        if command_wanted
            && !dialog_picked
            && let Some(state) = mover.take()
        {
            command_t = Some(move_tool::tool(&scope, state, host));
        }
        if alt_wanted
            && !dialog_picked
            && let Some(state) = eyedropper.take()
        {
            alt_t = Some(super::eyedropper_tool::stand_in_tool(&scope, state, host));
        } else if source_wanted
            && !dialog_picked
            && let Some(state) = retouch.take()
        {
            alt_t = Some(retouch::source_tool(&scope, state, host));
        }
        let alt_stand_in = alt_t.is_some();
        let mut pre = Preflight { scope: &scope, inner: selected, tool, dialog: dialog_picked, outline_open, alt_stand_in };
        let mut tool_host = ToolHost::new().momentary(momentary);
        if let Some(mover) = command_t.as_mut() {
            tool_host = tool_host.stand_in(StandInKey::Command, dynamic(mover));
        }
        if let Some(stand_in) = alt_t.as_mut() {
            tool_host = tool_host.stand_in(StandInKey::Alt, dynamic(stand_in));
        }
        f(&mut pre, tool_host)
    };
    let transform_keys = transform.as_mut().map(|t| t.take_keys());
    drop(transform);
    app.ui.transform = free_transform;
    if let Some(keys) = transform_keys {
        crate::transform_tool::tool_keys(app, keys);
    }
    let symmetry_events = symmetry.as_mut().map(|t| t.take_events());
    drop(symmetry);
    if symmetry_transform.is_some() {
        app.ui.symmetry_transform = symmetry_transform;
    }
    if let Some(events) = symmetry_events {
        crate::symmetry_ui::finish_tool(app, events);
    }
    if let Some(pass) = crop_pass {
        let output = CropToolPass::finish(pass);
        if let Some(tool) = crop_tool {
            tools.crop.tool = tool;
        }
        crop::finish(app, output);
    }
    result
}

/// Runs the tool host for the canvas `host` on the canvas's output `out`, and applies what it reports: the view a
/// tool or the autoscroll moved (in the response's `view`), the Scroll All Windows follow, and each family's drain.
/// Tools are active in the main window only.
pub(crate) fn run(app: &mut PhotocraftApp, ui: &egui::Ui, host: CanvasHost, out: &CanvasOutput, shown: BodyShown) -> ToolHostResponse {
    let ctx = ui.ctx().clone();
    let primary = host.window.is_none() && host.viewport == egui::ViewportId::ROOT;
    let mut tools = std::mem::take(&mut app.canvas_tools);
    let tool = app.ui.tool;
    let navigation = document_display::navigation(app);
    // A pick a dialog arms on the canvas stands in for the selected tool, so the Hand's Cmd and Alt zoom is not in effect.
    let dialog_picks = app.session.active_index() == Some(host.idx) && crate::dialogs::canvas_input::armed(app).is_some();
    tools.momentary.hand_selected(tool == Tool::Hand && app.ui.transform.is_none() && !dialog_picks);
    tools.momentary.set_options(navigation.clone());
    tools.hand.options = navigation.clone();
    // Zoom In or Out and Scrubby Zoom are the selected Zoom tool's own options, never the momentary Zoom's.
    tools.zoom.out = !app.ui.tool_options.zoom_in && !crate::spring_tools::holds(app, Tool::Zoom);
    tools.zoom.options = navigation;
    tools.zoom.options.scrubby_zoom = app.ui.tool_options.zoom_scrubby;
    let smart = super::snap_ui::smart_guide_color(app);
    let mut response =
        with_rig(app, &ctx, host, &mut tools, primary, shown, |selected, tool_host| tool_host.enabled(primary).smart_guide_color(smart).run(ui, out, selected));
    // The autoscroll moves the view the pass ended with; the caller copies the response's view into its own.
    if let (Some(scroll), Some(space)) = (response.scroll, out.space.as_ref()) {
        let size = app.session.documents().get(host.idx).map(|state| Size::new(f64::from(state.doc.size.width), f64::from(state.doc.size.height)));
        if let Some(size) = size {
            let mut view = response.view.clone().unwrap_or(space.view);
            if Autoscroll::scroll(&mut view, scroll, size).is_some() {
                response.view = Some(view);
            }
        }
        ctx.request_repaint();
    }
    match response.gesture {
        Some(GesturePhase::Begin) => tools.gesture_on = app.session.documents().get(host.idx).map(|state| (state.doc.id.0, host)),
        Some(GesturePhase::Commit | GesturePhase::Cancel) => tools.gesture_on = None,
        _ => {}
    }
    after_run(app, &mut tools, &response);
    #[cfg(test)]
    if primary {
        tools.last_response = Some(response.clone());
    }
    app.canvas_tools = tools;
    response
}

/// Ends a gesture that began on another document than the one the canvas `host` shows now: a document switch while the
/// button is down. Call it at the start of the canvas pass of the active document.
pub(crate) fn end_foreign_gesture(app: &mut PhotocraftApp, ctx: &egui::Context, shown: u64) {
    let Some((started, host)) = app.canvas_tools.gesture_on else { return };
    if started != shown {
        app.canvas_tools.gesture_on = None;
        tool_changed(app, ctx, host);
    }
}

/// What the app does with a finished pass of the host: each family takes its gesture's events (the painting family
/// first, so a fed stroke shows in the pass's picture), and a pan follows into the other windows when Scroll All
/// Windows asks.
pub(crate) fn after_run(app: &mut PhotocraftApp, tools: &mut CanvasTools, response: &ToolHostResponse) {
    super::paint_tool::drain(app, &mut tools.paint, response);
    selection::drain(app, &mut tools.selection, response);
    super::eyedropper_tool::drain(app, &mut tools.eyedropper, response);
    crate::vector_ui::host::drain(app, &mut tools.vector, response);
    crate::type_tool::drain(app, &mut tools.type_tool, response);
    crate::gradient_ui::tool::drain(app, &mut tools.gradient, response);
    crate::analysis::canvas::drain(app, &mut tools.analysis, response);
    crate::artboard_tool::drain(app, &mut tools.artboard, response);
    crate::frame_tool::drain(app, &mut tools.frame, response);
    crate::slice_ui::drain(app, &mut tools.slice, response);
    retouch::drain(app, &mut tools.retouch, response);
    crate::perspective_crop_ui::drain(app, &mut tools.perspective, response);
    if let Some(pan) = response.pan {
        let host = app.session.active_index().map(CanvasHost::main);
        if let Some(host) = host {
            let source = host.window.map_or(Source::Main(host.idx), Source::Window);
            scroll_all_windows::follow(app, source, pan);
        }
    }
    // PLAN: the sheet sends `response.ignored_press` under Blur Gallery to the momentary Zoom (p01 §2.2 dialog row); the
    // framework gives a press no way to start a momentary tool, so the press stays ignored.
}

/// Ends the gesture of the canvas `host` now, without a commit: the tool or the document changed. Call it before the
/// app replaces the tool, with `app.ui.tool` still the selected tool of the gesture's pass. The host is built with the
/// pass's stand-ins and momentary set, so a gesture the ⌘ Move, an Alt stand-in or a held Hand runs is cancelled on
/// that tool. The one cancel path of the canvas tools.
pub(crate) fn tool_changed(app: &mut PhotocraftApp, ctx: &egui::Context, host: CanvasHost) {
    let mut tools = std::mem::take(&mut app.canvas_tools);
    let id = host.canvas_id();
    let primary = host.window.is_none() && host.viewport == egui::ViewportId::ROOT;
    with_rig(app, ctx, host, &mut tools, primary, BodyShown::default(), |selected, tool_host| {
        tool_host.cancel(ctx, id, selected, CancelReason::HostChanged);
    });
    app.canvas_tools = tools;
}

/// Whether the active document's main canvas has a gesture running, for the readers with no `Context` at hand.
pub(crate) fn in_gesture(app: &PhotocraftApp) -> bool {
    let Some(idx) = app.session.active_index() else { return false };
    ToolHost::in_gesture(app.ctx(), CanvasHost::main(idx).canvas_id())
}
