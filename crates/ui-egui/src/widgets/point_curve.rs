//! The point insert behind the Curves eyedroppers. The graph itself is the library's `CurveEditor`.

/// Most points a curve can hold (Photoshop's limit).
pub const MAX_POINTS: usize = 16;
/// Points closer than this (input levels) are the same point.
pub const MIN_GAP: f32 = 2.0;

/// Adds a point at `v` (kept sorted). A point already at that input is selected instead; a
/// full curve takes no more points.
pub fn insert(pts: &mut Vec<[f32; 2]>, v: [f32; 2]) -> Option<usize> {
    if !v.iter().all(|x| x.is_finite()) {
        return None;
    }
    let v = [v[0].clamp(0.0, 255.0), v[1].clamp(0.0, 255.0)];
    if let Some(i) = pts.iter().position(|q| (q[0] - v[0]).abs() < MIN_GAP) {
        return Some(i);
    }
    if pts.len() >= MAX_POINTS {
        return None;
    }
    let i = pts.iter().position(|q| q[0] > v[0]).unwrap_or(pts.len());
    pts.insert(i, v);
    Some(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_keeps_order_dedupes_and_respects_the_limit() {
        let mut p = vec![[0.0, 0.0], [255.0, 255.0]];
        assert_eq!(insert(&mut p, [128.0, 150.0]), Some(1));
        assert_eq!(insert(&mut p, [64.0, 40.0]), Some(1));
        assert_eq!(p, vec![[0.0, 0.0], [64.0, 40.0], [128.0, 150.0], [255.0, 255.0]]);
        // A click on an existing input selects that point instead of stacking a second one.
        assert_eq!(insert(&mut p, [129.0, 10.0]), Some(2));
        assert_eq!(p.len(), 4);
        let mut full: Vec<[f32; 2]> = (0..MAX_POINTS).map(|i| [i as f32 * 10.0, 0.0]).collect();
        assert_eq!(insert(&mut full, [255.0, 0.0]), None);
        assert_eq!(insert(&mut p, [-50.0, 900.0]), Some(0), "clamped onto the black point");
        assert_eq!(insert(&mut p, [f32::NAN, 128.0]), None);
    }
}
