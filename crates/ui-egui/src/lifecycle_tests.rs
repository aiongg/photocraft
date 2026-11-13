//! Synthetic input around the app's passes: input queued for one active document is withdrawn
//! when another document becomes active, not on an edit, and a hidden window keeps painting while
//! input or a capture waits. The queue itself is the window host's (craft-automation's
//! `synthetic` trials).

use crate::{PhotocraftApp, Services};
use craft::ui::runtime::windows::WindowHost;
use eframe::App as _;
use egui::{Context, RawInput};
use photocraft_engine::Session;
use serde_json::json;

fn app_with_doc() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(Session::new(), Services::default());
    app.run("file.new", json!({"width": 32, "height": 32})).expect("a document");
    app
}

/// Sends `ui.key V` through a control channel and runs it: the reply waits on the queued input.
fn queue_key(app: &mut PhotocraftApp, ctx: &Context) -> std::sync::mpsc::Receiver<serde_json::Value> {
    let (transport, rx) = std::sync::mpsc::channel();
    app.control = Some(craft::automation::ControlChannel::new(rx));
    let (req, reply) = crate::ControlRequest::new("ui.key", json!({"key": "V"}));
    transport.send(req).expect("the channel is open");
    app.drain_control(ctx);
    assert!(!WindowHost::of(ctx).injected_idle(), "the key is queued");
    reply
}

/// Hidden logic ticks still receive the current viewport's native geometry. The default ROOT
/// entry has no geometry, so it cannot represent a window with a capturable size.
fn host_input(viewport: egui::ViewportId) -> RawInput {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0));
    let mut raw = RawInput { viewport_id: viewport, screen_rect: Some(rect), focused: true, ..Default::default() };
    raw.viewports
        .insert(viewport, egui::ViewportInfo { inner_rect: Some(rect), native_pixels_per_point: Some(1.0), minimized: Some(false), ..Default::default() });
    raw
}

/// A switch of the active document before the queued input ran withdraws it and fails the
/// waiting reply; an edit in the same document (a new revision, the same activation) does not.
#[test]
fn queued_input_is_withdrawn_only_when_the_active_document_changes() {
    for (command, params, withdrawn) in [("file.new", json!({"width": 16, "height": 16}), true), ("layer.new.layer", json!({"name": "queued"}), false)] {
        let ctx = Context::default();
        let mut app = app_with_doc();
        let reply = queue_key(&mut app, &ctx);
        app.run(command, params).expect(command);
        app.withdraw_stale_input(&ctx);
        app.poll_input(&ctx);
        let host = WindowHost::of(&ctx);
        if withdrawn {
            let reply = reply.try_recv().expect("the waiting request is answered");
            assert_eq!(reply["ok"], false, "{command}");
            assert!(reply["error"].as_str().is_some_and(|e| e.contains("document or focus changed")), "{reply}");
            assert!(host.injected_idle(), "nothing is left for the new document");
        } else {
            assert!(reply.try_recv().is_err(), "{command}: no cancellation reply");
            assert!(!host.injected_idle(), "{command}: the input stays queued");
        }
    }
}

/// A hidden host runs `logic` without a UI pass. Both a capture that is waiting for its first
/// geometry and queued synthetic input must ask the host for a paint, or the window stays
/// hidden forever and the control request times out.
#[test]
fn a_logic_tick_requests_a_hidden_paint_for_waiting_captures_and_queued_input() {
    let paints = |out: &egui::LogicOutput| {
        out.viewport_commands.get(&egui::ViewportId::ROOT).is_some_and(|commands| commands.contains(&egui::ViewportCommand::RequestPaintWhileHidden))
    };
    let mut frame = eframe::Frame::_new_kittest();

    let ctx = Context::default();
    let mut app = app_with_doc();
    let (tx, _rx) = std::sync::mpsc::channel();
    let document = app.document_identity();
    let (_transport, requests) = std::sync::mpsc::channel();
    let mut channel = craft::automation::ControlChannel::new(requests);
    channel.captures.admit(90, None, tx, document, 0.0, craft::ui::runtime::motion::now()).expect("parked before the first pass");
    app.control = Some(channel);
    let out = ctx.run_logic(&host_input(egui::ViewportId::ROOT), |ctx| app.logic(ctx, &mut frame));
    assert!(paints(&out), "a capture waiting for geometry keeps the hidden window painting");

    let ctx = Context::default();
    let mut app = app_with_doc();
    let _reply = queue_key(&mut app, &ctx);
    let out = ctx.run_logic(&host_input(egui::ViewportId::ROOT), |ctx| app.logic(ctx, &mut frame));
    assert!(paints(&out), "queued synthetic input keeps the hidden window painting");
}
