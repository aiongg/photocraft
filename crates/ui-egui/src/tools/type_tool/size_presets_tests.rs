//! #12052: the Type options bar's font size field has the reference application's preset sizes. Picking one
//! edits the same targets as typing a size.

use egui::{Modifiers, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use photocraft_doc::LayerId;
use serde_json::json;

use crate::PhotocraftApp;

fn new_app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 1600, "height": 900})).unwrap();
    app.sync_views();
    app.ui.tool = crate::state::Tool::Type;
    app
}

fn add_layer(app: &mut PhotocraftApp, size: f32) -> LayerId {
    let made = app.run("type.create", json!({"text": "AéBCD", "font": "Inter", "size": size, "color": "#000000"})).unwrap();
    LayerId(made["layer"].as_u64().unwrap())
}

/// The options bar in a window, once the app's fonts are set up. Up to 16 passes, because the font list arrives
/// in batches (see `canvas_tests::controls`).
fn harness(app: PhotocraftApp) -> Harness<'static, PhotocraftApp> {
    let ctx = app.ctx().clone();
    let mut h = Harness::builder().with_context(ctx).with_size(vec2(1400.0, 300.0)).with_max_steps(16).build_ui_state(
        move |ui, app| {
            ui.horizontal(|ui| super::options_bar(app, ui));
        },
        app,
    );
    h.run_steps(4);
    h
}

/// Opens the size field's preset list with Alt+Down and picks `label`.
fn pick(h: &mut Harness<'static, PhotocraftApp>, label: &str) {
    // PLAN: the size field's accessible name is assumed to be the bar's "Set font size" words.
    h.get_by_label("Set font size").click();
    h.run();
    h.key_press_modifiers(Modifiers::ALT, egui::Key::ArrowDown);
    h.run();
    h.get_by_label(label).scroll_to_me();
    h.run();
    h.get_by_label(label).click();
    h.run();
}

/// Each run of the layer's text as (bytes, size in points).
fn runs(app: &PhotocraftApp, id: LayerId) -> Vec<(usize, f32)> {
    let t = super::super::text_layer(&app.session.active().unwrap().doc, id).unwrap();
    t.char_runs().iter().map(|r| (r.len, r.style.size_pt)).collect()
}

fn history_len(app: &PhotocraftApp) -> usize {
    app.session.active().unwrap().history.entries().len()
}

#[test]
fn picking_a_preset_sets_the_selected_layers_and_the_tool_default_in_one_step() {
    // Selected layers: both layers and the tool default, in one history step.
    let mut app = new_app();
    let ids = [add_layer(&mut app, 12.0), add_layer(&mut app, 16.0)];
    app.run("layer.select", json!({"layer": ids[0].0, "mode": "replace"})).unwrap();
    app.run("layer.select", json!({"layer": ids[1].0, "mode": "add"})).unwrap();
    let steps = history_len(&app);
    let mut h = harness(app);
    pick(&mut h, "72 pt");
    assert_eq!(runs(h.state(), ids[0]), [(6, 72.0)]);
    assert_eq!(runs(h.state(), ids[1]), [(6, 72.0)]);
    assert_eq!(h.state().ui.tool_options.type_size, 72.0);
    assert_eq!(history_len(h.state()), steps + 1);

    // An editing range: only the selected characters.
    let mut app = new_app();
    let id = add_layer(&mut app, 12.0);
    app.run("layer.select", json!({"layer": id.0, "mode": "replace"})).unwrap();
    app.ui.text_edit = Some(crate::state::TextEdit {
        layer: id.0,
        document: None,
        caret: 3,
        anchor: 1,
        session: "size-presets".into(),
        created: false,
        mask: false,
    });
    let mut h = harness(app);
    pick(&mut h, "48 pt");
    // "A" is one byte, "éB" three, "CD" two.
    assert_eq!(runs(h.state(), id), [(1, 12.0), (3, 48.0), (2, 12.0)]);

    // No type layer: the tool default only, no history.
    let app = new_app();
    let steps = history_len(&app);
    let mut h = harness(app);
    pick(&mut h, "72 pt");
    assert_eq!(h.state().ui.tool_options.type_size, 72.0);
    assert_eq!(history_len(h.state()), steps);
}
