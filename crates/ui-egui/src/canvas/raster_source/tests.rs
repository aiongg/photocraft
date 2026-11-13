use super::*;
use egui::Color32;
use serde_json::json;

fn app_with(width: u32, height: u32, background: &str) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": width, "height": height, "background": background})).unwrap();
    app
}

fn read(source: &impl RasterSource, region: PixelRect, factor: u32) -> ColorImage {
    let texels = |n: i32| (n as u32).div_ceil(factor) as usize;
    let size = [texels(region.width()), texels(region.height())];
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

type Step = fn(&mut PhotocraftApp);

/// One pass of a source: its revision and size, its damage since the revision asked about, and since a revision it
/// never handed out.
struct Pass {
    revision: Revision,
    size: PixelSize,
    damage: Option<PixelRect>,
    unknown: Option<PixelRect>,
}

fn pass(source: &impl RasterSource, since: Revision) -> Pass {
    Pass { revision: source.revision(), size: source.size(), damage: source.damage(since), unknown: source.damage(Revision::new(7)) }
}

/// Runs each case on a fresh document from `setup`: a pass, the event, a second pass, and the second pass's damage
/// since the first.
fn check_damage(setup: fn() -> PhotocraftApp, run: fn(&mut PhotocraftApp, &egui::Context, Revision) -> Pass, cases: &[(&str, Step, Want)]) {
    let ctx = egui::Context::default();
    for (event, step, want) in cases {
        let mut app = setup();
        let first = run(&mut app, &ctx, Revision::new(7)).revision;
        step(&mut app);
        let Pass { size, damage, unknown, .. } = run(&mut app, &ctx, first);
        let bounds = PixelRect::of_size(size);
        match want {
            Want::Nothing => assert!(damage.is_some_and(|d| d.is_empty()), "{event}: {damage:?}"),
            Want::Covers(r) => {
                let d = damage.unwrap_or_else(|| panic!("{event}: the source could not tell"));
                assert_eq!(d.intersect(r), *r, "{event}: {d:?} misses {r:?}");
                assert_eq!(d.intersect(&bounds), d, "{event}: {d:?} leaves the document");
            }
            Want::Everything => assert_eq!(damage, None, "{event}"),
        }
        assert_eq!(unknown, None, "{event}: a revision the source never handed out");
    }
}

#[test]
fn composite_damage_since_the_shown_revision_follows_each_event() {
    let setup = || {
        let mut app = app_with(16, 8, "#336699");
        app.run("select.rect", json!({"x": 2, "y": 1, "width": 3, "height": 2})).unwrap();
        app
    };
    let cases: [(&str, Step, Want); 3] = [
        ("nothing changed", |_| {}, Want::Nothing),
        (
            "a fill inside the selection",
            |app| {
                app.run("edit.fill", json!({"color": "#cc2200"})).unwrap();
            },
            Want::Covers(PixelRect::new(2, 1, 5, 3)),
        ),
        (
            "the canvas grows",
            |app| {
                app.run("image.canvasSize", json!({"width": 24, "height": 12})).unwrap();
            },
            Want::Everything,
        ),
    ];
    check_damage(setup, |app, ctx, since| pass(&CompositePrep::new(app, ctx, 0, None).unwrap().source(app), since), &cases);
}

#[test]
fn overlay_damage_since_the_shown_revision_follows_each_event() {
    // A layer mask shown over the composite.
    let setup = || {
        let mut app = app_with(16, 8, "#336699");
        app.run("layer.layerMask.revealAll", json!({})).unwrap();
        app.run("view.layerMask", json!({"mode": "overlay"})).unwrap();
        app
    };
    let cases: [(&str, Step, Want); 2] = [
        ("nothing changed", |_| {}, Want::Nothing),
        (
            "the mask shows in grey instead",
            |app| {
                app.run("view.layerMask", json!({"mode": "gray"})).unwrap();
            },
            Want::Everything,
        ),
    ];
    check_damage(setup, |app, ctx, since| pass(&OverlayPrep::new(app, ctx, 0, None).unwrap().source(app), since), &cases);
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
    let source = CompositePrep::new(&mut app, &ctx, 0, None).unwrap().source(&app);
    let (red, blue) = (Color32::from_rgb(255, 0, 0), Color32::from_rgb(0, 0, 255));
    assert_eq!(read(&source, PixelRect::new(1, 0, 3, 2), 2).pixels, vec![red]);
    assert_eq!(read(&source, PixelRect::new(0, 0, 4, 1), 1).pixels, vec![blue, red, red, blue]);
    // A region past the document's end reads what lies inside and leaves the rest.
    assert_eq!(read(&source, PixelRect::new(2, 0, 6, 1), 1).pixels, vec![red, blue, Color32::TRANSPARENT, Color32::TRANSPARENT]);
}

#[test]
fn a_channel_view_draws_its_own_picture() {
    let mut app = app_with(8, 4, "#ffffff");
    app.run("select.rect", json!({"x": 0, "y": 0, "width": 4, "height": 4})).unwrap();
    let ctx = egui::Context::default();
    assert!(OverlayPrep::new(&mut app, &ctx, 0, None).is_none(), "the plain composite has no overlay");
    app.run("select.editInQuickMaskMode", json!({})).unwrap();
    let px = read(&OverlayPrep::new(&mut app, &ctx, 0, None).unwrap().source(&app), PixelRect::new(0, 0, 8, 1), 1).pixels;
    assert_eq!(Color32::WHITE.blend(px[0]), Color32::WHITE, "selected: nothing over the composite");
    assert_eq!(px[6], Color32::from_rgba_premultiplied(128, 0, 0, 128), "masked: the rubylith");
    let composite = read(&CompositePrep::new(&mut app, &ctx, 0, None).unwrap().source(&app), PixelRect::new(0, 0, 8, 1), 1).pixels;
    assert_eq!(composite[6], Color32::WHITE, "the composite stays the document's");
}

#[test]
fn during_a_proxy_adjustment_the_overlay_reads_the_preview_and_the_composite_the_document() {
    let mut app = app_with(2400, 2200, "#808080");
    let st = app.session.active().unwrap();
    assert!(crate::proxy::preview_factor(&st.doc, crate::proxy::reduced_previews(&app)) > 1, "large enough for a proxy preview");
    // Only the red channel shows: an opaque view over the composite.
    let view = &mut app.session.active_mut().unwrap().channel_view;
    view.set_color_visible(1, false);
    view.set_color_visible(2, false);
    let ctx = egui::Context::default();
    let at = PixelRect::new(0, 0, 1, 1);
    let overlay_before = read(&OverlayPrep::new(&mut app, &ctx, 0, None).unwrap().source(&app), at, 1).pixels;
    let composite_before = read(&CompositePrep::new(&mut app, &ctx, 0, None).unwrap().source(&app), at, 1).pixels;
    let id = crate::adjust_dialog::open(&mut app, "image.adjustments.levels").unwrap();
    app.ui.dialog_mut(id).unwrap().fields.insert("inBlack".into(), json!(100));
    assert!(crate::adjust_preview::display_doc(&mut app, 0).is_some(), "the adjustment previews");
    let overlay = read(&OverlayPrep::new(&mut app, &ctx, 0, None).unwrap().source(&app), at, 1).pixels;
    let composite = read(&CompositePrep::new(&mut app, &ctx, 0, None).unwrap().source(&app), at, 1).pixels;
    assert_ne!(overlay, overlay_before, "the overlay shows the live preview");
    assert_eq!(composite, composite_before, "the composite shows the document");
}
