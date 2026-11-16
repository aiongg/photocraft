use super::appearance::{display_scale, selected_theme};
use super::*;
use crate::theme::{AppTheme, Tokens};
use craft::draw::ui::transform::Interpolation;
use craft::keymap::{Binding, CommandRef, Policy};
use craft::menu::{Chord, ItemKind, Node, ThemeChoice, ThemeLabels};
use craft::settings::testing::{MemoryFiles, retry_delay};
use craft::settings::{Location, OpenOptions, Settings, SettingsFiles, SettingsStore};
use craft::theme::ThemeId;
use craft::ui::patterns::preferences::{PrefValue, SettingsDraft};
use egui::{Color32, vec2};
use photocraft_engine::prefs::{self, AppearanceMode};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// --- helpers --------------------------------------------------------------------------------------------------

/// The folder of the scratch settings store; its file lives in memory.
const DIR: &str = "/memory/photocraft";

/// The title of the notice a failed preferences save posts.
const SAVE_NOTICE: &str = "Preferences can't be saved";

fn config_path() -> std::path::PathBuf {
    std::path::Path::new(DIR).join("config.toml")
}

/// Services whose preferences store reads and writes `files`. A zero debounce makes the first `tick` after a
/// change write it.
fn services_on(files: &Arc<MemoryFiles>) -> crate::Services {
    let options = OpenOptions::new(Location::Dir(DIR.into())).debounce(Duration::ZERO).files(files.clone());
    crate::Services { settings: Some(SettingsStore::<prefs::SavedPrefs>::open_with(options)), ..Default::default() }
}

/// One upkeep pass at the current time. A trial that steps the store's clock calls `save::tick` with its own `Instant`.
fn tick(app: &mut PhotocraftApp, ctx: &egui::Context) {
    super::save::tick(app, ctx, Instant::now());
}

/// An app whose preferences live in `files`.
fn app_on(files: &Arc<MemoryFiles>) -> PhotocraftApp {
    PhotocraftApp::new(photocraft_engine::Session::new(), services_on(files))
}

fn app_with_store() -> (PhotocraftApp, Arc<MemoryFiles>) {
    let files = Arc::new(MemoryFiles::new());
    (app_on(&files), files)
}

/// An app that starts on a file holding `saved` (the JSON form of the preferences), at the current schema version.
fn app_with_json(saved: &Value) -> (PhotocraftApp, Arc<MemoryFiles>) {
    app_with_json_at(saved, Some(<prefs::SavedPrefs as Settings>::SCHEMA_VERSION))
}

/// An app that starts on a file holding `saved` with the given `schema_version` (`None`: no version key).
fn app_with_json_at(saved: &Value, version: Option<u32>) -> (PhotocraftApp, Arc<MemoryFiles>) {
    let files = Arc::new(MemoryFiles::new());
    seed(&files, saved, version);
    (app_on(&files), files)
}

/// An app that starts on a legacy `preferences.json` holding `saved` with the given `version` key (`None`: none), the
/// file the settings store imports on its first run, before it has a `config.toml`. Only the import reads values one by
/// one and migrates a file by its version; a `config.toml` section that does not fit resets as a whole.
fn app_with_legacy_json(saved: &Value, version: Option<u32>) -> (PhotocraftApp, Arc<MemoryFiles>) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let mut saved = saved.clone();
    if let (Some(map), Some(version)) = (saved.as_object_mut(), version) {
        map.insert("version".into(), json!(version));
    }
    let dir = std::env::temp_dir().join(format!("photocraft-legacy-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("preferences.json");
    std::fs::write(&path, saved.to_string()).unwrap();
    let files = Arc::new(MemoryFiles::new());
    let legacy = vec![craft::settings::LegacyFile { path, parse: prefs::parse_legacy_prefs }];
    let options = OpenOptions::new(Location::Dir(DIR.into())).debounce(Duration::ZERO).files(files.clone()).legacy(legacy);
    let services = crate::Services { settings: Some(SettingsStore::<prefs::SavedPrefs>::open_with(options)), ..Default::default() };
    let app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
    std::fs::remove_dir_all(&dir).ok();
    (app, files)
}

/// Writes the settings file as TOML, which has no null: nulls are left out.
fn seed(files: &MemoryFiles, saved: &Value, version: Option<u32>) {
    fn strip_nulls(value: &mut Value) {
        match value {
            Value::Object(map) => {
                map.retain(|_, v| !v.is_null());
                map.values_mut().for_each(strip_nulls);
            }
            Value::Array(items) => {
                items.retain(|v| !v.is_null());
                items.iter_mut().for_each(strip_nulls);
            }
            _ => {}
        }
    }
    let mut saved = saved.clone();
    strip_nulls(&mut saved);
    if let (Some(map), Some(version)) = (saved.as_object_mut(), version) {
        map.insert("schema_version".into(), json!(version));
    }
    files.write(&config_path(), &toml::to_string(&saved).unwrap()).unwrap();
}

/// The settings file as JSON; `null` while nothing was written.
fn stored(files: &MemoryFiles) -> Value {
    files.text(&config_path()).map_or(Value::Null, |text| toml::from_str::<Value>(&text).unwrap())
}

fn saved_text(files: &MemoryFiles) -> Option<String> {
    files.text(&config_path())
}

/// Opens Preferences on `section`.
fn open_prefs(app: &mut PhotocraftApp, section: &str) {
    open_preferences(app, section);
    assert!(window_open(app), "Preferences did not open");
}

/// Edits the open window's draft the way a row does.
fn edit(app: &mut PhotocraftApp, key: &str, value: PrefValue) {
    assert!(set_draft(app, key, value), "{key}: the window refused the value");
}

/// A kittest harness on an app (built inside it, on the harness's context) with Preferences open on `section`.
fn prefs_harness(
    files: &Arc<MemoryFiles>,
    section: &'static str,
    size: egui::Vec2,
    setup: impl FnOnce(&mut PhotocraftApp) + 'static,
) -> egui_kittest::Harness<'static, PhotocraftApp> {
    let files = files.clone();
    let mut h = egui_kittest::Harness::builder().with_size(size).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::with_context(cc.egui_ctx.clone(), photocraft_engine::Session::new(), services_on(&files));
        setup(&mut app);
        open_prefs(&mut app, section);
        app
    });
    h.run_steps(4);
    h
}

/// The Appearance menu's rows (items only).
fn theme_rows(app: &PhotocraftApp) -> Vec<craft::menu::Item> {
    let nodes = theme_menu(app).items(&ThemeLabels::default());
    nodes.into_iter().filter_map(|node| if let Node::Item(item) = node { Some(item) } else { None }).collect()
}

/// The row of `choice` among `rows`.
fn theme_row<'a>(rows: &'a [craft::menu::Item], choice: &ThemeChoice) -> &'a craft::menu::Item {
    rows.iter().find(|item| item.command == choice.command()).unwrap_or_else(|| panic!("no row for {choice:?}"))
}

/// Runs a theme choice as the menu does.
fn pick_theme(app: &mut PhotocraftApp, ctx: &egui::Context, choice: &ThemeChoice) {
    let command = choice.command();
    crate::menus::invoke(app, ctx, &command.id, command.params.clone()).unwrap();
}

fn chord(text: &str) -> Chord {
    Chord::parse(text).unwrap_or_else(|e| panic!("{text}: {e:?}"))
}

/// The shortcut the menu shows for `id`.
fn menu_shortcut(app: &PhotocraftApp, id: &str) -> Option<Chord> {
    let rows = crate::menus::menu_rows(app);
    rows.iter().find(|row| row.command.id == id).unwrap_or_else(|| panic!("no menu row {id}")).shortcut.clone()
}

// --- the preferences' own rules ---------------------------------------------------------------------------------

#[test]
fn rendering_mode_display_respects_explicit_mode_and_legacy_disable() {
    use super::custom_rows::rendering_mode_value;
    // A draft holding the `performance.*` values of `saved`; a null is a value the draft doesn't hold.
    let draft = |saved: serde_json::Value| {
        let mut draft = SettingsDraft::default();
        for (key, value) in saved.as_object().unwrap().iter().filter(|(_, value)| !value.is_null()) {
            draft.set(&format!("performance.{key}"), value);
        }
        draft
    };
    let legacy = draft(serde_json::json!({"useGpu": false, "gpuBackend": "auto"}));
    assert_eq!(rendering_mode_value(&legacy), "cpu");
    let explicit = draft(serde_json::json!({"renderingMode": "gpu", "useGpu": false, "gpuBackend": "cpu"}));
    assert_eq!(rendering_mode_value(&explicit), "gpu");
    let automatic = draft(serde_json::json!({"renderingMode": null, "useGpu": true, "gpuBackend": "auto"}));
    assert_eq!(rendering_mode_value(&automatic), "auto");
}

#[test]
fn appearance_resolves_both_saved_themes_and_follows_system() {
    let mut interface = prefs::Interface { appearance_mode: AppearanceMode::Auto, ..prefs::Interface::default() };
    assert_eq!(selected_theme(&interface, Some(egui::Theme::Dark)), shown(AppTheme::pro_medium()));
    assert_eq!(selected_theme(&interface, Some(egui::Theme::Light)), shown(AppTheme::pro_light()));
    interface.dark_theme = "craft.studio".into();
    interface.light_theme = "craft.classic".into();
    assert_eq!(selected_theme(&interface, Some(egui::Theme::Dark)), shown(AppTheme::studio()));
    assert_eq!(selected_theme(&interface, Some(egui::Theme::Light)), shown(AppTheme::classic()));
    interface.appearance_mode = AppearanceMode::Dark;
    assert_eq!(selected_theme(&interface, Some(egui::Theme::Light)), shown(AppTheme::studio()));
    interface.appearance_mode = AppearanceMode::Light;
    assert_eq!(selected_theme(&interface, Some(egui::Theme::Dark)), shown(AppTheme::classic()));
}

/// The theme id and mode `theme` shows, as `selected_theme` answers.
fn shown(theme: AppTheme) -> (ThemeId, bool) {
    (theme.id().clone(), theme.is_dark())
}

#[test]
fn breeze_themes_are_chosen_by_preference_and_fix_the_matching_mode() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    let ctx = app.ctx().clone();
    for (id, kind, mode) in [
        ("breezeLight", AppTheme::breeze_light(), AppearanceMode::Light),
        ("breezeDark", AppTheme::breeze_dark(), AppearanceMode::Dark),
        ("softLight", AppTheme::soft_light(), AppearanceMode::Light),
        ("softDark", AppTheme::soft_dark(), AppearanceMode::Dark),
    ] {
        app.run("prefs.set", json!({"path": "interface.theme", "value": id})).unwrap();
        tick(&mut app, &ctx);
        let interface = app.session.prefs().interface.clone();
        assert_eq!(interface.appearance_mode, mode, "{id}");
        assert_eq!(selected_theme(&interface, None), shown(kind.clone()), "{id}");
        assert_eq!(app.ui.theme, kind, "{id}");
    }
    // The two slots are independent: choosing the dark one left the light choice alone.
    assert_eq!(app.session.prefs().interface.light_theme, "craft.soft");
    assert_eq!(app.session.prefs().interface.dark_theme, "craft.soft");
}

#[test]
fn auto_uses_native_system_theme_when_egui_does_not_report_one() {
    let (mut app, _) = app_with_store();
    app.run("prefs.set", json!({"path": "interface.appearanceMode", "value": "auto"})).unwrap();
    let ctx = egui::Context::default();
    // The reader reports light; no `Event::SystemTheme` reaches egui (X11, and Wayland without the settings portal).
    craft::ui::testing::style::report_os_theme(&ctx, false);
    tick(&mut app, &ctx);
    assert_eq!(app.ui.theme, AppTheme::pro_light());
}

#[test]
fn appearance_button_cycles_modes_without_losing_theme_choices() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    let ctx = app.ctx().clone();
    app.run("prefs.set", json!({"values": {"interface.darkTheme": "studio", "interface.lightTheme": "classic"}})).unwrap();
    for (mode, visible) in
        [(AppearanceMode::Auto, AppTheme::studio()), (AppearanceMode::Light, AppTheme::classic()), (AppearanceMode::Dark, AppTheme::studio())]
    {
        cycle_appearance(&mut app, &ctx);
        assert_eq!(app.session.prefs().interface.appearance_mode, mode);
        assert_eq!(app.ui.theme, visible);
        assert_eq!(app.session.prefs().interface.dark_theme, "craft.studio");
        assert_eq!(app.session.prefs().interface.light_theme, "craft.classic");
    }
}

#[test]
fn darken_and_lighten_interface_step_through_photoshops_brightnesses_and_stop_at_the_ends() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    let ctx = app.ctx().clone();
    sync_appearance(&mut app, &ctx);
    assert_eq!(app.ui.theme, AppTheme::pro_medium(), "Photoshop's default brightness");
    let press = |app: &mut PhotocraftApp, id: &str| crate::menus::invoke(app, &ctx, id, json!({})).unwrap();
    for expected in [AppTheme::pro_light_gray(), AppTheme::pro_light(), AppTheme::pro_light()] {
        press(&mut app, "view.lightenInterface");
        assert_eq!(app.ui.theme, expected);
    }
    // Saved like a choice in Preferences › Interface: the light slot and the Light appearance.
    let i = &app.session.prefs().interface;
    assert_eq!((i.appearance_mode, i.light_theme.as_str()), (AppearanceMode::Light, "craft.pro"));
    for expected in [AppTheme::pro_light_gray(), AppTheme::pro_medium(), AppTheme::pro(), AppTheme::pro()] {
        press(&mut app, "view.darkenInterface");
        assert_eq!(app.ui.theme, expected);
    }
    let i = &app.session.prefs().interface;
    assert_eq!((i.appearance_mode, i.dark_theme.as_str()), (AppearanceMode::Dark, "craft.pro"));
    // Another theme steps from the nearest brightness.
    for (theme, id, expected) in [
        ("studio", "view.darkenInterface", AppTheme::pro()),
        ("studio", "view.lightenInterface", AppTheme::pro_light_gray()),
        ("classic", "view.darkenInterface", AppTheme::pro_medium()),
        ("classic", "view.lightenInterface", AppTheme::pro_light()),
    ] {
        app.run("prefs.set", json!({"path": "interface.theme", "value": theme})).unwrap();
        sync_appearance(&mut app, &ctx);
        press(&mut app, id);
        assert_eq!(app.ui.theme, expected, "{theme} {id}");
    }
}

/// One upkeep frame after the OS reports `appearance`; `None` reports nothing, so a context that never had a report
/// has no answer, and one that had keeps its last (the reader holds the OS's latest colour scheme).
fn appearance_frame(app: &mut PhotocraftApp, ctx: &egui::Context, appearance: Option<egui::Theme>) {
    if let Some(theme) = appearance {
        craft::ui::testing::style::report_os_theme(ctx, theme == egui::Theme::Dark);
    }
    ctx.run_ui(egui::RawInput::default(), |ui| tick(app, ui.ctx())).textures_delta.clear();
}

fn assert_palette(ctx: &egui::Context, kind: &AppTheme) {
    let t = Tokens::get(ctx);
    assert_eq!(t, kind.tokens());
    for style in [ctx.global_style(), ctx.style_of(egui::Theme::Light), ctx.style_of(egui::Theme::Dark)] {
        assert_eq!(style.visuals.panel_fill, t.chrome);
        assert_eq!(style.visuals.window_fill, t.card);
        assert_eq!(style.visuals.override_text_color, Some(t.text));
        assert_eq!(style.visuals.widgets.inactive.bg_fill, t.field);
        assert_eq!(style.visuals.dark_mode, t.dark());
        assert_eq!(style.text_styles[&egui::TextStyle::Body].size, if t.pro { 12.0 } else { 12.5 });
    }
    assert_eq!(ctx.options(|o| o.theme_preference), egui::ThemePreference::System);
}

#[test]
fn auto_appearance_follows_live_changes_without_saving_resolved_palettes() {
    for (dark, light) in [(AppTheme::pro_medium(), AppTheme::studio_light()), (AppTheme::solarized_dark(), AppTheme::adwaita())] {
        let (mut app, store) = app_with_store();
        let ctx = egui::Context::default();
        PhotocraftApp::setup_context(&ctx, Default::default());
        app.run("prefs.set", json!({"values": {"interface.appearanceMode": "auto", "interface.darkTheme": dark.id(), "interface.lightTheme": light.id()}}))
            .unwrap();
        appearance_frame(&mut app, &ctx, Some(egui::Theme::Light));
        let revision = app.session.prefs.rev();
        let interface = app.session.prefs().interface.clone();
        let saved = saved_text(&store).unwrap();
        assert_eq!(stored(&store)["interface"]["appearanceMode"], "auto");
        let gpu_style = canvas_style(&app);
        for (appearance, expected) in
            [(Some(egui::Theme::Light), light.clone()), (Some(egui::Theme::Dark), dark.clone()), (Some(egui::Theme::Light), light.clone())]
        {
            appearance_frame(&mut app, &ctx, appearance);
            assert_palette(&ctx, &expected);
            assert_eq!(app.ui.theme, expected);
            assert_eq!(app.session.prefs().interface, interface);
            assert_eq!(app.session.prefs.rev(), revision);
            assert_eq!(saved_text(&store), Some(saved.clone()));
            assert_eq!(canvas_style(&app), gpu_style);
            assert_eq!(pasteboard_color(&app), None);
        }
        // A fresh launch resolves its saved Auto choice against the new OS input.
        for (appearance, expected) in [(Some(egui::Theme::Light), light.clone()), (Some(egui::Theme::Dark), dark.clone()), (None, dark.clone())] {
            let mut restarted = app_on(&store);
            let fresh = egui::Context::default();
            PhotocraftApp::setup_context(&fresh, Default::default());
            appearance_frame(&mut restarted, &fresh, appearance);
            assert_palette(&fresh, &expected);
            assert_eq!(restarted.ui.theme, expected);
            assert_eq!(restarted.session.prefs().interface, interface);
            assert_eq!(saved_text(&store), Some(saved.clone()));
        }
        app.run("prefs.set", json!({"values": {"interface.canvasColor": "custom", "interface.canvasCustomColor": "#123456"}})).unwrap();
        for appearance in [Some(egui::Theme::Light), Some(egui::Theme::Dark)] {
            appearance_frame(&mut app, &ctx, appearance);
            assert_eq!(pasteboard_color(&app), Some(Color32::from_rgb(0x12, 0x34, 0x56)));
            assert_eq!(canvas_style(&app), gpu_style);
        }
    }
}

#[test]
fn manual_theme_tokens_and_global_styles_stay_fixed_across_os_changes() {
    let (mut app, store) = app_with_store();
    let ctx = egui::Context::default();
    PhotocraftApp::setup_context(&ctx, Default::default());
    for kind in AppTheme::all() {
        app.save_theme(&kind).unwrap();
        appearance_frame(&mut app, &ctx, Some(egui::Theme::Dark));
        let interface = app.session.prefs().interface.clone();
        let revision = app.session.prefs.rev();
        let saved = saved_text(&store).unwrap();
        assert_eq!(interface.appearance_mode, if kind.tokens().dark() { AppearanceMode::Dark } else { AppearanceMode::Light });
        for appearance in [Some(egui::Theme::Dark), Some(egui::Theme::Light), Some(egui::Theme::Dark), None] {
            appearance_frame(&mut app, &ctx, appearance);
            assert_palette(&ctx, &kind);
            assert_eq!(Tokens::get(&ctx), kind.tokens());
            assert_eq!(app.ui.theme, kind);
            assert_eq!(app.session.prefs().interface, interface);
            assert_eq!(app.session.prefs.rev(), revision);
            assert_eq!(saved_text(&store), Some(saved.clone()));
        }
    }
}

#[test]
fn system_theme_menu_choice_stays_checked_as_the_appearance_changes() {
    let (mut app, store) = app_with_store();
    let ctx = egui::Context::default();
    PhotocraftApp::setup_context(&ctx, Default::default());
    app.run("prefs.set", json!({"values": {"interface.darkTheme": "solarizedDark", "interface.lightTheme": "adwaita"}})).unwrap();
    appearance_frame(&mut app, &ctx, Some(egui::Theme::Light));
    pick_theme(&mut app, &ctx, &ThemeChoice::System);
    appearance_frame(&mut app, &ctx, Some(egui::Theme::Light));
    let interface = app.session.prefs().interface.clone();
    let saved = saved_text(&store).unwrap();
    let revision = app.session.prefs.rev();
    for appearance in [Some(egui::Theme::Light), Some(egui::Theme::Dark), None] {
        appearance_frame(&mut app, &ctx, appearance);
        let rows = theme_rows(&app);
        assert_eq!(theme_row(&rows, &ThemeChoice::System).kind, ItemKind::Radio(true));
        for item in rows.iter().filter(|item| item.command != ThemeChoice::System.command() && item.command != ThemeChoice::NextMode.command()) {
            assert_eq!(item.kind, ItemKind::Radio(false), "{}", item.label);
        }
        assert_eq!(app.session.prefs().interface.appearance_mode, AppearanceMode::Auto);
        assert_eq!(app.session.prefs().interface, interface);
        assert_eq!(app.session.prefs.rev(), revision);
        assert_eq!(saved_text(&store), Some(saved.clone()));
    }
    let solarized = ThemeChoice::Theme { id: AppTheme::solarized_dark().id().clone(), dark: true };
    pick_theme(&mut app, &ctx, &solarized);
    for appearance in [Some(egui::Theme::Light), Some(egui::Theme::Dark)] {
        appearance_frame(&mut app, &ctx, appearance);
        assert_palette(&ctx, &AppTheme::solarized_dark());
        let rows = theme_rows(&app);
        assert_eq!(theme_row(&rows, &solarized).kind, ItemKind::Radio(true));
        assert_eq!(theme_row(&rows, &ThemeChoice::System).kind, ItemKind::Radio(false));
        assert_eq!(stored(&store)["interface"]["appearanceMode"], "dark");
        assert_eq!(app.session.prefs().interface.light_theme, "craft.adwaita");
    }
}

#[test]
fn preferences_interface_offers_and_applies_sync_with_system() {
    let (mut app, store) = app_with_store();
    app.run(
        "prefs.set",
        json!({"values": {"interface.language": "en", "interface.appearanceMode": "dark", "interface.darkTheme": "solarizedDark", "interface.lightTheme": "adwaita"}}),
    )
    .unwrap();
    let interface = app.session.prefs().interface.clone();
    open_prefs(&mut app, "interface");
    edit(&mut app, "interface.appearanceMode", PrefValue::Text("auto".into()));
    assert_eq!(app.session.prefs().interface.appearance_mode, AppearanceMode::Dark, "nothing is applied before Apply");
    apply_window(&mut app).unwrap();
    assert_eq!(app.session.prefs().interface.appearance_mode, AppearanceMode::Auto);
    assert_eq!(app.session.prefs().interface.dark_theme, interface.dark_theme);
    assert_eq!(app.session.prefs().interface.light_theme, interface.light_theme);
    let ctx = egui::Context::default();
    PhotocraftApp::setup_context(&ctx, Default::default());
    appearance_frame(&mut app, &ctx, Some(egui::Theme::Light));
    assert_palette(&ctx, &AppTheme::adwaita());
    assert_eq!(stored(&store)["interface"]["appearanceMode"], "auto");
    let mut restarted = app_on(&store);
    let fresh = egui::Context::default();
    PhotocraftApp::setup_context(&fresh, Default::default());
    appearance_frame(&mut restarted, &fresh, Some(egui::Theme::Dark));
    assert_palette(&fresh, &AppTheme::solarized_dark());
    assert_eq!(restarted.session.prefs().interface.appearance_mode, AppearanceMode::Auto);
    assert_eq!(restarted.session.prefs().interface.dark_theme, interface.dark_theme);
    assert_eq!(restarted.session.prefs().interface.light_theme, interface.light_theme);
}

#[test]
fn theme_star_saves_favorite_and_menu_groups_it() {
    use egui_kittest::kittest::Queryable;
    let (_, files) = app_with_store();
    let mut h = prefs_harness(&files, "interface", vec2(1280.0, 800.0), |_| {});
    h.get_by_label("Classic").hover();
    h.run_steps(2);
    h.get_by_label("Add to Favorites: Classic").click();
    h.run_steps(2);
    h.get_by_label("Apply").click();
    h.run_steps(2);
    assert_eq!(h.state().session.prefs().interface.favorite_themes, vec!["craft.classic"]);
    assert!(h.query_by_label("Remove from Favorites: Classic").is_some());
    let nodes = theme_menu(h.state()).favorite_items(&ThemeLabels::default(), CommandRef::new("edit.preferences.interface"));
    let listed: Vec<_> = nodes.iter().filter_map(|node| if let Node::Item(item) = node { Some(item.command.clone()) } else { None }).collect();
    let classic = ThemeChoice::Theme { id: ThemeId::parse("craft.classic").unwrap(), dark: false };
    assert!(listed.contains(&classic.command()), "{listed:?}");
    assert_eq!(listed.len(), 2, "the settings row and the one favourite: {listed:?}");
}

<<<<<<< HEAD
// --- the settings store -------------------------------------------------------------------------------------------
=======
/// The favourite star badge overhangs its swatch; showing it on hover must not move the swatches
/// after it, or the row would shift under the pointer.
#[test]
fn theme_star_badge_on_hover_does_not_move_the_next_swatch() {
    use egui_kittest::{Harness, kittest::Queryable};
    let (mut app, _) = app_with_store();
    open_preferences(&mut app, "interface");
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        app
    });
    h.run_steps(4);
    let first = h.get_by_label("Classic").rect();
    let next = h.get_by_label("Adwaita").rect();
    h.hover_at(first.right_top() + vec2(-3.0, 3.0));
    h.run_steps(3);
    assert!(h.query_by_label("Add to favorites: Classic").is_some(), "the badge shows while the swatch is hovered");
    assert_eq!(h.get_by_label("Classic").rect(), first);
    assert_eq!(h.get_by_label("Adwaita").rect(), next);
}

#[test]
fn preferences_interface_shows_language_without_scrolling() {
    // #2532: in a small window the theme cards filled the page and Language sat out of view
    // below them. Like Photoshop, Presentation follows Appearance and both fit.
    use egui_kittest::{Harness, kittest::Queryable};
    let (mut app, _) = app_with_store();
    app.run("prefs.set", json!({"values": {"interface.language": "en"}})).unwrap();
    open_preferences(&mut app, "interface");
    let mut h = Harness::builder().with_size(vec2(1000.0, 750.0)).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        app
    });
    h.run_steps(4);
    let language = h.query_all_by_label("Language").next().expect("the Language row").rect();
    let dark = h.query_all_by_label("Dark Theme").next().expect("the dark theme row").rect();
    let ok = h.get_by_label("OK").rect();
    assert!(dark.bottom() < language.top(), "Presentation {language:?} follows the theme swatches {dark:?}");
    assert!(language.bottom() < ok.top() - 20.0, "Language {language:?} must show above the buttons {ok:?} without scrolling");
    // Every theme in the option lists gets a swatch, Breeze included.
    for (id, _) in LIGHT_THEME_OPTIONS.iter().chain(DARK_THEME_OPTIONS) {
        let label = choice_label(id);
        assert!(h.query_by_label(&label).is_some(), "missing theme swatch {label}");
    }
    assert!(h.query_by_label("Breeze light").is_some() && h.query_by_label("Breeze dark").is_some());
}

#[test]
fn preferences_interface_search_hides_rows_and_skips_empty_groups() {
    use egui_kittest::{Harness, kittest::Queryable};
    let render = |search: &str| {
        let (mut app, _) = app_with_store();
        app.run("prefs.set", json!({"values": {"interface.language": "en"}})).unwrap();
        let id = open_preferences(&mut app, "interface");
        app.ui.dialog_mut(id).unwrap().fields.insert("__search".into(), json!(search));
        let mut h = Harness::builder().with_size(vec2(1000.0, 750.0)).build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            app
        });
        h.run_steps(4);
        h
    };
    let shown = |h: &Harness<'_, PhotocraftApp>, label: &str| h.query_all_by_label(label).next().is_some();

    let h = render("language");
    assert!(shown(&h, "Language") && shown(&h, "Presentation"));
    for gone in ["Appearance", "Appearance Mode", "Dark Theme", "Options", "UI scale"] {
        assert!(!shown(&h, gone), "\"language\" must not show {gone}");
    }

    let h = render("theme");
    assert!(shown(&h, "Appearance") && shown(&h, "Light Theme") && shown(&h, "Dark Theme") && shown(&h, "Breeze dark"));
    for gone in ["Appearance Mode", "Presentation", "Language", "Options"] {
        assert!(!shown(&h, gone), "\"theme\" must not show {gone}");
    }

    let h = render("");
    for group in ["Appearance", "Appearance Mode", "Presentation", "Options", "UI scale"] {
        assert!(shown(&h, group), "an empty search shows {group}");
    }
}

pub(super) fn app_with_store() -> (PhotocraftApp, Arc<Mutex<Option<String>>>) {
    app_with_saved(None)
}

fn app_with_saved(text: Option<String>) -> (PhotocraftApp, Arc<Mutex<Option<String>>>) {
    let store: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(text));
    (app_on(&store, Arc::default()), store)
}

/// An app whose preferences live in `store`; writes fail while `fail` is set.
fn app_on(store: &Arc<Mutex<Option<String>>>, fail: Arc<std::sync::atomic::AtomicBool>) -> PhotocraftApp {
    let (a, b) = (store.clone(), store.clone());
    let services = crate::Services {
        load_prefs: Some(Box::new(move || a.lock().unwrap().clone())),
        save_prefs: Some(Box::new(move |s: &str| {
            if fail.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("storage is full".into());
            }
            *b.lock().unwrap() = Some(s.to_string());
            Ok(())
        })),
        ..Default::default()
    };
    PhotocraftApp::new(photocraft_engine::Session::new(), services)
}

fn stored(store: &Arc<Mutex<Option<String>>>) -> Value {
    serde_json::from_str(store.lock().unwrap().as_deref().unwrap_or("null")).unwrap()
}
>>>>>>> upstream/main

#[test]
fn saving_preferences_waits_for_the_actions_write_only_for_its_shortcuts() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let old = "actions.playById:00112233445566778899aabbccddeeff";
    let new = "actions.playById:ffeeddccbbaa99887766554433221100";
    let (mut app, store) = app_with_saved(Some(json!({"shortcuts": {(old): "F2"}, "alternateShortcuts": {(old): ["F3"]}}).to_string()));
    let pending = Arc::new(AtomicBool::new(false));
    let observed = pending.clone();
    app.services.actions_write_pending = Some(Box::new(move || observed.load(Ordering::Relaxed)));
    let ctx = egui::Context::default();
    tick(&mut app, &ctx);

    pending.store(true, Ordering::Relaxed);
    app.session.edit_prefs(|prefs| {
        prefs.shortcuts.remove(old);
        prefs.shortcuts.insert(new.into(), "F2".into());
        prefs.alternate_shortcuts.remove(old);
        prefs.file_handling.recent_file_count = 12;
    });
    tick(&mut app, &ctx);
    let before_commit = stored(&store);
    assert_eq!(before_commit["fileHandling"]["recentFileCount"], 12);
    assert_eq!(before_commit["shortcuts"][old], "F2");
    assert_eq!(before_commit["alternateShortcuts"][old], json!(["F3"]));
    assert!(before_commit["shortcuts"].get(new).is_none());

    pending.store(false, Ordering::Relaxed);
    tick(&mut app, &ctx);
    let after_commit = stored(&store);
    assert!(after_commit["shortcuts"].get(old).is_none());
    assert!(after_commit["alternateShortcuts"].get(old).is_none());
    assert_eq!(after_commit["shortcuts"][new], "F2");
}

#[test]
fn desktop_shortcuts_wait_for_the_actions_definition_write() {
    use photocraft_engine::actions_cmds;
    use photocraft_engine::preset_store::{ACTIONS_FILE, MemBackend, PresetBackend, open};
    use std::sync::atomic::{AtomicBool, Ordering};

    struct RetriableActionsWrite {
        store: MemBackend,
        reject: Arc<AtomicBool>,
    }

    impl PresetBackend for RetriableActionsWrite {
        fn list(&self) -> Result<Vec<(String, u64)>, String> {
            self.store.list()
        }

        fn read(&self, name: &str, max: u64) -> Result<Vec<u8>, String> {
            self.store.read(name, max)
        }

        fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
            if name == ACTIONS_FILE && self.reject.load(Ordering::Relaxed) {
                return Err("injected Actions write rejection".into());
            }
            self.store.write(name, bytes)
        }

        fn remove(&self, name: &str) -> Result<(), String> {
            self.store.remove(name)
        }
    }

    let old_id = "00112233445566778899aabbccddeeff";
    let old = actions_cmds::occurrence_key(old_id);
    let original = json!({"version":1, "actions":[{"name":"Saved", "action_id":old_id}]});
    let ctx = egui::Context::default();
    for attach in [true, false] {
        let (tx, rx) = std::sync::mpsc::channel();
        let (mut app, prefs) = app_with_saved(Some(json!({"shortcuts":{(&old):"F4"}, "alternateShortcuts":{(&old):["F4","F8"]}}).to_string()));
        app.services.preset_store = Some(rx);
        app.run("actions.record", json!({"name":"Early"})).unwrap();
        app.run("actions.stop", json!({})).unwrap();
        crate::actions::assign_shortcut(&mut app, 0, "F4").unwrap();
        let new_id = app.session.actions.list[0].action_id.clone().unwrap();
        let new = actions_cmds::occurrence_key(&new_id);
        let assert_held = |ordinary| {
            let saved = stored(&prefs);
            assert_eq!(saved["fileHandling"]["recentFileCount"], ordinary);
            assert_eq!(saved["shortcuts"][&old], "F4");
            assert_eq!(saved["alternateShortcuts"][&old], json!(["F4", "F8"]));
            assert!(saved["shortcuts"].get(&new).is_none());
        };

        app.session.edit_prefs(|p| p.file_handling.recent_file_count = 12);
        save_preferences(&mut app).unwrap();
        assert_held(12);
        app.session.edit_prefs(|p| p.file_handling.recent_file_count = 13);
        tick(&mut app, &ctx);
        assert_held(13);

        let definitions = MemBackend::default();
        definitions.write(ACTIONS_FILE, original.to_string().as_bytes()).unwrap();
        let reject = Arc::new(AtomicBool::new(true));
        if attach {
            tx.send(open(Box::new(RetriableActionsWrite { store: definitions.clone(), reject: reject.clone() }))).unwrap();
        }
        drop(tx);
        tick(&mut app, &ctx);
        assert!(app.services.preset_store.is_none());
        assert_eq!(app.session.preset_store.is_some(), attach);
        app.session.edit_prefs(|p| p.file_handling.recent_file_count = 14);
        save_preferences(&mut app).unwrap();
        assert_held(14);
        assert_eq!(serde_json::from_slice::<Value>(&definitions.read(ACTIONS_FILE, 4096).unwrap()).unwrap(), original);

        if attach {
            reject.store(false, Ordering::Relaxed);
            // A normal command retries the failed Actions write before preferences are saved.
            app.run("file.new", json!({"width":2,"height":2})).unwrap();
            save_preferences(&mut app).unwrap();
            let durable = serde_json::from_slice::<Value>(&definitions.read(ACTIONS_FILE, 4096).unwrap()).unwrap();
            assert_eq!(durable["actions"][0]["action_id"], new_id);
            let saved = stored(&prefs);
            assert_eq!(saved["shortcuts"][&new], "F4");
            assert_ne!(saved["shortcuts"][&old], "F4");
            assert_eq!(saved["alternateShortcuts"][&old], json!(["F8"]));
            assert_eq!(saved["fileHandling"]["recentFileCount"], 14);
        } else {
            app.run("actions.record", json!({"name":"After disconnect"})).unwrap();
            app.run("actions.stop", json!({})).unwrap();
            crate::actions::assign_shortcut(&mut app, 1, "F8").unwrap();
            let late_id = app.session.actions.list[1].action_id.clone().unwrap();
            let late = actions_cmds::occurrence_key(&late_id);
            save_preferences(&mut app).unwrap();
            assert_held(14);
            assert!(stored(&prefs)["shortcuts"].get(&late).is_none());

            let (tx, rx) = std::sync::mpsc::channel();
            tx.send(open(Box::new(definitions.clone()))).unwrap();
            app.services.preset_store = Some(rx);
            tick(&mut app, &ctx);
            let durable = serde_json::from_slice::<Value>(&definitions.read(ACTIONS_FILE, 4096).unwrap()).unwrap();
            assert_eq!(durable["actions"][1]["action_id"], late_id);
            let saved = stored(&prefs);
            assert_eq!(saved["shortcuts"][&new], "F4");
            assert_eq!(saved["shortcuts"][&late], "F8");
            assert!(saved["alternateShortcuts"][&old].as_array().is_none_or(Vec::is_empty));
        }
    }
}

#[test]
fn a_wrongly_typed_value_resets_only_itself_and_is_repaired_on_disk() {
    // #6611: one bad value used to reset every preference, on every launch.
    let saved = json!({
        "fileHandling": {"recentFileCount": "ten", "autosaveMinutes": 5},
        "performance": {"historyStates": 12},
        "shortcuts": {"edit.fill": "Cmd+Shift+F", "file.open": 7},
        "futureSection": {"kept": true},
    });
    let (mut app, store) = app_with_json(&saved);
    let ctx = egui::Context::default();
    tick(&mut app, &ctx);
    let p = app.session.prefs();
    assert_eq!((p.file_handling.recent_file_count, p.file_handling.autosave_minutes, p.performance.history_states), (20, 5, 12));
    assert!(!app.ui.status.contains("reset"), "{}", app.ui.status);
    let notice = app.ui.notices.iter().find(|n| n.title == "Some preferences were reset").expect("a notice names the bad values");
    let text = notice.text(&notice.lines[0]);
    assert!(text.contains("fileHandling.recentFileCount, shortcuts.file.open"), "{text}");
    // The file now holds the default in its place and keeps everything else.
    let v = stored(&store);
    assert_eq!(v["fileHandling"]["recentFileCount"], 20);
    assert_eq!(v["fileHandling"]["autosaveMinutes"], 5);
    assert_eq!(v["performance"]["historyStates"], 12);
    assert_eq!(v["shortcuts"]["edit.fill"], "Cmd+Shift+F");
    assert!(v["shortcuts"].get("file.open").is_none());
    assert_eq!(v["futureSection"]["kept"], true, "keys this version doesn't know survive");
    // So the next launch loads cleanly.
    let mut next = app_on(&store);
    tick(&mut next, &ctx);
    assert!(next.ui.notices.iter().all(|n| n.title != "Some preferences were reset"));
    assert_eq!(next.session.prefs().file_handling.autosave_minutes, 5);
}

#[test]
fn an_old_files_shift_tool_switch_migration_is_saved_with_the_new_version() {
    // #5647: a file before version 2 loads with Use Shift Key for Tool Switch on. Saving
    // writes that and the current version, so turning it off later isn't taken for the old
    // default on the next launch.
    let ctx = egui::Context::default();
    for version in [Some(1), None] {
        let saved = json!({"tools": {"useShiftKeyForToolSwitch": false}});
        let (mut app, store) = app_with_legacy_json(&saved, version);
        tick(&mut app, &ctx);
        assert!(app.session.prefs().tools.use_shift_key_for_tool_switch, "{version:?}");
        app.run("prefs.set", json!({"values": {"performance.historyStates": 12}})).unwrap();
        tick(&mut app, &ctx);
        let v = stored(&store);
        assert_eq!(v["schema_version"], json!(<prefs::SavedPrefs as Settings>::SCHEMA_VERSION), "{version:?}");
        assert_eq!(v["tools"]["useShiftKeyForToolSwitch"], true, "{version:?}");
        app.run("prefs.set", json!({"values": {"tools.useShiftKeyForToolSwitch": false}})).unwrap();
        tick(&mut app, &ctx);
        let mut next = app_on(&store);
        tick(&mut next, &ctx);
        assert!(!next.session.prefs().tools.use_shift_key_for_tool_switch, "{version:?}: turned off, it stays off");
    }
}

#[test]
fn failed_preference_saves_are_retried() {
    let (mut app, files) = app_with_store();
    let ctx = egui::Context::default();
    tick(&mut app, &ctx);
    files.fail_writes(true);
    app.run("prefs.set", json!({"values": {"interface.darkTheme": "craft.studio"}})).unwrap();
    // The trial passes the store's backoff as an `Instant` instead of sleeping.
    let start = Instant::now();
    super::save::tick(&mut app, &ctx, start);
    assert!(crate::notices::shown_titled(&app, SAVE_NOTICE), "a failed write is reported");
    assert!(saved_text(&files).is_none_or(|text| !text.contains("craft.studio")));
    // A second failure keeps the one notice.
    super::save::tick(&mut app, &ctx, start + retry_delay(1));
    assert!(crate::notices::shown_titled(&app, SAVE_NOTICE));
    // Storage works again: the pending change is written once the backoff has passed, with no new edit.
    files.fail_writes(false);
    super::save::tick(&mut app, &ctx, start + retry_delay(1) + retry_delay(2));
    assert_eq!(stored(&files)["interface"]["darkTheme"], "craft.studio");
    assert!(!crate::notices::shown_titled(&app, SAVE_NOTICE), "the notice goes once preferences are saved");
}

#[test]
fn preferences_persist_through_services_and_theme_follows() {
    let (mut app, store) = app_with_store();
    let ctx = egui::Context::default();
    tick(&mut app, &ctx);
    app.run("prefs.set", json!({"values": {"interface.lightTheme": "craft.studio", "interface.appearanceMode": "light", "performance.historyStates": 12}}))
        .unwrap();
    tick(&mut app, &ctx);
    assert_eq!(app.ui.theme, AppTheme::studio_light());
    assert_eq!(stored(&store)["performance"]["historyStates"], 12);
    // A new app instance loads them.
    let mut app2 = app_on(&store);
    tick(&mut app2, &ctx);
    assert_eq!(app2.session.prefs().performance.history_states, 12);
    assert_eq!(app2.ui.theme, AppTheme::studio_light());
    // Picking a theme from the Window menu becomes the preference.
    pick_theme(&mut app2, &ctx, &ThemeChoice::Theme { id: AppTheme::classic().id().clone(), dark: false });
    tick(&mut app2, &ctx);
    assert_eq!(app2.session.prefs().interface.light_theme, "craft.classic");
    assert_eq!(stored(&store)["interface"]["lightTheme"], "craft.classic");
}

// --- scale and fonts -----------------------------------------------------------------------------------------------

#[test]
fn auto_scale_preserves_system_scale() {
    use prefs::UiScale::Auto;
    // Whatever the monitor, 4K included: an unscaled desktop stays at 100% (#1278), and a
    // fractional or below-100% system scale is the user's choice (#1072).
    for dpi in [0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0] {
        assert_eq!(display_scale(Auto, Some(dpi)), dpi);
    }
    assert_eq!(display_scale(Auto, None), 1.0);
    assert_eq!(display_scale(Auto, Some(f32::NAN)), 1.0);
    assert_eq!(display_scale(Auto, Some(-2.0)), 1.0);
}

#[test]
fn scale_preferences_and_monitor_changes_apply_live() {
    let (mut app, _) = app_with_store();
    let ctx = egui::Context::default();
    {
        let mut step = |physical: egui::Vec2, native: f32, expected: f32| {
            let mut input = egui::RawInput::default();
            let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
            viewport.native_pixels_per_point = Some(native);
            viewport.monitor_size = Some(physical / (native * ctx.zoom_factor()));
            ctx.run_ui(input, |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
            // Scale changes take effect on the following pass.
            let mut input = egui::RawInput::default();
            let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
            viewport.native_pixels_per_point = Some(native);
            viewport.monitor_size = Some(physical / (native * ctx.zoom_factor()));
            ctx.run_ui(input, |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
            assert!((ctx.pixels_per_point() - expected).abs() < 1e-4);
        };
        for _ in 0..4 {
            step(vec2(3840.0, 2160.0), 1.0, 1.0);
        }
        step(vec2(1920.0, 1080.0), 1.0, 1.0);
        step(vec2(3840.0, 2160.0), 1.5, 1.5);
        step(vec2(3840.0, 2160.0), 1.25, 1.25);
        step(vec2(2560.0, 1440.0), 1.75, 1.75);
        step(vec2(3840.0, 2160.0), 1.0, 1.0);
    }
    for (pref, expected) in [
        ("200", 2.0),
        ("125", 1.25),
        ("150", 1.5),
        ("100", 1.0),
        ("105", 1.05),
        ("110", 1.1),
        ("115", 1.15),
        ("120", 1.2),
        ("95", 0.95),
        ("90", 0.9),
        ("85", 0.85),
        ("80", 0.8),
        ("auto", 1.5),
    ] {
        app.run("prefs.set", json!({"values": {"interface.uiScale": pref}})).unwrap();
        let mut input = egui::RawInput::default();
        input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap().native_pixels_per_point = Some(1.5);
        ctx.run_ui(input.clone(), |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
        ctx.run_ui(input, |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
        assert!((ctx.pixels_per_point() - expected).abs() < 1e-4);
    }
}

#[test]
fn intermediate_ui_scale_choices_can_be_applied_from_preferences() {
    let (mut app, store) = app_with_store();
    let ctx = egui::Context::default();
    app.run("prefs.set", json!({"path": "interface.uiScale", "value": "100"})).unwrap();
    open_prefs(&mut app, "interface");
    for value in ["105", "110", "115", "120"] {
        edit(&mut app, "interface.uiScale", PrefValue::Choice(value.into()));
        apply_window(&mut app).unwrap();
        tick(&mut app, &ctx);
        assert_eq!(app.session.prefs().interface.ui_scale.name(), value);
        assert_eq!(stored(&store)["interface"]["uiScale"], value);
        assert!(window_open(&app), "Apply keeps the window open");
    }
}

#[test]
fn ui_font_size_commands_follow_themes_and_dpi_and_reset_live() {
    let (mut app, _) = app_with_store();
    let ctx = egui::Context::default();
    PhotocraftApp::setup_context(&ctx, AppTheme::pro());
    let mut input = egui::RawInput::default();
    input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap().native_pixels_per_point = Some(1.5);
    for (theme, scale, expected) in
        [("pro", "100", 1.0), ("studio", "auto", 1.5), ("studioLight", "200", 2.0), ("proMedium", "100", 1.0), ("classic", "auto", 1.5)]
    {
        app.run("prefs.set", json!({"values": {"interface.uiFontSize": "large", "interface.theme": theme, "interface.uiScale": scale}})).unwrap();
        for _ in 0..3 {
            ctx.run_ui(input.clone(), |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
        }
        assert!((ctx.pixels_per_point() - expected).abs() < 1e-4);
        assert_eq!(ctx.fonts(|f| f.definitions().font_data["Inter"].tweak.scale), 16.0 / 12.0);
    }
    app.run("prefs.reset", json!({"path": "interface.uiFontSize"})).unwrap();
    for _ in 0..2 {
        ctx.run_ui(input.clone(), |ui| tick(&mut app, ui.ctx())).textures_delta.clear();
    }
    assert_eq!(ctx.fonts(|f| f.definitions().font_data["Inter"].tweak.scale), 1.0);
    assert!((ctx.pixels_per_point() - 1.5).abs() < 1e-4, "resetting font size keeps UI Scale");
}

// --- remembered state -----------------------------------------------------------------------------------------------

/// One case of a remembered-state table: what is set (or seeded) in the first app, and what holds after a restart
/// on the same store.
struct Case {
    name: &'static str,
    /// A hand-written file the first app starts on.
    seed: Option<Value>,
    /// State set in the first app; the table ticks afterwards.
    set: Box<dyn Fn(&mut PhotocraftApp, &egui::Context)>,
    /// What holds in the restarted app, given the file's contents.
    check: Box<dyn Fn(&mut PhotocraftApp, &egui::Context, &Value)>,
}

fn run_cases(cases: Vec<Case>) {
    for case in cases {
        eprintln!("case: {}", case.name);
        let files = Arc::new(MemoryFiles::new());
        if let Some(seed_value) = &case.seed {
            seed(&files, seed_value, Some(<prefs::SavedPrefs as Settings>::SCHEMA_VERSION));
        }
        let mut app = app_on(&files);
        let ctx = app.ctx().clone();
        tick(&mut app, &ctx);
        (case.set)(&mut app, &ctx);
        tick(&mut app, &ctx);
        let mut restarted = app_on(&files);
        tick(&mut restarted, &ctx);
        (case.check)(&mut restarted, &ctx, &stored(&files));
    }
}

/// A case that only seeds a file and checks what the app loads from it.
fn loaded(name: &'static str, seed: Value, check: impl Fn(&PhotocraftApp) + 'static) -> Case {
    Case { name, seed: Some(seed), set: Box::new(|_, _| {}), check: Box::new(move |app, _, _| check(app)) }
}

fn restart_cases() -> Vec<Case> {
    use crate::state::Tool;
    let mem = photocraft_engine::preset_store::MemBackend::default();
    let (early, late) = (mem.clone(), mem.clone());
    vec![
        Case {
            name: "recent files and their count",
            seed: None,
            set: Box::new(|app, _| {
                app.push_recent("/work/a.psd");
                app.push_recent("/work/b.png");
            }),
            check: Box::new(|app, ctx, _| {
                // The list comes back, newest first.
                assert_eq!(app.ui.recent_files, vec!["/work/b.png".to_string(), "/work/a.psd".to_string()]);
                // Lowering "Recent File List Contains" shortens the menu at once; 0 turns it off.
                app.run("prefs.set", json!({"values": {"fileHandling.recentFileCount": 1}})).unwrap();
                tick(app, ctx);
                assert_eq!(app.ui.recent_files, vec!["/work/b.png".to_string()]);
                app.push_recent("/work/c.tif");
                assert_eq!(app.session.prefs().file_handling.recent_files, vec!["/work/c.tif".to_string()]);
                app.run("prefs.set", json!({"values": {"fileHandling.recentFileCount": 0}})).unwrap();
                tick(app, ctx);
                app.push_recent("/work/d.tif");
                assert!(app.ui.recent_files.is_empty());
                // Clearing the list in Preferences (or by an agent) reaches the menu.
                app.run("prefs.set", json!({"values": {"fileHandling.recentFileCount": 20, "fileHandling.recentFiles": ["/x.psd"]}})).unwrap();
                tick(app, ctx);
                assert_eq!(app.ui.recent_files, vec!["/x.psd".to_string()]);
                // A hostile count from a hand-edited preferences file is capped, not trusted.
                app.run("prefs.set", json!({"values": {"fileHandling.recentFileCount": 100}})).unwrap();
                app.session.prefs.edit(|p| p.file_handling.recent_file_count = u32::MAX);
                assert_eq!(app.recent_cap(), 100);
            }),
        },
        Case {
            name: "selected tool",
            seed: None,
            set: Box::new(|app, ctx| {
                // Start from a non-default tool so returning to Move is an explicit change that gets persisted.
                for tool in [Tool::Brush, Tool::Eraser] {
                    app.ui.tool = tool;
                    crate::tool_memory::persist(app, ctx);
                    tick(app, ctx);
                }
            }),
            check: Box::new(|app, _, stored| {
                assert_eq!(stored["selectedTool"], json!("Eraser"));
                assert_eq!(app.ui.tool, Tool::Eraser, "restored before the first frame");
                assert_eq!(app.ui.group_tool(Tool::Eraser.key()), Some(Tool::Eraser));
                assert_eq!(app.ui.tool_options, crate::state::ToolOptions::default());
            }),
        },
        Case {
            name: "selecting the default tool overwrites a remembered one",
            seed: None,
            set: Box::new(|app, ctx| {
                for tool in [Tool::Brush, Tool::Move] {
                    app.ui.tool = tool;
                    crate::tool_memory::persist(app, ctx);
                    tick(app, ctx);
                }
            }),
            check: Box::new(|app, _, stored| {
                assert_eq!(stored["selectedTool"], json!("Move"));
                assert_eq!(app.ui.tool, Tool::Move);
            }),
        },
        Case {
            // #3569: a restart keeps the last tool of every group, not only the selected tool's.
            name: "group tools",
            seed: None,
            set: Box::new(|app, ctx| {
                for tool in [Tool::Brush, Tool::Pencil, Tool::PaintBucket, Tool::PolygonLasso, Tool::Pencil, Tool::Zoom] {
                    app.ui.tool = tool;
                    crate::tool_memory::persist(app, ctx);
                    tick(app, ctx);
                }
            }),
            check: Box::new(|app, _, stored| {
                let expected = [Tool::Zoom, Tool::Pencil, Tool::PolygonLasso, Tool::PaintBucket, Tool::Brush, Tool::Move];
                assert_eq!(stored["groupTools"], json!(["Zoom", "Pencil", "PolygonLasso", "PaintBucket", "Brush", "Move"]), "most recent first, each once");
                assert_eq!(app.ui.tool, Tool::Zoom);
                assert_eq!(app.ui.group_tools, expected);
                assert_eq!(app.ui.group_tool('B'), Some(Tool::Pencil), "the group's latest tool");
                assert_eq!(app.ui.group_tool('G'), Some(Tool::PaintBucket));
                assert_eq!(app.ui.group_tool('L'), Some(Tool::PolygonLasso));
            }),
        },
        Case {
            name: "tool options",
            seed: None,
            set: Box::new(|app, ctx| {
                app.ui.tool_options.move_auto_select = false;
                app.ui.tool_options.feather = 12.0;
                app.ui.tool_options.transform.interpolation = Some(Interpolation::NearestNeighbor);
                crate::tool_memory::persist(app, ctx);
            }),
            check: Box::new(|app, _, stored| {
                assert_eq!(stored["toolOptions"]["transform"]["interpolation"], "nearestNeighbor");
                assert!(!app.ui.tool_options.move_auto_select);
                assert_eq!(app.ui.tool_options.feather, 12.0);
                assert_eq!(app.ui.tool_options.transform.interpolation, Some(Interpolation::NearestNeighbor));
                assert_eq!(app.ui.tool_options.tolerance, crate::state::ToolOptions::default().tolerance, "only what differs is stored");
            }),
        },
        Case {
            // #6212: every painting tool keeps its own brush, and the session brush comes back as the brush of the
            // tool that had it.
            name: "tool brushes",
            seed: None,
            set: Box::new(|app, ctx| {
                let set_size = |app: &mut PhotocraftApp, size: f32| {
                    app.session.tools.brush.size = size;
                    app.session.tools.brush.opacity = size / 100.0;
                    app.session.tools.brush_changed();
                };
                set_size(app, 77.0);
                app.ui.tool = Tool::Eraser;
                crate::paint_mouse::sync_tool_brush(app);
                set_size(app, 33.0);
                app.ui.tool = Tool::CloneStamp;
                crate::paint_mouse::sync_tool_brush(app);
                set_size(app, 12.0);
                app.ui.tool = Tool::Eraser;
                crate::paint_mouse::sync_tool_brush(app);
                crate::last_brush::persist(app, ctx);
                crate::tool_memory::persist(app, ctx);
            }),
            check: Box::new(|app, _, stored| {
                assert_eq!(stored["toolBrushes"]["owner"], json!("Eraser"));
                assert_eq!(stored["toolBrushes"]["brushes"]["Brush"]["size"], json!(77.0));
                assert_eq!(stored["toolBrushes"]["brushes"]["CloneStamp"]["size"], json!(12.0));
                assert!(stored["toolBrushes"]["brushes"].get("Eraser").is_none(), "the owner's brush is the last brush");
                assert_eq!((app.ui.tool, app.ui.brush_tool), (Tool::Eraser, Tool::Eraser));
                assert_eq!(app.session.tools.brush.size, 33.0, "the Eraser's brush is the session brush");
                for (tool, size) in [(Tool::Brush, 77.0), (Tool::CloneStamp, 12.0), (Tool::Eraser, 33.0)] {
                    app.ui.tool = tool;
                    crate::paint_mouse::sync_tool_brush(app);
                    assert_eq!(app.session.tools.brush.size, size, "{tool:?}");
                    assert_eq!(app.session.tools.brush.opacity, size / 100.0, "{tool:?}");
                }
            }),
        },
        Case {
            name: "untouched View items store nothing",
            seed: None,
            set: Box::new(|app, ctx| crate::view_cmds::persist(app, ctx)),
            check: Box::new(|_, _, stored| {
                // So later default changes still reach the user.
                assert!(stored.get("viewExtras").is_none_or(Value::is_null));
            }),
        },
        Case {
            name: "View Show items",
            seed: None,
            set: Box::new(|app, ctx| {
                // The menu commands, as a user toggles them (#3762: Pixel Grid off).
                for id in ["view.show.pixelGrid", "view.show.grid", "view.snapTo.layers", "view.extras"] {
                    crate::menus::invoke(app, ctx, id, json!({})).unwrap();
                }
                crate::view_cmds::persist(app, ctx);
            }),
            check: Box::new(|app, _, stored| {
                assert_eq!(stored["viewExtras"], json!({"extras": false, "grid": true, "show": {"pixelGrid": false}, "snapTo": {"layers": false}}));
                let v = &app.ui.view;
                assert!(!v.extras && !v.show.pixel_grid && !v.snap_to.layers && app.ui.extras.grid);
                assert!(v.show.selection_edges && v.snap_to.guides && app.ui.extras.guides, "the rest keep their defaults");
            }),
        },
        Case {
            name: "View snap off",
            seed: None,
            set: Box::new(|app, ctx| {
                assert!(app.ui.extras.snap, "on by default");
                crate::menus::invoke(app, ctx, "view.snap", json!({})).unwrap();
                assert!(!app.ui.extras.snap);
                crate::view_cmds::persist(app, ctx);
            }),
            check: Box::new(|app, ctx, stored| {
                assert_eq!(stored["viewExtras"], json!({"snap": false}));
                assert!(!app.ui.extras.snap, "still off after a restart");
                crate::menus::invoke(app, ctx, "view.snap", json!({})).unwrap();
                crate::view_cmds::persist(app, ctx);
                assert!(app.session.prefs().view_extras.is_null());
            }),
        },
        Case {
            // #9479: Interface › Enable animations is gone (Photoshop has none). A file saved with it off still
            // loads whole, and egui's fades and scroll glides keep their defaults.
            name: "a saved enable-animations flag",
            seed: Some(json!({"interface": {"enableAnimations": false, "showTooltips": false}})),
            set: Box::new(|_, _| {}),
            check: Box::new(|app, ctx, _| {
                let style = egui::Style::default();
                let both = |ctx: &egui::Context| {
                    [egui::Theme::Dark, egui::Theme::Light].map(|t| (ctx.style_of(t).animation_time, ctx.style_of(t).scroll_animation.duration.max))
                };
                let expected = (style.animation_time, style.scroll_animation.duration.max);
                assert!(expected.0 > 0.0 && expected.1 > 0.0);
                assert!(app.ui.status.is_empty(), "{}", app.ui.status);
                assert!(!app.session.prefs().interface.show_tooltips, "the rest of the file loads");
                assert_eq!(both(ctx), [expected, expected]);
                app.apply_theme(ctx, crate::theme::AppTheme::studio_light());
                tick(app, ctx);
                assert_eq!(both(ctx), [expected, expected], "after a theme change");
                assert!(app.run("prefs.set", json!({"values": {"interface.enableAnimations": true}})).is_err());
            }),
        },
        Case {
            name: "brush presets made before the store attached",
            seed: None,
            set: Box::new(move |app, ctx| {
                let (tx, rx) = std::sync::mpsc::channel();
                app.services.preset_store = Some(rx);
                // Still loading: presets made now are kept and written once the store arrives.
                tick(app, ctx);
                app.run("brush.presets.save", json!({"name": "Early"})).unwrap();
                assert!(early.files.lock().unwrap().is_empty());
                tx.send(photocraft_engine::preset_store::open(Box::new(early.clone()))).unwrap();
                tick(app, ctx);
                assert!(app.services.preset_store.is_none() && app.session.preset_store.is_some());
                app.run("brush.presets.save", json!({"name": "Late"})).unwrap();
            }),
            check: Box::new(move |_, _, _| {
                let mut session = photocraft_engine::Session::new();
                assert!(session.attach_preset_store(photocraft_engine::preset_store::open(Box::new(late.clone()))).is_empty());
                for n in ["Early", "Late"] {
                    assert!(session.tools.presets.iter().any(|p| p.name == n), "{n}");
                }
            }),
        },
        Case {
            name: "tool colours",
            seed: None,
            set: Box::new(|app, ctx| {
                app.run("tools.setColors", json!({"foreground": [0.2, 0.4, 0.6, 0.8], "background": [0.9, 0.7, 0.3, 1.0]})).unwrap();
                crate::tool_memory::persist(app, ctx);
            }),
            check: Box::new(|app, ctx, _| {
                assert_eq!(app.session.tools.foreground, [0.2, 0.4, 0.6, 0.8]);
                assert_eq!(app.session.tools.background, [0.9, 0.7, 0.3, 1.0]);
                let rev = app.session.prefs.rev();
                crate::tool_memory::persist(app, ctx);
                assert_eq!(app.session.prefs.rev(), rev, "unchanged colours do not rewrite preferences");
            }),
        },
    ]
}

#[test]
fn remembered_state_survives_a_restart() {
    run_cases(restart_cases());
}

fn damaged_cases() -> Vec<Case> {
    use crate::state::Tool;
    let d = crate::state::ToolOptions::default;
    let mut cases = Vec::new();
    // The file is TOML, which has no null: a damaged value is of the wrong type, or out of range.
    for bad in [json!("FutureTool"), json!(42), json!({}), json!([])] {
        cases.push(loaded("selected tool", json!({"selectedTool": bad, "toolOptions": {"feather": 12.0}}), |app| {
            assert_eq!(app.ui.tool, Tool::Move);
            assert_eq!(app.ui.tool_options.feather, 12.0);
        }));
    }
    for bad in [json!("Pencil"), json!(42), json!({"B": "Pencil"})] {
        cases.push(loaded("group tools", json!({"groupTools": bad, "toolOptions": {"feather": 12.0}}), |app| {
            assert_eq!(app.ui.group_tools, [Tool::Move]);
            assert_eq!(app.ui.tool_options.feather, 12.0);
        }));
    }
    cases.push(loaded("group tools that partly read back", json!({"groupTools": ["FutureTool", "Pencil", 7, {}, "PaintBucket", "Pencil"]}), |app| {
        assert_eq!(app.ui.group_tools, [Tool::Move, Tool::Pencil, Tool::PaintBucket]);
    }));
    // #6212: a value its options-bar field could never hold is clamped to the field's range, or takes the default.
    cases.push(loaded(
        "tool options out of range",
        json!({"toolOptions": {
            "feather": 1e30, "tolerance": -5.0, "polygon_sides": 0, "eyedropper_size": 7,
            "crop": {"guideOrientation": 9}, "fill_opacity": 0.0, "magnetic": {"width": 1e9},
            "pattern_stamp_angle": -720.0, "marquee_width": 1e7, "shape_stroke": {"miter_limit": -1.0, "opacity": 5.0},
        }}),
        move |app| {
            let o = &app.ui.tool_options;
            let d = d();
            assert_eq!((o.feather, o.tolerance, o.polygon_sides, o.eyedropper.size), (1000.0, 0.0, 3, d.eyedropper.size));
            assert_eq!((o.crop.guide_orientation, o.fill_opacity, o.magnetic.width), (d.crop.guide_orientation, 1.0, 256.0));
            assert_eq!((o.pattern_stamp_angle, o.marquee_width), (-180.0, 999.0));
            assert_eq!(o.shape_stroke, d.shape_stroke, "an invalid stroke style takes the default");
        },
    ));
    cases.push(loaded("tool options: a good value is left alone", json!({"toolOptions": {"feather": 12.5}}), move |app| {
        assert_eq!(app.ui.tool_options, crate::state::ToolOptions { feather: 12.5, ..d() });
        let mut def = d();
        crate::tool_memory::sanitize(&mut def);
        assert_eq!(def, d(), "in-range defaults never change");
    }));
    for (bad, tolerance) in [(json!("junk"), 32.0), (json!({"move_auto_select": "no", "feather": 1e300, "tolerance": 5.0, "unknown": 1}), 5.0)] {
        cases.push(loaded("tool options of the wrong type", json!({"toolOptions": bad}), move |app| {
            let o = &app.ui.tool_options;
            assert!(o.move_auto_select && o.feather == 0.0);
            assert_eq!(o.tolerance, tolerance);
        }));
    }
    for bad in [json!("junk"), json!(42), json!({"owner": [1], "brushes": "x"}), json!({"brushes": {"Eraser": 5}})] {
        cases.push(loaded("tool brushes", json!({"toolBrushes": bad}), |app| {
            assert_eq!(app.ui.brush_tool, Tool::Brush);
            assert!(app.ui.tool_brushes.is_empty());
        }));
    }
    // What reads back loads, clamped to the options bar's ranges; the rest is skipped and those tools start from the brush.
    cases.push(loaded(
        "tool brushes out of range",
        json!({"toolBrushes": {"owner": "Move", "brushes": {
            "Eraser": {"size": 1e30}, "Bogus": {"size": 5.0}, "Move": {"size": 5.0},
            "Smudge": {"size": -3.0, "opacity": 50.0, "flow": -2.0, "hardness": 1e30},
            "Brush": {"size": 9.0},
        }}}),
        |app| {
            assert_eq!(app.ui.brush_tool, Tool::Brush, "the Move tool has no brush");
            let tools: Vec<Tool> = app.ui.tool_brushes.iter().map(|(t, _)| *t).collect();
            assert_eq!(tools, vec![Tool::Smudge], "too large, unknown, brushless and the owner's own are skipped");
            let b = &app.ui.tool_brushes[0].1;
            assert_eq!((b.size, b.opacity, b.flow, b.hardness), (1.0, 1.0, 0.0, 1.0));
        },
    ));
    // CMYK picks: the table is bounded, and malformed or out-of-range entries are dropped.
    let limit = photocraft_engine::ToolState::CMYK_PICKS;
    let mut picks: Vec<Value> = (0..limit + 3).map(|i| json!([[i as f32 / 16.0, 0.2, 0.3], [0.1, 0.2, 0.3, 0.4]])).collect();
    picks.extend([json!("bad"), json!([[0, 0, 0], [1, 0, 0]]), json!([[0, 0, 0], [2, 0, 0, 0]]), json!([[1e100, 0, 0], [1, 0, 0, 0]])]);
    cases.push(loaded("CMYK picks", json!({"toolColors": {"foreground": [0.2, 0.4, 0.6, 1.0], "cmykPicks": picks}}), move |app| {
        assert_eq!(app.session.tools.foreground, [0.2, 0.4, 0.6, 1.0]);
        assert_eq!(app.session.tools.cmyk_picks.len(), limit);
        assert_eq!(app.session.tools.picked_cmyk([0.0, 0.2, 0.3]), None);
        assert_eq!(app.session.tools.picked_cmyk([(limit + 2) as f32 / 16.0, 0.2, 0.3]), Some([0.1, 0.2, 0.3, 0.4]));
    }));
    for bad in [json!("red"), json!([1, 0, 0]), json!([1e100, 0, 0, 1]), json!([0, "bad", 0, 1])] {
        cases.push(loaded(
            "tool colour swatch",
            json!({"toolColors": {"foreground": bad, "background": [0.2, 0.4, 0.6, 1.0]}, "toolOptions": {"feather": 12.0}}),
            |app| {
                assert_eq!(app.session.tools.foreground, [0.0, 0.0, 0.0, 1.0]);
                assert_eq!(app.session.tools.background, [0.2, 0.4, 0.6, 1.0]);
                assert_eq!(app.ui.tool_options.feather, 12.0);
            },
        ));
    }
    cases.push(loaded("no tool colours saved", json!({"toolOptions": {"feather": 12.0}}), |app| {
        assert_eq!(app.session.tools.foreground, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(app.session.tools.background, [1.0; 4]);
    }));
    // A tool-colour preset with an invalid channel is refused and leaves the remembered colours alone.
    for bad_channel in [json!(1e100), json!("red"), json!(true), json!({}), json!([])] {
        cases.push(Case {
            name: "tool colours after an invalid preset",
            seed: None,
            set: Box::new(move |app, _| {
                app.run("tools.setColors", json!({"foreground": [0.2, 0.4, 0.6, 0.8]})).unwrap();
                crate::tool_memory::save_on_exit(app).unwrap();
                app.run("tool.presets.new", json!({"name": "Invalid", "tool": "move", "options": {"foreground": [bad_channel.clone(), 0.4, 0.6, 1.0]}}))
                    .unwrap();
                assert!(app.run("tool.presets.select", json!({"preset": "Invalid"})).is_err());
                crate::tool_memory::save_on_exit(app).unwrap();
            }),
            check: Box::new(|app, _, stored| {
                // The file holds the f32 colour widened to f64.
                assert_eq!(stored["toolColors"]["foreground"], json!([0.2_f32 as f64, 0.4_f32 as f64, 0.6_f32 as f64, 0.8_f32 as f64]));
                assert_eq!(app.session.tools.foreground, [0.2, 0.4, 0.6, 0.8]);
            }),
        });
    }
    cases
}

#[test]
fn a_damaged_remembered_value_resets_only_itself() {
    run_cases(damaged_cases());
}

#[test]
fn selected_tool_is_flushed_on_exit_without_another_frame() {
    let (mut app, store) = app_with_store();
    app.ui.tool = crate::state::Tool::Brush;
    crate::tool_memory::save_on_exit(&mut app).unwrap();
    let app2 = app_on(&store);
    assert_eq!(app2.ui.tool, crate::state::Tool::Brush);
}

#[test]
fn selected_tool_explicit_eraser_survives_exit_during_temporary_switches() {
    use crate::state::Tool;
    for spring in [false, true] {
        for menu in [false, true] {
            let (mut app, store) = app_with_store();
            app.stylus.tool_before_eraser = Some(Tool::Zoom);
            app.ui.tool = Tool::Eraser;
            if spring {
                crate::spring_tools::pressed(&mut app, 'E', Tool::Zoom, true, 0.0);
            }
            if menu {
                crate::menus::invoke(&mut app, &egui::Context::default(), "tools.select.eraser", json!({})).unwrap();
            } else {
                crate::tool_memory::select(&mut app, Tool::Eraser);
            }
            assert_eq!(app.stylus.tool_before_eraser, None);
            assert!(app.ui.spring.is_none());
            crate::tool_memory::save_on_exit(&mut app).unwrap();
            let restored = app_on(&store);
            assert_eq!(restored.ui.tool, Tool::Eraser);
        }
    }
}

#[test]
fn selected_tool_excludes_temporary_switches() {
    use crate::state::Tool;
    let (mut app, store) = app_with_store();
    let ctx = egui::Context::default();
    app.ui.tool = Tool::Brush;
    crate::tool_memory::persist(&mut app, &ctx);
    tick(&mut app, &ctx);
    app.ui.tool = Tool::Zoom;
    crate::spring_tools::pressed(&mut app, 'Z', Tool::Brush, true, 0.0);
    crate::tool_memory::persist(&mut app, &ctx);
    tick(&mut app, &ctx);
    assert_eq!(stored(&store)["selectedTool"], json!("Brush"));
    // A quick tap commits the choice once the letter is up (its command's release) and spring_tools clears the switch.
    crate::spring_tools::released(&mut app, 'Z');
    crate::spring_tools::tick(&mut app, &ctx);
    crate::tool_memory::persist(&mut app, &ctx);
    tick(&mut app, &ctx);
    assert_eq!(stored(&store)["selectedTool"], json!("Zoom"));
    app.stylus.tool_before_eraser = Some(Tool::Zoom);
    app.ui.tool = Tool::Eraser;
    crate::tool_memory::persist(&mut app, &ctx);
    tick(&mut app, &ctx);
    assert_eq!(stored(&store)["selectedTool"], json!("Zoom"));
    // An explicit selection during a temporary switch takes precedence.
    app.ui.tool = Tool::PolygonLasso;
    crate::tool_memory::persist(&mut app, &ctx);
    tick(&mut app, &ctx);
    assert_eq!(stored(&store)["selectedTool"], json!("PolygonLasso"));
}

#[test]
fn group_tools_are_flushed_on_exit_without_another_frame() {
    use crate::state::Tool;
    let (mut app, store) = app_with_store();
    app.ui.tool = Tool::Pencil;
    crate::tool_memory::persist(&mut app, &egui::Context::default());
    app.ui.tool = Tool::PaintBucket;
    crate::tool_memory::save_on_exit(&mut app).unwrap();
    let app2 = app_on(&store);
    assert_eq!(app2.ui.group_tool('B'), Some(Tool::Pencil));
    assert_eq!(app2.ui.group_tool('G'), Some(Tool::PaintBucket));
}

/// A frame as far as the tools go: the key handling, then what is remembered.
fn tool_frame(app: &mut PhotocraftApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    ctx.begin_pass(egui::RawInput { events, ..Default::default() });
    crate::shortcuts::handle(app, ctx);
    ctx.end_pass().textures_delta.clear();
    crate::tool_memory::persist(app, ctx);
    tick(app, ctx);
}

/// A tool picked during a drag that ends on the tool it began with: the selected tool never
/// changed, but that tool's group did.
#[test]
fn group_tools_are_saved_when_the_selected_tool_stays_the_same() {
    use crate::state::Tool;
    let (mut app, store) = app_with_store();
    let ctx = egui::Context::default();
    let pointer = |pressed| egui::Event::PointerButton { pos: egui::Pos2::ZERO, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
    app.ui.tool = Tool::Brush;
    tool_frame(&mut app, &ctx, vec![]);
    tool_frame(&mut app, &ctx, vec![pointer(true)]);
    app.ui.tool = Tool::PaintBucket;
    tool_frame(&mut app, &ctx, vec![]);
    app.ui.tool = Tool::Brush;
    tool_frame(&mut app, &ctx, vec![]);
    tool_frame(&mut app, &ctx, vec![pointer(false)]);
    assert_eq!(stored(&store)["selectedTool"], json!("Brush"));
    let app2 = app_on(&store);
    assert_eq!(app2.ui.group_tool('G'), Some(Tool::PaintBucket));
}

/// The pen's eraser end and a held tool letter don't change what a group shows or brings back,
/// neither during the session nor when quitting while they are in use.
#[test]
fn group_tools_exclude_temporary_switches() {
    use crate::state::Tool;
    let (mut app, store) = app_with_store();
    let ctx = egui::Context::default();
    for tool in [Tool::MagicEraser, Tool::Brush] {
        app.ui.tool = tool;
        tool_frame(&mut app, &ctx, vec![]);
    }
    app.stylus.tool_before_eraser = Some(Tool::Brush);
    app.ui.tool = Tool::Eraser;
    tool_frame(&mut app, &ctx, vec![]);
    assert_eq!(app.ui.group_tool('E'), Some(Tool::MagicEraser), "the pen's eraser end");
    app.stylus.tool_before_eraser = None;
    app.ui.tool = Tool::Brush;
    // The window host holds Z down; the app's own frames route it as the tool command and keep the letter held.
    craft::ui::runtime::windows::WindowHost::of(app.ctx()).inject_key(egui::ViewportId::ROOT, craft::keymap::Key::Z, craft::keymap::Mods::default(), true);
    for _ in 0..3 {
        crate::canvas::testing::frame(&mut app);
    }
    tool_frame(&mut app, &ctx, vec![]);
    assert_eq!(app.ui.tool, Tool::Zoom, "Z is still down");
    assert_eq!(app.ui.group_tool('Z'), None, "a held letter");
    crate::tool_memory::save_on_exit(&mut app).unwrap();
    assert_eq!(stored(&store)["groupTools"], json!(["Brush", "MagicEraser", "Move"]));
}

/// A selected tool this version doesn't know starts on the default tool. The saved list stays as
/// it is, names this version doesn't know included, until the next tool is picked.
#[test]
fn a_launch_alone_saves_no_group_tools() {
    use crate::state::Tool;
    let saved = json!({"selectedTool": "FutureTool", "groupTools": ["Pencil", "FutureTool", "PaintBucket"]});
    let (mut app, store) = app_with_json(&saved);
    let ctx = egui::Context::default();
    tool_frame(&mut app, &ctx, vec![]);
    assert_eq!(stored(&store)["groupTools"], saved["groupTools"]);
    app.ui.tool = Tool::Brush;
    tool_frame(&mut app, &ctx, vec![]);
    assert_eq!(stored(&store)["groupTools"], json!(["Brush", "Move", "Pencil", "PaintBucket"]));
}

/// #6212: `toolBrushes.owner` names the tool whose brush `last_brush` holds. A brush that
/// can't be remembered yet (a sampled tip too big to embed) leaves `last_brush` at the old
/// owner's brush, so the old owner stays saved: the next launch gives that brush back to it,
/// not to the tool selected since.
#[test]
fn the_saved_brush_owner_follows_the_saved_last_brush() {
    use crate::state::Tool;
    use photocraft_engine::paint::{GrayTile, TipShape};
    let (mut app, store) = app_with_store();
    let ctx = egui::Context::default();
    tick(&mut app, &ctx);
    app.session.tools.brush.size = 77.0;
    app.session.tools.brush_changed();
    crate::last_brush::persist(&mut app, &ctx);
    crate::tool_memory::persist(&mut app, &ctx);
    // The Brush's tip becomes a big sampled one, then the Blur tool takes over that brush (the Eraser has its own
    // starting tip since #11936).
    app.session.tools.brush.tip = TipShape::Sampled(GrayTile::from_fn(400, 300, |x, y| ((x * y) % 7) as f32 / 7.0));
    app.session.tools.brush_changed();
    app.ui.tool = Tool::Blur;
    crate::paint_mouse::sync_tool_brush(&mut app);
    crate::last_brush::persist(&mut app, &ctx);
    crate::tool_memory::persist(&mut app, &ctx);
    tick(&mut app, &ctx);
    let saved = stored(&store);
    assert_eq!(saved["lastBrush"]["brush"]["size"], json!(77.0), "the big tip is not remembered yet");
<<<<<<< HEAD
    assert_ne!(saved["toolBrushes"]["owner"], json!("Eraser"), "{saved}");
    let mut app2 = app_on(&store);
=======
    assert_ne!(saved["toolBrushes"]["owner"], json!("Blur"), "{saved}");
    let (mut app2, _) = app_with_saved(store.lock().unwrap().clone());
>>>>>>> upstream/main
    tick(&mut app2, &ctx);
    assert_eq!(app2.ui.brush_tool, Tool::Brush, "the 77 px brush is the Brush's");
    assert_eq!(app2.session.tools.brush.size, 77.0);

    // Once the Blur tool's brush is saved, the Blur tool owns it in the preferences too.
    app.session.tools.brush.tip = TipShape::Round;
    app.session.tools.brush.size = 33.0;
    app.session.tools.brush_changed();
    crate::last_brush::persist(&mut app, &ctx);
    crate::tool_memory::persist(&mut app, &ctx);
    tick(&mut app, &ctx);
    let saved = stored(&store);
    assert_eq!(saved["toolBrushes"]["owner"], json!("Blur"), "{saved}");
    assert_eq!(saved["lastBrush"]["brush"]["size"], json!(33.0));
}

/// #6212: Reset Tool forgets that tool's remembered options and brush; Reset All Tools
/// forgets every tool's: each painting tool keeps only its own starting brush (#11810).
#[test]
fn reset_tools_clear_the_remembered_options_and_brushes() {
    use crate::state::Tool;
    let (mut app, store) = app_with_store();
    let ctx = egui::Context::default();
    tick(&mut app, &ctx);
    app.ui.tool_options.feather = 12.0;
    app.ui.tool_options.strength = 80.0;
    app.session.tools.brush.size = 77.0;
    app.session.tools.brush_changed();
    for tool in [Tool::Eraser, Tool::Smudge, Tool::Brush] {
        app.ui.tool = tool;
        crate::paint_mouse::sync_tool_brush(&mut app);
    }
    app.ui.tool = Tool::Smudge;
    crate::paint_mouse::sync_tool_brush(&mut app);
    crate::last_brush::persist(&mut app, &ctx);
    crate::tool_memory::persist(&mut app, &ctx);
    tick(&mut app, &ctx);
    let saved = stored(&store);
    assert_eq!(saved["toolOptions"]["strength"], json!(80.0));
    assert!(saved["toolBrushes"]["brushes"].get("Eraser").is_some() && saved["toolBrushes"]["brushes"].get("Brush").is_some());

    crate::menus::invoke(&mut app, &ctx, crate::tool_reset::RESET, json!({})).unwrap();
    crate::last_brush::persist(&mut app, &ctx);
    crate::tool_memory::persist(&mut app, &ctx);
    tick(&mut app, &ctx);
    let saved = stored(&store);
    assert!(saved["toolOptions"].get("strength").is_none(), "the Smudge's Strength is forgotten");
    assert_eq!(saved["toolOptions"]["feather"], json!(12.0), "the marquee's Feather is not the Smudge's");
    assert!(saved["toolBrushes"]["brushes"].get("Eraser").is_some());

    crate::menus::invoke(&mut app, &ctx, crate::tool_reset::RESET_ALL, json!({"confirmed": true})).unwrap();
    crate::last_brush::persist(&mut app, &ctx);
    crate::tool_memory::persist(&mut app, &ctx);
    tick(&mut app, &ctx);
    let saved = stored(&store);
    assert!(saved["toolOptions"].is_null(), "{saved}");
    let starting =
        |brushes: &[(Tool, photocraft_engine::BrushSettings)]| brushes.iter().all(|(t, b)| *b == crate::paint_mouse::fresh_brush(*t, b) && b.size != 77.0);
    assert!(starting(&app.ui.tool_brushes), "{saved}");
    let app2 = app_on(&store);
    assert_eq!(app2.ui.tool_options, crate::state::ToolOptions::default());
    assert!(!app2.ui.tool_brushes.is_empty() && starting(&app2.ui.tool_brushes), "remembered across a restart");
}

<<<<<<< HEAD
// --- the Preferences window ----------------------------------------------------------------------------------------
=======
#[test]
fn view_show_items_survive_a_restart() {
    let (mut app, store) = app_with_store();
    let ctx = egui::Context::default();
    tick(&mut app, &ctx);
    // Untouched: nothing is stored, so later default changes still reach the user.
    crate::view_cmds::persist(&mut app, &ctx);
    tick(&mut app, &ctx);
    assert!(stored(&store).get("viewExtras").is_none_or(Value::is_null));
    // The menu commands, as a user toggles them (#3762: Pixel Grid off).
    for id in ["view.show.pixelGrid", "view.show.grid", "view.snapTo.layers", "view.extras"] {
        crate::menus::invoke(&mut app, &ctx, id, json!({})).unwrap();
    }
    crate::view_cmds::persist(&mut app, &ctx);
    tick(&mut app, &ctx);
    assert_eq!(stored(&store)["viewExtras"], json!({"extras": false, "grid": true, "show": {"pixelGrid": false}, "snapTo": {"layers": false}}));
    // A restart gets them back.
    let saved = store.lock().unwrap().clone().unwrap();
    let (mut app2, _) = app_with_saved(Some(saved));
    tick(&mut app2, &ctx);
    let v = &app2.ui.view;
    assert!(!v.extras && !v.show.pixel_grid && !v.snap_to.layers && app2.ui.extras.grid);
    assert!(v.show.selection_edges && v.snap_to.guides && app2.ui.extras.guides, "the rest keep their defaults");
    // A hand-edited file: unknown keys and non-booleans keep the defaults, good ones still load.
    for bad in [json!("junk"), json!({"extras": 0, "show": {"pixelGrid": "no", "mesh": true, "bogus": true}, "snapTo": [1]})] {
        let (mut app3, _) = app_with_saved(Some(json!({"viewExtras": bad}).to_string()));
        tick(&mut app3, &ctx);
        let v = &app3.ui.view;
        assert!(v.extras && v.show.pixel_grid && v.snap_to.layers, "{bad}");
        assert_eq!(v.show.mesh, bad.is_object());
    }
}

#[test]
fn view_snap_off_survives_a_restart() {
    let (mut app, store) = app_with_store();
    let ctx = egui::Context::default();
    tick(&mut app, &ctx);
    assert!(app.ui.extras.snap, "on by default");
    crate::menus::invoke(&mut app, &ctx, "view.snap", json!({})).unwrap();
    assert!(!app.ui.extras.snap);
    crate::view_cmds::persist(&mut app, &ctx);
    tick(&mut app, &ctx);
    assert_eq!(stored(&store)["viewExtras"], json!({"snap": false}));
    let saved = store.lock().unwrap().clone().unwrap();
    let (mut app2, _) = app_with_saved(Some(saved));
    tick(&mut app2, &ctx);
    assert!(!app2.ui.extras.snap, "still off after a restart");
    crate::menus::invoke(&mut app2, &ctx, "view.snap", json!({})).unwrap();
    crate::view_cmds::persist(&mut app2, &ctx);
    assert!(app2.session.prefs().view_extras.is_null());
}

#[test]
fn show_tooltips_preferences_turn_tooltips_off() {
    let (mut app, _) = app_with_store();
    let ctx = egui::Context::default();
    tick(&mut app, &ctx);
    assert_eq!(ctx.global_style().interaction.tooltip_delay, crate::theme::TOOLTIP_DELAY);
    for path in ["interface.showTooltips", "tools.showTooltips"] {
        app.run("prefs.set", json!({"values": {path: false}})).unwrap();
        tick(&mut app, &ctx);
        assert!(ctx.global_style().interaction.tooltip_delay.is_infinite(), "{path} off hides tooltips");
        app.run("prefs.set", json!({"values": {path: true}})).unwrap();
        tick(&mut app, &ctx);
        assert_eq!(ctx.global_style().interaction.tooltip_delay, crate::theme::TOOLTIP_DELAY);
    }
}

#[test]
fn a_saved_enable_animations_loads_and_leaves_egui_animating() {
    // #9479: Interface › Enable animations is gone (Photoshop has none). A file saved with it
    // off still loads whole, and egui's fades and scroll glides keep their defaults.
    let style = egui::Style::default();
    let both =
        |ctx: &egui::Context| [egui::Theme::Dark, egui::Theme::Light].map(|t| (ctx.style_of(t).animation_time, ctx.style_of(t).scroll_animation.duration.max));
    let expected = (style.animation_time, style.scroll_animation.duration.max);
    assert!(expected.0 > 0.0 && expected.1 > 0.0);
    for value in [json!(false), json!(true), json!("nope")] {
        let saved = json!({"version": 2, "interface": {"enableAnimations": value, "showTooltips": false}});
        let (mut app, _) = app_with_saved(Some(saved.to_string()));
        let ctx = egui::Context::default();
        tick(&mut app, &ctx);
        assert!(app.ui.status.is_empty(), "{value}: {}", app.ui.status);
        assert!(!app.session.prefs().interface.show_tooltips, "{value}: the rest of the file loads");
        assert_eq!(both(&ctx), [expected, expected], "{value}");
        app.apply_theme(&ctx, crate::theme::ThemeKind::StudioLight);
        tick(&mut app, &ctx);
        assert_eq!(both(&ctx), [expected, expected], "{value}: after a theme change");
        assert!(app.run("prefs.set", json!({"values": {"interface.enableAnimations": true}})).is_err());
    }
}

#[test]
fn unimplemented_preferences_are_hidden_from_the_dialog() {
    let values = prefs::Preferences::default().to_json();
    assert!(has_visible_fields(&values, "general"));
    assert!(has_visible_fields(&values, "fileHandling"));
    assert!(has_visible_fields(&values, "scratchDisks"));
    assert!(!prefs::is_hidden("scratchDisks.disks"));
    // MCP: the agent connection and its folders apply at once (`agents.rs`).
    assert!(has_visible_fields(&values, "integrations"));
    assert!(!prefs::is_hidden("integrations.allowAgentControl"));
    assert!(!prefs::is_hidden("integrations.controlPort"));
    assert!(!prefs::is_hidden("integrations.readRoot"));
    assert!(!prefs::is_hidden("integrations.writeRoot"));
    assert!(!prefs::is_hidden("integrations.connectAgents"));
    // "Fill new type layers with placeholder text" and "Use Escape to Commit" are live; other Type rows stay hidden.
    assert!(has_visible_fields(&values, "type"));
    assert!(!prefs::is_hidden("type.fillNewTypeLayersWithPlaceholder"));
    assert!(!prefs::is_hidden("type.useEscToCommit"));
    assert!(!prefs::is_hidden("type.smartQuotes"));
    assert!(!prefs::is_hidden("type.showFontNamesInEnglish"));
    // Rotate View with Trackpad, the pinch zoom and Enable Gestures are live; the other
    // Enhanced Controls rows stay hidden.
    assert!(has_visible_fields(&values, "enhancedControls"));
    assert!(!prefs::is_hidden("enhancedControls.rotateViewWithTrackpad"));
    assert!(!prefs::is_hidden("enhancedControls.zoomWithTrackpadPinch"));
    assert!(!prefs::is_hidden("enhancedControls.touchGestures"));
    assert!(prefs::is_hidden("enhancedControls.scrubbySliderAcceleration"));
    // Camera Raw Defaults are all live: the dialog's Open applies them (camera_raw_ui).
    assert!(has_visible_fields(&values, "rawDefaults"));
    for k in ["openInCameraRaw", "demosaic", "applyAutoTone", "colorSpace", "bitDepth", "resolution", "sharpenFor", "openAsSmartObject", "writeXmpSidecars"] {
        assert!(!prefs::is_hidden(&format!("rawDefaults.{k}")), "{k}");
    }
    assert!(!prefs::is_hidden("general.autoShowHomeScreen"));
    assert!(!prefs::is_hidden("general.animatedZoom"));
    assert!(!prefs::is_hidden("general.zoomResizesWindows"));
    assert!(!prefs::is_hidden("interface.uiScale"));
    assert!(!prefs::is_hidden("interface.uiFontSize"));
    // Hidden values still round-trip through the dialog untouched.
    let (mut app, _) = app_with_store();
    app.run("prefs.set", json!({"values": {"type.smartQuotes": false}})).unwrap();
    let id = open_preferences(&mut app, "type");
    let fields = app.ui.dialogs.iter().find(|d| d.id == id).map(|d| d.fields.clone()).unwrap();
    confirm(&mut app, &fields).unwrap();
    assert!(!app.session.prefs().type_.smart_quotes);
}

#[test]
fn preferences_persist_through_services_and_theme_follows() {
    let (mut app, store) = app_with_store();
    let ctx = egui::Context::default();
    tick(&mut app, &ctx);
    app.run("prefs.set", json!({"values": {"interface.theme": "studioLight", "performance.historyStates": 12}})).unwrap();
    tick(&mut app, &ctx);
    assert_eq!(app.ui.theme, ThemeKind::StudioLight);
    let saved = store.lock().unwrap().clone().unwrap();
    assert!(saved.contains("\"historyStates\": 12"));
    // A new app instance loads them.
    let (mut app2, store2) = app_with_saved(Some(saved));
    tick(&mut app2, &ctx);
    assert_eq!(app2.session.prefs().performance.history_states, 12);
    assert_eq!(app2.ui.theme, ThemeKind::StudioLight);
    // Picking a theme from the Window menu becomes the preference.
    crate::menus::invoke(&mut app2, &ctx, "window.theme.classic", json!({})).unwrap();
    tick(&mut app2, &ctx);
    assert_eq!(app2.session.prefs().interface.theme, Theme::Classic);
    assert!(store2.lock().unwrap().as_ref().unwrap().contains("classic"));
}

#[test]
fn preset_export_dialog_offers_tool_presets() {
    use egui_kittest::{Harness, kittest::Queryable};
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    let ctx = egui::Context::default();
    let opened = crate::menus::invoke(&mut app, &ctx, "edit.presets.exportImportPresets", json!({})).unwrap();
    let fields = app.ui.dialog_mut(opened["dialog"].as_u64().unwrap()).unwrap().fields.clone();
    assert_eq!(fields["tools"], true);
    let mut h = Harness::builder().with_size(vec2(420.0, 260.0)).build_ui_state(presets_io_body, fields);
    h.run_steps(2);
    h.get_by_label("Tool Presets").click();
    h.run_steps(2);
    assert_eq!(h.state()["tools"], false);
    assert_eq!(h.state()["brushes"], true);
    assert_eq!(h.state()["customShapes"], true);
}

#[test]
fn brush_preset_store_attaches_when_loaded_and_persists() {
    use photocraft_engine::preset_store::{MemBackend, open};
    let mem = MemBackend::default();
    let ctx = egui::Context::default();
    let (tx, rx) = std::sync::mpsc::channel();
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services { preset_store: Some(rx), ..Default::default() });
    // Still loading: presets made now are kept and written once the store arrives.
    tick(&mut app, &ctx);
    app.run("brush.presets.save", json!({"name": "Early"})).unwrap();
    assert!(mem.files.lock().unwrap().is_empty());
    tx.send(open(Box::new(mem.clone()))).unwrap();
    tick(&mut app, &ctx);
    assert!(app.services.preset_store.is_none() && app.session.preset_store.is_some());
    app.run("brush.presets.save", json!({"name": "Late"})).unwrap();
    let mut s2 = photocraft_engine::Session::new();
    assert!(s2.attach_preset_store(open(Box::new(mem))).is_empty());
    for n in ["Early", "Late"] {
        assert!(s2.tools.presets.iter().any(|p| p.name == n), "{n}");
    }
}
>>>>>>> upstream/main

#[test]
fn preferences_dialog_round_trip() {
    let (mut app, _) = app_with_store();
    open_prefs(&mut app, "cursors");
    edit(&mut app, "cursors.painting", PrefValue::Choice("fullSizeTip".into()));
    edit(&mut app, "unitsAndRulers.rulers", PrefValue::Choice("inches".into()));
    apply_window(&mut app).unwrap();
    assert_eq!(app.session.prefs().cursors.painting, prefs::PaintingCursor::FullSizeTip);
    assert_eq!(app.session.prefs().units_and_rulers.rulers, prefs::Unit::Inches);
<<<<<<< HEAD
    // Reopening shows what the engine holds.
    let pages = serde_json::to_value(super::fields::PrefPages::of(app.session.prefs())).unwrap();
    assert_eq!(pages["cursors"]["painting"], "fullSizeTip");
    assert_eq!(pages["unitsAndRulers"]["rulers"], "inches");
=======
    // Invalid values are rejected and nothing changes.
    let id = open_preferences(&mut app, "performance");
    let mut values = app.ui.dialogs.iter().find(|d| d.id == id).unwrap().fields["values"].clone();
    values["performance"]["historyStates"] = json!(0);
    app.ui.dialog_mut(id).unwrap().fields.insert("values".into(), values);
    assert!(crate::dialogs::confirm(&mut app, id).is_err());
    assert_eq!(app.session.prefs().performance.history_states, 50);
}

#[test]
fn preferences_apply_button_saves_without_closing_and_cancel_keeps_applied_values() {
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };

    let (mut app, store) = app_with_store();
    app.run("prefs.set", json!({"values": {"interface.language": "en", "type.smartQuotes": false}})).unwrap();
    let id = open_preferences(&mut app, "interface");
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        app
    });
    h.run_steps(4);
    assert!(h.get_by_label("Apply").accesskit_node().is_disabled());

    let values = h.state_mut().ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap();
    values["interface"]["appearanceMode"] = json!("light");
    values["interface"]["lightTheme"] = json!("studioLight");
    values["interface"]["uiFontSize"] = json!("large");
    values["performance"]["historyStates"] = json!(12);
    h.run_steps(2);
    assert!(!h.get_by_label("Apply").accesskit_node().is_disabled());
    h.get_by_label("Apply").click();
    h.run_steps(4);

    assert_eq!(h.state().session.prefs().performance.history_states, 12);
    assert_eq!(h.state().ui.theme, ThemeKind::StudioLight);
    assert_eq!(h.ctx.fonts(|f| f.definitions().font_data["Inter"].tweak.scale), 16.0 / 12.0);
    assert!(!h.state().session.prefs().type_.smart_quotes, "hidden settings round-trip unchanged");
    let d = h.state().ui.dialogs.iter().find(|d| d.id == id).unwrap();
    assert_eq!(d.fields["section"], "interface");
    assert_eq!(d.fields["values"]["performance"]["historyStates"], 12);
    assert!(h.get_by_label("Apply").accesskit_node().is_disabled());
    let saved: Value = serde_json::from_str(store.lock().unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(saved["performance"]["historyStates"], 12);
    assert_eq!(saved["interface"]["appearanceMode"], "light");
    assert_eq!(saved["interface"]["lightTheme"], "studioLight");
    assert_eq!(saved["interface"]["uiFontSize"], "large");

    // Repeated Apply starts from the validated values, not the dialog's original snapshot.
    h.state_mut().ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["performance"]["historyStates"] = json!(22);
    h.run_steps(2);
    h.get_by_label("Apply").click();
    h.run_steps(4);
    assert_eq!(h.state().session.prefs().performance.history_states, 22);
    h.state_mut().ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["performance"]["historyStates"] = json!(33);
    h.state_mut().ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["interface"]["uiFontSize"] = json!("tiny");
    h.run_steps(2);
    h.get_by_label("Cancel").click();
    h.run_steps(4);
    assert!(h.state().ui.dialogs.iter().all(|d| d.id != id));
    assert_eq!(h.state().session.prefs().performance.history_states, 22);
    assert_eq!(h.ctx.fonts(|f| f.definitions().font_data["Inter"].tweak.scale), 16.0 / 12.0);
    let saved = store.lock().unwrap().clone().unwrap();
    let (mut restarted, _) = app_with_saved(Some(saved));
    tick(&mut restarted, &egui::Context::default());
    assert_eq!(restarted.session.prefs().performance.history_states, 22);
    assert_eq!(restarted.ui.theme, ThemeKind::StudioLight);
    assert_eq!(restarted.session.prefs().interface.ui_font_size, prefs::UiFontSize::Large);
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "macos")))]
#[test]
fn system_title_bar_explains_next_launch_and_preserves_apply_and_cancel() {
    use egui_kittest::{Harness, kittest::Queryable};

    let (mut app, store) = app_with_store();
    app.run("prefs.set", json!({"values": {"interface.language": "en"}})).unwrap();
    app.custom_titlebar = true;
    let id = open_preferences(&mut app, "interface");
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        app
    });
    h.run_steps(4);
    h.get_by_label("System title bar").scroll_to_me();
    h.run_steps(4);
    h.get_by_label("Applies at next launch.");
    for desired in [true, false] {
        h.get_by_label("System title bar").click();
        h.run_steps(4);
        assert_eq!(h.state().ui.dialogs.iter().find(|d| d.id == id).unwrap().fields["values"]["interface"]["systemTitleBar"], desired);
        assert_eq!(h.state().session.prefs().interface.system_title_bar, !desired, "the draft is not applied yet");
        h.get_by_label("Applies at next launch.");
        h.get_by_label("Apply").click();
        h.run_steps(4);
        assert_eq!(h.state().session.prefs().interface.system_title_bar, desired);
        assert_eq!(stored(&store)["interface"]["systemTitleBar"], desired);
        assert!(h.state().custom_titlebar, "do not change the current window or restart it");
        let (reloaded, _) = app_with_saved(store.lock().unwrap().clone());
        assert_eq!(reloaded.session.prefs().interface.system_title_bar, desired);
        h.get_by_label("Applies at next launch.");
    }
    h.get_by_label("System title bar").click();
    h.run_steps(4);
    h.get_by_label("Cancel").click();
    h.run_steps(4);
    assert!(h.state().ui.dialogs.iter().all(|d| d.id != id));
    assert!(!h.state().session.prefs().interface.system_title_bar);
    assert_eq!(stored(&store)["interface"]["systemTitleBar"], false);
    assert!(h.state().custom_titlebar);
}

/// #7939: switching Rendering Mode waits for the next launch, and says so next to the setting.
#[test]
fn rendering_mode_explains_next_launch() {
    use egui_kittest::{Harness, kittest::Queryable};
    let obj = json!({"renderingMode": "auto", "historyStates": 50}).as_object().unwrap().clone();
    let mut h = Harness::new_ui_state(
        |ui, obj: &mut Map<String, Value>| section_fields(ui, "performance", obj, &[], crate::i18n::Lang::from_pref("en"), None, ""),
        obj,
    );
    h.run_steps(4);
    assert!(h.query_all_by_label("Rendering Mode").next().is_some(), "the Rendering Mode setting");
    assert!(h.query_all_by_label("Applies at next launch.").next().is_some(), "its next-launch notice");
    // A search that leaves Rendering Mode out leaves its notice out too.
    let obj = json!({"renderingMode": "auto", "historyStates": 50}).as_object().unwrap().clone();
    let mut h = Harness::new_ui_state(
        |ui, obj: &mut Map<String, Value>| section_fields(ui, "performance", obj, &[], crate::i18n::Lang::from_pref("en"), None, "history"),
        obj,
    );
    h.run_steps(4);
    assert!(h.query_by_label("Applies at next launch.").is_none());
}

/// Integrations' rows apply at once, so none of them says "Applies at next launch." (it did
/// before the agent connection followed the preferences live).
#[test]
fn integrations_apply_without_a_restart() {
    for key in ["allowAgentControl", "connectAgents", "controlPort", "readRoot", "writeRoot", "whenAppClosed"] {
        assert!(!graphics::read_at_launch(&format!("integrations.{key}")), "{key}");
    }
}

#[test]
fn next_launch_notice_does_not_mark_unrelated_interface_preferences() {
    use egui_kittest::{Harness, kittest::Queryable};
    let obj = json!({"showTooltips": true}).as_object().unwrap().clone();
    let mut h =
        Harness::new_ui_state(|ui, obj: &mut Map<String, Value>| section_fields(ui, "interface", obj, &[], crate::i18n::Lang::from_pref("en"), None, ""), obj);
    h.run_steps(4);
    assert!(h.query_by_label("Applies at next launch.").is_none());
    h.get_by_label("Show tooltips").click();
    h.run_steps(4);
    assert_eq!(h.state()["showTooltips"], false);
    assert!(h.query_by_label("Applies at next launch.").is_none());
}

#[test]
fn preferences_confirm_after_apply_commits_later_edits() {
    let (mut app, _) = app_with_store();
    let id = open_preferences(&mut app, "cursors");
    app.ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["cursors"]["painting"] = json!("fullSizeTip");
    apply(&mut app, id).unwrap();
    app.ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["unitsAndRulers"]["rulers"] = json!("inches");
    crate::dialogs::confirm(&mut app, id).unwrap();
    assert!(app.ui.dialogs.iter().all(|d| d.id != id));
    assert_eq!(app.session.prefs().cursors.painting, prefs::PaintingCursor::FullSizeTip);
    assert_eq!(app.session.prefs().units_and_rulers.rulers, prefs::Unit::Inches);
}

/// #11120: a live theme change made while Preferences is open survives Apply (and OK) of an
/// unrelated setting, and the next draft shows it.
#[test]
fn preferences_apply_keeps_a_live_theme_change_made_while_open() {
    let ctx = egui::Context::default();
    for ok in [false, true] {
        let (mut app, store) = app_with_store();
        app.set_theme(&ctx, ThemeKind::Studio);
        let id = open_preferences(&mut app, "interface");
        assert_eq!(app.ui.dialog_mut(id).unwrap().fields["values"]["interface"]["theme"], "studio");
        app.set_theme(&ctx, ThemeKind::Classic);
        assert!(!preferences_changed(&app, &app.ui.dialogs.iter().find(|d| d.id == id).unwrap().fields), "a change made elsewhere is not a dialog edit");
        app.ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["interface"]["uiScale"] = json!("200");
        assert!(preferences_changed(&app, &app.ui.dialogs.iter().find(|d| d.id == id).unwrap().fields));
        if ok {
            crate::dialogs::confirm(&mut app, id).unwrap();
            assert!(app.ui.dialogs.iter().all(|d| d.id != id));
        } else {
            apply(&mut app, id).unwrap();
            let d = app.ui.dialogs.iter().find(|d| d.id == id).unwrap();
            assert_eq!(d.fields["values"]["interface"]["theme"], "classic", "the next draft follows the live theme");
            assert!(!preferences_changed(&app, &d.fields));
        }
        let p = app.session.prefs();
        assert_eq!((p.interface.theme, p.interface.ui_scale), (Theme::Classic, prefs::UiScale::P200), "ok: {ok}");
        assert_eq!(app.ui.theme, ThemeKind::Classic);
        tick(&mut app, &ctx);
        let saved = stored(&store);
        assert_eq!((saved["interface"]["theme"].as_str(), saved["interface"]["uiScale"].as_str()), (Some("classic"), Some("200")));
    }
}

/// A setting edited both in the dialog and elsewhere while it is open takes the dialog's value.
#[test]
fn preferences_apply_prefers_the_dialog_edit_of_a_setting_changed_elsewhere() {
    let ctx = egui::Context::default();
    let (mut app, store) = app_with_store();
    app.set_theme(&ctx, ThemeKind::Studio);
    let id = open_preferences(&mut app, "interface");
    app.set_theme(&ctx, ThemeKind::Classic);
    app.ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["interface"]["theme"] = json!("studioLight");
    apply(&mut app, id).unwrap();
    assert_eq!(app.session.prefs().interface.theme, Theme::StudioLight);
    // The dialog's value is the one shown (from the next frame) and saved, not only the one held.
    tick(&mut app, &ctx);
    assert_eq!(app.ui.theme, ThemeKind::StudioLight);
    assert_eq!(stored(&store)["interface"]["theme"].as_str(), Some("studioLight"));
}

/// Apply makes the committed values the next base: a live change made after one Apply survives
/// the next one too.
#[test]
fn preferences_apply_twice_keeps_each_live_change() {
    let ctx = egui::Context::default();
    let (mut app, store) = app_with_store();
    app.set_theme(&ctx, ThemeKind::Studio);
    let id = open_preferences(&mut app, "interface");
    app.set_theme(&ctx, ThemeKind::Classic);
    app.ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["interface"]["uiScale"] = json!("200");
    apply(&mut app, id).unwrap();
    app.set_theme(&ctx, ThemeKind::StudioLight);
    app.ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["interface"]["uiScale"] = json!("150");
    apply(&mut app, id).unwrap();
    let p = app.session.prefs();
    assert_eq!((p.interface.theme, p.interface.ui_scale), (Theme::StudioLight, prefs::UiScale::P150));
    assert_eq!(app.ui.theme, ThemeKind::StudioLight);
    tick(&mut app, &ctx);
    let saved = stored(&store);
    assert_eq!((saved["interface"]["theme"].as_str(), saved["interface"]["uiScale"].as_str()), (Some("studioLight"), Some("150")));
}

/// The merge is not about the theme: a setting changed elsewhere (History States, by command)
/// survives Apply of a theme edited in the dialog.
#[test]
fn preferences_apply_keeps_a_setting_changed_elsewhere_beside_a_dialog_theme_edit() {
    let ctx = egui::Context::default();
    let (mut app, store) = app_with_store();
    app.set_theme(&ctx, ThemeKind::Studio);
    let id = open_preferences(&mut app, "interface");
    app.run("prefs.set", json!({"path": "performance.historyStates", "value": 30})).unwrap();
    app.ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["interface"]["theme"] = json!("classic");
    apply(&mut app, id).unwrap();
    let p = app.session.prefs();
    assert_eq!((p.interface.theme, p.performance.history_states), (Theme::Classic, 30));
    tick(&mut app, &ctx);
    assert_eq!(app.ui.theme, ThemeKind::Classic);
    let saved = stored(&store);
    assert_eq!((saved["interface"]["theme"].as_str(), saved["performance"]["historyStates"].as_u64()), (Some("classic"), Some(30)));
}

/// A Preferences dialog without `__base` (opened before it was kept) commits its draft as it is:
/// a change made elsewhere meanwhile is not kept.
#[test]
fn preferences_without_a_base_commit_the_draft_as_it_is() {
    let ctx = egui::Context::default();
    let (mut app, _store) = app_with_store();
    app.set_theme(&ctx, ThemeKind::Studio);
    let id = open_preferences(&mut app, "interface");
    app.ui.dialog_mut(id).unwrap().fields.remove("__base");
    app.set_theme(&ctx, ThemeKind::Classic);
    app.ui.dialog_mut(id).unwrap().fields.get_mut("values").unwrap()["interface"]["uiScale"] = json!("200");
    apply(&mut app, id).unwrap();
    let p = app.session.prefs();
    assert_eq!((p.interface.theme, p.interface.ui_scale), (Theme::Studio, prefs::UiScale::P200), "the draft's theme");
>>>>>>> upstream/main
}

#[test]
fn preferences_apply_rejects_invalid_drafts_and_keeps_them_open() {
    let (mut app, _) = app_with_store();
    let before = app.session.prefs().to_json();
    open_prefs(&mut app, "fileHandling");
    // The window keeps choices to their options and numbers to their ranges, so the draft that reaches the engine and
    // is refused is a number that has no range of its own: the autosave interval is one of a fixed list.
    edit(&mut app, "fileHandling.autosaveMinutes", PrefValue::Int(7));
    assert!(apply_window(&mut app).is_err());
    assert_eq!(app.session.prefs().to_json(), before);
    assert!(window_open(&app), "the window stays open with the draft");
    let error = app.prefs_rt.window.as_ref().and_then(|session| session.state.error.clone());
    assert!(error.is_some_and(|e| e.contains("fileHandling.autosaveMinutes")), "the refusal shows in the window's error line");
}

#[test]
fn shortcut_dialog_moves_shortcuts_and_shell_dispatch_follows() {
    use super::shortcuts::{press_ok, rebind};
    let (mut app, _) = app_with_store();
    let ctx = egui::Context::default();
    let steal = |app: &mut PhotocraftApp, id: &str, key: &str| {
        open_shortcuts(app, 0);
        rebind(app, &CommandRef::new(id), Binding::parse(key).unwrap(), Policy::Steal).unwrap();
        press_ok(app, &ctx);
    };
    // ⌘O (File › Open, a shell command) given to Layer › New › Layer.
    steal(&mut app, "layer.new.layer", "Cmd+O");
    assert_eq!(effective_shortcut(&app, "layer.new.layer", None).as_deref(), Some("Cmd+O"));
    assert_eq!(effective_shortcut(&app, "file.open", Some("Cmd+O")), None, "taken from File › Open");
    // The menu rows show the effective shortcut.
    assert_eq!(menu_shortcut(&app, "layer.new.layer"), Some(chord("Cmd+O")));
    assert_eq!(menu_shortcut(&app, "file.open"), None);

    // Re-assigning a shortcut already held by a custom override takes it from the override.
    steal(&mut app, "layer.new.group", "Cmd+O");
    assert_eq!(effective_shortcut(&app, "layer.new.group", None).as_deref(), Some("Cmd+O"));
    assert_eq!(effective_shortcut(&app, "layer.new.layer", None), None, "taken from layer.new.layer override");

    // OK with nothing changed keeps every shortcut where it is.
    let unchanged = app.session.prefs().shortcuts.clone();
    open_shortcuts(&mut app, 0);
    press_ok(&mut app, &ctx);
    assert_eq!(app.session.prefs().shortcuts, unchanged);

    // Giving the shortcut back to Layer › New › Layer takes it from the group override.
    steal(&mut app, "layer.new.layer", "Cmd+O");
    assert_eq!(effective_shortcut(&app, "layer.new.layer", None).as_deref(), Some("Cmd+O"));
    assert_eq!(effective_shortcut(&app, "layer.new.group", None), None, "taken back from the group");
    assert_eq!(effective_shortcut(&app, "file.open", Some("Cmd+O")), None, "File › Open stays cleared");
}

/// #3665: the dialog keeps a command's further keys: they move like main keys, and menus show only the main key.
#[test]
fn shortcut_dialog_moves_and_keeps_alternate_keys() {
    use super::shortcuts::{assign, press_ok};
    let (mut app, _) = app_with_store();
    let ctx = egui::Context::default();
    let cut = menu_shortcut(&app, "edit.cut");
    let paste = menu_shortcut(&app, "edit.paste");
    assert!(cut.is_some() && paste.is_some());
    open_shortcuts(&mut app, 0);
    // F2 (Cut's further key) given to Rename Layer is taken from Cut, which keeps its main key.
    assign(&mut app, &CommandRef::new("layer.renameLayer"), Binding::parse("F2").unwrap(), Policy::Steal).unwrap();
    // F4 added to Paste is a further key: the menu keeps showing ⌘V.
    assign(&mut app, &CommandRef::new("edit.paste"), Binding::parse("F4").unwrap(), Policy::Steal).unwrap();
    press_ok(&mut app, &ctx);
    assert_eq!(menu_shortcut(&app, "layer.renameLayer"), Some(chord("F2")));
    assert_eq!(menu_shortcut(&app, "edit.cut"), cut, "Cut keeps its main key");
    assert_eq!(menu_shortcut(&app, "edit.paste"), paste, "the menu shows only the main key");
    assert_eq!(effective_shortcut(&app, "layer.renameLayer", None).as_deref(), Some("F2"));
    assert_eq!(effective_shortcut(&app, "edit.paste", Some("Cmd+V")).as_deref(), Some("Cmd+V"));
}

#[test]
fn hidden_menu_items_disappear() {
    let (mut app, _) = app_with_store();
    app.run("edit.menus", json!({"hide": ["edit.fade"]})).unwrap();
    assert!(!crate::menus::menu_rows(&app).iter().any(|row| row.command.id == "edit.fade"));
}

/// #5096: preferences that list Edit › Menus or File › Exit as hidden still show them.
#[test]
fn essential_menu_items_stay_visible() {
    let (mut app, _) = app_with_store();
    app.run("prefs.set", json!({"values": {"menus": {"hidden": ["edit.menus", "file.exit", "edit.fade"], "colors": {}}}})).unwrap();
    let rows = crate::menus::menu_rows(&app);
    assert!(rows.iter().any(|row| row.command.id == "edit.menus") && rows.iter().any(|row| row.command.id == "file.exit"));
    assert!(!rows.iter().any(|row| row.command.id == "edit.fade"));
}

#[test]
fn autosave_runs_for_dirty_documents() {
    type Saved = Arc<Mutex<Vec<(u64, u64)>>>;
    let saved: Saved = Arc::default();
    let s2 = saved.clone();
    let services = crate::Services {
        autosave: Some(Box::new(move |doc: &Arc<photocraft_doc::Document>, rev: u64, _path: Option<&str>| {
            s2.lock().unwrap().push((doc.id.0, rev));
            Ok(())
        })),
        ..Default::default()
    };
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
    let ctx = app.ctx().clone();
    app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
    tick(&mut app, &ctx);
    autosave_now(&mut app);
    tick(&mut app, &ctx);
    assert!(saved.lock().unwrap().is_empty(), "clean documents are not autosaved");
    app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
    autosave_now(&mut app);
    tick(&mut app, &ctx);
    assert_eq!(saved.lock().unwrap().len(), 1);
    autosave_now(&mut app);
    tick(&mut app, &ctx);
    assert_eq!(saved.lock().unwrap().len(), 1, "unchanged since the last autosave");

    // Loaded copies can have the same persisted identity and dirty revision.
    let original = app.session.active().unwrap().doc.clone();
    let original_id = original.id;
    app.session.add_document(original.as_ref().clone(), None);
    app.run("edit.fill", json!({"color": "#0000ff"})).unwrap();
    autosave_now(&mut app);
    tick(&mut app, &ctx);
    {
        let saved = saved.lock().unwrap();
        assert_eq!(saved.len(), 2, "both dirty copies need independent autosaves");
        assert_eq!(saved[0].0, original_id.0);
        assert_ne!(saved[0].0, saved[1].0, "recovery ownership must be distinct");
        assert_eq!(saved[0].1, saved[1].1, "the revisions must match to reproduce suppression");
    }
    app.run("prefs.set", json!({"path": "fileHandling.autosave", "value": false})).unwrap();
    app.run("edit.fill", json!({"color": "#00ff00"})).unwrap();
    autosave_now(&mut app);
    tick(&mut app, &ctx);
    assert_eq!(saved.lock().unwrap().len(), 2, "autosave off");
}

#[test]
fn recovered_documents_adopt_their_entries_until_saved_or_closed() {
    type Log = Arc<Mutex<Vec<String>>>;
    let log: Log = Arc::default();
    let (l1, l2, l3) = (log.clone(), log.clone(), log.clone());
    use photocraft_doc::{Color, ColorMode, Document, SampleType, Size};
    let doc = || Document::with_background("R", Size::new(4, 4), ColorMode::Rgb, SampleType::U8, Color::WHITE);
    let services = crate::Services {
        autosave: Some(Box::new(move |d: &Arc<Document>, _: u64, _: Option<&str>| {
            l1.lock().unwrap().push(format!("save {}", d.id.0));
            Ok(())
        })),
        discard_autosave: Some(Box::new(move |id: u64| l2.lock().unwrap().push(format!("discard {id}")))),
        recover: Some(Box::new(move || {
            ["a", "b"].map(|key| crate::Recoverable { key: key.into(), name: "R".into(), path: None, load: Box::new(move || Ok(doc())) }).into()
        })),
        adopt_autosave: Some(Box::new(move |id: u64, key: &str| l3.lock().unwrap().push(format!("adopt {id} {key}")))),
        ..Default::default()
    };
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
    let ctx = app.ctx().clone();
    tick(&mut app, &ctx);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while app.session.has_jobs() && std::time::Instant::now() < deadline {
        crate::jobs_ui::tick(&mut app, &ctx);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(!app.session.has_jobs(), "recovery finished");
    let ids: Vec<u64> = app.session.documents().iter().map(|d| d.doc.id.0).collect();
    assert!(app.session.documents().iter().all(|d| d.is_dirty()), "recovered documents are unsaved");
    let take = || std::mem::take(&mut *log.lock().unwrap());
    assert_eq!(take(), [format!("adopt {} a", ids[0]), format!("adopt {} b", ids[1])]);
    // Their entries are current: nothing to autosave until they change.
    tick(&mut app, &ctx);
    autosave_now(&mut app);
    tick(&mut app, &ctx);
    assert!(take().is_empty());
    // Closing one drops its entry; editing the other autosaves over its own.
    app.run("file.close", json!({"document": 0})).unwrap();
    app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
    autosave_now(&mut app);
    tick(&mut app, &ctx);
    assert_eq!(take(), [format!("discard {}", ids[0]), format!("save {}", ids[1])]);
}

#[test]
fn edit_dialogs_open() {
    let (mut app, _) = app_with_store();
    let ctx = egui::Context::default();
    app.run("file.new", json!({"width": 32, "height": 32})).unwrap();
    app.sync_views();
    // Color Settings opens on the host, with the monitor line naming the fallback profile.
    crate::menus::invoke(&mut app, &ctx, "edit.colorSettings", json!({})).unwrap();
    assert!(app.dialog_open(), "Color Settings opened");
    let note = crate::monitor_status::note_cell(&app).lock().unwrap().clone();
    assert!(note.starts_with("Monitor profile in use: sRGB") && note.contains("fallback"), "{note}");
    assert!(crate::menus::invoke(&mut app, &ctx, "edit.fade", json!({})).is_err());
    // Preset Manager opens on the host and Done closes it.
    let open = app.dialog_count();
    let id = super::presets::open_manager(&mut app);
    assert_eq!(app.dialog_count(), open + 1, "the Preset Manager opened over Color Settings");
    crate::dialogs::testing::confirm(&mut app, id).unwrap();
    assert_eq!(app.dialog_count(), open, "Done closes the Preset Manager and leaves Color Settings");
}

#[test]
fn tool_colors_first_saves_keep_other_instances_separate_edits() {
    for initial in [None, Some(json!({})), Some(json!({"toolColors": photocraft_engine::prefs::ToolColors::default().0}))] {
        for background_first in [false, true] {
            let files = Arc::new(MemoryFiles::new());
            if let Some(initial) = &initial {
                seed(&files, initial, Some(<prefs::SavedPrefs as Settings>::SCHEMA_VERSION));
            }
            let (mut foreground, mut background) = (app_on(&files), app_on(&files));
            let fg = [0.25, 0.5, 0.75, 1.0];
            let bg = [0.75, 0.5, 0.25, 1.0];
            foreground.run("tools.setColors", json!({"foreground": fg})).unwrap();
            background.run("tools.setColors", json!({"background": bg})).unwrap();
            let (first, second) = if background_first { (&mut background, &mut foreground) } else { (&mut foreground, &mut background) };
            crate::tool_memory::save_on_exit(first).unwrap();
            crate::tool_memory::save_on_exit(second).unwrap();
            // Further saves must still keep the other window's independent field.
            save_preferences(first).unwrap();
            save_preferences(second).unwrap();
            let restored = app_on(&files);
            assert_eq!(restored.session.tools.foreground, fg);
            assert_eq!(restored.session.tools.background, bg);
            assert_eq!(restored.session.tools.other_colors, photocraft_engine::ToolState::MASK_COLORS);
        }
    }
}

#[test]
fn tool_colors_preserve_finite_hdr_components() {
    let (mut app, store) = app_with_store();
    let fg = [1.5, -0.1, 0.6, 1.0];
    app.run("tools.setColors", json!({"foreground": fg})).unwrap();
    crate::tool_memory::save_on_exit(&mut app).unwrap();
    let restored = app_on(&store);
    assert_eq!(restored.session.tools.foreground, fg);
}

#[test]
fn tool_colors_restore_cmyk_inks_for_picker_and_fill_after_swapping_and_exiting_on_a_mask() {
    let (mut app, store) = app_with_store();
    let cyan = [1.0, 0.0, 0.0, 0.0];
    let magenta = [0.0, 1.0, 0.0, 0.0];
    let fg = photocraft_color::convert::cmyk_to_rgb(cyan).map(|c| c.clamp(0.0, 1.0));
    let bg = photocraft_color::convert::cmyk_to_rgb(magenta).map(|c| c.clamp(0.0, 1.0));
    app.run(
        "tools.setColors",
        json!({
            "foreground": fg, "foregroundCmyk": cyan, "background": bg, "backgroundCmyk": magenta
        }),
    )
    .unwrap();
    app.run("tools.swapColors", json!({})).unwrap();
    app.session.tools.target_mask(true);
    crate::tool_memory::save_on_exit(&mut app).unwrap();
    let mut restored = app_on(&store);
    assert_eq!(restored.session.tools.picked_cmyk(fg), Some(cyan));
    assert_eq!(restored.session.tools.picked_cmyk(bg), Some(magenta));
    for (target, expected) in [("foreground", magenta), ("background", cyan)] {
        let picker = crate::color_picker_ui::open(&mut restored, target);
        let shown = crate::color_picker_ui::testing::current(&restored, picker).expect("the picker is open");
        let inks = crate::color_picker_ui::inks_of(&shown).expect("the picker shows the inks it was picked as");
        assert!(crate::color_picker_ui::same_inks(inks, expected), "{target}: {inks:?}");
    }
    restored.run("file.new", json!({"width": 4, "height": 4})).unwrap();
    restored.run("image.mode.cmyk", json!({})).unwrap();
    for (command, expected) in [("edit.fillForeground", magenta), ("edit.fillBackground", cyan)] {
        restored.run(command, json!({})).unwrap();
        let st = restored.session.active().unwrap();
        let surface = st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap();
        let pixel = surface.read_region(photocraft_geom::Rect::new(0, 0, 1, 1));
        assert!(pixel.iter().take(4).zip(expected).all(|(a, b)| (a - b).abs() < 0.01), "{command}: {pixel:?}");
    }
}

#[test]
fn tool_colors_save_changed_cmyk_inks_even_when_rgba_is_unchanged() {
    let (mut app, store) = app_with_store();
    let ctx = egui::Context::default();
    let rgb = [0.2, 0.4, 0.6];
    app.run("tools.setColors", json!({"foreground": rgb, "foregroundCmyk": [1, 0, 0, 0]})).unwrap();
    crate::tool_memory::persist(&mut app, &ctx);
    tick(&mut app, &ctx);
    let rev = app.session.prefs.rev();
    let inks = [0.1, 0.2, 0.3, 0.4];
    app.run("tools.setColors", json!({"foreground": rgb, "foregroundCmyk": inks})).unwrap();
    crate::tool_memory::persist(&mut app, &ctx);
    assert!(app.session.prefs.rev() > rev);
    tick(&mut app, &ctx);
    let restored = app_on(&store);
    assert_eq!(restored.session.tools.picked_cmyk(rgb), Some(inks));
}

#[test]
fn tool_colors_flush_swaps_and_defaults_on_exit_without_another_frame() {
    let (mut app, store) = app_with_store();
    app.run("tools.setColors", json!({"foreground": "#336699", "background": "#cc8844"})).unwrap();
    app.run("tools.swapColors", json!({})).unwrap();
    crate::tool_memory::save_on_exit(&mut app).unwrap();
    let mut restored = app_on(&store);
    assert_eq!(restored.session.tools.foreground, app.session.tools.foreground);
    assert_eq!(restored.session.tools.background, app.session.tools.background);
    restored.run("tools.defaultColors", json!({})).unwrap();
    crate::tool_memory::save_on_exit(&mut restored).unwrap();
    let defaults = app_on(&store);
    assert_eq!(defaults.session.tools.foreground, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(defaults.session.tools.background, [1.0; 4]);
}

#[test]
fn tool_colors_keep_pixel_and_mask_pairs_when_exiting_on_a_mask() {
    let (mut app, store) = app_with_store();
    app.session.tools.foreground = [0.2, 0.4, 0.6, 1.0];
    app.session.tools.background = [0.9, 0.7, 0.3, 1.0];
    let pixels = [app.session.tools.foreground, app.session.tools.background];
    app.session.tools.target_mask(true);
    app.session.tools.foreground = [0.5, 0.5, 0.5, 1.0];
    let masks = [app.session.tools.foreground, app.session.tools.background];
    crate::tool_memory::save_on_exit(&mut app).unwrap();
    let mut restored = app_on(&store);
    assert_eq!([restored.session.tools.foreground, restored.session.tools.background], pixels);
    restored.session.tools.target_mask(true);
    assert_eq!([restored.session.tools.foreground, restored.session.tools.background], masks);
}

#[test]
fn reset_section_on_type_keeps_the_favourite_fonts() {
    // The favourites are the font menu's library, not a Type setting.
    use egui_kittest::kittest::Queryable;
    let (_, store) = app_with_store();
    let mut h = prefs_harness(&store, "type", vec2(1280.0, 800.0), |app| {
        app.run("prefs.set", json!({"values": {"interface.language": "en", "type.smartQuotes": false}})).unwrap();
        app.run("type.font.favorite", json!({"family": "Bebas Neue"})).unwrap();
    });
    h.get_by_label("Reset Section").click();
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(4);
    let prefs = h.state().session.prefs().type_.clone();
    assert!(prefs.smart_quotes, "the Type settings are reset");
    assert_eq!(prefs.favorite_fonts, ["Bebas Neue"], "the favourites are kept");
    assert_eq!(stored(&store)["type"]["favoriteFonts"], json!(["Bebas Neue"]));
}
