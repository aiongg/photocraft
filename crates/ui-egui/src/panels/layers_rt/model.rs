//! The rt `Doc` rebuilt from PhotoCraft's Layers rows, and the thumbnails it points at.

use std::collections::HashMap;

use craft_rt_layers::{Blend, Doc, Kind, Layer};
use craft_ui_rt::{ImageKey, ImageStore, RgbaImage, SizeU, hash_key};
use photocraft_doc::{Document, LayerContent, LayerId};

use crate::PhotocraftApp;
use crate::app::thumbs::{layer_thumb_image, layer_thumb_rev};
use crate::layer_filter_ui::Filter;
use crate::panels::mask_thumbs_ui::THUMB_LAYER;
use crate::state::LayerThumbnailSize;
use crate::theme::ThemeKind;

/// Everything the rt `Doc` is built from. The model is rebuilt only when this moves, and read at the panel's own turn.
#[derive(Clone, PartialEq)]
pub(super) struct ModelKey {
    pub doc: u64,
    pub revision: u64,
    pub theme: ThemeKind,
    pub thumbnail: LayerThumbnailSize,
    pub filter: Filter,
    pub isolation: Vec<LayerId>,
    /// A Type Mask session's working text is no row (`layer_tree_ui::panel_rows`).
    pub type_mask: Option<u64>,
    /// Clip Thumbnails to Layer Bounds changes the thumbnails' pixels without a revision.
    pub clip: bool,
}

pub(super) fn image_key(layer: LayerId) -> ImageKey {
    ImageKey(hash_key(&("photocraft-layer-thumb", layer.0, THUMB_LAYER)))
}

/// The thumbnail pictures this binding put in the rt image store: each layer's last picture revision.
#[derive(Default)]
pub(super) struct Thumbs {
    pushed: HashMap<u64, u64>,
}

impl Thumbs {
    /// The layer's picture is in `images` at its current revision.
    fn ensure(&mut self, images: &mut ImageStore, doc: &Document, layer: &photocraft_doc::Layer, clip: bool) {
        let rev = layer_thumb_rev(doc, layer, clip);
        if self.pushed.get(&layer.id.0) == Some(&rev) && images.get(image_key(layer.id)).is_some() {
            return;
        }
        let (_, img) = layer_thumb_image(doc, layer, clip);
        let [w, h] = img.size;
        let pixels = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
        images.insert(image_key(layer.id), std::sync::Arc::new(RgbaImage { size: SizeU::new(w as u32, h as u32), pixels }));
        self.pushed.insert(layer.id.0, rev);
    }

    /// Drops the pictures of layers that are no longer rows.
    fn retain(&mut self, images: &mut ImageStore, live: &std::collections::HashSet<u64>) {
        self.pushed.retain(|id, _| {
            let keep = live.contains(id);
            if !keep {
                images.remove(image_key(LayerId(*id)));
            }
            keep
        });
    }
}

fn kind_of(content: &LayerContent) -> Kind {
    match content {
        LayerContent::Text(_) => Kind::Type,
        LayerContent::Raster(_) | LayerContent::Smart(_) | LayerContent::Shape(_) => Kind::Pixel,
        LayerContent::Group(_) | LayerContent::Adjustment(_) | LayerContent::Fill(_) => Kind::Fill,
    }
}

/// The rt document of the rows `layers()` lists: top of the stack first, closed groups' contents hidden, the filter and
/// Select > Isolate Layers applied, as the egui rows do. Group, mask and effect rows are not drawn by the rt panel
/// (stage a); groups and fills show without a picture.
#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    app: &PhotocraftApp,
    doc: &Document,
    selection: &[LayerId],
    active: Option<LayerId>,
    isolated: &[LayerId],
    images: &mut ImageStore,
    thumbs: &mut Thumbs,
    with_thumbs: bool,
) -> Doc {
    let filter = Filter::with_selection(app, doc, selection);
    let clip = app.session.prefs().interface.clip_thumbnails_to_layer_bounds;
    let mut rows = Vec::new();
    let mut live = std::collections::HashSet::new();
    for (_, l) in crate::layer_tree_ui::panel_rows(app, doc) {
        if !photocraft_engine::select_extra_cmds::isolation_shows(doc, isolated, l.id) || !filter.shows(l) {
            continue;
        }
        let kind = kind_of(&l.content);
        let thumb = (with_thumbs && kind == Kind::Pixel).then(|| {
            thumbs.ensure(images, doc, l, clip);
            live.insert(l.id.0);
            image_key(l.id)
        });
        rows.push(Layer {
            id: l.id.0,
            name: l.name.clone(),
            kind,
            visible: l.visible,
            locked: l.locks.any(),
            blend: if l.blend == photocraft_color::BlendMode::Multiply { Blend::Multiply } else { Blend::Normal },
            thumb,
            // The picture is a letterboxed square already: the whole of it shows.
            thumb_uv: Doc::FULL_UV,
        });
    }
    thumbs.retain(images, &live);
    let shown: std::collections::HashSet<u64> = rows.iter().map(|r| r.id).collect();
    let selected = selection.iter().map(|id| id.0).filter(|id| shown.contains(id));
    Doc::from_rows(rows, selected, active.map(|a| a.0).filter(|a| shown.contains(a)))
}
