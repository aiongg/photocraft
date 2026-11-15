use super::dialogs::{front, params_for};
use super::*;
use crate::menus::{invoke as menu, is_live, menu_rows, row};
use craft::automation::actions::{Action, ActionStep};
use craft::automation::ui::actions::ActionSelection;
use craft::ui::dialog::DialogId;
use craft::ui::patterns::form_dialogs::{HostedParamDialog, ParamKind};

fn app_with(n: usize) -> (PhotocraftApp, egui::Context) {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    for i in 0..n {
        app.run("file.new", json!({"width": 100 + i as u32 * 10, "height": 80})).unwrap();
    }
    app.sync_views();
    let ctx = app.ctx().clone();
    (app, ctx)
}

fn checked(app: &PhotocraftApp, id: &str) -> Option<bool> {
    row(&menu_rows(app), id).and_then(|r| r.kind.checked())
}

/// The dialog a command's reply names.
fn opened(app: &PhotocraftApp, reply: &Value) -> DialogId {
    let want = reply["dialog"].as_str().expect("the reply names a dialog");
    // PLAN: assumed `app.framework.dialogs: DialogHost` and `OpenDialog { id, .. }`
    app.framework.dialogs.open_dialogs().into_iter().map(|d| d.id).find(|id| id.value().to_string() == want).expect("the named dialog is open")
}

/// Runs a menu command that opens a form and returns the form's dialog.
fn open_form(app: &mut PhotocraftApp, ctx: &egui::Context, id: &str) -> DialogId {
    let reply = menu(app, ctx, id, json!({})).unwrap();
    opened(app, &reply)
}

/// The most recently opened dialog.
fn last_dialog(app: &PhotocraftApp) -> DialogId {
    app.framework.dialogs.open_dialogs().last().expect("a dialog is open").id
}

/// The values of the hosted form.
fn fields(app: &PhotocraftApp, id: DialogId) -> Value {
    serde_json::to_value(app.framework.dialogs.get_by_id::<HostedParamDialog>(id).expect("a hosted form").fields()).unwrap()
}

/// Sets a form value as the user editing that row would.
fn set(app: &mut PhotocraftApp, ctx: &egui::Context, id: DialogId, key: &str, value: Value) {
    app.framework.dialogs.set_json_field(ctx, id, key, value).unwrap_or_else(|e| panic!("{key}: {e}"));
}

fn confirm(app: &mut PhotocraftApp, ctx: &egui::Context, id: DialogId) -> Result<(), String> {
    crate::dialogs::confirm(app, ctx, id)
}

fn cancel(app: &mut PhotocraftApp, ctx: &egui::Context, id: DialogId) {
    crate::dialogs::cancel(app, ctx, id).unwrap();
}

/// The option ids of choice row `key` of command `command`'s form.
fn choice_ids(app: &mut PhotocraftApp, command: &str, key: &str) -> Vec<String> {
    let (params, _) = params_for(app, command).expect("the command has a form");
    match &params.iter().find(|p| p.key == key).unwrap_or_else(|| panic!("no row {key}")).kind {
        ParamKind::Choice { options, .. } => options.iter().map(|(id, _)| id.clone()).collect(),
        other => panic!("{key} is not a choice: {other:?}"),
    }
}

fn label_of(app: &mut PhotocraftApp, command: &str, key: &str) -> String {
    let (params, _) = params_for(app, command).expect("the command has a form");
    params.iter().find(|p| p.key == key).unwrap_or_else(|| panic!("no row {key}")).label.as_str().to_owned()
}

fn assert_mode(app: &PhotocraftApp, mode: &str) {
    assert_eq!(checked(app, &format!("view.screenMode.{mode}")), Some(true), "{mode}");
}

/// The names of the files directly in `dir`, sorted.
fn names_in(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

/// Fires a view command on the canvas and runs the frame that drains it.
fn fire(app: &mut PhotocraftApp, ctx: &egui::Context, command: craft::ui::canvas::ViewCommands) {
    // PLAN: assumed `app::frame::run(app, ui)` (top-w29's drain)
    craft::ui::runtime::commands::fire(ctx, command, craft::ui::runtime::commands::FireSource::Menu);
    ctx.run_ui(Default::default(), |ui| crate::app::frame::run(app, ui));
}

/// The form must list every source first and default to Current only when a document exists.
#[test]
fn image_processor_source_choices_include_current_and_all_open_documents() {
    for n in [0, 2] {
        let (mut app, ctx) = app_with(n);
        let id = open_form(&mut app, &ctx, "file.scripts.imageProcessor");
        assert_eq!(fields(&app, id)["source"], json!(if n == 0 { "folder" } else { "current" }));
        assert_eq!(choice_ids(&mut app, "file.scripts.imageProcessor", "source"), ["current", "open", "folder"]);
        assert_eq!(label_of(&mut app, "file.scripts.imageProcessor", "source"), "Source");
        let (params, _) = params_for(&mut app, "file.scripts.imageProcessor").unwrap();
        assert_eq!(params.first().map(|p| p.key.as_str()), Some("source"));
    }
}

/// Confirming either document source must ignore a stale input-folder field, export the
/// expected number of copies and retain the open documents' original snapshots.
#[test]
fn image_processor_dialog_confirms_open_document_sources_without_reading_the_input_folder() {
    for source in ["current", "open"] {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, ctx) = app_with(0);
        for name in ["First", "Second"] {
            app.run("file.new", json!({"name": name, "width": 100, "height": 80})).unwrap();
        }
        let before: Vec<_> = app.session.documents().iter().map(|st| st.doc.clone()).collect();
        let id = open_form(&mut app, &ctx, "file.scripts.imageProcessor");
        // The input row shows only for Folder: type the stale folder there, then switch to the document source.
        set(&mut app, &ctx, id, "source", json!("folder"));
        set(&mut app, &ctx, id, "input", json!(dir.path().join("missing-input")));
        set(&mut app, &ctx, id, "source", json!(source));
        set(&mut app, &ctx, id, "output", json!(dir.path()));
        set(&mut app, &ctx, id, "format", json!("png"));
        set(&mut app, &ctx, id, "width", json!(50));
        set(&mut app, &ctx, id, "height", json!(50));
        confirm(&mut app, &ctx, id).unwrap();
        assert_eq!(names_in(dir.path()).len(), if source == "current" { 1 } else { 2 }, "{source}");
        for (st, original) in app.session.documents().iter().zip(&before) {
            assert!(std::sync::Arc::ptr_eq(&st.doc, original));
        }
    }
}

/// #1209: the browser picker supplies bytes even when the engine cannot read native paths.
#[test]
fn place_embedded_enablement_does_not_require_native_doc() {
    for has_doc in [true, false] {
        for has_picker in [true, false] {
            let (mut app, _) = app_with(usize::from(has_doc));
            if has_picker {
                // PLAN: assumed top-w28's test fake: `Services::files: Option<Arc<dyn Files>>` over `ScriptedFiles`
                app.services.files = Some(std::sync::Arc::new(craft::host::files::ScriptedFiles::new()));
            }
            for native_filesystem in [false, true] {
                let native_doc = native_filesystem && has_doc;
                assert_eq!(
                    is_enabled_with_engine(&app, "file.placeEmbedded", || native_doc),
                    Some(has_doc && has_picker),
                    "Place Embedded: doc={has_doc}, picker={has_picker}, native_doc={native_doc}"
                );
                assert_eq!(
                    is_enabled_with_engine(&app, "file.placeLinked", || native_doc),
                    Some(native_doc),
                    "Place Linked must retain the engine's native_doc guard"
                );
            }
        }
    }
}

/// Menu ids that open a form: the hosted form's key is the command, a refusal says why, and nothing edits the
/// document before OK.
#[test]
fn command_dialogs_open_and_refuse() {
    type Setup = fn(&mut PhotocraftApp);
    let nothing: Setup = |_| {};
    let type_layer: Setup = |app| {
        app.run("type.create", json!({"x": 5, "y": 40, "text": "Warp", "size": 20})).unwrap();
    };
    let data_sets: Setup = |app| {
        app.run("image.variables.dataSets", json!({"dataSets": [{"name": "Set 1", "values": []}]})).unwrap();
    };
    let sets_open = if cfg!(target_arch = "wasm32") { Err("unavailable on the web") } else { Ok(()) };
    let cases: [(&str, Setup, Result<(), &str>); 9] = [
        ("file.automate.fitImage", nothing, Ok(())),
        ("file.fileInfo", nothing, Ok(())),
        ("layer.videoLayers.interpretFootage", nothing, Ok(())),
        ("image.applyImage", nothing, Ok(())),
        ("image.calculations", nothing, Ok(())),
        ("view.newGuideLayout", nothing, Ok(())),
        ("type.warpText", type_layer, Ok(())),
        ("file.automate.batch", nothing, Err("record an action")),
        ("file.export.dataSetsAsFiles", data_sets, sets_open),
    ];
    for (id, setup, want) in cases {
        let (mut app, ctx) = app_with(2);
        setup(&mut app);
        let revision = app.session.active().unwrap().revision;
        let reply = menu(&mut app, &ctx, id, json!({}));
        match want {
            Ok(()) => {
                let dialog = opened(&app, &reply.unwrap_or_else(|e| panic!("{id}: {e}")));
                assert_eq!(app.session.active().unwrap().revision, revision, "{id} edited before confirmation");
                if super::dialogs::is_form(id) {
                    assert!(app.framework.dialogs.get_by_id::<HostedParamDialog>(dialog).is_some(), "{id} opens a hosted parameter form");
                }
            }
            Err(text) => assert!(reply.unwrap_err().contains(text), "{id}"),
        }
    }
    // OK runs the command with the form's values.
    let (mut app, ctx) = app_with(2);
    let id = open_form(&mut app, &ctx, "file.automate.fitImage");
    assert_eq!(fields(&app, id)["width"], 110);
    set(&mut app, &ctx, id, "width", json!(55));
    set(&mut app, &ctx, id, "height", json!(1000));
    confirm(&mut app, &ctx, id).unwrap();
    assert_eq!(app.session.active().unwrap().doc.size.width, 55);
    let id = open_form(&mut app, &ctx, "file.fileInfo");
    set(&mut app, &ctx, id, "title", json!("Wheat Field"));
    confirm(&mut app, &ctx, id).unwrap();
    assert_eq!(app.run("file.fileInfo", json!({})).unwrap()["title"], "Wheat Field");
    // Close Others keeps the active document's view (the documents are dirty; the unsaved-changes prompt is covered in discard_ui).
    app.ui.views[1].zoom = 4.0;
    crate::menus::invoke_unguarded(&mut app, &ctx, "file.closeOthers", json!({})).unwrap();
    assert_eq!(app.ui.views.len(), 1);
    assert_eq!(app.ui.views[0].zoom, 4.0);
    crate::menus::invoke_unguarded(&mut app, &ctx, "file.closeAll", json!({})).unwrap();
    assert!(app.ui.views.is_empty() && app.session.documents().is_empty());
}

#[test]
fn data_sets_as_files_enablement_respects_target_platform() {
    let (app, _) = app_with(1);
    let enabled = is_enabled_with_engine(&app, "file.export.dataSetsAsFiles", || true);
    assert_eq!(enabled, Some(!cfg!(target_arch = "wasm32")));
}

#[test]
fn show_snap_extras_and_screen_modes_are_state() {
    let (mut app, ctx) = app_with(1);
    for id in [
        "view.show.layerEdges",
        "view.show.pixelGrid",
        "view.snapTo.layers",
        "view.extras",
        "view.screenMode.fullScreen",
        "view.flipHorizontal",
        "window.arrange.fourUp",
        "type.fontPreviewSize.huge",
        "type.languageOptions.eastAsianFeatures",
    ] {
        assert!(is_live(id), "{id}");
    }
    assert_eq!(checked(&app, "view.show.layerEdges"), Some(false));
    menu(&mut app, &ctx, "view.show.layerEdges", json!({})).unwrap();
    assert_eq!(checked(&app, "view.show.layerEdges"), Some(true));
    // Hiding Extras hides the edges; showing any item turns Extras back on.
    menu(&mut app, &ctx, "view.extras", json!({})).unwrap();
    assert!(!app.ui.view.extras && !app.ui.view.shows(app.ui.view.show.layer_edges));
    menu(&mut app, &ctx, "view.show.mesh", json!({})).unwrap();
    assert!(app.ui.view.extras);
    // F cycles the screen modes.
    for want in ["fullScreenWithMenuBar", "fullScreen", "standard"] {
        menu(&mut app, &ctx, "view.screenMode.cycle", json!({})).unwrap();
        assert_mode(&app, want);
    }
}

#[test]
fn zoom_presets_and_fit_layers() {
    use craft::ui::canvas::ViewCommands;
    let (mut app, ctx) = app_with(1);
    fire(&mut app, &ctx, ViewCommands::TwoHundredPercent);
    assert_eq!(app.ui.views[0].zoom, 2.0);
    app.run("image.imageSize", json!({"resolution": 144, "resample": "none"})).unwrap();
    fire(&mut app, &ctx, ViewCommands::PrintSize);
    assert_eq!(app.ui.views[0].zoom, 0.5);
    app.run("layer.new.layer", json!({})).unwrap();
    app.run("select.rect", json!({"x": 10, "y": 20, "width": 10, "height": 10})).unwrap();
    app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
    fire(&mut app, &ctx, ViewCommands::FitLayers);
    assert_eq!(app.ui.views[0].center, [15.0, 25.0]);
    assert!(app.ui.views[0].zoom > 10.0);
}

/// Print Size divides the document's ppi into the Screen Resolution preference
/// (Units & Rulers), not into a constant 72 ppi.
#[test]
fn print_size_uses_the_screen_resolution_preference() {
    let (mut app, ctx) = app_with(1);
    app.run("image.imageSize", json!({"resolution": 1, "resample": "none"})).unwrap();
    app.run("prefs.set", json!({"path": "unitsAndRulers.screenResolution", "value": 96.0})).unwrap();
    fire(&mut app, &ctx, craft::ui::canvas::ViewCommands::PrintSize);
    assert_eq!(app.ui.views[0].zoom, 96.0, "96 ppi screen / 1 ppi document = 9600 %");
}

#[test]
fn arrange_layouts_floating_windows_and_matching() {
    let (mut app, ctx) = app_with(3);
    menu(&mut app, &ctx, "window.arrange.threeUpStacked", json!({})).unwrap();
    assert_eq!(checked(&app, "window.arrange.threeUpStacked"), Some(true));
    menu(&mut app, &ctx, "window.arrange.floatAllInWindows", json!({})).unwrap();
    assert_eq!(app.ui.windows.len(), 3);
    app.ui.views[2].zoom = 3.0;
    app.ui.views[2].center = [1.0, 2.0];
    menu(&mut app, &ctx, "window.arrange.matchAll", json!({})).unwrap();
    assert!(app.ui.views.iter().all(|v| v.zoom == 3.0 && v.center == [1.0, 2.0]));
    assert!(app.ui.windows.iter().all(|w| w.view.zoom == 3.0));
    menu(&mut app, &ctx, "window.arrange.consolidateAllToTabs", json!({})).unwrap();
    assert!(app.ui.windows.is_empty());
    menu(&mut app, &ctx, "window.arrange.newWindowForDocument", json!({})).unwrap();
    assert_eq!(app.ui.windows.len(), 1);
}

/// Match Zoom clamps the matched zoom to each document's own range: a 10 x 10 document can't go
/// below the zoom at which its shorter side is 2 px, even when the source view is lower.
#[test]
fn match_zoom_clamps_to_each_documents_zoom_range() {
    // PLAN: assumed `ZoomLadder::min(Size)` takes the framework's `Size::new(width, height)`
    let lowest = craft::behavior::canvas::ZoomLadder::min(craft::behavior::geometry::Size::new(10.0, 10.0)) as f32;
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 10, "height": 10})).unwrap();
    app.run("file.new", json!({"width": 2000, "height": 2000})).unwrap();
    app.sync_views();
    let ctx = app.ctx().clone();
    assert_eq!(app.session.active_index(), Some(1));
    app.ui.views[1].zoom = 0.05;
    menu(&mut app, &ctx, "window.arrange.matchZoom", json!({})).unwrap();
    assert_eq!(app.ui.views[1].zoom, 0.05);
    assert_eq!(app.ui.views[0].zoom, lowest);
    // Floating windows too, before their first frame (no `doc_size` yet).
    menu(&mut app, &ctx, "window.arrange.floatAllInWindows", json!({})).unwrap();
    app.ui.views[1].zoom = 0.05;
    menu(&mut app, &ctx, "window.arrange.matchZoom", json!({})).unwrap();
    assert_eq!(app.ui.windows.len(), 2);
    assert!(app.ui.windows.iter().all(|w| w.view.zoom == if w.document == 0 { lowest } else { 0.05 }));
}

#[test]
fn type_menu_checkmarks_follow_the_layer() {
    let (mut app, ctx) = app_with(1);
    assert!(!crate::menus::is_enabled(&app, "type.antiAlias.crisp"));
    app.run("type.create", json!({"x": 10, "y": 50, "text": "Type", "size": 24})).unwrap();
    menu(&mut app, &ctx, "type.antiAlias.crisp", json!({})).unwrap();
    assert_eq!(checked(&app, "type.antiAlias.crisp"), Some(true));
    assert_eq!(checked(&app, "type.antiAlias.smooth"), Some(false));
    menu(&mut app, &ctx, "type.openType.fractions", json!({})).unwrap();
    assert_eq!(checked(&app, "type.openType.fractions"), Some(true));
    assert_eq!(checked(&app, "type.openType.standardLigatures"), Some(true));
    assert_eq!(checked(&app, "type.orientation.horizontal"), Some(true));
}

#[test]
fn numerals_checkmark_follows_the_selection() {
    let (mut app, ctx) = app_with(1);
    let id = "type.openType.numerals.tabularLining";
    assert!(!crate::menus::is_enabled(&app, id));
    assert_eq!(checked(&app, id), None, "no type layer");
    app.run("type.create", json!({"x": 10, "y": 50, "text": "Type", "size": 24})).unwrap();
    assert!(crate::menus::is_enabled(&app, id));
    assert_eq!(checked(&app, "type.openType.numerals.default"), Some(true));
    assert_eq!(checked(&app, id), Some(false));
    menu(&mut app, &ctx, id, json!({})).unwrap();
    assert_eq!(checked(&app, id), Some(true));
    assert_eq!(checked(&app, "type.openType.numerals.default"), Some(false));
    assert_eq!(checked(&app, "type.openType.numerals.lining"), Some(false));
    menu(&mut app, &ctx, "type.openType.numerals.default", json!({})).unwrap();
    assert_eq!(checked(&app, "type.openType.numerals.default"), Some(true));
    assert_eq!(checked(&app, id), Some(false));
    assert_eq!(checked(&app, "type.openType.numerals.notAStyle"), None);
}

#[test]
fn stylistic_set_checkmark_follows_the_toggle() {
    let (mut app, ctx) = app_with(1);
    let id = "type.openType.stylisticSets.set4";
    assert!(!crate::menus::is_enabled(&app, id));
    assert_eq!(checked(&app, id), None, "no type layer");
    app.run("type.create", json!({"x": 10, "y": 50, "text": "Type", "size": 24})).unwrap();
    assert!(crate::menus::is_enabled(&app, id));
    assert_eq!(checked(&app, id), Some(false));
    menu(&mut app, &ctx, id, json!({})).unwrap();
    assert_eq!(checked(&app, id), Some(true));
    assert_eq!(checked(&app, "type.openType.stylisticSets.set5"), Some(false));
    menu(&mut app, &ctx, id, json!({})).unwrap();
    assert_eq!(checked(&app, id), Some(false));
    for bad in ["set0", "set21", "set04", "set", "set-1", "setx", "set100"] {
        // Not through `checked`, which only finds catalog items: these ids must reach the resolver.
        assert_eq!(super::dispatch::checked(&app, &format!("type.openType.stylisticSets.{bad}")), None, "{bad}");
    }
}

/// Two actions of one name in sets A and B (a loaded library): the Batch form lists the name once
/// and both sets, starts on the selected action's set, and runs that set's action.
#[test]
fn batch_menu_runs_the_selected_sets_action_among_same_named_ones() {
    let (mut app, ctx) = app_with(0);
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.png").to_string_lossy().into_owned();
    let output = dir.path().join("out");
    app.run("file.new", json!({"width": 2, "height": 1})).unwrap();
    app.run("file.saveACopy", json!({"path": input})).unwrap();
    for (set, step) in [("A", "image.adjustments.invert"), ("B", "image.imageRotation.90cw")] {
        let mut action = Action::new("Retouch", vec![ActionStep::new(step, json!({}))]);
        action.set = Some(set.into());
        app.session.actions.list.push(action).unwrap();
    }
    app.ui.actions.panel.selected = Some(ActionSelection::Action(1));
    let id = open_form(&mut app, &ctx, "file.automate.batch");
    assert_eq!(choice_ids(&mut app, "file.automate.batch", "action"), ["Retouch"]);
    assert_eq!(choice_ids(&mut app, "file.automate.batch", "set"), ["A", "B"]);
    let shown = fields(&app, id);
    assert_eq!((&shown["action"], &shown["set"]), (&json!("Retouch"), &json!("B")));
    set(&mut app, &ctx, id, "input", json!(dir.path().to_string_lossy()));
    set(&mut app, &ctx, id, "output", json!(output.to_string_lossy()));
    confirm(&mut app, &ctx, id).unwrap();
    let saved = names_in(&output).into_iter().next().unwrap();
    let image = photocraft_io::import("out.png", &std::fs::read(output.join(saved)).unwrap()).unwrap().document;
    assert_eq!((image.size.width, image.size.height), (1, 2), "set B's action (a rotation) ran");
}

/// Choosing a different action changes the output; cancelled or deleted choices write nothing.
#[test]
fn batch_menu_runs_the_action_chosen_in_the_form() {
    let (mut app, ctx) = app_with(0);
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("in");
    std::fs::create_dir(&source).unwrap();
    let output = dir.path().join("out");
    app.run("file.new", json!({"width": 2, "height": 1})).unwrap();
    app.run("file.saveACopy", json!({"path": source.join("source.png").to_string_lossy()})).unwrap();
    for (name, step) in [("Invert", "image.adjustments.invert"), ("RotateRGB", "image.imageRotation.90cw")] {
        app.session.actions.list.push(Action::new(name, vec![ActionStep::new(step, json!({}))])).unwrap();
    }
    app.ui.actions.panel.selected = Some(ActionSelection::Action(0));
    let id = open_form(&mut app, &ctx, "file.automate.batch");
    assert!(!fields(&app, id).as_object().unwrap().contains_key("steps"));
    set(&mut app, &ctx, id, "input", json!(source.to_string_lossy()));
    set(&mut app, &ctx, id, "output", json!(output.to_string_lossy()));
    set(&mut app, &ctx, id, "action", json!("RotateRGB"));
    assert_eq!(fields(&app, id)["action"], "RotateRGB");
    confirm(&mut app, &ctx, id).unwrap();
    let written = names_in(&output);
    assert_eq!(written.len(), 1);
    let image = photocraft_io::import("source.png", &std::fs::read(output.join(&written[0])).unwrap()).unwrap().document;
    assert_eq!((image.size.width, image.size.height), (1, 2));

    let cancelled = dir.path().join("cancelled");
    let id = open_form(&mut app, &ctx, "file.automate.batch");
    set(&mut app, &ctx, id, "output", json!(cancelled.to_string_lossy()));
    cancel(&mut app, &ctx, id);
    assert!(!cancelled.exists());

    let id = open_form(&mut app, &ctx, "file.automate.batch");
    set(&mut app, &ctx, id, "action", json!("RotateRGB"));
    set(&mut app, &ctx, id, "output", json!(cancelled.to_string_lossy()));
    let rotate = app.session.actions.list.actions().iter().position(|action| action.name == "RotateRGB").unwrap();
    app.session.actions.list.delete_action(rotate).unwrap();
    assert!(confirm(&mut app, &ctx, id).unwrap_err().contains("no action named"));
    assert!(!cancelled.exists());
}

#[test]
fn apply_image_dialog_offers_source_layer_scale_and_offset() {
    let (mut app, ctx) = app_with(0);
    app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
    app.run("edit.fill", json!({"color": "#c08040"})).unwrap();
    app.run("layer.new.layer", json!({"name": "Skin High"})).unwrap();
    app.run("edit.fill", json!({"color": "#406080"})).unwrap();
    app.sync_views();
    let background = app.session.active().unwrap().doc.walk()[0].2.name.clone();
    let d = open_form(&mut app, &ctx, "image.applyImage");
    let shown = fields(&app, d);
    assert_eq!(shown["sourceLayer"], "merged");
    assert_eq!(shown["scale"], 1.0);
    assert_eq!(shown["offset"], 0);
    // Merged first, then the layers top to bottom, shown by their own names.
    assert_eq!(choice_ids(&mut app, "image.applyImage", "sourceLayer"), ["merged", "Skin High", &background]);
    // The frequency separation recipe: a layer subtracted from itself at Scale 2, Offset 128 is mid grey.
    for (k, v) in [("sourceLayer", json!("Skin High")), ("blending", json!("subtract")), ("scale", json!(2.0)), ("offset", json!(128))] {
        set(&mut app, &ctx, d, k, v);
    }
    confirm(&mut app, &ctx, d).unwrap();
    let st = app.session.active().unwrap();
    let p = st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap().pixel(3, 3);
    assert!(p[..3].iter().all(|v| (v - 128.0 / 255.0).abs() < 0.003), "{p:?}");
    app.run("edit.undo", json!({})).unwrap();
    // The chosen layer is the source, not the merged image.
    let d = open_form(&mut app, &ctx, "image.applyImage");
    for (k, v) in [("sourceLayer", json!(background)), ("blending", json!("subtract")), ("scale", json!(2.0)), ("offset", json!(128))] {
        set(&mut app, &ctx, d, k, v);
    }
    confirm(&mut app, &ctx, d).unwrap();
    let st = app.session.active().unwrap();
    let p = st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap().pixel(3, 3);
    let want = (64.0 - 192.0) / 255.0 / 2.0 + 128.0 / 255.0;
    assert!((p[0] - want).abs() < 0.003, "{p:?} vs {want}");
}

#[test]
fn new_guide_layout_remembers_successful_values_not_cancelled_edits() {
    let (mut app, ctx) = app_with(1);
    let first = open_form(&mut app, &ctx, "view.newGuideLayout");
    assert_eq!(fields(&app, first)["columns"], 8);
    assert_eq!(fields(&app, first)["gutter"], "20 px");
    for (key, value) in [("columns", json!(2)), ("gutter", json!("0 px")), ("rows", json!(2)), ("rowGutter", json!("0 px"))] {
        set(&mut app, &ctx, first, key, value);
    }
    confirm(&mut app, &ctx, first).unwrap();

    let second = open_form(&mut app, &ctx, "view.newGuideLayout");
    let shown = fields(&app, second);
    assert_eq!(shown["columns"], 2);
    assert_eq!(shown["gutter"], "0 px");
    assert_eq!(shown["rows"], 2);
    assert_eq!(shown["rowGutter"], "0 px");
    assert_eq!(shown["clearExisting"], false);
    // A cancelled edit must not replace the last successfully used values.
    set(&mut app, &ctx, second, "columns", json!(9));
    cancel(&mut app, &ctx, second);
    let third = open_form(&mut app, &ctx, "view.newGuideLayout");
    assert_eq!(fields(&app, third)["columns"], 2);
}

#[test]
fn new_guide_layout_does_not_remember_a_failed_command() {
    let (mut app, ctx) = app_with(0);
    let reply = front(&mut app, &ctx, "view.newGuideLayout", &json!({})).unwrap().unwrap();
    let id = opened(&app, &reply);
    set(&mut app, &ctx, id, "columns", json!(3));
    assert!(confirm(&mut app, &ctx, id).is_err());
    let again = open_form(&mut app, &ctx, "view.newGuideLayout");
    assert_eq!(fields(&app, again)["columns"], 8);
}

#[test]
fn lookup_export_dialog_uses_the_current_adjustment_selection() {
    let (mut app, ctx) = app_with(1);
    app.run("layer.newAdjustmentLayer.invert", json!({})).unwrap();
    let current = app.session.active().unwrap().active_layer.unwrap();
    let opened_reply = front(&mut app, &ctx, "file.export.colorLookupTables", &json!({})).unwrap().unwrap();
    let id = opened(&app, &opened_reply);
    let shown = fields(&app, id);
    assert_eq!(shown["scope"], "selected");
    assert_eq!(shown["size"], 33);
    assert_eq!(choice_ids(&mut app, "file.export.colorLookupTables", "scope"), ["all", "selected"]);
    cancel(&mut app, &ctx, id);

    app.run("layer.new.layer", json!({"name": "Unrelated pixels"})).unwrap();
    let opened_reply = front(&mut app, &ctx, "file.export.colorLookupTables", &json!({})).unwrap().unwrap();
    let id = opened(&app, &opened_reply);
    assert_eq!(fields(&app, id)["scope"], "all");
    cancel(&mut app, &ctx, id);

    app.run("layer.select", json!({"layer": current})).unwrap();
    let direct = app.run("file.export.colorLookupTables", json!({"size": 3, "scope": "selected"})).unwrap();
    assert_eq!(direct["layerCount"], 1);
    assert!(direct["cube"].as_str().unwrap().contains("LUT_3D_SIZE 3"));
}

/// Batch and Image Processor carry Photoshop's "Include All Subfolders" checkbox (AUTO-219-5,
/// #3753): off by default, and confirming it on processes a subfolder's images too.
#[test]
fn batch_dialogs_include_all_subfolders() {
    let (mut app, ctx) = app_with(1);
    let dir = std::env::temp_dir().join(format!("photocraft-ui-batch-subfolders-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("in/sub")).unwrap();
    let input = dir.join("in").to_string_lossy().into_owned();
    app.run("file.saveACopy", json!({"path": dir.join("in/sub/a.png").to_string_lossy()})).unwrap();
    app.session.actions.list.push(Action::new("Invert", vec![ActionStep::new("image.adjustments.invert", json!({}))])).unwrap();
    for (command, out) in [("file.automate.batch", "batch"), ("file.scripts.imageProcessor", "processed")] {
        let d = open_form(&mut app, &ctx, command);
        let shown = fields(&app, d);
        assert_eq!(shown["includeSubfolders"], json!(false), "{command}");
        assert_eq!(label_of(&mut app, command, "includeSubfolders"), "Include All Subfolders", "{command}");
        if command == "file.automate.batch" {
            assert_eq!(shown["source"], json!("folder"));
            assert_eq!(shown["stopOnError"], json!(false));
            assert_eq!(shown["errorLog"], json!(""));
        }
        // The folder rows show only for the Folder source.
        set(&mut app, &ctx, d, "source", json!("folder"));
        set(&mut app, &ctx, d, "input", json!(input));
        set(&mut app, &ctx, d, "output", json!(dir.join(out).to_string_lossy()));
        set(&mut app, &ctx, d, "includeSubfolders", json!(true));
        confirm(&mut app, &ctx, d).unwrap();
        if command == "file.automate.batch" {
            assert!(dir.join(out).join("a.png").is_file(), "{command} writes subfolder inputs flat");
            assert!(!dir.join(out).join("sub").exists());
        } else {
            assert!(dir.join(out).join("sub").is_dir(), "{command} keeps the subfolder");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// #10682 / #10113: an action recorded with the Type tool (a new type layer, typed in key by key)
/// runs in Batch on every file of the folder and saves each into the destination.
#[test]
fn batch_runs_an_action_recorded_with_the_type_tool() {
    let (mut app, ctx) = app_with(1);
    let dir = std::env::temp_dir().join(format!("photocraft-ui-batch-type-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("in")).unwrap();
    for name in ["a.png", "b.png"] {
        app.run("file.saveACopy", json!({"path": dir.join("in").join(name).to_string_lossy()})).unwrap();
    }
    app.run("actions.record", json!({"name": "Caption"})).unwrap();
    let id = app.run("type.create", json!({"x": 10, "y": 40, "text": "", "size": 20})).unwrap()["layer"].as_u64().unwrap();
    for (i, c) in "Hi".chars().enumerate() {
        app.run("type.edit", json!({"layer": id, "replace": {"start": i, "end": i, "text": c.to_string()}, "coalesce": "typing"})).unwrap();
    }
    app.run("actions.stop", json!({})).unwrap();
    app.run("file.saveACopy", json!({"path": dir.join("ref.png").to_string_lossy()})).unwrap();
    assert_eq!(crate::actions::selected_action(&app).map(|a| a.steps.len()), Some(1), "one step for the typing");
    // CodeRabbit on #10730: and that step makes the layer with the text typed.
    assert_eq!(crate::actions::selected_action(&app).and_then(|a| a.steps.first()).map(|step| step.params["text"].clone()), Some(json!("Hi")));
    let d = open_form(&mut app, &ctx, "file.automate.batch");
    set(&mut app, &ctx, d, "input", json!(dir.join("in").to_string_lossy()));
    set(&mut app, &ctx, d, "output", json!(dir.join("out").to_string_lossy()));
    confirm(&mut app, &ctx, d).unwrap();
    assert_eq!(names_in(&dir.join("out")), ["a.png", "b.png"]);
    for name in ["a.png", "b.png"] {
        // CodeRabbit on #10730: the action ran on each file: the typed text is in its pixels.
        let pixels = |folder: &str| photocraft_codecs::decode(&std::fs::read(dir.join(folder).join(name)).unwrap()).unwrap().data().to_vec();
        assert_ne!(pixels("out"), pixels("in"), "{name}: the text was laid on it");
    }
    // CodeRabbit on #10730: all of it, as typed by hand: the document the action was recorded
    // on (the same image, with the text typed into it) has the pixels Batch wrote.
    let decoded = |path: std::path::PathBuf| photocraft_codecs::decode(&std::fs::read(path).unwrap()).unwrap().data().to_vec();
    for name in ["a.png", "b.png"] {
        assert_eq!(decoded(dir.join("out").join(name)), decoded(dir.join("ref.png")), "{name}: Batch laid the whole text, as it was typed by hand");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// #10682: a Batch whose action fails on every file said nothing; the status bar and a notice now
/// name each file that failed and why.
#[test]
fn a_batch_that_fails_on_its_files_says_which_and_why() {
    let (mut app, ctx) = app_with(1);
    let dir = std::env::temp_dir().join(format!("photocraft-ui-batch-fails-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("in")).unwrap();
    for name in ["a.png", "b.png"] {
        app.run("file.saveACopy", json!({"path": dir.join("in").join(name).to_string_lossy()})).unwrap();
    }
    // A step for a layer that only the recording document has.
    app.session
        .actions
        .list
        .push(Action::new("Stale", vec![ActionStep::new("layer.setProps", json!({"layer": 424_242, "opacity": 50}))]))
        .unwrap();
    let d = open_form(&mut app, &ctx, "file.automate.batch");
    set(&mut app, &ctx, d, "input", json!(dir.join("in").to_string_lossy()));
    set(&mut app, &ctx, d, "output", json!(dir.join("out").to_string_lossy()));
    let notices = app.ui.notices.len();
    confirm(&mut app, &ctx, d).unwrap();
    assert!(app.ui.status_error, "the status bar reports it: {}", app.ui.status);
    let notice = app.ui.notices.get(notices).expect("a notice lists the files");
    assert_eq!(notice.lines.len(), 2);
    // CodeRabbit on #10730: a failure, not a warning, and each file by its whole path.
    assert!(notice.error && notice.title.ends_with("2 files failed"), "{}", notice.title);
    let path = |name: &str| format!("{}: ", dir.join("in").join(name).to_string_lossy());
    assert!(notice.lines[0].starts_with(&path("a.png")) && notice.lines[1].starts_with(&path("b.png")), "{:?}", notice.lines);
    // CodeRabbit on #10730: each line is its file's path, then a reason.
    for (line, name) in notice.lines.iter().zip(["a.png", "b.png"]) {
        assert!(!line.strip_prefix(&path(name)).unwrap_or_default().trim().is_empty(), "{line}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Float in Window takes the document and the screen position a dragged-off tab was dropped at
/// (#3733); bad params are errors, not panics.
#[test]
fn float_in_window_takes_a_document_and_position() {
    let (mut app, ctx) = app_with(3);
    let r = menu(&mut app, &ctx, "window.arrange.floatInWindow", json!({"document": 1, "position": [1950.0, 40.5]})).unwrap();
    let w = app.ui.windows.last().unwrap();
    assert_eq!(r, json!({"window": w.id}));
    assert_eq!((w.document, w.position), (1, Some([1950.0, 40.5])));
    assert!(w.floating);
    assert_eq!(app.session.active_index(), Some(1), "the floated document stays active");
    let again = menu(&mut app, &ctx, "window.arrange.floatInWindow", json!({"document": 1})).unwrap();
    assert_eq!(again, r, "tearing off the same document reuses its window");
    assert_eq!(app.ui.windows.len(), 1);
    assert_eq!(app.ui.windows[0].position, Some([1950.0, 40.5]));
    for bad in [
        json!({"document": 3}),
        json!({"document": -1}),
        json!({"document": "b"}),
        json!({"position": [1.0]}),
        json!({"position": [1.0, "x"]}),
        json!({"position": [1.0e30, 0.0]}),
        json!({"position": 5}),
    ] {
        assert!(menu(&mut app, &ctx, "window.arrange.floatInWindow", bad.clone()).is_err(), "{bad}");
    }
    assert_eq!(app.ui.windows.len(), 1, "rejected params open no window");
    app.ui.windows[0].open = false;
    let reopened = menu(&mut app, &ctx, "window.arrange.floatInWindow", json!({"document": 1})).unwrap();
    assert_ne!(reopened, r, "a closed window can be floated again");
    assert_eq!(app.ui.windows.last().map(|w| (w.document, w.position)), Some((1, None)));
}

#[test]
fn float_all_in_windows_gives_each_document_its_own_view() {
    let (mut app, ctx) = app_with(3);
    menu(&mut app, &ctx, "window.arrange.floatAllInWindows", json!({})).unwrap();
    assert_eq!(app.ui.windows.iter().map(|w| w.document).collect::<Vec<_>>(), [0, 1, 2]);
    assert!(app.ui.windows.iter().all(|w| w.floating));
}

#[test]
fn save_selection_opens_its_dialog_and_saves_on_confirm() {
    // #3713: Select › Save Selection… asks for the channel, name and operation first.
    let (mut app, ctx) = app_with(1);
    app.run("select.all", json!({})).unwrap();
    let revision = app.session.active().unwrap().revision;
    let dialog = open_form(&mut app, &ctx, "select.saveSelection");
    assert_eq!(app.session.active().unwrap().revision, revision, "saved before confirmation");
    assert!(app.session.active().unwrap().doc.channels.is_empty());
    assert_eq!(fields(&app, dialog)["channel"], "new");
    assert_eq!(choice_ids(&mut app, "select.saveSelection", "operation"), ["new", "replace", "add", "subtract", "intersect"]);
    set(&mut app, &ctx, dialog, "name", json!("Keep"));
    confirm(&mut app, &ctx, dialog).unwrap();
    let names: Vec<_> = app.session.active().unwrap().doc.channels.iter().map(|c| c.name.clone()).collect();
    assert_eq!(names, ["Keep"]);
    // A second save lists the new channel as a destination.
    open_form(&mut app, &ctx, "select.saveSelection");
    assert_eq!(choice_ids(&mut app, "select.saveSelection", "channel"), ["new", "Keep"]);
}

#[test]
fn load_selection_opens_its_dialog_and_loads_on_confirm() {
    // #3713: Select › Load Selection… used to fail with "missing `channel`".
    let (mut app, ctx) = app_with(1);
    app.run("select.all", json!({})).unwrap();
    app.run("select.saveSelection", json!({"name": "Keep"})).unwrap();
    app.run("select.deselect", json!({})).unwrap();
    let dialog = open_form(&mut app, &ctx, "select.loadSelection");
    assert!(app.session.active().unwrap().doc.selection.is_none(), "loaded before confirmation");
    let shown = fields(&app, dialog);
    assert_eq!(shown["channel"], "Keep");
    assert_eq!(shown["invert"], false);
    assert_eq!(shown["operation"], "new");
    let channels = choice_ids(&mut app, "select.loadSelection", "channel");
    assert!(channels.contains(&"Keep".to_owned()) && channels.contains(&"transparency".to_owned()));
    // The layer has no mask, so its mask is not offered.
    assert!(!channels.contains(&"mask".to_owned()), "{channels:?}");
    confirm(&mut app, &ctx, dialog).unwrap();
    assert!(app.session.active().unwrap().doc.selection.is_some());
}

#[test]
fn load_selection_without_alpha_channels_offers_the_composite() {
    let (mut app, ctx) = app_with(1);
    let dialog = open_form(&mut app, &ctx, "select.loadSelection");
    assert_eq!(fields(&app, dialog)["channel"], "RGB");
    confirm(&mut app, &ctx, dialog).unwrap();
}

/// One frame of the shortcut handler with a plain Esc press.
fn press_esc(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let key = egui::Key::Escape;
    let input = egui::RawInput {
        focused: true,
        events: vec![egui::Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }],
        ..Default::default()
    };
    ctx.run_ui(input, |ui| crate::shortcuts::handle(app, ui.ctx())).textures_delta.clear();
}

#[test]
fn esc_leaves_full_screen_with_the_crop_tool_showing_its_default_frame() {
    let (mut app, ctx) = app_with(1);
    app.ui.tool = crate::Tool::Crop;
    crate::crop_ui::each_pass(&mut app);
    // PLAN: assumed `crop_ui::is_default_frame(&PhotocraftApp) -> bool` (tools-w14): the pending frame is the untouched default
    let default_frame = |app: &PhotocraftApp| crate::crop_ui::frame(app).is_some() && crate::crop_ui::is_default_frame(app);
    assert!(default_frame(&app));
    for mode in ["fullScreen", "fullScreenWithMenuBar"] {
        menu(&mut app, &ctx, &format!("view.screenMode.{mode}"), json!({})).unwrap();
        press_esc(&mut app, &ctx);
        assert_mode(&app, "standard");
        assert!(default_frame(&app), "{mode}: the default frame stays");
    }
    // With Straighten armed, Esc drops it first and the mode stays.
    menu(&mut app, &ctx, "view.screenMode.fullScreen", json!({})).unwrap();
    crate::crop_ui::toggle_straighten(&mut app);
    assert!(crate::crop_ui::straightening(&app));
    press_esc(&mut app, &ctx);
    assert!(!crate::crop_ui::straightening(&app));
    assert_mode(&app, "fullScreen");
    crate::crop_ui::each_pass(&mut app);
    press_esc(&mut app, &ctx);
    assert_mode(&app, "standard");
}

/// #9275: a gesture on the default frame is something to cancel, so Esc in full screen cancels it
/// (as in Standard Screen Mode) and full screen stays.
#[test]
fn esc_in_full_screen_cancels_a_crop_drag_on_the_default_frame() {
    use crate::canvas::testing::{ToolEvent, tool_event};
    let none = craft::keymap::Mods::NONE;
    // Inside draws a new box, on the corner handle resizes, outside turns.
    for (what, from) in [("draw", [20.0, 20.0]), ("resize", [100.0, 80.0]), ("turn", [130.0, 110.0])] {
        let (mut app, ctx) = app_with(1);
        app.ui.tool = crate::Tool::Crop;
        crate::crop_ui::each_pass(&mut app);
        menu(&mut app, &ctx, "view.screenMode.fullScreen", json!({})).unwrap();
        tool_event(&mut app, ToolEvent::Down { x: from[0], y: from[1], pressure: 1.0 }, none);
        tool_event(&mut app, ToolEvent::Move { x: 60.0, y: 50.0, pressure: 1.0 }, none);
        assert!(crate::crop_ui::dragging(&app) && crate::crop_ui::is_default_frame(&app), "{what}: mid-gesture on the default frame");
        press_esc(&mut app, &ctx);
        assert_mode(&app, "fullScreen");
        assert!(!crate::crop_ui::dragging(&app), "{what}: the gesture is cancelled");
        tool_event(&mut app, ToolEvent::Up { x: 60.0, y: 50.0 }, none);
        assert!(!crate::crop_ui::pending(&app), "{what}: the release leaves no pending crop");
    }
}

#[test]
fn esc_leaves_full_screen_modes_unless_something_else_owns_it() {
    let (mut app, ctx) = app_with(1);
    // Both full screen modes return to Standard, and the menu check follows (#4116).
    for mode in ["fullScreen", "fullScreenWithMenuBar"] {
        menu(&mut app, &ctx, &format!("view.screenMode.{mode}"), json!({})).unwrap();
        press_esc(&mut app, &ctx);
        assert_mode(&app, "standard");
    }
    // In Standard, Esc leaves the mode alone.
    press_esc(&mut app, &ctx);
    assert_mode(&app, "standard");
    // An open dialog takes Esc first.
    menu(&mut app, &ctx, "view.screenMode.fullScreen", json!({})).unwrap();
    menu(&mut app, &ctx, "image.canvasSize", json!({})).unwrap();
    assert_eq!(app.framework.dialogs.open_dialogs().len(), 1);
    press_esc(&mut app, &ctx);
    assert_mode(&app, "fullScreen");
    let dialog = last_dialog(&app);
    cancel(&mut app, &ctx, dialog);
    // So does Free Transform: Esc cancels it and the mode stays.
    app.run("select.all", json!({})).unwrap();
    crate::transform_tool::begin(&mut app, &ctx).unwrap();
    assert!(app.ui.transform.is_some());
    press_esc(&mut app, &ctx);
    assert!(app.ui.transform.is_none());
    assert_mode(&app, "fullScreen");
    // A selection stays while Esc leaves full screen; the next Esc deselects.
    press_esc(&mut app, &ctx);
    assert_mode(&app, "standard");
    assert!(app.session.active().unwrap().doc.selection.is_some());
}

/// A selection command that makes a new selection shows Extras again; anything else leaves them as the user set
/// them, and a new selection keeps the individual Show choices.
#[test]
fn selection_outcomes_restore_hidden_extras() {
    type Setup = fn(&mut PhotocraftApp);
    let fill: Setup = |app| {
        app.run("edit.fill", json!({"color": "#cc4422"})).unwrap();
    };
    let selected: Setup = |app| {
        app.run("select.rect", json!({"x": 10, "y": 10, "width": 20, "height": 20})).unwrap();
    };
    let white_selected: Setup = |app| {
        app.run("edit.fill", json!({"color": "#ffffff"})).unwrap();
        app.run("select.rect", json!({"x": 10, "y": 10, "width": 20, "height": 20})).unwrap();
    };
    let object_scene: Setup = |app| {
        app.run("file.new", json!({"width": 200, "height": 160})).unwrap();
        app.run("edit.fill", json!({"color": "#4073b3"})).unwrap();
        app.run("select.rect", json!({"x": 55, "y": 35, "width": 90, "height": 90, "ellipse": true})).unwrap();
        app.run("edit.fill", json!({"color": "#e64d26"})).unwrap();
        app.run("select.deselect", json!({})).unwrap();
    };
    let edges_off: Setup = |app| {
        app.ui.view.show.selection_edges = false;
        app.ui.view.show.pixel_grid = false;
    };
    let tri = json!([[10, 10], [30, 10], [20, 30]]);
    // (name, prepare, command, params, Extras shown after, selection edges shown after)
    let cases: Vec<(&str, Setup, &str, Value, bool, bool)> = vec![
        ("rect", fill, "select.rect", json!({"x": 10, "y": 10, "width": 20, "height": 20}), true, true),
        ("ellipse", fill, "select.rect", json!({"x": 10, "y": 10, "width": 20, "height": 20, "ellipse": true}), true, true),
        ("lasso", fill, "select.lasso", json!({"points": tri}), true, true),
        ("wand", fill, "select.magicWand", json!({"x": 10, "y": 10}), true, true),
        ("load", fill, "select.loadSelection", json!({"channel": "transparency"}), true, true),
        ("luminosity", fill, "select.loadLuminosity", json!({}), true, true),
        ("failed", selected, "select.rect", json!({"x": "invalid"}), false, false),
        ("fill under selection", selected, "edit.fill", json!({"color": "#cc4422"}), false, false),
        ("deselect", selected, "select.deselect", json!({}), false, false),
        ("object finds nothing", white_selected, "select.object", json!({"point": [3, 3]}), false, false),
        ("object finds one", object_scene, "select.object", json!({"rect": [45, 25, 110, 110]}), true, true),
        ("show choices kept", edges_off, "select.rect", json!({"x": 10, "y": 10, "width": 20, "height": 20}), true, false),
    ];
    for (name, prepare, command, params, extras, edges) in cases {
        let (mut app, ctx) = app_with(1);
        prepare(&mut app);
        menu(&mut app, &ctx, "view.extras", json!({})).unwrap();
        assert!(!app.ui.view.extras, "{name}: Extras hidden before the command");
        let _ = app.run(command, params);
        assert_eq!(app.ui.view.extras, extras, "{name}");
        assert_eq!(app.ui.view.shows(app.ui.view.show.selection_edges), edges, "{name}");
        if name == "show choices kept" {
            assert!(!app.ui.view.show.pixel_grid, "{name}: the pixel grid stays off");
        }
        if name == "object finds one" {
            let selection = app.session.active().unwrap().doc.selection.as_ref().unwrap();
            assert!(selection.sample_channel(100, 80, 0) > 0.5, "the central object is selected");
        }
    }
}

/// `view.rotateView` takes an angle, a delta or a reset; anything else, or a number that is not finite, is refused
/// and leaves the view as it was. A finite f64 that overflows f32 is refused, not wrapped to 0, which would reset a
/// rotated view.
#[test]
fn rotate_view_params_set_add_reset_and_reject() {
    let (mut app, ctx) = app_with(1);
    // PLAN: assumed the document's view keeps its rotation in degrees as `views[0].rotation`
    let angle = |app: &PhotocraftApp| f64::from(app.ui.views[0].rotation);
    let mut run = |app: &mut PhotocraftApp, p: Value| menu(app, &ctx, "view.rotateView", p);
    run(&mut app, json!({"angle": 30.0})).unwrap();
    assert_eq!(angle(&app), 30.0);
    run(&mut app, json!({"delta": 15.0})).unwrap();
    assert_eq!(angle(&app), 45.0);
    run(&mut app, json!({"reset": true})).unwrap();
    assert_eq!(angle(&app), 0.0);
    run(&mut app, json!({"angle": 170.0})).unwrap();
    run(&mut app, json!({"delta": 20.0})).unwrap();
    assert_eq!(angle(&app), -170.0, "the angle wraps at 180");
    run(&mut app, json!({"angle": 32.0})).unwrap();
    for bad in [
        json!({}),
        Value::Null,
        json!("30"),
        json!({"angle": f64::NAN}),
        json!({"angle": f64::INFINITY}),
        json!({"delta": f64::INFINITY}),
        json!({"angle": 1.0e100}),
        json!({"delta": -1.0e100}),
        json!({"reset": "yes"}),
        json!({"zoom": 1}),
    ] {
        assert!(run(&mut app, bad.clone()).is_err(), "{bad}");
        assert_eq!(angle(&app), 32.0, "{bad}: the view stays");
    }
    run(&mut app, json!({"angle": 1.0e20})).unwrap();
    assert!(angle(&app).is_finite() && angle(&app).abs() <= 180.0);
}
