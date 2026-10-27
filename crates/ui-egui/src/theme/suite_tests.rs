//! Suite themes in PhotoCraft: the adapter's core colours, the migration of saved preferences,
//! the ways back to PhotoCraft's own themes and the restart round trip. Every suite folder here
//! is in memory.

use super::suite::Suite;
use super::*;
use crate::PhotocraftApp;
use craft_theme::{AppChoice, Mode, Role, ThemeId, Variant};
use craft_theme_store::testing::{SAMPLE_THEME_ID, SAMPLE_THEME_JSON, sample_roles};
use craft_theme_store::{Op, SelectionChange, SuiteWatch};
use craft_ui::theme::color32;
use serde_json::json;
use std::sync::{Arc, Mutex};

type Store = Arc<Mutex<Option<String>>>;

/// An app whose preferences live in `store` (`None`: a fresh install).
fn app_on(store: &Store) -> PhotocraftApp {
    let (a, b) = (store.clone(), store.clone());
    let services = crate::Services {
        load_prefs: Some(Box::new(move || a.lock().unwrap().clone())),
        save_prefs: Some(Box::new(move |s: &str| {
            *b.lock().unwrap() = Some(s.to_string());
            Ok(())
        })),
        ..Default::default()
    };
    PhotocraftApp::new(photocraft_engine::Session::new(), services)
}

fn saved(prefs: serde_json::Value) -> Store {
    Arc::new(Mutex::new(Some(prefs.to_string())))
}

fn frame(app: &mut PhotocraftApp, ctx: &egui::Context, os: Option<egui::Theme>) {
    ctx.run_ui(egui::RawInput { system_theme: os, ..Default::default() }, |ui| crate::prefs_ui::tick(app, ui.ctx())).textures_delta.clear();
}

fn slots(ctx: &egui::Context) -> (Tokens, Tokens) {
    installed_tokens(ctx).expect("tokens installed")
}

fn id(s: &str) -> ThemeId {
    ThemeId::parse(s).unwrap()
}

/// The core-mapped token fields equal the theme's roles in that variant.
fn assert_core(t: &Tokens, roles: &craft_theme::CoreRoles, what: &str) {
    let pairs = [
        ("text", t.text, Role::Text),
        ("card", t.card, Role::Surface),
        ("dock", t.dock, Role::Background),
        ("field", t.field, Role::Raised),
        ("accent", t.accent, Role::Accent),
        ("row_selected", t.row_selected, Role::Selection),
        ("danger", t.danger, Role::Bad),
        ("primary_text", t.primary_text, Role::OnAccent),
    ];
    for (field, got, role) in pairs {
        assert_eq!(got, color32(roles.get(role)), "{what}: {field}");
    }
}

#[test]
fn the_sample_theme_and_every_built_in_give_their_core_colours_in_both_slots() {
    let watch = SuiteWatch::memory();
    watch.request(Op::Import { bytes: Arc::from(SAMPLE_THEME_JSON.as_bytes()), as_copy: false });
    let mut app = app_on(&Arc::default());
    app.suite = Suite::new(watch);
    app.suite.change(SelectionChange::mode(Mode::System));
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, Some(egui::Theme::Dark));
    let mut themes: Vec<(ThemeId, [Option<craft_theme::CoreRoles>; 2])> =
        craft_theme::builtins().iter().map(|t| (t.id.clone(), [t.variant(Variant::Light).copied(), t.variant(Variant::Dark).copied()])).collect();
    themes.push((id(SAMPLE_THEME_ID), [Some(sample_roles(Variant::Light)), Some(sample_roles(Variant::Dark))]));
    for (theme, [light, dark]) in themes {
        app.suite.set_theme(theme.clone());
        frame(&mut app, &ctx, Some(egui::Theme::Dark));
        assert!(app.suite.failure().is_none() && app.suite.conflict().is_none(), "{theme:?}");
        let (l, d) = slots(&ctx);
        assert_core(&l, &light.or(dark).unwrap(), &format!("{theme:?} light slot"));
        assert_core(&d, &dark.or(light).unwrap(), &format!("{theme:?} dark slot"));
        assert_eq!(
            (l.kind, d.kind),
            (if light.is_some() { ThemeKind::ProLight } else { ThemeKind::ProMedium }, if dark.is_some() { ThemeKind::ProMedium } else { ThemeKind::ProLight })
        );
        // The OS answer moves the layout to the slot egui draws; nothing is rebuilt.
        frame(&mut app, &ctx, Some(egui::Theme::Light));
        assert_eq!(app.ui.theme, l.kind);
        assert_eq!(Tokens::get(&ctx), l);
        // The ink on accent fills is on_accent, not accent_text.
        assert_eq!(ctx.style_of(egui::Theme::Light).visuals.selection.stroke.color, l.primary_text);
    }
}

#[test]
fn saved_preferences_keep_their_look_and_a_fresh_install_follows_the_suite() {
    let artcraft = craft_theme::builtin(&ThemeId::suite_default()).unwrap();
    // Fresh install: follows the suite; with no suite selection, ArtCraft in Dark.
    let fresh: Store = Arc::default();
    let mut app = app_on(&fresh);
    assert_eq!(app.session.prefs().interface.app_choice(), AppChoice::FollowSuite);
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, Some(egui::Theme::Light));
    let (l, d) = slots(&ctx);
    assert_core(&l, artcraft.variant(Variant::Dark).unwrap(), "fresh light slot");
    assert_core(&d, artcraft.variant(Variant::Dark).unwrap(), "fresh dark slot");
    // A preferences file from before suite themes: PhotoCraft's own themes in its saved mode.
    for (mode, expected, pair) in [
        ("auto", Mode::System, (ThemeKind::Classic, ThemeKind::Studio)),
        ("dark", Mode::Dark, (ThemeKind::Studio, ThemeKind::Studio)),
        ("light", Mode::Light, (ThemeKind::Classic, ThemeKind::Classic)),
    ] {
        let mut app = app_on(&saved(json!({"interface": {"appearanceMode": mode, "darkTheme": "studio", "lightTheme": "classic"}})));
        let choice = app.session.prefs().interface.app_choice();
        assert_eq!(choice, AppChoice::Own(craft_theme::Selection { theme: ThemeId::app_default(), mode: expected }), "{mode}");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, Some(egui::Theme::Dark));
        assert_eq!(installed(&ctx), Some(pair), "{mode}");
        assert_eq!(slots(&ctx), (Tokens::for_kind(pair.0), Tokens::for_kind(pair.1)), "{mode}");
    }
    // `suiteAppearance` present: used as saved.
    let app = app_on(&saved(json!({"interface": {"suiteAppearance": {"followSuite": true}}})));
    assert_eq!(app.session.prefs().interface.app_choice(), AppChoice::FollowSuite);
    let mut app = app_on(&saved(json!({"interface": {"appearanceMode": "light", "suiteAppearance": {"followSuite": false, "themeId": "craft.graphite"}}})));
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, Some(egui::Theme::Dark));
    let graphite = craft_theme::builtin(&id("craft.graphite")).unwrap();
    assert_core(&slots(&ctx).1, graphite.variant(Variant::Light).unwrap(), "own graphite, light mode");
}

#[test]
fn picking_a_photocraft_theme_leaves_the_suite_theme() {
    let ctx = egui::Context::default();
    let mut app = app_on(&Arc::default());
    let follow = |app: &mut PhotocraftApp| crate::prefs_ui::choose_suite_appearance(app, &ctx, Some(true), Some(id("craft.graphite")));
    follow(&mut app);
    assert_eq!(app.session.prefs().interface.app_choice(), AppChoice::FollowSuite);
    crate::menus::invoke(&mut app, &ctx, "window.theme.studio", json!({})).unwrap();
    assert!(matches!(app.session.prefs().interface.app_choice(), AppChoice::Own(sel) if sel.theme.is_app_default()));
    assert_eq!(app.ui.theme, ThemeKind::Studio);
    assert_eq!(installed(&ctx), Some((ThemeKind::Studio, ThemeKind::Studio)));
    follow(&mut app);
    app.run("prefs.set", json!({"path": "interface.theme", "value": "classic"})).unwrap();
    crate::prefs_ui::sync_appearance(&mut app, &ctx);
    assert!(matches!(app.session.prefs().interface.app_choice(), AppChoice::Own(sel) if sel.theme.is_app_default()));
    assert_eq!(installed(&ctx), Some((ThemeKind::Classic, ThemeKind::Classic)));
}

/// An app whose preferences live in `store` and whose suite folder is `dir` (a temporary one).
fn app_on_dir(store: &Store, dir: &std::path::Path) -> PhotocraftApp {
    let (a, b) = (store.clone(), store.clone());
    let dir = craft_theme_store::SuiteDir::at(dir);
    let services = crate::Services {
        load_prefs: Some(Box::new(move || a.lock().unwrap().clone())),
        save_prefs: Some(Box::new(move |s: &str| {
            *b.lock().unwrap() = Some(s.to_string());
            Ok(())
        })),
        suite_watch: Some(Box::new(move |options| SuiteWatch::open(&dir, options, || {}))),
        ..Default::default()
    };
    PhotocraftApp::new(photocraft_engine::Session::new(), services)
}

/// Frames until `done`, since the suite folder is read and written on the watcher's thread.
fn wait_for(app: &mut PhotocraftApp, ctx: &egui::Context, what: &str, done: impl Fn(&PhotocraftApp) -> bool) {
    for _ in 0..250 {
        frame(app, ctx, Some(egui::Theme::Dark));
        if done(app) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("timed out waiting for {what}");
}

fn suite_theme_is(app: &PhotocraftApp, name: &str) -> bool {
    !app.suite.client().is_pending() && app.suite.accepted().suite.theme.as_ref().map(ThemeId::as_str) == Some(name)
}

#[test]
fn the_choice_survives_a_restart_and_the_override_leaves_the_suite_alone() {
    let folder = tempfile::tempdir().unwrap();
    let store: Store = Arc::default();
    let dark = |name: &str| *craft_theme::builtin(&id(name)).unwrap().variant(Variant::Dark).unwrap();
    let ctx = egui::Context::default();
    let mut app = app_on_dir(&store, folder.path());
    crate::prefs_ui::choose_suite_appearance(&mut app, &ctx, Some(true), Some(id("craft.graphite")));
    // Shown at once, before the watcher wrote it.
    frame(&mut app, &ctx, Some(egui::Theme::Dark));
    assert_core(&slots(&ctx).1, &dark("craft.graphite"), "picked");
    wait_for(&mut app, &ctx, "the suite selection written", |app| suite_theme_is(app, "craft.graphite"));
    assert!(folder.path().join("selection.json").exists());
    assert!(store.lock().unwrap().as_deref().is_some_and(|s| s.contains("craft.graphite")), "lastSuite saved");
    drop(app);
    // Restart: the choice and the suite selection are both read back.
    let ctx = egui::Context::default();
    let mut app = app_on_dir(&store, folder.path());
    assert_eq!(app.session.prefs().interface.app_choice(), AppChoice::FollowSuite);
    frame(&mut app, &ctx, Some(egui::Theme::Dark));
    assert_core(&slots(&ctx).1, &dark("craft.graphite"), "after a restart");
    // Override on: PhotoCraft keeps its own theme while the suite changes.
    crate::prefs_ui::choose_suite_appearance(&mut app, &ctx, Some(false), Some(id("craft.high-contrast")));
    app.suite.set_theme(id("craft.artcraft"));
    wait_for(&mut app, &ctx, "the suite's new theme", |app| suite_theme_is(app, "craft.artcraft"));
    assert_core(&slots(&ctx).1, &dark("craft.high-contrast"), "own choice");
    // Override off: the suite's current theme.
    crate::prefs_ui::choose_suite_appearance(&mut app, &ctx, Some(true), None);
    frame(&mut app, &ctx, Some(egui::Theme::Dark));
    assert_core(&slots(&ctx).1, &dark("craft.artcraft"), "following again");
}
