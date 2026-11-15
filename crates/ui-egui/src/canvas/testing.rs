//! Headless driver for the canvas tools' tests. Each call queues window input through craft-automation's synthetic
//! input and runs the app's own frame (key route, canvas pass, hosted dialogs) on the app's context (`app.ctx()`, the
//! one its code outside the pass reads) until the window host has delivered it,
//! so a press passes the preflight, the stand-ins, the dialog tool and the canvas controls of the body exactly as a
//! mouse press does. Document points map through the view the canvas last showed (`view`).

use craft::behavior::{CanvasView, PointerButton};
use craft::behavior::kurbo::Point;
use craft::keymap::{Key, Mods};
use craft::ui::canvas::ToolHostResponse;
use craft::ui::runtime::windows::{Released, WindowHost};
use craft::ui::testing::Frames;
use egui::{Modifiers, Pos2};

use super::{active_view, tools};
use crate::PhotocraftApp;
use crate::stylus::PenSample;

/// Frames a queued input may take before the driver gives up: a click with a count of 2 is five steps.
const MAX_FRAMES: usize = 64;

/// One pointer event of a tool test, in document coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ToolEvent {
    /// The primary button goes down.
    Down {
        /// Document x.
        x: f64,
        /// Document y.
        y: f64,
        /// The pen's pressure, 1 for a mouse.
        pressure: f32,
    },
    /// The pointer moves, dragging while a button is down.
    Move {
        /// Document x.
        x: f64,
        /// Document y.
        y: f64,
        /// The pen's pressure, 1 for a mouse.
        pressure: f32,
    },
    /// The primary button comes up.
    Up {
        /// Document x.
        x: f64,
        /// Document y.
        y: f64,
    },
}

/// Delivers `ev` with `mods` held: `Down` is `press`, `Move` is `move_to` and `Up` is `release`, with the pressure
/// of `Down` and `Move` fed through the stylus feed first. Returns what the tool host reported in the main canvas's
/// pass of the last frame the delivery ran (the default when that frame ran no main canvas pass).
pub(crate) fn tool_event(app: &mut PhotocraftApp, ev: ToolEvent, mods: Modifiers) -> ToolHostResponse {
    match ev {
        ToolEvent::Down { x, y, pressure } => {
            pen(app, pressure);
            press(app, [x, y], mods);
        }
        ToolEvent::Move { x, y, pressure } => {
            pen(app, pressure);
            move_to(app, [x, y], mods);
        }
        ToolEvent::Up { x, y } => {
            release(app, [x, y], mods);
            app.stylus.feed.set(None);
        }
    }
    app.canvas_tools.last_response.take().unwrap_or_default()
}

/// Feeds the pen's pressure for the next frame; a mouse (pressure 1) feeds no pen.
fn pen(app: &PhotocraftApp, pressure: f32) {
    app.stylus.feed.set((pressure != 1.0).then(|| PenSample { pressure, ..PenSample::default() }));
}

/// The context the canvas pass runs in: the app's own, which its code outside the pass reads too.
pub(crate) fn ctx(app: &PhotocraftApp) -> egui::Context {
    app.ctx().clone()
}

/// `mods` as the window host takes them: the macOS reading of egui's modifiers (⌘ is `mac_cmd`, or `command` on a
/// platform where egui folds Ctrl into it; Ctrl is its own key only where it is not the command key).
fn held(mods: Modifiers) -> Mods {
    Mods { cmd: mods.mac_cmd || mods.command, ctrl: mods.ctrl && (mods.mac_cmd || !mods.command), alt: mods.alt, shift: mods.shift }
}

/// One frame of the app on its own context: the key route, then the document area and the hosted dialogs, as the
/// app's frame orders them.
fn run_frame(app: &mut PhotocraftApp) {
    let ctx = ctx(app);
    let mut frames = app.canvas_tools.frames.take().unwrap_or_else(|| Frames::over(ctx));
    app.canvas_tools.last_response = None;
    frames.frame(Vec::new(), |ui| {
        let ctx = ui.ctx().clone();
        crate::shortcuts::handle(app, &ctx);
        egui::CentralPanel::default().show(ui, |ui| super::document_area(app, ui));
        crate::dialogs::run(app, ui);
    });
    app.canvas_tools.frames = Some(frames);
}

/// Runs frames until the window host has delivered everything queued.
fn settle(app: &mut PhotocraftApp) {
    let host = WindowHost::of(&ctx(app));
    for _ in 0..MAX_FRAMES {
        run_frame(app);
        if host.injected_idle() {
            return;
        }
    }
    panic!("the injected input was not taken in {MAX_FRAMES} frames");
}

/// The active document's main canvas view as its last pass showed it, rect included.
pub(crate) fn view(app: &PhotocraftApp) -> Option<CanvasView> {
    active_view(app)
}

/// The screen point of the document point `at` through the view the canvas last showed.
pub(crate) fn to_screen(app: &PhotocraftApp, at: [f64; 2]) -> Pos2 {
    view(app).expect("the canvas shows a view of the active document").to_screen(Point::new(at[0], at[1]))
}

/// Runs a frame when the canvas has shown nothing yet, so an input has a view to map through.
fn shown(app: &mut PhotocraftApp) {
    if view(app).is_none() {
        run_frame(app);
    }
}

/// Presses the primary button at the document point `at` with `mods` held, and leaves it down.
pub(crate) fn press(app: &mut PhotocraftApp, at: [f64; 2], mods: Modifiers) {
    shown(app);
    let pos = to_screen(app, at);
    WindowHost::of(&ctx(app)).inject_click(egui::ViewportId::ROOT, pos, PointerButton::Primary, 1, held(mods), true);
    settle(app);
}

/// Moves the pointer to `at`; a drag while the button is down.
pub(crate) fn move_to(app: &mut PhotocraftApp, at: [f64; 2], mods: Modifiers) {
    shown(app);
    let pos = to_screen(app, at);
    WindowHost::of(&ctx(app)).inject_move(egui::ViewportId::ROOT, pos, held(mods), 0.0);
    settle(app);
}

/// Lets the primary button go at `at`.
pub(crate) fn release(app: &mut PhotocraftApp, at: [f64; 2], mods: Modifiers) {
    shown(app);
    let pos = to_screen(app, at);
    let host = WindowHost::of(&ctx(app));
    host.inject_move(egui::ViewportId::ROOT, pos, held(mods), 0.0);
    host.inject_release(egui::ViewportId::ROOT, Some(pos), Released::Button(PointerButton::Primary));
    settle(app);
}

/// A press and a release at `at`.
pub(crate) fn click(app: &mut PhotocraftApp, at: [f64; 2], mods: Modifiers) {
    shown(app);
    let pos = to_screen(app, at);
    WindowHost::of(&ctx(app)).inject_click(egui::ViewportId::ROOT, pos, PointerButton::Primary, 1, held(mods), false);
    settle(app);
}

/// A secondary-button press and release at `at`.
pub(crate) fn secondary_click(app: &mut PhotocraftApp, at: [f64; 2], mods: Modifiers) {
    shown(app);
    let pos = to_screen(app, at);
    WindowHost::of(&ctx(app)).inject_click(egui::ViewportId::ROOT, pos, PointerButton::Secondary, 1, held(mods), false);
    settle(app);
}

/// A double click at `at`: one click with a count of 2, so egui counts the presses.
pub(crate) fn double_click(app: &mut PhotocraftApp, at: [f64; 2], mods: Modifiers) {
    shown(app);
    let pos = to_screen(app, at);
    WindowHost::of(&ctx(app)).inject_click(egui::ViewportId::ROOT, pos, PointerButton::Primary, 2, held(mods), false);
    settle(app);
}

/// A key press and release with `mods` held. The app's key route runs first in the frame and reads the claims the tool
/// host made in the last pass, so a Delete or Escape the tool claimed reaches it in the pass after its claim.
pub(crate) fn key(app: &mut PhotocraftApp, key: egui::Key, mods: Modifiers) {
    let Some(key) = Key::from_egui(key) else { panic!("{key:?} has no key of the window host") };
    WindowHost::of(&ctx(app)).inject_key(egui::ViewportId::ROOT, key, held(mods), false);
    settle(app);
}

/// Changes the held modifiers with no pointer event: the pointer stays where it is.
pub(crate) fn modifiers(app: &mut PhotocraftApp, mods: Modifiers) {
    let ctx = ctx(app);
    let at = ctx.input(|i| i.pointer.latest_pos()).or_else(|| view(app).map(|v| v.rect.center())).unwrap_or(Pos2::ZERO);
    WindowHost::of(&ctx).inject_move(egui::ViewportId::ROOT, at, held(mods), 0.0);
    settle(app);
}

/// One pass with no input: what a tool change cancels, a document switch drops.
pub(crate) fn frame(app: &mut PhotocraftApp) {
    run_frame(app);
}

/// Whether the active canvas has a gesture in progress.
pub(crate) fn in_gesture(app: &PhotocraftApp) -> bool {
    tools::in_gesture(app)
}
