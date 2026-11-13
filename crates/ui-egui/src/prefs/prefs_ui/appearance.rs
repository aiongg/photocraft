use super::dialogs::open_preferences;
use super::fields::{color_of, preference_matches};
use super::{AppearanceMode, Color32, Map, PhotocraftApp, RichText, Sense, Tokens, Value, json, prefs, vec2};
use crate::theme::AppTheme;
use craft::theme::ThemeId;

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

/// The suite theme and mode the preferences show: the slot of the appearance mode, which `Auto`
/// takes from the system.
pub(super) fn selected_theme(interface: &prefs::Interface, system: Option<egui::Theme>) -> (ThemeId, bool) {
    let (slot, dark) = selected_slot(interface, system);
    (ThemeId::parse(slot).unwrap_or_else(|_| ThemeId::suite_default()), dark)
}

/// The theme id the mode shows, as the preferences hold it, and whether that mode is dark.
fn selected_slot(interface: &prefs::Interface, system: Option<egui::Theme>) -> (&str, bool) {
    let light = match interface.appearance_mode {
        AppearanceMode::Light => true,
        AppearanceMode::Dark => false,
        AppearanceMode::Auto => system == Some(egui::Theme::Light),
    };
    (if light { &interface.light_theme } else { &interface.dark_theme }, !light)
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

/// Darken Interface (⇧⌘F1, `lighter` false) and Lighten Interface (⇧⌘F2): the next of
/// Photoshop's four brightnesses from the theme on screen, stopping at either end, as in
/// Photoshop. Another theme steps from the nearest brightness: Medium Gray for a dark one, Light
/// Gray for a light one. The choice is saved like picking it in Preferences › Interface.
pub(crate) fn step_brightness(app: &mut PhotocraftApp, ctx: &egui::Context, lighter: bool) -> Result<(), String> {
    let brightnesses = AppTheme::brightnesses();
    let at = brightnesses.iter().position(|k| *k == app.ui.theme).unwrap_or(if app.ui.theme.is_dark() { 1 } else { 2 });
    let next = if lighter { (at + 1).min(brightnesses.len() - 1) } else { at.saturating_sub(1) };
    let Some(theme) = brightnesses.get(next) else { return Ok(()) };
    if *theme == app.ui.theme {
        return Ok(());
    }
    app.save_theme(theme)?;
    sync_appearance(app, ctx);
    Ok(())
}

/// The favorite themes of each row, light then dark, in the rows' order.
pub(super) fn favorite_theme_groups(favorites: &[String]) -> (Vec<ThemeId>, Vec<ThemeId>) {
    let group = |row: [AppTheme; 7]| row.iter().filter(|theme| favorites.iter().any(|f| f == theme.id().as_str())).map(|theme| theme.id().clone()).collect();
    (group(AppTheme::light_row()), group(AppTheme::dark_row()))
}

/// Right-click menu on the top-right appearance button. A plain click still cycles modes.
pub(crate) fn appearance_context_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if ui.button(tl!("Appearance Settings…")).clicked() {
        open_preferences(app, "interface");
        ui.close();
    }
    let (light, dark) = favorite_theme_groups(&app.session.prefs().interface.favorite_themes);
    let has_light = !light.is_empty();
    if !light.is_empty() || !dark.is_empty() {
        ui.separator();
    }
    for id in light {
        favorite_item(app, ui, &id, false);
    }
    if has_light && !dark.is_empty() {
        ui.separator();
    }
    for id in dark {
        favorite_item(app, ui, &id, true);
    }
}

/// One favorite theme in the appearance button's menu.
fn favorite_item(app: &mut PhotocraftApp, ui: &mut egui::Ui, id: &ThemeId, dark: bool) {
    let shown = app.ui.theme.id() == id && app.ui.theme.is_dark() == dark;
    let name = app.framework.theme_name(id, dark);
    if ui.selectable_label(shown, tl!(&name)).clicked() {
        let theme = app.framework.theme_for(id, dark, ui.ctx());
        app.set_theme(ui.ctx(), theme);
        ui.close();
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
    ctx.set_zoom_factor(interface_zoom(app.session.prefs().interface.ui_scale, ctx));
}

/// The zoom factor over the native pixels per point that gives `pref`'s scale. The craft runtime
/// sets the same one before each frame (`app/framework.rs`).
pub(crate) fn interface_zoom(pref: prefs::UiScale, ctx: &egui::Context) -> f32 {
    let native = ctx.native_pixels_per_point();
    let native_scale = native.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(1.0);
    display_scale(pref, native) / native_scale
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

/// Shows the theme the saved appearance mode and slots select, without changing them, when it
/// isn't the one on screen. Runs every frame, so it only compares the slot's id and the mode; a
/// slot the suite can't resolve, shown as the mode's default, resolves again only when
/// `Framework::sync_theme` reports a change.
pub(crate) fn sync_appearance(app: &mut PhotocraftApp, ctx: &egui::Context) {
    // A restored or embedding-supplied pin must not suppress native appearance events.
    ctx.set_theme(egui::ThemePreference::System);
    // The native service also covers Wayland sessions where winit has no system theme.
    let (slot, dark) = selected_slot(&app.session.prefs().interface, system_theme(app, ctx));
    let shown = &app.ui.theme;
    if dark == shown.is_dark() && (slot == shown.id().as_str() || app.framework.shows_fallback_for(slot, shown)) {
        return;
    }
    show_selected_theme(app, ctx);
}

/// Resolves the theme the saved appearance mode and slots select and shows it when it differs
/// from the one on screen: also after the suite moved (`Framework::sync_theme`), when a library
/// theme's definition may have changed under the same id.
pub(crate) fn show_selected_theme(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let (id, dark) = selected_theme(&app.session.prefs().interface, system_theme(app, ctx));
    // A library theme the suite is loading: the current look stays until the suite's read shows it.
    if app.framework.theme_availability(&id) == Err(crate::app::framework::ThemeUnavailable::Loading) {
        return;
    }
    let selected = app.framework.theme_for(&id, dark, ctx);
    if app.ui.theme != selected {
        app.apply_theme(ctx, selected);
    }
}

/// One theme as a Color Theme swatch: its panel colour under a chrome strip with an accent dot,
/// outlined when chosen. The name is the tooltip and the accessible label. A star badge on its
/// corner adds the theme to (or removes it from) the favourites listed by the appearance
/// button's menu; it shows while the swatch is hovered, and stays for a favourite.
fn theme_swatch(ui: &mut egui::Ui, theme: &AppTheme, selected: &mut String, favorites: &mut Vec<String>) {
    const BADGE: f32 = 7.0;
    let t = Tokens::get(ui.ctx());
    let p = theme.tokens();
    let id = theme.id().as_str();
    let label = tl!(AppTheme::palette_label(id, theme.is_dark()).unwrap_or(id)).to_string();
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
    let response = ui.interact(badge, ui.scope_id().with((id, theme.is_dark(), "favorite")), Sense::click()).on_hover_text(action);
    crate::icons::paint(ui, badge, icon, 10.0, tint);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{action}: {label}")));
    if response.clicked() {
        if favorite {
            favorites.retain(|saved| saved != id);
        } else {
            favorites.push(id.to_owned());
        }
    }
}

/// A grid row of theme swatches, marked Active when the appearance mode shows this family.
fn theme_row(ui: &mut egui::Ui, title: &str, active: bool, selected: &mut String, favorites: &mut Vec<String>, row: [AppTheme; 7]) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(tl!(title)).color(t.text_dim));
    ui.horizontal(|ui| {
        // Room for the favourite star badges on the swatch corners.
        ui.spacing_mut().item_spacing.x = 6.0;
        for theme in &row {
            theme_swatch(ui, theme, selected, favorites);
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
}

impl AppearanceRows {
    /// `query` is lowercased; empty shows every row.
    pub(super) fn matching(query: &str) -> Self {
        let shown = |key: &str| query.is_empty() || preference_matches("interface", key, query);
        Self { mode: shown("appearanceMode"), light: shown("lightTheme"), dark: shown("darkTheme") }
    }

    pub(super) fn any(self) -> bool {
        self.mode || self.light || self.dark
    }
}

/// Appearance Mode and the light and dark Color Theme swatches, as rows of the Appearance grid.
pub(super) fn appearance_rows(ui: &mut egui::Ui, obj: &mut Map<String, Value>, system: Option<egui::Theme>, rows: AppearanceRows) {
    let t = Tokens::get(ui.ctx());
    let mut mode = obj.get("appearanceMode").and_then(Value::as_str).unwrap_or("auto").to_string();
    if rows.mode {
        ui.label(RichText::new(tl!("Appearance Mode")).color(t.text_dim));
        let choices: [(String, &str); 3] = [("auto".into(), tl!("Sync with system")), ("dark".into(), tl!("Dark")), ("light".into(), tl!("Light"))];
        let pairs: Vec<(String, &str)> = choices.iter().map(|(id, label)| (id.clone(), *label)).collect();
        crate::widgets::dropdown(ui, "appearance-mode", tl!("Appearance Mode"), &mut mode, &pairs, 220.0);
        ui.end_row();
    }
    let light_active = mode == "light" || (mode == "auto" && system == Some(egui::Theme::Light));
    obj.insert("appearanceMode".into(), json!(mode));
    let defaults = prefs::Interface::default();
    let mut light = obj.get("lightTheme").and_then(Value::as_str).map_or(defaults.light_theme, str::to_owned);
    let mut dark = obj.get("darkTheme").and_then(Value::as_str).map_or(defaults.dark_theme, str::to_owned);
    let mut favorites: Vec<String> = obj.get("favoriteThemes").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    if rows.light {
        theme_row(ui, "Light Theme", light_active, &mut light, &mut favorites, AppTheme::light_row());
    }
    if rows.dark {
        theme_row(ui, "Dark Theme", !light_active, &mut dark, &mut favorites, AppTheme::dark_row());
    }
    obj.insert("lightTheme".into(), json!(light));
    obj.insert("darkTheme".into(), json!(dark));
    obj.insert("favoriteThemes".into(), json!(favorites));
}
