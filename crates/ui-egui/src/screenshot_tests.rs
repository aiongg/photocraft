//! `ui.screenshot` through the app: craft-automation's capture queue driven by PhotoCraft's logic
//! and UI phases, the geometry the runtime reports, and the automation write authority. The
//! queue's own rules (budgets, settling, deadlines) are craft-automation's and tested there.

use craft::automation::screenshot::{CaptureGeometry, Captures, DocumentId};
use craft::ui::runtime::motion;
use egui::{Color32, ColorImage, Context, ViewportId, vec2};
use std::sync::Arc;

fn captures(app: &crate::PhotocraftApp) -> &Captures {
    &app.control.as_ref().expect("a control channel").captures
}

fn captures_mut(app: &mut crate::PhotocraftApp) -> &mut Captures {
    &mut app.control.as_mut().expect("a control channel").captures
}

/// Execute the same logic→UI order as eframe, with genuinely new host geometry before logic.
fn actual_capture_frame(app: &mut crate::PhotocraftApp, ctx: &Context, size: egui::Vec2, scale: f32, minimized: bool) -> egui::FullOutput {
    use eframe::App as _;
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
    let mut viewports = egui::ViewportIdMap::default();
    viewports.insert(
        ViewportId::ROOT,
        egui::ViewportInfo { inner_rect: Some(rect), native_pixels_per_point: Some(scale), minimized: Some(minimized), ..Default::default() },
    );
    let mut input = egui::RawInput { screen_rect: Some(rect), viewports, focused: true, ..Default::default() };
    app.raw_input_hook(ctx, &mut input);
    let mut frame = eframe::Frame::_new_kittest();
    let mut output = ctx.run_ui(input, |ui| {
        app.logic(ui.ctx(), &mut frame);
        app.ui(ui, &mut frame);
    });
    output.textures_delta.clear();
    output
}

fn actual_callback(output: &egui::FullOutput) -> Option<egui::ScreenshotCallback> {
    output.viewport_output.values().flat_map(|viewport| &viewport.commands).find_map(|command| match command {
        egui::ViewportCommand::Screenshot(callback) => Some(callback.clone()),
        _ => None,
    })
}

type ScreenshotWrites = std::rc::Rc<std::cell::RefCell<Vec<(String, Vec<u8>)>>>;

struct ActualCallbackFixture {
    app: crate::PhotocraftApp,
    ctx: Context,
    writes: ScreenshotWrites,
    reply: std::sync::mpsc::Receiver<serde_json::Value>,
    document: Option<DocumentId>,
    old_geometry: CaptureGeometry,
    old_pixels: std::sync::Weak<ColorImage>,
}

fn actual_callback_fixture() -> ActualCallbackFixture {
    use std::cell::RefCell;
    use std::rc::Rc;

    let writes = Rc::new(RefCell::new(Vec::new()));
    let written = writes.clone();
    let services = crate::Services {
        automation_write: Some(Box::new(move |path, bytes| {
            written.borrow_mut().push((path.to_owned(), bytes.to_vec()));
            Ok(())
        })),
        ..Default::default()
    };
    let mut app = crate::PhotocraftApp::new(photocraft_engine::Session::new(), services).with_control(std::sync::mpsc::channel().1);
    app.run("file.new", serde_json::json!({"width": 32, "height": 32})).unwrap();
    let ctx = Context::default();
    // `raw_input_hook` hands the craft runtime its frame input, as on the app's context.
    crate::PhotocraftApp::setup_context(&ctx, Default::default());
    for _ in 0..4 {
        actual_capture_frame(&mut app, &ctx, vec2(600.0, 400.0), 1.0, false);
    }
    let old_geometry = captures(&app).last_geometry().expect("the old real UI geometry");
    let document = app.document_identity();
    let (tx, reply) = std::sync::mpsc::channel();
    captures_mut(&mut app).admit(91, Some("fixture/current.png".into()), tx, document, 0.0, motion::now()).unwrap();
    let mut callback = None;
    for _ in 0..8 {
        let output = actual_capture_frame(&mut app, &ctx, vec2(600.0, 400.0), 1.0, false);
        if let Some(found) = actual_callback(&output) {
            callback = Some(found);
            break;
        }
    }
    let old_image = Arc::new(ColorImage::filled(old_geometry.expected_pixels().unwrap(), Color32::RED));
    let old_pixels = Arc::downgrade(&old_image);
    callback.expect("actual app issued one screenshot callback").complete(old_image);
    assert!(writes.borrow().is_empty(), "the real callback only stores pixels");
    ActualCallbackFixture { app, ctx, writes, reply, document, old_geometry, old_pixels }
}

#[test]
fn completed_callback_revalidates_next_raw_input_before_actual_app_writes() {
    for (size, scale, minimized) in
        [(vec2(600.0, 400.0), 1.0, false), (vec2(640.0, 400.0), 1.0, false), (vec2(600.0, 400.0), 2.0, false), (vec2(600.0, 400.0), 1.0, true)]
    {
        let mut fixture = actual_callback_fixture();
        if size == vec2(600.0, 400.0) && scale == 1.0 && !minimized {
            verify_unchanged_capture(&mut fixture);
        } else {
            verify_changed_capture(&mut fixture, size, scale, minimized);
        }
    }
}

fn verify_unchanged_capture(fixture: &mut ActualCallbackFixture) {
    actual_capture_frame(&mut fixture.app, &fixture.ctx, vec2(600.0, 400.0), 1.0, false);
    assert_successful_capture(fixture, vec2(600.0, 400.0), 1.0, false);
}

fn verify_changed_capture(fixture: &mut ActualCallbackFixture, size: egui::Vec2, scale: f32, minimized: bool) {
    let output = actual_capture_frame(&mut fixture.app, &fixture.ctx, size, scale, minimized);
    assert!(fixture.writes.borrow().is_empty(), "no stale write on the first changed input");
    assert!(fixture.old_pixels.upgrade().is_none(), "old pixels are released on geometry rejection");
    let current = captures(&fixture.app).last_geometry().unwrap();
    assert!(
        current.size_points != fixture.old_geometry.size_points
            || current.pixels_per_point != fixture.old_geometry.pixels_per_point
            || current.minimized != fixture.old_geometry.minimized
    );
    if minimized {
        verify_minimized_capture(fixture, size, scale, &output);
        return;
    }
    assert!(fixture.reply.try_recv().is_err(), "bounded retry has not delivered a stale success");
    let mut retry = actual_callback(&output);
    for _ in 0..8 {
        if retry.is_some() {
            break;
        }
        retry = actual_callback(&actual_capture_frame(&mut fixture.app, &fixture.ctx, size, scale, minimized));
    }
    retry
        .expect("the actual app reissues at bounded current geometry")
        .complete(Arc::new(ColorImage::filled(current.expected_pixels().unwrap(), Color32::BLUE)));
    actual_capture_frame(&mut fixture.app, &fixture.ctx, size, scale, minimized);
    assert_successful_capture(fixture, size, scale, minimized);
}

fn verify_minimized_capture(fixture: &mut ActualCallbackFixture, size: egui::Vec2, scale: f32, output: &egui::FullOutput) {
    let reply = fixture.reply.try_recv().expect("one minimized error");
    assert_eq!(reply["ok"], false, "{reply}");
    assert!(reply["error"].as_str().unwrap().contains("minimized"));
    assert_eq!(captures(&fixture.app).pending(), 0);
    assert_eq!(captures(&fixture.app).reserved_bytes(), 0);
    assert!(actual_callback(output).is_none());
    actual_capture_frame(&mut fixture.app, &fixture.ctx, size, scale, false);
    let (tx, restored_rx) = std::sync::mpsc::channel();
    captures_mut(&mut fixture.app).admit(92, Some("fixture/restored.png".into()), tx, fixture.document, 0.0, motion::now()).unwrap();
    for _ in 0..8 {
        let output = actual_capture_frame(&mut fixture.app, &fixture.ctx, size, scale, false);
        if let Some(callback) = actual_callback(&output) {
            callback.complete(Arc::new(ColorImage::filled(captures(&fixture.app).last_geometry().unwrap().expected_pixels().unwrap(), Color32::BLUE)));
            break;
        }
    }
    actual_capture_frame(&mut fixture.app, &fixture.ctx, size, scale, false);
    let reply = restored_rx.try_recv().expect("the restored window accepts a new request");
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(fixture.writes.borrow().len(), 1);
    assert!(fixture.writes.borrow()[0].1.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(captures(&fixture.app).pending(), 0);
    assert_eq!(captures(&fixture.app).reserved_bytes(), 0);
    assert!(fixture.reply.try_recv().is_err() && restored_rx.try_recv().is_err());
}

fn assert_successful_capture(fixture: &mut ActualCallbackFixture, size: egui::Vec2, scale: f32, minimized: bool) {
    let reply = fixture.reply.try_recv().expect("one response after the valid callback");
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(fixture.writes.borrow().len(), 1);
    assert!(fixture.writes.borrow()[0].1.starts_with(b"\x89PNG\r\n\x1a\n"), "real PNG encoding and automation_write ran");
    assert_eq!(captures(&fixture.app).pending(), 0);
    assert_eq!(captures(&fixture.app).reserved_bytes(), 0);
    actual_capture_frame(&mut fixture.app, &fixture.ctx, size, scale, minimized);
    assert_eq!(fixture.writes.borrow().len(), 1, "no duplicate output");
    assert!(fixture.reply.try_recv().is_err());
}
