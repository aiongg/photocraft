//! The document canvas on the framework's `Canvas`: the view binding, the navigation options from Preferences, and
//! the document's picture in its body (a GPU picture or the CPU composite, and the Channels panel's view over it).

use craft::behavior::geometry::quad::Homography;
use craft::behavior::kurbo::{Point, Rect as Rect64, Size};
use craft::behavior::{CanvasView, PixelSize};
use craft::ui::canvas::{Canvas, CanvasOutput, Extent, NavigationOptions, RasterDisplay, RasterSource};
use craft::ui::contract::{Bind, Props, Show, ShowContainer};
use craft::ui::runtime::env::Label;
use craft::ui::primitives::Checker;
use craft::ui::runtime::focus::{KeyUse, host_canvas};
use craft::ui::style::DataColor;
use egui::{Color32, Pos2, Rect, Sense};

use super::gpu_picture::{GpuPicture, GpuRequest, gpu_picture};
use super::raster_source::{CompositePrep, OverlayPrep};
use super::view_xform::ViewXform;
use super::{DRect, Document, PhotocraftApp, View};

impl View {
    /// This view as the framework's canvas view of `rect` at `ppp` device pixels per point, mirrored by View › Flip
    /// Horizontal and stretched by View › Pixel Aspect Ratio Correction.
    pub(crate) fn canvas_view(&self, rect: Rect, ppp: f32, flip: bool, aspect: f32) -> CanvasView {
        let center = Point::new(f64::from(self.center[0]), f64::from(self.center[1]));
        CanvasView::new(rect, center, CanvasView::points_per_unit(f64::from(self.zoom), ppp))
            .rotation(f64::from(self.rotation))
            .flip_x(flip)
            .aspect(f64::from(aspect))
    }

    /// Takes the zoom, centre and rotation of `view`, shown at `ppp` device pixels per point.
    pub(crate) fn set_from(&mut self, view: &CanvasView, ppp: f32) {
        self.zoom = (view.zoom * f64::from(ppp)) as f32;
        self.center = [view.center.x as f32, view.center.y as f32];
        self.rotation = crate::rotate_view::wrap_deg(view.rotation as f32);
    }
}

/// The view a canvas shows as PhotoCraft's mapping, for the overlays and tools that still map through it.
pub(crate) fn view_xform(view: &CanvasView) -> ViewXform {
    ViewXform {
        rect: view.rect,
        zoom: view.zoom as f32,
        center: [view.center.x as f32, view.center.y as f32],
        flip: view.flip_x,
        rotation: view.rotation as f32,
        aspect: view.aspect as f32,
    }
}

/// The user's navigation choices from Preferences (General, Tools, Enhanced Controls).
pub(crate) fn navigation(app: &PhotocraftApp) -> NavigationOptions {
    let p = app.session.prefs();
    let mut options = NavigationOptions::default();
    options.zoom_with_wheel = p.general.zoom_with_scroll_wheel;
    options.animated_zoom = p.general.animated_zoom;
    options.flick_panning = p.tools.enable_flick_panning;
    options.overscroll = p.tools.overscroll;
    options.pinch_zooms = p.enhanced_controls.zoom_with_trackpad_pinch;
    options.trackpad_rotate = p.enhanced_controls.rotate_view_with_trackpad;
    options.touch_gestures = p.enhanced_controls.touch_gestures;
    options
}

/// One document view's canvas this pass.
pub(crate) struct CanvasPass<'a> {
    pub ctx: &'a egui::Context,
    pub idx: usize,
    pub doc: &'a std::sync::Arc<Document>,
    /// The display the canvas is on (`None`: the main window's).
    pub output: Option<u32>,
    pub rect: Rect,
    pub ppp: f32,
    pub flip: bool,
    pub aspect: f32,
    /// The keys PhotoCraft's tool widget takes while it is the key host; `None` while another control has them.
    pub keys: Option<KeyUse>,
    /// PhotoCraft pressed on the canvas itself this pass (its own pan, a tool drag): an ease stops.
    pub jumped: bool,
}

/// What the canvas pass hands back: the canvas's output, the view it shows, and PhotoCraft's tool widget, which the
/// tool code reads after the canvas.
pub(crate) struct Shown {
    pub output: CanvasOutput,
    pub view: CanvasView,
    pub response: egui::Response,
}

/// The document's picture as prepared for this pass, before the app is borrowed shared.
enum Prepared {
    Gpu(GpuPicture),
    Cpu(CompositePrep),
}

/// Shows document `pass.idx` in a framework `Canvas` over `pass.rect` through `view`: the canvas folds the wheel,
/// pinch, page keys, view commands and the Overscroll clamp into the view, eases PhotoCraft's own eased zooms, and its
/// body draws the document's picture. `view` is the view after the canvas's pass.
pub(crate) fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui, view: &mut View, pass: &CanvasPass<'_>) -> Shown {
    let (ctx, idx, doc, output, rect, ppp) = (pass.ctx, pass.idx, pass.doc, pass.output, pass.rect, pass.ppp);
    let given = view.canvas_view(rect, ppp, pass.flip, pass.aspect);
    // What the canvas drew last pass: an eased zoom starts there.
    let drawn = view.display_camera.map_or(given, |(zoom, center)| View { zoom, center, ..view.clone() }.canvas_view(rect, ppp, pass.flip, pass.aspect));
    let ease = view.zoom_animating.then(|| (drawn, view.zoom_anchor.map(|(doc, screen)| (Point::new(doc[0], doc[1]), Pos2::new(screen[0], screen[1])))));
    view.zoom_animating = false;
    view.zoom_anchor = None;

    let size = PixelSize::new(doc.size.width, doc.size.height);
    let prefs = app.session.prefs();
    let checker = prefs.transparency_and_gamut.square().map(|square| {
        let [light, dark] = prefs.transparency_and_gamut.colors();
        let color = |c: [u8; 3]| DataColor::new(Color32::from_rgb(c[0], c[1], c[2]));
        Checker::new().size(square).colors(color(light), color(dark))
    });
    let border = prefs.interface.canvas_border;
    let pattern = app.ui.view.pattern_preview;
    // A flipped or pixel-aspect-corrected view draws through the CPU path (the GPU canvas shader has no mirroring or
    // non-square pixels). Rotation stays on the GPU: it is a view uniform.
    let gpu_ok = crate::gpu_canvas::gpu_view_ok(pass.flip, view.rotation, pass.aspect);
    if app.gpu.is_some() {
        if gpu_ok {
            crate::render_path::view_ok(app, doc.id.0);
        } else {
            crate::render_path::view_cpu(app, doc.id.0);
        }
    }
    let xf = view_xform(&drawn);
    let target = view_xform(&given);
    let picture = gpu_ok
        .then(|| gpu_picture(app, &GpuRequest { ctx, idx, doc, output, xf: &xf, target: &target, zoom: view.zoom, render_zoom: view.zoom }))
        .flatten();
    match &picture {
        Some(picture) if picture.shows_document() && app.navigator_plain.contains(&doc.id) => {
            app.navigator_gpu.insert(doc.id, picture.params(&given));
        }
        _ => {
            app.navigator_gpu.remove(&doc.id);
        }
    }
    let prepared = match picture {
        Some(picture) => Some(Prepared::Gpu(picture)),
        None => CompositePrep::new(app, ctx, idx, output).map(Prepared::Cpu),
    };
    let overlay = OverlayPrep::new(app, ctx, idx, output);
    let beyond = super::paint::beyond_canvas(app, ctx, idx, output);
    // The canvas edge isn't outlined while the pixels past it show (Photoshop's crop preview), nor while the Crop tool
    // hides everything outside its box (Show Cropped Area off).
    let edge = border == photocraft_engine::prefs::CanvasBorder::Line && beyond.is_none() && !crate::crop_shield::hides_outside(app);
    let label = super::tabs::tab_title_at(app, idx, view.zoom);
    let pasteboard = crate::prefs_ui::pasteboard_color(app);
    let dots = app.session.prefs().interface.show_canvas_dots;
    let theme = crate::theme::Tokens::get(ctx);
    let shadow = border == photocraft_engine::prefs::CanvasBorder::DropShadow && !theme.bevel && !theme.pro && !pattern && beyond.is_none();
    let navigation = navigation(app);

    let app: &PhotocraftApp = app;
    let (gpu, composite) = match prepared {
        Some(Prepared::Gpu(picture)) => (Some(picture), None),
        Some(Prepared::Cpu(prep)) => (None, Some(prep.source(app))),
        None => (None, None),
    };
    let overlay = overlay.map(|prep| prep.source(app));
    let bounds = gpu.as_ref().map(GpuPicture::bounds).or_else(|| composite.as_ref().map(|c| c.bounds())).unwrap_or_else(|| doc.bounds());
    let mapping = (bounds != doc.bounds()).then(|| placement(bounds));
    let widget = egui::Id::unique(("pc-canvas-tools", idx, ctx.viewport_id()));

    let mut bound = given;
    let mut canvas = Canvas::new(label.clone()).id_salt(("pc-canvas", idx)).view(Bind::bound(&mut bound)).document(Size::new(f64::from(size.width), f64::from(size.height))).navigation(navigation);
    canvas = match ease {
        Some((from, anchor)) => canvas.ease_from(from, anchor),
        None if pass.jumped => canvas.jumped(),
        None => canvas,
    };
    let shown = canvas.show_at(ui, rect, |ui, body| {
        // Pasteboard: the Interface colour or the theme's canvas with its dots, under the document's shadow.
        match pasteboard {
            Some(color) => {
                ui.painter_at(body.rect).rect_filled(body.rect, 0.0, color);
            }
            None => super::tabs::paint_dots(ui, body.rect, dots),
        }
        if let Some(space) = craft::ui::canvas::canvas_space(&body.env) {
            let painter = ui.painter_at(body.rect);
            if shadow {
                let area = Rect::from_points(&space.view.quad(rect64(bounds)));
                painter.add(egui::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(150) }.as_shape(area, 0));
            }
            // Crop: the layers' pixels past the canvas, under it (its edge blends into them).
            if let Some(beyond) = &beyond {
                beyond.paint(&painter, &view_xform(&space.view));
            }
        }
        let frame = Frame { checker: checker.as_ref(), edge, mapping, pattern };
        if let Some(picture) = &gpu {
            frame.apply(RasterDisplay::painted(label.clone(), size, picture.clone().into_paint())).show_at(ui, body.rect);
        } else if let Some(composite) = &composite {
            frame.apply(RasterDisplay::new(label.clone(), composite as &dyn RasterSource)).show_at(ui, body.rect);
        }
        if let Some(overlay) = &overlay {
            let display = RasterDisplay::new(Label::Empty, overlay as &dyn RasterSource).id_salt("pc-channel-view");
            let display = match mapping {
                Some(frame) => display.mapped(frame, 1),
                None => display,
            };
            display.show_at(ui, body.rect);
        }
        // P-15 replaces this widget with the framework's ToolHost: PhotoCraft's tools read its response until then.
        let response = ui.interact(body.rect, widget, Sense::click_and_drag());
        if let Some(keys) = pass.keys {
            host_canvas(ui.ctx(), response.id, keys);
        }
        response
    });
    drop((composite, overlay));
    let output = shown.output;
    if let Some(next) = output.view {
        view.set_from(&next, ppp);
    }
    let shown_view = output.space.map_or(given, |space| space.view);
    view.display_camera = Some(((shown_view.zoom * f64::from(ppp)) as f32, [shown_view.center.x as f32, shown_view.center.y as f32]));
    let response = shown.inner.unwrap_or_else(|| ui.interact(rect, widget, Sense::click_and_drag()));
    Shown { output, view: shown_view, response }
}

/// How the document's picture sits on the canvas: the transparency checkerboard under it, its edge, where a reduced or
/// size-changing picture lies, and View › Pattern Preview's copies around it.
struct Frame<'c> {
    checker: Option<&'c Checker>,
    edge: bool,
    mapping: Option<Homography>,
    pattern: bool,
}

impl Frame<'_> {
    fn apply<'d>(&self, display: RasterDisplay<'d>) -> RasterDisplay<'d> {
        let display = match self.checker {
            Some(checker) => display.checkerboard(checker.clone()),
            None => display,
        };
        let display = if self.edge { display.outlined() } else { display };
        match self.mapping {
            Some(frame) => display.mapped(frame, 1),
            None if self.pattern => display.extent(Extent::Tiled),
            None => display,
        }
    }
}

/// The unit square onto `bounds`, the document area a reduced or size-changing picture covers.
fn placement(bounds: DRect) -> Homography {
    let (x0, y0) = (f64::from(bounds.x0), f64::from(bounds.y0));
    let (w, h) = (f64::from(bounds.width()), f64::from(bounds.height()));
    Homography::from_rows([[w, 0.0, x0], [0.0, h, y0], [0.0, 0.0, 1.0]])
}

fn rect64(r: DRect) -> Rect64 {
    Rect64::new(f64::from(r.x0), f64::from(r.y0), f64::from(r.x1), f64::from(r.y1))
}
