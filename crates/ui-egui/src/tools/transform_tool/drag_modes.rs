use super::begin::moves_kind;
use super::pointer::{Hit, box_rules};
use super::{LayerContent, PhotocraftApp, TransformMode, TransformSession};

/// The same effective modifiers drive the geometry and decide whether Ctrl is a distortion
/// gesture rather than the ordinary snap override. Keep type and Content-Aware Scale refusals.
pub(super) fn drag_mods(app: &PhotocraftApp, t: &TransformSession, hit: Hit, mods: egui::Modifiers) -> (TransformMode, egui::Modifiers) {
    let (mode, mods) = if distort_refused(app, t) { scale_not_distort(t.mode, hit, mods) } else { (t.mode, mods) };
    let legacy = app.session.prefs().general.use_legacy_free_transform;
    (mode, legacy_mods(legacy, hit, unlinked_mods(app.ui.transform_unlinked, legacy, hit, mods)))
}

/// Only a freely distorted corner can snap its proposed position. Perspective moves paired
/// corners on one axis; Skew, Warp and Content-Aware Scale have their own constrained geometry.
pub(crate) fn corner_distorts(app: &PhotocraftApp, index: usize, mods: egui::Modifiers) -> bool {
    let Some(t) = app.ui.transform.as_ref() else { return false };
    if t.quad.get(index).is_none() || t.warp.is_some() || t.content_aware.is_some() || t.mode == TransformMode::Skew {
        return false;
    }
    let (mode, mods) = drag_mods(app, t, Hit::Corner(index), mods);
    let mods = mode_mods(mode, Hit::Corner(index), mods);
    mods.command && !(mods.alt && mods.shift)
}

/// The pointer a handle shows for a drag that distorts or skews the box rather than scaling or
/// rotating it (#5405).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HandleIndicator {
    Distort,
    Skew,
}

/// What dragging handle `h` with `mods` does, worked out by the same rules as the drag itself
/// ([`drag_mods`], [`mode_mods`]): ⌘ on a corner distorts (⌘⌥⇧ adds
/// perspective) and on an edge skews, Distort and Perspective modes distort from the corners,
/// Skew mode skews from the edges. Rotate mode turns the box whatever the keys, a Skew-mode corner
/// slides along an edge, editable type refuses Distort and Content-Aware Scale only scales: `None`.
pub(super) fn handle_indicator(app: &PhotocraftApp, t: &TransformSession, h: Hit, mods: egui::Modifiers) -> Option<HandleIndicator> {
    if t.warp.is_some() || t.content_aware.is_some() || !box_rules(app, t).allows(h.into()) {
        return None;
    }
    let (mode, mods) = drag_mods(app, t, h, mods);
    match (mode, h) {
        (TransformMode::Rotate, _) | (TransformMode::Skew, Hit::Corner(_)) => None,
        (TransformMode::Skew, Hit::Edge(_)) => Some(HandleIndicator::Skew),
        (_, Hit::Corner(_)) if mode_mods(mode, h, mods).command => Some(HandleIndicator::Distort),
        (_, Hit::Edge(_)) if mods.command => Some(HandleIndicator::Skew),
        _ => None,
    }
}

/// Editable type can be scaled, rotated and skewed but not distorted (#2630): the rule that greys
/// Distort and Perspective in `enable_rules::disabled`, for the box's own layer. A box on a path,
/// on the selection outline or on a lone mask or channel moves no type.
fn distort_refused(app: &PhotocraftApp, t: &TransformSession) -> bool {
    t.path.is_none() && !t.selection && t.target.is_none() && moves_kind(app, t, |c| matches!(c, LayerContent::Text(_)))
}

/// Where Distort and Perspective are refused ([`distort_refused`]), ⌘-drag and ⌘⌥⇧-drag of a
/// corner scale like a plain corner drag (with ⌥ and ⇧ keeping their meaning), so the box never
/// takes a shape the commit would refuse (#3010).
fn scale_not_distort(mode: TransformMode, hit: Hit, mut mods: egui::Modifiers) -> (TransformMode, egui::Modifiers) {
    if matches!(hit, Hit::Corner(_)) {
        mods.command = false;
    }
    let mode = if matches!(mode, TransformMode::Distort | TransformMode::Perspective) { TransformMode::Free } else { mode };
    (mode, mods)
}

/// Preferences › General › Use Legacy Free Transform, Photoshop's scaling before CC 2019: corner
/// drags stretch freely and ⇧ keeps the proportions, edge drags stretch their one axis whatever
/// the keys. The default is proportional for both, with ⇧ freeing them.
pub(crate) fn legacy_mods(legacy: bool, hit: Hit, mut mods: egui::Modifiers) -> egui::Modifiers {
    if legacy && !mods.command {
        match hit {
            Hit::Corner(_) => mods.shift = !mods.shift,
            Hit::Edge(_) => mods.shift = true,
            _ => {}
        }
    }
    mods
}

/// The options bar's Maintain Aspect Ratio link turned off: corner and edge drags stretch freely
/// and ⇧ keeps the proportions, as in Photoshop CC 2019 and later (#4174). The legacy preference
/// already works that way, so it is left alone there.
pub(crate) fn unlinked_mods(unlinked: bool, legacy: bool, hit: Hit, mut mods: egui::Modifiers) -> egui::Modifiers {
    if unlinked && !legacy && !mods.command && matches!(hit, Hit::Corner(_) | Hit::Edge(_)) {
        mods.shift = !mods.shift;
    }
    mods
}

/// Distort / Perspective modes: a corner drag acts as if that gesture's keys were held (⌘-drag a
/// corner distorts, ⌘⌥⇧-drag a corner adds perspective). The library's Skew mode skews from the edges.
pub(super) fn mode_mods(mode: TransformMode, hit: Hit, mut mods: egui::Modifiers) -> egui::Modifiers {
    match (mode, hit) {
        (TransformMode::Distort, Hit::Corner(_)) => mods.command = true,
        (TransformMode::Perspective, Hit::Corner(_)) => {
            mods.command = true;
            mods.alt = true;
            mods.shift = true;
        }
        _ => {}
    }
    mods
}
