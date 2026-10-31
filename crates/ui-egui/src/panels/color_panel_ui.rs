//! Photoshop's Color panel modes, picked from its panel menu and the same in every theme (#6000,
//! #2569): Hue Cube and Brightness Cube (drawn by [`crate::panels::color_field`]), Color Wheel (HSB
//! sliders over a hue ring around a saturation/brightness triangle), and the Grayscale, RGB, HSB,
//! CMYK, Lab and Web Color sliders over a colour ramp. The panel menu also has Copy Color as HTML,
//! Copy Color's Hex Code, the ramp choices (RGB Spectrum, CMYK Spectrum, Grayscale Ramp, Current
//! Colors) and Make Ramp Web Safe. Shift-click on the ramp cycles the ramps, Alt-click picks the
//! colour the panel isn't editing, and a right-click opens the ramp menu.

use craft::ui::theme::StrokeExt;
use std::sync::Arc;

use craft::color::ui::{ColorRamp, ColorWarning, ColorWarningChip, ColorWheel};
use craft::theme::{Channel, ColorModel, DefaultOn, HsbModel, LabModel, MAX_CHANNELS, ModelValues, RgbModel, Srgba};
use craft::ui::menu::{ContextMenu, MenuEntry, MenuItem, OpenAt};
use egui::{Color32, Rect, Sense, StrokeKind, UiBuilder, pos2, vec2};
use serde_json::json;

use crate::PhotocraftApp;
use crate::color_picker_ui::{CmykModel, hex, not_web_safe, out_of_gamut, srgba};
use crate::panels::{color_field_fill_id, field_chips};
use crate::state::{ColorPanelMode, ColorPanelState, ColorRamp as PanelRamp};
use crate::theme::Tokens;
use crate::widgets;
use craft::ui::controls::{Slider, SliderThumb};
use photocraft_color::convert;

/// Width of the chips column, as the cube field's.
const CHIPS_W: f32 = 38.0;
/// Height of one slider row.
const ROW_H: f32 = 22.0;
/// Width of a slider row's value field.
const FIELD_W: f32 = 44.0;
/// Width of the unit after a field ("°", "%").
const UNIT_W: f32 = 14.0;

/// The Color tab: whatever the panel menu picked.
pub(crate) fn panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    match app.ui.color_panel.mode {
        ColorPanelMode::HueCube | ColorPanelMode::BrightnessCube => crate::panels::color_field(app, ui),
        ColorPanelMode::ColorWheel => wheel(app, ui),
        mode => sliders(app, ui, mode),
    }
    crate::apply_color::track_focus(ui.ctx());
}

/// The colour model a slider mode edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Model {
    Gray,
    Rgb,
    Hsb,
    Cmyk,
    Lab,
    Web,
}

impl Model {
    pub(crate) fn of(mode: ColorPanelMode) -> Model {
        match mode {
            ColorPanelMode::GrayscaleSlider => Model::Gray,
            ColorPanelMode::HsbSliders | ColorPanelMode::ColorWheel => Model::Hsb,
            ColorPanelMode::CmykSliders => Model::Cmyk,
            ColorPanelMode::LabSliders => Model::Lab,
            ColorPanelMode::WebColorSliders => Model::Web,
            ColorPanelMode::RgbSliders | ColorPanelMode::HueCube | ColorPanelMode::BrightnessCube => Model::Rgb,
        }
    }

    /// The model whose channels and conversions the sliders use, in Photoshop's order.
    pub(crate) fn color_model(self) -> &'static dyn ColorModel {
        match self {
            Model::Gray => &GrayModel,
            Model::Rgb | Model::Web => &RgbModel,
            Model::Hsb => &HsbModel,
            Model::Cmyk => &CmykModel(None),
            Model::Lab => &LabModel,
        }
    }

    /// Each slider: label, range and unit.
    pub(crate) fn channels(self) -> &'static [Channel] {
        self.color_model().channels()
    }

    /// The sRGB colour of slider values `v` (in the units the fields show).
    pub(crate) fn to_rgb(self, v: &ModelValues) -> [f32; 3] {
        let c = self.color_model().to_srgb(&v.values);
        [c.r, c.g, c.b].map(|x| if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.0 })
    }

    /// Slider values of `rgb`, rounded to whole numbers as Photoshop's fields show them.
    #[cfg(test)]
    pub(crate) fn values_of(self, rgb: [f32; 3]) -> ModelValues {
        let mut v = self.color_model().from_srgb(srgba(rgb));
        round_values(&mut v);
        v
    }
}

fn round_values(v: &mut ModelValues) {
    for x in &mut v.values {
        *x = if x.is_finite() { x.round() } else { 0.0 };
    }
}

const GRAY_CHANNELS: [Channel; 1] =
    [Channel { name: "Gray", qualified: "Gray", label: "K", min: 0.0, max: 100.0, unit: "%", decimals: 0, step: 1.0, track_stops: 2 }];

/// The Grayscale Slider's one channel: the ink coverage of black, from white at 0 to black at 100.
struct GrayModel;

impl ColorModel for GrayModel {
    fn id(&self) -> &'static str {
        "gray"
    }
    fn name(&self) -> &'static str {
        "Grayscale"
    }
    fn channels(&self) -> &[Channel] {
        &GRAY_CHANNELS
    }
    fn from_srgb(&self, color: Srgba) -> ModelValues {
        ModelValues::new("gray", &[(1.0 - convert::rgb_to_gray([color.r, color.g, color.b])) * 100.0])
    }
    fn to_srgb(&self, values: &[f32; MAX_CHANNELS]) -> Srgba {
        let g = 1.0 - values[0] / 100.0;
        Srgba::rgb(g, g, g)
    }
    fn default_on(&self) -> DefaultOn {
        DefaultOn { full: false, compact: false }
    }
}

/// The colour the panel edits.
pub(crate) fn active(app: &PhotocraftApp) -> [f32; 3] {
    let c = if app.ui.color_panel.background { app.session.tools.background } else { app.session.tools.foreground };
    [c[0], c[1], c[2]].map(|x| x.clamp(0.0, 1.0))
}

/// Set the background colour, or the foreground and recolour what is selected as the tool in hand
/// says (`apply_color`); the picks of one drag or typed value are one history step.
pub(crate) fn set_color(app: &mut PhotocraftApp, ctx: &egui::Context, background: bool, rgb: [f32; 3]) {
    let c = [rgb[0], rgb[1], rgb[2], 1.0];
    if background {
        app.session.tools.background = c;
    } else {
        app.session.tools.foreground = c;
        crate::apply_color::foreground_changed(app, crate::apply_color::gesture(ctx));
    }
}

/// Slider values for the colour the panel edits: the ones last shown while they still give it.
fn current_values(ui: &egui::Ui, model: Model, background: bool, rgb: [f32; 3]) -> ModelValues {
    let mut v = crate::panels::kept_values(ui, model.color_model(), background, rgb);
    round_values(&mut v);
    v
}

/// Apply edited slider values: the colour, and the values remembered.
fn apply_values(app: &mut PhotocraftApp, ui: &egui::Ui, model: Model, v: ModelValues) {
    let background = app.ui.color_panel.background;
    set_color(app, ui.ctx(), background, model.to_rgb(&v));
    crate::panels::remember_values(ui, background, v);
}

/// Track colours of slider `i` at `steps` + 1 points, the other sliders at `v`.
fn track_stops(model: Model, v: &ModelValues, i: usize, steps: usize) -> Vec<Color32> {
    let Some(ch) = model.channels().get(i) else { return Vec::new() };
    (0..=steps)
        .map(|k| {
            let mut w = *v;
            if let Some(slot) = w.values.get_mut(i) {
                *slot = ch.min + (ch.max - ch.min) * k as f32 / steps as f32;
            }
            // HSB's hue track is the full spectrum, whatever the saturation and brightness.
            if model == Model::Hsb && i == 0 {
                w.values[1] = 100.0;
                w.values[2] = 100.0;
            }
            rgb32(model.to_rgb(&w))
        })
        .collect()
}

pub(crate) fn rgb32(c: [f32; 3]) -> Color32 {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(q(c[0]), q(c[1]), q(c[2]))
}

/// The slider rows of `model`. Returns the edited values, if any.
fn slider_rows(ui: &mut egui::Ui, model: Model, v: ModelValues, background: bool, dynamic: bool) -> Option<ModelValues> {
    let t = Tokens::get(ui.ctx());
    // Dynamic Color Sliders off: the tracks keep the colours they had when a drag began.
    let stops_key = egui::Id::unique(("color-panel-stops", model, background));
    let dragging = ui.input(|i| i.pointer.any_down());
    let stops_from = if dynamic || !dragging { v } else { ui.data(|d| d.get_temp::<ModelValues>(stops_key)).unwrap_or(v) };
    if !dragging {
        ui.data_mut(|d| d.insert_temp(stops_key, v));
    }
    let mut out = None;
    let mut values = v;
    for (i, ch) in model.channels().iter().enumerate() {
        let (label, lo, hi, unit) = (ch.label, ch.min, ch.max, ch.unit);
        let Some(&current) = values.values.get(i) else { continue };
        let mut x = current;
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.set_height(ROW_H);
            let (lr, _) = ui.allocate_exact_size(vec2(12.0, ROW_H), Sense::hover());
            ui.painter().text(lr.left_center(), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(12.0), t.text_dim);
            let track_w = (ui.available_width() - FIELD_W - UNIT_W - 8.0).max(24.0);
            let web = model == Model::Web;
            let stops = track_stops(model, &stops_from, i, 24);
            // Web Color tracks snap to the web steps (00, 33, 66, 99, CC, FF).
            let mut slider = Slider::new(("color-panel-slider", model, background, i), label, f64::from(x), f64::from(lo)..=f64::from(hi))
                .step(if web { 51.0 } else { 1.0 })
                .width(track_w)
                .track_colors(stops)
                .thumb(SliderThumb::Triangle);
            if web {
                slider = slider.ticks(5);
            }
            if let Some(v) = slider.show(ui).event.as_ref().and_then(|e| e.value()).copied() {
                let v = v as f32;
                if v != x {
                    x = v;
                    changed = true;
                }
            }
            if model == Model::Web {
                changed |= hex_pair(ui, ui.scope_id().with(("web-field", i, background)), &mut x);
            } else if widgets::value_field(ui, &mut x, lo..=hi, "", FIELD_W).changed() {
                changed = true;
            }
            let (ur, _) = ui.allocate_exact_size(vec2(UNIT_W, ROW_H), Sense::hover());
            ui.painter().text(ur.left_center(), egui::Align2::LEFT_CENTER, unit, egui::FontId::proportional(12.0), t.text_faint);
        });
        if changed && let Some(slot) = values.values.get_mut(i) {
            *slot = x.clamp(lo, hi).round();
            out = Some(values);
        }
    }
    out
}

/// A Web Color slider's field: the channel as two hex digits. Valid input sets `value` (0..255)
/// and returns true; while focused it shows what is typed, and an invalid entry snaps back.
fn hex_pair(ui: &mut egui::Ui, id: egui::Id, value: &mut f32) -> bool {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(FIELD_W, ROW_H), Sense::hover());
    widgets::surface(ui, rect, t.field, false);
    if !t.bevel {
        ui.painter().rect_stroke(rect, t.radius_sm, craft::ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.field_border), StrokeKind::Inside);
    }
    let byte = value.clamp(0.0, 255.0).round() as u8;
    let typing = if ui.memory(|m| m.has_focus(id)) { ui.data(|d| d.get_temp::<String>(id)) } else { None };
    let mut text = typing.unwrap_or_else(|| format!("{byte:02X}"));
    let inner = rect.shrink2(vec2(5.0, 2.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let resp = child
        .add(egui::TextEdit::singleline(&mut text).id(id).char_limit(2).desired_width(inner.width()).frame(egui::Frame::NONE).font(crate::theme::mono(12.0)));
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    // Two hex digits only: `from_str_radix` would also take a sign ("+F").
    if resp.changed()
        && text.len() == 2
        && text.bytes().all(|b| b.is_ascii_hexdigit())
        && let Ok(b) = u8::from_str_radix(&text, 16)
    {
        *value = f32::from(b);
        return true;
    }
    false
}

/// The slider modes: chips (with the out-of-gamut and web-colour alerts under them) beside the
/// sliders, over the colour ramp.
fn sliders(app: &mut PhotocraftApp, ui: &mut egui::Ui, mode: ColorPanelMode) {
    let model = Model::of(mode);
    let background = app.ui.color_panel.background;
    let rgb = active(app);
    let v = current_values(ui, model, background, rgb);
    let dynamic = app.session.prefs().interface.dynamic_color_sliders;
    let rows_h = model.channels().len() as f32 * (ROW_H + ui.spacing().item_spacing.y);
    let top_h = rows_h.max(CHIPS_W + 40.0);
    let mut edited = None;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (col, _) = ui.allocate_exact_size(vec2(CHIPS_W, top_h), Sense::hover());
        field_chips(app, ui, col);
        alerts(app, ui, col, rgb, model == Model::Web);
        ui.vertical(|ui| {
            edited = slider_rows(ui, model, v, background, dynamic);
        });
    });
    if let Some(v) = edited {
        apply_values(app, ui, model, v);
    }
    ui.add_space(craft::ui::theme::scale::Space::S6.px());
    let fill = ui.data(|d| d.get_temp::<f32>(color_field_fill_id()));
    let gaps = 2.0 * ui.spacing().item_spacing.y + 6.0;
    let ramp_h = fill.filter(|f| f.is_finite()).map_or(36.0, |f| (f - top_h - gaps).clamp(28.0, 2000.0));
    ramp(app, ui, ramp_h);
}

/// Photoshop's alerts under the chips: ⚠ when the colour can't print (click: the nearest CMYK
/// colour) and, for Web Color sliders, the cube when it isn't a web colour (click: the nearest).
pub(crate) fn alerts(app: &mut PhotocraftApp, ui: &mut egui::Ui, col: Rect, rgb: [f32; 3], web: bool) {
    let background = app.ui.color_panel.background;
    let mut warnings = Vec::new();
    if let Some(near) = out_of_gamut(rgb) {
        warnings.push(ColorWarning::new(tl!("Out of gamut for printing: click for the nearest printable colour"), srgba(near)));
    }
    if web && let Some(near) = not_web_safe(rgb) {
        warnings.push(ColorWarning::web_colors(tl!("Not a web colour: click for the nearest web colour"), srgba(near)));
    }
    let area = Rect::from_min_max(pos2(col.left() + 1.0, col.top() + CHIPS_W + 2.0), col.max);
    let row = UiBuilder::new().max_rect(area).layout(egui::Layout::left_to_right(egui::Align::Min));
    let mut row = ui.new_child(row);
    row.spacing_mut().item_spacing.x = 4.0;
    for (slot, warning) in warnings.into_iter().enumerate() {
        if ColorWarningChip::new(("color-panel-alert", slot), warning.clone()).show(&mut row).clicked() {
            let near = warning.nearest();
            set_color(app, row.ctx(), background, [near.r, near.g, near.b]);
        }
    }
}

/// The colour ramp with the white and black boxes at its right end. A click or drag picks; with
/// Alt it picks the colour the panel isn't editing; Shift-click cycles the ramps; a right-click
/// opens the ramp menu.
fn ramp(app: &mut PhotocraftApp, ui: &mut egui::Ui, height: f32) {
    let cp = app.ui.color_panel;
    let (fg, bg) = (rgb3(app.session.tools.foreground), rgb3(app.session.tools.background));
    // The CMYK Spectrum is the spectrum through a CMYK round trip.
    let gamut = |c: Srgba| srgba(convert::cmyk_to_rgb(convert::rgb_to_cmyk([c.r, c.g, c.b])).map(|v| v.clamp(0.0, 1.0)));
    let revision = convert::active_cmyk_space().map_or(0, |space| Arc::as_ptr(&space) as usize as u64);
    let mut ramp = ColorRamp::new("color-panel-ramp", tl!("Color Ramp"), cp.ramp.kind())
        .kind_name(tl!(cp.ramp.label()))
        .ends(srgba(fg), srgba(bg))
        .web_safe(cp.web_safe_ramp)
        .height(height)
        .press_waits(true)
        .texts(tl!("White"), tl!("Black"));
    if cp.ramp == PanelRamp::CmykSpectrum {
        ramp = ramp.gamut(&gamut).gamut_revision(revision);
    }
    let shown = ramp.show(ui);
    if shown.cycle {
        app.ui.color_panel.ramp = cp.ramp.next();
    }
    if let Some(pick) = shown.edit.event.as_ref().and_then(|e| e.value()) {
        let target = cp.background != pick.other;
        set_color(app, ui.ctx(), target, [pick.color.r, pick.color.g, pick.color.b]);
    }
    let entries = ramp_entries(&cp);
    let menu = ContextMenu::new(&entries).own_gestures(false).open_at(shown.menu_at.map(OpenAt::Pointer)).show(ui, &shown.bar);
    if let Some(choice) = menu.chosen {
        choice.apply(&mut app.ui.color_panel);
    }
}

fn rgb3(c: [f32; 4]) -> [f32; 3] {
    [c[0], c[1], c[2]].map(|v| v.clamp(0.0, 1.0))
}

/// The wheel's words, in PhotoCraft's.
fn wheel_word(key: &str) -> String {
    let text: &str = match key {
        "Hue" => tl!("Hue"),
        "Saturation and brightness" => tl!("Saturation and brightness"),
        "Saturation" => tl!("Saturation"),
        "Brightness" => tl!("Brightness"),
        other => tl!(other),
    };
    text.to_string()
}

/// The Color Wheel: HSB sliders over a hue ring (red at the right, running counter-clockwise
/// through yellow and green) around a triangle whose tip is the hue, with white and black at its
/// other corners; the add-to-swatches button sits under it.
fn wheel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let top = ui.cursor().top();
    let background = app.ui.color_panel.background;
    let rgb = active(app);
    let v = current_values(ui, Model::Hsb, background, rgb);
    let dynamic = app.session.prefs().interface.dynamic_color_sliders;
    let rows_h = 3.0 * (ROW_H + ui.spacing().item_spacing.y);
    let top_h = rows_h.max(CHIPS_W);
    let mut edited = None;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (col, _) = ui.allocate_exact_size(vec2(CHIPS_W, top_h), Sense::hover());
        field_chips(app, ui, col);
        ui.vertical(|ui| {
            edited = slider_rows(ui, Model::Hsb, v, background, dynamic);
        });
    });
    let mut v = edited.unwrap_or(v);
    let mut changed = edited.is_some();
    ui.add_space(craft::ui::theme::scale::Space::S6.px());
    let fill = ui.data(|d| d.get_temp::<f32>(color_field_fill_id()));
    let w = ui.available_width();
    // The wheel takes what the dock group has left under the sliders as drawn (they stand taller
    // than `ROW_H`), as the slider modes' ramp: an estimate and an 80 pt floor ran it past the
    // bottom of the Color group at its default height.
    let used = ui.cursor().top() - top;
    let side = fill.filter(|f| f.is_finite()).map_or(w.min(220.0), |f| (f - used).min(w));
    let shown = ColorWheel::new("color-panel-wheel", tl!("Color Wheel"), v).side(side).press_waits(true).text(wheel_word).show(ui);
    if let Some(picked) = shown.edit.event.as_ref().and_then(|e| e.value()) {
        v = *picked;
        changed = true;
    }
    // Photoshop's add-to-swatches button under the wheel.
    let area = shown.area;
    let add = Rect::from_min_size(pos2(area.right() - 16.0, area.bottom() - 16.0), vec2(14.0, 14.0));
    ui.painter().rect_stroke(add, craft::ui::theme::scale::Radius::R2.px(), craft::ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.text_dim), StrokeKind::Inside);
    ui.painter().line_segment(
        [pos2(add.center().x, add.top() + 3.5), pos2(add.center().x, add.bottom() - 3.5)],
        craft::ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.text_dim),
    );
    ui.painter().line_segment(
        [pos2(add.left() + 3.5, add.center().y), pos2(add.right() - 3.5, add.center().y)],
        craft::ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.text_dim),
    );
    let add_resp = ui.interact(add, ui.scope_id().with("color-wheel-add"), Sense::click());
    add_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), "Add to Swatches"));
    if add_resp.on_hover_text(tl!("Create new swatch")).clicked()
        && let Err(e) = app.run("swatches.add", json!({"color": hex(rgb)}))
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
    if changed {
        apply_values(app, ui, Model::Hsb, v);
    }
}

/// `color="#rrggbb"`, what Copy Color as HTML puts on the clipboard.
pub(crate) fn html_color(rgb: [f32; 3]) -> String {
    format!("color=\"{}\"", hex(rgb))
}

/// `rrggbb`, what Copy Color's Hex Code puts on the clipboard.
pub(crate) fn hex_code(rgb: [f32; 3]) -> String {
    hex(rgb).trim_start_matches('#').to_string()
}

/// A panel menu row with Photoshop's check mark column.
fn check_item(ui: &mut egui::Ui, on: bool, label: &str) -> bool {
    ui.button(format!("{}{}", if on { "✔ " } else { "   " }, tl!(label))).clicked()
}

/// A choice of the ramp menu.
#[derive(Clone, Copy, Debug, PartialEq)]
enum RampChoice {
    Ramp(PanelRamp),
    WebSafe,
}

impl RampChoice {
    fn apply(self, cp: &mut ColorPanelState) {
        match self {
            RampChoice::Ramp(r) => cp.ramp = r,
            RampChoice::WebSafe => cp.web_safe_ramp = !cp.web_safe_ramp,
        }
    }
}

/// The ramp choices and Make Ramp Web Safe (the panel menu's last section and the ramp's
/// right-click menu).
fn ramp_entries(cp: &ColorPanelState) -> Vec<MenuEntry<RampChoice>> {
    let mut entries: Vec<MenuEntry<RampChoice>> =
        PanelRamp::ALL.into_iter().map(|r| MenuItem::new(tl!(r.label()), RampChoice::Ramp(r)).radio(cp.ramp == r).into()).collect();
    entries.push(MenuEntry::Separator);
    entries.push(MenuItem::new(tl!("Make Ramp Web Safe"), RampChoice::WebSafe).check(cp.web_safe_ramp).into());
    entries
}

/// The ramp section of the panel menu.
fn ramp_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    for entry in ramp_entries(&app.ui.color_panel) {
        match entry {
            MenuEntry::Separator => {
                ui.separator();
            }
            MenuEntry::Item(item) => {
                if let Some(choice) = item.command().copied()
                    && ui.button(format!("{}{}", if item.item_kind().checked() == Some(true) { "✔ " } else { "   " }, item.label())).clicked()
                {
                    choice.apply(&mut app.ui.color_panel);
                    ui.close();
                }
            }
        }
    }
}

/// The Color panel's panel menu, Photoshop's: the modes, the copy commands, the ramps.
pub(crate) fn panel_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    // One section, as Photoshop's: no separator between Color Wheel and Grayscale Slider.
    for mode in ColorPanelMode::ALL {
        if check_item(ui, app.ui.color_panel.mode == mode, mode.label()) {
            app.ui.color_panel.mode = mode;
            ui.close();
        }
    }
    ui.separator();
    let rgb = active(app);
    if ui.button(tl!("Copy Color as HTML")).clicked() {
        ui.ctx().copy_text(html_color(rgb));
        ui.close();
    }
    if ui.button(tl!("Copy Color's Hex Code")).clicked() {
        ui.ctx().copy_text(hex_code(rgb));
        ui.close();
    }
    ui.separator();
    ramp_menu(app, ui);
}
