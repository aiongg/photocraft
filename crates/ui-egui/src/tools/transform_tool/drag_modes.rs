use super::pointer::{Hit, box_rules};
use super::{PhotocraftApp, TransformSession};
use craft::ui::canvas::quad::BoxDrag;

/// Only a freely distorted corner can snap its proposed position. Perspective moves paired
/// corners on one axis; Skew, Warp and Content-Aware Scale have their own constrained geometry.
pub(crate) fn corner_distorts(app: &PhotocraftApp, index: usize, mods: egui::Modifiers) -> bool {
    let Some(t) = app.ui.transform.as_ref() else { return false };
    if t.quad.get(index).is_none() || t.warp.is_some() || t.content_aware.is_some() {
        return false;
    }
    box_rules(app, t).drag_kind(Hit::Corner(index).into(), mods) == Some(BoxDrag::Distort)
}

/// The pointer a handle shows for a drag that distorts or skews the box rather than scaling or
/// rotating it (#5405).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HandleIndicator {
    Distort,
    Skew,
}

/// What dragging handle `h` with `mods` does, by the library's rules for the drag itself: ⌘ on a
/// corner distorts (⌘⌥⇧ adds perspective) and on an edge skews, Distort and Perspective modes
/// distort from the corners, Skew mode skews from the edges. Rotate mode turns the box whatever
/// the keys, a Skew-mode corner slides along an edge, editable type refuses Distort and
/// Content-Aware Scale only scales: `None`.
pub(super) fn handle_indicator(app: &PhotocraftApp, t: &TransformSession, h: Hit, mods: egui::Modifiers) -> Option<HandleIndicator> {
    if t.warp.is_some() || t.content_aware.is_some() {
        return None;
    }
    match (box_rules(app, t).drag_kind(h.into(), mods)?, h) {
        (BoxDrag::Distort | BoxDrag::Perspective, Hit::Corner(_)) => Some(HandleIndicator::Distort),
        (BoxDrag::Skew, Hit::Edge(_)) => Some(HandleIndicator::Skew),
        _ => None,
    }
}
