use super::*;
use craft::ui::dialog::{CanvasSizeDialog, ImageSizeDialog};
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn app_with(width: u32, height: u32, resolution: u32) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.session.execute("file.new", json!({"width": width, "height": height, "resolution": resolution})).unwrap();
    app
}

fn harness(app: PhotocraftApp) -> Harness<'static, PhotocraftApp> {
    let h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_ui_state(|ui, app: &mut PhotocraftApp| crate::dialogs::show(app, ui.ctx()), app);
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h
}

fn num(f: &Map<String, Value>, k: &str) -> f64 {
    f.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}

/// The JSON fields of dialog `id`, as `ui.inspect` and `ui.dialog.confirm` read them.
fn fields(app: &PhotocraftApp, id: u64) -> Map<String, Value> {
    app.ui.dialog_fields(id).expect("the dialog is open")
}

fn set(app: &mut PhotocraftApp, id: u64, key: &str, value: Value) {
    crate::dialogs::hosted::set_field(app, id, key, value).unwrap();
}

#[test]
#[ignore = "library row 162: sizing dialogs have no setters or getters"]
fn dialog_maps_to_engine_command() {
    let mut app = app_with(200, 100, 72);
    let id = open(&mut app, "image.imageSize").unwrap();
    assert!(app.ui.hosted(id).is_some() && app.ui.dialogs.is_empty(), "the dialog is on the host");
    set(&mut app, id, "width", json!(100));
    set(&mut app, id, "height", json!(50));
    crate::dialogs::confirm(&mut app, id).unwrap();
    assert_eq!(app.session.active().unwrap().doc.size, photocraft_doc::Size::new(100, 50));
    assert!(!app.ui.host.is_open::<ImageSizeDialog>() && app.ui.hosted(id).is_none());
    let id = open(&mut app, "image.canvasSize").unwrap();
    set(&mut app, id, "width", json!(120));
    set(&mut app, id, "anchor", json!("topLeft"));
    crate::dialogs::confirm(&mut app, id).unwrap();
    assert_eq!(app.session.active().unwrap().doc.size, photocraft_doc::Size::new(120, 50));
}

#[test]
fn confirming_unchanged_dialogs_keeps_the_document() {
    let mut app = app_with(200, 100, 72);
    for command in ["image.imageSize", "image.canvasSize"] {
        let id = open(&mut app, command).unwrap();
        let f = fields(&app, id);
        assert_eq!((num(&f, "width"), num(&f, "height")), (200.0, 100.0), "{command}: {f:?}");
        crate::dialogs::confirm(&mut app, id).unwrap();
        assert_eq!(app.session.active().unwrap().doc.size, photocraft_doc::Size::new(200, 100), "{command}");
    }
}

#[test]
#[ignore = "library row 162: sizing dialogs have no setters or getters"]
fn cancel_closes_the_dialog_and_runs_nothing() {
    let mut app = app_with(200, 100, 72);
    let id = open(&mut app, "image.canvasSize").unwrap();
    set(&mut app, id, "width", json!(300));
    crate::dialogs::cancel(&mut app, id).unwrap();
    assert!(app.ui.hosted(id).is_none() && !app.ui.host.is_open::<CanvasSizeDialog>());
    assert_eq!(app.session.active().unwrap().doc.size.width, 200);
}

#[test]
fn opening_again_replaces_the_open_dialog() {
    let mut app = app_with(200, 100, 72);
    let first = open(&mut app, "image.imageSize").unwrap();
    let second = open(&mut app, "image.imageSize").unwrap();
    assert!(app.ui.hosted(first).is_none() && app.ui.hosted(second).is_some());
}

/// #5756: points and picas follow Preferences › Units & Rulers › Point/Pica Size (Traditional is
/// 72.27 pt/in).
#[test]
fn traditional_point_size_drives_points_and_picas() {
    let mut app = app_with(100, 50, 72);
    app.session.execute("prefs.set", json!({"path": "unitsAndRulers.pointSize", "value": "traditional"})).unwrap();
    let id = open(&mut app, "image.imageSize").unwrap();
    assert_eq!(app.ui.host.get::<ImageSizeDialog>().unwrap().fields().points_per_inch, 72.27);
    app.ui.close_dialog(id);
    open(&mut app, "image.canvasSize").unwrap();
    assert_eq!(app.ui.host.get::<CanvasSizeDialog>().unwrap().fields().points_per_inch, 72.27);
}

#[test]
fn canvas_extension_dropdown_is_named_for_its_setting() {
    use egui_kittest::kittest::NodeT;
    let mut h = harness(app_with(100, 50, 72));
    open(h.state_mut(), "image.canvasSize").unwrap();
    h.run_steps(3);
    let labels: Vec<String> = h.query_all_by_role(Role::ComboBox).filter_map(|n| n.accesskit_node().label()).collect();
    assert!(labels.iter().any(|label| label.trim_end_matches(':') == "Canvas extension color"), "{labels:?}");
    assert!(!labels.iter().any(|label| label == "Size"), "the dialog's Size label names a different field: {labels:?}");
}

fn indexed_app() -> PhotocraftApp {
    let mut app = app_with(200, 100, 72);
    app.session.execute("image.mode.indexedColor", json!({})).unwrap();
    app
}

/// #6019: on an Indexed Color document the dialog opens on Automatic or Nearest Neighbor.
#[test]
fn indexed_color_opens_on_nearest_neighbor() {
    let mut app = indexed_app();
    for (pref, method) in [("bicubic", "nearest"), ("bicubicAutomatic", "automatic"), ("nearestNeighbor", "nearest")] {
        app.session.execute("prefs.set", json!({"path": "general.imageInterpolation", "value": pref})).unwrap();
        let id = open(&mut app, "image.imageSize").unwrap();
        assert_eq!(fields(&app, id).get("resample"), Some(&json!(method)), "{pref}");
        crate::dialogs::cancel(&mut app, id).unwrap();
    }
}

/// #6019: the open list offers only Automatic and Nearest Neighbor on an Indexed Color document.
#[test]
#[ignore = "gap Z4: ImageSizeDialog offers every Resample method; it has no document mode to filter by"]
fn indexed_color_offers_nearest_neighbor_and_automatic_only() {
    let mut app = indexed_app();
    app.session.execute("prefs.set", json!({"path": "general.imageInterpolation", "value": "nearestNeighbor"})).unwrap();
    let mut h = harness(app);
    open(h.state_mut(), "image.imageSize").unwrap();
    h.run_steps(3);
    let combo = h.query_all_by_role(Role::ComboBox).find(|n| n.value().as_deref() == Some("Nearest Neighbor (hard edges)"));
    combo.expect("the Resample list shows Nearest Neighbor").click();
    h.run_steps(3);
    assert!(h.query_by_label_contains("Automatic").is_some(), "Automatic is offered");
    for gone in ["Bicubic", "Bilinear", "Lanczos", "Preserve Details"] {
        assert!(h.query_by_label_contains(gone).is_none(), "{gone} is not offered for Indexed Color");
    }
}

/// The dialog starts on Preferences › General › Image Interpolation: Bicubic Automatic opens
/// as Automatic and the others as themselves.
#[test]
fn resample_follows_the_interpolation_preference() {
    let mut app = app_with(200, 100, 72);
    for (pref, method) in
        [("bicubicAutomatic", "automatic"), ("bicubicSharper", "bicubicSharper"), ("bicubicSmoother", "bicubicSmoother"), ("nearestNeighbor", "nearest")]
    {
        app.session.execute("prefs.set", json!({"path": "general.imageInterpolation", "value": pref})).unwrap();
        let id = open(&mut app, "image.imageSize").unwrap();
        let f = fields(&app, id);
        assert_eq!((f.get("resample"), f.get("__resample")), (Some(&json!(method)), Some(&json!(method))), "{pref}");
        crate::dialogs::cancel(&mut app, id).unwrap();
    }
}

#[test]
#[ignore = "library row 162: sizing dialogs have no setters or getters"]
fn resample_preference_dialog_resizes_on_ok() {
    let mut app = app_with(200, 100, 72);
    let id = open(&mut app, "image.imageSize").unwrap();
    set(&mut app, id, "width", json!(50));
    set(&mut app, id, "height", json!(25));
    crate::dialogs::confirm(&mut app, id).unwrap();
    assert_eq!(app.session.active().unwrap().doc.size, photocraft_doc::Size::new(50, 25));
}

#[test]
fn a_script_cannot_set_size_dialog_values() {
    let mut app = app_with(200, 100, 72);
    let id = open(&mut app, "image.canvasSize").unwrap();
    assert!(crate::dialogs::hosted::set_field(&mut app, id, "width", json!(300)).is_err());
    crate::dialogs::cancel(&mut app, id).unwrap();
    assert!(app.ui.hosted(id).is_none() && !app.ui.host.is_open::<CanvasSizeDialog>());
    assert_eq!(app.session.active().unwrap().doc.size.width, 200);
}

/// Lanczos is the library's last method and reaches the command by the name it takes.
#[test]
fn lanczos_maps_to_the_command_name() {
    for name in ["automatic", "preserveDetails", "bicubic", "bicubicSmoother", "bicubicSharper", "bilinear", "nearest", "lanczos", "none"] {
        assert_eq!(resample_name(resample_of(name)), name);
    }
}

/// #441: a typed zero width showed "0 px × 1 px" while OK resized to 1×1.
#[test]
fn typed_zero_width_shows_the_applied_size() {
    let mut h = harness(app_with(4, 4, 72));
    let id = open(h.state_mut(), "image.imageSize").unwrap();
    h.run_steps(3);
    assert!(h.query_all_by_role(Role::SpinButton).next().expect("the Width field").is_focused());
    h.event(egui::Event::Text("0".into()));
    h.run_steps(1);
    h.key_press(egui::Key::Tab);
    h.run_steps(3);
    let f = fields(h.state(), id);
    assert_eq!((num(&f, "width"), num(&f, "height")), (1.0, 1.0), "{f:?}");
    crate::dialogs::confirm(h.state_mut(), id).unwrap();
    assert_eq!(h.state().session.active().unwrap().doc.size, photocraft_doc::Size::new(1, 1));
}

/// Picks `option` in the list named `list`.
fn pick(h: &mut Harness<'static, PhotocraftApp>, list: &str, option: &str) {
    h.get_by_label(list).click();
    h.run_steps(3);
    h.get_by_label(option).click();
    h.run_steps(3);
}

/// An Image Size dialog for a 100 x 50 px, 72 ppi document with Resample off, shown in `unit`.
fn resample_off_dialog(unit: &str) -> (Harness<'static, PhotocraftApp>, u64) {
    let mut h = harness(app_with(100, 50, 72));
    let id = open(h.state_mut(), "image.imageSize").unwrap();
    h.run_steps(3);
    h.get_by_label("Resample").click();
    h.run_steps(3);
    if unit != "Pixels" {
        pick(&mut h, "Width unit", unit);
    }
    (h, id)
}

/// Types `text` into the `n`th number field (Width, Height, Resolution) and leaves it with Tab.
fn type_into(h: &mut Harness<'static, PhotocraftApp>, n: usize, text: &str) {
    let width = h.query_all_by_role(Role::SpinButton).nth(n).map(|n| n.rect()).expect("the number field");
    h.hover_at(width.center());
    h.run_steps(1);
    h.drag_at(width.center());
    h.run_steps(1);
    h.drop_at(width.center());
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text(text.into()));
    h.run_steps(1);
    h.key_press(egui::Key::Tab);
    h.run_steps(3);
}

/// #2393: with Resample off, a pixel Width can't be edited and the readouts keep showing the
/// pixels OK keeps.
#[test]
fn resample_off_keeps_pixel_dimensions_in_the_dialog() {
    let (mut h, id) = resample_off_dialog("Pixels");
    let f = fields(h.state(), id);
    assert_eq!(f.get("resample"), Some(&json!("none")), "{f:?}");
    assert_eq!((num(&f, "width"), num(&f, "height"), num(&f, "resolution")), (100.0, 50.0, 72.0), "{f:?}");
    assert!(h.query_by_label("100 px × 50 px").is_some(), "the Dimensions line shows the kept pixels");
    crate::dialogs::confirm(h.state_mut(), id).unwrap();
    let doc = &h.state().session.active().unwrap().doc;
    assert_eq!((doc.size, doc.resolution_dpi), (photocraft_doc::Size::new(100, 50), 72.0));
}

/// With Resample off, a physical Width changes the resolution and keeps the pixels (Photoshop):
/// 100 px at 72 ppi is 1.39 in; 2 in wide is 50 ppi.
#[test]
fn resample_off_physical_width_changes_the_resolution() {
    let (mut h, id) = resample_off_dialog("Inches");
    type_into(&mut h, 0, "2");
    let f = fields(h.state(), id);
    assert_eq!((num(&f, "width"), num(&f, "height")), (100.0, 50.0), "{f:?}");
    assert!((num(&f, "resolution") - 50.0).abs() < 1e-3, "{f:?}");
    crate::dialogs::confirm(h.state_mut(), id).unwrap();
    let doc = &h.state().session.active().unwrap().doc;
    assert_eq!(doc.size, photocraft_doc::Size::new(100, 50));
    assert!((doc.resolution_dpi - 50.0).abs() < 1e-3, "{}", doc.resolution_dpi);
}

/// #5756: with Traditional points (72.27 pt/in) and Resample off, 100 px typed as 72.27 pt wide
/// is 100 ppi.
#[test]
#[ignore = "gap Z5: the library's size field rounds a typed value to the unit's display places (1 for points): 72.27 pt becomes 72.3 pt, 99.96 ppi"]
fn traditional_points_with_resample_off_set_the_resolution() {
    let mut app = app_with(100, 50, 72);
    app.session.execute("prefs.set", json!({"path": "unitsAndRulers.pointSize", "value": "traditional"})).unwrap();
    let mut h = harness(app);
    let id = open(h.state_mut(), "image.imageSize").unwrap();
    h.run_steps(3);
    h.get_by_label("Resample").click();
    h.run_steps(3);
    pick(&mut h, "Width unit", "Points");
    type_into(&mut h, 0, "72.27");
    let f = fields(h.state(), id);
    assert_eq!((num(&f, "width"), num(&f, "height")), (100.0, 50.0), "{f:?}");
    assert!((num(&f, "resolution") - 100.0).abs() < 1e-2, "{f:?}");
}

/// #6979: with Resample on, a new Resolution keeps the print size in a physical unit and
/// resamples the pixels (Photoshop): 720 x 576 px at 72 ppi is 10 x 8 in, so 300 ppi is
/// 3000 x 2400 px. In pixels the pixel count stays.
#[test]
fn resample_on_resolution_keeps_the_physical_size() {
    for (unit, size) in [("Inches", (3000.0, 2400.0)), ("Centimeters", (3000.0, 2400.0)), ("Pixels", (720.0, 576.0))] {
        let mut h = harness(app_with(720, 576, 72));
        let id = open(h.state_mut(), "image.imageSize").unwrap();
        h.run_steps(3);
        if unit != "Pixels" {
            pick(&mut h, "Width unit", unit);
        }
        type_into(&mut h, 2, "300");
        let f = fields(h.state(), id);
        assert_eq!((num(&f, "width"), num(&f, "height"), num(&f, "resolution")), (size.0, size.1, 300.0), "{unit}: {f:?}");
        crate::dialogs::confirm(h.state_mut(), id).unwrap();
        let doc = &h.state().session.active().unwrap().doc;
        assert_eq!((doc.size, doc.resolution_dpi), (photocraft_doc::Size::new(size.0 as u32, size.1 as u32), 300.0), "{unit}");
    }
}

/// #4162: a document wider than 30000 units in a physical unit (40000 px at 72 ppi is 40000 pt)
/// kept its size in the fields and on OK.
#[test]
fn large_documents_in_physical_units_open_unchanged() {
    let mut wrong = Vec::new();
    for command in ["image.imageSize", "image.canvasSize"] {
        let mut h = harness(app_with(40000, 4, 72));
        let id = open(h.state_mut(), command).unwrap();
        h.run_steps(3);
        pick(&mut h, "Width unit", "Points");
        let f = fields(h.state(), id);
        let shown = (num(&f, "width"), num(&f, "height"));
        crate::dialogs::confirm(h.state_mut(), id).unwrap();
        let size = h.state().session.active().unwrap().doc.size;
        if shown != (40000.0, 4.0) || size != photocraft_doc::Size::new(40000, 4) {
            wrong.push(format!("{command}: fields {shown:?}, document {size:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// #2329: Image Size and Canvas Size open on Width with its value selected, as in Photoshop:
/// typing replaces it and Enter applies it.
#[test]
fn sizing_dialogs_open_on_width_selected() {
    for command in ["image.imageSize", "image.canvasSize"] {
        let mut h = harness(app_with(200, 100, 72));
        open(h.state_mut(), command).unwrap();
        h.run_steps(3);
        assert!(h.query_all_by_role(Role::SpinButton).next().expect("the Width field").is_focused(), "{command}: Width has focus");
        h.event(egui::Event::Text("50".into()));
        h.run_steps(1);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert!(!h.state().ui.any_dialog_open(), "{command}: Enter is OK");
        assert_eq!(h.state().session.active().unwrap().doc.size.width, 50, "{command}");
    }
}
