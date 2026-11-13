use super::{DRect, Document, PhotocraftApp, Pos2, Rect, Vec2, View, vec2};
use craft::behavior::CanvasView;
use craft::behavior::kurbo::Point;
use craft::ui::canvas::pixel_aligned;

/// Document ↔ screen mapping for a canvas rect and a view.
#[derive(Clone, Copy, Debug)]
pub struct ViewXform {
    pub rect: Rect,
    pub zoom: f32,
    pub center: [f32; 2],
    /// View › Flip Horizontal: the view is mirrored left-right (the document is not).
    pub flip: bool,
    /// Camera rotation in degrees around [`Self::center`]. Clockwise (Y-down) is positive; applied
    /// before flip.
    pub rotation: f32,
    /// View › Pixel Aspect Ratio Correction: a document pixel is drawn this many times wider than
    /// tall (1 = square). Applied in document space, before rotation, so a rotated view still
    /// shows the stretched image turned as a whole.
    pub aspect: f32,
}

impl ViewXform {
    /// The main canvas's mapping for the active document as last laid out (inside the rulers).
    pub fn active(app: &PhotocraftApp) -> Option<Self> {
        let idx = app.session.active_index()?;
        let v = app.ui.views.get(idx)?;
        let (zoom, center) = super::document_display::shown_camera(app.canvas_ctx.as_ref(), idx, egui::ViewportId::ROOT, v, app.canvas_ppp());
        Some(
            Self {
                rect: crate::rulers::content_rect(app, app.last_canvas_rect),
                zoom: zoom / app.canvas_ppp(),
                center,
                flip: app.ui.view.flip_horizontal,
                rotation: v.rotation,
                aspect: app.ui.view.display_aspect(),
            }
            .shown(app, app.canvas_ppp()),
        )
    }

    /// This mapping as the canvas draws it at `ppp` device pixels per point: pixel-aligned by the canvas's own rule
    /// ([`pixel_aligned`]) unless the view follows the pointer, so tools and overlays land where the picture is.
    pub(crate) fn shown(self, app: &PhotocraftApp, ppp: f32) -> Self {
        if super::document_display::view_follows_pointer(app) {
            return self;
        }
        let view = CanvasView::new(self.rect, Point::new(f64::from(self.center[0]), f64::from(self.center[1])), f64::from(self.zoom))
            .rotation(f64::from(self.rotation))
            .flip_x(self.flip)
            .aspect(f64::from(self.aspect));
        super::document_display::view_xform(&pixel_aligned(view, ppp))
    }

    fn sincos(self) -> (f32, f32) {
        if self.rotation.abs() < 1e-8 {
            (1.0, 0.0)
        } else {
            let r = self.rotation.to_radians();
            (r.cos(), r.sin())
        }
    }

    /// Document pixel width over height on screen: [`Self::aspect`], or 1 when it is unusable.
    fn sx(self) -> f32 {
        if self.aspect.is_finite() && self.aspect > 0.0 { self.aspect } else { 1.0 }
    }

    /// Screen points per document pixel along the document's x axis.
    pub fn zoom_x(&self) -> f32 {
        self.zoom * self.sx()
    }

    /// Unflip, unrotate and unstretch a screen-space vector (no zoom). Hand pan and zoom-about
    /// use this.
    pub fn unmap_vec(self, d: Vec2) -> Vec2 {
        let d = if self.flip { vec2(-d.x, d.y) } else { d };
        let (c, s) = self.sincos();
        vec2((d.x * c + d.y * s) / self.sx(), -d.x * s + d.y * c)
    }

    /// A document-space vector on screen: stretched, rotated, flipped and zoomed as the view
    /// draws it (the inverse of [`Self::unmap_vec`], with the zoom).
    pub fn map_vec(self, d: Vec2) -> Vec2 {
        let (c, s) = self.sincos();
        let dx = d.x * self.sx();
        let v = vec2(dx * c - d.y * s, dx * s + d.y * c) * self.zoom;
        if self.flip { vec2(-v.x, v.y) } else { v }
    }

    pub fn to_screen(&self, x: f32, y: f32) -> Pos2 {
        let (c, s) = self.sincos();
        let dx = (x - self.center[0]) * self.sx();
        let dy = y - self.center[1];
        let rx = dx * c - dy * s;
        let ry = dx * s + dy * c;
        let sx = if self.flip { -1.0 } else { 1.0 };
        self.rect.center() + vec2(rx * self.zoom * sx, ry * self.zoom)
    }
    pub fn to_doc(&self, p: Pos2) -> [f64; 2] {
        let d = (p - self.rect.center()) / self.zoom;
        let u = self.unmap_vec(d);
        [(u.x + self.center[0]) as f64, (u.y + self.center[1]) as f64]
    }
    /// On-screen bounds of the image, including camera rotation and pixel aspect correction.
    pub fn image_bounds(self, size: [u32; 2]) -> Rect {
        let (c, s) = self.sincos();
        let w = size[0] as f32 * self.sx() * self.zoom;
        let h = size[1] as f32 * self.zoom;
        let center = self.to_screen(size[0] as f32 / 2.0, size[1] as f32 / 2.0);
        Rect::from_center_size(center, vec2(c.abs() * w + s.abs() * h, s.abs() * w + c.abs() * h))
    }
    pub fn doc_rect(&self, r: DRect) -> Rect {
        let pts = [
            self.to_screen(r.x0 as f32, r.y0 as f32),
            self.to_screen(r.x1 as f32, r.y0 as f32),
            self.to_screen(r.x1 as f32, r.y1 as f32),
            self.to_screen(r.x0 as f32, r.y1 as f32),
        ];
        let mut min = pts[0];
        let mut max = pts[0];
        for p in pts {
            min = min.min(p);
            max = max.max(p);
        }
        Rect::from_min_max(min, max)
    }
}

/// Document point under a screen position on the main canvas, or `None` when the position is
/// outside it: the rulers are not part of the canvas, so drops on the ruler strip are not
/// canvas drops. Preset-panel drops and drag-and-drop placement share this so both land exactly
/// where the canvas draws.
pub fn doc_point_at(app: &PhotocraftApp, pos: Pos2) -> Option<[f64; 2]> {
    let xf = ViewXform::active(app)?;
    xf.rect.contains(pos).then(|| xf.to_doc(pos))
}

/// Fit the document into `area` (egui points) for a display with `ppp` physical pixels per
/// point; `View::zoom` is stored in physical pixels per document pixel. `aspect` is the
/// displayed pixel aspect ratio ([`ViewXform::aspect`]): the image is that much wider on screen.
pub fn fit_view(view: &mut View, doc: &Document, area: Vec2, ppp: f32, aspect: f32) {
    let (w, h) = (doc.size.width as f32, doc.size.height as f32);
    let shown_w = w * if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
    // Fit never magnifies past 100%: one image pixel per *physical* pixel, so the cap applies
    // after the points → device pixels conversion (a 150% display would otherwise open at 150%).
    let fit = ((area.x - 40.0) / shown_w).min((area.y - 40.0) / h) * ppp;
    let zoom = crate::zoom_levels::clamp(fit.min(1.0), [doc.size.width, doc.size.height]);
    view.zoom = zoom;
    view.center = [w / 2.0, h / 2.0];
    view.fit_pending = false;
    view.fill_pending = false;
}

/// Centre the document and zoom until it fills the canvas. One axis can extend beyond the
/// viewport, matching the Hand tool's Fill Screen action.
pub fn fill_view(view: &mut View, doc: &Document, area: Vec2, ppp: f32, aspect: f32) {
    let (w, h) = (doc.size.width as f32, doc.size.height as f32);
    let shown_w = w * if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
    let points = (area.x / shown_w).max(area.y / h);
    let zoom = crate::zoom_levels::clamp(points * ppp, [doc.size.width, doc.size.height]);
    view.zoom = zoom;
    view.center = [w / 2.0, h / 2.0];
    view.fit_pending = false;
    view.fill_pending = false;
}

/// Zoom to `new_zoom` about `p`: keep its document point under the pointer, or move it to the
/// view centre when Zoom Clicked Point to Center (`center_on_point`) is enabled.
pub(super) fn zoom_about(view: &mut View, xf: &ViewXform, p: Pos2, new_zoom: f32, center_on_point: bool, ppp: f32) {
    let before = xf.to_doc(p);
    view.zoom = new_zoom;
    if center_on_point {
        view.center = [before[0] as f32, before[1] as f32];
        return;
    }
    // `new_zoom` is device px per document pixel; the pointer vector is in egui points.
    let ppp = if ppp.is_finite() && ppp > 0.0 { ppp } else { 1.0 };
    let u = xf.unmap_vec((p - xf.rect.center()) / (new_zoom / ppp));
    view.center = [before[0] as f32 - u.x, before[1] as f32 - u.y];
}
