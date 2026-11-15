//! #6288: ⌘ (Ctrl off the Mac) over a guide is the Move tool's guide drag with every tool on
//! which ⌘ is the Move tool, and the pointer shows it (the guide's double arrow), as in
//! Photoshop. Driven through real egui input, so the held key goes through `hold_keys`.

use egui::{Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use serde_json::json;

use crate::PhotocraftApp;
use crate::state::Tool;

fn harness(tool: Tool) -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
    app.sync_views();
    app.ui.extras.rulers = false;
    app.ui.tool = tool;
    app.run("view.newGuide", json!({"orientation": "vertical", "position": 200})).unwrap();
    let mut h = Harness::builder().with_size(vec2(1000.0, 700.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            let ctx = ui.ctx().clone();
            if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            crate::shortcuts::handle(app, &ctx);
            egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::AppTheme::pro());
    h.run_steps(4);
    let v = &mut h.state_mut().ui.views[0];
    v.zoom = 1.0;
    v.center = [200.0, 150.0];
    v.fit_pending = false;
    h.run_steps(2);
    h
}

fn screen(h: &Harness<'static, PhotocraftApp>, x: f32, y: f32) -> Pos2 {

    crate::canvas::testing::to_screen(h.state(), [f64::from(x), f64::from(y)])
}

fn hover(h: &mut Harness<'static, PhotocraftApp>, x: f32, y: f32, m: Modifiers) -> egui::CursorIcon {
    h.event(egui::Event::ModifiersChanged(m));
    let p = screen(h, x, y);
    h.event(egui::Event::PointerMoved(p));
    h.run_steps(2);
    h.output().platform_output.cursor_icon
}

fn drag(h: &mut Harness<'static, PhotocraftApp>, from: f32, to: f32, m: Modifiers) {
    let p = screen(h, from, 150.0);
    h.event(egui::Event::PointerMoved(p));
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: m });
    h.run_steps(1);
    for k in 1..=4 {
        let q = screen(h, from + (to - from) * k as f32 / 4.0, 150.0);
        h.event(egui::Event::PointerMoved(q));
        h.run_steps(1);
    }
    let q = screen(h, to, 150.0);
    h.event(egui::Event::PointerButton { pos: q, button: PointerButton::Primary, pressed: false, modifiers: m });
    h.run_steps(2);
}

fn diagonal(h: &mut Harness<'static, PhotocraftApp>, a: [f32; 2], b: [f32; 2], m: Modifiers) {
    h.event(egui::Event::ModifiersChanged(m));
    let p = screen(h, a[0], a[1]);
    h.event(egui::Event::PointerMoved(p));
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: m });
    h.run_steps(1);
    for k in 1..=4 {
        let t = k as f32 / 4.0;
        let q = screen(h, a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
        h.event(egui::Event::PointerMoved(q));
        h.run_steps(1);
    }
    let q = screen(h, b[0], b[1]);
    h.event(egui::Event::PointerButton { pos: q, button: PointerButton::Primary, pressed: false, modifiers: m });
    h.run_steps(2);
}

/// The cursor over the guide with ⌘ held (or latched in Window › Modifier Keys, with no key held),
/// and where a ⌘-drag from 200 to 240 leaves it.
fn command_drag(tool: Tool, locked: bool, latched: bool) -> (egui::CursorIcon, Vec<f64>) {
    let mut h = harness(tool);
    h.state_mut().ui.extras.lock_guides = locked;
    let held = if latched {
        h.state_mut().ui.shell.latched = craft::keymap::Mods::CMD;
        Modifiers::NONE
    } else {
        Modifiers::COMMAND
    };
    let c = hover(&mut h, 200.0, 150.0, held);
    drag(&mut h, 200.0, 240.0, held);
    (c, h.state().session.active().unwrap().doc.guides.vertical.clone())
}

const GUIDE: egui::CursorIcon = egui::CursorIcon::ResizeHorizontal;

/// (tool, guides locked, ⌘ latched instead of held) → whether ⌘ takes the guide: the guide cursor
/// over it, and the guide after a drag from 200 to 240. ⌘ is the Move tool on the painting, selection,
/// Frame and Artboard tools. The tools that keep ⌘ for themselves (Photoshop: the Pen's Direct
/// Selection, the Hand, Zoom, Crop, type, shapes, slices) neither show the guide cursor nor drag
/// the guide, and a locked guide never moves. A latched ⌘ takes the guide and shows its cursor as a
/// held one does (the press took the guide but the cursor stayed the brush's).
#[test]
fn command_over_a_guide_takes_it_where_command_is_the_move_tool() {
    let takes = |tool, latched| (tool, false, latched, true);
    let rows = [
        takes(Tool::Brush, false),
        takes(Tool::Eraser, false),
        takes(Tool::Gradient, false),
        takes(Tool::PaintBucket, false),
        takes(Tool::CloneStamp, false),
        takes(Tool::Eyedropper, false),
        takes(Tool::Dodge, false),
        takes(Tool::SpotHealing, false),
        takes(Tool::Smudge, false),
        takes(Tool::QuickSelection, false),
        takes(Tool::RectMarquee, false),
        takes(Tool::Lasso, false),
        // The Frame and Artboard tools took a ⌘ press as their own (a new frame or board) although ⌘
        // is the Move tool on them, so the guide cursor promised a drag that never moved the guide.
        takes(Tool::Frame, false),
        takes(Tool::Artboard, false),
        takes(Tool::Brush, true),
        takes(Tool::Eraser, true),
        takes(Tool::RectMarquee, true),
        (Tool::Brush, true, false, false),
        (Tool::Eraser, true, false, false),
        (Tool::Frame, true, false, false),
        (Tool::Pen, false, false, false),
        (Tool::Hand, false, false, false),
        (Tool::Zoom, false, false, false),
        (Tool::Crop, false, false, false),
        (Tool::Type, false, false, false),
        (Tool::Rectangle, false, false, false),
        (Tool::Slice, false, false, false),
    ];
    for (tool, locked, latched, takes_guide) in rows {
        let (cursor, guides) = command_drag(tool, locked, latched);
        if takes_guide {
            assert_eq!((cursor, guides), (GUIDE, vec![240.0]), "{tool:?} latched={latched}");
        } else {
            assert_ne!(cursor, GUIDE, "{tool:?} locked={locked}");
            assert_eq!(guides, vec![200.0], "{tool:?} locked={locked}");
        }
    }
}

/// Off a guide ⌘ is the Move tool on the Frame tool too: a ⌘-drag draws no frame (the Frame tool
/// took ⌘), and without ⌘ it still draws one.
#[test]
fn a_command_drag_off_a_guide_draws_no_frame() {
    let mut h = harness(Tool::Frame);
    let layers = h.state().session.active().unwrap().doc.layers.len();
    diagonal(&mut h, [100.0, 100.0], [160.0, 140.0], Modifiers::COMMAND);
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), layers, "⌘-drag with the Frame tool draws no frame");
    assert!(!h.state().canvas_tools.frame.busy());
    diagonal(&mut h, [20.0, 20.0], [120.0, 90.0], Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), layers + 1, "a plain drag draws a frame");
}
