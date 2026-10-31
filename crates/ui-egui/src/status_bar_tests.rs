//! The status bar's zoom field and Fit button sit inside the bar with an even margin above and
//! below, in every theme (#2823).

use egui::{Rect, pos2, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;

use crate::PhotocraftApp;
use crate::theme::{ThemeKind, Tokens};

const SIZE: egui::Vec2 = vec2(900.0, 200.0);

fn bottom_id() -> egui::Id {
    egui::Id::unique("status-bar-test-bottom")
}

fn harness(kind: ThemeKind) -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 1080, "height": 1080, "resolution": 72, "background": "white"})).unwrap();
    app.sync_views();
    let mut h = Harness::builder().with_size(SIZE).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            // The first frame runs before `setup_context` installs the theme's fonts.
            if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                // Where the bar goes: the bottom of the harness's (margined) root Ui.
                let bottom = ui.max_rect().bottom();
                ui.ctx().data_mut(|d| d.insert_temp(bottom_id(), bottom));
                // The shell's status slot.
                let height = craft::ui::theme::current(ui.ctx()).size.bar.status;
                egui::Panel::bottom("status")
                    .exact_size(height)
                    .frame(egui::Frame::NONE)
                    .show_separator_line(false)
                    .show(ui, |ui| crate::panels::status_bar(app, ui));
            }
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, kind);
    h.run_steps(3);
    h
}

#[test]
fn status_bar_controls_sit_inside_the_bar_with_even_margins() {
    for kind in ThemeKind::ALL {
        let h = harness(kind);
        let t = Tokens::for_kind(kind);
        let bar_h = craft::ui::theme::current(&h.ctx).size.bar.status;
        assert_eq!(bar_h, if t.pro { 24.0 } else { 30.0 }, "{kind:?}");
        let bottom = h.ctx.data(|d| d.get_temp::<f32>(bottom_id())).unwrap();
        let bar = Rect::from_min_max(pos2(0.0, bottom - bar_h), pos2(SIZE.x, bottom));
        // Half a point of slack for egui's pixel rounding of the centred row.
        let inner = bar.expand(0.5);
        // The zoom number (its box is painted around it and centred).
        let number = h.get_by_role(egui::accesskit::Role::SpinButton).rect();
        assert!(inner.contains_rect(number), "{kind:?}: bar {bar:?}, number {number:?}");
        assert!((number.center().y - bar.center().y).abs() < 0.51, "{kind:?}: bar {bar:?}, number {number:?}");
        // Pro (Photoshop) has no Fit button in the status bar.
        if t.pro {
            assert!(h.query_by_label("Fit").is_none(), "{kind:?}");
            continue;
        }
        let fit = h.get_by_label("Fit").rect();
        assert!(inner.contains_rect(fit), "{kind:?}: bar {bar:?}, Fit {fit:?}");
        assert!((fit.center().y - bar.center().y).abs() < 0.51, "{kind:?}: bar {bar:?}, Fit {fit:?}");
    }
}

/// The zoom field takes typed values and arrow steps. (Its scrub wrap at the screen edge in full
/// screen is the library's `NumberField::scrub_wrap`, tested in craft-libs.)
#[test]
fn status_zoom_takes_typing_and_arrow_steps() {
    let mut h = harness(ThemeKind::ProMedium);
    h.get_by_role(egui::accesskit::Role::SpinButton).click();
    h.run_steps(2);
    h.event(egui::Event::Text("250%".into()));
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().ui.views[0].zoom, 2.5);
    h.get_by_role(egui::accesskit::Role::SpinButton).click();
    h.run_steps(2);
    h.key_press(egui::Key::ArrowUp);
    h.run_steps(2);
    assert!((h.state().ui.views[0].zoom - 2.51).abs() < 0.0001, "arrow keys step by one percent");
}

/// #5016: toggling appearance between dark and light with default settings preserves the Pro
/// layout and does not make a Fit button appear.
#[test]
fn appearance_toggle_preserves_pro_layout_and_has_no_fit_button() {
    let mut h = harness(ThemeKind::ProMedium);
    assert!(h.query_by_label("Fit").is_none());
    assert!(Tokens::get(&h.ctx).pro);

    // Switch to Light mode (top-bar appearance button).
    let ctx = h.ctx.clone();
    crate::prefs_ui::cycle_appearance(h.state_mut(), &ctx);
    if h.state().session.prefs().interface.appearance_mode == photocraft_engine::prefs::AppearanceMode::Auto {
        crate::prefs_ui::cycle_appearance(h.state_mut(), &ctx);
    }
    crate::prefs_ui::tick(h.state_mut(), &ctx);
    h.run_steps(3);
    assert_eq!(h.state().session.prefs().interface.appearance_mode, photocraft_engine::prefs::AppearanceMode::Light);
    assert_eq!(h.state().ui.theme, ThemeKind::ProLight);
    assert!(Tokens::get(&h.ctx).pro);
    assert!(h.query_by_label("Fit").is_none(), "Fit button must not appear in light mode (#5016)");
}
