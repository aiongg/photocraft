//! The rules the tools' cursors and the Alt stand-in share: which tools show a brush-tip centre,
//! which let ⌥ sample or load paint, and the Pencil's tip shape.

use super::{PhotocraftApp, Tool};

/// Whether an aliased (Pencil) dab of `size` pixels is round rather than its whole square. The dab
/// fills the pixels whose centres lie within its radius of the grid centre
/// (`paint::grid_center`), so up to 3 px the corner pixels are in and the dab is a square; from 4 px
/// on they fall outside and it is a disc (#2662).
pub(crate) fn pencil_tip_is_round(size: f32) -> bool {
    let n = if size.is_finite() { size.round().clamp(1.0, 100_000.0) } else { 1.0 };
    let h = n / 2.0;
    (h - 0.5) * std::f32::consts::SQRT_2 > h
}

/// Whether a brush-tip circle draws its centre mark.
/// Clone Stamp and Healing Brush stay an empty circle until Option is held, the source-point
/// cursor. Other brushes follow Preferences › Cursors › Show Crosshair in Brush Tip, as in
/// Photoshop; the Background Eraser always shows its sampling hotspot, and the Remove tool its
/// centre + (#12550).
pub(crate) fn brush_tip_centre(tool: Tool, option: bool, show_crosshair: bool) -> bool {
    if tool == Tool::QuickSelection {
        return false;
    }
    if matches!(tool, Tool::CloneStamp | Tool::Healing) {
        return option || show_crosshair;
    }
    show_crosshair || matches!(tool, Tool::BackgroundEraser | Tool::Remove)
}

/// Tools on which holding ⌥ (Alt) switches to the Eyedropper: a click or drag sets the
/// foreground colour, as with the Eyedropper itself (#417, the Background Eraser #11906, the Color
/// Replacement tool #11914).
pub(crate) fn alt_samples(tool: Tool, mods: craft::keymap::Mods) -> bool {
    // Control+Alt is the brush-resize drag (`brush_resize`), not sampling.
    mods.alt && !mods.ctrl && matches!(tool, Tool::Brush | Tool::Pencil | Tool::Gradient | Tool::PaintBucket | Tool::BackgroundEraser | Tool::ColorReplacement)
}

/// Whether ⌥ loads the Mixer Brush with paint from the canvas instead of painting.
pub(crate) fn alt_loads_mixer(tool: Tool, mods: craft::keymap::Mods) -> bool {
    mods.alt && !mods.ctrl && tool == Tool::MixerBrush
}

/// Whether the Mixer Brush's target layer has Lock All on, so its cursor shows the not-allowed mark.
pub(crate) fn mixer_target_locked(app: &PhotocraftApp) -> bool {
    app.session.active().and_then(|st| st.active_layer.and_then(|id| st.doc.layer(id))).is_some_and(|layer| layer.locks.all)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every row is (tool, option held, preference on) → centre mark.
    #[test]
    fn brush_tip_centre_rule() {
        let rows = [
            (Tool::QuickSelection, true, true, false),
            (Tool::CloneStamp, false, false, false),
            (Tool::CloneStamp, true, false, true),
            (Tool::CloneStamp, false, true, true),
            (Tool::Healing, false, false, false),
            (Tool::Healing, true, false, true),
            (Tool::PatternStamp, true, false, false),
            (Tool::PatternStamp, false, true, true),
            (Tool::BackgroundEraser, false, false, true),
            (Tool::Remove, false, false, true),
            (Tool::Brush, false, false, false),
            (Tool::Brush, false, true, true),
            (Tool::Eraser, true, false, false),
        ];
        for (tool, option, preference, centre) in rows {
            assert_eq!(brush_tip_centre(tool, option, preference), centre, "{tool:?} option={option} preference={preference}");
        }
    }
}
