use super::dropdown::swatch_popup;
use super::{Color32, CornerRadius, Pos2, Rect, Response, Sense, StrokeKind, Tokens, Ui, Vec2, pos2};
use craft_theme::Srgba;
use craft_ui::theme::StrokeExt;

/// Camera Raw-style continuous RGB ribbons on craft-ui's [`Histogram`]. Raw bins remain exact;
/// the library filters them for display.
pub(crate) mod rgb_histogram {
    use craft_ui::composites::{Histogram, HistogramScale};
    use photocraft_algo::histogram::RgbHistogram;

    /// `h` as RGB ribbons. `typical` scales by the typical columns as Camera Raw does, so a shadow or
    /// highlight spike runs off the top; otherwise the tallest interior column fits.
    pub(crate) fn histogram(id: &str, label: &str, h: &RgbHistogram, typical: bool) -> Histogram {
        let [red, green, blue] = &h.channels;
        let histogram = Histogram::new(id, label).rgb([&red[..], &green[..], &blue[..]]);
        if typical { histogram.scale(HistogramScale::Typical) } else { histogram }
    }
}

/// Cached RGB data; drawing work is bounded by the 256 display bins, never the image size.
pub fn rgb_histogram(ui: &mut Ui, histogram: &photocraft_algo::histogram::RgbHistogram, height: f32) -> egui::Response {
    crate::rgb_histogram::histogram("rgb-histogram", tl!("Histogram"), histogram, false).height(height.max(1.0)).show(ui).response
}

/// The fully saturated colour at hue `deg` (degrees), built on sRGB values as Photoshop's hue
/// tracks are: 0 is (255,0,0), 60 is (255,255,0), 120 is (0,255,0) and so on (#6540).
pub fn hue_color(deg: f32) -> Color32 {
    let [r, g, b] = photocraft_color::convert::hsb_to_rgb([deg, 1.0, 1.0]);
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(q(r), q(g), q(b))
}

/// Hue spectrum stops for colour sliders: red, yellow, green, cyan, blue, magenta, red.
pub fn hue_stops() -> Vec<Color32> {
    (0..=12).map(|i| hue_color(i as f32 * 30.0)).collect()
}

/// Paint a small checkerboard (transparency) in `rect`.
pub fn checker(painter: &egui::Painter, rect: Rect, cell: f32) {
    painter.rect_filled(rect, craft_ui::theme::scale::Radius::R0.px(), Color32::from_gray(250));
    let nx = (rect.width() / cell).ceil() as i32;
    let ny = (rect.height() / cell).ceil() as i32;
    for j in 0..ny {
        for i in 0..nx {
            if (i + j) % 2 == 1 {
                let r = Rect::from_min_size(Pos2::new(rect.left() + i as f32 * cell, rect.top() + j as f32 * cell), Vec2::splat(cell)).intersect(rect);
                painter.rect_filled(r, craft_ui::theme::scale::Radius::R0.px(), Color32::from_gray(214));
            }
        }
    }
}

/// A transparency checkerboard of `cell`-point squares clipped to `rect` with `radius` corners,
/// for rounded swatches (#3208): [`checker`] paints square cells, which showed past a rounded
/// fill's corners. This one is a repeating two-by-two tile textured onto the rounded rect.
pub fn checker_rounded(painter: &egui::Painter, rect: Rect, cell: f32, radius: impl Into<CornerRadius>) {
    let tile = checker_tile(painter.ctx());
    painter.add(egui::epaint::RectShape::filled(rect, radius, Color32::WHITE).with_texture(tile.id(), checker_uv(rect, cell)));
}

/// The texture range that tiles `rect` with `cell`-point squares: the tile holds two cells a side.
pub(super) fn checker_uv(rect: Rect, cell: f32) -> Rect {
    let cell = if cell.is_finite() && cell > 0.0 { cell } else { 4.0 };
    Rect::from_min_max(Pos2::ZERO, pos2(rect.width() / (2.0 * cell), rect.height() / (2.0 * cell)))
}

pub(super) fn checker_tile_id() -> egui::Id {
    egui::Id::unique("transparency-checker-tile")
}

/// The checker's two-by-two tile (light, dark / dark, light), loaded once per context and shared
/// by every rounded checker; it repeats and never blurs.
fn checker_tile(ctx: &egui::Context) -> egui::TextureHandle {
    if let Some(tile) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(checker_tile_id())) {
        return tile;
    }
    let (light, dark) = (Color32::from_gray(250), Color32::from_gray(214));
    let image = egui::ColorImage::new([2, 2], vec![light, dark, dark, light]);
    let options = egui::TextureOptions {
        magnification: egui::TextureFilter::Nearest,
        minification: egui::TextureFilter::Nearest,
        wrap_mode: egui::TextureWrapMode::Repeat,
        mipmap_mode: None,
    };
    let tile = ctx.load_texture("transparency-checker-tile", image, options);
    ctx.data_mut(|d| d.insert_temp(checker_tile_id(), tile.clone()));
    tile
}

/// A compact colour popup with the shared desktop eyedropper. Keep egui's colour cache so hue
/// survives black/white and alpha edits, and keep linear and encoded call sites distinct.
pub fn color_edit_button_srgba(ui: &mut Ui, color: &mut Color32) -> Response {
    color_edit_button(ui, color, egui::color_picker::Alpha::BlendOrAdditive)
}

pub(crate) fn color_swatch(ui: &mut Ui, color: Color32) -> Response {
    let t = Tokens::get(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(ui.spacing().interact_size, Sense::click());
    // A swatch's meaning is the colour it edits; callers with a more specific caption use
    // `color_swatch_button` or add `labelled_by`.
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ColorWell, ui.is_enabled(), tl!("Color")));
    checker_rounded(ui.painter(), rect, 4.0, t.radius_sm);
    ui.painter().rect_filled(rect.shrink(1.0), t.radius_sm, color);
    ui.painter().rect_stroke(rect, t.radius_sm, craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.field_border), StrokeKind::Inside);
    response
}

/// Where a press or drag on a picking surface (a colour field, a hue strip) points, from the
/// frame the button goes down and on every move after it. `dragged()` waits for egui's click
/// threshold (a few points of travel, or a long hold), so a press picked nothing until the
/// pointer had moved away and fine adjustments jumped (#7793). Only the primary button picks:
/// `is_pointer_button_down_on` is true for any button, so a right or middle press is ignored.
pub fn pick_pos(resp: &Response) -> Option<Pos2> {
    let held = resp.is_pointer_button_down_on()
        && resp.ctx.input(|i| i.pointer.primary_down() && !i.pointer.button_down(egui::PointerButton::Secondary) && !i.pointer.middle_down());
    if held || resp.clicked() { resp.interact_pointer_pos() } else { None }
}

/// A colour swatch that only reports clicks (no popup): for colours edited in PhotoCraft's own
/// Color Picker dialog. `label` names it for hover text and accessibility.
pub fn color_swatch_button(ui: &mut Ui, color: Color32, label: &str) -> Response {
    let t = Tokens::get(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(ui.spacing().interact_size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ColorWell, true, label));
    checker_rounded(ui.painter(), rect, 4.0, t.radius_sm);
    ui.painter().rect_filled(rect.shrink(1.0), t.radius_sm, color);
    ui.painter().rect_stroke(rect, t.radius_sm, craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.field_border), StrokeKind::Inside);
    response.on_hover_text(label)
}

fn color_edit_button(ui: &mut Ui, color: &mut Color32, alpha: egui::color_picker::Alpha) -> Response {
    let mut response = color_swatch(ui, *color);
    let id = response.id.with("screen-color");
    if let Some(rgb) = crate::screen_picker::take(ui.ctx(), id) {
        *color = sampled_color(rgb, *color, alpha);
        response.mark_changed();
    }
    let with_alpha = !matches!(alpha, egui::color_picker::Alpha::Opaque);
    swatch_popup(&response).show(|ui| {
        let [r, g, b, a] = color.to_srgba_unmultiplied();
        let current = Srgba::new(f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, if with_alpha { f32::from(a) / 255.0 } else { 1.0 });
        if let Some(picked) = popup_picker(ui, id, current, with_alpha) {
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            *color = if with_alpha {
                Color32::from_rgba_unmultiplied(byte(picked.r), byte(picked.g), byte(picked.b), byte(picked.a))
            } else {
                Color32::from_rgb(byte(picked.r), byte(picked.g), byte(picked.b))
            };
            response.mark_changed();
        }
    });
    response
}

/// The library's Compact colour picker in a swatch popup. Returns the colour when it was edited.
/// The eyedropper starts a screen pick for `screen_id`; the caller takes the result, also when
/// the popup has closed meanwhile.
fn popup_picker(ui: &mut Ui, screen_id: egui::Id, color: Srgba, alpha: bool) -> Option<Srgba> {
    use craft_ui::composites::{ColorPicker, ColorPickerLayout};
    let numbers = craft_ui::localized::LocaleNumbers::of(ui);
    let shown = ColorPicker::new("popup-picker", tl!("Color"), color, &numbers)
        .layout(ColorPickerLayout::Compact)
        .alpha(alpha)
        .eyedropper(tl!("Pick screen color"))
        .eyedropper_busy(!crate::screen_picker::available(ui.ctx()) || crate::screen_picker::busy(ui.ctx()))
        .text(|key| tl!(key).to_string())
        .show(ui);
    if shown.eyedropper_requested {
        crate::screen_picker::request(ui.ctx(), screen_id);
    }
    shown.edit.event.as_ref().and_then(|e| e.value()).map(|p| p.color)
}

fn sampled_color(rgb: [f32; 3], original: Color32, alpha: egui::color_picker::Alpha) -> Color32 {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    if matches!(alpha, egui::color_picker::Alpha::BlendOrAdditive) && original.is_additive() {
        return Color32::from_rgb_additive(byte(rgb[0]), byte(rgb[1]), byte(rgb[2]));
    }
    Color32::from_rgba_unmultiplied(
        byte(rgb[0]),
        byte(rgb[1]),
        byte(rgb[2]),
        if matches!(alpha, egui::color_picker::Alpha::Opaque) { 255 } else { original.a() },
    )
}

pub fn color_edit_button_srgb(ui: &mut Ui, rgb: &mut [u8; 3]) -> Response {
    let mut color = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let response = color_edit_button(ui, &mut color, egui::color_picker::Alpha::Opaque);
    if response.changed() {
        *rgb = [color.r(), color.g(), color.b()];
    }
    response
}

/// Like egui's float RGB widget, this entry point stores linear RGB (not encoded hex values).
pub fn color_edit_button_rgb(ui: &mut Ui, rgb: &mut [f32; 3]) -> Response {
    let mut response = color_swatch(ui, Color32::from(egui::Rgba::from_rgb(rgb[0], rgb[1], rgb[2])));
    let id = response.id.with("screen-color");
    let to_linear = |color: [f32; 3]| color.map(egui::ecolor::linear_from_gamma);
    let to_encoded = |color: [f32; 3]| color.map(egui::ecolor::gamma_from_linear);
    if let Some(color) = crate::screen_picker::take(ui.ctx(), id) {
        *rgb = to_linear(color);
        response.mark_changed();
    }
    swatch_popup(&response).show(|ui| {
        let [r, g, b] = to_encoded(*rgb);
        if let Some(picked) = popup_picker(ui, id, Srgba::rgb(r, g, b), false) {
            *rgb = to_linear([picked.r, picked.g, picked.b]);
            response.mark_changed();
        }
    });
    response
}
