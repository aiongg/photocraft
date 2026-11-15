//! #1353: Export As must scale the height even when the rounded width stays unchanged.
//! Drive the real Scale field and Export button, then decode the PNG the writer received.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use craft::host::ScriptedFiles;
use egui_kittest::kittest::Queryable;
use photocraft_ui_egui::control::{ControlRequest, Reply};
use photocraft_ui_egui::{PhotocraftApp, Services, menus};
use serde_json::{Value, json};

type Harness = egui_kittest::Harness<'static, PhotocraftApp>;
type Writes = Rc<RefCell<Vec<(String, Vec<u8>)>>>;

fn harness() -> Option<(Harness, Writes)> {
    let built = std::panic::catch_unwind(|| {
        let writes = Writes::default();
        let captured = writes.clone();
        let h = egui_kittest::Harness::builder().with_size(egui::vec2(1100.0, 760.0)).with_pixels_per_point(1.0).with_max_steps(64).wgpu().build_eframe(
            move |cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                let services = Services {
                    // Save dialogs answer with the suggested name.
                    files: Some(Arc::new(ScriptedFiles::new().saving_suggested())),
                    export: Some(Box::new(|doc, path, settings| {
                        let opts = photocraft_io::ExportOptions {
                            xmp: if settings.xmp_all { photocraft_io::XmpEmbed::All } else { photocraft_io::XmpEmbed::None },
                            ..Default::default()
                        };
                        photocraft_io::export(doc, path, &opts).map(|r| (r.bytes, r.warnings)).map_err(|e| e.to_string())
                    })),
                    write: Some(Box::new(move |path, bytes| {
                        captured.borrow_mut().push((path.to_string(), bytes.to_vec()));
                        Ok(())
                    })),
                    ..Default::default()
                };
                let mut app = PhotocraftApp::with_context(cc.egui_ctx.clone(), photocraft_engine::Session::new(), services);
                app.session.prefs.edit(|p| p.interface.language = "en".into());
                if let Some(rs) = cc.wgpu_render_state.as_ref() {
                    app.set_wgpu(rs.clone());
                }
                app
            },
        );
        (h, writes)
    });
    match built {
        Ok(h) => Some(h),
        Err(_) => {
            eprintln!("skipping: no GPU adapter");
            None
        }
    }
}

/// One control request, as a transport sends it.
fn control(h: &mut Harness, method: &str, params: Value) -> Value {
    let (request, _) = ControlRequest::new(method, params);
    let ctx = h.ctx.clone();
    match photocraft_ui_egui::control::handle(h.state_mut(), &ctx, &request) {
        Reply::Done(value) => value,
        _ => panic!("{method} did not answer at once"),
    }
}

/// Opens Export As and returns its dialog id.
fn open_export_as(h: &mut Harness) -> String {
    let ctx = h.ctx.clone();
    menus::invoke(h.state_mut(), &ctx, "file.export.exportAs", json!({})).expect("open Export As");
    h.run_steps(6);
    h.state().top_dialog().expect("the menu opens a dialog").value().to_string()
}

fn set_field(h: &mut Harness, id: &str, field: &str, value: Value) {
    let reply = control(h, "ui.dialog.set", json!({"dialog": id, "field": field, "value": value}));
    assert!(reply.get("error").is_none() && reply["ok"] != json!(false), "{field}: {reply}");
    h.run_steps(3);
}

/// The open dialog's fields, as `ui.inspect` lists them.
fn dialog_fields(h: &mut Harness, id: &str) -> Value {
    let listed = control(h, "ui.inspect", json!({}));
    // PLAN: assumed the `ui.inspect` entry of a hosted dialog keeps its values under `fields`, as in `variables_data_sets.rs`.
    listed["result"]["dialogs"].as_array().into_iter().flatten().find(|d| d["id"] == json!(id)).map(|d| d["fields"].clone()).expect("the dialog is listed")
}

/// Presses Export… and lets the answered save dialog finish the export.
fn export(h: &mut Harness) {
    h.get_by_label("Export…").click();
    h.run_steps(8);
}

fn evidence(h: &mut Harness, depth: u8, stage: &str) {
    if let Some(dir) = std::env::var_os("PHOTOCRAFT_EXPORT_EVIDENCE_DIR") {
        std::fs::create_dir_all(&dir).expect("evidence directory");
        h.render().expect("render UI").save(std::path::Path::new(&dir).join(format!("{stage}-{depth}bit.png"))).expect("save UI evidence");
    }
}

#[test]
fn export_as_scales_a_narrow_image_without_editing_the_source() {
    let _gpu = photocraft_testkit::gpu_lock();
    let Some((mut h, writes)) = harness() else { return };
    h.run_steps(4);
    for depth in [8, 16, 32] {
        writes.borrow_mut().clear();
        {
            let app = h.state_mut();
            while app.session.active().is_some() {
                app.run("file.close", json!({"discard": true})).expect("close");
            }
            app.run("file.new", json!({"width": 1, "height": 100, "depth": depth, "background": "white"})).expect("new narrow document");
            app.sync_views();
        }
        let st = h.state().session.active().expect("document");
        let source = st.doc.clone();
        let history = st.history.entries();
        let revision = st.revision;

        let id = open_export_as(&mut h);
        set_field(&mut h, &id, "size", json!({"Scale": 50.0}));
        assert!(h.query_by_label("1 × 50 px").is_some(), "{depth}-bit: dialog shows the requested output size");
        evidence(&mut h, depth, "dialog");

        export(&mut h);
        evidence(&mut h, depth, "result");
        let written = writes.borrow();
        assert_eq!(written.len(), 1, "{depth}-bit: Export writes one file");
        let (path, bytes) = &written[0];
        assert!(path.ends_with(".png"), "PNG export path: {path}");
        if let Some(dir) = std::env::var_os("PHOTOCRAFT_EXPORT_EVIDENCE_DIR") {
            std::fs::write(std::path::Path::new(&dir).join(format!("exported-{depth}bit.png")), bytes).expect("save exported PNG evidence");
        }
        let png = photocraft_codecs::decode_as(photocraft_codecs::Format::Png, bytes).expect("decode the written PNG");
        assert_eq!(png.dimensions(), (1, 50), "{depth}-bit: written dimensions match the dialog");

        let st = h.state().session.active().expect("source remains open");
        assert!(Arc::ptr_eq(&st.doc, &source), "{depth}-bit: source document is unchanged");
        assert_eq!((st.doc.size.width, st.doc.size.height), (1, 100));
        assert_eq!(st.history.entries(), history, "export must not add source history");
        assert_eq!(st.revision, revision, "export must not change source revision");
    }
}

/// #5838: Export As › Image Size › Resample. Choose Nearest Neighbor in the real dropdown, export
/// at 200 %, and every source pixel arrives as an exact 2 × 2 block; the choice is remembered.
#[test]
fn export_as_resample_menu_writes_nearest_neighbor_blocks() {
    let _gpu = photocraft_testkit::gpu_lock();
    let Some((mut h, writes)) = harness() else { return };
    h.run_steps(4);
    {
        let app = h.state_mut();
        app.run("file.new", json!({"width": 4, "height": 3, "depth": 8, "background": "white"})).expect("new document");
        app.session
            .edit("checker", |doc, active| {
                let b = doc.bounds();
                let layer = (*active).ok_or(photocraft_engine::EngineError::NoDocument)?;
                let surf = doc.layer_mut(layer).and_then(|l| l.surface_mut()).ok_or(photocraft_engine::EngineError::NoDocument)?;
                let fmt = surf.format();
                let mut data = Vec::new();
                for y in b.y0..b.y1 {
                    for x in b.x0..b.x1 {
                        data.extend(photocraft_raster::from_rgba(&fmt, if (x + y) % 2 == 0 { [1.0, 0.0, 0.0, 1.0] } else { [0.0, 0.0, 1.0, 1.0] }));
                    }
                }
                surf.write_region(b, &data);
                Ok(())
            })
            .expect("paint a checkerboard");
        app.sync_views();
    }
    let source = h.state().session.active().expect("document").doc.clone();
    let id = open_export_as(&mut h);
    // The dialog's own Resample list, as a user picks from it.
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Resample").click();
    h.run_steps(3);
    h.get_by_label("Nearest Neighbor").click();
    h.run_steps(3);
    set_field(&mut h, &id, "size", json!({"Scale": 200.0}));
    assert!(h.query_by_label("8 × 6 px").is_some(), "dialog shows the requested output size");
    evidence(&mut h, 8, "resample-dialog");

    export(&mut h);
    let written = writes.borrow();
    assert_eq!(written.len(), 1, "Export writes one file");
    let png = photocraft_io::import("out.png", &written[0].1).expect("decode the written PNG").document;
    assert_eq!((png.size.width, png.size.height), (8, 6));
    let surf = png.layers[0].surface().expect("pixels");
    for y in 0..6 {
        for x in 0..8 {
            let want = if (x / 2 + y / 2) % 2 == 0 { [1.0, 0.0, 0.0, 1.0] } else { [0.0, 0.0, 1.0, 1.0] };
            assert_eq!(surf.rgba(x, y), want, "pixel {x},{y}");
        }
    }
    let st = h.state().session.active().expect("source remains open");
    assert!(Arc::ptr_eq(&st.doc, &source), "the source document is unchanged");
    drop(written);
    let again = open_export_as(&mut h);
    assert_eq!(dialog_fields(&mut h, &again)["resample"], "nearestNeighbor", "the choice is remembered");
}
