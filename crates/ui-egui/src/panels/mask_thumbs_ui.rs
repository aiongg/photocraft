//! Layers panel row: the mask thumbnails after the layer thumbnail (#153).
//!
//! Like Photoshop, each mask (the pixel mask, then the vector mask) gets a link chain and a
//! thumbnail: the chain shows whether the mask moves with the layer and toggles it on click
//! (`layer.layerMask.linked` / `layer.vectorMask.linked`); a disabled mask is crossed out with
//! a red X and ⇧-clicking a mask thumbnail toggles it (`…enabled`). Vector masks render as a
//! white (revealed) / grey (hidden) thumbnail, cached like the other row thumbnails.
//!
//! #196: ⌥-click a layer-mask thumbnail to view the mask (⇧⌥: as an overlay), ⌘-click either
//! mask to load it as a selection, and click the vector mask to target it (brackets).

use egui::{Color32, Pos2, Rect};
use photocraft_doc::{Document, Layer, LayerId, VectorMask};
use serde_json::{Value, json};

use crate::PhotocraftApp;

/// Thumbnail cache kinds (the second half of `PhotocraftApp::thumbs` keys).
pub const THUMB_LAYER: u8 = 0;
pub const THUMB_MASK: u8 = 1;
pub const THUMB_VECTOR: u8 = 2;
pub const THUMB_SMART_MASK: u8 = 3;

/// Which of a layer's masks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskKind {
    Pixel,
    Vector,
}

impl MaskKind {
    /// The engine command prefix for this mask.
    pub(crate) fn command(self) -> &'static str {
        match self {
            MaskKind::Pixel => "layer.layerMask",
            MaskKind::Vector => "layer.vectorMask",
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            MaskKind::Pixel => "layer mask",
            MaskKind::Vector => "vector mask",
        }
    }
}

/// `(kind, enabled, linked)` for each of `l`'s masks, in row order.
pub fn masks(l: &Layer) -> Vec<(MaskKind, bool, bool)> {
    let mut v = Vec::with_capacity(2);
    if let Some(m) = &l.mask {
        v.push((MaskKind::Pixel, m.enabled, m.linked));
    }
    if let Some(m) = &l.vector_mask {
        v.push((MaskKind::Vector, m.enabled, m.linked));
    }
    v
}

/// Where a row's mask thumbnails and chains went.
#[derive(Clone, Debug, Default)]
pub struct MaskRects {
    pub thumbs: Vec<(MaskKind, Rect)>,
    pub chains: Vec<(MaskKind, Rect)>,
    /// A chain was clicked this frame (the row shouldn't treat the click as a selection).
    pub clicked: bool,
}

impl MaskRects {
    pub fn thumb(&self, kind: MaskKind) -> Option<Rect> {
        self.thumbs.iter().find(|(k, _)| *k == kind).map(|(_, r)| *r)
    }
    /// The mask thumbnail under `p`, if any.
    pub fn hit(&self, p: Pos2) -> Option<MaskKind> {
        self.thumbs.iter().find(|(_, r)| r.expand(2.0).contains(p)).map(|(k, _)| *k)
    }
}

type Rects = (Vec<(MaskKind, Rect)>, Vec<(MaskKind, Rect)>);

/// Memory key of the per-layer record of where the mask thumbnails and chains were drawn.
fn rects_id() -> egui::Id {
    egui::Id::unique("mask-thumb-rects")
}

/// Record where the row of layer `id` drew its mask thumbnails and chains.
pub fn record(ctx: &egui::Context, id: u64, thumbs: Vec<(MaskKind, Rect)>, chains: Vec<(MaskKind, Rect)>) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<std::collections::HashMap<u64, Rects>>(rects_id()).insert(id, (thumbs, chains)));
}

/// The mask thumbnails and chains last drawn for layer `id` (for tests and automation).
pub fn recorded(ctx: &egui::Context, id: u64) -> Option<Rects> {
    ctx.data(|d| d.get_temp::<std::collections::HashMap<u64, Rects>>(rects_id())).and_then(|m| m.get(&id).cloned())
}

/// Drop what was recorded for layer `id`, so a row drawn without mask thumbnails (thumbnail size
/// None) can't be hit through rects from an earlier frame.
pub fn forget(ctx: &egui::Context, id: u64) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<std::collections::HashMap<u64, Rects>>(rects_id()).remove(&id));
}

/// The command a click on mask thumbnail `kind` issues with modifiers `m`, if it is one that
/// replaces the normal click: ⇧ toggles the mask on/off; ⌥ views a layer mask alone in
/// grayscale and ⇧⌥ as a rubylith overlay (again: back to the composite), like Photoshop.
pub fn click_command(l: &Layer, kind: MaskKind, m: egui::Modifiers) -> Option<(String, Value)> {
    if m.command {
        return None;
    }
    if m.alt {
        let mode = if m.shift { "toggleOverlay" } else { "toggleGray" };
        return (kind == MaskKind::Pixel && l.mask.is_some()).then(|| (photocraft_engine::mask_view_cmds::ID.into(), json!({"layer": l.id.0, "mode": mode})));
    }
    if !m.shift {
        return None;
    }
    let enabled = masks(l).into_iter().find(|(k, _, _)| *k == kind)?.1;
    Some((format!("{}.enabled", kind.command()), json!({"layer": l.id.0, "enabled": !enabled})))
}

/// The `select.loadSelection` params for a ⌘-click on mask thumbnail `kind` (⌘⇧ add, ⌘⌥
/// subtract, ⌘⇧⌥ intersect).
pub fn load_params(l: &Layer, kind: MaskKind, m: egui::Modifiers) -> Value {
    let channel = match kind {
        MaskKind::Pixel => "mask",
        MaskKind::Vector => "vectorMask",
    };
    json!({"channel": channel, "layer": l.id.0, "operation": crate::channels_panel::load_operation(m)})
}

/// What double-clicking a mask thumbnail does: pixel mask launches Select and Mask if preference is enabled.
pub fn double_click_command(app: &PhotocraftApp, kind: MaskKind) -> Option<(&'static str, Value)> {
    if kind == MaskKind::Pixel
        && app.session.prefs().tools.double_click_layer_mask_launches_select_and_mask
        && crate::menus::is_enabled(app, "select.selectAndMask")
    {
        Some(("select.selectAndMask", Value::Null))
    } else {
        None
    }
}

/// The active layer of `st` has a vector mask (shape layers' paths are content, not masks).
pub fn has_vector_mask(st: &photocraft_engine::DocState) -> bool {
    st.active_layer.and_then(|id| st.doc.layer(id)).is_some_and(|l| l.vector_mask.is_some() && !matches!(l.content, photocraft_doc::LayerContent::Shape(_)))
}

/// Cheap identity of a vector mask's geometry and state.
pub fn vector_fingerprint(m: &VectorMask) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |v: u64| h = (h ^ v).wrapping_mul(0x100_0000_01b3);
    mix(u64::from(m.path.inverted) | u64::from(m.path.fill_rule == photocraft_doc::FillRule::EvenOdd) << 1);
    mix(u64::from(m.density.to_bits()));
    for s in &m.path.subpaths {
        mix(u64::from(s.closed) | (s.op as u64) << 1 | (s.knots.len() as u64) << 8);
        for k in &s.knots {
            for p in [k.anchor, k.in_ctrl, k.out_ctrl] {
                mix(p.x.to_bits());
                mix(p.y.to_bits());
            }
        }
    }
    h
}

/// A `side`-pixel square thumbnail of vector mask `m`: white where it reveals, grey where it
/// hides, letterboxed to the document's aspect like the other row thumbnails. Disabled masks
/// still show their shape (the row crosses them out).
pub fn vector_thumb_image(doc: &Document, m: &VectorMask, side: usize) -> egui::ColorImage {
    let (w, h) = (f64::from(doc.size.width.max(1)), f64::from(doc.size.height.max(1)));
    let s = side as f64 / w.max(h);
    let (ox, oy) = ((side as f64 - w * s) / 2.0, (side as f64 - h * s) / 2.0);
    let shown = VectorMask { path: m.path.transform(&photocraft_geom::Affine { m: [s, 0.0, 0.0, s, ox, oy] }), enabled: true, ..m.clone() };
    let n = side as i32;
    let values = photocraft_vector::vector_mask_values(&shown, photocraft_geom::Rect::new(0, 0, n, n));
    let (x0, y0, x1, y1) = (ox.floor() as usize, oy.floor() as usize, (ox + w * s).ceil() as usize, (oy + h * s).ceil() as usize);
    let mut px = vec![Color32::TRANSPARENT; side * side];
    for (i, p) in px.iter_mut().enumerate() {
        let (tx, ty) = (i % side, i / side);
        if tx < x0 || ty < y0 || tx >= x1 || ty >= y1 {
            continue;
        }
        let v = values.get(i).copied().unwrap_or(1.0).clamp(0.0, 1.0);
        *p = Color32::from_gray((128.0 + 127.0 * v + 0.5) as u8);
    }
    egui::ColorImage::new([side, side], px)
}

impl PhotocraftApp {
    /// Cached thumbnail of `l`'s vector mask (see [`vector_thumb_image`]).
    pub fn vector_mask_thumb(&mut self, ctx: &egui::Context, doc: &Document, id: LayerId, m: &VectorMask) -> egui::TextureId {
        let rev = vector_fingerprint(m) ^ (u64::from(doc.size.width) << 40) ^ (u64::from(doc.size.height) << 20);
        let key = (id, THUMB_VECTOR);
        if let Some((r, tex)) = self.thumbs.get(&key)
            && *r == rev
        {
            return tex.id();
        }
        self.store_thumb(ctx, key, rev, vector_thumb_image(doc, m, 64))
    }
}

#[cfg(test)]
mod tests;
