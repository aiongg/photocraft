//! Selection outlines on the screen, drawn by the library's `Marquee` painter path.
//!
//! The callers hold points already placed on the screen (a transformed outline, a drag preview),
//! so they are drawn through a view that maps every point to itself.

use craft::ui::canvas::kurbo::Point;
use craft::ui::canvas::{CanvasView, Marquee, MarqueeLook};
use egui::{Painter, Pos2};

/// A view of the painter's clip rect that places each document point at the same screen point.
fn screen_view(painter: &Painter) -> Option<CanvasView> {
    let rect = painter.clip_rect();
    rect.is_finite().then(|| {
        let c = rect.center();
        CanvasView::new(rect, Point::new(f64::from(c.x), f64::from(c.y)), 1.0)
    })
}

fn point(p: Pos2) -> Point {
    Point::new(f64::from(p.x), f64::from(p.y))
}

/// Marching ants along `lines` (screen points) at frame time `time`.
pub(crate) fn draw(painter: &Painter, lines: impl Iterator<Item = (Pos2, Pos2)>, time: f64) {
    let Some(view) = screen_view(painter) else { return };
    Marquee::segments(lines.map(|(a, b)| (point(a), point(b))), "").paint(painter, &view, MarqueeLook::Ants, time);
}

/// Animated ants along the polyline `points` (screen points), back to the first when `closed`.
pub(crate) fn draw_polyline(painter: &Painter, points: &[Pos2], closed: bool) {
    let Some(view) = screen_view(painter) else { return };
    let time = painter.ctx().input(|i| i.time);
    let marquee = if closed {
        Marquee::polygon(points.iter().map(|&p| point(p)), "")
    } else {
        Marquee::segments(points.windows(2).map(|w| (point(w[0]), point(w[1]))), "")
    };
    marquee.paint(painter, &view, MarqueeLook::Ants, time);
}

/// The polygon `points` (screen points) in a non-ants `look`: open in the drawing look, closed in
/// the contrast look.
pub(crate) fn draw_look(painter: &Painter, points: &[Pos2], look: MarqueeLook) {
    let Some(view) = screen_view(painter) else { return };
    let time = painter.ctx().input(|i| i.time);
    Marquee::polygon(points.iter().map(|&p| point(p)), "").paint(painter, &view, look, time);
}
