//! Linux: the custom title bar's caption buttons. craft-window's policy picks the family and
//! reads the button layout at startup (GNOME's `button-layout`, KWin's order, GTK's
//! `gtk-decoration-layout`); [`LayoutWatch`] follows it while the app runs: GNOME's through the
//! appearance watcher on the settings portal, the others' files again when the window regains
//! focus. The round buttons' glyphs come from the desktop's icon theme, which GNOME's portal
//! reports and announces as it changes.
//!
//! `PHOTOCRAFT_CAPTION_STYLE` (`windows`, `adwaita`, `breeze`, `generic`),
//! `PHOTOCRAFT_CAPTION_LAYOUT` (a GTK layout such as `close,minimize:`) and
//! `PHOTOCRAFT_CAPTION_ICONS` (an icon theme, or `none` for PhotoCraft's own glyphs) override what
//! is found.

#[cfg(target_os = "linux")]
use craft_window::Layout;
#[cfg(target_os = "linux")]
use craft_window::policy::{CaptionFamily, DesktopKind, Frame, LayoutWatch};
#[cfg(target_os = "linux")]
use photocraft_ui_egui::{CaptionIcon, CaptionIcons, CaptionStyle, Captions};

/// GNOME's `button-layout` as the appearance watcher last heard it (`None` until it answers).
pub type GnomeLayoutFn = Box<dyn Fn() -> Option<String> + Send + Sync>;

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

/// [`load_icons`] for the last theme asked for, kept, so a layout change doesn't reread it.
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

#[cfg(target_os = "linux")]
const INTERFACE_NAMESPACE: &str = "org.gnome.desktop.interface";
#[cfg(target_os = "linux")]
const ICONS_KEY: &str = "icon-theme";

/// A string setting from the portal, unwrapping the variants some portals nest it in.
#[cfg(target_os = "linux")]
fn portal_string(value: zbus::zvariant::OwnedValue) -> Option<String> {
    let mut value: zbus::zvariant::Value<'_> = value.into();
    for _ in 0..4 {
        match value {
            zbus::zvariant::Value::Value(inner) => value = *inner,
            zbus::zvariant::Value::Str(s) => return Some(s.to_string()),
            _ => return None,
        }
    }
    None
}

/// The caption style of `decision`'s app-drawn frame, following the layout as it changes and
/// bringing the icon theme's glyphs. A worker reads the glyphs (and on GNOME follows the icon
/// theme) and wakes the UI when they change; the first answer usually lands before the first
/// frame. `None` for any other frame, and off Linux.
pub fn service(decision: &craft_window::policy::Decision, gnome_layout: GnomeLayoutFn) -> Option<photocraft_ui_egui::CaptionStyleFn> {
    #[cfg(target_os = "linux")]
    {
        use std::sync::{Arc, Mutex, OnceLock, PoisonError};
        let Frame::Integrated(captions) = &decision.frame else { return None };
        let var = |k: &str| std::env::var(k).ok();
        let (family, fixed) = overrides(captions.family, &var);
        let gnome = decision.desktop.kind == DesktopKind::Gnome;
        let desktop_icons = decision.desktop.icon_theme.clone();
        let icons: Arc<Mutex<Option<CaptionIcons>>> = Arc::new(Mutex::new(None));
        let wake: Arc<OnceLock<egui::Context>> = Arc::new(OnceLock::new());
        let (shared, waker) = (Arc::clone(&icons), Arc::clone(&wake));
        let cache = IconCache::default();
        let publish = move |portal: Option<&str>| {
            let next = icon_theme_name(portal, desktop_icons.as_deref(), &|k: &str| std::env::var(k).ok()).and_then(|t| cache.get(&t));
            let mut cur = shared.lock().unwrap_or_else(PoisonError::into_inner);
            if *cur != next {
                *cur = next;
                if let Some(ctx) = waker.get() {
                    ctx.request_repaint();
                }
            }
        };
        let (ready, wait) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("caption-icons".into()).spawn(move || {
            let proxy = if gnome { crate::appearance::portal_proxy() } else { None };
            let Some(proxy) = proxy else {
                publish(None);
                let _ = ready.send(());
                return;
            };
            // Subscribe before the first read, so a change during start-up isn't lost.
            let signals = proxy.receive_signal("SettingChanged").ok();
            let value: Option<zbus::zvariant::OwnedValue> = proxy.call("Read", &(INTERFACE_NAMESPACE, ICONS_KEY)).ok();
            let mut theme = value.and_then(portal_string);
            publish(theme.as_deref());
            let _ = ready.send(());
            for message in signals.into_iter().flatten() {
                let Ok((namespace, key, value)) = message.body().deserialize::<(String, String, zbus::zvariant::OwnedValue)>() else { continue };
                if (namespace.as_str(), key.as_str()) == (INTERFACE_NAMESPACE, ICONS_KEY) {
                    theme = portal_string(value);
                    publish(theme.as_deref());
                }
            }
        });
        // A fast desktop answers before the first frame; a slow one updates the bar later.
        if spawned.is_ok() {
            let _ = wait.recv_timeout(std::time::Duration::from_millis(150));
        }
        let layouts = Mutex::new((LayoutWatch::new(decision), None::<bool>));
        Some(Box::new(move |ctx| {
            let _ = wake.set(ctx.clone());
            let mut guard = layouts.lock().unwrap_or_else(PoisonError::into_inner);
            let (watch, was_focused) = &mut *guard;
            watch.set_gnome(gnome_layout().as_deref());
            let focused = ctx.input(|i| i.viewport().focused);
            if focused == Some(true) && *was_focused != Some(true) {
                watch.on_focus();
            }
            *was_focused = focused;
            let layout = fixed.clone().unwrap_or_else(|| watch.layout().clone());
            CaptionStyle { captions: Captions { family, layout }, icons: icons.lock().unwrap_or_else(PoisonError::into_inner).clone() }
        }))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (decision, gnome_layout);
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

    #[test]
    fn portal_strings_unwrap_nested_variants() {
        use zbus::zvariant::{OwnedValue, Value};
        let nested = Value::Value(Box::new(Value::Value(Box::new(Value::from("Papirus")))));
        assert_eq!(portal_string(OwnedValue::try_from(nested).unwrap()).as_deref(), Some("Papirus"));
        assert_eq!(portal_string(OwnedValue::try_from(Value::U32(7)).unwrap()), None);
    }
}
