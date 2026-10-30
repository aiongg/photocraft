//! Linux: the custom title bar's caption buttons. craft-window's policy picks the family and
//! reads the button layout at startup (GNOME's `button-layout`, KWin's order, GTK's
//! `gtk-decoration-layout`); [`LayoutWatch`] follows it while the app runs: GNOME's through the
//! appearance watcher on the settings portal, the others' files again when the window regains
//! focus. The round buttons' glyphs come from the desktop's icon theme, which the same watcher
//! reports as it changes.
//!
//! `PHOTOCRAFT_CAPTION_STYLE` (`windows`, `adwaita`, `breeze`, `generic`),
//! `PHOTOCRAFT_CAPTION_LAYOUT` (a GTK layout such as `close,minimize:`) and
//! `PHOTOCRAFT_CAPTION_ICONS` (an icon theme, or `none` for PhotoCraft's own glyphs) override what
//! is found.

use craft_platform::appearance::EguiService;
#[cfg(target_os = "linux")]
use craft_window::Layout;
#[cfg(target_os = "linux")]
use craft_window::policy::{CaptionFamily, DesktopKind, Frame, LayoutWatch};
#[cfg(target_os = "linux")]
use photocraft_ui_egui::{CaptionIcon, CaptionIcons, CaptionStyle, Captions};

/// The family and, when the layout is overridden, the fixed layout.
#[cfg(target_os = "linux")]
fn overrides(found: CaptionFamily, var: &dyn Fn(&str) -> Option<String>) -> (CaptionFamily, Option<Layout>) {
    let family = var("PHOTOCRAFT_CAPTION_STYLE").and_then(|s| CaptionFamily::parse(&s)).unwrap_or(found);
    (family, var("PHOTOCRAFT_CAPTION_LAYOUT").map(|l| Layout::parse(&l)))
}

/// The icon theme the glyphs come from: `PHOTOCRAFT_CAPTION_ICONS` (`none` draws PhotoCraft's own),
/// the portal's answer, the desktop's setting, or Adwaita.
#[cfg(target_os = "linux")]
fn icon_theme_name(portal: Option<&str>, desktop: Option<&str>, var: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    match var("PHOTOCRAFT_CAPTION_ICONS") {
        Some(name) if name.trim().eq_ignore_ascii_case("none") => None,
        Some(name) => Some(name.trim().to_string()),
        None => Some(portal.or(desktop).unwrap_or("Adwaita").to_string()),
    }
}

/// The caption glyphs of icon theme `theme` (`window-*-symbolic.svg`), read now and rendered at
/// whatever size the bar asks for. `None` when the theme (with its parents) has none of them.
#[cfg(target_os = "linux")]
fn load_icons(theme: &str) -> Option<CaptionIcons> {
    use crate::icon_theme::{IconTheme, base_dirs, rasterize_mask, read_small};
    let found = IconTheme::load(theme, &base_dirs(&|k: &str| std::env::var(k).ok()))?;
    let svg = |name: &str| found.find_svg(name).and_then(|p| read_small(&p)).map(std::sync::Arc::<[u8]>::from);
    let files = [svg("window-minimize-symbolic"), svg("window-maximize-symbolic"), svg("window-restore-symbolic"), svg("window-close-symbolic")];
    if files.iter().all(Option::is_none) {
        return None;
    }
    let render = move |icon: CaptionIcon, px: u32| {
        let i = match icon {
            CaptionIcon::Minimize => 0,
            CaptionIcon::Maximize => 1,
            CaptionIcon::Restore => 2,
            CaptionIcon::Close => 3,
        };
        files.get(i)?.as_deref().and_then(|svg| rasterize_mask(svg, px))
    };
    Some(CaptionIcons { theme: theme.to_string(), render: std::sync::Arc::new(render) })
}

/// [`load_icons`] for the last theme asked for, kept, so a repeated publish of the same theme (the portal re-sends its setting) doesn't reread its files.
#[cfg(target_os = "linux")]
#[derive(Default)]
struct IconCache(std::sync::Mutex<Option<(String, Option<CaptionIcons>)>>);

#[cfg(target_os = "linux")]
impl IconCache {
    fn get(&self, theme: &str) -> Option<CaptionIcons> {
        let mut last = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((name, icons)) = last.as_ref()
            && name == theme
        {
            return icons.clone();
        }
        let icons = load_icons(theme);
        *last = Some((theme.to_string(), icons.clone()));
        icons
    }
}

/// The caption style of `decision`'s app-drawn frame, following the layout and the icon theme as
/// the desktop watcher (`appearance`) hears them; it wakes the UI when they change. `None` for any
/// other frame, and off Linux.
pub fn service(decision: &craft_window::policy::Decision, appearance: Option<EguiService>) -> Option<photocraft_ui_egui::CaptionStyleFn> {
    #[cfg(target_os = "linux")]
    {
        use std::sync::{Mutex, PoisonError};
        let Frame::Integrated(captions) = &decision.frame else { return None };
        let var = |k: &str| std::env::var(k).ok();
        let (family, fixed) = overrides(captions.family, &var);
        let gnome = decision.desktop.kind == DesktopKind::Gnome;
        let desktop_icons = decision.desktop.icon_theme.clone();
        let cache = IconCache::default();
        let layouts = Mutex::new((LayoutWatch::new(decision), None::<bool>));
        Some(Box::new(move |ctx| {
            let mut guard = layouts.lock().unwrap_or_else(PoisonError::into_inner);
            let (watch, was_focused) = &mut *guard;
            watch.set_gnome(appearance.as_ref().and_then(EguiService::button_layout).as_deref());
            let focused = ctx.input(|i| i.viewport().focused);
            if focused == Some(true) && *was_focused != Some(true) {
                watch.on_focus();
            }
            *was_focused = focused;
            let layout = fixed.clone().unwrap_or_else(|| watch.layout().clone());
            let portal = appearance.as_ref().filter(|_| gnome).and_then(EguiService::icon_theme);
            let icons = icon_theme_name(portal.as_deref(), desktop_icons.as_deref(), &|k: &str| std::env::var(k).ok()).and_then(|t| cache.get(&t));
            CaptionStyle { captions: Captions { family, layout }, icons }
        }))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (decision, appearance);
        None
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let pairs: Vec<(String, String)> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| pairs.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone())
    }

    #[test]
    fn the_icon_theme_comes_from_the_override_the_portal_or_the_desktop() {
        assert_eq!(icon_theme_name(Some("Papirus"), Some("breeze"), &env(&[])).as_deref(), Some("Papirus"));
        assert_eq!(icon_theme_name(None, Some("breeze"), &env(&[])).as_deref(), Some("breeze"));
        assert_eq!(icon_theme_name(None, None, &env(&[])).as_deref(), Some("Adwaita"));
        assert_eq!(icon_theme_name(Some("Papirus"), None, &env(&[("PHOTOCRAFT_CAPTION_ICONS", " None ")])), None);
        assert_eq!(icon_theme_name(Some("Papirus"), None, &env(&[("PHOTOCRAFT_CAPTION_ICONS", "Tela")])).as_deref(), Some("Tela"));
    }

    #[test]
    fn the_overrides_replace_the_family_and_fix_the_layout() {
        use craft_window::Button::{Close, Maximize, Minimize};
        assert_eq!(overrides(CaptionFamily::Adwaita, &env(&[])), (CaptionFamily::Adwaita, None));
        let forced = env(&[("PHOTOCRAFT_CAPTION_STYLE", "Breeze"), ("PHOTOCRAFT_CAPTION_LAYOUT", "minimize,close:maximize")]);
        let (family, layout) = overrides(CaptionFamily::Adwaita, &forced);
        assert_eq!(family, CaptionFamily::Breeze);
        assert_eq!(layout, Some(Layout { left: vec![Minimize, Close], right: vec![Maximize] }));
        // An unknown name keeps the family the policy found.
        assert_eq!(overrides(CaptionFamily::Breeze, &env(&[("PHOTOCRAFT_CAPTION_STYLE", "aqua")])).0, CaptionFamily::Breeze);
    }
}
