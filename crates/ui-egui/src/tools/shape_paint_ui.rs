//! Shape fill and stroke paints shared by the options bar and the Properties panel.

use craft::color::ui::{GradientPreview, Paint};
use craft::theme::Srgba;
use serde_json::{Value, json};

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
