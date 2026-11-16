//! Ctrl+/ (Cmd+/ on the Mac) opens Layer > Lock Layers..., the same dialog the Layer menu and the Layers panel menu
//! open. Plain / still toggles Lock Transparent Pixels (#11397).

use egui::{Key, Modifiers};
use serde_json::json;

use crate::PhotocraftApp;

fn app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 20, "height": 10})).unwrap();
    app.run("layer.new.layer", json!({})).unwrap();
    app
}

/// The key press and release through the window host, with the app's own frames routing it.
fn press(app: &mut PhotocraftApp, key: Key, modifiers: Modifiers) {
    crate::canvas::testing::key(app, key, modifiers);
}

fn locks(app: &PhotocraftApp) -> photocraft_doc::Locks {
    let st = app.session.active().unwrap();
    st.doc.layer(st.active_layer.unwrap()).unwrap().locks
}

/// OK in the open Lock Layers dialog.
fn confirm_dialog(app: &mut PhotocraftApp) {
    let ctx = app.ctx().clone();
    let id = app.framework.dialogs.stack().top().expect("the Lock Layers dialog is open");
    crate::dialogs::confirm(app, &ctx, id).unwrap();
}

#[test]
fn command_slash_runs_what_the_menu_item_runs() {
    let mut via_key = app();
    press(&mut via_key, Key::Slash, Modifiers::COMMAND);
    let mut via_menu = app();
    crate::menus::invoke(&mut via_menu, &egui::Context::default(), "layer.lockLayers", json!({})).unwrap();
    for app in [&mut via_key, &mut via_menu] {
        assert!(app.dialog_open(), "Lock Layers… asks first");
        assert!(!locks(app).any(), "nothing is locked before OK");
        confirm_dialog(app);
    }
    assert_eq!(locks(&via_key), locks(&via_menu));
    assert!(locks(&via_key).all, "Lock Layers... with no choices locks the layer");
}

#[test]
fn command_slash_does_nothing_when_the_menu_item_is_greyed() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    assert!(!crate::menus::is_enabled(&app, "layer.lockLayers"));
    press(&mut app, Key::Slash, Modifiers::COMMAND);
    assert!(app.session.active().is_none());
    assert!(!app.dialog_open());
}

#[test]
fn plain_slash_toggles_lock_transparency_and_command_slash_does_not() {
    let mut app = app();
    press(&mut app, Key::Slash, Modifiers::NONE);
    assert!(locks(&app).transparency && !locks(&app).all);
    press(&mut app, Key::Slash, Modifiers::NONE);
    assert!(!locks(&app).transparency);
    press(&mut app, Key::Slash, Modifiers::COMMAND);
    confirm_dialog(&mut app);
    assert!(locks(&app).all);
    assert!(!locks(&app).transparency, "Ctrl+/ leaves Lock Transparent Pixels alone");
}
