use super::*;
use craft::behavior::kurbo::Point;
use egui::{Rect, pos2};

fn picture(shows: Shows, k: u32, growth: [f32; 2]) -> GpuPicture {
    let params = ViewParams {
        doc: 1,
        doc_size: [400, 300],
        zoom: 1.0,
        center: [0.0, 0.0],
        rotation: 0.0,
        shadow: false,
        pixel_grid: false,
        view_key: 9,
        display: 0,
        output: 0,
        hdr: None,
        offset: [0.0, 0.0],
    };
    GpuPicture { params, shows, k, growth, doc_size: [400, 300], pattern: false }
}

#[test]
fn a_canvas_view_maps_to_the_shader_view() {
    let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(800.0, 600.0));
    let half_turn = std::f32::consts::FRAC_PI_2;
    // (picture, view, zoom, centre, rotation in radians)
    let cases = [
        (picture(Shows::Document, 1, [0.0, 0.0]), CanvasView::new(rect, Point::new(100.0, 50.0), 2.0).rotation(90.0), 2.0, [100.0, 50.0], half_turn),
        // A preview reduced by 4 that grew 3 texels left and right and shrank 1 at the top and bottom.
        (picture(Shows::Preview, 4, [3.0, -1.0]), CanvasView::new(rect, Point::new(100.0, 40.0), 0.5), 2.0, [28.0, 9.0], 0.0),
        (picture(Shows::Pages, 1, [0.0, 0.0]), CanvasView::new(rect, Point::new(0.0, 0.0), 1.0).rotation(f64::NAN), 1.0, [0.0, 0.0], 0.0),
    ];
    for (picture, view, zoom, center, rotation) in cases {
        let p = picture.params(&view);
        assert_eq!((p.zoom, p.center, p.doc, p.view_key), (zoom, center, 1, 9), "{view:?}");
        assert!((p.rotation - rotation).abs() < 1e-6, "{view:?}: {}", p.rotation);
    }
    // The shader's own shadow and grid only where today's canvas asks, and never a grid over a preview's texels.
    let framed = |shows| picture(shows, 1, [0.0, 0.0]).framed(true, true).params;
    assert_eq!((framed(Shows::Document).shadow, framed(Shows::Document).pixel_grid), (true, true));
    assert_eq!((framed(Shows::Preview).shadow, framed(Shows::Preview).pixel_grid), (true, false));
    assert_eq!((picture(Shows::Document, 1, [0.0, 0.0]).params.shadow, picture(Shows::Document, 1, [0.0, 0.0]).params.pixel_grid), (false, false));
}
