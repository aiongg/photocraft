//! The Crop tool's composition overlay (Photoshop's options-bar overlay menu, #1919): which guide
//! the crop box shows (Rule of Thirds, Grid, Diagonal, Triangle, Golden Ratio, Golden Spiral),
//! when it shows it (Auto: only while the box is dragged; Always; Never), and its orientation.
//! With the Crop tool showing a box, O cycles the overlay and ⇧O its orientation (Triangle and
//! Golden Spiral); otherwise O stays the Dodge/Burn/Sponge key.
//!
//! The guide geometry and the show rule are the library's [`CropGuide`] and [`GuideShow`]; the
//! state lives in `ToolOptions` (`crop_overlay`, `crop_overlay_show`, `crop_overlay_orientation`),
//! which `ui.inspect` reports and `ui.set` drives by the types' serde names.

use craft_ui::canvas::{CropGuide, GuideShow, Polyline};
use egui::{Key, Modifiers};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::PhotocraftApp;
use crate::state::{Tool, ToolOptions};

/// The `ui.set` / serde name of a guide or show rule.
pub fn wire_id<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

/// The guide or show rule with `ui.set` name `id`.
pub fn from_wire_id<T: DeserializeOwned>(id: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(id.to_owned())).ok()
}

/// The guides the pending crop box shows now: none when the options say not to.
pub fn current_guides(app: &PhotocraftApp, w: f32, h: f32) -> Vec<Polyline> {
    let o = &app.ui.tool_options;
    if app.ui.tool != Tool::Crop || !o.crop_overlay_show.shows(app.crop.drag.is_some()) {
        return Vec::new();
    }
    o.crop_overlay.guides(o.crop_overlay_orientation, w, h)
}

/// O: the next overlay (the orientation starts over).
pub fn cycle(o: &mut ToolOptions) {
    o.crop_overlay = o.crop_overlay.next();
    o.crop_overlay_orientation = 0;
}

/// Picks overlay `k`; another overlay starts at its first orientation.
pub fn set_overlay(o: &mut ToolOptions, k: CropGuide) {
    if o.crop_overlay != k {
        o.crop_overlay_orientation = 0;
    }
    o.crop_overlay = k;
}

/// ⇧O: the overlay's next orientation (nothing for a symmetric one).
pub fn cycle_orientation(o: &mut ToolOptions) {
    o.crop_overlay_orientation = o.crop_overlay.next_orientation(o.crop_overlay_orientation);
}

/// O / ⇧O with the Crop tool showing a box and no drag in progress. Returns true when a key was
/// used; otherwise O stays the Dodge/Burn/Sponge tool key.
pub fn keys(app: &mut PhotocraftApp, ctx: &egui::Context) -> bool {
    if app.ui.tool != Tool::Crop || app.ui.crop_rect.is_none() || app.crop.drag.is_some() {
        return false;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::O)) {
        cycle_orientation(&mut app.ui.tool_options);
        return true;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::O)) {
        cycle(&mut app.ui.tool_options);
        return true;
    }
    false
}

/// The options bar's overlay button and its menu (Photoshop's "Set the overlay options for the
/// Crop tool").
pub fn options_button(o: &mut ToolOptions, ui: &mut egui::Ui) {
    let r =
        crate::icons::button(ui, "grid-3x3", 24.0, false, &format!("{}  ({})", tl!("Set the overlay options for the Crop tool"), tl!(o.crop_overlay.name())));
    egui::Popup::menu(&r).show(|ui| {
        ui.set_min_width(220.0);
        let item = |ui: &mut egui::Ui, on: bool, label: &str, key: Option<&str>| {
            let mut b = egui::Button::new(format!("{} {}", if on { "✔" } else { "  " }, label));
            if let Some(k) = key {
                b = b.shortcut_text(crate::shortcuts::pretty(k));
            }
            ui.add(b).clicked()
        };
        for k in CropGuide::ALL {
            if item(ui, o.crop_overlay == k, tl!(k.name()), None) {
                set_overlay(o, k);
                ui.close();
            }
        }
        ui.separator();
        for s in GuideShow::ALL {
            if item(ui, o.crop_overlay_show == s, tl!(s.name()), None) {
                o.crop_overlay_show = s;
                ui.close();
            }
        }
        ui.separator();
        if item(ui, false, tl!("Cycle Overlay"), Some("O")) {
            cycle(o);
            ui.close();
        }
        let turns = o.crop_overlay.orientations() > 1;
        let b = egui::Button::new(format!("   {}", tl!("Cycle Orientation"))).shortcut_text(crate::shortcuts::pretty("Shift+O"));
        if ui.add_enabled(turns, b).clicked() {
            cycle_orientation(o);
            ui.close();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycling_overlays_and_orientations() {
        let mut o = ToolOptions::default();
        assert_eq!((o.crop_overlay, o.crop_overlay_show, o.crop_overlay_orientation), (CropGuide::Thirds, GuideShow::Auto, 0));
        let mut seen = vec![o.crop_overlay];
        for _ in 0..6 {
            cycle(&mut o);
            seen.push(o.crop_overlay);
        }
        assert_eq!(&seen[..6], &CropGuide::ALL);
        assert_eq!(seen[6], CropGuide::Thirds, "wraps around");
        cycle_orientation(&mut o);
        assert_eq!(o.crop_overlay_orientation, 0, "thirds has one orientation");
        o.crop_overlay = CropGuide::GoldenSpiral;
        let turns: Vec<u8> = (0..5)
            .map(|_| {
                cycle_orientation(&mut o);
                o.crop_overlay_orientation
            })
            .collect();
        assert_eq!(turns, [1, 2, 3, 0, 1]);
        cycle(&mut o);
        assert_eq!((o.crop_overlay, o.crop_overlay_orientation), (CropGuide::Thirds, 0));
        // A stray orientation (an old or hand-edited state) still cycles in range.
        o.crop_overlay = CropGuide::Triangle;
        o.crop_overlay_orientation = 200;
        cycle_orientation(&mut o);
        assert!(o.crop_overlay_orientation < 2);
    }

    #[test]
    fn visibility_modes() {
        for k in CropGuide::ALL {
            assert_eq!(from_wire_id::<CropGuide>(&wire_id(&k)), Some(k));
        }
        for s in GuideShow::ALL {
            assert_eq!(from_wire_id::<GuideShow>(&wire_id(&s)), Some(s));
        }
        assert_eq!(wire_id(&CropGuide::GoldenSpiral), "goldenSpiral");
        assert_eq!(from_wire_id::<CropGuide>("spiral"), None);
    }

    /// One key press through the shortcut handler, as egui delivers it.
    fn press(app: &mut PhotocraftApp, mods: Modifiers) {
        let ctx = egui::Context::default();
        ctx.input_mut(|i| {
            i.modifiers = mods;
            i.events.push(egui::Event::Key { key: Key::O, physical_key: None, pressed: true, repeat: false, modifiers: mods });
        });
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
    fn o_cycles_the_overlay_while_the_crop_tool_shows_a_box() {
        let mut app = crop_app();
        press(&mut app, Modifiers::NONE);
        assert_eq!((app.ui.tool, app.ui.tool_options.crop_overlay), (Tool::Crop, CropGuide::Grid));
        // ⇧O on a symmetric overlay changes nothing, and doesn't switch tools either.
        press(&mut app, Modifiers::SHIFT);
        assert_eq!((app.ui.tool, app.ui.tool_options.crop_overlay, app.ui.tool_options.crop_overlay_orientation), (Tool::Crop, CropGuide::Grid, 0));
        press(&mut app, Modifiers::NONE);
        press(&mut app, Modifiers::NONE);
        assert_eq!(app.ui.tool_options.crop_overlay, CropGuide::Triangle);
        press(&mut app, Modifiers::SHIFT);
        assert_eq!(app.ui.tool_options.crop_overlay_orientation, 1);
        assert_eq!(app.ui.tool, Tool::Crop);
    }

    #[test]
    fn o_stays_the_dodge_key_without_a_crop_box() {
        // No document: the Crop tool shows no box, so O picks the Dodge tool.
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.ui.tool = Tool::Crop;
        press(&mut app, Modifiers::NONE);
        assert_eq!(app.ui.tool, Tool::Dodge);
        assert_eq!(app.ui.tool_options.crop_overlay, CropGuide::Thirds);
        // Another tool: O is its tool key as before.
        let mut app = crop_app();
        app.ui.tool = Tool::Brush;
        press(&mut app, Modifiers::NONE);
        assert_eq!((app.ui.tool, app.ui.tool_options.crop_overlay), (Tool::Dodge, CropGuide::Thirds));
        // A crop drag in progress leaves the overlay alone.
        let mut app = crop_app();
        app.crop.drag = Some(crate::crop_ui::CropDrag::Move { start: [0.0, 0.0], rect: [0.0, 0.0, 10.0, 10.0] });
        let ctx = egui::Context::default();
        ctx.input_mut(|i| i.events.push(egui::Event::Key { key: Key::O, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }));
        assert!(!keys(&mut app, &ctx));
        assert_eq!(app.ui.tool_options.crop_overlay, CropGuide::Thirds);
    }

    #[test]
    fn the_overlay_shows_by_mode_and_gesture() {
        let mut app = crop_app();
        assert!(current_guides(&app, 300.0, 200.0).is_empty(), "Auto: no guides without a drag");
        app.crop.drag = Some(crate::crop_ui::CropDrag::Move { start: [0.0, 0.0], rect: [0.0, 0.0, 10.0, 10.0] });
        assert_eq!(current_guides(&app, 300.0, 200.0).len(), 4, "Auto: the thirds while dragging");
        app.ui.tool_options.crop_overlay_show = GuideShow::Never;
        assert!(current_guides(&app, 300.0, 200.0).is_empty());
        app.crop.drag = None;
        app.ui.tool_options.crop_overlay_show = GuideShow::Always;
        app.ui.tool_options.crop_overlay = CropGuide::Triangle;
        assert_eq!(current_guides(&app, 300.0, 200.0).len(), 3);
        app.ui.tool = Tool::Brush;
        assert!(current_guides(&app, 300.0, 200.0).is_empty(), "a pending frame under another tool shows no guide");
    }

    #[test]
    fn old_state_files_get_photoshops_defaults() {
        let o: ToolOptions = serde_json::from_str(r#"{"crop_ratio": "1:1"}"#).unwrap();
        assert_eq!((o.crop_overlay, o.crop_overlay_show, o.crop_overlay_orientation), (CropGuide::Thirds, GuideShow::Auto, 0));
        let o: ToolOptions = serde_json::from_str(r#"{"crop_overlay": "goldenSpiral", "crop_overlay_show": "always", "crop_overlay_orientation": 3}"#).unwrap();
        assert_eq!((o.crop_overlay, o.crop_overlay_show, o.crop_overlay_orientation), (CropGuide::GoldenSpiral, GuideShow::Always, 3));
    }
}
