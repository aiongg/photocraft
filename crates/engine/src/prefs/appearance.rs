//! The app's own appearance choice (the override of the Craft suite's selection) and the legacy
//! theme ids that map onto it.
//!
//! The engine stores the override as strings (`interface.themeLight`, `themeDark`, `size`);
//! reading and writing the suite's `selection.json` belongs to the shell. The 14 legacy theme
//! names (`pro`, `studioLight`, …) stay valid for `prefs.set` and for saved files: each names a
//! library theme and the Size that keeps its geometry.

use serde_json::{Map, Value, json};

use super::{AppSize, AppearanceMode, DarkTheme, Interface, LightTheme, Theme};

/// What a legacy theme name stands for: the library theme, whether it is the dark variant, and
/// the Size that keeps its geometry. `dark_slot` decides which theme the default light theme,
/// `proLight`, belongs to: it is ArtCraft's light variant beside the default dark theme.
pub fn legacy_target(theme: Theme, dark_slot: DarkTheme) -> (&'static str, bool, AppSize) {
    use AppSize::{Comfortable, Compact};
    match theme {
        Theme::Pro => ("craft.pro", true, Compact),
        Theme::ProMedium => ("craft.artcraft", true, Compact),
        Theme::ProLightGray => ("craft.pro-light-gray", false, Compact),
        Theme::ProLight => (if dark_slot == DarkTheme::ProMedium { "craft.artcraft" } else { "craft.pro" }, false, Compact),
        Theme::Studio => ("craft.studio", true, Comfortable),
        Theme::StudioLight => ("craft.studio", false, Comfortable),
        Theme::Classic => ("craft.classic", false, Comfortable),
        Theme::SolarizedDark => ("craft.solarized-dark", true, Comfortable),
        Theme::Adwaita => ("craft.adwaita", false, Comfortable),
        Theme::AdwaitaDark => ("craft.adwaita", true, Comfortable),
        Theme::BreezeLight => ("craft.breeze", false, Comfortable),
        Theme::BreezeDark => ("craft.breeze", true, Comfortable),
        Theme::SoftDark => ("craft.soft", true, Comfortable),
        Theme::SoftLight => ("craft.soft", false, Comfortable),
    }
}

/// The legacy dark theme that shows library theme `id`, if there is one.
pub fn legacy_dark(id: &str) -> Option<DarkTheme> {
    Some(match id {
        "craft.pro" => DarkTheme::Pro,
        "craft.artcraft" => DarkTheme::ProMedium,
        "craft.studio" => DarkTheme::Studio,
        "craft.solarized-dark" => DarkTheme::SolarizedDark,
        "craft.adwaita" => DarkTheme::AdwaitaDark,
        "craft.breeze" => DarkTheme::BreezeDark,
        "craft.soft" => DarkTheme::SoftDark,
        _ => return None,
    })
}

/// The legacy light theme that shows library theme `id`, if there is one.
pub fn legacy_light(id: &str) -> Option<LightTheme> {
    Some(match id {
        "craft.pro" | "craft.artcraft" => LightTheme::ProLight,
        "craft.pro-light-gray" => LightTheme::ProLightGray,
        "craft.studio" => LightTheme::StudioLight,
        "craft.classic" => LightTheme::Classic,
        "craft.adwaita" => LightTheme::Adwaita,
        "craft.breeze" => LightTheme::BreezeLight,
        "craft.soft" => LightTheme::SoftLight,
        _ => return None,
    })
}

/// A library theme id as stored in `interface.themeLight`/`themeDark`/`favoriteThemes`:
/// lowercase letters, digits, `.`, `-` and `_`, at most 64 characters.
pub fn is_theme_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_'))
}

/// A favourite as a library id: a legacy name becomes its theme, an id stays.
fn favorite_id(s: &str, dark_slot: DarkTheme) -> Option<String> {
    match Theme::parse(s) {
        Some(t) => Some(legacy_target(t, dark_slot).0.to_string()),
        None => is_theme_id(s).then(|| s.to_string()),
    }
}

impl Interface {
    /// `prefs.set interface.theme`: the legacy single theme. Sets the slot of its family, the Size
    /// it implies and the appearance mode, as the old setting did.
    pub(super) fn apply_legacy_theme(&mut self, theme: Theme) {
        let (id, dark, size) = legacy_target(theme, self.dark_theme);
        self.theme = theme;
        self.size = size;
        if dark {
            self.dark_theme = DarkTheme::parse(theme.name()).unwrap_or(self.dark_theme);
            self.theme_dark = id.into();
            self.appearance_mode = AppearanceMode::Dark;
        } else {
            self.light_theme = LightTheme::parse(theme.name()).unwrap_or(self.light_theme);
            self.theme_light = id.into();
            self.appearance_mode = AppearanceMode::Light;
        }
    }

    /// Brings the override and its legacy aliases in line after `prev` became `self`: the legacy
    /// ids written through (`force_theme`: a write to `interface.theme` always applies), and the
    /// legacy slots following the library ids.
    pub(super) fn reconcile(&mut self, prev: &Interface, force_theme: bool) {
        if force_theme || self.theme != prev.theme {
            self.apply_legacy_theme(self.theme);
        } else if self.dark_theme != prev.dark_theme {
            let theme = Theme::parse(self.dark_theme.name()).unwrap_or(self.theme);
            self.apply_slot(theme, true);
        } else if self.light_theme != prev.light_theme {
            let theme = Theme::parse(self.light_theme.name()).unwrap_or(self.theme);
            self.apply_slot(theme, false);
        } else {
            if self.theme_dark != prev.theme_dark
                && let Some(d) = legacy_dark(&self.theme_dark)
            {
                self.dark_theme = d;
            }
            if self.theme_light != prev.theme_light
                && let Some(l) = legacy_light(&self.theme_light)
            {
                self.light_theme = l;
            }
        }
        self.normalize_favorites();
    }

    /// A legacy slot write: the slot's theme and the Size it implies; the mode stays.
    fn apply_slot(&mut self, theme: Theme, dark: bool) {
        let (id, _, size) = legacy_target(theme, self.dark_theme);
        if dark {
            self.theme_dark = id.into();
        } else {
            self.theme_light = id.into();
        }
        self.size = size;
    }

    /// Favourites as library ids, without duplicates, in order.
    pub(super) fn normalize_favorites(&mut self) {
        let mut out: Vec<String> = Vec::with_capacity(self.favorite_themes.len());
        for f in &self.favorite_themes {
            if let Some(id) = favorite_id(f, self.dark_theme)
                && !out.contains(&id)
            {
                out.push(id);
            }
        }
        self.favorite_themes = out;
    }
}

/// The one-time import of a saved file from before the override existed (no
/// `appearanceImported`): the legacy theme slots become the override pair and Size, so the user
/// keeps their own look and other Craft apps keep theirs. `appearanceMode` stays the app's mode.
/// A file without any appearance key is a fresh install and follows the suite.
pub(super) fn migrate(interface: &mut Map<String, Value>) {
    if interface.contains_key("appearanceImported") {
        return;
    }
    let had = ["theme", "darkTheme", "lightTheme", "appearanceMode"].iter().any(|k| interface.contains_key(*k));
    interface.insert("appearanceImported".into(), json!(true));
    if !had {
        return;
    }
    let read = |m: &Map<String, Value>, k: &str| m.get(k).and_then(Value::as_str).map(str::to_string);
    let dark = read(interface, "darkTheme").and_then(|s| DarkTheme::parse(&s)).unwrap_or_default();
    let light = read(interface, "lightTheme").and_then(|s| LightTheme::parse(&s)).unwrap_or_default();
    let dark_target = legacy_target(Theme::parse(dark.name()).unwrap_or_default(), dark);
    let light_target = legacy_target(Theme::parse(light.name()).unwrap_or(Theme::ProLight), dark);
    // One Size for the pair: the dark slot's, unless the mode is light.
    let size = if read(interface, "appearanceMode").as_deref() == Some("light") { light_target.2 } else { dark_target.2 };
    interface.entry("themeDark").or_insert_with(|| json!(dark_target.0));
    interface.entry("themeLight").or_insert_with(|| json!(light_target.0));
    interface.entry("size").or_insert_with(|| json!(size.name()));
}
