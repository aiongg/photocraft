//! #9648: with a filter dialog that has a preview window on top (Smart Sharpen, Unsharp Mask…),
//! the image under it is the preview's locator, as in Photoshop: the pointer is a small square
//! there, and a click centres the dialog's preview window on that document point, without reaching
//! the tool or making a history step. Only the press counts: a drag after it leaves the window
//! where the press put it, as in the reference application.

use craft::ui::dialog::DialogId;
use craft::ui::patterns::form_dialogs::HostedParamDialog;
use egui::{Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::state::Tool;

const SMART_SHARPEN: &str = "filter.sharpen.smartSharpen";

/// A 400×300 document at 400 % (so it fills the window round the dialog) with the Brush selected,
/// and the `command` filter dialog open over it when there is one.
fn harness(command: Option<&str>, rotation: f32) -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
    app.sync_views();
    app.ui.extras.rulers = false;
    app.ui.tool = Tool::Brush;
    let mut h = Harness::builder().with_context(app.ctx().clone()).with_size(vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            let ctx = ui.ctx().clone();
            if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            crate::shortcuts::handle(app, &ctx);
            egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
            crate::dialogs::run(app, ui);
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::AppTheme::pro());
    h.run_steps(4);
    // PLAN: assumed `ui.set` takes `rotation` beside `zoom` and `center` (it sets the active view).
    control(&mut h, "ui.set", json!({"zoom": 4.0, "center": [200.0, 150.0], "rotation": rotation}));
    if let Some(command) = command {
        crate::filter_dialog::open(h.state_mut(), command).expect("the dialog opens");
    }
    h.run_steps(4);
    h
}

fn control(h: &mut Harness<'static, PhotocraftApp>, method: &str, params: Value) {
    let ctx = h.ctx.clone();
    let (req, _rx) = crate::control::ControlRequest::new(method, params);
    let _ = crate::control::handle(h.state_mut(), &ctx, &req);
    h.run_steps(2);
}

/// The screen point of document point `p` in the shown view.
fn screen(h: &Harness<'static, PhotocraftApp>, p: [f64; 2]) -> Pos2 {
    // PLAN: assumed `canvas::testing::to_screen(&PhotocraftApp, [f64; 2]) -> Pos2` (cw.P2b).
    crate::canvas::testing::to_screen(h.state(), p)
}

fn dialog_id(h: &Harness<'static, PhotocraftApp>) -> DialogId {
    h.state().framework.dialogs.stack().top().expect("a dialog is open")
}

/// The preview window's centre, in document pixels.
fn centre(h: &Harness<'static, PhotocraftApp>) -> [f64; 2] {
    let dialog = h.state().framework.dialogs.get_by_id::<HostedParamDialog>(dialog_id(h)).expect("the filter dialog");
    let p = dialog.preview_centre().expect("the preview pane has shown");
    [f64::from(p.x), f64::from(p.y)]
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 0.5 && (a[1] - b[1]).abs() < 0.5
}

/// The document's revision and undo depth: a click that reached a tool or ran a command moves them.
fn edits(h: &Harness<'static, PhotocraftApp>) -> (u64, usize) {
    let st = h.state().session.active().unwrap();
    (st.revision, st.history.past_len())
}

/// Whether any pixel of the active layer in the box was darkened by the Brush.
fn painted(h: &Harness<'static, PhotocraftApp>, x: std::ops::Range<i32>, y: std::ops::Range<i32>) -> bool {
    let st = h.state().session.active().unwrap();
    let surface = st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap();
    y.flat_map(|y| x.clone().map(move |x| (x, y))).any(|(x, y)| surface.pixel(x, y)[0] < 0.99)
}

fn button(h: &mut Harness<'static, PhotocraftApp>, p: Pos2, down: bool) {
    h.event(egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: down, modifiers: Modifiers::NONE });
    h.run_steps(1);
}

fn move_to(h: &mut Harness<'static, PhotocraftApp>, p: Pos2) {
    h.event(egui::Event::PointerMoved(p));
    h.run_steps(1);
}

fn click(h: &mut Harness<'static, PhotocraftApp>, p: Pos2) {
    move_to(h, p);
    button(h, p, true);
    button(h, p, false);
    h.run_steps(2);
}

/// Sends `events` as one `ui.pointer` request, then lets the frames run: the tool host takes
/// the events on a later pass and the reply comes after.
fn pointer(h: &mut Harness<'static, PhotocraftApp>, events: Value) {
    let ctx = h.ctx.clone();
    let (req, _rx) = crate::control::ControlRequest::new("ui.pointer", json!({"events": events}));
    let _ = crate::control::handle(h.state_mut(), &ctx, &req);
    h.run_steps(3);
}

#[test]
fn a_click_on_the_image_centres_the_smart_sharpen_preview() {
    let mut h = harness(Some(SMART_SHARPEN), 0.0);
    // PLAN: assumed `filter_preview_ui::shows(command: &str) -> bool` (dialogs-w25).
    assert!(crate::filter_preview_ui::shows(SMART_SHARPEN), "Smart Sharpen has a preview window");
    assert!(near(centre(&h), [200.0, 150.0]), "it opens centred: {:?}", centre(&h));
    let before = edits(&h);

    // Over the image (left of the dialog) the pointer is the locator square; over the dialog it is
    // the normal pointer.
    let a_doc = [70.0, 230.0];
    let a = screen(&h, a_doc);
    move_to(&mut h, a);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::None, "the locator replaces the pointer");
    move_to(&mut h, pos2(600.0, 400.0));
    assert_ne!(h.output().platform_output.cursor_icon, egui::CursorIcon::None, "over the dialog it is the normal pointer");

    // A click centres the preview on the document point under it, through the canvas transform.
    click(&mut h, a);
    assert!(near(centre(&h), a_doc), "{:?} vs {a_doc:?}", centre(&h));

    // A drag centres it where it was pressed: neither its moves nor its release move it on.
    let b_doc = [85.0, 75.0];
    let b = screen(&h, b_doc);
    move_to(&mut h, b);
    button(&mut h, b, true);
    assert!(near(centre(&h), b_doc), "the press centres it: {:?} vs {b_doc:?}", centre(&h));
    for i in 1..=4 {
        move_to(&mut h, b + (a - b) * (i as f32 / 4.0));
        assert!(near(centre(&h), b_doc), "a held move leaves it: {:?} vs {b_doc:?}", centre(&h));
    }
    button(&mut h, a, false);
    h.run_steps(2);
    assert!(near(centre(&h), b_doc), "the release leaves it: {:?} vs {b_doc:?}", centre(&h));

    // The Brush never painted, no command ran, and the dialog stays open.
    assert_eq!(edits(&h), before, "no stroke and no history step");
    assert!(!crate::canvas::testing::in_gesture(h.state()), "no tool gesture started");
    assert_eq!(h.state().framework.dialogs.open_dialogs().len(), 1);

    // With the Hand tool the click centres the preview too (the tool doesn't matter).
    h.state_mut().ui.tool = Tool::Hand;
    click(&mut h, a);
    assert!(near(centre(&h), a_doc), "{:?}", centre(&h));
    assert_eq!(edits(&h), before);
}

#[test]
fn a_rotated_view_centres_the_preview_on_the_point_under_the_pointer() {
    let mut h = harness(Some(SMART_SHARPEN), 90.0);
    let at = [200.0, 90.0];
    let a = screen(&h, at);
    let before = edits(&h);
    click(&mut h, a);
    assert!(near(centre(&h), at), "{:?} vs {at:?}", centre(&h));
    assert_eq!(edits(&h), before);
}

#[test]
fn without_a_preview_dialog_the_click_still_reaches_the_tool() {
    let mut h = harness(None, 0.0);
    let before = edits(&h);
    click(&mut h, screen(&h, [70.0, 230.0]));
    let after = edits(&h);
    assert!(after.0 != before.0 && after.1 == before.1 + 1, "the Brush paints one stroke: {before:?} -> {after:?}");
}

#[test]
fn agents_centre_the_preview_with_ui_pointer_without_painting() {
    let mut h = harness(Some(SMART_SHARPEN), 0.0);
    let before = edits(&h);
    let events = json!([{"kind": "down", "x": 60.5, "y": 250}, {"kind": "move", "x": 40, "y": 30}, {"kind": "up", "x": 40, "y": 30}]);
    pointer(&mut h, events);
    assert!(near(centre(&h), [60.5, 250.0]), "the press decides, not the drag: {:?}", centre(&h));
    assert_eq!(edits(&h), before, "the Brush must not paint under the dialog");

    // With the dialog cancelled the same gesture paints with the Brush again.
    let id = dialog_id(&h);
    let ctx = h.ctx.clone();
    h.state_mut().framework.dialogs.cancel_by_id(&ctx, id);
    h.run_steps(3);
    let events = json!([{"kind": "down", "x": 40, "y": 30}, {"kind": "move", "x": 60, "y": 30}, {"kind": "up", "x": 60, "y": 30}]);
    pointer(&mut h, events);
    assert_ne!(edits(&h).0, before.0, "the Brush paints once no preview dialog is open");
}

/// Agents send a press, its moves and its release as separate requests: only the press centres
/// the preview; a hover, a move with the press held and the release leave it.
#[test]
fn ui_pointer_centres_only_on_the_press() {
    let mut h = harness(Some(SMART_SHARPEN), 0.0);
    pointer(&mut h, json!([{"kind": "move", "x": 40, "y": 30}]));
    // An event without a kind is a move, so a hover too.
    pointer(&mut h, json!([{"x": 50, "y": 40}]));
    assert!(near(centre(&h), [200.0, 150.0]), "a hover leaves the preview where it was: {:?}", centre(&h));
    pointer(&mut h, json!([{"kind": "down", "x": 40, "y": 30}]));
    assert!(near(centre(&h), [40.0, 30.0]), "{:?}", centre(&h));
    pointer(&mut h, json!([{"kind": "move", "x": 80, "y": 60}]));
    assert!(near(centre(&h), [40.0, 30.0]), "a move while the press is held leaves it: {:?}", centre(&h));
    pointer(&mut h, json!([{"kind": "up", "x": 80, "y": 60}]));
    pointer(&mut h, json!([{"kind": "move", "x": 120, "y": 90}]));
    assert!(near(centre(&h), [40.0, 30.0]), "so do the release and a later hover: {:?}", centre(&h));
}

/// A Brush stroke an agent began before the dialog opened still ends along its own path: its moves
/// and release reach the tool, not the locator.
#[test]
fn a_tool_gesture_begun_before_the_dialog_still_ends_under_it() {
    let mut h = harness(None, 0.0);
    let before = edits(&h);
    pointer(&mut h, json!([{"kind": "down", "x": 40, "y": 30}, {"kind": "move", "x": 80, "y": 30}]));
    assert!(crate::canvas::testing::in_gesture(h.state()), "the Brush stroke is under way");
    crate::filter_dialog::open(h.state_mut(), SMART_SHARPEN).expect("the dialog opens");
    h.run_steps(2);
    assert!(crate::canvas::testing::in_gesture(h.state()), "the stroke is still open under the dialog");
    pointer(&mut h, json!([{"kind": "move", "x": 80, "y": 120}, {"kind": "move", "x": 120, "y": 120}]));
    pointer(&mut h, json!([{"kind": "up", "x": 120, "y": 120}]));
    assert!(!crate::canvas::testing::in_gesture(h.state()), "the release reached the Brush");
    assert_eq!(edits(&h).1, before.1 + 1, "the stroke is one history step");
    assert!(painted(&h, 95..105, 115..125), "the moves under the dialog reached the stroke");
    assert!(near(centre(&h), [200.0, 150.0]), "the stroke's moves and release are not locator presses: {:?}", centre(&h));
}

/// The Crop tool keeps its frame drag apart from the other tools' gestures: one an agent began before
/// the dialog opened still ends along its path, so no stale drag reshapes the frame on later hovers.
#[test]
fn a_crop_drag_begun_before_the_dialog_still_ends_under_it() {
    use crate::tools::crop_ui;
    let extent = |h: &Harness<'static, PhotocraftApp>| {
        let corners = crop_ui::frame(h.state()).expect("a crop frame").corners();
        // PLAN: assumed `Point { x, y }` fields of the behavior geometry point.
        (corners.iter().map(|p| p.x).fold(f64::MIN, f64::max), corners.iter().map(|p| p.y).fold(f64::MIN, f64::max))
    };
    let mut h = harness(None, 0.0);
    h.state_mut().ui.tool = Tool::Crop;
    h.run_steps(2);
    pointer(&mut h, json!([{"kind": "down", "x": 40, "y": 30}, {"kind": "move", "x": 80, "y": 60}]));
    assert!(crop_ui::dragging(h.state()), "the Crop frame drag is under way");
    crate::filter_dialog::open(h.state_mut(), SMART_SHARPEN).expect("the dialog opens");
    h.run_steps(2);
    pointer(&mut h, json!([{"kind": "move", "x": 160, "y": 120}]));
    let (x, y) = extent(&h);
    assert!(x > 150.0 && y > 110.0, "the move under the dialog reached the frame before the release: {x}, {y}");
    pointer(&mut h, json!([{"kind": "up", "x": 160, "y": 120}]));
    assert!(!crop_ui::dragging(h.state()), "the release reached the Crop tool");
    let (x, y) = extent(&h);
    assert!(x > 150.0 && y > 110.0, "the moves under the dialog reached the frame: {x}, {y}");
    assert!(near(centre(&h), [200.0, 150.0]), "the drag's moves and release are not locator presses: {:?}", centre(&h));
}

/// A second press while an agent's Brush stroke is still open (no mouse can make one) is dropped:
/// it neither centres the preview nor reaches the Brush, and the stroke's own moves and release
/// still finish it.
#[test]
fn a_press_during_an_open_tool_gesture_is_dropped() {
    let mut h = harness(None, 0.0);
    let before = edits(&h);
    pointer(&mut h, json!([{"kind": "down", "x": 40, "y": 30}, {"kind": "move", "x": 80, "y": 30}]));
    crate::filter_dialog::open(h.state_mut(), SMART_SHARPEN).expect("the dialog opens");
    h.run_steps(2);
    pointer(&mut h, json!([{"kind": "down", "x": 300, "y": 200}]));
    assert!(near(centre(&h), [200.0, 150.0]), "the extra press doesn't centre the preview: {:?}", centre(&h));
    assert!(!painted(&h, 290..310, 190..210), "nor does it reach the open stroke");
    pointer(&mut h, json!([{"kind": "move", "x": 80, "y": 120}]));
    assert!(near(centre(&h), [200.0, 150.0]), "the stroke's moves are not a locator drag: {:?}", centre(&h));
    pointer(&mut h, json!([{"kind": "up", "x": 80, "y": 120}]));
    assert!(!crate::canvas::testing::in_gesture(h.state()), "the release ended the stroke");
    assert!(painted(&h, 75..85, 100..125), "the move reached the stroke");
    assert_eq!(edits(&h).1, before.1 + 1, "the stroke is one history step");
}
