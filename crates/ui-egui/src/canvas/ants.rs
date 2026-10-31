//! Selection outlines on the screen, drawn by the library's `Marquee` painter path. The callers
//! hold points already placed on the screen (a transformed outline, a drag preview), so they are
//! painted with `Marquee::paint_screen`, clipped to the painter.

use craft::ui::canvas::kurbo::Point;
use craft::ui::canvas::{Marquee, MarqueeLook};
use egui::{Painter, Pos2};

fn point(p: Pos2) -> Point {
    Point::new(f64::from(p.x), f64::from(p.y))
}

/// Paints `marquee` (screen points) in `look` within the painter's clip rect.
fn paint(painter: &Painter, marquee: &Marquee, look: MarqueeLook, time: f64) {
    let clip = painter.clip_rect();
    if clip.is_finite() {
        marquee.paint_screen(painter, clip, look, time);
    }
}

/// Marching ants along `lines` (screen points) at frame time `time`.
pub(crate) fn draw(painter: &Painter, lines: impl Iterator<Item = (Pos2, Pos2)>, time: f64) {
    paint(painter, &Marquee::segments(lines.map(|(a, b)| (point(a), point(b))), ""), MarqueeLook::Ants, time);
}

/// Animated ants along the polyline `points` (screen points), back to the first when `closed`.
pub(crate) fn draw_polyline(painter: &Painter, points: &[Pos2], closed: bool) {
    let time = painter.ctx().input(|i| i.time);
    let points = points.iter().map(|&p| point(p));
    let marquee = if closed { Marquee::polygon(points, "") } else { Marquee::polyline(points, "") };
    paint(painter, &marquee, MarqueeLook::Ants, time);
}

/// The polygon `points` (screen points) in a non-ants `look`: open in the drawing look, closed in
/// the contrast look.
pub(crate) fn draw_look(painter: &Painter, points: &[Pos2], look: MarqueeLook) {
    let time = painter.ctx().input(|i| i.time);
    paint(painter, &Marquee::polygon(points.iter().map(|&p| point(p)), ""), look, time);
}
