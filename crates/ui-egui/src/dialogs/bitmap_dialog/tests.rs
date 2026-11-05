use craft::ui::dialog::ParamDialog;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Map, Value, json};

use super::*;

type H = Harness<'static, PhotocraftApp>;

/// The app with a grayscale document of `dpi` and `depth`, and the dialogs drawn each frame.
fn harness(dpi: f64, depth: u64) -> H {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": 40, "height": 20, "mode": "grayscale", "depth": depth, "resolution": dpi})).unwrap();
    app.sync_views();
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            let ctx = ui.ctx().clone();
            if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            crate::dialogs::show(app, &ctx);
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h.run_steps(4);
    h
}

fn open(h: &mut H) -> u64 {
    let id = crate::filter_dialog::open(h.state_mut(), COMMAND).unwrap();
    h.run_steps(3);
    id
}

fn fields(h: &H, id: u64) -> Map<String, Value> {
    h.state().ui.dialog_fields(id).unwrap()
}

fn set(h: &mut H, id: u64, key: &str, value: Value) {
    crate::dialogs::hosted::set_field(h.state_mut(), id, key, value).unwrap();
    h.run_steps(3);
}

fn confirm(h: &mut H, id: u64) {
    let ctx = h.ctx.clone();
    crate::dialogs::confirm_in(h.state_mut(), &ctx, id).unwrap();
}

#[test]
fn opens_with_output_at_the_input_resolution_and_a_pattern_id() {
    let mut h = harness(300.0, 16);
    let id = open(&mut h);
    let f = fields(&h, id);
    assert!(h.state().ui.dialogs.is_empty() && h.state().ui.host.is_open::<ParamDialog>(), "the dialog is hosted");
    assert_eq!(f["__command"], COMMAND);
    assert_eq!(f["__inputResolution"].as_f64(), Some(300.0));
    assert_eq!(f["resolution"].as_f64(), Some(300.0));
    assert!(f["pattern"].is_string(), "the schema's placeholder 0 is replaced by a pattern id: {:?}", f["pattern"]);
    let p = crate::filter_dialog::params_of(&f);
    assert!(p.as_object().unwrap().keys().all(|k| !k.starts_with("__")), "{p}");
}

/// Photoshop's Halftone Screen settings show only for the Halftone Screen method. The hosted rows
/// cannot be hidden (library gap: no row visibility), so they are shown disabled instead.
#[test]
#[ignore = "gap Y: ParamDialog has no row visibility, so the Halftone Screen rows are disabled, not hidden"]
fn halftone_settings_show_only_for_halftone_screen() {
    let mut h = harness(72.0, 8);
    let id = open(&mut h);
    for (method, halftone) in [("diffusion", false), ("threshold", false), ("pattern", false), ("customPattern", false), ("halftone", true)] {
        set(&mut h, id, "method", json!(method));
        for label in ["Frequency:", "Angle:", "Shape:"] {
            assert_eq!(h.query_by_label(label).is_some(), halftone, "{method}: {label}");
        }
        for label in ["Input:", "Output:", "Use:", "Custom Pattern:"] {
            assert!(h.query_by_label(label).is_some(), "{method}: {label}");
        }
    }
}

#[test]
fn halftone_settings_are_enabled_only_for_halftone_screen_and_the_pattern_only_for_custom_pattern() {
    let mut h = harness(72.0, 8);
    let id = open(&mut h);
    for (method, halftone, pattern) in [("diffusion", false, false), ("threshold", false, false), ("customPattern", false, true), ("halftone", true, false)] {
        set(&mut h, id, "method", json!(method));
        let disabled = |h: &H, label: &str| {
            let nodes: Vec<_> = h.query_all_by_label_contains(label).collect();
            assert!(!nodes.is_empty(), "{method}: no node named {label}");
            nodes.iter().all(|n| n.accesskit_node().is_disabled())
        };
        assert_eq!(!disabled(&h, "Angle"), halftone, "{method}: Angle");
        assert_eq!(!disabled(&h, "Frequency"), halftone, "{method}: Frequency");
        assert_eq!(!disabled(&h, "Custom Pattern"), pattern, "{method}: pattern picker");
        assert!(!disabled(&h, "Output") && !disabled(&h, "Use"), "{method}");
    }
}

#[test]
fn pixels_per_centimeter_show_the_same_resolution() {
    let mut h = harness(254.0, 8);
    let id = open(&mut h);
    h.state_mut().ui.hosted_fields_mut(id).unwrap().set(RES_UNIT, "ppcm");
    h.run_steps(3);
    assert!(h.query_by_label_contains("100 Pixels/Centimeter").is_some(), "Input 254 ppi is 100 px/cm");
    assert_eq!(fields(&h, id)["resolution"].as_f64(), Some(254.0), "the param stays in pixels/inch");
}

#[test]
fn ok_resamples_to_the_output_resolution() {
    for depth in [8, 16] {
        let mut h = harness(72.0, depth);
        let id = open(&mut h);
        set(&mut h, id, "method", json!("threshold"));
        set(&mut h, id, "resolution", json!(144));
        confirm(&mut h, id);
        let doc = &h.state().session.active().unwrap().doc;
        assert_eq!((doc.size.width, doc.size.height, doc.resolution_dpi, doc.mode), (80, 40, 144.0, photocraft_color::ColorMode::Bitmap), "{depth}");
    }
}

#[test]
fn ok_with_custom_pattern_and_halftone_screen_converts() {
    for method in ["customPattern", "halftone"] {
        let mut h = harness(72.0, 8);
        let id = open(&mut h);
        set(&mut h, id, "method", json!(method));
        confirm(&mut h, id);
        let doc = &h.state().session.active().unwrap().doc;
        assert_eq!((doc.size.width, doc.mode), (40, photocraft_color::ColorMode::Bitmap), "{method}");
    }
}

#[test]
fn ok_remembers_the_method_and_alt_reset_restores_the_values_it_opened_with() {
    let mut h = harness(72.0, 8);
    let id = open(&mut h);
    set(&mut h, id, "method", json!("halftone"));
    confirm(&mut h, id);
    assert_eq!(h.state().session.prefs().dialogs[COMMAND]["method"], "halftone");
    let id = open(&mut h);
    assert_eq!(fields(&h, id)["method"], "halftone", "the dialog opens on the remembered method");
    set(&mut h, id, "method", json!("threshold"));
    h.get_by_label("Cancel").click_modifiers(egui::Modifiers::ALT);
    h.run_steps(3);
    assert_eq!(fields(&h, id)["method"], "halftone", "Reset keeps the dialog open on the opening values");
}

#[test]
fn the_preview_keeps_the_canvas_size() {
    let h = harness(72.0, 8);
    let st = h.state().session.active().unwrap();
    let p = json!({"method": "threshold", "resolution": 144});
    let shown = crate::filter_dialog::preview_document(&st.doc, st.active_layer, COMMAND, &p, 1).unwrap();
    assert_eq!((shown.size.width, shown.size.height), (40, 20));
    assert_eq!(shown.mode, photocraft_color::ColorMode::Bitmap);
}
