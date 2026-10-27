use super::dialogs::open_preferences;
use super::fields::{choice_label, color_of, preference_matches};
use super::*;
use craft_theme::{Mode, ThemeId};
use craft_theme_store::{SelectionChange, SuiteChoice};

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

/// PhotoCraft's own (Light, Dark) themes: what `craft.app-default` stands for.
pub(super) fn own_themes(interface: &prefs::Interface) -> (ThemeKind, ThemeKind) {
    (light_kind(interface.light_theme), dark_kind(interface.dark_theme))
}

/// PhotoCraft's own themes for egui's Light and Dark style slots. A fixed mode puts its theme in
/// both, so the OS appearance cannot change what is drawn.
#[cfg(test)]
pub(super) fn slot_themes(interface: &prefs::Interface) -> (ThemeKind, ThemeKind) {
    let (light, dark) = own_themes(interface);
    match interface.appearance_mode {
        AppearanceMode::Light => (light, light),
        AppearanceMode::Dark => (dark, dark),
        AppearanceMode::Auto => (light, dark),
    }
}

/// The theme on screen when the OS reports `system`.
#[cfg(test)]
pub(super) fn selected_theme(interface: &prefs::Interface, system: Option<egui::Theme>) -> ThemeKind {
    let (light, dark) = slot_themes(interface);
    if system == Some(egui::Theme::Light) { light } else { dark }
}

fn light_kind(theme: LightTheme) -> ThemeKind {
    match theme {
        LightTheme::StudioLight => ThemeKind::StudioLight,
        LightTheme::ProLightGray => ThemeKind::ProLightGray,
        LightTheme::ProLight => ThemeKind::ProLight,
        LightTheme::Classic => ThemeKind::Classic,
        LightTheme::Adwaita => ThemeKind::Adwaita,
        LightTheme::BreezeLight => ThemeKind::BreezeLight,
        LightTheme::SoftLight => ThemeKind::SoftLight,
    }
}

fn dark_kind(theme: DarkTheme) -> ThemeKind {
    match theme {
        DarkTheme::Pro => ThemeKind::Pro,
        DarkTheme::ProMedium => ThemeKind::ProMedium,
        DarkTheme::Studio => ThemeKind::Studio,
        DarkTheme::SolarizedDark => ThemeKind::SolarizedDark,
        DarkTheme::AdwaitaDark => ThemeKind::AdwaitaDark,
        DarkTheme::BreezeDark => ThemeKind::BreezeDark,
        DarkTheme::SoftDark => ThemeKind::SoftDark,
    }
}

/// The title-bar mode button and Window › Next Appearance Mode: Sync with system, Light, Dark in
/// turn, for the choice that applies (the suite's while following it).
pub(crate) fn cycle_appearance(app: &mut PhotocraftApp, ctx: &egui::Context) {
    use craft_theme::Mode;
    let next = match effective_selection(app).mode {
        Mode::System => Mode::Light,
        Mode::Light => Mode::Dark,
        Mode::Dark => Mode::System,
    };
    if let Err(e) = set_scope_mode(app, ctx, next) {
        app.ui.status = e;
    }
}

/// Sets the appearance mode of the choice that applies: the suite's while following it (shared
/// with the other Craft apps), otherwise PhotoCraft's own (`appearanceMode`).
pub(crate) fn set_scope_mode(app: &mut PhotocraftApp, ctx: &egui::Context, mode: craft_theme::Mode) -> Result<(), String> {
    if app_choice(app) == craft_theme::AppChoice::FollowSuite {
        app.suite.change(SelectionChange::mode(mode));
    } else {
        app.run("prefs.set", json!({"path": "interface.appearanceMode", "value": mode_name(mode)}))?;
    }
    sync_appearance(app, ctx);
    Ok(())
}

/// A mode as `appearanceMode` and the Appearance Mode dropdown name it.
fn mode_name(mode: craft_theme::Mode) -> &'static str {
    match mode {
        craft_theme::Mode::System => "auto",
        craft_theme::Mode::Dark => "dark",
        craft_theme::Mode::Light => "light",
    }
}

fn mode_of(name: &str) -> craft_theme::Mode {
    match name {
        "light" => craft_theme::Mode::Light,
        "dark" => craft_theme::Mode::Dark,
        _ => craft_theme::Mode::System,
    }
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

pub(super) const LIGHT_THEME_OPTIONS: &[(&str, ThemeKind)] = &[
    ("proLight", ThemeKind::ProLight),
    ("proLightGray", ThemeKind::ProLightGray),
    ("studioLight", ThemeKind::StudioLight),
    ("classic", ThemeKind::Classic),
    ("adwaita", ThemeKind::Adwaita),
    ("breezeLight", ThemeKind::BreezeLight),
    ("softLight", ThemeKind::SoftLight),
];

pub(super) const DARK_THEME_OPTIONS: &[(&str, ThemeKind)] = &[
    ("proMedium", ThemeKind::ProMedium),
    ("pro", ThemeKind::Pro),
    ("studio", ThemeKind::Studio),
    ("solarizedDark", ThemeKind::SolarizedDark),
    ("adwaitaDark", ThemeKind::AdwaitaDark),
    ("breezeDark", ThemeKind::BreezeDark),
    ("softDark", ThemeKind::SoftDark),
];

pub(super) fn favorite_theme_groups(favorites: &[SavedTheme]) -> (Vec<ThemeKind>, Vec<ThemeKind>) {
    let group = |options: &[(&str, ThemeKind)]| {
        options.iter().filter_map(|(id, kind)| SavedTheme::parse(id).filter(|theme| favorites.contains(theme)).map(|_| *kind)).collect()
    };
    (group(LIGHT_THEME_OPTIONS), group(DARK_THEME_OPTIONS))
}

/// Right-click menu on the top-right appearance button. A plain click still cycles modes.
pub(crate) fn appearance_context_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if ui.button(tl!("Appearance Settings…")).clicked() {
        open_preferences(app, "interface");
        ui.close();
    }
    let (light, dark) = favorite_theme_groups(&app.session.prefs().interface.favorite_themes);
    let own = own_themes_shown(app);
    let has_light = !light.is_empty();
    if !light.is_empty() || !dark.is_empty() {
        ui.separator();
    }
    for kind in light {
        if ui.selectable_label(own && app.ui.theme == kind, tl!(kind.label())).clicked() {
            app.set_theme(ui.ctx(), kind);
            ui.close();
        }
    }
    if has_light && !dark.is_empty() {
        ui.separator();
    }
    for kind in dark {
        if ui.selectable_label(own && app.ui.theme == kind, tl!(kind.label())).clicked() {
            app.set_theme(ui.ctx(), kind);
            ui.close();
        }
    }
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
    let native = ctx.native_pixels_per_point();
    let native_scale = native.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(1.0);
    let scale = display_scale(app.session.prefs().interface.ui_scale, native);
    ctx.set_zoom_factor(scale / native_scale);
}

/// Interface › Show Tooltips and Tools › Show Tooltips (either one off hides them): egui never
/// shows a tooltip whose delay is infinite. Re-checked every frame because a theme change
/// rebuilds the style; that's one style read, and a write only when it differs.
pub(super) fn sync_tooltips(app: &PhotocraftApp, ctx: &egui::Context) {
    let p = app.session.prefs();
    let delay = if p.interface.show_tooltips && p.tools.show_tooltips { crate::theme::TOOLTIP_DELAY } else { f32::INFINITY };
    if ctx.global_style().interaction.tooltip_delay != delay {
        ctx.all_styles_mut(|s| s.interaction.tooltip_delay = delay);
    }
}

/// Interface › Enable Animations: off, egui's fades and slides take no time and scroll areas jump
/// straight to their target (#3273). Re-checked every frame because a theme change rebuilds the
/// style; both the dark and light styles are set, so a system appearance switch keeps it.
pub(super) fn sync_animations(app: &PhotocraftApp, ctx: &egui::Context) {
    let on = app.session.prefs().interface.enable_animations;
    let time = if on { egui::Style::default().animation_time } else { 0.0 };
    if ctx.global_style().animation_time != time {
        let scroll = if on { egui::style::ScrollAnimation::default() } else { egui::style::ScrollAnimation::none() };
        ctx.all_styles_mut(|s| {
            s.animation_time = time;
            s.scroll_animation = scroll;
        });
    }
}

/// Resolve the appearance choice (PhotoCraft's own themes or a suite theme) and the suite's
/// selection without changing either. The styles are rebuilt only when a slot's tokens changed;
/// an OS appearance change only moves the layout to the theme of the slot egui now draws.
pub(crate) fn sync_appearance(app: &mut PhotocraftApp, ctx: &egui::Context) {
    // A restored or embedding-supplied pin must not suppress native appearance events.
    ctx.set_theme(egui::ThemePreference::System);
    // Every frame, not only when this sync saw a change: `Suite::slots` syncs again and may be
    // the one that takes a new snapshot.
    app.suite.sync();
    remember_last_suite(app);
    let interface = &app.session.prefs().interface;
    let slots = app.suite.slots(&choice_where(interface, app.suite.is_shared()), own_themes(interface));
    if crate::theme::installed_tokens(ctx) != Some(slots) {
        crate::theme::install_tokens(ctx, slots.0, slots.1);
        app.checker = None;
    }
    let selected = if ctx.theme() == egui::Theme::Light { slots.0.kind } else { slots.1.kind };
    if app.ui.theme != selected {
        app.sync_layout_theme(selected);
    }
}

/// `ui.set` `followSuite` and `themeId`: turn following the suite on or off, then pick `theme`
/// for the suite while following, or for PhotoCraft's own choice otherwise. `theme` must be one
/// [`crate::theme::suite::Suite::knows`].
pub(crate) fn choose_suite_appearance(app: &mut PhotocraftApp, ctx: &egui::Context, follow: Option<bool>, theme: Option<craft_theme::ThemeId>) {
    let current = app.session.prefs().interface.suite_appearance.clone();
    let mut next = current.clone().unwrap_or_default();
    if let Some(follow) = follow {
        next.follow_suite = follow;
    }
    if let Some(id) = theme {
        if next.follow_suite {
            app.suite.set_theme(id);
        } else {
            next.theme_id = id;
        }
    }
    if current.as_ref() != Some(&next) {
        let value = serde_json::to_value(&next).unwrap_or_default();
        if let Err(e) = app.run("prefs.set", json!({"path": "interface.suiteAppearance", "value": value})) {
            app.ui.status = e;
        }
    }
    sync_appearance(app, ctx);
}

/// The appearance choice for `ui.inspect`: whether PhotoCraft follows the suite, the selection
/// that applies, PhotoCraft's own choice and the suite's.
pub(crate) fn inspect_appearance(app: &PhotocraftApp) -> Value {
    let interface = &app.session.prefs().interface;
    let choice = choice_where(interface, app.suite.is_shared());
    let effective = app.suite.effective(&choice);
    let own = match &choice {
        craft_theme::AppChoice::Own(sel) => sel.theme.clone(),
        craft_theme::AppChoice::FollowSuite => interface.suite_appearance.as_ref().map_or_else(craft_theme::ThemeId::app_default, |s| s.theme_id.clone()),
    };
    let suite = app.suite.shown();
    json!({
        "followSuite": matches!(choice, craft_theme::AppChoice::FollowSuite),
        "themeId": effective.theme,
        "mode": effective.mode,
        "ownThemeId": own,
        "suite": {
            "themeId": suite.theme,
            "mode": suite.mode,
            "revision": app.suite.accepted().revision,
            "pending": app.suite.client().is_pending(),
            "conflict": app.suite.conflict().map(|(disk, _)| json!({"themeId": disk.suite.theme, "mode": disk.suite.mode})),
            "failure": app.suite.failure().map(|(_, e)| e.to_string()),
            "problems": app.suite.problems().iter().map(|p| p.to_string()).collect::<Vec<_>>(),
        },
    })
}

/// The theme and mode that apply now: PhotoCraft's own choice, or the suite's while following it.
pub(crate) fn effective_selection(app: &PhotocraftApp) -> craft_theme::Selection {
    app.suite.effective(&app_choice(app))
}

/// PhotoCraft's appearance choice as it applies here: the saved one, except that following the
/// suite needs a shared suite folder. Without one (a portable install with a copied desktop
/// file, `prefs.set`) the saved `followSuite` is ignored and PhotoCraft's own choice applies,
/// as Preferences shows it.
pub(crate) fn app_choice(app: &PhotocraftApp) -> craft_theme::AppChoice {
    choice_where(&app.session.prefs().interface, app.suite.is_shared())
}

fn choice_where(interface: &prefs::Interface, shared: bool) -> craft_theme::AppChoice {
    match interface.app_choice() {
        craft_theme::AppChoice::FollowSuite if !shared => craft_theme::AppChoice::Own(interface.own_selection()),
        choice => choice,
    }
}

/// The error for turning on `followSuite` where the suite isn't shared (`ui.set`, `prefs.set`).
pub(crate) const FOLLOW_WITHOUT_SUITE: &str = "followSuite: the Craft suite's theme isn't shared here (portable or web); pick a themeId for PhotoCraft";

/// `prefs.set` may not turn on `followSuite` where the suite isn't shared: Preferences hides the
/// choice there, and it would not apply. A value already saved (a copied file) may stay.
pub(crate) fn refuse_follow_without_suite(app: &PhotocraftApp, params: &Value) -> Result<(), String> {
    const KEY: [&str; 3] = ["interface", "suiteAppearance", "followSuite"];
    let following = app.session.prefs().interface.suite_appearance.as_ref().is_some_and(|s| s.follow_suite);
    if app.suite.is_shared() || following {
        return Ok(());
    }
    let mut changes: Vec<(&str, &Value)> = Vec::new();
    if let (Some(path), Some(value)) = (params.get("path").and_then(Value::as_str), params.get("value")) {
        changes.push((path, value));
    }
    if let Some(values) = params.get("values").and_then(Value::as_object) {
        changes.extend(values.iter().map(|(k, v)| (k.as_str(), v)));
    }
    let turns_on = changes.into_iter().any(|(path, value)| {
        let keys: Vec<&str> = path.split('.').filter(|k| !k.is_empty()).collect();
        keys.len() <= KEY.len()
            && keys.iter().zip(KEY).all(|(a, b)| *a == b)
            && KEY[keys.len()..].iter().try_fold(value, |v, k| v.get(*k)) == Some(&Value::Bool(true))
    });
    if turns_on { Err(FOLLOW_WITHOUT_SUITE.into()) } else { Ok(()) }
}

/// One of PhotoCraft's own themes is on screen (`craft.app-default`), not a suite theme on its
/// base layout: only then does a PhotoCraft theme show as chosen.
pub(crate) fn own_themes_shown(app: &PhotocraftApp) -> bool {
    effective_selection(app).theme.is_app_default()
}

/// Keep the suite selection PhotoCraft last saw in its preferences (`lastSuite`), so a damaged
/// or unreadable suite folder at the next start still shows the same look.
fn remember_last_suite(app: &mut PhotocraftApp) {
    // Nothing is written while the preferences file can't be read, so there is nothing to keep it in.
    if app.prefs_rt.unreadable.is_some() {
        return;
    }
    let accepted = app.suite.accepted();
    let saved = app.session.prefs().interface.suite_appearance.as_ref().map(|s| &s.last_suite);
    if saved.unwrap_or(&Default::default()) == accepted {
        return;
    }
    let accepted = accepted.clone();
    app.session.prefs.edit(|p| p.interface.suite_appearance.get_or_insert_with(Default::default).last_suite = accepted);
}

/// One theme as a Color Theme swatch: its panel colour under a chrome strip with an accent dot,
/// outlined when chosen. The name is the tooltip and the accessible label. A star badge on its
/// corner adds the theme to (or removes it from) the favourites listed by the appearance
/// button's menu; it shows while the swatch is hovered, and stays for a favourite.
fn theme_swatch(ui: &mut egui::Ui, kind: ThemeKind, id: &str, selected: &mut String, favorites: &mut Vec<SavedTheme>) {
    const BADGE: f32 = 7.0;
    let t = Tokens::get(ui.ctx());
    let p = Tokens::for_kind(kind);
    let label = tl!(&choice_label(id)).to_string();
    let on = selected.as_str() == id;
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 26.0), Sense::click());
    let painter = ui.painter();
    let inner = rect.shrink(3.0);
    painter.rect_filled(inner, 2.0, p.dock);
    painter.rect_filled(egui::Rect::from_min_size(inner.min, vec2(inner.width(), 5.0)), 0.0, p.chrome);
    painter.circle_filled(inner.right_bottom() - vec2(6.0, 6.0), 2.5, p.accent);
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
    let Some(theme) = SavedTheme::parse(id) else { return };
    let favorite = favorites.contains(&theme);
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
            favorites.retain(|saved| *saved != theme);
        } else {
            favorites.push(theme);
        }
    }
}

/// A grid row of theme swatches, marked Active when the appearance mode shows this family.
fn theme_row(ui: &mut egui::Ui, title: &str, active: bool, selected: &mut String, favorites: &mut Vec<SavedTheme>, options: &[(&str, ThemeKind)]) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(tl!(title)).color(t.text_dim));
    ui.horizontal(|ui| {
        // Room for the favourite star badges on the swatch corners.
        ui.spacing_mut().item_spacing.x = 6.0;
        for (id, kind) in options {
            theme_swatch(ui, *kind, id, selected, favorites);
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
    themes: bool,
    mode: bool,
    light: bool,
    dark: bool,
}

impl AppearanceRows {
    /// `query` is lowercased; empty shows every row.
    pub(super) fn matching(query: &str) -> Self {
        let shown = |key: &str| query.is_empty() || preference_matches("interface", key, query);
        Self { themes: shown("suiteAppearance"), mode: shown("appearanceMode"), light: shown("lightTheme"), dark: shown("darkTheme") }
    }

    pub(super) fn any(self) -> bool {
        self.themes || self.mode || self.light || self.dark
    }
}

/// What Preferences › Interface shows of the suite, read from the app before the dialog draws
/// (its rows edit only the draft), and the suite changes clicked while it drew. Those are shared
/// with the other Craft apps: while PhotoCraft follows the suite they apply at once, not on OK,
/// and Cancel keeps them. While only the draft follows it, they wait in the draft
/// ([`SUITE_PENDING`]) and apply on OK or Apply, so Cancel leaves the other apps alone.
pub(super) struct SuiteRows {
    /// The OS appearance answer.
    pub(super) system: Option<egui::Theme>,
    /// The suite folder is shared (otherwise in memory, and "Follow suite" is hidden).
    shared: bool,
    /// PhotoCraft follows the suite now (not only in the draft).
    live_follows: bool,
    /// Suite picks held in the draft until OK or Apply.
    pending: SuiteChoice,
    cards: Vec<ThemeCard>,
    /// The suite's selection as shown, with PhotoCraft's defaults filled in.
    suite: craft_theme::Selection,
    /// One line each, with the buttons that act on it.
    notices: Vec<(String, Vec<(&'static str, SuiteClick)>)>,
    clicked: Vec<SuiteClick>,
}

impl Default for SuiteRows {
    fn default() -> Self {
        let defaults = crate::theme::suite::app_defaults();
        SuiteRows {
            system: None,
            shared: false,
            live_follows: false,
            pending: SuiteChoice::default(),
            cards: Vec::new(),
            suite: craft_theme::Selection { theme: defaults.theme, mode: defaults.mode },
            notices: Vec::new(),
            clicked: Vec::new(),
        }
    }
}

/// A suite theme as a card: its preview tokens for (Light, Dark), `None` while its file isn't read,
/// and its contrast findings, described.
struct ThemeCard {
    id: ThemeId,
    name: String,
    preview: Option<(Tokens, Tokens)>,
    diagnostics: Vec<String>,
}

#[derive(Clone, Debug)]
pub(super) enum SuiteClick {
    Change(SelectionChange),
    ApplyMine,
    Retry,
    Dismiss,
}

/// Store problems listed under the theme cards before the rest are counted.
const PROBLEMS_SHOWN: usize = 3;

/// Where the Preferences dialog holds suite picks made while only its draft follows the suite.
pub(super) const SUITE_PENDING: &str = "__suitePending";

/// Reads the suite for the Appearance rows; `pending` is the dialog's [`SUITE_PENDING`].
pub(super) fn suite_rows(app: &mut PhotocraftApp, system: Option<egui::Theme>, pending: Option<&Value>) -> SuiteRows {
    let live_follows = app_choice(app) == craft_theme::AppChoice::FollowSuite;
    let pending = pending.cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    app.suite.read_previews();
    let suite = &app.suite;
    let cards = suite
        .themes()
        .into_iter()
        .map(|(id, name)| ThemeCard {
            preview: if suite.is_loading(&id) { None } else { suite.preview(&id) },
            diagnostics: suite.diagnostics(&id).iter().map(describe_diagnostic).collect(),
            id,
            name,
        })
        .collect();
    let mut notices = Vec::new();
    if let Some(e) = &app.prefs_rt.unreadable {
        let line = crate::i18n::fmt(tl!("Preferences couldn't be read, so changes aren't saved ({error})."), &[("error", e)]);
        notices.push((line, Vec::new()));
    }
    if let Some((disk, mine)) = suite.conflict() {
        let shown = disk.suite.with_defaults(&crate::theme::suite::app_defaults());
        let line = if mine.theme.is_some() {
            crate::i18n::fmt(tl!("Another app changed the suite theme to {name}."), &[("name", &suite.name(&shown.theme))])
        } else {
            crate::i18n::fmt(tl!("Another app changed the suite appearance mode to {mode}."), &[("mode", mode_label(shown.mode))])
        };
        notices.push((line, vec![("Apply mine", SuiteClick::ApplyMine), ("Dismiss", SuiteClick::Dismiss)]));
    }
    if let Some((_, error)) = suite.failure() {
        let line = crate::i18n::fmt(tl!("The suite theme couldn't be saved ({error})."), &[("error", &error.to_string())]);
        notices.push((line, vec![("Retry", SuiteClick::Retry), ("Dismiss", SuiteClick::Dismiss)]));
    }
    let problems = suite.problems();
    notices.extend(problems.iter().take(PROBLEMS_SHOWN).map(|p| (p.to_string(), Vec::new())));
    if problems.len() > PROBLEMS_SHOWN {
        let more = (problems.len() - PROBLEMS_SHOWN).to_string();
        notices.push((crate::i18n::fmt(tl!("and {n} more"), &[("n", &more)]), Vec::new()));
    }
    let shown = suite.shown().with_defaults(&crate::theme::suite::app_defaults());
    SuiteRows { system, shared: suite.is_shared(), live_follows, pending, cards, suite: shown, notices, clicked: Vec::new() }
}

/// Applies what was clicked in the suite rows to the suite, and keeps the picks held for OK in
/// the dialog's fields `f`.
pub(super) fn apply_suite_clicks(app: &mut PhotocraftApp, ctx: &egui::Context, rows: SuiteRows, f: &mut Map<String, Value>) {
    // Once PhotoCraft follows the suite (Window › Theme › Follow suite while the dialog is open),
    // the cards show the live suite theme: a pick held from before would apply unseen on OK.
    if rows.pending.is_empty() || rows.live_follows {
        f.remove(SUITE_PENDING);
    } else {
        f.insert(SUITE_PENDING.into(), serde_json::to_value(&rows.pending).unwrap_or_default());
    }
    if rows.clicked.is_empty() {
        return;
    }
    for click in rows.clicked {
        match click {
            SuiteClick::Change(change) => app.suite.change(change),
            SuiteClick::ApplyMine => app.suite.apply_mine(),
            SuiteClick::Retry => app.suite.retry(),
            SuiteClick::Dismiss => app.suite.dismiss(),
        }
    }
    sync_appearance(app, ctx);
}

/// OK or Apply in Preferences: the suite picks held in the draft (`pending`, the dialog's
/// [`SUITE_PENDING`]) go to the suite once PhotoCraft follows it.
pub(super) fn apply_suite_pending(app: &mut PhotocraftApp, pending: Option<&Value>) {
    let Some(pending) = pending.cloned().and_then(|v| serde_json::from_value::<SuiteChoice>(v).ok()) else { return };
    if !pending.is_empty() && app_choice(app) == craft_theme::AppChoice::FollowSuite {
        app.suite.change(SelectionChange { theme: pending.theme, mode: pending.mode });
    }
}

/// Where the Preferences dialog keeps the [`LIVE_KEYS`] as they were when the draft last took
/// them.
pub(super) const DRAFT_BASE: &str = "__liveAppearance";

/// Interface keys that change outside Preferences while it is open: Window › Theme, the mode
/// button and its commands, `ui.set`, and the suite's `lastSuite`.
const LIVE_KEYS: [&str; 5] = ["suiteAppearance", "appearanceMode", "theme", "darkTheme", "lightTheme"];

/// Until the user edits one of the [`LIVE_KEYS`] in the dialog (its draft still equals `base`),
/// the draft takes the live value, so Apply stays off and OK writes nothing stale. An edited
/// draft keeps the user's choice; an edited `suiteAppearance` takes the live `lastSuite`, which is
/// never an edit. Returns the next base.
pub(super) fn sync_live_draft(app: &PhotocraftApp, base: Option<&Value>, values: &mut Value) -> Value {
    let live_prefs = app.session.prefs().to_json();
    let live_interface = live_prefs.get("interface");
    let mut next = Map::new();
    let Some(interface) = values.get_mut("interface").and_then(Value::as_object_mut) else { return Value::Object(next) };
    for key in LIVE_KEYS {
        let live = live_interface.and_then(|i| i.get(key)).cloned().unwrap_or_default();
        let draft = interface.get(key).cloned().unwrap_or_default();
        let key_base = base.and_then(|b| b.get(key));
        if key_base.is_none_or(|b| *b == draft) {
            if draft != live {
                interface.insert(key.into(), live.clone());
            }
            next.insert(key.into(), live);
            continue;
        }
        if key == "suiteAppearance"
            && let Some(draft) = interface.get_mut(key).and_then(Value::as_object_mut)
        {
            let last = live.get("lastSuite").cloned().unwrap_or_else(|| serde_json::to_value(craft_theme_store::Accepted::default()).unwrap_or_default());
            if draft.get("lastSuite") != Some(&last) {
                draft.insert("lastSuite".into(), last);
            }
        }
        next.insert(key.into(), key_base.cloned().unwrap_or_default());
    }
    Value::Object(next)
}

fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::System => tl!("Sync with system"),
        Mode::Dark => tl!("Dark"),
        Mode::Light => tl!("Light"),
    }
}

/// A theme colour as the contrast findings name it: a core role of craft-theme's role list, or
/// one of the states derived from them.
fn paint_label(p: craft_theme::contrast::Paint) -> String {
    use craft_theme::Role;
    use craft_theme::contrast::Paint;
    let label = match p {
        Paint::Role(role) => match role {
            Role::Background => tl!("Background"),
            Role::Surface => tl!("Surface"),
            Role::Raised => tl!("Raised surface"),
            Role::Text => tl!("Text"),
            Role::TextMuted => tl!("Muted text"),
            Role::Line => tl!("Lines"),
            Role::Accent => tl!("Accent"),
            Role::OnAccent => tl!("Text on accent"),
            Role::Selection => tl!("Selection"),
            Role::Focus => tl!("Focus ring"),
            Role::Good => tl!("Success"),
            Role::Warn => tl!("Warning"),
            Role::Bad => tl!("Error"),
        },
        Paint::Derived(name) => match name {
            "hover" => tl!("Hover"),
            "pressed" => tl!("Pressed"),
            "selected_hover" => tl!("Selected hover"),
            "accent_hover" => tl!("Accent hover"),
            "accent_pressed" => tl!("Accent pressed"),
            "accent_text" => tl!("Accent text"),
            "accent_soft" => tl!("Soft accent"),
            "text_faint" => tl!("Faint text"),
            "control_edge" => tl!("Control edges"),
            other => return other.replace('_', " "),
        },
    };
    label.to_string()
}

/// "Text on Surface is 2.10:1, needs 4.5:1 (dark)".
fn describe_diagnostic(d: &craft_theme::contrast::Diagnostic) -> String {
    let paint = paint_label;
    let variant = match d.variant {
        craft_theme::Variant::Light => tl!("light"),
        craft_theme::Variant::Dark => tl!("dark"),
    };
    let args = [
        ("fg", paint(d.fg)),
        ("bg", paint(d.bg)),
        ("ratio", format!("{:.2}", d.ratio)),
        ("need", format!("{:.1}", d.need.min_ratio())),
        ("variant", variant.to_string()),
    ];
    let args: Vec<(&str, &str)> = args.iter().map(|(k, v)| (*k, v.as_str())).collect();
    crate::i18n::fmt(tl!("{fg} on {bg} is {ratio}:1, needs {need}:1 ({variant})"), &args)
}

/// Size of a theme card's preview.
const CARD: egui::Vec2 = vec2(96.0, 64.0);

/// One theme as a card: a miniature of its colours (dock, a chrome strip, a card with a text and
/// a dim text line, an accent dot), its name under it (cut to the card's width; `tip` has it
/// whole), outlined when chosen. Without `preview` (a library theme not read yet) the card is
/// blank. A radio button for accessibility.
fn theme_card(ui: &mut egui::Ui, name: &str, preview: Option<&Tokens>, on: bool, tip: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(CARD + vec2(0.0, 18.0), Sense::click());
    let thumb = egui::Rect::from_min_size(rect.min, CARD);
    let painter = ui.painter();
    let inner = thumb.shrink(3.0);
    if let Some(c) = preview {
        painter.rect_filled(inner, 2.0, c.dock);
        painter.rect_filled(egui::Rect::from_min_size(inner.min, vec2(inner.width(), 9.0)), 0.0, c.chrome);
        let block = egui::Rect::from_min_max(inner.min + vec2(8.0, 16.0), inner.max - vec2(22.0, 8.0));
        painter.rect(block, 2.0, c.card, egui::Stroke::new(1.0, c.card_border), egui::StrokeKind::Inside);
        let line = |y: f32, width: f32, color: Color32| {
            painter.rect_filled(egui::Rect::from_min_size(block.min + vec2(6.0, y), vec2(width, 3.0)), 1.0, color);
        };
        line(9.0, block.width() - 14.0, c.text);
        line(18.0, (block.width() - 14.0) * 0.6, c.text_dim);
        painter.circle_filled(inner.right_bottom() - vec2(9.0, 9.0), 4.0, c.accent);
    } else {
        painter.rect_filled(inner, 2.0, t.field);
    }
    let stroke = if on {
        egui::Stroke::new(2.0, t.accent)
    } else if resp.hovered() {
        egui::Stroke::new(1.0, t.text_dim)
    } else {
        egui::Stroke::new(1.0, t.card_border)
    };
    painter.rect_stroke(thumb.shrink(1.0), 4.0, stroke, egui::StrokeKind::Inside);
    let mut job = egui::text::LayoutJob::simple_singleline(name.to_owned(), egui::FontId::proportional(11.5), if on { t.text } else { t.text_dim });
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width());
    let galley = painter.layout_job(job);
    painter.galley(egui::pos2(rect.center().x - galley.size().x / 2.0, thumb.bottom() + 3.0), galley, t.text);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, on, name));
    resp.on_hover_text(tip)
}

/// A notice line under the theme cards, marked with a warning sign, with its buttons.
fn notice_row(ui: &mut egui::Ui, line: &str, tip: Option<&str>, buttons: &[(&'static str, SuiteClick)], clicked: &mut Vec<SuiteClick>) {
    let t = Tokens::get(ui.ctx());
    ui.label("");
    ui.horizontal_wrapped(|ui| {
        // The warning colour marks the line; the text stays in the text colour, since warning
        // fills don't reach text contrast on every theme.
        ui.add(crate::icons::image("triangle-alert", 12.0, t.warning));
        let label = ui.label(RichText::new(line).color(t.text));
        if let Some(tip) = tip {
            label.on_hover_text(tip);
        }
        for (label, click) in buttons {
            if crate::widgets::compact_button(ui, tl!(label), 20.0).clicked() {
                clicked.push(click.clone());
            }
        }
    });
    ui.end_row();
}

/// The Appearance rows of Preferences › Interface: whether PhotoCraft follows the suite, the
/// theme cards, notices about the suite, Appearance Mode, and the PhotoCraft themes' light and
/// dark swatches when they are this app's choice. Edits to PhotoCraft's own choice go into the
/// draft (`obj`); suite changes go into `suite.clicked`.
pub(super) fn appearance_rows(ui: &mut egui::Ui, obj: &mut Map<String, Value>, suite: &mut SuiteRows, rows: AppearanceRows) {
    let t = Tokens::get(ui.ctx());
    let saved: Option<prefs::SuiteAppearance> = obj.get("suiteAppearance").cloned().and_then(|v| serde_json::from_value(v).ok()).flatten();
    let mut own = saved.clone();
    let mut following = suite.shared && own.as_ref().is_some_and(|s| s.follow_suite);
    if rows.themes && suite.shared {
        ui.label("");
        ui.vertical(|ui| {
            if ui.radio(following, tl!("Follow suite")).clicked() {
                following = true;
            }
            ui.label(RichText::new(tl!("Changes here apply to every Craft app that follows the suite.")).color(t.text_faint).small());
            if ui.radio(!following, tl!("Use a different theme in this app")).clicked() {
                following = false;
            }
        });
        ui.end_row();
        if following != own.as_ref().is_some_and(|s| s.follow_suite) {
            own.get_or_insert_with(Default::default).follow_suite = following;
        }
    }
    // Suite picks wait in the draft while only the draft follows the suite.
    let hold = following && !suite.live_follows;
    let shown_suite = if hold {
        craft_theme::Selection {
            theme: suite.pending.theme.clone().unwrap_or_else(|| suite.suite.theme.clone()),
            mode: suite.pending.mode.unwrap_or(suite.suite.mode),
        }
    } else {
        suite.suite.clone()
    };
    let draft_mode = obj.get("appearanceMode").and_then(Value::as_str).unwrap_or("auto").to_string();
    let mode = if following { shown_suite.mode } else { mode_of(&draft_mode) };
    let light_shown = match mode {
        Mode::Light => true,
        Mode::Dark => false,
        Mode::System => suite.system == Some(egui::Theme::Light),
    };
    let pick = |pair: (Tokens, Tokens)| if light_shown { pair.0 } else { pair.1 };
    let own_theme = own.as_ref().map_or_else(ThemeId::app_default, |s| s.theme_id.clone());
    let chosen = if following { shown_suite.theme.clone() } else { own_theme };
    let mut light = obj.get("lightTheme").and_then(Value::as_str).unwrap_or("studioLight").to_string();
    let mut dark = obj.get("darkTheme").and_then(Value::as_str).unwrap_or("proMedium").to_string();
    if rows.themes {
        ui.label(RichText::new(if following { tl!("Suite theme") } else { tl!("Theme") }).color(t.text_dim));
        let mut picked = None;
        // Top-aligned: a wrapped row centres widgets taller than a line on it, over the rows around.
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true), |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for card in &suite.cards {
                let tip = card.diagnostics.first().map_or_else(|| card.name.clone(), |d| format!("{}\n{d}", card.name));
                if theme_card(ui, &card.name, card.preview.map(pick).as_ref(), chosen == card.id, &tip).clicked() {
                    picked = Some(card.id.clone());
                }
            }
            if !following {
                let kind = |options: &[(&str, ThemeKind)], id: &str| options.iter().find(|(o, _)| *o == id).map(|(_, k)| *k);
                let pair =
                    (kind(LIGHT_THEME_OPTIONS, &light).unwrap_or(ThemeKind::StudioLight), kind(DARK_THEME_OPTIONS, &dark).unwrap_or(ThemeKind::ProMedium));
                let preview = Tokens::for_kind(if light_shown { pair.0 } else { pair.1 });
                let name = tl!("PhotoCraft themes");
                if theme_card(ui, name, Some(&preview), chosen.is_app_default(), name).clicked() {
                    picked = Some(ThemeId::app_default());
                }
            }
        });
        ui.end_row();
        match picked {
            Some(id) if hold => suite.pending.theme = Some(id),
            Some(id) if following => suite.clicked.push(SuiteClick::Change(SelectionChange::theme(id))),
            Some(id) => {
                let own = own.get_or_insert_with(Default::default);
                own.follow_suite = false;
                own.theme_id = id;
            }
            None => {}
        }
        for (line, buttons) in &suite.notices {
            notice_row(ui, line, None, buttons, &mut suite.clicked);
        }
        if let Some(card) = suite.cards.iter().find(|c| c.id == chosen && !c.id.is_builtin())
            && let Some(first) = card.diagnostics.first()
        {
            let line = crate::i18n::fmt(tl!("{name} has low contrast: {finding}"), &[("name", &card.name), ("finding", first)]);
            notice_row(ui, &line, Some(&card.diagnostics.join("\n")), &[], &mut suite.clicked);
        }
    }
    if rows.mode {
        ui.label(RichText::new(tl!("Appearance Mode")).color(t.text_dim));
        let choices: [(String, &str); 3] = [("auto".into(), tl!("Sync with system")), ("dark".into(), tl!("Dark")), ("light".into(), tl!("Light"))];
        let pairs: Vec<(String, &str)> = choices.iter().map(|(id, label)| (id.clone(), *label)).collect();
        let mut shown = if following { mode_name(shown_suite.mode).to_string() } else { draft_mode.clone() };
        crate::widgets::dropdown(ui, "appearance-mode", tl!("Appearance Mode"), &mut shown, &pairs, 220.0);
        ui.end_row();
        if following {
            if shown != mode_name(shown_suite.mode) {
                if hold {
                    suite.pending.mode = Some(mode_of(&shown));
                } else {
                    suite.clicked.push(SuiteClick::Change(SelectionChange::mode(mode_of(&shown))));
                }
            }
        } else if shown != draft_mode {
            obj.insert("appearanceMode".into(), json!(shown));
        }
    }
    let own_rows = !following && own.as_ref().is_none_or(|s| s.theme_id.is_app_default());
    let mut favorites: Vec<SavedTheme> = obj.get("favoriteThemes").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    if own_rows && rows.light {
        theme_row(ui, "Light Theme", light_shown, &mut light, &mut favorites, LIGHT_THEME_OPTIONS);
    }
    if own_rows && rows.dark {
        theme_row(ui, "Dark Theme", !light_shown, &mut dark, &mut favorites, DARK_THEME_OPTIONS);
    }
    obj.insert("lightTheme".into(), json!(light));
    obj.insert("darkTheme".into(), json!(dark));
    obj.insert("favoriteThemes".into(), json!(favorites));
    if own != saved {
        obj.insert("suiteAppearance".into(), serde_json::to_value(&own).unwrap_or_default());
    }
}
