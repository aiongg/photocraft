use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use photocraft_color::convert::{hsb_to_rgb, rgb_to_hsb};

use super::*;

fn app_harness(app: PhotocraftApp) -> Harness<'static, PhotocraftApp> {
    let h = Harness::builder().with_size(vec2(760.0, 480.0)).build_ui_state(|ui, app| crate::dialogs::show(app, ui.ctx()), app);
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h
}

fn tools_hex(app: &PhotocraftApp, target: &str) -> String {
    let c = if target == "background" { app.session.tools.background } else { app.session.tools.foreground };
    hex([c[0], c[1], c[2]])
}

#[test]
fn hex_field_has_focus_in_tab_order_and_enter_confirms_the_typed_color() {
    let mut h = app_harness(PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default()));
    open(h.state_mut(), "foreground");
    h.run_steps(3);
    assert!(h.get_by_role(Role::TextInput).is_focused(), "the hex field has focus on open");
    for ch in "ff8800".chars() {
        h.event(egui::Event::Text(ch.to_string()));
        h.run_steps(1);
    }
    h.key_press(egui::Key::Tab);
    h.run_steps(2);
    assert!(!h.get_by_role(Role::TextInput).is_focused(), "Tab moves on from the hex field");
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_role(Role::TextInput).is_focused(), "Shift+Tab returns to the hex field");
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(tools_hex(h.state(), "foreground"), "#ff8800");
}

#[test]
fn side_buttons_and_keyboard_confirm_or_cancel_without_changing_the_other_color() {
    for target in ["foreground", "background"] {
        for action in ["OK", "Cancel", "Enter", "Escape"] {
            let mut h = app_harness(PhotocraftApp::new(photocraft_engine::Session::new(), Default::default()));
            let (fg, bg) = (h.state().session.tools.foreground, h.state().session.tools.background);
            open(h.state_mut(), target);
            h.run_steps(3);
            for ch in "336699".chars() {
                h.event(egui::Event::Text(ch.to_string()));
                h.run_steps(1);
            }
            match action {
                "Enter" => h.key_press(egui::Key::Enter),
                "Escape" => h.key_press(egui::Key::Escape),
                _ => h.get_by_label(action).click(),
            }
            h.run_steps(3);
            assert!(!h.state().ui.any_dialog_open(), "{target}: {action} closes the picker");
            let confirm = matches!(action, "OK" | "Enter");
            let tools = &h.state().session.tools;
            if confirm && target == "foreground" {
                assert_eq!(tools_hex(h.state(), "foreground"), "#336699");
            } else {
                assert_eq!(tools.foreground, fg);
            }
            if confirm && target == "background" {
                assert_eq!(tools_hex(h.state(), "background"), "#336699");
            } else {
                assert_eq!(tools.background, bg);
            }
        }
    }
}

#[test]
fn add_to_swatches_keeps_the_picker_open() {
    let mut h = app_harness(PhotocraftApp::new(photocraft_engine::Session::new(), Default::default()));
    let before = h.state().session.presets.swatches[0].items.len();
    open(h.state_mut(), "foreground");
    h.run_steps(3);
    h.get_by_label("Add to Swatches").click();
    h.run_steps(3);
    assert_eq!(h.state().session.presets.swatches[0].items.len(), before + 1);
    assert!(top(h.state()).is_some(), "the picker stays open");
}

/// #4196: the CMYK inks a colour was picked as stay its inks through OK, so a CMYK document is
/// filled with them.
#[test]
fn cmyk_values_survive_ok_and_a_reopen() {
    let mut h = app_harness(PhotocraftApp::new(photocraft_engine::Session::new(), Default::default()));
    let cyan = [1.0, 0.0, 0.0, 0.0];
    let rgb = photocraft_color::convert::cmyk_to_rgb(cyan).map(|c| c.clamp(0.0, 1.0));
    h.state_mut().run("tools.setColors", json!({"foreground": hex(rgb), "foregroundCmyk": cyan})).unwrap();
    open(h.state_mut(), "foreground");
    h.run_steps(3);
    h.get_by_label("OK").click();
    h.run_steps(3);
    let fg = h.state().session.tools.foreground;
    assert_eq!(h.state().session.tools.picked_cmyk([fg[0], fg[1], fg[2]]), Some(cyan));
}

#[test]
fn the_top_picker_samples_the_image() {
    let mut h = app_harness(PhotocraftApp::new(photocraft_engine::Session::new(), Default::default()));
    let app = h.state_mut();
    app.run("file.new", json!({"width": 40, "height": 20, "background": "transparent"})).unwrap();
    app.run("shape.create", json!({"kind": "rect", "rect": [0, 0, 20, 20], "fill": "#ff0000"})).unwrap();
    app.run("shape.create", json!({"kind": "rect", "rect": [20, 0, 10, 20], "fill": "#00ff00"})).unwrap();
    let id = open(app, "foreground");
    h.run_steps(3);
    assert_eq!(top(h.state()), Some(id));
    let ctx = h.ctx.clone();
    let shown = |h: &Harness<'static, PhotocraftApp>| h.state().ui.dialog_fields(id).unwrap()["color"].as_str().unwrap().to_owned();
    sample_at(h.state_mut(), &ctx, 25.5, 5.5);
    h.run_steps(2);
    assert_eq!(shown(&h), "#00ff00");
    assert_eq!(h.state().ui.dialog_fields(id).unwrap()["__orig"], json!("#000000"), "the current colour stays");
    // Off the image or over transparency nothing changes.
    for (x, y) in [(35.0, 5.0), (-3.0, 5.0), (5.0, 20.0), (1e12, -1e12), (f64::NAN, f64::INFINITY)] {
        sample_at(h.state_mut(), &ctx, x, y);
        h.run_steps(2);
        assert_eq!(shown(&h), "#00ff00", "({x}, {y})");
    }
    // Agents: `ui.pointer` samples into the picker instead of driving the tool.
    h.state_mut().ui.tool = crate::state::Tool::Brush;
    let rev = h.state().session.active().unwrap().revision;
    let events = json!([{"kind": "down", "x": 5, "y": 5}, {"kind": "up", "x": 5, "y": 5}]);
    let (req, _rx) = crate::control::ControlRequest::new("ui.pointer", json!({"events": events}));
    let _ = crate::control::handle(h.state_mut(), &ctx, &req);
    h.run_steps(2);
    assert_eq!(shown(&h), "#ff0000");
    assert_eq!(h.state().session.active().unwrap().revision, rev, "the Brush must not paint");
    assert_eq!(h.state().session.tools.foreground, [0.0, 0.0, 0.0, 1.0], "only OK sets the foreground");
}

#[test]
fn gamut_and_web_warnings() {
    // Photoshop 25.4: #714343 is printable but not a web colour; #fd0303 is neither.
    let rgb = |h| parse_hex(h).unwrap();
    assert_eq!(out_of_gamut(rgb("#714343")), None);
    assert!(out_of_gamut(rgb("#fd0303")).is_some());
    assert_eq!(out_of_gamut(rgb("#000000")), None, "black prints");
    assert_eq!(out_of_gamut(rgb("#ffffff")), None);
    assert_eq!(not_web_safe(rgb("#714343")).map(hex).as_deref(), Some("#663333"));
    assert_eq!(not_web_safe(rgb("#cc3366")), None);
}

/// Hex parsing refuses text whose bytes split a character instead of panicking.
#[test]
fn parse_hex_rejects_multibyte_text() {
    // Six bytes, three characters: the byte range 0..2 splits "é", which used to panic.
    assert_eq!(parse_hex("aé€"), None);
    assert_eq!(parse_hex("#ff880"), None);
}

#[test]
fn parse_hex_rejects_signs() {
    assert_eq!(parse_hex("#+f+f+f"), None);
    assert_eq!(parse_hex("+1+2+3"), None);
    assert!(parse_hex("#0f0f0f").is_some());
}

#[test]
fn hsv_round_trips() {
    for c in [[1.0, 0.0, 0.0], [0.2, 0.6, 0.4], [0.5, 0.5, 0.5], [0.0, 0.0, 1.0], [0.9, 0.8, 0.1]] {
        let h = rgb_to_hsb(c);
        let back = hsb_to_rgb(h);
        for i in 0..3 {
            assert!((back[i] - c[i]).abs() < 1e-5, "{c:?} -> {h:?} -> {back:?}");
        }
    }
    assert_eq!(hex([1.0, 0.5, 0.0]), "#ff8000");
    assert_eq!(parse_hex("#FF8000").map(hex).as_deref(), Some("#ff8000"));
}

/// Turns Only Web Colors on for the pickers that follow: the checkbox, then OK stores the setting.
fn remember_web_only(h: &mut Harness<'static, PhotocraftApp>) {
    open(h.state_mut(), "foreground");
    h.run_steps(3);
    h.get_by_label("Only Web Colors").click();
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
}

/// Only Web Colors is remembered from OK, and every picker target opens on the nearest web colour
/// with the compare swatch following, so OK with no edit changes nothing.
#[test]
fn only_web_colors_survives_a_reopen_and_other_picker_targets() {
    let mut h = app_harness(PhotocraftApp::new(photocraft_engine::Session::new(), Default::default()));
    h.state_mut().run("tools.setColors", json!({"foreground": "#123456"})).unwrap();
    remember_web_only(&mut h);
    assert_eq!(tools_hex(h.state(), "foreground"), "#003366", "OK stored the snapped colour");
    h.state_mut().run("tools.setColors", json!({"foreground": "#123456"})).unwrap();
    let id = open(h.state_mut(), "foreground");
    h.run_steps(3);
    let f = h.state().ui.dialog_fields(id).unwrap();
    assert_eq!((f["color"].as_str(), f["__orig"].as_str()), (Some("#003366"), Some("#003366")));
    let id = open_for_target(h.state_mut(), "noteColor", "Color Picker", [1.0, 0.53, 0.0]);
    h.run_steps(3);
    let f = h.state().ui.dialog_fields(id).unwrap();
    assert_eq!(
        (f["color"].as_str(), f["__orig"].as_str()),
        (Some("#ff9900"), Some("#ff9900")),
        "a start colour that is not a web colour is not committed by opening"
    );
}

/// A canvas sample into the picker is snapped while Only Web Colors is on.
#[test]
fn a_sample_snaps_when_only_web_colors_is_on() {
    let mut h = app_harness(PhotocraftApp::new(photocraft_engine::Session::new(), Default::default()));
    h.state_mut().run("file.new", json!({"width": 20, "height": 20, "background": "transparent"})).unwrap();
    h.state_mut().run("shape.create", json!({"kind": "rect", "rect": [0, 0, 20, 20], "fill": "#123456"})).unwrap();
    remember_web_only(&mut h);
    let id = open(h.state_mut(), "foreground");
    h.run_steps(3);
    let ctx = h.ctx.clone();
    sample_at(h.state_mut(), &ctx, 5.5, 5.5);
    h.run_steps(2);
    assert_eq!(h.state().ui.dialog_fields(id).unwrap()["color"], json!("#003366"));
}

/// Typing a multi-digit CMYK value into a field gives PhotoCraft's profile conversion of the
/// colour with that ink changed. (The Lab and HSB rows are the library's: `typed_lab_and_host_model_values_stay_while_they_give_the_same_colour`.)
#[test]
fn a_cmyk_field_takes_multi_digit_values() {
    let mut h = app_harness(PhotocraftApp::new(photocraft_engine::Session::new(), Default::default()));
    h.state_mut().run("tools.setColors", json!({"foreground": "#336699"})).unwrap();
    let id = open(h.state_mut(), "foreground");
    h.run_steps(3);
    let base = parse_hex("#336699").unwrap();
    let cmyk = photocraft_color::convert::rgb_to_cmyk(base);
    h.get_by_role_and_label(Role::SpinButton, "Cyan").click();
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text("40".into()));
    h.run_steps(1);
    h.key_press(egui::Key::Tab);
    h.run_steps(2);
    let want = photocraft_color::convert::cmyk_to_rgb([0.4, cmyk[1], cmyk[2], cmyk[3]]);
    assert_eq!(h.state().ui.dialog_fields(id).unwrap()["color"], json!(hex(want)));
}
