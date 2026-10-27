//! The suite theme in PhotoCraft's UI: Preferences › Interface cards, radios and notices,
//! Window › Theme and the title-bar mode button. Suite folders are in memory or temporary.

use super::*;
use craft_theme::{AppChoice, Mode, Selection, ThemeId};
use craft_theme_store::{Op, SelectionChange, SuiteWatch};
use egui_kittest::{Harness, kittest::Queryable};
use std::sync::{Arc, Mutex};

type Store = Arc<Mutex<Option<String>>>;

/// A fresh install whose suite folder is `dir` (shared, as on the desktop) or in memory.
fn app(dir: Option<&std::path::Path>) -> PhotocraftApp {
    let store: Store = Arc::default();
    let (a, b) = (store.clone(), store);
    let suite_watch: Option<crate::theme::suite::OpenWatchFn> = dir.map(|dir| {
        let dir = craft_theme_store::SuiteDir::at(dir);
        Box::new(move |options| SuiteWatch::open(&dir, options, || {})) as crate::theme::suite::OpenWatchFn
    });
    let services = crate::Services {
        load_prefs: Some(Box::new(move || a.lock().unwrap().clone())),
        save_prefs: Some(Box::new(move |s: &str| {
            *b.lock().unwrap() = Some(s.to_string());
            Ok(())
        })),
        suite_watch,
        ..Default::default()
    };
    PhotocraftApp::new(photocraft_engine::Session::new(), services)
}

fn harness<'a>(mut app: PhotocraftApp) -> Harness<'a, PhotocraftApp> {
    open_preferences(&mut app, "interface");
    let mut h = Harness::builder().with_size(vec2(1280.0, 900.0)).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        app
    });
    h.run_steps(4);
    h
}

/// Frames until `done`: a shared suite folder is read and written on the watcher's thread.
fn wait_for(h: &mut Harness<'_, PhotocraftApp>, what: &str, done: impl Fn(&PhotocraftApp) -> bool) {
    for _ in 0..250 {
        h.run_steps(1);
        if done(h.state()) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("timed out waiting for {what}");
}

fn id(s: &str) -> ThemeId {
    ThemeId::parse(s).unwrap()
}

fn suite_theme(app: &PhotocraftApp) -> Option<ThemeId> {
    app.suite.shown().theme
}

fn own(theme: &str, mode: Mode) -> AppChoice {
    AppChoice::Own(Selection { theme: id(theme), mode })
}

#[test]
fn a_suite_card_writes_the_suite_when_following_and_the_app_choice_otherwise() {
    let folder = tempfile::tempdir().unwrap();
    let mut h = harness(app(Some(folder.path())));
    assert_eq!(h.state().session.prefs().interface.app_choice(), AppChoice::FollowSuite);
    assert!(h.query_by_label("Suite theme").is_some());
    // Following: the card writes the suite at once. It is shared state, so Cancel keeps it.
    h.get_by_label("Graphite").click();
    h.run_steps(2);
    assert_eq!(suite_theme(h.state()), Some(id("craft.graphite")));
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert_eq!(suite_theme(h.state()), Some(id("craft.graphite")));
    wait_for(&mut h, "the suite selection written", |app| !app.suite.client().is_pending());
    assert!(folder.path().join("selection.json").exists());
    // The per-app choice: the card goes into the draft and applies on OK; the suite stays.
    open_preferences(h.state_mut(), "interface");
    h.run_steps(4);
    h.get_by_label("Use a different theme in this app").click();
    h.run_steps(2);
    assert!(h.query_by_label("Theme").is_some() && h.query_by_label("PhotoCraft themes").is_some());
    h.get_by_label("High contrast").click();
    h.run_steps(2);
    assert_eq!(h.state().session.prefs().interface.app_choice(), AppChoice::FollowSuite, "not before OK");
    h.get_by_label("OK").click();
    h.run_steps(2);
    assert_eq!(h.state().session.prefs().interface.app_choice(), own("craft.high-contrast", Mode::Dark));
    assert_eq!(suite_theme(h.state()), Some(id("craft.graphite")));
}

#[test]
fn the_photocraft_themes_card_shows_the_swatch_rows_that_set_photocrafts_own_themes() {
    let mut h = harness(app(None));
    // In memory there is no suite to follow: the per-app choice is the only one, ArtCraft at first.
    assert_eq!(h.state().session.prefs().interface.app_choice(), own("craft.artcraft", Mode::Dark));
    assert!(h.query_by_label("Follow suite").is_none() && h.query_by_label("Use a different theme in this app").is_none());
    assert!(h.query_by_label("Dark Theme").is_none(), "the swatch rows belong to the PhotoCraft themes card");
    h.get_by_label("PhotoCraft themes").click();
    h.run_steps(2);
    h.get_by_label("Classic").click();
    h.run_steps(2);
    h.get_by_label("Studio").click();
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(2);
    let interface = &h.state().session.prefs().interface;
    assert_eq!(interface.app_choice(), own(craft_theme::APP_DEFAULT, Mode::Dark));
    assert_eq!((interface.light_theme, interface.dark_theme), (LightTheme::Classic, DarkTheme::Studio));
    assert_eq!(h.state().ui.theme, ThemeKind::Studio);
}

#[test]
fn the_mode_button_and_menu_change_the_mode_of_the_choice_that_applies() {
    let ctx = egui::Context::default();
    let mut app = app(None);
    choose_suite_appearance(&mut app, &ctx, Some(true), None);
    let mode = |app: &PhotocraftApp| (app.suite.shown().mode, app.session.prefs().interface.appearance_mode);
    // Following: the suite's mode, starting from PhotoCraft's default (Dark).
    cycle_appearance(&mut app, &ctx);
    assert_eq!(mode(&app), (Some(Mode::System), AppearanceMode::Dark));
    cycle_appearance(&mut app, &ctx);
    assert_eq!(mode(&app), (Some(Mode::Light), AppearanceMode::Dark));
    assert_eq!(effective_selection(&app).mode, Mode::Light);
    // The per-app choice: `appearanceMode`.
    choose_suite_appearance(&mut app, &ctx, Some(false), None);
    cycle_appearance(&mut app, &ctx);
    assert_eq!(mode(&app), (Some(Mode::Light), AppearanceMode::Auto));
    cycle_appearance(&mut app, &ctx);
    assert_eq!(mode(&app), (Some(Mode::Light), AppearanceMode::Light));
    // Window › Theme › Sync with system, in both.
    crate::menus::invoke(&mut app, &ctx, "window.theme.system", json!({})).unwrap();
    assert_eq!(mode(&app), (Some(Mode::Light), AppearanceMode::Auto));
    choose_suite_appearance(&mut app, &ctx, Some(true), None);
    crate::menus::invoke(&mut app, &ctx, "window.theme.system", json!({})).unwrap();
    assert_eq!(mode(&app), (Some(Mode::System), AppearanceMode::Auto));
}

/// A library theme whose dark text is nearly the colour of its surface.
const MURKY: &str = r##"{
  "schema": "craft-theme", "schema_version": 1, "id": "test.murky", "name": "Murky", "version": 1,
  "dark": {
    "background": "#0F1424", "surface": "#161C30", "raised": "#1E2540",
    "text": "#2A3048", "text_muted": "#C4BCA8", "line": "#2C3452",
    "accent": "#E8C27A", "on_accent": "#1B1606", "selection": "#2F3A63",
    "focus": "#E8C27A", "good": "#7CC47F", "warn": "#E0B34A", "bad": "#F08A80"
  }
}"##;

#[test]
fn a_low_contrast_library_theme_shows_its_warning_and_keeps_its_colours() {
    let app = app(None);
    app.suite.watch().request(Op::Import { bytes: Arc::from(MURKY.as_bytes()), as_copy: false });
    let mut h = harness(app);
    h.run_steps(2);
    assert!(h.query_by_label_contains("has low contrast").is_none(), "only for the chosen theme");
    h.get_by_label("Murky").click();
    h.run_steps(2);
    assert!(h.query_by_label_contains("Murky has low contrast: text on").is_some());
    h.get_by_label("OK").click();
    h.run_steps(4);
    assert_eq!(h.state().session.prefs().interface.app_choice(), own("test.murky", Mode::Dark));
    let tokens = crate::theme::Tokens::get(&h.ctx);
    assert_eq!(tokens.text, Color32::from_rgb(0x2A, 0x30, 0x48), "kept as the file has it");
}

#[test]
fn a_conflict_shows_its_notice_and_apply_mine_resolves_it() {
    let folder = tempfile::tempdir().unwrap();
    let mut h = harness(app(Some(folder.path())));
    // Another app writes the suite theme; this one picks before it has seen that write.
    let base = h.state().suite.accepted().clone();
    h.state().suite.watch().request(Op::Update { base, change: SelectionChange::theme(id("craft.high-contrast")) });
    h.state_mut().suite.set_theme(id("craft.graphite"));
    wait_for(&mut h, "the conflict", |app| app.suite.conflict().is_some());
    h.run_steps(2);
    assert!(h.query_by_label("Another app changed the suite theme to High contrast.").is_some());
    h.get_by_label("Apply mine").click();
    wait_for(&mut h, "my choice applied", |app| {
        app.suite.conflict().is_none() && !app.suite.client().is_pending() && app.suite.accepted().suite.theme == Some(id("craft.graphite"))
    });
    h.run_steps(2);
    assert!(h.query_by_label_contains("Another app changed").is_none());
}

#[test]
fn window_theme_lists_the_suite_themes_and_hides_follow_suite_in_memory() {
    let ctx = egui::Context::default();
    let ids = |app: &PhotocraftApp| {
        crate::menus::menu_items(app).into_iter().filter(|i| i.path.starts_with(&["Window".into(), "Theme".into()])).map(|i| i.id).collect::<Vec<_>>()
    };
    let checked = |app: &PhotocraftApp, wanted: &str| crate::menus::menu_items(app).into_iter().find(|i| i.id == wanted).and_then(|i| i.checked);
    let mut memory = app(None);
    let listed = ids(&memory);
    assert!(!listed.iter().any(|i| i == crate::menus::FOLLOW_SUITE_ITEM));
    let suite = ["window.theme.suite.craft.artcraft", "window.theme.suite.craft.graphite", "window.theme.suite.craft.high-contrast"];
    assert_eq!(listed[..4], [suite[0], suite[1], suite[2], "---"]);
    assert_eq!(checked(&memory, suite[0]), Some(true));
    // In memory a suite theme item sets this app's choice.
    crate::menus::invoke(&mut memory, &ctx, suite[1], json!({})).unwrap();
    assert_eq!(memory.session.prefs().interface.app_choice(), own("craft.graphite", Mode::Dark));
    // Shared: Follow suite first; a suite theme item writes the suite while following.
    let folder = tempfile::tempdir().unwrap();
    let mut shared = app(Some(folder.path()));
    let before = crate::menus::native_menu::structure_key(&crate::menus::native_menu::from_items(&crate::menus::TOP_MENUS, &crate::menus::menu_items(&shared)));
    assert_eq!(ids(&shared)[0], crate::menus::FOLLOW_SUITE_ITEM);
    assert_eq!(checked(&shared, crate::menus::FOLLOW_SUITE_ITEM), Some(true));
    crate::menus::invoke(&mut shared, &ctx, suite[2], json!({})).unwrap();
    assert_eq!(suite_theme(&shared), Some(id("craft.high-contrast")));
    assert_eq!((checked(&shared, suite[2]), checked(&shared, suite[0]), checked(&shared, "window.theme.proMedium")), (Some(true), Some(false), Some(false)));
    crate::menus::invoke(&mut shared, &ctx, crate::menus::FOLLOW_SUITE_ITEM, json!({})).unwrap();
    assert_eq!(checked(&shared, crate::menus::FOLLOW_SUITE_ITEM), Some(false));
    // A new library theme rebuilds the native menu.
    shared.suite.watch().request(Op::Import { bytes: Arc::from(MURKY.as_bytes()), as_copy: false });
    for _ in 0..250 {
        shared.suite.sync();
        if shared.suite.knows(&id("test.murky")) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let after = crate::menus::native_menu::structure_key(&crate::menus::native_menu::from_items(&crate::menus::TOP_MENUS, &crate::menus::menu_items(&shared)));
    assert!(ids(&shared).iter().any(|i| i == "window.theme.suite.test.murky"));
    assert_ne!(before, after);
}
