//! The Color panel tab is Photoshop's spectrum field in every theme (#4367, #6000): the light
//! themes used to show HSB sliders only, then a short field with the sliders under it. The field
//! takes the height its dock group gives it, less the readout, in every theme.

use egui_kittest::kittest::Queryable;

use crate::PhotocraftApp;
use crate::dock::Group;
use crate::theme::AppTheme;

/// The height of the Hue Cube's field in the Color tab of `kind`, alone in a 300 pt wide group
/// `height` tall; panics where the tab shows a channel slider.
fn field_height(kind: AppTheme, height: f32) -> f32 {
    let app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    let tab = Group::Color.tabs(kind.tokens().pro).iter().position(|name| *name == "Color").unwrap();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(300.0, height)).build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                crate::panels::dock_body(app, ui, Group::Color, tab);
            }
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, kind);
    h.run_steps(3);
    for slider in ["Hue", "Saturation", "Brightness"] {
        assert!(h.query_all_by_label(slider).next().is_none(), "{kind:?}: no {slider} slider (#6000)");
    }
    h.get_by_label("Saturation and brightness").rect().height()
}

/// Every theme shows the field, never sliders, and stretches it over the group: a taller group
/// gives a taller field, and a short one a short field with the readout still in view.
#[test]
fn every_theme_stretches_the_field_over_the_group() {
    for kind in AppTheme::all() {
        let (short, tall) = (field_height(kind.clone(), 90.0), field_height(kind.clone(), 380.0));
        assert!(short > 0.0 && short < 90.0, "{kind:?}: the field fits a short group ({short})");
        assert!(tall > short + 200.0, "{kind:?}: the field grows with the group ({short} -> {tall})");
    }
}
