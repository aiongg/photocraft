use photocraft_text::warp::Warp;

use super::{Affine, Arc, Document, LayerContent, LayerId, PhotocraftApp, Point, PoisonError, TextLayer, TextLayout, text_layer};

/// Layout of a type layer (cached per document revision) and its text → document transform.
pub fn layout(app: &mut PhotocraftApp, id: LayerId) -> Option<(Arc<TextLayout>, Affine, String)> {
    let dragged = crate::type_transform::current_transform(app, id);
    // Borrows the document in place (no clone): the canvas hover hit-tests type every frame.
    let st = app.session.active()?;
    let t = text_layer(&st.doc, id)?;
    let transform = dragged.unwrap_or(t.transform);
    let key = (st.doc.id.0, st.revision, id.0);
    if let Some((k, l)) = &app.type_layout
        && *k == key
    {
        return Some((l.clone(), transform, t.text.clone()));
    }
    let l = Arc::new(photocraft_text::shared().lock().ok()?.layout(t, st.doc.resolution_dpi));
    app.type_layout = Some((key, l.clone()));
    Some((l, transform, t.text.clone()))
}

pub(super) fn to_text(aff: &Affine, x: f64, y: f64) -> (f32, f32) {
    let p = aff.inverse().unwrap_or(Affine::IDENTITY).apply(Point::new(x, y));
    (p.x as f32, p.y as f32)
}

/// Topmost shown type layer (it and every group around it visible, #4320) whose laid-out text
/// contains the document point. The press and the canvas hover (`hover_path_target`) share it, so
/// the pointer never promises a path click that the press would turn into editing this layer.
pub(crate) fn hit_layer(app: &mut PhotocraftApp, x: f64, y: f64) -> Option<LayerId> {
    let slop = 6.0 / app.point_zoom().max(0.01);
    // A Type Mask session's text is a hidden layer; it is still the text being edited.
    let editing = app.ui.text_edit.as_ref().filter(|ed| ed.mask).map(|ed| LayerId(ed.layer));
    let mut ids: Vec<LayerId> = {
        let doc = &app.session.active()?.doc;
        doc.walk()
            .into_iter()
            .filter(|(path, _, l)| matches!(l.content, LayerContent::Text(_)) && (editing == Some(l.id) || doc.shown_at(path)))
            .map(|(_, _, l)| l.id)
            .collect()
    };
    ids.reverse(); // walk() is bottom-up; hit the topmost first
    ids.into_iter().find(|id| {
        // The pixels shown count too: a PSD's type keeps Photoshop's rendering until edited,
        // which can sit off our layout when fonts are substituted (#123).
        let shown = app.session.active().and_then(|st| text_layer(&st.doc, *id)).and_then(|t| t.cache.as_ref()).map(|c| c.content_bounds()).is_some_and(|r| {
            let s = f64::from(slop);
            !r.is_empty() && x >= f64::from(r.x0) - s && x <= f64::from(r.x1) + s && y >= f64::from(r.y0) - s && y <= f64::from(r.y1) + s
        });
        // The hover calls this every frame: skip the layout when the pixels already answer.
        if shown {
            return true;
        }
        let Some((l, aff, _)) = layout(app, *id) else { return false };
        let (tx, ty) = to_text(&aff, x, y);
        photocraft_text::text_point_inside(&l, tx, ty, slop)
    })
}

/// True when the layer's pixels are what our engine draws for it. A PSD's type layer keeps
/// Photoshop's pixels until it is edited, and with substituted fonts or a different line layout
/// they sit somewhere else than the glyphs the caret and selection are placed on.
pub(crate) fn shows_own_layout(doc: &Document, t: &TextLayer) -> bool {
    let Some(cache) = &t.cache else { return false };
    let mut eng = photocraft_text::shared().lock().unwrap_or_else(PoisonError::into_inner);
    let ours = eng.render(t, doc.resolution_dpi, doc.pixel_format()).1.surface.content_bounds();
    let have = cache.content_bounds();
    [(ours.x0, have.x0), (ours.y0, have.y0), (ours.x1, have.x1), (ours.y1, have.y1)].iter().all(|(a, b)| (a - b).abs() <= 1)
}

/// Whether layer `id` runs its text vertically.
pub(super) fn is_vertical(app: &PhotocraftApp, id: LayerId) -> bool {
    app.session.active().and_then(|s| text_layer(&s.doc, id)).is_some_and(|t| t.orientation == photocraft_doc::text::Orientation::Vertical)
}

/// The warp of a layer set on a path (type on a path), `None` for any other text. The path carries the line, so
/// carets, selections and hits go through it.
pub(super) fn path_warp(app: &PhotocraftApp, id: LayerId, l: &TextLayout) -> Option<Warp> {
    let t = text_layer(&app.session.active()?.doc, id)?;
    let text_start = l.line_bounds().map_or(0.0, |bounds| f64::from(bounds[0]));
    Warp::on_path_oriented(&t.shape, text_start, l.vertical)
}

/// A line-space point (along the line, across it) in document space: the line turns to text space, bends along the
/// path when there is one, and the layer's transform places it.
pub(super) fn doc_point(l: &TextLayout, aff: &Affine, warp: Option<&Warp>, x: f32, y: f32) -> Point {
    let (tx, ty) = l.to_text(x, y);
    let (tx, ty) = match warp {
        Some(w) => {
            let (wx, wy) = w.apply(f64::from(tx), f64::from(ty));
            (wx as f32, wy as f32)
        }
        None => (tx, ty),
    };
    aff.apply(Point::new(f64::from(tx), f64::from(ty)))
}

/// The character offset of the caret position nearest document point (`x`, `y`) in layer `id`, and the laid-out
/// line the point is on.
pub(super) fn hit_at(app: &mut PhotocraftApp, id: LayerId, x: f64, y: f64) -> (usize, usize) {
    let Some((l, aff, text)) = layout(app, id) else { return (0, 0) };
    let (tx, ty) = to_text(&aff, x, y);
    // Type on a path: the point's place along the path is the position in the (single) line.
    let text_end = l.line_bounds().map_or(0.0, |bounds| f64::from(bounds[2]));
    let (tx, ty) = path_warp(app, id, &l)
        .and_then(|warp| warp.path_text_x_near_text(f64::from(tx), f64::from(ty), text_end))
        .map_or((tx, ty), |offset| l.to_text(offset as f32, -1.0));
    photocraft_text::hit_char(&l, &text, tx, ty)
}

/// The character offset of the caret position nearest document point (`x`, `y`) in layer `id`.
pub(crate) fn offset_at(app: &mut PhotocraftApp, id: LayerId, x: f64, y: f64) -> usize {
    hit_at(app, id, x, y).0
}
