use std::cell::RefCell;
use std::rc::Rc;

use craft::menu::{Backend, Chord, CommandRef, Dispatch, Event, ItemRole, MenuBar, MenuRole, Node};
use egui_kittest::Harness;
use serde_json::json;

use super::*;

type Synced = Rc<RefCell<Vec<MenuBar>>>;
type Events = Rc<RefCell<Vec<Event>>>;

struct Fake {
    synced: Synced,
    events: Events,
}

impl Backend for Fake {
    fn sync(&mut self, bar: &MenuBar) {
        self.synced.borrow_mut().push(bar.clone());
    }
    fn drain(&mut self) -> Vec<Event> {
        std::mem::take(&mut *self.events.borrow_mut())
    }
    fn attach(&mut self, _dispatch: Dispatch) {}
}

fn app(doc: bool) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    if doc {
        app.run("file.new", json!({"width": 64, "height": 48})).unwrap();
        app.sync_views();
    }
    app
}

/// The whole app on its own context, with a Mac menu over a fake backend.
fn harness(doc: bool) -> (Harness<'static, PhotocraftApp>, Synced, Events) {
    let (synced, events): (Synced, Events) = Default::default();
    let backend = Fake { synced: synced.clone(), events: events.clone() };
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::with_context(cc.egui_ctx.clone(), photocraft_engine::Session::new(), crate::Services::default());
        if doc {
            app.run("file.new", json!({"width": 64, "height": 48})).unwrap();
            app.sync_views();
        }
        app.services.native_menu = Some(NativeMenu::new(Box::new(backend)));
        app
    });
    h.run_steps(2);
    (h, synced, events)
}

/// A menu click: the event the backend reports, polled before the frame like the app's input hook does.
fn click(h: &mut Harness<'static, PhotocraftApp>, events: &Events, id: &str) {
    events.borrow_mut().push(Event::Click(CommandRef::new(id)));
    let ctx = h.ctx.clone();
    poll(h.state_mut(), &ctx);
    h.run_steps(3);
}

fn showing(layout: &Layout, key: &str) -> Vec<String> {
    let chord = Chord::parse(key).unwrap();
    layout.bar.items().into_iter().filter(|it| it.shortcut.as_ref() == Some(&chord)).map(|it| it.command.id.clone()).collect()
}

fn laid(app: &mut PhotocraftApp) -> Layout {
    let ctx = app.ctx().clone();
    laid_out(app, &ctx)
}

/// Saved bindings decide which row shows ⌘, ⌘H and ⌘M: an owner outside the menus or an alternate
/// key takes the key from the system item as a menu row does.
#[test]
fn saved_bindings_decide_which_row_shows_the_system_keys() {
    type Pairs = &'static [(&'static str, &'static str)];
    struct Case {
        case: &'static str,
        shortcuts: Pairs,
        alternates: Pairs,
        key: &'static str,
        shown_by: &'static [&'static str],
    }
    let cases = [
        Case {
            case: "Settings saved on ⌘, gives way to Hide Layers",
            shortcuts: &[("edit.preferences.general", "Cmd+,")],
            alternates: &[],
            key: "Cmd+,",
            shown_by: &["layer.hideLayers"],
        },
        Case {
            case: "a command without a row takes ⌘,",
            shortcuts: &[("edit.fillForeground", "Cmd+,"), ("layer.hideLayers", "")],
            alternates: &[],
            key: "Cmd+,",
            shown_by: &[],
        },
        Case { case: "a command without a row takes ⌘H", shortcuts: &[("edit.fillForeground", "Cmd+H")], alternates: &[], key: "Cmd+H", shown_by: &[] },
        Case { case: "a command without a row takes ⌘M", shortcuts: &[("edit.fillForeground", "Cmd+M")], alternates: &[], key: "Cmd+M", shown_by: &[] },
        Case {
            case: "an alternate on a visible command removes Settings' ⌘,",
            shortcuts: &[],
            alternates: &[("view.extras", "Cmd+,")],
            key: "Cmd+,",
            shown_by: &[],
        },
        Case {
            case: "About saved on ⌘, takes it from Settings",
            shortcuts: &[("help.about", "Cmd+,"), ("layer.hideLayers", "")],
            alternates: &[],
            key: "Cmd+,",
            shown_by: &["help.about"],
        },
    ];
    for c in cases {
        let mut a = app(true);
        a.session.prefs.edit(|p| {
            for (id, key) in c.shortcuts {
                p.shortcuts.insert((*id).into(), (*key).into());
            }
            for (id, key) in c.alternates {
                p.alternate_shortcuts.insert((*id).into(), vec![(*key).into()]);
            }
        });
        // The runtime's keymap takes the saved bindings at the start of a frame.
        crate::canvas::testing::frame(&mut a);
        let layout = laid(&mut a);
        assert_eq!(showing(&layout, c.key), c.shown_by, "{}", c.case);
    }
}

/// A Photoshop `.kys` set imported through Edit › Keyboard Shortcuts moves ⌘M to Levels: the
/// system items take the moved chords.
#[test]
fn an_imported_kys_set_owns_the_system_keys_it_binds() {
    const SET: &str = "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<photoshop-keyboard-shortcuts version=\"4\" filename=\"$$$/FileName/Presets/KeyboardCustomization/mine=mine\">
\t<command kind=\"static\" name=\"Levels...\" id=\"1801\"><shortcut>Cmd+M</shortcut></command>
\t<command kind=\"static\" name=\"Hide Photoshop\" id=\"7001\"><shortcut>Cmd+H</shortcut></command>
</photoshop-keyboard-shortcuts>
";
    let mut a = app(true);
    // PLAN: assumed the import still runs through `edit.keyboardShortcuts {importKys}` (top-w20 owns the `ImportPlan` route).
    a.session.execute("edit.keyboardShortcuts", json!({"importKys": SET})).unwrap();
    crate::canvas::testing::frame(&mut a);
    let layout = laid(&mut a);
    assert_eq!(showing(&layout, "Cmd+M"), ["image.adjustments.levels"]);

    assert!(!showing(&layout, "Ctrl+Cmd+M").is_empty(), "Minimize moves to ⌃⌘M");
    assert_eq!(showing(&layout, "Cmd+H"), ["view.extras"], "Hide Photoshop was not imported");
}

/// The ThemeMenu submenu lands in the app menu's Appearance, the current theme checked.
#[test]
fn themes_move_to_the_app_menus_appearance() {
    let mut a = app(false);
    // The menu's checked row is the saved choice, which picking a theme sets.
    let ctx = a.ctx().clone();
    let classic = crate::theme::AppTheme::classic().command();
    crate::menus::invoke(&mut a, &ctx, &classic.id, classic.params.clone()).unwrap();
    let layout = laid(&mut a);
    let app_menu = layout.bar.menu(MenuRole::App).unwrap();
    let appearance = app_menu.children.iter().find(|n| matches!(n, Node::Submenu { role: Some(ItemRole::Appearance), .. }));
    assert!(appearance.is_some(), "an Appearance submenu in the app menu");
    let checked = |c: CommandRef| layout.bar.find(&c).and_then(|it| it.kind.checked());
    assert_eq!(checked(crate::theme::AppTheme::classic().command()), Some(true));
    assert_eq!(checked(crate::theme::AppTheme::pro().command()), Some(false));
    let window = layout.bar.menu(MenuRole::Window).unwrap();
    assert!(window.children.iter().all(|n| !matches!(n, Node::Submenu { role: Some(ItemRole::Appearance), .. })), "Theme left the Window menu");
}

/// A Language row fires `app.language {lang}`: the preference is stored, the old `sr` id still
/// switches, and an unknown code is an error.
#[test]
fn the_app_menu_switches_the_ui_language() {
    // The offered languages are the embedded catalogs'.
    if !crate::i18n::needs_catalogs() {
        return;
    }
    let mut a = app(false);
    let ctx = a.ctx().clone();
    // PLAN: assumed the fired command is `app.language` with `{"lang": code}` (top-w12's `LanguageChoice` handling in `invoke`).
    let set = |a: &mut PhotocraftApp, code: &str| crate::menus::invoke(a, &ctx, "app.language", json!({"lang": code}));
    set(&mut a, "fr").unwrap();
    assert_eq!(a.session.prefs().interface.language, "fr");
    set(&mut a, "sr").unwrap();
    assert_eq!(a.session.prefs().interface.language, "sr-latn", "the old Serbian id still switches");
    set(&mut a, "auto").unwrap();
    assert_eq!(a.session.prefs().interface.language, "auto");
    assert!(set(&mut a, "xx-nope").is_err(), "an unknown code is an error");
}

<<<<<<< HEAD
/// #1354: a dialog is modal for the menu bar too. Its rows are greyed (view navigation aside), and
/// a click that arrives anyway doesn't run under the dialog.
=======
/// The menus show the shortcut that runs each command (Undo ⌘Z, Copy ⌘C were blank).
#[test]
fn menu_rows_show_the_shortcut_that_runs_them() {
    let l = layout(true);
    let sc = |id: &str| l.bar.find(id).and_then(|it| it.shortcut.clone());
    assert_eq!(sc("edit.undo").as_deref(), Some("Cmd+Z"));
    assert_eq!(sc("edit.copy").as_deref(), Some("Cmd+C"));
    assert_eq!(sc("edit.paste").as_deref(), Some("Cmd+V"));
    assert_eq!(sc("layer.hideLayers").as_deref(), Some("Cmd+,"));
    let app = app(true);
    let bindings = crate::shortcut_dispatch::bindings(&app);
    for it in crate::menus::menu_items(&app).iter().filter(|it| it.enabled) {
        let Some(shown) = it.shortcut.as_deref().and_then(crate::shortcuts::parse) else { continue };
        let runs = bindings.iter().find(|(_, b)| *b == shown).map(|(id, _)| id.as_str());
        assert!(runs.is_some(), "{} shows {:?}, which runs nothing", it.id, it.shortcut);
    }
}

/// A native menu can't turn a plain item into a check item in place: opening the first
/// document must not change any item's kind, or the whole menu is rebuilt. Window's list of
/// documents gains a row, which rebuilds only the Window menu.
#[test]
fn opening_a_document_changes_state_not_structure() {
    let _registry = crate::menus::plugin_ui::registry_guard();
    let (a, b) = (crate::menus::menu_items(&app(false)), crate::menus::menu_items(&app(true)));
    let kind = |items: &[MenuItem]| items.iter().map(|i| (i.id.clone(), i.checked.is_some())).collect::<HashMap<_, _>>();
    let (ka, kb) = (kind(&a), kind(&b));
    let flips: Vec<&String> = ka.iter().filter(|(id, k)| kb.get(*id).is_some_and(|o| o != *k)).map(|(id, _)| id).collect();
    assert!(flips.is_empty(), "items that change kind: {flips:?}");
    let (no_doc, one_doc) = (layout(false).bar, layout(true).bar);
    assert_eq!(structure_key(&no_doc), structure_key(&one_doc));
    assert_ne!(window_key(&no_doc), window_key(&one_doc), "the Window menu is rebuilt for its new row");
}

/// #2991: the open documents follow Bring All to Front, where a Mac app lists its windows,
/// the active one checked. Switching documents moves the check without a rebuild.
#[test]
fn the_documents_follow_bring_all_to_front() {
    let _registry = crate::menus::plugin_ui::registry_guard();
    let none = ids(&layout(false).bar.menu(MenuRole::Window).unwrap().children);
    assert_eq!(none.last().map(String::as_str), Some("<BringAllToFront>"));

    let mut a = app(true);
    a.run("file.new", json!({"width": 32, "height": 32})).unwrap();
    a.sync_views();
    let bar = photocraft_layout(&crate::menus::menu_items(&a), Lang::EN, "auto").bar;
    let window = ids(&bar.menu(MenuRole::Window).unwrap().children);
    assert_eq!(window[window.len() - 4..], ["<BringAllToFront>", "---", "window.document.0", "window.document.1"]);
    assert_eq!(bar.find("window.document.1").and_then(|i| i.checked), Some(true));
    assert_eq!(bar.find("window.document.0").and_then(|i| i.checked), Some(false));

    a.session.set_active(0);
    let switched = photocraft_layout(&crate::menus::menu_items(&a), Lang::EN, "auto").bar;
    assert_eq!(switched.find("window.document.0").and_then(|i| i.checked), Some(true));
    assert_eq!((structure_key(&switched), window_key(&switched)), (structure_key(&bar), window_key(&bar)), "a check moves in place");
}

/// The guard those two tests hold keeps a plug-in install out from between their menu builds:
/// an install on another thread doesn't finish until the guard is dropped, whatever is already
/// installed (a reinstall of the same plug-in leaves the menus as they were).
#[test]
fn a_plugin_install_waits_for_a_test_comparing_menu_builds() {
    use std::time::Duration;
    let guard = crate::menus::plugin_ui::registry_guard();
    assert!(matches!(crate::menus::plugin_ui::REGISTRY.try_lock(), Err(std::sync::TryLockError::WouldBlock)), "the guard holds the registry mutex");
    let before = structure_key(&layout(false).bar);
    let (installed, done) = std::sync::mpsc::channel();
    let installer = std::thread::spawn(move || {
        let _registry = crate::menus::plugin_ui::registry_guard();
        photocraft_plugins::registry::install_bytes(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../plugins/tests/fixtures/invert.wasm"))).unwrap();
        installed.send(()).unwrap();
    });
    assert!(done.recv_timeout(Duration::from_millis(200)).is_err(), "the install waits for the guard");
    assert_eq!(structure_key(&layout(false).bar), before, "no plug-in row appeared between the builds");
    drop(guard);
    done.recv_timeout(Duration::from_secs(30)).expect("the install goes ahead once the guard is dropped");
    installer.join().unwrap();
}

#[test]
fn native_key_equivalents_are_command_and_function_keys_only() {
    assert!(Chord::parse("Cmd+B").unwrap().native_ok());
    assert!(Chord::parse("F5").unwrap().native_ok());
    assert!(!Chord::parse("B").unwrap().native_ok());
    assert!(!Chord::parse("Alt+F").unwrap().native_ok());
    assert!(!Chord::parse("Shift+X").unwrap().native_ok());
    assert_eq!(Chord::parse("Ctrl+Cmd+M").unwrap().portable(), "Ctrl+Cmd+M");
    assert_eq!(Chord::parse("Cmd++").unwrap().key, "+");
}

/// Every menu shortcut is written so it parses and turns back into the key press egui gets.
#[test]
fn every_shortcut_becomes_a_key_press() {
    for it in layout(true).bar.items() {
        let Some(sc) = &it.shortcut else { continue };
        let c = Chord::parse(sc).unwrap_or_else(|| panic!("{sc} on {} doesn't parse", it.id));
        assert_eq!(&c.portable(), sc, "{} writes its modifiers in another order", it.id);
        let Some(egui::Event::Key { key, modifiers, .. }) = c.key_event(true) else { panic!("{sc} on {} has no egui key", it.id) };
        let parsed = crate::shortcuts::parse(sc).unwrap();
        assert!(crate::shortcuts::key_matches(&parsed, key, modifiers), "{sc} on {} doesn't match its own key press", it.id);
    }
}

fn frame(ctx: &egui::Context, raw: egui::RawInput, f: impl FnMut(&mut egui::Ui)) {
    let mut out = ctx.run_ui(raw, f);
    out.textures_delta.clear();
}

struct Fake {
    synced: std::rc::Rc<std::cell::RefCell<Vec<MenuBar>>>,
    events: Vec<Event>,
}

impl Backend for Fake {
    fn sync(&mut self, bar: &MenuBar) {
        self.synced.borrow_mut().push(bar.clone());
    }
    fn drain(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}

/// A key equivalent reaches PhotoCraft's own key handling as the key press it was, so the
/// shortcut runs once, through the same rules as without a native menu.
#[test]
fn key_equivalents_run_through_the_normal_shortcut_path() {
    let synced = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut app = app(true);
    let ctx = egui::Context::default();
    let rulers = Chord::parse("Cmd+R").unwrap();
    app.services.native_menu = Some(NativeMenu::new(Box::new(Fake { synced: synced.clone(), events: vec![Event::Key(rulers)] })));
    let before = app.ui.extras.rulers;
    let mut raw = egui::RawInput::default();
    if let Some(m) = app.services.native_menu.as_mut() {
        m.raw_input(&mut raw);
    }
    assert_eq!(raw.events.len(), 1, "the press: winit delivers the release");
    frame(&ctx, raw, |ui| {
        run(&mut app, ui.ctx());
        crate::shortcuts::handle(&mut app, ui.ctx());
        sync(&mut app, ui.ctx());
    });
    assert_eq!(app.ui.extras.rulers, !before, "⌘R toggled View › Rulers once");
    assert_eq!(synced.borrow().len(), 1, "the menu was built");
    // Nothing changed: no rebuild of the rows on the next frame.
    frame(&ctx, egui::RawInput::default(), |ui| sync(&mut app, ui.ctx()));
    assert_eq!(synced.borrow().len(), 1);
}

/// #1638: ⌘V through the Mac menu pasted three layers. AppKit takes the key-down for Edit ›
/// Paste's key equivalent, but winit still delivers the key-up (it forwards key-ups while ⌘ is
/// held). The menu's press plus that real release must paste once, whether the release
/// arrives in a later frame or already sits in the same input as the menu event.
#[test]
fn a_paste_key_equivalent_pastes_once() {
    let release = || egui::Event::Key { key: Key::V, physical_key: Some(Key::V), pressed: false, repeat: false, modifiers: Modifiers::COMMAND };
    for same_frame in [false, true] {
        let mut app = app(true);
        let ctx = egui::Context::default();
        app.run("select.rect", json!({"x": 0, "y": 0, "width": 8, "height": 4})).unwrap();
        app.run("edit.copy", json!({})).unwrap();
        let layers = |app: &PhotocraftApp| app.session.active().unwrap().doc.layer_count();
        let before = layers(&app);
        let paste = Chord::parse("Cmd+V").unwrap();
        app.services.native_menu = Some(NativeMenu::new(Box::new(Fake { synced: Default::default(), events: vec![Event::Key(paste)] })));
        let step = |app: &mut PhotocraftApp, events: Vec<egui::Event>| {
            let mut raw = egui::RawInput { events, ..Default::default() };
            eframe::App::raw_input_hook(app, &ctx, &mut raw);
            frame(&ctx, raw, |ui| {
                run(app, ui.ctx());
                crate::shortcuts::handle(app, ui.ctx());
            });
        };
        if same_frame {
            // A quick tap: the real key-up is already queued when the menu event is drained.
            step(&mut app, vec![egui::Event::ModifiersChanged(Modifiers::COMMAND), release()]);
        } else {
            step(&mut app, vec![egui::Event::ModifiersChanged(Modifiers::COMMAND)]);
            step(&mut app, vec![release()]);
        }
        step(&mut app, vec![egui::Event::ModifiersChanged(Modifiers::NONE)]);
        assert_eq!(layers(&app), before + 1, "one ⌘V pastes one layer (key-up in the same frame: {same_frame})");
    }
}

#[test]
fn clicks_run_the_command_and_resync() {
    let synced = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut app = app(true);
    let ctx = egui::Context::default();
    app.services.native_menu = Some(NativeMenu::new(Box::new(Fake { synced: synced.clone(), events: vec![Event::Click("window.panel.layers".into())] })));
    let before = app.ui.panels.layers;
    let mut raw = egui::RawInput::default();
    if let Some(m) = app.services.native_menu.as_mut() {
        m.raw_input(&mut raw);
    }
    frame(&ctx, raw, |ui| {
        run(&mut app, ui.ctx());
        sync(&mut app, ui.ctx());
    });
    assert_eq!(app.ui.panels.layers, !before);
    let bar = synced.borrow().last().cloned().unwrap();
    assert_eq!(bar.find("window.panel.layers").unwrap().checked, Some(!before));
}

/// #1354: a dialog is modal for the menu bar too. Its rows are disabled (view navigation
/// aside), and a click that arrives anyway doesn't run under the dialog.
>>>>>>> upstream/main
#[test]
fn an_open_dialog_blocks_menu_clicks_except_view_navigation() {
    let (mut h, synced, events) = harness(true);
    let ctx = h.ctx.clone();
    crate::menus::invoke(h.state_mut(), &ctx, "image.imageSize", json!({})).unwrap();
    h.run_steps(2);
    assert!(h.state().dialog_open(), "Image Size opened its dialog");
    let size = |app: &PhotocraftApp| app.session.active().map(|d| (d.doc.size.width, d.doc.size.height));
    let zoom = h.state().ui.views[0].zoom;
    click(&mut h, &events, "image.imageRotation.90cw");
    click(&mut h, &events, "view.zoomIn");
    assert_eq!(size(h.state()), Some((64, 48)), "the rotation didn't run under the dialog");
    assert!(h.state().ui.views[0].zoom > zoom, "Zoom In still works under a dialog");
    let find = |id: &str| synced.borrow().last().and_then(|b| b.find(&CommandRef::new(id)).map(|it| it.enabled));
    assert_eq!(find("image.imageRotation.90cw"), Some(false));
    assert_eq!(find("view.zoomIn"), Some(true));
    // With the dialog closed the rows come back.
    let id = h.state().framework.dialogs.stack().top().unwrap();
    h.state_mut().framework.dialogs.cancel_by_id(&ctx, id);
    h.run_steps(3);
    assert_eq!(find("image.imageRotation.90cw"), Some(true));
}

fn type_layer(app: &mut PhotocraftApp, ctx: &egui::Context) -> photocraft_doc::LayerId {
    let id = photocraft_doc::LayerId(app.run("type.create", json!({"text": "Hello", "size": 12, "x": 4, "y": 24})).unwrap()["layer"].as_u64().unwrap());
    crate::menus::invoke(app, ctx, "type.editText", json!({})).unwrap();
    id
}

fn type_text(app: &PhotocraftApp, id: photocraft_doc::LayerId) -> String {
    match &app.session.active().unwrap().doc.layer(id).unwrap().content {
        photocraft_doc::LayerContent::Text(t) => t.text.clone(),
        _ => panic!("type layer changed kind"),
    }
}

/// Edit › Cut while editing type cuts the editor's selection (#1355): the click is fired `edit.cut`
/// and the focused editor takes it.
#[test]
fn cut_selected_type_from_native_menu_matches_keyboard_cut() {
    let (mut h, synced, events) = harness(true);
    let ctx = h.ctx.clone();
    let id = type_layer(h.state_mut(), &ctx);
    h.run_steps(2);
    let edit = h.state().ui.text_edit.as_ref().unwrap();
    assert_eq!((edit.anchor, edit.caret), (0, 5));
    assert_eq!(synced.borrow().last().unwrap().find(&CommandRef::new("edit.cut")).map(|it| it.enabled), Some(true), "Cut is enabled for selected characters");
    click(&mut h, &events, "edit.cut");
    assert!(type_text(h.state(), id).is_empty());
    let edit = h.state().ui.text_edit.as_ref().unwrap();
    assert_eq!((edit.anchor, edit.caret), (0, 0));
    assert!(h.state().session.clipboard.is_none(), "text Cut must not copy rendered pixels");
    assert_eq!(synced.borrow().last().unwrap().find(&CommandRef::new("edit.cut")).map(|it| it.enabled), Some(false), "Cut is disabled at an empty caret");
}

/// Select › All, Edit › Copy and Edit › Paste work on the text being typed, not on pixels (#2893).
#[test]
fn copy_and_paste_type_from_native_menu() {
    let (mut h, synced, events) = harness(true);
    let ctx = h.ctx.clone();
    let id = type_layer(h.state_mut(), &ctx);
    if let Some(e) = h.state_mut().ui.text_edit.as_mut() {
        (e.anchor, e.caret) = (5, 5);
    }
    let layers = h.state().session.active().unwrap().doc.layers.len();
    click(&mut h, &events, "select.all");
    let edit = h.state().ui.text_edit.as_ref().unwrap();
    assert_eq!((edit.anchor, edit.caret), (0, 5), "Select › All selects the characters");
    let enabled = |c: &str| synced.borrow().last().unwrap().find(&CommandRef::new(c)).map(|it| it.enabled);
    assert_eq!((enabled("edit.copy"), enabled("edit.paste")), (Some(true), Some(true)));
    click(&mut h, &events, "edit.copy");
    assert!(h.state().session.clipboard.is_none(), "text Copy must not copy rendered pixels");
    // The window answers Paste's request with the clipboard text; it replaces the selection.
    h.event(egui::Event::Paste("World".into()));
    h.run_steps(3);
    assert_eq!(type_text(h.state(), id), "World");
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), layers, "no pixel layer was pasted");
    assert!(!h.state().ui.status_error, "{}", h.state().ui.status);
}

/// Without a type edit, Cut is the pixel clipboard command.
#[test]
fn native_cut_without_type_edit_keeps_pixel_clipboard_behavior() {
    let (mut h, synced, events) = harness(false);
    h.state_mut().run("file.new", json!({"width": 4, "height": 4, "background": "transparent"})).unwrap();
    h.state_mut().run("edit.fill", json!({"color": "#ff0000"})).unwrap();
    h.run_steps(2);
    assert_eq!(synced.borrow().last().unwrap().find(&CommandRef::new("edit.cut")).map(|it| it.enabled), Some(true));
    click(&mut h, &events, "edit.cut");
    assert_eq!(h.state_mut().run("document.pixel", json!({"x": 1, "y": 1})).unwrap(), json!([0.0, 0.0, 0.0, 0.0]));
    let clip = h.state().session.clipboard.as_ref().unwrap();
    assert_eq!((clip.bounds.width(), clip.bounds.height()), (4, 4));
    let mut px = vec![[0u8; 4]; 16];
    clip.surface.read_rgba8_into(clip.bounds, &mut px);
    assert_eq!(px, vec![[255u8, 0, 0, 255]; 16]);
    assert!(h.state().ui.text_edit.is_none());
}

#[test]
fn opening_or_closing_a_workspace_without_input_rebuilds_the_menus() {
    let mut app = app(true);
    let ctx = app.ctx().clone();
    app.run("layer.new.layer", json!({})).unwrap();
    app.run("edit.fill", json!({"color": "#808080"})).unwrap();
    let closed = state_hash(&app, None);
    crate::gallery_ui::open(&mut app).unwrap();
    let open = state_hash(&app, None);
    assert_ne!(open, closed, "the menus grey out when an agent opens Filter Gallery");
    let id = app.framework.dialogs.stack().top().unwrap();
    app.framework.dialogs.cancel_by_id(&ctx, id);
    assert_eq!(state_hash(&app, None), closed, "and come back when it is cancelled");
}
