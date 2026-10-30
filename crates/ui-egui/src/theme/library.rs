//! The Craft theme library behind PhotoCraft's [`Tokens`]: the 14 legacy theme kinds as library
//! themes, and the flat colour struct the widgets read, derived from resolved library tokens.

use super::{ThemeKind, Tokens};
use craft_theme::{Appearance, Mode, ModeChoice, Selection, Size, ThemeId, ThemePair};
use craft_ui::theme::ToEgui;
use egui::Color32;

impl ThemeKind {
    /// The library theme this kind is a variant of.
    pub fn craft_id(self) -> &'static str {
        match self {
            ThemeKind::ProMedium => "craft.artcraft",
            ThemeKind::Pro | ThemeKind::ProLight => "craft.pro",
            ThemeKind::ProLightGray => "craft.pro-light-gray",
            ThemeKind::Studio | ThemeKind::StudioLight => "craft.studio",
            ThemeKind::Classic => "craft.classic",
            ThemeKind::SolarizedDark => "craft.solarized-dark",
            ThemeKind::Adwaita | ThemeKind::AdwaitaDark => "craft.adwaita",
            ThemeKind::BreezeLight | ThemeKind::BreezeDark => "craft.breeze",
            ThemeKind::SoftDark | ThemeKind::SoftLight => "craft.soft",
        }
    }

    /// The Size that keeps this kind's geometry: the four Pro kinds are Compact.
    pub fn size(self) -> Size {
        if self.is_pro() { Size::Compact } else { Size::Comfortable }
    }

    /// The legacy kind a library theme shown in `dark` or light mode corresponds to. Themes
    /// without a legacy kind (Graphite, user themes) take the Pro layout at Compact size and the
    /// Studio layout otherwise.
    pub fn of_library(id: &str, dark: bool, size: Size) -> ThemeKind {
        use ThemeKind::*;
        let pick = |d, l| if dark { d } else { l };
        match id {
            "craft.artcraft" => pick(ProMedium, ProLight),
            "craft.pro" => pick(Pro, ProLight),
            "craft.pro-light-gray" => ProLightGray,
            "craft.studio" => pick(Studio, StudioLight),
            "craft.classic" => Classic,
            "craft.solarized-dark" => SolarizedDark,
            "craft.adwaita" => pick(AdwaitaDark, Adwaita),
            "craft.breeze" => pick(BreezeDark, BreezeLight),
            "craft.soft" => pick(SoftDark, SoftLight),
            _ if size == Size::Compact => pick(Pro, ProLight),
            _ => pick(Studio, StudioLight),
        }
    }

    /// Whether the legacy kind is a dark appearance (its library variant is the dark one).
    pub fn mode(self) -> Mode {
        if self.is_dark() { Mode::Dark } else { Mode::Light }
    }
}

/// The selection that shows `kind` and nothing else of the user's: both slots name its theme.
pub fn selection_for(kind: ThemeKind) -> Selection {
    let id = ThemeId::parse(kind.craft_id()).unwrap_or_else(|_| ThemeId::suite_default());
    Selection {
        theme: ThemePair::same(id),
        mode: if kind.is_dark() { ModeChoice::Dark } else { ModeChoice::Light },
        contrast: craft_theme::ContrastChoice::Standard,
        motion: craft_theme::MotionChoice::Full,
        size: kind.size(),
        text_size: craft_theme::TextSize::Default,
    }
}

impl Tokens {
    /// Derives the flat tokens for `kind` from library tokens resolved for `mode`.
    pub fn from_library(kind: ThemeKind, t: &craft_theme::Tokens, appearance: Appearance) -> Tokens {
        let c = &t.color;
        let dark = appearance.mode == Mode::Dark;
        let col = |v: craft_theme::Rgba| -> Color32 { v.to_egui() };
        let pro = kind.is_pro();
        let primary = &t.component.button.primary;
        let canvas = col(c.surface.canvas);
        // The library's canvas pattern colour: equal to the canvas unless the theme sets a dot pattern.
        let canvas_dot = col(c.surface.canvas_pattern);
        let r = &t.shape.radius;
        Tokens {
            kind,
            chrome: col(c.surface.chrome),
            canvas,
            canvas_dot,
            dock: col(c.surface.background),
            card: col(c.surface.panel),
            card_border: col(c.line.border),
            field: col(c.surface.sunken),
            field_border: col(c.line.control_edge),
            hover: col(c.state.hover),
            pressed: col(c.state.pressed),
            text: col(c.text.normal),
            text_dim: col(c.text.muted),
            text_faint: col(c.text.faint),
            icon: col(c.text.normal),
            accent: col(c.accent.solid),
            accent_soft: col(c.accent.soft),
            accent_border: col(c.accent.border),
            separator: col(c.line.divider),
            shadow: col(c.surface.shadow),
            primary_bg: col(primary.fill.rest),
            primary_text: col(primary.text.rest),
            danger: col(c.status.bad.fg),
            warning: col(c.status.warn.fg),
            radius_sm: r.control,
            radius: r.popup,
            radius_lg: r.window,
            bevel: matches!(t.shape.edge, craft_theme::Edge::Bevel { .. }),
            pro,
            // The Studio-layout themes paint neither (they use `hover` and `accent_soft`).
            tab_strip: if pro || kind == ThemeKind::Classic { col(t.component.tab.strip) } else { Color32::TRANSPARENT },
            row_selected: if pro || kind == ThemeKind::Classic { col(c.state.selected) } else { Color32::TRANSPARENT },
            histogram_bg: col(c.data.scope),
            histogram_level: if dark || pro { 225 } else { 240 },
            caption_close: col(c.caption.close_hover),
            caption_close_text: col(c.caption.close_text),
            scrim: col(c.surface.scrim),
        }
    }

    /// The tokens of a built-in legacy kind at its own Size, resolved without a theme store.
    pub fn for_kind(kind: ThemeKind) -> Tokens {
        let sel = selection_for(kind);
        let defaults = craft_theme::AppDefaults { theme: sel.theme.clone(), mode: sel.mode, size: sel.size };
        let base =
            craft_ui::theme::Base::resolve(&sel, Default::default(), &defaults, None, &mut |id| craft_theme::builtins().iter().find(|t| t.id == *id).cloned());
        let shown = base.shown(kind.mode());
        Tokens::from_library(kind, &shown.resolved.tokens, shown.appearance)
    }
}
