use super::*;
use egui::Color32;
use serde_json::json;

fn app_with(width: u32, height: u32, background: &str) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": width, "height": height, "background": background})).unwrap();
    app
}

fn read(source: &PhotoComposite<'_>, region: PixelRect, factor: u32) -> ColorImage {
    let f = factor as i32;
    let size = [region.width().div_ceil(f) as usize, region.height().div_ceil(f) as usize];
    let mut out = ColorImage::filled(size, Color32::TRANSPARENT);
    source.read(region, factor, &mut out);
    out
}

/// What the second of two passes reports since the first.
#[derive(Debug)]
enum Want {
    Nothing,
    /// A region holding this rectangle, inside the document.
    Covers(PixelRect),
    /// The source cannot tell: the display reads everything.
    Everything,
}

#[test]
fn damage_since_the_shown_revision_follows_each_event() {
    type Step = fn(&mut PhotocraftApp);
    let cases: [(&str, Step, Want); 4] = [
        ("nothing changed", |_| {}, Want::Nothing),
        (
            "a fill inside the selection",
            |app| {
                app.run("edit.fill", json!({"color": "#cc2200"})).unwrap();
            },
            Want::Covers(PixelRect::new(2, 1, 5, 3)),
        ),
        (
            "Quick Mask shows over the composite",
            |app| {
                app.run("select.editInQuickMaskMode", json!({})).unwrap();
            },
            Want::Everything,
        ),
        (
            "the canvas grows",
            |app| {
                app.run("image.canvasSize", json!({"width": 24, "height": 12})).unwrap();
            },
            Want::Everything,
        ),
    ];
    let ctx = egui::Context::default();
    for (event, step, want) in cases {
        let mut app = app_with(16, 8, "#336699");
        app.run("select.rect", json!({"x": 2, "y": 1, "width": 3, "height": 2})).unwrap();
        let first = PhotoComposite::new(&mut app, &ctx, 0, None).unwrap().revision();
        step(&mut app);
        let source = PhotoComposite::new(&mut app, &ctx, 0, None).unwrap();
        let damage = source.damage(first);
        let bounds = PixelRect::of_size(source.size());
        match want {
            Want::Nothing => assert!(damage.is_some_and(|d| d.is_empty()), "{event}: {damage:?}"),
            Want::Covers(r) => {
                let d = damage.unwrap_or_else(|| panic!("{event}: the source could not tell"));
                assert_eq!(d.intersect(&r), r, "{event}: {d:?} misses {r:?}");
                assert_eq!(d.intersect(&bounds), d, "{event}: {d:?} leaves the document");
            }
            Want::Everything => assert_eq!(damage, None, "{event}"),
        }
        assert_eq!(source.damage(Revision::new(7)), None, "{event}: a revision the source never handed out");
    }
}

#[test]
fn read_reduces_from_the_region_origin() {
    // Columns 1 and 2 red, 0 and 3 blue: the texel of columns 1..3 at factor 2 is red, where cells counted from the
    // document's origin would mix red and blue.
    let mut app = app_with(4, 2, "#0000ff");
    app.run("select.rect", json!({"x": 1, "y": 0, "width": 2, "height": 2})).unwrap();
    app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
    app.run("select.deselect", json!({})).unwrap();
    let ctx = egui::Context::default();
    let source = PhotoComposite::new(&mut app, &ctx, 0, None).unwrap();
    let (red, blue) = (Color32::from_rgb(255, 0, 0), Color32::from_rgb(0, 0, 255));
    assert_eq!(read(&source, PixelRect::new(1, 0, 3, 2), 2).pixels, vec![red]);
    assert_eq!(read(&source, PixelRect::new(0, 0, 4, 1), 1).pixels, vec![blue, red, red, blue]);
    // A region past the document's end reads what lies inside and leaves the rest.
    assert_eq!(read(&source, PixelRect::new(2, 0, 6, 1), 1).pixels, vec![red, blue, Color32::TRANSPARENT, Color32::TRANSPARENT]);
}

#[test]
fn a_channel_view_draws_over_the_composite() {
    let mut app = app_with(8, 4, "#ffffff");
    app.run("select.rect", json!({"x": 0, "y": 0, "width": 4, "height": 4})).unwrap();
    app.run("select.editInQuickMaskMode", json!({})).unwrap();
    let ctx = egui::Context::default();
    let source = PhotoComposite::new(&mut app, &ctx, 0, None).unwrap();
    let px = read(&source, PixelRect::new(0, 0, 8, 1), 1).pixels;
    assert_eq!(px[0], Color32::WHITE, "selected: the composite");
    assert_eq!(px[6], Color32::WHITE.blend(Color32::from_rgba_premultiplied(128, 0, 0, 128)), "masked: the rubylith over it");
}
