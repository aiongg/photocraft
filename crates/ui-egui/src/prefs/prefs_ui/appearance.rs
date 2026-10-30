use super::dialogs::open_preferences;
use super::fields::{choice_label, color_of, preference_matches};
use super::{AppearanceMode, Color32, Map, PhotocraftApp, RichText, Sense, ThemeKind, Tokens, Value, json, prefs, vec2};
use craft_theme::{
    AppChoice, AppDefaults, AppId, Appearance, Mode, ModeChoice, MotionChoice, Selection, Size, SystemAppearance, ThemeDef, ThemeId, ThemePair, Variants,
};
use craft_theme_store::{Accepted, StoreOptions, SuiteClient, SuiteDir, SuiteWatch};
use std::sync::Arc;

/// The GPU canvas colours from Preferences › Transparency & Gamut.
pub fn canvas_style(app: &PhotocraftApp) -> crate::gpu_canvas::CanvasStyle {
    let t = &app.session.prefs().transparency_and_gamut;
    let [l, d] = t.colors();
    let f = |c: [u8; 3]| c.map(|v| v as f32 / 255.0);
    let g = prefs::parse_hex(&t.gamut_warning_color).unwrap_or([128; 3]);
    crate::gpu_canvas::CanvasStyle {
        checker_square: t.square().unwrap_or(0.0),
        checker_light: f(l),
        checker_dark: f(d),
        gamut_color: f(g),
        gamut_opacity: t.gamut_warning_opacity as f32 / 100.0,
    }
}

/// Pasteboard colour from Preferences › Interface (`None` = the theme's default canvas).
pub fn pasteboard_color(app: &PhotocraftApp) -> Option<Color32> {
    use prefs::CanvasColor;
    let i = &app.session.prefs().interface;
    Some(match i.canvas_color {
        CanvasColor::Default => return None,
        CanvasColor::Black => Color32::BLACK,
        CanvasColor::DarkGray => Color32::from_gray(40),
        CanvasColor::MediumGray => Color32::from_gray(83),
        CanvasColor::LightGray => Color32::from_gray(184),
        CanvasColor::Custom => color_of(&i.canvas_custom_color),
    })
}

/// What PhotoCraft shows when the user and the suite have chosen nothing: ArtCraft, dark, Compact.
pub(crate) fn app_defaults() -> AppDefaults {
    AppDefaults { theme: ThemePair::same(ThemeId::suite_default()), mode: ModeChoice::Dark, size: Size::Compact }
}

/// The app's own choice from its preferences: the theme pair and Size where set, the mode always
/// (PhotoCraft has always kept its own mode; it defaults to dark).
pub(crate) fn app_choice(i: &prefs::Interface, suite: &Selection) -> AppChoice {
    let id = |s: &str| ThemeId::parse(s).ok();
    let (light, dark) = (id(&i.theme_light), id(&i.theme_dark));
    let theme = (light.is_some() || dark.is_some())
        .then(|| ThemePair { light: light.unwrap_or_else(|| suite.theme.light.clone()), dark: dark.unwrap_or_else(|| suite.theme.dark.clone()) });
    let mode = match i.appearance_mode {
        AppearanceMode::Auto => ModeChoice::System,
        AppearanceMode::Dark => ModeChoice::Dark,
        AppearanceMode::Light => ModeChoice::Light,
    };
    let size = match i.size {
        prefs::AppSize::Suite => None,
        prefs::AppSize::Compact => Some(Size::Compact),
        prefs::AppSize::Comfortable => Some(Size::Comfortable),
    };
    AppChoice { theme, mode: Some(mode), size }
}

/// The selection shown: the suite's choice with PhotoCraft's own on top, reduced motion where
/// Enable Animations is off.
pub(crate) fn app_selection(i: &prefs::Interface, suite: &craft_theme::SuiteChoice) -> Selection {
    let suite = suite.with_defaults(&app_defaults());
    let mut sel = app_choice(i, &suite).apply(&suite);
    if !i.enable_animations {
        sel.motion = MotionChoice::Reduced;
    }
    sel
}

/// The theme store connection and what the last frame resolved.
#[derive(Default)]
pub struct SuiteRt {
    client: Option<SuiteClient>,
    resolved: Option<(Selection, SystemAppearance, u64)>,
    /// The legacy kind and library theme id installed for `resolved`.
    shown: Option<(ThemeKind, String)>,
}

impl SuiteRt {
    /// The connection, once the first frame has opened it.
    pub(crate) fn client(&self) -> Option<&SuiteClient> {
        self.client.as_ref()
    }

    /// What the shown look was resolved from (suite choice and PhotoCraft's own on top).
    pub(crate) fn selection(&self) -> Option<&Selection> {
        self.resolved.as_ref().map(|(sel, _, _)| sel)
    }

    /// The library theme id on screen.
    pub(crate) fn shown_id(&self) -> &str {
        self.shown.as_ref().map_or("", |(_, id)| id)
    }
}

fn open_suite(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let mut options = StoreOptions::new(format!("photocraft {}", photocraft_engine::build_info::VERSION));
    if let Ok(last) = serde_json::from_value::<Accepted>(app.session.prefs().interface.last_suite.clone()) {
        options = options.last_accepted(last);
    }
    let dir = app.services.suite_dir.clone().unwrap_or_else(SuiteDir::memory);
    let ctx = ctx.clone();
    app.prefs_rt.suite.client = Some(SuiteClient::new(SuiteWatch::open(&dir, options, move || ctx.request_repaint())));
}

/// A library theme as the pickers list it.
#[derive(Clone)]
pub(crate) struct ThemeChoice {
    pub id: String,
    pub name: String,
    pub variants: Variants,
    pub def: ThemeDef,
}

/// The built-in themes, then the library's.
pub(crate) fn theme_choices(app: &PhotocraftApp) -> Vec<ThemeChoice> {
    let mut defs: Vec<ThemeDef> = craft_theme::builtins().to_vec();
    if let Some(client) = app.prefs_rt.suite.client() {
        let extra: Vec<ThemeDef> = client.snapshot().index.iter().map(|e| e.theme.clone()).filter(|t| !defs.iter().any(|d| d.id == t.id)).collect();
        defs.extend(extra);
    }
    defs.into_iter().map(|def| ThemeChoice { id: def.id.as_str().to_string(), name: def.name.clone(), variants: def.variants, def }).collect()
}

/// The legacy kind PhotoCraft's layout code reads for what the preferences show, without a theme
/// store: the override's themes, or ArtCraft where it follows the suite.
pub(super) fn selected_theme(interface: &prefs::Interface, system: Option<egui::Theme>) -> ThemeKind {
    let dark = match interface.appearance_mode {
        AppearanceMode::Light => false,
        AppearanceMode::Dark => true,
        AppearanceMode::Auto => system != Some(egui::Theme::Light),
    };
    let size = if interface.size == prefs::AppSize::Comfortable { Size::Comfortable } else { Size::Compact };
    let id = if dark { &interface.theme_dark } else { &interface.theme_light };
    ThemeKind::of_library(if id.is_empty() { "craft.artcraft" } else { id }, dark, size)
}

pub(crate) fn cycle_appearance(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let next = match app.session.prefs().interface.appearance_mode {
        AppearanceMode::Auto => AppearanceMode::Light,
        AppearanceMode::Light => AppearanceMode::Dark,
        AppearanceMode::Dark => AppearanceMode::Auto,
    };
    if let Err(e) = app.run("prefs.set", json!({"path": "interface.appearanceMode", "value": next.name()})) {
        app.ui.status = e;
        return;
    }
    sync_appearance(app, ctx);
}

/// Photoshop's four interface brightnesses, darkest first.
const BRIGHTNESSES: [ThemeKind; 4] = [ThemeKind::Pro, ThemeKind::ProMedium, ThemeKind::ProLightGray, ThemeKind::ProLight];

/// Darken Interface (⇧⌘F1, `lighter` false) and Lighten Interface (⇧⌘F2): the next of
/// Photoshop's four brightnesses from the theme on screen, stopping at either end, as in
/// Photoshop. Another theme steps from the nearest brightness: Medium Gray for a dark one, Light
/// Gray for a light one. The choice is saved like picking it in Preferences › Interface.
pub(crate) fn step_brightness(app: &mut PhotocraftApp, ctx: &egui::Context, lighter: bool) -> Result<(), String> {
    let current = app.ui.theme;
    let at = BRIGHTNESSES.iter().position(|k| *k == current).unwrap_or(if crate::theme::Tokens::for_kind(current).dark() { 1 } else { 2 });
    let next = if lighter { (at + 1).min(BRIGHTNESSES.len() - 1) } else { at.saturating_sub(1) };
    let Some(&kind) = BRIGHTNESSES.get(next) else { return Ok(()) };
    if kind == current {
        return Ok(());
    }
    app.run("prefs.set", json!({"path": "interface.theme", "value": kind.id()}))?;
    sync_appearance(app, ctx);
    Ok(())
}

/// The favourites that have a light variant and those that have a dark one, in favourite order.
pub(super) fn favorite_theme_groups(favorites: &[String], choices: &[ThemeChoice]) -> (Vec<ThemeChoice>, Vec<ThemeChoice>) {
    let found: Vec<&ThemeChoice> = favorites.iter().filter_map(|f| choices.iter().find(|c| c.id == *f)).collect();
    let group = |keep: fn(Variants) -> bool| found.iter().filter(|c| keep(c.variants)).map(|c| (*c).clone()).collect();
    (group(|v| v != Variants::DarkOnly), group(|v| v != Variants::LightOnly))
}

/// The Size that keeps a built-in theme's geometry from before the library: the Pro themes are
/// Compact, the others Comfortable. `None` for a theme the user added.
fn default_size(id: &str) -> Option<&'static str> {
    Some(match id {
        "craft.artcraft" | "craft.pro" | "craft.pro-light-gray" | "craft.graphite" => "compact",
        "craft.studio" | "craft.classic" | "craft.solarized-dark" | "craft.adwaita" | "craft.breeze" | "craft.soft" => "comfortable",
        _ => return None,
    })
}

/// Picks library theme `id` for the light or dark slot, with the Size it implies, and shows that
/// appearance mode.
pub(crate) fn pick_theme(app: &mut PhotocraftApp, ctx: &egui::Context, id: &str, dark: bool) {
    let mut sets = vec![
        (if dark { "interface.themeDark" } else { "interface.themeLight" }, json!(id)),
        ("interface.appearanceMode", json!(if dark { "dark" } else { "light" })),
    ];
    if let Some(size) = default_size(id) {
        sets.push(("interface.size", json!(size)));
    }
    for (path, value) in sets {
        if let Err(e) = app.run("prefs.set", json!({"path": path, "value": value})) {
            app.ui.status = e;
            return;
        }
    }
    sync_appearance(app, ctx);
}

/// Right-click menu on the top-right appearance button. A plain click still cycles modes.
pub(crate) fn appearance_context_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if ui.button(tl!("Appearance Settings…")).clicked() {
        open_preferences(app, "interface");
        ui.close();
    }
    let choices = theme_choices(app);
    let (light, dark) = favorite_theme_groups(&app.session.prefs().interface.favorite_themes, &choices);
    let has_light = !light.is_empty();
    if !light.is_empty() || !dark.is_empty() {
        ui.separator();
    }
    let shown = app.prefs_rt.suite.shown_id().to_string();
    for (group, is_dark) in [(light, false), (dark, true)] {
        if is_dark && has_light && !group.is_empty() {
            ui.separator();
        }
        for choice in group {
            if ui.selectable_label(shown == choice.id, tl!(&choice.name)).clicked() {
                pick_theme(app, ui.ctx(), &choice.id, is_dark);
                ui.close();
            }
        }
    }
}

pub(super) fn system_theme(app: &PhotocraftApp, ctx: &egui::Context) -> Option<egui::Theme> {
    app.services.system_theme.as_ref().and_then(|read| read(ctx)).or_else(|| ctx.system_theme())
}

/// Resolve an absolute pixels-per-point preference without multiplying the OS DPI twice.
/// Monitor dimensions are physical pixels, independent of window size and UI zoom.
pub(super) fn display_scale(pref: prefs::UiScale, native: Option<f32>) -> f32 {
    let native = native.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(1.0);
    match pref {
        // The system's scale, fractional ones included (125%, 150%, 175%: #1072), and 100% on a
        // large 4K display run unscaled (#1278); 200% there is a fixed choice.
        prefs::UiScale::Auto => native,
        fixed => fixed.name().parse::<f32>().map_or(1.0, |pct| pct / 100.0),
    }
}

pub(super) fn sync_display_scale(app: &PhotocraftApp, ctx: &egui::Context) {
    craft_ui::interface_scale::InterfaceScale::from_code(app.session.prefs().interface.ui_scale.name()).unwrap_or_default().apply(ctx);
}

/// Interface › Show Tooltips and Tools › Show Tooltips (either one off hides them): egui never
/// shows a tooltip whose delay is infinite. Re-checked every frame because a theme change
/// rebuilds the style; that's one style read, and a write only when it differs.
pub(super) fn sync_tooltips(app: &PhotocraftApp, ctx: &egui::Context) {
    let p = app.session.prefs();
    let delay = if p.interface.show_tooltips && p.tools.show_tooltips { crate::theme::TOOLTIP_DELAY } else { f32::INFINITY };
    if ctx.global_style().interaction.tooltip_delay != delay {
        ctx.global_style_mut(|s| s.interaction.tooltip_delay = delay);
    }
}

/// Interface › Enable Animations: off, egui's fades and slides take no time and scroll areas jump
/// straight to their target (#3273). The selection already asks for reduced motion; this
/// re-checks every frame because a theme change rebuilds the style, and covers the scroll glide,
/// which the library's reduced-motion style leaves on. On, the library's motion stands. Both the
/// dark and light styles are set, so a system appearance switch keeps it.
pub(super) fn sync_animations(app: &PhotocraftApp, ctx: &egui::Context) {
    if app.session.prefs().interface.enable_animations {
        return;
    }
    let none = egui::style::ScrollAnimation::none();
    if [egui::Theme::Dark, egui::Theme::Light].iter().any(|t| {
        let s = ctx.style_of(*t);
        s.animation_time != 0.0 || s.scroll_animation != none
    }) {
        ctx.all_styles_mut(|s| {
            s.animation_time = 0.0;
            s.scroll_animation = egui::style::ScrollAnimation::none();
        });
    }
}

/// Resolves the suite's selection with PhotoCraft's own choice on top, and installs the result
/// when it differs from what is shown. Called every frame; it resolves only when the selection,
/// the OS appearance or the store's contents changed.
pub(crate) fn sync_appearance(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if app.prefs_rt.suite.client.is_none() {
        open_suite(app, ctx);
    }
    let system = SystemAppearance { mode: system_theme(app, ctx).map(|t| if t == egui::Theme::Dark { Mode::Dark } else { Mode::Light }), ..Default::default() };
    let defaults = app_defaults();
    let (sel, generation) = {
        let interface = &app.session.prefs().interface;
        let rt = &mut app.prefs_rt.suite;
        let Some(client) = rt.client.as_mut() else { return };
        client.sync();
        let sel = app_selection(interface, &client.shown());
        client.set_app_theme(app_choice(interface, &sel).theme.filter(|_| !interface.theme_light.is_empty() || !interface.theme_dark.is_empty()));
        (sel, client.snapshot().generation)
    };
    persist_suite(app);
    let key = (sel.clone(), system, generation);
    let rt = &mut app.prefs_rt.suite;
    // A restored or embedding-supplied pin must not suppress native appearance events while the
    // mode follows the system.
    if sel.mode == ModeChoice::System && ctx.options(|o| o.theme_preference) != egui::ThemePreference::System && ctx.system_theme().is_some() {
        ctx.set_theme(egui::ThemePreference::System);
    }
    // A kind set from outside (tests, `ui.set`) is put back to what the selection resolves to.
    if rt.resolved.as_ref() == Some(&key) && rt.shown.as_ref().is_some_and(|(kind, _)| *kind == app.ui.theme) {
        return;
    }
    let Some(client) = rt.client.as_ref() else { return };
    let app_id = AppId::parse("photocraft").ok();
    let mut base = craft_ui::theme::Base::resolve(&sel, system, &defaults, app_id.as_ref(), &mut |id| client.theme(id));
    let mode = match sel.mode {
        ModeChoice::Light => Mode::Light,
        ModeChoice::Dark => Mode::Dark,
        ModeChoice::System => system.mode.unwrap_or(Mode::Dark),
    };
    // egui follows the OS appearance itself; where only the app's service knows it (Wayland),
    // pin the mode it reports.
    if sel.mode == ModeChoice::System && ctx.system_theme().is_none() {
        base.mode = if mode == Mode::Dark { ModeChoice::Dark } else { ModeChoice::Light };
    }
    let shown = base.shown(mode);
    let id = shown.theme.as_str().to_string();
    let kind = ThemeKind::of_library(&id, shown.resolved.variant_used == Mode::Dark, sel.size);
    rt.resolved = Some(key);
    rt.shown = Some((kind, id));
    app.apply_theme_base(ctx, &base, kind);
}

/// Keeps the last accepted suite selection in the preferences (the web build's only copy).
fn persist_suite(app: &mut PhotocraftApp) {
    let Some(client) = app.prefs_rt.suite.client.as_ref() else { return };
    let accepted = client.accepted();
    if accepted.revision == 0 && accepted.suite.is_empty() {
        return;
    }
    let Ok(value) = serde_json::to_value(accepted) else { return };
    if app.session.prefs().interface.last_suite != value {
        let _ = app.session.execute("prefs.set", json!({"path": "interface.lastSuite", "value": value}));
    }
}

/// One theme as a Color Theme swatch: its panel colour under a chrome strip with an accent dot,
/// outlined when chosen. The name is the tooltip and the accessible label. A star badge on its
/// corner adds the theme to (or removes it from) the favourites listed by the appearance
/// button's menu; it shows while the swatch is hovered, and stays for a favourite.
fn theme_swatch(ui: &mut egui::Ui, choice: &ThemeChoice, mode: Mode, selected: &mut String, favorites: &mut Vec<String>) {
    const BADGE: f32 = 7.0;
    let t = Tokens::get(ui.ctx());
    let id = choice.id.as_str();
    let label = tl!(&choice.name).to_string();
    let appearance = Appearance { mode, contrast: Default::default(), size: Size::Compact, text_size: Default::default(), reduced_motion: false };
    let c = craft_theme::resolve(&choice.def, appearance, None).tokens.color;
    let col = |v: craft_theme::Rgba| craft_ui::theme::ToEgui::to_egui(v);
    let on = selected.as_str() == id;
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 26.0), Sense::click());
    let painter = ui.painter();
    let inner = rect.shrink(3.0);
    painter.rect_filled(inner, 2.0, col(c.surface.background));
    painter.rect_filled(egui::Rect::from_min_size(inner.min, vec2(inner.width(), 5.0)), 0.0, col(c.surface.chrome));
    painter.circle_filled(inner.right_bottom() - vec2(6.0, 6.0), 2.5, col(c.accent.solid));
    let stroke = if on {
        egui::Stroke::new(2.0, t.accent)
    } else if resp.hovered() {
        egui::Stroke::new(1.0, t.text_dim)
    } else {
        egui::Stroke::new(1.0, t.card_border)
    };
    painter.rect_stroke(rect.shrink(1.0), 3.0, stroke, egui::StrokeKind::Inside);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, on, &label));
    if resp.on_hover_text(&label).clicked() {
        *selected = id.to_string();
    }
    let favorite = favorites.iter().any(|f| f == id);
    let center = rect.right_top() + vec2(-3.0, 3.0);
    let badge = egui::Rect::from_center_size(center, vec2(BADGE * 2.0, BADGE * 2.0));
    if !(favorite || ui.rect_contains_pointer(rect.union(badge))) {
        return;
    }
    ui.painter().circle(center, BADGE, t.card, egui::Stroke::new(1.0, t.card_border));
    let (icon, tint) = if favorite { ("star-filled", t.accent) } else { ("star", t.text_dim) };
    let action = if favorite { tl!("Remove from favorites") } else { tl!("Add to favorites") };
    // Interact and paint without allocating: a widget placed at `badge` would grow the row by the
    // 4 px the badge overhangs the swatch, re-centring the swatches after this one whenever the
    // pointer arrives, and oscillating when that moves the swatch out from under it.
    let response = ui.interact(badge, ui.scope_id().with((id, "favorite")), Sense::click()).on_hover_text(action);
    crate::icons::paint(ui, badge, icon, 10.0, tint);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{action}: {label}")));
    if response.clicked() {
        if favorite {
            favorites.retain(|saved| saved != id);
        } else {
            favorites.push(id.to_string());
        }
    }
}

/// A grid row of theme swatches, marked Active when the appearance mode shows this family.
fn theme_row(ui: &mut egui::Ui, title: &str, active: bool, mode: Mode, selected: &mut String, favorites: &mut Vec<String>, options: &[ThemeChoice]) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(tl!(title)).color(t.text_dim));
    ui.horizontal_wrapped(|ui| {
        // Room for the favourite star badges on the swatch corners.
        ui.spacing_mut().item_spacing.x = 6.0;
        for choice in options {
            theme_swatch(ui, choice, mode, selected, favorites);
        }
        if active {
            ui.add_space(6.0);
            ui.label(RichText::new(tl!("Active")).color(t.accent_text).small());
        }
    });
    ui.end_row();
}

/// Which of the Appearance rows drawn by `appearance_rows` a search shows.
#[derive(Clone, Copy)]
pub(super) struct AppearanceRows {
    mode: bool,
    light: bool,
    dark: bool,
    size: bool,
}

impl AppearanceRows {
    /// `query` is lowercased; empty shows every row.
    pub(super) fn matching(query: &str) -> Self {
        let shown = |key: &str| query.is_empty() || preference_matches("interface", key, query);
        Self { mode: shown("appearanceMode"), light: shown("lightTheme"), dark: shown("darkTheme"), size: shown("size") }
    }

    pub(super) fn any(self) -> bool {
        self.mode || self.light || self.dark || self.size
    }
}

/// The themes the Appearance rows offer, set by the dialog body each frame.
pub(crate) fn offer_themes(ctx: &egui::Context, themes: Vec<ThemeChoice>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("appearance-theme-choices"), Arc::new(themes)));
}

/// Appearance Mode, the light and dark Color Theme swatches and Size, as rows of the Appearance
/// grid. The swatches write `themeLight`/`themeDark`; an empty slot follows the suite.
pub(super) fn appearance_rows(ui: &mut egui::Ui, obj: &mut Map<String, Value>, system: Option<egui::Theme>, rows: AppearanceRows) {
    let t = Tokens::get(ui.ctx());
    let choices: Arc<Vec<ThemeChoice>> = ui.ctx().data(|d| d.get_temp(egui::Id::new("appearance-theme-choices"))).unwrap_or_default();
    let mut mode = obj.get("appearanceMode").and_then(Value::as_str).unwrap_or("dark").to_string();
    if rows.mode {
        ui.label(RichText::new(tl!("Appearance Mode")).color(t.text_dim));
        let modes: [(String, &str); 3] = [("auto".into(), tl!("Sync with system")), ("dark".into(), tl!("Dark")), ("light".into(), tl!("Light"))];
        let pairs: Vec<(String, &str)> = modes.iter().map(|(id, label)| (id.clone(), *label)).collect();
        crate::widgets::dropdown(ui, "appearance-mode", tl!("Appearance Mode"), &mut mode, &pairs, 220.0);
        ui.end_row();
    }
    let light_active = mode == "light" || (mode == "auto" && system == Some(egui::Theme::Light));
    obj.insert("appearanceMode".into(), json!(mode));
    let text = |obj: &Map<String, Value>, k: &str| obj.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let (mut light, mut dark) = (text(obj, "themeLight"), text(obj, "themeDark"));
    let (was_light, was_dark) = (light.clone(), dark.clone());
    let mut favorites: Vec<String> = obj.get("favoriteThemes").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    if rows.light {
        let options: Vec<ThemeChoice> = choices.iter().filter(|c| c.variants != Variants::DarkOnly).cloned().collect();
        theme_row(ui, "Light Theme", light_active, Mode::Light, &mut light, &mut favorites, &options);
    }
    if rows.dark {
        let options: Vec<ThemeChoice> = choices.iter().filter(|c| c.variants != Variants::LightOnly).cloned().collect();
        theme_row(ui, "Dark Theme", !light_active, Mode::Dark, &mut dark, &mut favorites, &options);
    }
    obj.insert("themeLight".into(), json!(light));
    obj.insert("themeDark".into(), json!(dark));
    obj.insert("favoriteThemes".into(), json!(favorites));
    // A built-in theme picked here brings the Size that keeps its geometry.
    let picked = if light != was_light {
        Some(&light)
    } else if dark != was_dark {
        Some(&dark)
    } else {
        None
    };
    if let Some(size) = picked.and_then(|id| default_size(id)) {
        obj.insert("size".into(), json!(size));
    }
    if rows.size {
        let mut size = text(obj, "size");
        if size.is_empty() {
            size = "suite".into();
        }
        ui.label(RichText::new(tl!("Size")).color(t.text_dim));
        let sizes: [(String, &str); 3] =
            [("suite".into(), tl!("Suite default")), ("compact".into(), tl!("Compact")), ("comfortable".into(), tl!("Comfortable"))];
        let pairs: Vec<(String, &str)> = sizes.iter().map(|(id, label)| (id.clone(), *label)).collect();
        crate::widgets::dropdown(ui, "appearance-size", tl!("Size"), &mut size, &pairs, 220.0);
        obj.insert("size".into(), json!(size));
        ui.end_row();
    }
}
