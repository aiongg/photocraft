//! Shape fill and stroke paints shared by the options bar and the Properties panel.

use craft::color::palette::SwatchGroup;
use craft::color::ui::{GradientPreview, Paint, PaintChip, PaintChipOutput, SwatchKey};
use craft::draw::stroke::StrokeStyle;
use craft::draw::ui::shape::{ShapeAppearance, ShapePaint};
use craft::theme::Srgba;
use craft::ui::contract::{Bind, Show as _};
use craft::ui::patterns::presets::PresetState;
use serde_json::{Value, json};

use crate::shape_stroke_ui::StrokeOptions;

pub(crate) fn color_of(f: &photocraft_doc::Fill) -> Option<[f32; 4]> {
    match f {
        photocraft_doc::Fill::Solid(c) => Some(rgba_of(c)),
        _ => None,
    }
}

pub(crate) fn rgba_of(c: &photocraft_doc::Color) -> [f32; 4] {
    let [r, g, b] = c.to_rgb();
    [r, g, b, c.alpha]
}

/// A float RGBA colour as the framework's colour.
pub(crate) fn srgba(c: [f32; 4]) -> Srgba {
    Srgba::new(c[0], c[1], c[2], c[3])
}

/// The framework's colour as float RGBA.
pub(crate) fn rgba(c: Srgba) -> [f32; 4] {
    [c.r, c.g, c.b, c.a]
}

/// A solid or gradient paint exactly as `shape.create` / `shape.edit` take it: float RGBA, the
/// gradient's stops as stored colours (a CMYK or Lab stop keeps its model) and every gradient
/// field, so a swatch edit (angle, style…) keeps the rest of the gradient. The swatches never
/// hand it a pattern (they don't offer one).
pub(crate) fn paint_param(f: &photocraft_doc::Fill) -> Value {
    match f {
        photocraft_doc::Fill::Gradient { stops, angle, scale, style, reverse, opacity_stops, midpoints, offset, dither, align } => json!({"gradient": {
            "stops": stops.iter().map(|(t, c)| json!([t, c])).collect::<Vec<_>>(),
            "angle": angle, "scale": scale * 100.0, "style": format!("{style:?}").to_ascii_lowercase(), "reverse": reverse,
            "opacityStops": opacity_stops, "midpoints": midpoints, "offset": [offset.0, offset.1], "dither": dither, "align": align,
        }}),
        other => json!(color_of(other)),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PaintChange {
    Color([f32; 4]),
    /// A `Fill::Gradient` (the chip's Gradient tab).
    Gradient(photocraft_doc::Fill),
    None,
}

/// The chip's preview of a gradient fill: its stops by position, ascending.
pub(crate) fn gradient_preview(fill: &photocraft_doc::Fill) -> GradientPreview {
    let photocraft_doc::Fill::Gradient { stops, .. } = fill else { return GradientPreview::new(Vec::new()) };
    let mut ramp: Vec<(f32, Srgba)> = stops.iter().map(|(p, c)| (p.clamp(0.0, 1.0), srgba(rgba_of(c)))).collect();
    ramp.sort_by(|a, b| a.0.total_cmp(&b.0));
    GradientPreview::new(ramp)
}

/// The paint a chip shows for `fill` (`None` is No Color). A pattern shows as No Color: shapes
/// do not offer one.
pub(crate) fn paint_of(fill: Option<&photocraft_doc::Fill>) -> Paint {
    match fill {
        Some(photocraft_doc::Fill::Solid(c)) => Paint::Solid(srgba(rgba_of(c))),
        Some(f @ photocraft_doc::Fill::Gradient { .. }) => Paint::Gradient(gradient_preview(f)),
        _ => Paint::None,
    }
}

/// The `shape.edit` parameter of a chip's `paint`: a colour as float RGBA, a gradient as its stops, `null` for No
/// Color or a paint the shape tools do not offer (a pattern or picture).
pub(crate) fn fill_param(paint: &Paint) -> Value {
    match paint {
        Paint::Solid(c) => json!(rgba(*c)),
        Paint::Gradient(preview) => {
            let stops: Vec<Value> = preview.stops.iter().map(|(t, c)| json!([t, photocraft_doc::Color::rgba(c.r, c.g, c.b, c.a)])).collect();
            json!({"gradient": {"stops": stops}})
        }
        _ => Value::Null,
    }
}

/// The colour of a solid `paint`, for the Recently Used Colors.
pub(crate) fn solid_rgb(paint: &Paint) -> Option<[f32; 3]> {
    match paint {
        Paint::Solid(c) => Some([c.r, c.g, c.b]),
        _ => None,
    }
}

/// What the Shape Properties panel and the shape options bar show for shape layer `sh`; `locked` disables every field.
pub(crate) fn appearance_of(sh: &photocraft_doc::ShapeLayer, locked: bool) -> ShapeAppearance {
    let width = sh.stroke.as_ref().map_or(0.0, |s| s.width);
    let style = sh.stroke.as_ref().map(|s| StrokeStyle::from(&StrokeOptions::from(s))).unwrap_or_default();
    ShapeAppearance::new(paint_of(sh.fill.as_ref()), paint_of(sh.stroke.as_ref().map(|s| &s.paint)), f64::from(width)).stroke_style(style).locked(locked)
}

/// The Fill or Stroke chip of the Shape Properties panel: the Recently Used Colors, the swatch library and the full
/// Color Picker. `which` is the chip's own paint; the chip has no Gradient tab here.
pub(crate) fn decorate_chip<'a>(
    chip: PaintChip<'a>,
    _which: ShapePaint,
    recent: &'a [Srgba],
    swatches: &'a [SwatchGroup],
    folders: &'a mut PresetState<SwatchKey>,
    ui: &mut egui::Ui,
) -> PaintChipOutput {
    chip.recent(recent).swatches(swatches, Bind::bound(folders)).full_picker(true).show(ui)
}
