//! [`PhotoComposite`]: a document's canvas picture as the framework's raster display reads it, the composite with the
//! live preview, the colour management and the channel view folded in.

use std::collections::VecDeque;
use std::sync::Arc;

use craft::behavior::PixelSize;
use craft::ui::canvas::{PixelRect, RasterSource};
use craft::ui::primitives::Revision;
use egui::ColorImage;
use photocraft_engine::display_color::CanvasDisplay;

use super::channel_view::Overlay;
use super::display::{cache_key, canvas_display, display_doc, display_image};
use super::filter_preview::filter_channel_preview;
use super::textures::{damage_since, effect_reach, preview_bounds};
use super::{DRect, Document, PhotocraftApp};

/// How many of the revisions a source handed out it can still tell damage since: a display's whole-document and
/// detail textures may each lag a few passes behind.
const LOG_LEN: usize = 8;

#[cfg(test)]
mod tests;

/// What a source's picture showed at one [`Revision`]: the state [`damage_since`] compares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shown {
    /// The document's revision.
    revision: u64,
    /// The preview key with the display key folded in, as the CPU canvas caches keep it.
    key: u64,
    /// The channel view's key; 0 for the plain composite.
    overlay: u64,
    /// Where the picture lies in the document: the document's bounds, or a size-changing preview's own.
    bounds: DRect,
    /// The picture is a preview reduced for a zoomed-out view, in its own pixels.
    reduced: bool,
}

/// The revisions one document's source handed out on one display, newest last, so a display asking for the damage
/// since the revision its texture shows gets an answer. Kept per `(document, display)` in the app.
#[derive(Default)]
pub(crate) struct SourceLog {
    entries: VecDeque<(Revision, Shown)>,
}

impl SourceLog {
    fn record(&mut self, revision: Revision, shown: Shown) {
        if self.entries.back().is_some_and(|(r, _)| *r == revision) {
            return;
        }
        self.entries.retain(|(r, _)| *r != revision);
        if self.entries.len() == LOG_LEN {
            self.entries.pop_front();
        }
        self.entries.push_back((revision, shown));
    }

    fn find(&self, revision: Revision) -> Option<Shown> {
        self.entries.iter().rev().find(|(r, _)| *r == revision).map(|(_, s)| *s)
    }

    /// The pictures that showed a preview `was_preview` tells (by its key with display key `display_key` folded in)
    /// count as the committed document from now on, so the commit's damage refreshes only what it changed
    /// ([`super::textures::shown_as_document`]).
    pub(crate) fn shown_as_document(&mut self, display_key: u64, was_preview: impl Fn(u64) -> bool) {
        for (_, shown) in &mut self.entries {
            if was_preview(shown.key ^ display_key) {
                shown.key = display_key;
            }
        }
    }
}

/// Document `idx`'s canvas picture on one display, for a `RasterDisplay`: the composite the CPU canvas shows (the
/// committed document, or the live adjustment, filter or tool preview), through the display's colour management, with
/// the Channels panel's view over it. Built once per pass; damage comes from the same records the CPU canvas cache
/// reads ([`damage_since`]).
pub(crate) struct PhotoComposite<'a> {
    app: &'a PhotocraftApp,
    idx: usize,
    doc: Arc<Document>,
    display: Option<Arc<CanvasDisplay>>,
    overlay: Option<Overlay>,
    /// The document's revision and the raw preview key.
    now: (u64, u64),
    display_key: u64,
    /// The document's damage span start and its last damage, grown by its layer effects' reach.
    last_damage: (u64, Option<DRect>),
    shown: Shown,
    revision: Revision,
    log: Option<&'a SourceLog>,
}

impl<'a> PhotoComposite<'a> {
    /// Document `idx`'s picture on `output` (`None`: the main window's display); `None` when no such document is open.
    pub(crate) fn new(app: &'a mut PhotocraftApp, ctx: &egui::Context, idx: usize, output: Option<u32>) -> Option<Self> {
        let (revision, last_damage, id, committed) = {
            let st = app.session.documents().get(idx)?;
            let reach = |r: DRect| if r.is_empty() { r } else { r.inflate(effect_reach(&st.doc.layers)) };
            (st.revision, (st.damage_from(), st.last_damage.map(reach)), st.doc.id, st.doc.bounds())
        };
        let (mut doc, mut preview_key) = display_doc(app, idx);
        // A filter dialog's preview: reduced for a zoomed-out view (a proxy, factor `k`) it covers the whole document
        // at its own size; one whose dimensions change otherwise (Image › Rotation) covers its own area, centred on
        // the document's, as the GPU canvas draws it.
        let mut bounds = committed;
        let mut reduced = false;
        if let Some((preview, key)) = filter_channel_preview(app, idx, ctx) {
            let k = app.filter_preview.as_ref().map_or(1, |p| p.key.k.max(1));
            bounds = preview_bounds(committed.size(), preview.size, k);
            (doc, preview_key, reduced) = (preview, key, k > 1);
        }
        let reduced = reduced || bounds != committed;
        // An adjustment dialog's preview of a large document is taken by the GPU path as a proxy; read here it would
        // cost a full-size composite per change, so the document shows instead.
        if crate::adjust_preview::shown_key(app) == Some(preview_key)
            && let Some(st) = app.session.documents().get(idx)
            && crate::proxy::preview_factor(&st.doc, crate::proxy::reduced_previews(app)) > 1
        {
            (doc, preview_key) = (st.doc.clone(), 0);
        }
        let (display, display_key) = canvas_display(app, &doc, output);
        let overlay = Overlay::of(app, idx, &doc, output);
        let shown = Shown { revision, key: preview_key ^ display_key, overlay: overlay.as_ref().map_or(0, |o| o.key), bounds, reduced };
        let picture = Revision::of(&(id.0, shown.revision, shown.key, shown.overlay, (bounds.x0, bounds.y0, bounds.x1, bounds.y1), reduced));
        let log_key = cache_key(id, output);
        app.raster_logs.entry(log_key).or_default().record(picture, shown);
        let app: &'a PhotocraftApp = app;
        let log = app.raster_logs.get(&log_key);
        Some(PhotoComposite { app, idx, doc, display, overlay, now: (revision, preview_key), display_key, last_damage, shown, revision: picture, log })
    }

    /// Where the picture lies in the document, in document pixels: the document's bounds, or a filter preview's own
    /// area when it is reduced or changes the canvas size (its pixels are then its own, `size` of them over this).
    pub(crate) fn bounds(&self) -> DRect {
        self.shown.bounds
    }
}

impl RasterSource for PhotoComposite<'_> {
    fn size(&self) -> PixelSize {
        PixelSize::new(self.doc.size.width, self.doc.size.height)
    }

    fn revision(&self) -> Revision {
        self.revision
    }

    fn damage(&self, since: Revision) -> Option<PixelRect> {
        if since == self.revision {
            return Some(PixelRect::default());
        }
        let seen = self.log?.find(since)?;
        // A reduced or moved picture is in other pixels than the document's damage, and a new channel view draws
        // everything differently.
        if seen.reduced || self.shown.reduced || seen.overlay != self.shown.overlay || seen.bounds != self.shown.bounds {
            return None;
        }
        let r = damage_since(self.app, self.idx, (seen.revision, seen.key), self.now, self.display_key, self.last_damage)?;
        let r = r.intersect(&self.doc.bounds());
        Some(if r.is_empty() { PixelRect::default() } else { PixelRect::new(r.x0, r.y0, r.x1, r.y1) })
    }

    fn read(&self, region: PixelRect, factor: u32, out: &mut ColorImage) {
        let factor = factor.max(1);
        let f = i32::try_from(factor).unwrap_or(i32::MAX);
        let b = self.doc.bounds();
        // Whole texels of `region`'s grid that lie past the document's start are left as they are.
        let skip = |start: i32, lo: i32| if start >= lo { 0 } else { (lo - start + f - 1) / f };
        let (tx, ty) = (skip(region.x0, b.x0), skip(region.y0, b.y0));
        let area = DRect::new(region.x0 + tx * f, region.y0 + ty * f, region.x1.min(b.x1), region.y1.min(b.y1));
        if area.is_empty() {
            return;
        }
        let buf = photocraft_compose::render_reduced_by(&self.doc, area, factor);
        let mut img = display_image(self.display.as_deref(), &buf);
        if let Some(px) = self.overlay.as_ref().and_then(|o| o.render(&self.doc, area, factor))
            && px.len() == img.pixels.len()
        {
            for (under, over) in img.pixels.iter_mut().zip(px) {
                *under = under.blend(over);
            }
        }
        let (w, h) = (img.size[0], img.size[1]);
        let (tx, ty) = (tx as usize, ty as usize);
        let [out_w, out_h] = out.size;
        for row in 0..h.min(out_h.saturating_sub(ty)) {
            let n = w.min(out_w.saturating_sub(tx));
            let at = (ty + row) * out_w + tx;
            out.pixels[at..at + n].copy_from_slice(&img.pixels[row * w..row * w + n]);
        }
    }
}
