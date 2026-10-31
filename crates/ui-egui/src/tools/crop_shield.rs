//! The Crop tool's shield options (Photoshop's options-bar gear menu, "Set additional Crop
//! options", #1919): whether the area outside the crop box shows at all (Show Cropped Area, H),
//! and the shield dimming it (Enable Crop Shield: Match Canvas or a custom colour, an opacity, and
//! Auto Adjust Opacity, which lightens it while the box is dragged). Photoshop's defaults: all on,
//! Match Canvas, 75 %. The menu also holds the crop mode: Use Classic Mode (P; off by default) and
//! Auto Center Preview (on by default, default mode only), see `crop_mode`.
//!
//! The state lives in `ToolOptions::crop_shield` ([`CropShield`]), which `ui.inspect` reports and
//! `ui.set {cropShield: {...}}` patches. The library's [`CropShield::fill`] resolves it to the
//! colour the canvas paints over the area outside the box.

use craft::ui::theme::ToEgui;
use egui::{Color32, Key, Modifiers};

use crate::PhotocraftApp;
use crate::state::Tool;

pub use craft::draw::ui::{CropShield, ShieldColor};

/// The pasteboard colour the shield matches: Preferences › Interface's, else black in Full Screen
/// Mode (the backdrop there) and the theme's canvas otherwise.
pub fn canvas_color(app: &PhotocraftApp, ctx: &egui::Context) -> Color32 {
    crate::prefs_ui::pasteboard_color(app)
        .unwrap_or_else(|| if app.ui.view.hides_chrome() { Color32::BLACK } else { craft::ui::theme::current(ctx).color.surface.canvas.to_egui() })
}

/// The shield over the pending crop box now ([`CropShield::fill`]).
pub fn current(app: &PhotocraftApp, ctx: &egui::Context) -> Option<Color32> {
    app.ui.tool_options.crop_shield.fill(canvas_color(app, ctx), app.crop.drag.is_some())
}

/// Show Cropped Area is off with the Crop tool showing a box: nothing outside it shows.
pub fn hides_outside(app: &PhotocraftApp) -> bool {
    app.ui.tool == Tool::Crop && app.ui.crop_rect.is_some() && !app.ui.tool_options.crop_shield.shows_cropped_area()
}

/// H with the Crop tool showing a box and no drag in progress toggles Show Cropped Area (in
/// Classic Mode, where the option is greyed out, H is taken and does nothing), and P Use Classic
/// Mode. Returns true when a key was used; otherwise H stays the Hand tool key and P the Pen's.
pub fn keys(app: &mut PhotocraftApp, ctx: &egui::Context) -> bool {
    if app.ui.tool != Tool::Crop || app.ui.crop_rect.is_none() || app.crop.drag.is_some() {
        return false;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::H)) {
        let s = &mut app.ui.tool_options.crop_shield;
        if !s.classic_mode {
            s.show_cropped_area = !s.show_cropped_area;
        }
        return true;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::P)) {
        let s = &mut app.ui.tool_options.crop_shield;
        s.classic_mode = !s.classic_mode;
        // The view follows at once, about its own centre: default mode turns it with the frame,
        // Classic Mode puts the camera upright (never back to a Rotate View turn).
        crate::crop_mode::sync(app, None);
        return true;
    }
    false
}

/// The options bar's gear button and its menu. Returns true when Custom… asks for the Color
/// Picker ([`pick_custom_color`], which needs the app).
pub fn options_button(s: &mut CropShield, ui: &mut egui::Ui) -> bool {
    let mut pick = false;
    let r = crate::icons::button(ui, "settings", 24.0, false, tl!("Set additional Crop options"));
    egui::Popup::menu(&r).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_min_width(240.0);
        ui.set_max_width(300.0);
        // Photoshop 25.1's order; Classic Mode greys out the other two (Show Cropped Area then
        // shows as on, which is how it acts there).
        ui.horizontal(|ui| {
            crate::widgets::checkbox(ui, &mut s.classic_mode, tl!("Use Classic Mode"));
            ui.weak(crate::shortcuts::pretty("P"));
        });
        let classic = s.classic_mode;
        ui.add_enabled_ui(!classic, |ui| {
            ui.horizontal(|ui| {
                if classic {
                    crate::widgets::checkbox(ui, &mut true, tl!("Show Cropped Area"));
                } else {
                    crate::widgets::checkbox(ui, &mut s.show_cropped_area, tl!("Show Cropped Area"));
                }
                ui.weak(crate::shortcuts::pretty("H"));
            });
            crate::widgets::checkbox(ui, &mut s.auto_center_preview, tl!("Auto Center Preview"));
        });
        ui.separator();
        crate::widgets::checkbox(ui, &mut s.enabled, tl!("Enable Crop Shield"));
        ui.add_enabled_ui(s.enabled, |ui| {
            ui.indent("crop-shield", |ui| {
                ui.horizontal(|ui| {
                    ui.label(tl!("Color:"));
                    ui.radio_value(&mut s.color, ShieldColor::MatchCanvas, tl!("Match Canvas"));
                    if ui.radio(s.color == ShieldColor::Custom, tl!("Custom…")).clicked() {
                        s.color = ShieldColor::Custom;
                        pick = true;
                    }
                    if s.color == ShieldColor::Custom {
                        let [r, g, b] = s.custom_color;
                        if crate::widgets::color_swatch_button(ui, Color32::from_rgb(r, g, b), tl!("Crop Shield color")).clicked() {
                            pick = true;
                        }
                    }
                });
                if !s.opacity.is_finite() {
                    s.opacity = CropShield::default().opacity;
                }
                crate::widgets::slider_row(ui, tl!("Opacity:"), &mut s.opacity, 0.0..=100.0, "%", None);
                crate::widgets::checkbox(ui, &mut s.auto_adjust, tl!("Auto Adjust Opacity"));
            });
        });
    });
    pick
}

/// Custom…: PhotoCraft's Color Picker on the custom shield colour; OK sets it
/// (`color_picker_ui::confirm`, target `cropShield`).
pub fn pick_custom_color(app: &mut PhotocraftApp) -> u64 {
    let rgb = app.ui.tool_options.crop_shield.custom_color.map(|v| v as f32 / 255.0);
    crate::color_picker_ui::open_for_target(app, "cropShield", "Color Picker (Crop Shield Color)", rgb)
}

/// The Color Picker's OK for the custom shield colour (`"#rrggbb"`).
pub fn set_custom_color(app: &mut PhotocraftApp, hex: &str) -> Result<(), String> {
    let rgb = crate::color_picker_ui::parse_hex(hex).ok_or_else(|| format!("not a colour: {hex}"))?;
    let s = &mut app.ui.tool_options.crop_shield;
    s.custom_color = rgb.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    s.color = ShieldColor::Custom;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_settings_without_the_shield_get_the_defaults() {
        let o: crate::state::ToolOptions = serde_json::from_str(r#"{"crop_delete": false}"#).unwrap();
        assert_eq!(o.crop_shield, CropShield::default());
        assert!(!o.crop_delete);
        let s: CropShield = serde_json::from_str(r#"{"enabled": false}"#).unwrap();
        assert_eq!(s, CropShield { enabled: false, ..Default::default() });
        let back: CropShield = serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        assert_eq!(serde_json::to_value(ShieldColor::MatchCanvas).unwrap(), "matchCanvas");
    }

    fn press_h(app: &mut PhotocraftApp) {
        let ctx = egui::Context::default();
        ctx.input_mut(|i| i.events.push(egui::Event::Key { key: Key::H, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }));
        crate::shortcuts::handle(app, &ctx);
    }

    fn crop_app() -> PhotocraftApp {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", serde_json::json!({"width": 200, "height": 120})).unwrap();
        app.ui.tool = Tool::Crop;
        crate::crop_ui::ensure_frame(&mut app);
        assert!(app.ui.crop_rect.is_some());
        app
    }

    #[test]
    fn match_canvas_is_the_black_full_screen_pasteboard() {
        let mut app = crop_app();
        let ctx = egui::Context::default();
        let theme_canvas = craft::ui::theme::current(&ctx).color.surface.canvas.to_egui();
        assert_ne!(theme_canvas, Color32::BLACK);
        app.ui.tool_options.crop_shield.show_cropped_area = false;
        assert!(hides_outside(&app));
        // Full Screen Mode shows the document on black (lib.rs backdrop); the opaque shield
        // over the cropped area matches it.
        for (mode, want) in [
            ("view.screenMode.fullScreen", Color32::BLACK),
            ("view.screenMode.fullScreenWithMenuBar", theme_canvas),
            ("view.screenMode.standard", theme_canvas),
        ] {
            crate::menus::invoke(&mut app, &ctx, mode, serde_json::json!({})).unwrap();
            assert_eq!(current(&app, &ctx), Some(want), "{mode}");
        }
        // A chosen pasteboard colour is painted in every screen mode, and the shield follows it.
        app.run("prefs.set", serde_json::json!({"values": {"interface.canvasColor": "custom", "interface.canvasCustomColor": "#123456"}})).unwrap();
        for mode in ["view.screenMode.fullScreen", "view.screenMode.standard"] {
            crate::menus::invoke(&mut app, &ctx, mode, serde_json::json!({})).unwrap();
            assert_eq!(current(&app, &ctx), Some(Color32::from_rgb(0x12, 0x34, 0x56)), "{mode}");
        }
    }

    #[test]
    fn h_toggles_show_cropped_area_while_the_crop_tool_shows_a_box() {
        let mut app = crop_app();
        assert!(!hides_outside(&app));
        press_h(&mut app);
        assert_eq!(app.ui.tool, Tool::Crop, "H doesn't switch to the Hand tool");
        assert!(!app.ui.tool_options.crop_shield.show_cropped_area);
        assert!(hides_outside(&app));
        press_h(&mut app);
        assert!(app.ui.tool_options.crop_shield.show_cropped_area);
    }

    #[test]
    fn h_stays_the_hand_key_without_a_crop_box() {
        // No document: no box, so H picks the Hand tool.
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.ui.tool = Tool::Crop;
        press_h(&mut app);
        assert_eq!(app.ui.tool, Tool::Hand);
        assert!(app.ui.tool_options.crop_shield.show_cropped_area);
        // Another tool.
        let mut app = crop_app();
        app.ui.tool = Tool::Brush;
        press_h(&mut app);
        assert_eq!(app.ui.tool, Tool::Hand);
        assert!(app.ui.tool_options.crop_shield.show_cropped_area);
        // A crop drag in progress.
        let mut app = crop_app();
        app.crop.drag = Some(crate::crop_ui::test_drag());
        let ctx = egui::Context::default();
        ctx.input_mut(|i| i.events.push(egui::Event::Key { key: Key::H, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }));
        assert!(!keys(&mut app, &ctx));
        assert!(app.ui.tool_options.crop_shield.show_cropped_area);
    }

    #[test]
    fn the_color_picker_sets_the_custom_colour() {
        let mut app = crop_app();
        let id = pick_custom_color(&mut app);
        let f = app.ui.dialog_mut(id).map(|d| d.fields.clone()).unwrap();
        let mut f = f;
        f.insert("color".into(), serde_json::json!("#ff8000"));
        crate::color_picker_ui::confirm(&mut app, &f).unwrap();
        let s = &app.ui.tool_options.crop_shield;
        assert_eq!((s.color, s.custom_color), (ShieldColor::Custom, [255, 128, 0]));
        assert!(set_custom_color(&mut app, "nope").is_err());
        assert_eq!(app.ui.tool_options.crop_shield.custom_color, [255, 128, 0]);
    }
}
