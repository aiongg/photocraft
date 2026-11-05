//! #7942 / #7531: path fields of command dialogs get a Browse… button (Artboards to Files'
//! folder, Install Plug-in…'s .wasm file) that fills the field from a file dialog.
use crate::PhotocraftApp;
use egui::vec2;
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::{Value, json};

fn harness(answers: Vec<Option<craft::host::Picked>>) -> (Harness<'static, PhotocraftApp>, std::sync::Arc<crate::file_picks::scripted::ScriptedFiles>) {
    let (show, asked) = crate::file_picks::scripted::fake(answers);
    let h = Harness::builder().with_size(vec2(1440.0, 900.0)).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services { files: show, ..Default::default() })
    });
    (h, asked)
}

fn field(h: &Harness<'_, PhotocraftApp>, key: &str) -> Option<Value> {
    let ui = &h.state().ui;
    let id = ui.hosted.last().map(|h| h.id).or_else(|| ui.dialogs.last().map(|d| d.id))?;
    ui.dialog_fields(id)?.get(key).cloned()
}

#[test]
fn artboards_to_files_folder_is_chosen_with_browse() {
    let (mut h, asked) = harness(vec![Some(crate::file_picks::scripted::folder("/tmp/exports"))]);
    h.state_mut().run("file.new", json!({"width": 64, "height": 64})).unwrap();
    h.state_mut().sync_views();
    let ctx = h.ctx.clone();
    crate::menus::invoke(h.state_mut(), &ctx, "file.export.artboardsToFiles", json!({})).unwrap();
    h.run_steps(4);
    assert!(field(&h, "dir").is_some(), "the dialog has a Dir field");
    h.get_by_label("Browse…").click();
    h.run_steps(4);
    assert_eq!(crate::file_picks::scripted::kinds(&asked), [craft::host::PickKind::Folder]);
    assert_eq!(field(&h, "dir"), Some(json!("/tmp/exports")));
}

#[test]
fn cancelling_browse_keeps_the_typed_folder() {
    let (mut h, _) = harness(vec![None]);
    h.state_mut().run("file.new", json!({"width": 64, "height": 64})).unwrap();
    h.state_mut().sync_views();
    let ctx = h.ctx.clone();
    crate::menus::invoke(h.state_mut(), &ctx, "file.export.layersToFiles", json!({})).unwrap();
    h.run_steps(4);
    let before = field(&h, "dir");
    h.get_by_label("Browse…").click();
    h.run_steps(4);
    assert_eq!(field(&h, "dir"), before);
}

#[test]
fn install_plugin_path_is_chosen_with_browse() {
    let (mut h, asked) = harness(vec![Some(crate::file_picks::scripted::paths(["/tmp/blur.wasm"]))]);
    let ctx = h.ctx.clone();
    crate::menus::invoke(h.state_mut(), &ctx, "plugin.install", json!({})).unwrap();
    h.run_steps(4);
    h.get_by_label("Browse…").click();
    h.run_steps(4);
    match asked.requests().as_slice() {
        [r] if r.kind == craft::host::PickKind::Open && r.filters.len() == 1 => assert_eq!(r.filters[0].extensions, ["wasm".to_string()]),
        other => panic!("unexpected requests {other:?}"),
    }
    assert_eq!(field(&h, "path"), Some(json!("/tmp/blur.wasm")));
}

/// #9357: Image Processor's source and destination folders are chosen with Browse…, as
/// Photoshop's Select Folder… buttons choose them, instead of being typed.
#[test]
fn image_processor_folders_are_chosen_with_browse() {
    let (mut h, asked) = harness(vec![Some(crate::file_picks::scripted::folder("/tmp/shoot")), Some(crate::file_picks::scripted::folder("/tmp/web"))]);
    let ctx = h.ctx.clone();
    crate::menus::invoke(h.state_mut(), &ctx, "file.scripts.imageProcessor", json!({})).unwrap();
    h.run_steps(4);
    assert_eq!(h.query_all_by_label("Browse…").count(), 2, "one Browse… for the source and one for the destination");
    h.query_all_by_label("Browse…").next().unwrap().click();
    h.run_steps(4);
    assert_eq!(field(&h, "input"), Some(json!("/tmp/shoot")));
    h.query_all_by_label("Browse…").nth(1).unwrap().click();
    h.run_steps(4);
    assert_eq!(field(&h, "output"), Some(json!("/tmp/web")));
    assert_eq!(crate::file_picks::scripted::kinds(&asked), [craft::host::PickKind::Folder; 2]);
}
