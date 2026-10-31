//! The Crop tool's Straighten (Photoshop's options-bar level button, #1919): draw a line along
//! something that should be level (a horizon) or plumb (a wall), and the crop frame turns so that
//! line ends up horizontal, or vertical when it is nearer vertical (the library's
//! `CropBox::level_angle`).
//!
//! - The options-bar button arms it for one line, then disarms (one-shot); clicking it again
//!   disarms it without a line.
//! - Holding ⌘ (Ctrl off the Mac) with the Crop tool is a temporary Straighten: a ⌘-drag draws the
//!   line; releasing ⌘ gives the normal crop gestures back.
//!
//! While the line is drawn its angle shows beside the pointer. Straighten sets the frame's angle
//! and fits the frame to the image ([`fitted`]); in Classic Mode the frame then shows turned over
//! the image, in the default mode the image turns behind the upright box (`crop_mode`). It is only a pending frame edit:
//! ↵ commits it and Esc cancels it as any crop. A line shorter than the library's `crop.straighten_min`
//! does nothing.

use craft::draw::ui::CropBox;
use craft::ui::canvas::kurbo::{Rect, Size};

use crate::PhotocraftApp;
use crate::state::Tool;

/// Straighten state (not serialized: it only lasts for a line or while a key is held).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Straighten {
    /// The options-bar button is on: the next drag draws a straighten line.
    pub armed: bool,
    /// ⌘ (Ctrl) is held: set by the canvas every frame.
    pub held: bool,
}

/// ⌘ (Ctrl) is held over the canvas.
pub fn set_held(app: &mut PhotocraftApp, down: bool) {
    app.crop.straighten.held = down;
}

/// A drag with the Crop tool now draws a straighten line (or one is being drawn): the canvas
/// shows the straighten cursor instead of the frame's.
pub fn mode(app: &PhotocraftApp) -> bool {
    let s = &app.crop.straighten;
    app.ui.tool == Tool::Crop
        && match app.crop.drag {
            Some(d) => d.press.kind() == craft::draw::ui::CropPressKind::Straighten,
            None => s.armed || s.held,
        }
}

impl Straighten {
    /// The options-bar button: arms (or disarms) Straighten for one line.
    pub fn toggle(&mut self) {
        self.armed = !self.armed;
    }
}

/// [`Straighten::toggle`] for the app's Crop tool.
pub fn toggle(app: &mut PhotocraftApp) {
    app.crop.straighten.toggle();
}

/// Drops the armed button and any line (Esc, another tool).
pub fn reset(app: &mut PhotocraftApp) {
    app.crop.straighten.armed = false;
}

/// A released line that levels the frame at `deg` degrees (the library's `CropRelease::Straighten`):
/// the frame turns to it, as large as fits the canvas.
pub(crate) fn apply(app: &mut PhotocraftApp, deg: f64) {
    let Some(size) = app.session.active().map(|st| (f64::from(st.doc.size.width), f64::from(st.doc.size.height))) else { return };
    // The frame keeps its proportions (an options-bar ratio is already the frame's); no frame
    // takes the canvas's.
    let (w, h) = app.ui.crop_rect.filter(|r| r.iter().all(|v| v.is_finite())).map_or(size, |r| (r[2] - r[0], r[3] - r[1]));
    // The library's fit: the largest frame of that shape that stays inside the canvas when turned by `deg`.
    let Some(fit) = CropBox::upright(Rect::new(0.0, 0.0, w, h)).fitted(Size::new(size.0, size.1), deg) else { return };
    let r = [fit.rect.x0, fit.rect.y0, fit.rect.x1, fit.rect.y1];
    if app.ui.crop_rect == Some(r) && crate::crop_ui::angle(app) == deg {
        return;
    }
    app.ui.crop_rect = Some(r);
    app.ui.crop_angle = deg;
    app.crop.default_frame = false;
    app.crop.editing = true;
}

/// The options-bar Straighten button (Photoshop's level icon), lit while armed.
pub fn options_button(s: &mut Straighten, ui: &mut egui::Ui) {
    if crate::icons::button(ui, "ruler", 24.0, s.armed, tl!("Straighten the image by drawing a line on it")).clicked() {
        s.toggle();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{ToolEvent, tool_event};
    use crate::crop_ui::{ensure_frame, test_drag};
    use craft::draw::ui::CropPressKind;
    use egui::Modifiers;
    use photocraft_doc::{Color, ColorMode, Document, SampleType, Size};

    const NONE: Modifiers = Modifiers::NONE;
    const CMD: Modifiers = Modifiers::COMMAND;

    fn app(depth: SampleType) -> PhotocraftApp {
        let doc = Document::with_background("straighten", Size::new(200, 100), ColorMode::Rgb, depth, Color::WHITE);
        let mut s = photocraft_engine::Session::new();
        s.add_document(doc, None);
        let mut app = PhotocraftApp::new(s, crate::Services::default());
        app.ui.tool = Tool::Crop;
        // Classic Mode: the frame turns over the image (the default mode's Straighten is tested in
        // `crop_mode`).
        app.ui.tool_options.crop_shield.classic_mode = true;
        app.ui.extras.snap = false;
        app.ui.view.show.smart_guides = false;
        ensure_frame(&mut app);
        app
    }

    fn drag(app: &mut PhotocraftApp, pts: &[[f64; 2]], mods: Modifiers) {
        let (first, last) = (pts[0], pts[pts.len() - 1]);
        tool_event(app, ToolEvent::Down { x: first[0], y: first[1], pressure: 1.0 }, mods);
        for p in &pts[1..] {
            tool_event(app, ToolEvent::Move { x: p[0], y: p[1], pressure: 1.0 }, mods);
        }
        tool_event(app, ToolEvent::Up { x: last[0], y: last[1] }, mods);
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    /// Every corner of frame `r` turned `deg` lies inside the `w`×`h` canvas.
    fn inside(r: [f64; 4], deg: f64, w: f64, h: f64) -> bool {
        crate::crop_ui::corners(r, deg).iter().all(|p| p[0] >= -1e-6 && p[1] >= -1e-6 && p[0] <= w + 1e-6 && p[1] <= h + 1e-6)
    }

    /// A straighten is one box Undo step (#3145): Undo turns the frame back instead of undoing the
    /// document under the pending crop, and Redo turns it again.
    #[test]
    fn straighten_is_a_box_undo_step() {
        let mut app = app(SampleType::U8);
        let before = (app.ui.crop_rect, app.ui.crop_angle);
        drag(&mut app, &[[0.0, 0.0], [100.0, 17.6]], CMD);
        let turned = (app.ui.crop_rect, app.ui.crop_angle);
        assert!(turned.1.abs() > 5.0, "{turned:?}");
        assert!(crate::crop_ui::intercept(&mut app, "edit.undo").is_some(), "Undo steps the box");
        assert_eq!((app.ui.crop_rect, app.ui.crop_angle), before);
        assert!(crate::crop_ui::intercept(&mut app, "edit.redo").is_some());
        assert_eq!((app.ui.crop_rect, app.ui.crop_angle), turned);
    }

    #[test]
    fn the_angle_readout_never_shows_minus_zero_or_minus_180() {
        use crate::canvas::angle_readout;
        assert_eq!(angle_readout(-0.03), "0.0°");
        assert_eq!(angle_readout(0.0), "0.0°");
        assert_eq!(angle_readout(-179.97), "180.0°");
        assert_eq!(angle_readout(179.97), "180.0°");
        assert_eq!(angle_readout(12.34), "12.3°");
        assert_eq!(angle_readout(-12.36), "-12.4°");
    }

    #[test]
    fn the_button_straightens_one_line_then_disarms() {
        for depth in [SampleType::U8, SampleType::U16] {
            let mut app = app(depth);
            assert!(app.crop.default_frame && !mode(&app));
            toggle(&mut app);
            assert!(mode(&app), "armed");
            // A line inside the frame straightens instead of drawing a new frame.
            drag(&mut app, &[[20.0, 40.0], [100.0, 50.0], [180.0, 60.0]], NONE);
            let want = (20.0f64 / 160.0).atan().to_degrees();
            assert!(near(app.ui.crop_angle, want), "{depth:?}: {}", app.ui.crop_angle);
            let r = app.ui.crop_rect.unwrap();
            assert!(inside(r, want, 200.0, 100.0) && near((r[2] - r[0]) / (r[3] - r[1]), 2.0), "{r:?}");
            assert!(!app.crop.default_frame && app.crop.editing && crate::crop_ui::pending(&app));
            assert!(!app.crop.straighten.armed, "one-shot");
            // The next drag is a normal crop gesture again: inside the frame it moves it.
            let before = app.ui.crop_rect.unwrap();
            drag(&mut app, &[[100.0, 50.0], [105.0, 50.0]], NONE);
            let after = app.ui.crop_rect.unwrap();
            assert!(near(after[0] - before[0], 5.0) && near(app.ui.crop_angle, want), "{depth:?}");
            // ↵ commits the turn and the crop in one step.
            let steps = app.session.active().unwrap().history.past_len();
            crate::canvas::commit_crop(&mut app);
            let st = app.session.active().unwrap();
            assert_eq!(st.history.past_len(), steps + 1);
            assert_eq!((st.doc.depth, app.ui.crop_angle), (depth, 0.0));
        }
    }

    #[test]
    fn clicking_the_button_again_disarms_it_and_esc_drops_it() {
        let mut app = app(SampleType::U8);
        toggle(&mut app);
        toggle(&mut app);
        assert!(!app.crop.straighten.armed);
        toggle(&mut app);
        crate::crop_ui::cancel(&mut app);
        assert!(!app.crop.straighten.armed && !mode(&app));
        // Leaving the tool drops it too.
        toggle(&mut app);
        app.ui.tool = Tool::Brush;
        ensure_frame(&mut app);
        assert!(!app.crop.straighten.armed);
    }

    #[test]
    fn a_tiny_or_level_line_changes_nothing() {
        let mut app = app(SampleType::U8);
        toggle(&mut app);
        // A click (zero length), then a line under 3 screen points.
        drag(&mut app, &[[50.0, 50.0]], NONE);
        assert_eq!((app.ui.crop_rect, app.ui.crop_angle), (Some([0.0, 0.0, 200.0, 100.0]), 0.0));
        assert!(app.crop.default_frame, "the untouched frame stays the default");
        assert!(!app.crop.straighten.armed, "the click used the button");
        let zoom = f64::from(app.point_zoom());
        toggle(&mut app);
        drag(&mut app, &[[50.0, 50.0], [50.0 + 2.0 / zoom, 50.0 + 1.0 / zoom]], NONE);
        assert_eq!(app.ui.crop_angle, 0.0);
        assert!(app.ui.crop_rect.unwrap().iter().all(|v| v.is_finite()));
        // An already level line on the default frame leaves it as it is.
        toggle(&mut app);
        drag(&mut app, &[[10.0, 50.0], [190.0, 50.0]], NONE);
        assert_eq!((app.ui.crop_rect, app.ui.crop_angle), (Some([0.0, 0.0, 200.0, 100.0]), 0.0));
        // Non-finite pointer positions never reach it.
        toggle(&mut app);
        drag(&mut app, &[[f64::NAN, 50.0], [190.0, f64::INFINITY]], NONE);
        assert!(app.ui.crop_angle.is_finite() && app.ui.crop_rect.unwrap().iter().all(|v| v.is_finite()));
    }

    #[test]
    fn a_line_whose_release_was_lost_never_takes_over_the_next_drag() {
        let mut app = app(SampleType::U8);
        app.ui.crop_rect = Some([20.0, 20.0, 120.0, 70.0]);
        app.crop.default_frame = false;
        // A ⌘-drag whose release went to the temporary Hand (Space pressed mid-line).
        tool_event(&mut app, ToolEvent::Down { x: 0.0, y: 0.0, pressure: 1.0 }, CMD);
        tool_event(&mut app, ToolEvent::Move { x: 100.0, y: 17.6, pressure: 1.0 }, CMD);
        assert!(app.crop.drag.is_some_and(|d| d.press.kind() == CropPressKind::Straighten));
        // The next plain drag inside the frame moves it, and ends.
        drag(&mut app, &[[50.0, 40.0], [60.0, 40.0]], NONE);
        assert_eq!((app.ui.crop_rect, app.ui.crop_angle), (Some([30.0, 20.0, 130.0, 70.0]), 0.0));
        assert!(app.crop.drag.is_none());
    }

    /// A Straighten line's end points snap as point gestures: a line a
    /// few pixels off level on both ends snaps onto the document's centre line, so it is level.
    #[test]
    fn straighten_line_end_points_snap() {
        for snap in [false, true] {
            let mut app = app(SampleType::U8);
            app.ui.extras.snap = snap;
            toggle(&mut app);
            drag(&mut app, &[[10.0, 52.0], [100.0, 50.0], [190.0, 48.0]], NONE);
            if snap {
                assert_eq!(app.ui.crop_angle, 0.0, "both ends snap to y = 50");
            } else {
                assert!(app.ui.crop_angle.abs() > 1.0, "{}", app.ui.crop_angle);
            }
        }
    }

    #[test]
    fn holding_command_is_a_temporary_straighten() {
        let mut app = app(SampleType::U8);
        set_held(&mut app, true);
        assert!(mode(&app));
        assert_eq!(crate::crop_ui::cursor(&app, [100.0, 50.0]), Some(egui::CursorIcon::Crosshair));
        assert_eq!(crate::crop_ui::turn_cursor_dir(&app, [300.0, 50.0]), None, "no turn cursor while straightening");
        // A ⌘-drag outside the frame draws the line instead of turning the frame about the pointer.
        tool_event(&mut app, ToolEvent::Down { x: 250.0, y: 60.0, pressure: 1.0 }, CMD);
        tool_event(&mut app, ToolEvent::Move { x: 150.0, y: 0.0, pressure: 1.0 }, CMD);
        assert!(app.crop.drag.is_some_and(|d| d.press.kind() == CropPressKind::Straighten));
        tool_event(&mut app, ToolEvent::Up { x: 0.0, y: 0.0 }, CMD);
        // From (250, 60) to (0, 0): nearer horizontal, rising to the left.
        let want = (60.0f64 / 250.0).atan().to_degrees();
        assert!(near(app.ui.crop_angle, want), "{}", app.ui.crop_angle);
        // Released: the same drag outside turns the frame again.
        set_held(&mut app, false);
        assert!(!mode(&app));
        drag(&mut app, &[[300.0, 50.0], [300.0, 90.0]], NONE);
        assert!(app.crop.drag.is_none() && app.ui.crop_angle > want, "{}", app.ui.crop_angle);
        // And a ⌘-drag in the middle of a crop gesture doesn't hijack it.
        app.crop.drag = Some(test_drag());
        crate::crop_ui::pointer(&mut app, ToolEvent::Down { x: 100.0, y: 50.0, pressure: 1.0 }, CMD);
        assert!(app.crop.drag.is_some_and(|d| d.press.kind() != CropPressKind::Straighten));
    }
}
