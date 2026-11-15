//! Stamp Down (#217) through the real key handling: ⌘⌥E stamps into the layer below.

use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::Harness;
use serde_json::json;

use crate::PhotocraftApp;

fn harness() -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 120, "height": 80})).unwrap();
    app.run("layer.new.layer", json!({"name": "Paint"})).unwrap();
    app.run("edit.fill", json!({"color": "#3366cc"})).unwrap();
    app.sync_views();
    let mut h = Harness::builder().with_size(vec2(900.0, 600.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            let ctx = ui.ctx().clone();
            if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            crate::shortcuts::handle(app, &ctx);
            egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::AppTheme::pro());
    h.run_steps(4);
    h
}

/// ⌘ as the platform reports it (⌘ on the Mac, Ctrl elsewhere).
fn platform(m: Modifiers) -> Modifiers {
    let mut out = Modifiers { alt: m.alt, shift: m.shift, ctrl: m.ctrl, ..Default::default() };
    if m.command {
        out.command = true;
        if cfg!(target_os = "macos") {
            out.mac_cmd = true;
        } else {
            out.ctrl = true;
        }
    }
    out
}

fn tap(h: &mut Harness<'static, PhotocraftApp>, k: Key, m: Modifiers) {
    let m = platform(m);
    h.event(Event::ModifiersChanged(m));
    for pressed in [true, false] {
        h.event(Event::Key { key: k, physical_key: None, pressed, repeat: false, modifiers: m });
        h.run_steps(1);
    }
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(1);
}

fn layers(h: &Harness<'static, PhotocraftApp>) -> usize {
    h.state().session.active().unwrap().doc.walk().len()
}

#[test]
fn cmd_alt_e_stamps_down() {
    let mut h = harness();
    // Stamp Down into the Background: pixels change there, no layer is added.
    tap(&mut h, Key::E, Modifiers { alt: true, command: true, ..Default::default() });
    assert_eq!(layers(&h), 2);
    assert!(h.state().session.journal.iter().any(|(id, _)| id == "layer.stampDown"));
    let st = h.state().session.active().unwrap();
    let px = st.doc.layers[0].surface().unwrap().rgba(5, 5);
    assert!((px[2] - 0.8).abs() < 0.01, "the copy was merged into the Background: {px:?}");
    // On the Background there is nothing below: a status message, no crash.
    let bg = h.state().session.active().unwrap().doc.layers[0].id.0;
    h.state_mut().run("layer.select", json!({"layer": bg})).unwrap();
    tap(&mut h, Key::E, Modifiers { alt: true, command: true, ..Default::default() });
    assert!(h.state().ui.status.contains("below"), "{}", h.state().ui.status);
}
