use super::*;

fn text_sizes(ctx: &egui::Context) -> Vec<egui::Vec2> {
    let mut sizes = Vec::new();
    // The plugin queues changed fonts at the end of a pass; egui applies them next pass.
    ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
    ctx.run_ui(Default::default(), |ui| {
        // Include custom painter fonts and explicit RichText sizes, not just text styles.
        for font in [FontId::proportional(11.5), medium(12.0), semibold(15.0), mono(12.0), TextStyle::Body.resolve(ui.style())] {
            sizes.push(ui.painter().layout_no_wrap("Interface 123".into(), font, Color32::WHITE).size());
        }
        sizes.push(ui.label(egui::RichText::new("Interface 123").size(13.0)).rect.size());
    })
    .textures_delta
    .clear();
    sizes
}

#[test]
fn ui_font_sizes_scale_shaping_and_row_height_without_accumulating() {
    let ctx = egui::Context::default();
    install_fonts(&ctx);
    apply(&ctx, &AppTheme::pro());
    let original = text_sizes(&ctx);
    let definitions = ctx.fonts(|f| f.definitions().clone());
    for size in [UiFontSize::Tiny, UiFontSize::Small, UiFontSize::Medium, UiFontSize::Large, UiFontSize::Tiny, UiFontSize::Small] {
        set_ui_font_size(&ctx, size);
        let measured = text_sizes(&ctx);
        for (before, after) in original.iter().zip(&measured) {
            let expected = *before * font_scale(size);
            assert!((after.x - expected.x).abs() < 1.0, "{size:?}: width {after:?} vs {expected:?}");
            // egui rounds ascent/descent and the final row height to pixels separately.
            assert!((after.y - expected.y).abs() < 1.1, "{size:?}: height {after:?} vs {expected:?}");
        }
        set_ui_font_size(&ctx, size);
        assert_eq!(text_sizes(&ctx), measured, "setting the same size twice is stable");
    }
    assert_eq!(ctx.fonts(|f| f.definitions().clone()), definitions, "Small restores original font tweaks exactly");
    assert_eq!(ctx.zoom_factor(), 1.0);
}

// Regression test for #4630: the stock thin scroll bar fades its handle to the same opacity
// as its track while the pointer is on it, so the thumb disappeared when it was grabbed.
#[test]
fn scrollbar_thumb_stays_more_visible_than_its_track_in_every_theme() {
    for kind in AppTheme::all() {
        let ctx = egui::Context::default();
        apply(&ctx, &kind);
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            let scroll = ctx.style_of(theme).spacing.scroll;
            if !scroll.floating {
                continue;
            }
            assert!(
                scroll.interact_handle_opacity > scroll.interact_background_opacity,
                "{kind:?}/{theme:?}: handle {} vs track {} under the pointer",
                scroll.interact_handle_opacity,
                scroll.interact_background_opacity
            );
            assert_eq!(scroll.interact_handle_opacity, 1.0, "{kind:?}/{theme:?}: the thumb is opaque under the pointer");
        }
    }
}

/// The desktop app installs fonts and loads the saved UI Font Size in its first frame, while
/// egui's default fonts are still active. Any size but Small then panicked on the next pass
/// ("FontFamily::Name("medium") is not bound to any fonts") because the plugin rescaled a
/// copy of the defaults and that replaced the queued Inter stack.
#[test]
fn saved_ui_font_size_survives_first_frame_install() {
    let ctx = egui::Context::default();
    let medium_bound = |ctx: &egui::Context| ctx.fonts(|f| f.families().contains(&FontFamily::Name("medium".into())));
    ctx.run_ui(Default::default(), |ui| {
        assert!(!medium_bound(ui.ctx()), "egui's defaults are active in the first frame");
        install_fonts(ui.ctx());
        apply(ui.ctx(), &AppTheme::pro());
        set_ui_font_size(ui.ctx(), UiFontSize::Medium);
    })
    .textures_delta
    .clear();
    let mut width = 0.0;
    ctx.run_ui(Default::default(), |ui| {
        assert!(medium_bound(ui.ctx()), "the installed stack reaches the second pass");
        width = ui.painter().layout_no_wrap("PhotoCraft".into(), medium(13.0), Color32::WHITE).size().x;
    })
    .textures_delta
    .clear();
    assert!(width > 0.0);
    // The saved size applies one pass later, from the installed stack.
    ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
    let mut scaled = 0.0;
    ctx.run_ui(Default::default(), |ui| {
        assert!(medium_bound(ui.ctx()));
        scaled = ui.painter().layout_no_wrap("PhotoCraft".into(), medium(13.0), Color32::WHITE).size().x;
    })
    .textures_delta
    .clear();
    assert!((scaled - width * font_scale(UiFontSize::Medium)).abs() < 1.0, "{scaled} vs {width} × 14/12");
    // Re-installing (a language change) keeps the size and the named families.
    ctx.run_ui(Default::default(), |ui| install_fonts(ui.ctx())).textures_delta.clear();
    for _ in 0..2 {
        ctx.run_ui(Default::default(), |ui| {
            assert!(medium_bound(ui.ctx()));
            let w = ui.painter().layout_no_wrap("PhotoCraft".into(), medium(13.0), Color32::WHITE).size().x;
            assert!((w - scaled).abs() < 1.0, "{w} vs {scaled}");
        })
        .textures_delta
        .clear();
    }
}

#[test]
fn every_palette_keeps_complete_egui_styles_when_the_os_changes() {
    for kind in AppTheme::all() {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput { system_theme: Some(egui::Theme::Dark), ..Default::default() }, |ui| apply(ui.ctx(), &kind)).textures_delta.clear();
        let expected = ctx.global_style();
        for appearance in [Some(egui::Theme::Light), Some(egui::Theme::Dark), None] {
            ctx.run_ui(egui::RawInput { system_theme: appearance, ..Default::default() }, |_| {}).textures_delta.clear();
            assert_eq!(Tokens::get(&ctx), kind.tokens());
            assert_eq!(ctx.global_style(), expected, "{kind:?} changed with {appearance:?}");
            assert_eq!(ctx.style_of(egui::Theme::Light), expected);
            assert_eq!(ctx.style_of(egui::Theme::Dark), expected);
        }
    }
}

#[test]
fn applying_a_palette_clears_a_pinned_native_appearance_preference() {
    for pinned in [egui::Theme::Light, egui::Theme::Dark] {
        let ctx = egui::Context::default();
        ctx.set_theme(pinned);
        apply(&ctx, &AppTheme::pro_medium());
        assert_eq!(ctx.options(|o| o.theme_preference), egui::ThemePreference::System);
    }
}

/// WCAG 2 contrast ratio, the measure libadwaita's own palette is held to.
fn contrast(a: Color32, b: Color32) -> f32 {
    let lin = |v: u8| {
        let c = f32::from(v) / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let lum = |c: Color32| 0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b());
    let (hi, lo) = if lum(a) > lum(b) { (lum(a), lum(b)) } else { (lum(b), lum(a)) };
    (hi + 0.05) / (lo + 0.05)
}

/// The light themes outline their white slider knob so its edge holds WCAG's 3:1 non-text
/// contrast against the panels sliders sit on (#7927); the dark ones keep the plain knob.
#[test]
fn light_themes_outline_the_slider_knob() {
    for kind in AppTheme::all() {
        let t = kind.tokens();
        // The Pro layouts keep their dark ring (it rims a grey knob), Classic its bevelled knob.
        if t.pro || t.bevel {
            continue;
        }
        match t.knob_outline() {
            Some(outline) => {
                assert!(!t.dark(), "{kind:?}");
                for panel in [t.dock, t.card, t.chrome] {
                    assert!(contrast(outline, panel) >= 3.0, "{kind:?}: knob outline {outline:?} on {panel:?}");
                }
            }
            None => assert!(t.dark(), "{kind:?}: a light theme without a knob outline"),
        }
    }
    assert_eq!(AppTheme::pro().tokens().knob_outline(), Some(Color32::from_gray(40)), "Pro is unchanged");
    for kind in [AppTheme::studio_light(), AppTheme::adwaita(), AppTheme::breeze_light(), AppTheme::soft_light()] {
        assert!(kind.tokens().knob_outline().is_some(), "{kind:?}");
    }
}

#[test]
fn adwaita_themes_meet_text_contrast() {
    for kind in [AppTheme::adwaita(), AppTheme::adwaita_dark()] {
        let t = kind.tokens();
        assert_eq!(t.dark(), kind == AppTheme::adwaita_dark());
        assert!(!t.pro && !t.bevel, "{kind:?} uses the Studio layout");
        // AA body text (4.5:1) on every surface that carries labels.
        for surface in [t.chrome, t.dock, t.card, t.field] {
            assert!(contrast(t.text, surface) >= 7.0, "{kind:?}: text on {surface:?}");
            assert!(contrast(t.text_dim, surface) >= 4.5, "{kind:?}: dim text on {surface:?}");
        }
        assert!(contrast(t.primary_text, t.primary_bg) >= 3.0, "{kind:?}: suggested-action button label");
    }
}

#[test]
fn soft_themes_hold_text_contrast() {
    for kind in [AppTheme::soft_dark(), AppTheme::soft_light()] {
        assert!(AppTheme::all().contains(&kind));
        let t = kind.tokens();
        assert_eq!(t.dark(), kind == AppTheme::soft_dark());
        assert!(!t.pro && !t.bevel);
        // Soft means no extremes: surfaces stay off black and white.
        for c in [t.chrome, t.canvas, t.dock, t.card, t.field] {
            let (hi, lo) = (c.r().max(c.g()).max(c.b()), c.r().min(c.g()).min(c.b()));
            assert!(if t.dark() { (32..96).contains(&hi) } else { hi < 255 && lo > 200 }, "{kind:?}: {c:?}");
        }
        let tint = |under: Color32| {
            let k = f32::from(t.accent_soft.a()) / 255.0;
            let ch = |over: u8, under: u8| (f32::from(over) * k + f32::from(under) * (1.0 - k)).round() as u8;
            Color32::from_rgb(ch(t.accent.r(), under.r()), ch(t.accent.g(), under.g()), ch(t.accent.b(), under.b()))
        };
        let surfaces = [t.chrome, t.canvas, t.dock, t.card, t.field, t.hover, t.pressed, tint(t.dock), tint(t.card)];
        for (i, surface) in surfaces.into_iter().enumerate() {
            for (name, colour) in [("text", t.text), ("dim", t.text_dim), ("faint", t.text_faint), ("accent text", t.accent_text)] {
                assert!(contrast(colour, surface) >= 4.5, "{kind:?}: {name} on {surface:?}");
            }
            // Status colours are marks on hovered or selected surfaces (3:1), text on resting ones.
            let floor = if i < 5 { 4.5 } else { 3.0 };
            assert!(contrast(t.danger, surface) >= floor && contrast(t.warning, surface) >= floor, "{kind:?}: status on {surface:?}");
        }
        for k in [1.0, 0.93, 0.85] {
            assert!(contrast(t.primary_text, t.primary_bg.gamma_multiply(k)) >= 4.5, "{kind:?}: primary at {k}");
        }
    }
}

#[test]
fn breeze_themes_meet_text_contrast() {
    for kind in [AppTheme::breeze_light(), AppTheme::breeze_dark()] {
        assert!(AppTheme::all().contains(&kind));
        let t = kind.tokens();
        assert_eq!(t.dark(), kind == AppTheme::breeze_dark());
        assert!(!t.pro && !t.bevel, "{kind:?} uses the Studio layout");
        // The design intent, not exact values (the palette is an adaptation and may be tuned):
        // the canvas is the most recessed surface, then the dock and the cards, and controls
        // sit above the cards they are on; the accent is a blue.
        let lum = |c: Color32| u32::from(c.r()) + u32::from(c.g()) + u32::from(c.b());
        assert!(lum(t.canvas) < lum(t.dock) && lum(t.dock) < lum(t.card) && lum(t.card) < lum(t.field), "{kind:?}: surface order");
        assert!(t.accent.b() > t.accent.g() && t.accent.g() > t.accent.r(), "blue accent: {:?}", t.accent);
        // Plasma's surfaces are cool blue-greys: dark ones stay below 64 per channel and light
        // ones above 205, all with a little more blue than red (our own values, close to it).
        for c in [t.chrome, t.canvas, t.dock, t.card, t.field] {
            assert!(if t.dark() { c.r().max(c.g()).max(c.b()) < 64 } else { c.r().min(c.g()).min(c.b()) > 205 }, "{kind:?}: {c:?}");
            assert!(c.b() > c.r(), "{kind:?}: blue-grey {c:?}");
        }
        // All text must read: AA (4.5:1) for every text colour on every surface it can sit on,
        // including a hovered or pressed control and a selected (accent-tinted) row.
        let tint = |under: Color32| {
            let k = f32::from(t.accent_soft.a()) / 255.0;
            let ch = |over: u8, under: u8| (f32::from(over) * k + f32::from(under) * (1.0 - k)).round() as u8;
            Color32::from_rgb(ch(t.accent.r(), under.r()), ch(t.accent.g(), under.g()), ch(t.accent.b(), under.b()))
        };
        let resting = [t.chrome, t.canvas, t.dock, t.card, t.field];
        let active = [t.hover, t.pressed, tint(t.chrome), tint(t.dock), tint(t.card), tint(t.field)];
        for surface in resting.into_iter().chain(active) {
            for (name, colour) in [("text", t.text), ("dim text", t.text_dim), ("faint text", t.text_faint), ("accent text", t.accent_text)] {
                assert!(contrast(colour, surface) >= 4.5, "{kind:?}: {name} on {surface:?}");
            }
        }
        // The text must also keep a hierarchy, not collapse into one grey.
        assert!(contrast(t.text, t.card) > contrast(t.text_dim, t.card) && contrast(t.text_dim, t.card) > contrast(t.text_faint, t.card), "{kind:?}");
        // Danger and warning colours carry text on the resting surfaces; on hovered, pressed and
        // selected ones they are status marks, held to the 3:1 of graphical objects.
        for surface in resting {
            assert!(contrast(t.danger, surface) >= 4.5, "{kind:?}: danger on {surface:?}");
            assert!(contrast(t.warning, surface) >= 4.5, "{kind:?}: warning on {surface:?}");
        }
        for surface in active {
            assert!(contrast(t.danger, surface) >= 3.0, "{kind:?}: danger on {surface:?}");
            assert!(contrast(t.warning, surface) >= 3.0, "{kind:?}: warning on {surface:?}");
        }
        // A primary button darkens while hovered and pressed (button_impl), so check all three.
        for k in [1.0, 0.93, 0.85] {
            assert!(contrast(t.primary_text, t.primary_bg.gamma_multiply(k)) >= 4.5, "{kind:?}: primary button label at {k}");
        }
        assert!(contrast(t.caption_close_text, t.caption_close) >= 4.5, "{kind:?}: close button glyph");
    }
}
