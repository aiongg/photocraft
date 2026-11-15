//! The retouch family: Patch and Content-Aware Move (a selection drag or a lasso outline), Red Eye, Object
//! Selection, and the Alt-click source of Clone Stamp and Healing (their strokes are the paint family's).

use std::cell::RefCell;

use craft::behavior::cursor::{Cursor, PaintedCursor};
use craft::behavior::value::CancelReason;
use craft::imaging::ui::select::{LassoTool, SelectEvent, SelectionShape};
use craft::keymap::Mods;
use craft::ui::canvas::{CanvasTool, ToolAction, ToolContext, ToolHostResponse, ToolPath, ToolPointer};
use craft::ui::primitives::Draw;
use egui::PointerButton;

use super::dialog::doc_point;
use super::selection::PcSelection;
use super::{FamilyTool, ToolScope};
use crate::PhotocraftApp;
use crate::canvas::document_display::CanvasHost;
use crate::retouch_ui::{self, PatchPress};
use crate::state::Tool;

/// The retouch gestures in progress.
#[derive(Default)]
pub(crate) struct RetouchTools {
    /// The press of a Patch or Content-Aware Move selection drag, or of an Object Selection rectangle.
    press: Option<PatchPress>,
    /// The last drag point in document units.
    last: [f64; 2],
    /// Shift at the last drag sample.
    shift: bool,
    /// The outline a Patch or Content-Aware Move press outside the selection draws.
    lasso: LassoTool,
}

impl RetouchTools {
    /// The press of the selection drag in progress, for `patch_replaces_selection`, `cam_preview::wanted` and
    /// `patch_preview::drag_offset`.
    pub(crate) fn press(&self) -> Option<&PatchPress> {
        self.press.as_ref()
    }

    /// The last drag point in document units.
    pub(crate) fn last_point(&self) -> [f64; 2] {
        self.last
    }

    /// Whether Shift was held at the last drag sample.
    pub(crate) fn shift(&self) -> bool {
        self.shift
    }
}

/// The retouch tool for the selected tool (Patch, Content-Aware Move, Red Eye or Object Selection).
pub(crate) fn tool<'a>(scope: &'a ToolScope<'a>, state: &'a mut RetouchTools, _host: CanvasHost) -> FamilyTool<'a> {
    let tool = scope.with_app(|app| app.ui.tool);
    let probe = scope.with_app(|app| PcSelection::of(app));
    Box::new(RetouchTool { scope, state: RefCell::new(state), tool, probe })
}

/// The Alt-click source of Clone Stamp and Healing: the stand-in `tools::run` installs for `StandInKey::Alt`.
pub(crate) fn source_tool<'a>(scope: &'a ToolScope<'a>, _state: &'a mut RetouchTools, _host: CanvasHost) -> FamilyTool<'a> {
    Box::new(SourceTool { scope })
}

/// Applies what the lasso outline committed. A press that dragged the selection applied itself on release.
pub(crate) fn drain(app: &mut PhotocraftApp, state: &mut RetouchTools, _response: &ToolHostResponse) {
    for event in state.lasso.take_events() {
        // A press inside the selection is routed to the selection drag, so the lasso's own inside drag and
        // deselect events do not arise here.
        let SelectEvent::Commit { shape, operation, .. } = event else { continue };
        let tool = app.ui.tool;
        if tool == Tool::ContentAwareMove
            && let SelectionShape::Polygon(points) = &shape
        {
            let points: Vec<[f64; 2]> = points.iter().map(|p| [p.x, p.y]).collect();
            if retouch_ui::content_aware_move_no_pixels(app, &points) {
                continue;
            }
        }
        super::selection::run_commit(app, tool, shape, operation);
        if tool == Tool::Patch {
            retouch_ui::patch_hint(app);
        }
    }
}

struct RetouchTool<'a> {
    scope: &'a ToolScope<'a>,
    /// In a `RefCell` because the lasso paints through its pass, which needs `&mut` while `paint` takes `&self`.
    state: RefCell<&'a mut RetouchTools>,
    tool: Tool,
    probe: PcSelection,
}

impl RetouchTool<'_> {
    /// Whether a press outside the selection draws the outline (Patch and Content-Aware Move).
    fn lassos(&self) -> bool {
        matches!(self.tool, Tool::Patch | Tool::ContentAwareMove)
    }
}

impl CanvasTool for RetouchTool<'_> {
    fn press(&mut self, pointer: &ToolPointer, cx: &mut ToolContext<'_>) -> ToolAction {
        let at = doc_point(pointer, cx);
        let (tool, mods) = (self.tool, pointer.mods);
        let mut state = self.state.borrow_mut();
        if tool == Tool::RedEye {
            // PLAN: assumed `red_eye_click(app, at: [f64; 2])` (tools-w54).
            self.scope.with_app(|app| retouch_ui::red_eye_click(app, at));
            return ToolAction::Done;
        }
        // PLAN: assumed `cancel_stale(app, &mut Option<PatchPress>)` and `PatchPress::new(start, mods)` (tools-w54).
        let drags = self.scope.with_app(|app| {
            retouch_ui::cancel_stale(app, &mut state.press);
            tool == Tool::ObjectSelection || retouch_ui::patch_drags_selection(app, at, mods)
        });
        if drags {
            state.press = Some(PatchPress::new(at, mods));
            state.last = at;
            state.shift = mods.shift;
            return ToolAction::Claimed;
        }
        if self.lassos() {
            return state.lasso.pass(&self.probe).press(pointer, cx);
        }
        ToolAction::Ignored
    }

    fn drag(&mut self, pointer: &ToolPointer, cx: &mut ToolContext<'_>) {
        let mut state = self.state.borrow_mut();
        if state.press.is_some() {
            state.last = doc_point(pointer, cx);
            state.shift = pointer.mods.shift;
        } else if self.lassos() {
            state.lasso.pass(&self.probe).drag(pointer, cx);
        }
    }

    fn release(&mut self, pointer: &ToolPointer, cx: &mut ToolContext<'_>) {
        let mut state = self.state.borrow_mut();
        let Some(press) = state.press.take() else {
            if self.lassos() {
                state.lasso.pass(&self.probe).release(pointer, cx);
            }
            return;
        };
        let end = doc_point(pointer, cx);
        let (tool, mods) = (self.tool, pointer.mods);
        let start = press.start();
        state.last = end;
        state.shift = mods.shift;
        self.scope.with_app(|app| match tool {
            Tool::ObjectSelection => retouch_ui::finish_object_selection(app, start, end, mods),
            // A click with no drag moves nothing.
            _ if end == start => {}
            Tool::Patch => {
                // PLAN: assumed `patch_end(&PatchPress, end, shift) -> [f64; 2]` (tools-w54).
                let end = retouch_ui::patch_end(&press, end, mods.shift);
                retouch_ui::finish_patch(app, start, end);
            }
            Tool::ContentAwareMove => retouch_ui::finish_content_aware_move(app, start, end),
            _ => {}
        });
    }

    fn hover(&mut self, pointer: &ToolPointer, cx: &mut ToolContext<'_>) {
        if self.lassos() {
            self.state.borrow_mut().lasso.pass(&self.probe).hover(pointer, cx);
        }
    }

    fn cancel(&mut self, reason: CancelReason, cx: &mut ToolContext<'_>) {
        let mut state = self.state.borrow_mut();
        state.press = None;
        if self.lassos() {
            state.lasso.pass(&self.probe).cancel(reason, cx);
        }
    }

    fn paint(&self, draw: &Draw<'_>, cx: &ToolContext<'_>) {
        if self.lassos() {
            self.state.borrow_mut().lasso.pass(&self.probe).paint(draw, cx);
        }
    }

    fn cursor(&self, _cx: &ToolContext<'_>) -> Cursor {
        Cursor::Painted(PaintedCursor::Crosshair)
    }

    fn command_form(&self) -> ToolPath {
        ToolPath::Freehand("a retouch gesture is drawn with the pointer")
    }
}

struct SourceTool<'a> {
    scope: &'a ToolScope<'a>,
}

impl CanvasTool for SourceTool<'_> {
    fn press(&mut self, pointer: &ToolPointer, cx: &mut ToolContext<'_>) -> ToolAction {
        let at = doc_point(pointer, cx);
        self.scope.with_app(|app| retouch_ui::set_source(app, at[0], at[1]));
        ToolAction::Done
    }

    fn drag(&mut self, _pointer: &ToolPointer, _cx: &mut ToolContext<'_>) {}

    fn release(&mut self, _pointer: &ToolPointer, _cx: &mut ToolContext<'_>) {}

    fn cancel(&mut self, _reason: CancelReason, _cx: &mut ToolContext<'_>) {}

    fn takes(&self, button: PointerButton, mods: Mods) -> bool {
        button == PointerButton::Primary && mods.alt
    }

    fn paint(&self, _draw: &Draw<'_>, _cx: &ToolContext<'_>) {}

    fn cursor(&self, _cx: &ToolContext<'_>) -> Cursor {
        Cursor::Painted(PaintedCursor::Crosshair)
    }

    fn command_form(&self) -> ToolPath {
        ToolPath::Freehand("the source is picked with the pointer")
    }
}
