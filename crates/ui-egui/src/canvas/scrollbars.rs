//! Canvas scrollbars (#300, #620) and Preferences › Tools › Overscroll: PhotoCraft's `View` fed
//! to the library's `CanvasScrollbars`.
//!
//! `View::zoom` is device pixels per document pixel and its centre is `f32`; the library works in
//! points with a `CanvasView`, so each call converts in and writes the moved centre back.

use craft::ui::canvas::kurbo::{Point, Rect as DocRect};
use craft::ui::canvas::{CanvasScrollbars, CanvasView, ScrollbarNames};
use craft::ui::localized::Labels as _;
use egui::{Id, Rect, Ui};

use crate::state::View;

/// Thickness of a bar, in points.
pub fn thickness(ctx: &egui::Context) -> f32 {
    CanvasScrollbars::style(&craft::ui::theme::current(ctx)).width
}

/// `view` as the library's view of `rect` at `ppp` physical pixels per point.
fn canvas_view(view: &View, rect: Rect, ppp: f32, flip: bool, aspect: f32) -> CanvasView {
    let ppp = if ppp.is_finite() && ppp > 0.0 { ppp } else { 1.0 };
    let mut v = CanvasView::new(rect, Point::new(f64::from(view.center[0]), f64::from(view.center[1])), f64::from(view.zoom / ppp));
    v.rotation = f64::from(view.rotation);
    v.flip_x = flip;
    v.aspect = f64::from(aspect);
    v
}

fn doc_bounds(view: &View) -> DocRect {
    DocRect::new(0.0, 0.0, f64::from(view.doc_size[0]), f64::from(view.doc_size[1]))
}

/// Apply Preferences › Tools › Overscroll (off) to `view` for a canvas of `size` points on a
/// display with `ppp` physical pixels per point. True when the view moved (then repaint).
pub fn clamp_view(view: &mut View, size: egui::Vec2, ppp: f32) -> bool {
    clamp_view_with_aspect(view, size, ppp, 1.0)
}

/// Clamp against the displayed image bounds, so rotated tool auto-scroll is not undone by
/// the next frame's Overscroll-off clamp. `size` is the unmodified screen-space canvas size.
pub fn clamp_view_with_aspect(view: &mut View, size: egui::Vec2, ppp: f32, aspect: f32) -> bool {
    if view.doc_size[0] == 0 || view.doc_size[1] == 0 {
        return false;
    }
    let mut v = canvas_view(view, Rect::from_min_size(egui::Pos2::ZERO, size), ppp, false, aspect);
    let moved = CanvasScrollbars::clamp(&mut v, doc_bounds(view));
    if moved {
        view.center = [v.center.x as f32, v.center.y as f32];
    }
    moved
}

/// Show the scrollbars of the canvas `rect` and apply their drags and page clicks to `view`.
/// `flip` is View › Flip Horizontal (the bar then runs the other way through the image);
/// `aspect` is the displayed pixel aspect ratio (a document pixel is that many points wider per
/// point of height); `overscroll` is Preferences › Tools › Overscroll.
/// Returns true when the pointer is over a bar (the canvas then leaves the pointer alone).
pub fn show(ui: &Ui, rect: Rect, view: &mut View, flip: bool, aspect: f32, overscroll: bool, key: Id) -> bool {
    let mut v = canvas_view(view, rect, ui.ctx().pixels_per_point(), flip, aspect);
    let out = CanvasScrollbars::new(key, ScrollbarNames::of(ui)).overscroll(overscroll).show(ui, &mut v, doc_bounds(view));
    if out.changed {
        view.center = [v.center.x as f32, v.center.y as f32];
    }
    out.over
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::*;

    const THICKNESS: f32 = 12.0;

    #[test]
    fn overscroll_off_clamps_a_view() {
        let mut v = View {
            zoom: 1.0,
            center: [-500.0, 20.0],
            fit_pending: false,
            fill_pending: false,
            doc_size: [1000, 800],
            rotation: 0.0,
            zoom_animating: false,
            zoom_anchor: None,
            display_camera: None,
            zoom_transition: None,
        };
        assert!(clamp_view(&mut v, vec2(500.0, 400.0), 1.0));
        assert_eq!(v.center, [250.0, 200.0]);
        assert!(!clamp_view(&mut v, vec2(500.0, 400.0), 1.0), "stable: no repaint loop");
        v.doc_size = [0, 0];
        assert!(!clamp_view(&mut v, vec2(500.0, 400.0), 1.0));
    }

    mod canvas {
        use egui::{Modifiers, PointerButton, Pos2, pos2, vec2};
        use egui_kittest::Harness;
        use serde_json::json;

        use super::super::*;
        use super::THICKNESS;
        use crate::PhotocraftApp;

        fn harness(overscroll: bool) -> Harness<'static, PhotocraftApp> {
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
            app.run("file.new", json!({"width": 2000, "height": 1500})).unwrap();
            app.run("prefs.set", json!({"path": "tools.overscroll", "value": overscroll})).unwrap();
            app.sync_views();
            let mut h = Harness::builder().with_size(vec2(1000.0, 700.0)).build_ui_state(
                |ui, app: &mut PhotocraftApp| {
                    let ctx = ui.ctx().clone();
                    if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                        return;
                    }
                    egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
                },
                app,
            );
            PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::default());
            h.run_steps(4);
            h
        }

        fn view(h: &Harness<'static, PhotocraftApp>) -> View {
            h.state().ui.views[0].clone()
        }

        fn set_view(h: &mut Harness<'static, PhotocraftApp>, zoom: f32, center: [f32; 2]) {
            let v = &mut h.state_mut().ui.views[0];
            v.zoom = zoom;
            v.center = center;
            h.run_steps(2);
        }

        /// Centre and length of the horizontal thumb, in screen points.
        fn h_thumb(h: &Harness<'static, PhotocraftApp>) -> (Pos2, f32) {
            let r = h.state().last_canvas_rect;
            let v = view(h);
            // The visible span and the scrollable extent along x, in document pixels.
            let len = v.doc_size[0] as f32;
            let half = r.width() / v.zoom / 2.0;
            let (v0, v1) = (v.center[0] - half, v.center[0] + half);
            let (visible, mut lo, mut hi) = (v1 - v0, v0.min(0.0), v1.max(len));
            if h.state().session.prefs().tools.overscroll {
                (lo, hi) = (lo.min(len - visible), hi.max(visible));
            }
            let bar = Rect::from_min_max(pos2(r.left(), r.bottom() - THICKNESS), pos2(r.right() - THICKNESS, r.bottom()));
            let thumb = (visible / (hi - lo) * bar.width()).clamp(24.0, bar.width());
            let room = hi - lo - visible;
            let at = if room > 1e-6 { ((v0 - lo) / room).clamp(0.0, 1.0) * (bar.width() - thumb) } else { 0.0 };
            let (at, len) = (at, thumb);
            (pos2(bar.left() + at + len / 2.0, bar.center().y), len)
        }

        fn drag(h: &mut Harness<'static, PhotocraftApp>, p: Pos2, by: f32) {
            h.event(egui::Event::PointerMoved(p));
            h.run_steps(1);
            h.event(egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
            h.run_steps(1);
            for k in 1..=10 {
                h.event(egui::Event::PointerMoved(p + vec2(by * k as f32 / 10.0, 0.0)));
                h.run_steps(1);
            }
            h.event(egui::Event::PointerButton { pos: p + vec2(by, 0.0), button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
            h.run_steps(2);
        }

        fn click(h: &mut Harness<'static, PhotocraftApp>, p: Pos2) {
            h.event(egui::Event::PointerMoved(p));
            h.run_steps(1);
            h.event(egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
            h.run_steps(1);
            h.event(egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
            h.run_steps(2);
        }

        #[test]
        fn shown_and_scrollable_when_the_image_fits() {
            for overscroll in [true, false] {
                let mut h = harness(overscroll);
                let r = h.state().last_canvas_rect;
                let (p, len) = h_thumb(&h);
                let c0 = view(&h).center;
                drag(&mut h, p, -40.0);
                let c1 = view(&h).center;
                if overscroll {
                    // Fit on screen, the bar still moves the image (#620).
                    assert!(len < r.width() - 2.0 * THICKNESS, "the thumb leaves room: {len}");
                    assert!(c1[0] < c0[0] - 10.0, "{c0:?} -> {c1:?}");
                } else {
                    assert!((len - (r.width() - THICKNESS)).abs() < 1.0, "the thumb fills the track: {len}");
                    // Within float error: the fit zoom is not a round number for every canvas height.
                    assert!((c1[0] - c0[0]).abs() < 0.01 && (c1[1] - c0[1]).abs() < 0.01, "Overscroll off keeps a fitting image centred: {c0:?} -> {c1:?}");
                }
            }
        }

        #[test]
        fn full_screen_modes_have_no_bars() {
            let mut h = harness(true);
            h.state_mut().ui.tool = crate::state::Tool::Hand;
            let (p, _) = h_thumb(&h);
            for mode in ["fullScreenWithMenuBar", "fullScreen"] {
                h.state_mut().ui.view.screen_mode = mode.into();
                h.run_steps(2);
                // The drag over where the bar was reaches the Hand: the image follows the pointer.
                let c0 = view(&h).center;
                drag(&mut h, p, -40.0);
                assert!(view(&h).center[0] > c0[0], "{mode}: {c0:?} -> {:?}", view(&h).center);
            }
        }

        #[test]
        fn dragging_the_thumb_scrolls_and_paints_nothing() {
            let mut h = harness(true);
            h.state_mut().ui.tool = crate::state::Tool::Brush;
            set_view(&mut h, 1.0, [700.0, 750.0]);
            let (p, _) = h_thumb(&h);
            let c0 = view(&h).center;
            drag(&mut h, p, 100.0);
            let c1 = view(&h).center;
            // 100 points of thumb travel scroll about 2 px per point (1000 px of room, ~500 of travel).
            assert!(c1[0] - c0[0] > 150.0, "{c0:?} -> {c1:?}");
            assert_eq!(c1[1], c0[1]);
            assert!(!h.state().session.journal.iter().any(|(id, _)| id == "paint.stroke"), "the bar never reaches the tool");
            let (t, _) = h_thumb(&h);
            assert!((t.x - (p.x + 100.0)).abs() < 3.0, "the thumb followed the pointer: {t:?} vs {p:?}");
        }

        #[test]
        fn clicking_the_track_pages_by_a_screenful() {
            let mut h = harness(true);
            set_view(&mut h, 1.0, [500.0, 750.0]);
            let (p, _) = h_thumb(&h);
            let r = h.state().last_canvas_rect;
            let x0 = view(&h).center[0];
            click(&mut h, pos2(r.right() - 30.0, p.y));
            let x1 = view(&h).center[0];
            assert!((x1 - x0 - r.width()).abs() < 1.0, "paged right by the visible width: {x0} -> {x1}");
            // Paging stops at the end of the image.
            click(&mut h, pos2(r.right() - 30.0, p.y));
            click(&mut h, pos2(r.right() - 30.0, p.y));
            let end = 2000.0 - r.width() / 2.0;
            assert!((view(&h).center[0] - end).abs() < 1.0, "{}", view(&h).center[0]);
            click(&mut h, pos2(r.left() + 5.0, p.y));
            assert!((view(&h).center[0] - (end - r.width())).abs() < 1.0);
        }

        #[test]
        fn overscroll_off_keeps_the_image_in_view() {
            for overscroll in [true, false] {
                let mut h = harness(overscroll);
                set_view(&mut h, 1.0, [-3000.0, 750.0]);
                let x = view(&h).center[0];
                let w = h.state().last_canvas_rect.width();
                if overscroll {
                    assert_eq!(x, -3000.0, "overscroll on: the view is free");
                } else {
                    assert!((x - w / 2.0).abs() < 1e-3, "clamped to the left edge: {x}");
                }
                // Zoomed out below fit (Photoshop): the image is not re-centred, it stays where it
                // is while it fits in the view, and is pushed back in at the view's edge.
                set_view(&mut h, 0.1, [-50.0, 9000.0]);
                if !overscroll {
                    let r = h.state().last_canvas_rect;
                    let (hw, hh) = (r.width() / 0.1 / 2.0, r.height() / 0.1 / 2.0);
                    let c = view(&h).center;
                    assert_eq!(c[0], -50.0, "inside the view: left alone");
                    assert!((c[1] - hh).abs() < 1e-2, "pushed back in: the image's top on the view's: {c:?}");
                    assert!(c[0] - hw <= 0.0 && c[0] + hw >= 2000.0 && c[1] - hh <= 0.0 && c[1] + hh >= 1500.0, "{c:?}");
                }
            }
        }
    }
}
