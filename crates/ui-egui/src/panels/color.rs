use super::{PhotocraftApp, Rect, RichText, Sense, Tokens, theme, vec2, widgets};
use craft_theme::{ColorModel, HsbModel, ModelValues, Rgba, Srgba, kept_or_converted};
use craft_ui::composites::{AreaMode, ColorArea, ColorStrip, HexField, PairWell, SwatchPair, Well};
use craft_ui::localized::LocaleNumbers;
use craft_ui::theme::TokensOf as _;
use egui::UiBuilder;

/// The Color panel's foreground and background chips. A click picks the colour the field edits,
/// framed; a double-click opens the Color Picker on it.
pub(crate) fn field_chips(app: &mut PhotocraftApp, ui: &mut egui::Ui, chips: Rect) {
    let numbers = LocaleNumbers::of(ui);
    let well = |c: [f32; 4]| Srgba::rgb(c[0], c[1], c[2]);
    let wells = [
        PairWell::new(tl!("Foreground color"), well(app.session.tools.foreground)),
        PairWell::new(tl!("Background color"), well(app.session.tools.background)),
    ];
    let active = if app.ui.color_panel.background { Well::Secondary } else { Well::Primary };
    let pair = SwatchPair::new("color-panel-chips", tl!("Foreground and background colors"), wells, &numbers).active(active).popover(false);
    let mut child = ui.new_child(UiBuilder::new().max_rect(chips));
    let shown = pair.show(&mut child);
    app.ui.color_panel.background = shown.active == Well::Secondary;
    if let Some(well) = shown.double_clicked {
        crate::color_picker_ui::open(app, if well == Well::Secondary { "background" } else { "foreground" });
    }
}

/// Where the dock tells the Color field how tall its group body is.
pub(crate) fn color_field_fill_id() -> egui::Id {
    egui::Id::unique("color-field-fill-height")
}

/// The Color field stretches to its dock group's height instead of a fixed 120 pt, in every theme
/// (#6000): a dock calls this with the group body's `ui` before drawing the tab.
pub(crate) fn offer_color_field_fill(ui: &egui::Ui, pro: bool, group: crate::dock::Group, tab: usize) {
    if group == crate::dock::Group::Color && group.tabs(pro).get(tab) == Some(&"Color") {
        let inner = ui.available_height().max(0.0);
        ui.data_mut(|d| d.insert_temp(color_field_fill_id(), inner));
    }
}

/// Height of the Color field: the group body's height (when docked) less the hex/RGB readout,
/// which folds onto a second line in a narrow dock. 120 pt when nothing sets a height.
pub(super) fn color_field_height(avail: Option<f32>, width: f32) -> f32 {
    let reserve = 32.0 + if width < 280.0 { 26.0 } else { 0.0 };
    // When docked, shrink the field so the hex/RGB readout stays in view (#6885). A 120 pt floor
    // here pushed the readout below the scroll area in a compact Color group.
    avail.filter(|a| a.is_finite()).map_or(120.0, |a| (a - reserve).clamp(0.0, 2000.0))
}

/// The key of the HSB values last shown for the colour the panel edits.
fn remembered_key(model: &'static str, background: bool) -> egui::Id {
    egui::Id::unique(("color-panel-values", model, background))
}

/// The values of `model` the panel shows for `rgb`: the ones last shown while they still give it,
/// so a grey keeps its hue, black its hue and saturation, and CMYK or Lab values that don't
/// survive an sRGB round trip stay as typed (#3228).
pub(crate) fn kept_values(ui: &egui::Ui, model: &dyn ColorModel, background: bool, rgb: [f32; 3]) -> ModelValues {
    let kept: Option<ModelValues> = ui.data(|d| d.get_temp(remembered_key(model.id(), background)));
    kept_or_converted(model, Srgba::rgb(rgb[0], rgb[1], rgb[2]), kept.as_ref())
}

/// Remember `values`, to show again while they still give the colour.
pub(crate) fn remember_values(ui: &egui::Ui, background: bool, values: ModelValues) {
    ui.data_mut(|d| d.insert_temp(remembered_key(values.model, background), values));
}

/// Photoshop's Color panel Hue Cube (saturation/brightness field + hue strip) or Brightness Cube
/// (hue/saturation field + brightness strip), over a hex/RGB readout.
pub(crate) fn color_field(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let bg_active = app.ui.color_panel.background;
    let current = if bg_active { app.session.tools.background } else { app.session.tools.foreground };
    let values = kept_values(ui, &HsbModel, bg_active, [current[0], current[1], current[2]]);
    let w = ui.available_width();
    let strip_w = 14.0;
    // The strip takes the room of its pointers on both sides.
    let strip_outer = ColorStrip::outer_width(&ColorStrip::style(&craft_ui::theme::Tokens::of(ui)), strip_w);
    // Photoshop's Color panel field fills the panel in every theme (#6000): take the height the
    // dock offers, less the hex/RGB readout (which folds onto a second line in a narrow dock) and
    // the theme's spacing between the rows. Without a dock, 120 pt.
    let gaps = 2.0 * ui.spacing().item_spacing.y;
    let fill = ui.data(|d| d.get_temp::<f32>(color_field_fill_id())).map(|a| a - gaps);
    let h = color_field_height(fill, w);
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (chips, _) = ui.allocate_exact_size(vec2(38.0, h), Sense::hover());
        field_chips(app, ui, chips);
        // Photoshop's ⚠ under the chips when the colour can't print, as in the slider modes
        // (#8672); a compact field too short to hold it below the chips leaves it out.
        if chips.height() >= 38.0 + 15.0 {
            let rgb = crate::color_panel_ui::active(app);
            crate::color_panel_ui::alerts(app, ui, chips, rgb, false);
        }
        // Hue Cube: saturation across and brightness up, hue on the strip. Brightness Cube
        // (the Color Picker's B mode): hue across and saturation up, brightness on the strip.
        let mode = if app.ui.color_panel.mode == crate::state::ColorPanelMode::BrightnessCube { AreaMode::Brightness } else { AreaMode::Hue };
        let field_w = w - 38.0 - strip_outer - 16.0;
        let field = ColorArea::new("color-panel-field", tl!("Color field"), mode, values).size(vec2(field_w, h)).show(ui);
        let strip = ColorStrip::new("color-panel-strip", tl!("Color strip"), mode, values).size(vec2(strip_w, h)).show(ui);
        picked = field.event.as_ref().and_then(|e| e.value()).or(strip.event.as_ref().and_then(|e| e.value())).copied();
    });
    if let Some(v) = picked {
        let rgb = HsbModel.to_srgb(&v.values);
        let c = [rgb.r, rgb.g, rgb.b, 1.0];
        remember_values(ui, bg_active, v);
        if bg_active {
            app.session.tools.background = c;
        } else if app.session.tools.foreground != c {
            // A held, unmoved press repeats the pick: restyle selected type only on a change.
            app.session.tools.foreground = c;
            crate::apply_color::foreground_changed(app, crate::apply_color::gesture(ui.ctx()));
        }
    }
    ui.add_space(craft_ui::theme::scale::Space::S6.px());
    ui.horizontal(|ui| {
        let mut c = if bg_active { app.session.tools.background } else { app.session.tools.foreground };
        if color_readout(ui, ui.scope_id().with(("color-field", bg_active)), &mut c) {
            if bg_active {
                app.session.tools.background = c;
            } else {
                app.session.tools.foreground = c;
                crate::apply_color::foreground_changed(app, crate::apply_color::gesture(ui.ctx()));
            }
        }
    });
    crate::apply_color::track_focus(ui.ctx());
}

/// Tool colours are sRGB-encoded floats; these are their 8-bit channels.
pub(crate) fn srgb_bytes(c: [f32; 4]) -> [u8; 3] {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    [q(c[0]), q(c[1]), q(c[2])]
}

/// The Color panel's editable colour readout: a hex field and R, G, B fields, all drawn like the
/// dock's [`widgets::value_field`] so they sit in the theme. Editing any of them sets `color`
/// (sRGB-encoded floats) and returns `true`.
pub(crate) fn color_readout(ui: &mut egui::Ui, id: egui::Id, color: &mut [f32; 4]) -> bool {
    let t = Tokens::get(ui.ctx());
    let mut changed = false;
    // Wrapped so the fields fold onto a second line in a narrow dock instead of being clipped.
    ui.horizontal_wrapped(|ui| {
        // Docks zero the item spacing; keep the fields apart.
        ui.spacing_mut().item_spacing.x = 4.0;
        let [r, g, b] = srgb_bytes(*color);
        let hex = HexField::new(id.with("hex"), tl!("Color"), Rgba::rgb(r, g, b)).show(ui);
        if let Some(c) = hex.event.as_ref().and_then(|e| e.value()) {
            *color = [f32::from(c.0[0]) / 255.0, f32::from(c.0[1]) / 255.0, f32::from(c.0[2]) / 255.0, 1.0];
            changed = true;
        }
        let [mut r, mut g, mut b] = srgb_bytes(*color).map(f32::from);
        let mut rgb = false;
        for (label, name, v) in [("R", tl!("Red"), &mut r), ("G", tl!("Green"), &mut g), ("B", tl!("Blue"), &mut b)] {
            ui.label(RichText::new(label).font(theme::mono(11.0)).color(t.text_faint));
            rgb |= widgets::value_field(ui, v, 0.0..=255.0, "", 38.0).accessible_name(name).changed();
        }
        if rgb {
            *color = [r / 255.0, g / 255.0, b / 255.0, 1.0];
            changed = true;
        }
    });
    changed
}
