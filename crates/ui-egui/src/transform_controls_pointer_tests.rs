//! Move tool transform controls through the control channel (#11105), and Free Transform's snapping.

use egui::Modifiers;
use craft::ui::dialog::DialogId;
use photocraft_doc::LayerId;
use photocraft_geom::Rect;
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::canvas::testing::{key, move_to, press, release};
use crate::state::Tool;

/// Sends a control request and returns its reply, through the canvas driver's control channel (`testing::control`).
pub(crate) fn call(app: &mut PhotocraftApp, _ctx: &egui::Context, method: &str, params: Value) -> Value {
    crate::canvas::testing::control(app, method, params)
}

/// `call` that asserts the reply is ok.
pub(crate) fn accepted(app: &mut PhotocraftApp, ctx: &egui::Context, method: &str, params: Value) {
    let reply = call(app, ctx, method, params);
    assert_eq!(reply["ok"], true, "{method}: {reply}");
}

/// Two selected shapes share the measured controls frame (100,100)..(800,500).
fn fixture() -> (PhotocraftApp, egui::Context, [LayerId; 2]) {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 1000, "height": 700, "background": "white"})).unwrap();
    let mut layers = Vec::new();
    for rect in [[100, 100, 200, 100], [600, 400, 200, 100]] {
        app.run("shape.create", json!({"kind": "rect", "rect": rect, "fill": "#ff0000"})).unwrap();
        layers.push(app.session.active().unwrap().active_layer.unwrap());
    }
    app.run("layer.select", json!({"layer": layers[0].0, "mode": "add"})).unwrap();
    app.sync_views();
    crate::canvas::testing::window(&mut app, egui::vec2(1400.0, 900.0));
    app.ui.tool_options.move_auto_select = false;
    app.ui.extras.snap = false;
    app.ui.view.show.smart_guides = false;
    let ctx = crate::canvas::testing::ctx(&mut app);
    accepted(&mut app, &ctx, "ui.set", json!({"tool": "Move", "moveShowTransform": true, "zoom": 1.0, "center": [500, 350]}));
    (app, ctx, [layers[0], layers[1]])
}

fn bounds(app: &PhotocraftApp, id: LayerId) -> Rect {
    let state = app.session.active().unwrap();
    photocraft_engine::transform_cmds::transform_bounds(&state.doc, state.doc.layer(id).unwrap())
}

fn pointer(app: &mut PhotocraftApp, ctx: &egui::Context, kind: &str, at: [f64; 2]) {
    accepted(app, ctx, "ui.pointer", json!({"events": [{"kind": kind, "x": at[0], "y": at[1]}]}));
}

fn enter(app: &mut PhotocraftApp) {
    key(app, egui::Key::Enter, Modifiers::NONE);
}

/// The open session's box corners.
fn quad(app: &PhotocraftApp) -> [[f64; 2]; 4] {
    app.ui.transform.as_ref().expect("a Free Transform session is open").session.quad().map(|p| [p.x, p.y])
}

/// The open session's reference point.
fn pivot(app: &PhotocraftApp) -> [f64; 2] {
    let p = app.ui.transform.as_ref().expect("a Free Transform session is open").session.pivot();
    [p.x, p.y]
}

fn dialog_count(app: &PhotocraftApp) -> usize {
    app.framework.dialogs.stack().ids().count()
}

fn top_dialog(app: &PhotocraftApp) -> DialogId {
    app.framework.dialogs.stack().top().expect("a dialog is open")
}

/// The Color Picker's colour field, from the hosted dialog's `json_fields` as the host lists it.
fn color_field(app: &PhotocraftApp, id: DialogId) -> Value {
    let open = app.framework.dialogs.open_dialogs().into_iter().find(|dialog| dialog.id == id).expect("the Color Picker is open");
    open.fields.expect("the Color Picker lists its fields")["color"].clone()
}

fn close_dialog(app: &mut PhotocraftApp, ctx: &egui::Context, id: DialogId) {
    assert!(app.framework.dialogs.cancel_by_id(ctx, id), "the dialog was open");
}

fn assert_one_transform(app: &PhotocraftApp, steps: usize) {
    assert!(app.ui.transform.is_none(), "Enter closes Free Transform");
    let state = app.session.active().unwrap();
    assert_eq!(state.history.past_len(), steps + 1);
    assert_eq!(state.history.entries().last().map(String::as_str), Some("Free Transform"));
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Opens {
    /// A Free Transform session that scales both layers.
    Scale,
    /// A Free Transform session that rotates both layers.
    Rotate,
    /// An ordinary Move drag.
    Move,
    /// Nothing.
    Nothing,
}

/// Where a press lands decides the gesture: a corner, the rotation band at 100 %, the visible corner at 200 %, a handle
/// off the canvas and the interior.
#[test]
fn move_controls_press_zones_start_the_matching_gesture() {
    for (zoom, center, down, to, expect) in [
        (1.0, [500, 350], [800.0, 500.0], [700.0, 440.0], Opens::Scale),
        // Fourteen points beyond the corner: outside its 12pt handle, inside the rotation band.
        (1.0, [500, 350], [814.0, 500.0], [800.0, 514.0], Opens::Rotate),
        (2.0, [500, 350], [800.0, 500.0], [700.0, 440.0], Opens::Scale),
        (3.0, [500, 350], [800.0, 500.0], [700.0, 440.0], Opens::Nothing),
        (1.0, [0, 0], [800.0, 500.0], [700.0, 440.0], Opens::Nothing),
        (1.0, [500, 350], [200.0, 150.0], [220.0, 160.0], Opens::Move),
    ] {
        let (mut app, ctx, layers) = fixture();
        accepted(&mut app, &ctx, "ui.set", json!({"zoom": zoom, "center": center}));
        let before = app.session.active().unwrap().doc.clone();
        let old = layers.map(|id| bounds(&app, id));
        let steps = app.session.active().unwrap().history.past_len();
        let what = format!("zoom {zoom}, center {center:?}, press {down:?}");
        pointer(&mut app, &ctx, "down", down);
        match expect {
            Opens::Nothing => {
                assert!(app.ui.transform.is_none(), "{what}: a hidden handle cannot be pressed");
                pointer(&mut app, &ctx, "move", to);
                pointer(&mut app, &ctx, "up", to);
                assert!(app.ui.transform.is_none(), "{what}");
                assert_eq!(app.session.active().unwrap().doc, before, "{what}");
                assert_eq!(app.session.active().unwrap().history.past_len(), steps, "{what}");
                continue;
            }
            Opens::Move => {
                assert!(app.ui.transform.is_none(), "{what}");
                pointer(&mut app, &ctx, "move", to);
                pointer(&mut app, &ctx, "up", to);
                for (id, previous) in layers.into_iter().zip(old) {
                    assert_eq!(bounds(&app, id), previous.translate(20, 10), "{what}");
                }
                assert!(app.ui.transform.is_none(), "{what}");
                assert_eq!(app.session.active().unwrap().history.past_len(), steps + 1, "{what}");
                continue;
            }
            Opens::Scale | Opens::Rotate => {}
        }
        let transform = app.ui.transform.as_ref().unwrap_or_else(|| panic!("{what}: Move controls open Free Transform"));
        let r = transform.session.rect();
        assert_eq!([r.x0, r.y0, r.x1, r.y1], [100.0, 100.0, 800.0, 500.0], "{what}");
        assert_eq!(transform.layers, layers.map(|id| id.0), "{what}");
        let initial = quad(&app);
        pointer(&mut app, &ctx, "move", to);
        pointer(&mut app, &ctx, "up", to);
        assert!(app.ui.transform.is_some(), "{what}: Up ends the gesture, not the transform session");
        assert_eq!(app.session.active().unwrap().doc, before, "{what}: the preview has not changed the document");
        assert_eq!(app.session.active().unwrap().history.past_len(), steps, "{what}");
        let preview = quad(&app);
        assert_ne!(preview, initial, "{what}");
        let expected = if expect == Opens::Rotate {
            assert!((preview[1][1] - preview[0][1]).abs() > 1.0, "{what}: the upper edge rotates");
            // Both committed layer bounds follow the preview homography; a translation or one-layer rotation fails.
            let r = app.ui.transform.as_ref().unwrap().session.rect();
            let homography = photocraft_algo::transform::Homography::rect_to_quad([r.x0, r.y0, r.x1, r.y1], preview).unwrap();
            Some(old.map(|r| {
                [[r.x0, r.y0], [r.x1, r.y0], [r.x1, r.y1], [r.x0, r.y1]]
                    .map(|[x, y]| homography.apply(f64::from(x), f64::from(y)))
                    .into_iter()
                    .fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |b, (x, y)| [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)])
            }))
        } else {
            None
        };
        enter(&mut app);
        assert_one_transform(&app, steps);
        let after = app.session.active().unwrap().doc.clone();
        assert_ne!(after, before, "{what}: Enter commits the transform");
        for (index, (id, previous)) in layers.into_iter().zip(old).enumerate() {
            let now = bounds(&app, id);
            match &expected {
                None => assert!(now.width() < previous.width() && now.height() < previous.height(), "{what}: both layers scale: {previous:?} -> {now:?}"),
                Some(expected) => {
                    let expected = expected[index];
                    assert!((expected[2] - expected[0] - f64::from(previous.width())).abs() > 2.0, "{what}: this fixture distinguishes rotation from translation");
                    // The preview frame is continuous; committed raster bounds round to whole pixels.
                    for (actual, expected) in [now.x0, now.y0, now.x1, now.y1].into_iter().zip(expected) {
                        assert!((f64::from(actual) - expected).abs() <= 1.0, "{what}: layer {id:?}: committed bound {actual} does not match preview {expected}");
                    }
                }
            }
        }
        assert!(app.session.undo());
        assert_eq!(app.session.active().unwrap().doc, before, "{what}: one Undo restores both layers");
        assert!(app.session.redo());
        assert_eq!(app.session.active().unwrap().doc, after, "{what}: one Redo restores the transform");
    }
}

/// Visibility and setting gates must reject entry even when the point geometrically hits a handle.
#[test]
fn disabled_hidden_and_missed_controls_do_not_open_a_transform() {
    for (enabled, extras, at) in [(false, true, [800.0, 500.0]), (true, false, [800.0, 500.0]), (true, true, [900.0, 600.0])] {
        let (mut app, ctx, _) = fixture();
        accepted(&mut app, &ctx, "ui.set", json!({"moveShowTransform": enabled}));
        app.ui.view.extras = extras;
        pointer(&mut app, &ctx, "down", at);
        assert!(app.ui.transform.is_none(), "enabled={enabled}, Extras={extras}, at={at:?}");
        pointer(&mut app, &ctx, "up", at);
        assert!(app.ui.transform.is_none());
    }
}

/// Secondary buttons and Space-pan retain their established input ownership.
#[test]
fn nonprimary_and_space_presses_do_not_take_over_move_controls() {
    for params in [json!({"button": "right"}), json!({"button": "middle"}), json!({"space": true})] {
        let (mut app, ctx, _) = fixture();
        let mut params = params;
        params["events"] = json!([{"kind": "down", "x": 800, "y": 500}]);
        accepted(&mut app, &ctx, "ui.pointer", params);
        assert!(app.ui.transform.is_none());
    }
}

/// Disabling Move controls must not disable the explicit Free Transform command.
#[test]
fn explicit_free_transform_still_works_with_move_controls_disabled() {
    let (mut app, ctx, _) = fixture();
    accepted(&mut app, &ctx, "ui.set", json!({"moveShowTransform": false}));
    crate::menus::invoke(&mut app, &ctx, "edit.freeTransform", json!({})).unwrap();
    let steps = app.session.active().unwrap().history.past_len();
    pointer(&mut app, &ctx, "down", [800.0, 500.0]);
    pointer(&mut app, &ctx, "move", [700.0, 440.0]);
    pointer(&mut app, &ctx, "up", [700.0, 440.0]);
    enter(&mut app);
    assert_one_transform(&app, steps);
}

/// A malformed boolean must reject the complete patch before a valid sibling field changes.
#[test]
fn invalid_move_show_transform_is_atomic_with_other_ui_fields() {
    for invalid in [json!("true"), json!(1), json!(null), json!([]), json!({})] {
        let (mut app, ctx, _) = fixture();
        accepted(&mut app, &ctx, "ui.set", json!({"moveShowTransform": false}));
        let before = serde_json::to_value(&app.ui).unwrap();
        let doc = app.session.active().unwrap().doc.clone();
        let steps = app.session.active().unwrap().history.past_len();
        let reply = call(&mut app, &ctx, "ui.set", json!({"tool": "Brush", "zoom": 2.0, "moveShowTransform": invalid}));
        assert_eq!(reply["ok"], false, "{reply}");
        assert!(reply["error"].as_str().unwrap().contains("moveShowTransform"), "{reply}");
        assert_eq!(serde_json::to_value(&app.ui).unwrap(), before, "no UI field changed");
        assert_eq!(app.session.active().unwrap().doc, doc);
        assert_eq!(app.session.active().unwrap().history.past_len(), steps);
    }
}

/// Omission retains the setting, and later validation failure cannot partially apply a new value.
#[test]
fn move_show_transform_is_retained_when_omitted_and_waits_for_all_validation() {
    let (mut app, ctx, _) = fixture();
    accepted(&mut app, &ctx, "ui.set", json!({"moveShowTransform": false}));
    let reply = call(&mut app, &ctx, "ui.set", json!({"moveShowTransform": true, "zoom": "bad"}));
    assert_eq!(reply["ok"], false);
    assert!(!app.ui.tool_options.move_show_transform);
    accepted(&mut app, &ctx, "ui.set", json!({"moveShowTransform": true}));
    accepted(&mut app, &ctx, "ui.set", json!({"tool": "Brush"}));
    assert_eq!(app.ui.tool, Tool::Brush);
    assert!(app.ui.tool_options.move_show_transform);
}

/// A malformed later sample must be rejected before the first valid Down can open a session.
#[test]
fn invalid_later_pointer_event_does_not_begin_a_transform() {
    let (mut app, ctx, _) = fixture();
    let before = app.session.active().unwrap().doc.clone();
    let steps = app.session.active().unwrap().history.past_len();
    let reply = call(
        &mut app,
        &ctx,
        "ui.pointer",
        json!({"events": [
            {"kind": "down", "x": 800, "y": 500},
            {"kind": "unknown", "x": 700, "y": 440}
        ]}),
    );
    assert_eq!(reply["ok"], false);
    assert!(app.ui.transform.is_none());
    assert!(!crate::canvas::testing::in_gesture(&app));
    assert_eq!(app.session.active().unwrap().doc, before);
    assert_eq!(app.session.active().unwrap().history.past_len(), steps);
}

/// A modal owns the whole gesture, including ordinary Move with transform controls disabled.
#[test]
fn modal_dialogs_do_not_start_move_transform_controls() {
    for command in ["file.new", "edit.preferences.general"] {
        let (mut app, ctx, _) = fixture();
        crate::menus::invoke(&mut app, &ctx, command, json!({})).unwrap();
        assert_eq!(dialog_count(&app), 1, "{command} opened its modal");
        let before = app.session.active().unwrap().doc.clone();
        let steps = app.session.active().unwrap().history.past_len();
        let selected = app.session.active().unwrap().selected_layers();
        let active = app.session.active().unwrap().active_layer;
        for controls in [true, false] {
            app.ui.tool_options.move_show_transform = controls;
            for from in [[800.0, 500.0], [200.0, 150.0]] {
                let to = [from[0] + 20.0, from[1] + 10.0];
                for (kind, at) in [("down", from), ("move", to), ("up", to)] {
                    pointer(&mut app, &ctx, kind, at);
                    assert!(app.ui.transform.is_none(), "{command} owns input before the Move handle entry");
                    assert!(!crate::canvas::testing::in_gesture(&app), "{command} must not start an ordinary Move drag");
                    assert!(app.layer_box.is_none());
                }
                assert_eq!(app.session.active().unwrap().doc, before);
                assert_eq!(app.session.active().unwrap().history.past_len(), steps);
                assert_eq!(app.session.active().unwrap().selected_layers(), selected);
                assert_eq!(app.session.active().unwrap().active_layer, active);
                assert_eq!(app.ui.tool, Tool::Move);
                assert_eq!(app.ui.tool_options.move_show_transform, controls);
                assert_eq!(dialog_count(&app), 1, "the gesture leaves the modal open");
            }
        }
    }
}

/// A dialog opened after Down must not strand the Move drag or replace its starting point.
#[test]
fn ordinary_move_gesture_can_finish_after_a_dialog_opens() {
    let (mut app, ctx, layers) = fixture();
    let old = layers.map(|id| bounds(&app, id));
    let steps = app.session.active().unwrap().history.past_len();
    pointer(&mut app, &ctx, "down", [200.0, 150.0]);
    assert!(crate::canvas::testing::in_gesture(&app));
    accepted(&mut app, &ctx, "ui.dialog.open", json!({"kind": "about"}));
    let dialog = top_dialog(&app);
    pointer(&mut app, &ctx, "down", [250.0, 175.0]);
    assert!(crate::canvas::testing::in_gesture(&app), "the modal rejects a replacement Down");
    pointer(&mut app, &ctx, "move", [220.0, 160.0]);
    pointer(&mut app, &ctx, "up", [220.0, 160.0]);
    assert!(!crate::canvas::testing::in_gesture(&app));
    assert!(app.ui.transform.is_none());
    for (id, previous) in layers.into_iter().zip(old) {
        // The offset is measured from the first press, not the replacement Down.
        assert_eq!(bounds(&app, id), previous.translate(20, 10));
    }
    assert_eq!(app.session.active().unwrap().history.past_len(), steps + 1);
    assert_eq!(top_dialog(&app), dialog);
    close_dialog(&mut app, &ctx, dialog);
    let after = app.session.active().unwrap().doc.clone();
    pointer(&mut app, &ctx, "move", [270.0, 190.0]);
    pointer(&mut app, &ctx, "up", [270.0, 190.0]);
    assert_eq!(app.session.active().unwrap().doc, after, "later input cannot resume the finished gesture");
    assert_eq!(app.session.active().unwrap().history.past_len(), steps + 1);
}

/// A modal may interrupt a handle drag, but Up still ends its gesture before Enter commits.
#[test]
fn transform_handle_gesture_can_finish_after_a_dialog_opens() {
    let (mut app, ctx, layers) = fixture();
    let before = app.session.active().unwrap().doc.clone();
    let old = layers.map(|id| bounds(&app, id));
    let steps = app.session.active().unwrap().history.past_len();
    pointer(&mut app, &ctx, "down", [800.0, 500.0]);
    assert!(app.ui.transform.is_some(), "the handle starts Free Transform");
    let initial = quad(&app);
    accepted(&mut app, &ctx, "ui.dialog.open", json!({"kind": "about"}));
    let dialog = top_dialog(&app);
    pointer(&mut app, &ctx, "down", [200.0, 150.0]);
    assert_eq!(quad(&app), initial, "the modal cannot move the preview on Down");
    pointer(&mut app, &ctx, "move", [700.0, 440.0]);
    pointer(&mut app, &ctx, "up", [700.0, 440.0]);
    let preview = quad(&app);
    assert_ne!(preview, initial, "the existing handle gesture still updates its preview");
    assert_eq!(app.session.active().unwrap().doc, before);
    assert_eq!(app.session.active().unwrap().history.past_len(), steps);
    assert_eq!(top_dialog(&app), dialog);
    close_dialog(&mut app, &ctx, dialog);
    pointer(&mut app, &ctx, "move", [650.0, 410.0]);
    pointer(&mut app, &ctx, "up", [650.0, 410.0]);
    assert_eq!(quad(&app), preview, "later input cannot resume the released handle");
    enter(&mut app);
    assert_one_transform(&app, steps);
    for (id, previous) in layers.into_iter().zip(old) {
        let scaled = bounds(&app, id);
        assert!(scaled.width() < previous.width() && scaled.height() < previous.height());
    }
}

/// Full-window dialogs with embedded previews must not forward ordinary Move gestures.
#[test]
fn modal_workspaces_without_tool_event_controllers_do_not_move_layers() {
    for command in ["filter.filterGallery", "filter.adaptiveWideAngle"] {
        let (mut app, ctx, layers) = fixture();
        app.run("layer.rasterize.layer", json!({"layer": layers[0].0})).unwrap();
        app.run("layer.select", json!({"layer": layers[0].0})).unwrap();
        crate::menus::invoke(&mut app, &ctx, command, json!({})).unwrap();
        assert_eq!(crate::distort_ui::modal_workspace(&app), Some(command));
        let before = app.session.active().unwrap().doc.clone();
        let steps = app.session.active().unwrap().history.past_len();
        for controls in [true, false] {
            app.ui.tool_options.move_show_transform = controls;
            for (kind, at) in [("down", [200.0, 150.0]), ("move", [220.0, 160.0]), ("up", [220.0, 160.0])] {
                pointer(&mut app, &ctx, kind, at);
                assert!(app.ui.transform.is_none());
                assert!(!crate::canvas::testing::in_gesture(&app));
            }
            assert_eq!(app.session.active().unwrap().doc, before);
            assert_eq!(app.session.active().unwrap().history.past_len(), steps);
            assert_eq!(app.ui.tool, Tool::Move);
            assert_eq!(crate::distort_ui::modal_workspace(&app), Some(command));
        }
    }
}

/// Modal canvas samplers retain ownership even when Move and its controls are selected.
#[test]
fn move_pointer_still_samples_into_the_color_picker() {
    let (mut app, ctx, _) = fixture();
    let before = app.session.active().unwrap().doc.clone();
    let steps = app.session.active().unwrap().history.past_len();
    let _ = crate::color_picker_ui::open(&mut app, "foreground");
    let id = crate::color_picker_ui::top(&app).expect("the Color Picker is open");
    for (kind, at) in [("down", [200.0, 150.0]), ("move", [220.0, 160.0]), ("up", [220.0, 160.0])] {
        pointer(&mut app, &ctx, kind, at);
    }
    assert_eq!(color_field(&app, id), "#ff0000");
    assert!(app.ui.transform.is_none());
    assert!(!crate::canvas::testing::in_gesture(&app));
    assert_eq!(app.session.active().unwrap().doc, before);
    assert_eq!(app.session.active().unwrap().history.past_len(), steps);
}

/// A full-window editor receives its own stroke instead of a Move or Free Transform gesture.
#[test]
fn move_pointer_still_reaches_the_liquify_workspace() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 120, "height": 80, "depth": 8})).unwrap();
    app.run("layer.new.layer", json!({})).unwrap();
    app.sync_views();
    let ctx = crate::canvas::testing::ctx(&mut app);
    accepted(&mut app, &ctx, "ui.set", json!({"tool": "Move", "moveShowTransform": true}));
    crate::menus::invoke(&mut app, &ctx, "filter.liquify", json!({})).unwrap();
    let before = app.session.active().unwrap().doc.clone();
    let steps = app.session.active().unwrap().history.past_len();
    for (kind, at) in [("down", [50.0, 40.0]), ("move", [60.0, 40.0]), ("up", [60.0, 40.0])] {
        pointer(&mut app, &ctx, kind, at);
    }
    let fields = crate::liquify_ui::describe(&app).expect("the Liquify workspace is open");
    assert_eq!(fields["strokes"].as_u64(), Some(1));
    assert!(app.ui.transform.is_none());
    assert!(!crate::canvas::testing::in_gesture(&app));
    assert_eq!(app.session.active().unwrap().doc, before);
    assert_eq!(app.session.active().unwrap().history.past_len(), steps);
}

/// A sampling dialog owns its release without leaving an earlier transform drag armed.
#[test]
fn color_picker_release_ends_an_existing_transform_handle_drag() {
    let (mut app, ctx, layers) = fixture();
    let before = app.session.active().unwrap().doc.clone();
    let old = layers.map(|id| bounds(&app, id));
    let steps = app.session.active().unwrap().history.past_len();
    pointer(&mut app, &ctx, "down", [800.0, 500.0]);
    let initial = quad(&app);
    pointer(&mut app, &ctx, "move", [700.0, 440.0]);
    let preview = quad(&app);
    assert_ne!(preview, initial);
    accepted(&mut app, &ctx, "ui.dialog.open", json!({"kind": "colorPicker"}));
    let dialog = crate::color_picker_ui::top(&app).expect("the Color Picker is open");
    pointer(&mut app, &ctx, "down", [200.0, 150.0]);
    pointer(&mut app, &ctx, "move", [220.0, 160.0]);
    assert_eq!(color_field(&app, dialog), "#ff0000");
    pointer(&mut app, &ctx, "up", [999.0, 690.0]);
    assert_eq!(color_field(&app, dialog), "#ff0000", "release does not resample");
    assert_eq!(quad(&app), preview, "dialog-owned coordinates do not update the preview");
    assert_eq!(app.session.active().unwrap().doc, before);
    assert_eq!(app.session.active().unwrap().history.past_len(), steps);
    close_dialog(&mut app, &ctx, dialog);
    pointer(&mut app, &ctx, "move", [650.0, 410.0]);
    pointer(&mut app, &ctx, "up", [650.0, 410.0]);
    assert_eq!(quad(&app), preview, "later input needs a new press");
    enter(&mut app);
    assert_one_transform(&app, steps);
    for (id, previous) in layers.into_iter().zip(old) {
        let scaled = bounds(&app, id);
        assert!(scaled.width() < previous.width() && scaled.height() < previous.height());
    }
}

/// ⌘ makes another tool the Move tool for the press: a modal blocks that layer drag too.
#[test]
fn modal_dialogs_block_a_command_held_move_from_another_tool() {
    let (mut app, ctx, _) = fixture();
    accepted(&mut app, &ctx, "ui.set", json!({"tool": "Brush"}));
    crate::menus::invoke(&mut app, &ctx, "file.new", json!({})).unwrap();
    assert_eq!(dialog_count(&app), 1);
    let before = app.session.active().unwrap().doc.clone();
    let steps = app.session.active().unwrap().history.past_len();
    for (kind, at) in [("down", [200.0, 150.0]), ("move", [220.0, 160.0]), ("up", [220.0, 160.0])] {
        accepted(&mut app, &ctx, "ui.pointer", json!({"command": true, "events": [{"kind": kind, "x": at[0], "y": at[1]}]}));
        assert!(!crate::canvas::testing::in_gesture(&app), "{kind}: the modal owns the ⌘ Move press");
    }
    assert_eq!(app.session.active().unwrap().doc, before);
    assert_eq!(app.session.active().unwrap().history.past_len(), steps);
    assert_eq!(app.ui.tool, Tool::Brush);
}

/// The number of Smart Guide lines the tool host reported in its last pass.
fn guide_lines(app: &PhotocraftApp) -> usize {
    app.canvas_tools.last_response.as_ref().map_or(0, |response| response.snapped.len())
}

/// The Color Picker takes the release of a snapped handle drag: its alignment lines go with it.
#[test]
fn color_picker_release_clears_the_handle_drags_snap_lines() {
    let (mut app, ctx, _) = fixture();
    app.ui.extras.snap = true;
    app.ui.view.show.smart_guides = true;
    pointer(&mut app, &ctx, "down", [800.0, 500.0]);
    assert!(app.ui.transform.is_some());
    // 2 px from the document's centre (500, 350): the corner snaps there and the guides show.
    pointer(&mut app, &ctx, "move", [502.0, 352.0]);
    assert!(guide_lines(&app) > 0, "a snapped handle drag reports guide lines");
    accepted(&mut app, &ctx, "ui.dialog.open", json!({"kind": "colorPicker"}));
    pointer(&mut app, &ctx, "up", [999.0, 690.0]);
    assert_eq!(guide_lines(&app), 0, "the released drag reports no alignment lines");
}

/// Every sampler that takes the primary release ends a transform handle drag begun before it,
/// not only the Color Picker: the Curves eyedroppers and Color Range too. A move after the
/// sampler closes needs a new press.
#[test]
fn other_samplers_releases_end_an_existing_transform_handle_drag() {
    for sampler in ["curves", "colorRange"] {
        let (mut app, ctx, _) = fixture();
        pointer(&mut app, &ctx, "down", [800.0, 500.0]);
        pointer(&mut app, &ctx, "move", [700.0, 440.0]);
        let preview = quad(&app);
        let dialog = match sampler {
            "curves" => {
                let id = crate::adjust_dialog::open(&mut app, "image.adjustments.curves").expect("the Curves dialog opens");
                crate::adjust_dialog::hosted_mut(&mut app, id).set_picker(Some(craft::imaging::ui::adjust::TonePicker::Black));
                assert!(crate::dialogs::canvas_input::armed(&app).is_some(), "{sampler}");
                id
            }
            _ => {
                crate::menus::invoke(&mut app, &ctx, "select.colorRange", json!({})).unwrap();
                top_dialog(&app)
            }
        };
        pointer(&mut app, &ctx, "down", [200.0, 150.0]);
        pointer(&mut app, &ctx, "up", [999.0, 690.0]);
        assert_eq!(quad(&app), preview, "{sampler}: the sampler's coordinates do not move the box");
        close_dialog(&mut app, &ctx, dialog);
        pointer(&mut app, &ctx, "move", [650.0, 410.0]);
        assert_eq!(quad(&app), preview, "{sampler}: later moves need a new press");
    }
}

/// The Color Picker takes the release of an ordinary Move drag: the drag ends where it last was, and
/// moving after the picker closes needs a new press.
#[test]
fn color_picker_release_ends_an_ordinary_move_drag() {
    let (mut app, ctx, layers) = fixture();
    let old = layers.map(|id| bounds(&app, id));
    let steps = app.session.active().unwrap().history.past_len();
    pointer(&mut app, &ctx, "down", [200.0, 150.0]);
    pointer(&mut app, &ctx, "move", [220.0, 160.0]);
    assert!(crate::canvas::testing::in_gesture(&app));
    accepted(&mut app, &ctx, "ui.dialog.open", json!({"kind": "colorPicker"}));
    let dialog = crate::color_picker_ui::top(&app).expect("the Color Picker is open");
    pointer(&mut app, &ctx, "up", [999.0, 690.0]);
    assert!(!crate::canvas::testing::in_gesture(&app), "the picker's release ends the drag");
    close_dialog(&mut app, &ctx, dialog);
    pointer(&mut app, &ctx, "move", [270.0, 190.0]);
    pointer(&mut app, &ctx, "up", [270.0, 190.0]);
    for (id, previous) in layers.into_iter().zip(old) {
        assert_eq!(bounds(&app, id), previous.translate(20, 10), "moved only as far as before the picker opened");
    }
    assert_eq!(app.session.active().unwrap().history.past_len(), steps + 1);
}

// Snapping of the open box (View › Snap To, Smart Guides), driven through the canvas as window input.

/// A 400 × 300 document with a target block at (300..350, 200..260) and the moving block at (40..90, 40..80), at
/// 100 %, with Snap and Smart Guides on.
fn snap_box() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
    app.sync_views();
    app.ui.views[0].zoom = 1.0;
    app.ui.views[0].fit_pending = false;
    app.run("layer.new.layer", json!({"name": "target"})).unwrap();
    app.run("select.rect", json!({"x": 300, "y": 200, "width": 50, "height": 60})).unwrap();
    app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
    app.run("layer.new.layer", json!({"name": "mover"})).unwrap();
    app.run("select.rect", json!({"x": 40, "y": 40, "width": 50, "height": 40})).unwrap();
    app.run("edit.fill", json!({"color": "#0000ff"})).unwrap();
    app.run("select.deselect", json!({})).unwrap();
    app.ui.extras.snap = true;
    app.ui.view.show.smart_guides = true;
    app
}

fn mover_bounds(app: &PhotocraftApp) -> Rect {
    let st = app.session.active().unwrap();
    st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap().content_bounds()
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
}

fn assert_quad(app: &PhotocraftApp, expected: [[f64; 2]; 4], what: &str) {
    let q = quad(app);
    assert!(q.iter().zip(&expected).all(|(a, b)| near(*a, *b)), "{what}: {q:?} != {expected:?}");
}

/// The app of `snap_box` with guides at x 110 and y 30 and the box of `menu` open.
fn corner_guides(menu: &str) -> PhotocraftApp {
    let mut app = snap_box();
    app.run("view.newGuide", json!({"orientation": "vertical", "position": 110})).unwrap();
    app.run("view.newGuide", json!({"orientation": "horizontal", "position": 30})).unwrap();
    let app_ctx = app.ctx().clone();
    crate::menus::invoke(&mut app, &app_ctx, menu, json!({})).unwrap();
    app
}

/// Presses corner `index` off by `offset` with `down` held, then drags so the proposed corner is `end` with `moving`
/// held; the pointer keeps its grab offset.
fn move_corner(app: &mut PhotocraftApp, index: usize, offset: [f64; 2], end: [f64; 2], down: Modifiers, moving: Modifiers) {
    let corner = quad(app)[index];
    press(app, [corner[0] + offset[0], corner[1] + offset[1]], down);
    move_to(app, [end[0] + offset[0], end[1] + offset[1]], moving);
}

fn free_transform(app: &mut PhotocraftApp, reference_point: bool) {
    app.ui.tool_options.transform.show_reference_point = reference_point;
    let app_ctx = app.ctx().clone();
    crate::menus::invoke(app, &app_ctx, "edit.freeTransform", json!({})).unwrap();
}

/// Press at `from`, drag to `to`, release; returns the reference point.
fn drag(app: &mut PhotocraftApp, from: [f64; 2], to: [f64; 2], mods: Modifiers) -> [f64; 2] {
    press(app, from, mods);
    move_to(app, to, mods);
    release(app, to, mods);
    pivot(app)
}

#[test]
fn distort_corner_snaps_from_the_grab_offset_either_way() {
    for offset in [[4.0, 3.0], [-4.0, -3.0]] {
        let mut app = corner_guides("edit.transform.distort");
        let before = quad(&app);
        // The proposed corner is (108, 32); the pointer keeps its original grab offset.
        move_corner(&mut app, 1, offset, [108.0, 32.0], Modifiers::NONE, Modifiers::NONE);
        let moved = quad(&app);
        assert!(near(moved[1], [110.0, 30.0]), "grab offset {offset:?}: {moved:?}");
        assert_quad(&app, [before[0], moved[1], before[2], before[3]], "the other corners stay");
        assert_eq!(guide_lines(&app), 2, "a line per guide, grab offset {offset:?}");
        release(&mut app, [108.0 + offset[0], 32.0 + offset[1]], Modifiers::NONE);
        assert_quad(&app, moved, "the release keeps the snapped corner");
        assert_eq!(guide_lines(&app), 0, "the lines end with the gesture");
    }
}

#[test]
fn distort_corner_respects_guide_axes_tolerance_and_toggles() {
    for (snap, shown, guides, end, expected) in [
        (true, true, true, [108.0, 20.0], [110.0, 20.0]),
        (true, true, true, [100.0, 32.0], [100.0, 30.0]),
        (true, true, true, [100.0, 20.0], [100.0, 20.0]),
        (false, true, true, [108.0, 32.0], [108.0, 32.0]),
        (true, false, true, [108.0, 32.0], [108.0, 32.0]),
        (true, true, false, [108.0, 32.0], [108.0, 32.0]),
        // Layer edges are no targets for a distort corner: 2 px short of the target block's right edge stays put.
        (true, true, true, [348.0, 40.0], [348.0, 40.0]),
    ] {
        let mut app = corner_guides("edit.transform.distort");
        app.ui.extras.snap = snap;
        app.ui.extras.guides = shown;
        app.ui.view.snap_to.guides = guides;
        move_corner(&mut app, 1, [4.0, 3.0], end, Modifiers::NONE, Modifiers::NONE);
        let what = format!("Snap: {snap}, shown: {shown}, guides: {guides}, proposed: {end:?}");
        assert!(near(quad(&app)[1], expected), "{what}: {:?}", quad(&app)[1]);
        let snapped_axes = usize::from(expected[0] != end[0]) + usize::from(expected[1] != end[1]);
        assert_eq!(guide_lines(&app), snapped_axes, "{what}");
    }
}

#[test]
fn free_transform_command_corner_snaps_with_the_real_grab_offset() {
    let windows = Modifiers { command: true, ctrl: true, ..Modifiers::NONE };
    let mac = Modifiers { command: true, mac_cmd: true, ..Modifiers::NONE };
    for m in [windows, mac] {
        for offset in [[4.0, 3.0], [-4.0, -3.0], [0.0, 0.0]] {
            for down in [m, Modifiers::NONE] {
                let mut app = corner_guides("edit.freeTransform");
                let before = quad(&app);
                move_corner(&mut app, 1, offset, [108.0, 32.0], down, m);
                let what = format!("{m:?}, press {down:?}, offset {offset:?}");
                assert_quad(&app, [before[0], [110.0, 30.0], before[2], before[3]], &what);
                assert_eq!(guide_lines(&app), 2, "{what}");
            }
        }
    }
}

#[test]
fn skew_control_corner_keeps_its_axis_instead_of_snapping_as_distort() {
    let ctrl = Modifiers { command: true, ctrl: true, ..Modifiers::NONE };
    for legacy in [false, true] {
        for (menu, expected, snapping) in [
            ("edit.transform.skew", [[40.0, 40.0], [108.0, 40.0], [90.0, 80.0], [40.0, 80.0]], false),
            ("edit.transform.distort", [[40.0, 40.0], [110.0, 30.0], [90.0, 80.0], [40.0, 80.0]], true),
            ("edit.transform.perspective", [[22.0, 40.0], [108.0, 40.0], [90.0, 80.0], [40.0, 80.0]], false),
        ] {
            let mut app = corner_guides(menu);
            app.run("prefs.set", json!({"path": "general.useLegacyFreeTransform", "value": legacy})).unwrap();
            let doc = app.session.active().unwrap().doc.clone();
            let past = app.session.active().unwrap().history.past_len();
            let prefs = serde_json::to_value(app.session.prefs()).unwrap();
            // Ctrl arrives after the raw, off-centre press, as on the actual canvas path.
            move_corner(&mut app, 1, [4.0, 3.0], [108.0, 32.0], Modifiers::NONE, ctrl);
            assert_quad(&app, expected, &format!("{menu}, legacy {legacy}"));
            assert_eq!(guide_lines(&app), if snapping { 2 } else { 0 }, "{menu}");
            release(&mut app, [112.0, 35.0], ctrl);
            assert_quad(&app, expected, &format!("release preserves {menu}"));
            crate::transform_tool::cancel(&mut app);
            assert!(app.ui.transform.is_none());
            assert_eq!(guide_lines(&app), 0);
            assert!(std::sync::Arc::ptr_eq(&app.session.active().unwrap().doc, &doc), "cancel preserves {menu}'s document");
            assert_eq!(app.session.active().unwrap().history.past_len(), past, "cancel preserves {menu}'s history");
            assert_eq!(serde_json::to_value(app.session.prefs()).unwrap(), prefs, "{menu} does not change preferences");
        }
    }
}

#[test]
fn skew_control_corner_on_editable_type_slides_the_whole_edge() {
    let ctrl = Modifiers { command: true, ctrl: true, ..Modifiers::NONE };
    for legacy in [false, true] {
        let mut app = snap_box();
        app.run("type.create", json!({"x": 40, "y": 60, "text": "Hello"})).unwrap();
        app.run("prefs.set", json!({"path": "general.useLegacyFreeTransform", "value": legacy})).unwrap();
        let app_ctx = app.ctx().clone();
        crate::menus::invoke(&mut app, &app_ctx, "edit.transform.skew", json!({})).unwrap();
        let before = quad(&app);
        let end = [before[1][0] + 18.0, before[1][1] - 8.0];
        let doc = app.session.active().unwrap().doc.clone();
        let past = app.session.active().unwrap().history.past_len();
        move_corner(&mut app, 1, [2.0, 1.0], end, Modifiers::NONE, ctrl);
        let mut expected = before;
        expected[0][0] += 18.0;
        expected[1][0] += 18.0;
        assert_quad(&app, expected, &format!("type remains affine, legacy {legacy}"));
        assert_eq!(guide_lines(&app), 0);
        release(&mut app, [end[0] + 2.0, end[1] + 1.0], ctrl);
        assert_quad(&app, expected, "release preserves the type's edge");
        crate::transform_tool::cancel(&mut app);
        assert!(std::sync::Arc::ptr_eq(&app.session.active().unwrap().doc, &doc));
        assert_eq!(app.session.active().unwrap().history.past_len(), past);
    }
}

#[test]
fn a_foldover_constraint_never_claims_a_guide_alignment() {
    let mut app = corner_guides("edit.transform.distort");
    app.run("view.newGuide", json!({"orientation": "horizontal", "position": 100})).unwrap();
    let before = quad(&app);
    move_corner(&mut app, 0, [4.0, 3.0], [108.0, 98.0], Modifiers::NONE, Modifiers::NONE);
    let q = quad(&app);
    assert!(!near(q[0], [110.0, 100.0]), "the guide intersection would fold the quad");
    assert_quad(&app, [q[0], before[1], before[2], before[3]], "the other corners stay");
    for i in 0..4 {
        let (a, b, c) = (q[i], q[(i + 1) % 4], q[(i + 2) % 4]);
        assert!((b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]) > 0.0, "still convex: {q:?}");
    }
    assert_eq!(guide_lines(&app), 0, "a folded corner reports no guide line");
}

#[test]
fn a_snapped_distort_commits_one_undoable_transform() {
    let mut app = corner_guides("edit.transform.distort");
    let before_doc = app.session.active().unwrap().doc.clone();
    let before_bounds = mover_bounds(&app);
    let past = app.session.active().unwrap().history.past_len();
    move_corner(&mut app, 1, [4.0, 3.0], [108.0, 32.0], Modifiers::NONE, Modifiers::NONE);
    assert!(std::sync::Arc::ptr_eq(&app.session.active().unwrap().doc, &before_doc), "preview does not edit the document");
    release(&mut app, [112.0, 35.0], Modifiers::NONE);
    crate::transform_tool::commit(&mut app);
    assert_eq!(app.ui.status, "");
    let after_bounds = mover_bounds(&app);
    assert_ne!(after_bounds, before_bounds);
    assert_eq!(app.session.active().unwrap().history.past_len(), past + 1);
    app.run("edit.undo", json!({})).unwrap();
    assert_eq!(mover_bounds(&app), before_bounds);
    app.run("edit.redo", json!({})).unwrap();
    assert_eq!(mover_bounds(&app), after_bounds);
}

#[test]
fn free_transform_drags_and_handles_snap() {
    let mut app = snap_box();
    let app_ctx = app.ctx().clone();
    crate::menus::invoke(&mut app, &app_ctx, "edit.freeTransform", json!({})).unwrap();
    assert!(app.ui.transform.is_some());
    let m = Modifiers::NONE;
    // Drag from inside the box (40..90 × 40..80) so its left edge lands 2 px from x = 300.
    // (Away from the centre, which holds the reference point.)
    press(&mut app, [50.0, 50.0], m);
    move_to(&mut app, [312.0, 50.0], m);
    release(&mut app, [312.0, 50.0], m);
    let q = quad(&app);
    assert!((q[0][0] - 300.0).abs() < 1e-3, "{q:?}");
    // The right-edge handle snaps to the target's right edge (x = 350).
    press(&mut app, [q[1][0], (q[1][1] + q[2][1]) / 2.0], m);
    move_to(&mut app, [353.0, 60.0], m);
    release(&mut app, [353.0, 60.0], m);
    let q = quad(&app);
    assert!((q[1][0] - 350.0).abs() < 1e-3, "{q:?}");
    crate::transform_tool::commit(&mut app);
    assert_eq!(mover_bounds(&app).x0, 300);
}

/// #5751: a Rectangle drawn on a 1080 px canvas, then resized with Free Transform under the
/// Move tool. With Snap Vector Tools and Transforms to Pixel Grid on (the default) the box's
/// edges stay on whole pixels: the grab offset and a proportional scale used to leave them at
/// 20.6 and 1040.4. Off, the box follows the pointer exactly.
#[test]
fn free_transform_of_a_shape_keeps_whole_pixels() {
    for grid in [true, false] {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 1080, "height": 1080, "resolution": 72})).unwrap();
        app.run("prefs.set", json!({"path": "tools.snapVectorToolsAndTransformsToPixelGrid", "value": grid})).unwrap();
        app.sync_views();
        app.ui.views[0].zoom = 0.6;
        app.ui.views[0].fit_pending = false;
        app.ui.extras.snap = true;
        app.ui.view.show.smart_guides = true;
        app.ui.tool = Tool::Rectangle;
        let m = Modifiers::NONE;
        press(&mut app, [40.0, 31.0], m);
        move_to(&mut app, [300.0, 700.0], m);
        release(&mut app, [481.0, 1000.0], m);
        app.ui.tool = Tool::Move;
        let app_ctx = app.ctx().clone();
        crate::menus::invoke(&mut app, &app_ctx, "edit.freeTransform", json!({})).unwrap();
        let whole = |q: [[f64; 2]; 4]| q.iter().flatten().all(|v| v.fract() == 0.0);
        // The left edge handle, grabbed 1.3 px inside, dropped away from any snap target.
        let q = quad(&app);
        let mid_y = (q[0][1] + q[3][1]) / 2.0;
        press(&mut app, [q[0][0] + 1.3, mid_y], m);
        move_to(&mut app, [60.0, mid_y], m);
        release(&mut app, [80.6, mid_y], m);
        let q = quad(&app);
        assert_eq!(whole(q), grid, "grid {grid}: left edge {q:?}");
        // The bottom-right corner scaled proportionally (the default), off any snap target.
        press(&mut app, q[2], m);
        move_to(&mut app, [450.4, 900.1], m);
        release(&mut app, [450.4, 900.1], m);
        let q = quad(&app);
        assert_eq!(whole(q), grid, "grid {grid}: corner {q:?}");
        crate::transform_tool::commit(&mut app);
        let st = app.session.active().unwrap();
        let photocraft_doc::LayerContent::Shape(sh) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!("a shape layer") };
        let (x0, y0, x1, y1) = sh.path.control_bounds().unwrap();
        if grid {
            for v in [x0, y0, x1, y1] {
                assert!((v - v.round()).abs() < 1e-6, "committed shape {:?}", (x0, y0, x1, y1));
            }
        }
    }
}

/// #5153: a reference point drag (or an Alt-drag inside the box) moves the point alone. It used to
/// snap as a box move: the point stuck and snap lines showed at edges the box never reached.
#[test]
fn free_transform_reference_point_drag_snaps_the_point_not_the_box() {
    let mut app = snap_box();
    free_transform(&mut app, true);
    let quad0 = quad(&app);
    assert_eq!(pivot(&app), [65.0, 60.0]);
    let alt = Modifiers::ALT;
    // Moving the box's right edge by the same amount would land it 2 px from the target's left
    // edge (x = 300), but the point alone moves, so nothing pulls it there.
    press(&mut app, [65.0, 60.0], alt);
    move_to(&mut app, [273.0, 60.0], alt);
    assert_eq!(guide_lines(&app), 0);
    release(&mut app, [273.0, 60.0], alt);
    assert!(near(pivot(&app), [273.0, 60.0]), "{:?}", pivot(&app));
    assert_quad(&app, quad0, "the box stays put");
    // Alt-dragging from inside the box places the reference point the same way.
    assert!(near(drag(&mut app, [50.0, 50.0], [258.0, 50.0], alt), [258.0, 50.0]));
    // A plain drag of the reference point moves it too.
    assert!(near(drag(&mut app, [258.0, 50.0], [80.0, 63.0], Modifiers::NONE), [80.0, 63.0]));
    assert_quad(&app, quad0, "the box stays put");
}

/// A box dragged from inside doesn't snap back to the selection it transforms (measured in the
/// reference application: a 6 px drag moves it 6 px).
#[test]
fn free_transform_of_a_selection_never_snaps_back_to_it() {
    let mut app = snap_box();
    app.run("select.rect", json!({"x": 40, "y": 40, "width": 50, "height": 40})).unwrap();
    free_transform(&mut app, false);
    let quad0 = quad(&app);
    let m = Modifiers::NONE;
    press(&mut app, [50.0, 50.0], m);
    move_to(&mut app, [53.0, 50.0], m);
    assert_eq!(guide_lines(&app), 0);
    release(&mut app, [53.0, 50.0], m);
    let q = quad(&app);
    assert!((q[0][0] - (quad0[0][0] + 3.0)).abs() < 1e-3, "{q:?}");
}

/// The reference point snaps to the turned box's nine points, and only to them: not to guides,
/// other layers or the canvas, and with View > Snap off or Ctrl held too, as in the reference
/// application. With the point hidden an Alt-drag still places it and it snaps the same way.
#[test]
fn reference_point_snaps_to_the_turned_box_points_only() {
    for shown in [true, false] {
        let mut app = snap_box();
        app.run("view.newGuide", json!({"orientation": "vertical", "position": 150.0})).unwrap();
        free_transform(&mut app, shown);
        // Turn the box 30 degrees about its centre: its points leave the lines point snapping knows.
        app.ui.transform.as_mut().unwrap().edit_box(|b| *b = b.turned_about_pivot(30f64.to_radians()));
        let (q, c) = (quad(&app), pivot(&app));
        let m = if shown { Modifiers::NONE } else { Modifiers::ALT };
        // To 2 px off the turned top-right corner: lands on it.
        let p = drag(&mut app, c, [q[1][0] - 2.0, q[1][1] + 1.0], m);
        assert!(near(p, q[1]), "shown {shown}: {p:?} vs {:?}", q[1]);
        if !shown {
            assert_quad(&app, q, "the box never moved");
            continue;
        }
        // On the corner, a press grabs the point ahead of the corner handle, and it lands on the
        // bottom edge's midpoint.
        let mid = [(q[2][0] + q[3][0]) / 2.0, (q[2][1] + q[3][1]) / 2.0];
        let p = drag(&mut app, [q[1][0] + 1.0, q[1][1]], [mid[0] + 1.5, mid[1] - 1.5], m);
        assert!(near(p, mid), "{p:?} vs {mid:?}");
        assert_quad(&app, q, "the box never moved");
        // Ctrl held during the drag doesn't turn the pull off.
        let ctrl = Modifiers { ctrl: true, command: true, ..Modifiers::NONE };
        press(&mut app, p, m);
        move_to(&mut app, [c[0] + 2.0, c[1]], ctrl);
        release(&mut app, [c[0] + 2.0, c[1]], ctrl);
        assert!(near(pivot(&app), c));
        // Nor View > Snap off.
        app.ui.extras.snap = false;
        assert!(near(drag(&mut app, c, [q[1][0] - 2.0, q[1][1] + 1.0], m), q[1]));
        app.ui.extras.snap = true;
        // 2 px off the guide (x = 150) and the target layer's left edge (x = 300): stays put.
        let p = drag(&mut app, q[1], [148.0, 120.0], m);
        assert!(near(p, [148.0, 120.0]), "{p:?}");
        let p = drag(&mut app, p, [298.0, 150.0], m);
        assert!(near(p, [298.0, 150.0]), "{p:?}");
        assert_eq!(guide_lines(&app), 0);
        assert_quad(&app, q, "the box never moved");
    }
}
