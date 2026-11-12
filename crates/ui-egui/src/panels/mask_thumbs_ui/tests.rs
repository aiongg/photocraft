//! #153: mask thumbnails, link chains and the red X in Layers rows; #154: the layer mask in
//! Channels, the layer's path in Paths, aspect-correct channel thumbnails, the Paths footer.

use egui::{Modifiers, PointerButton, Pos2, Rect, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use super::{MaskKind, vector_thumb_image};
use crate::PhotocraftApp;
use craft::layers::ui::tree::LayerPart;

/// A 200×100 document: "Masked" (pixel + vector mask) and a "Badge" shape layer.
fn session() -> (photocraft_engine::Session, u64, u64) {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 200, "height": 100})).unwrap();
    let masked = s.execute("layer.new.layer", json!({"name": "Masked"})).unwrap()["layer"].as_u64().unwrap();
    s.execute("edit.fill", json!({"contents": "color", "color": "#336699"})).unwrap();
    s.execute("layer.layerMask.revealAll", json!({"layer": masked})).unwrap();
    let path = json!({"subpaths": [{"knots": [[20, 10], [120, 10], [120, 90], [20, 90]]}]});
    s.execute("layer.vectorMask.add", json!({"layer": masked, "path": path})).unwrap();
    let shape =
        s.execute("shape.create", json!({"kind": "rect", "rect": [140, 20, 40, 40], "fill": "#cc3333", "name": "Badge"})).unwrap()["layer"].as_u64().unwrap();
    s.execute("layer.select", json!({"layer": masked})).unwrap();
    (s, masked, shape)
}

fn harness(session: photocraft_engine::Session, tab: usize, ppp: f32, dock_width: f32) -> Harness<'static, PhotocraftApp> {
    let mut h = Harness::builder().with_size(vec2(1440.0, 1000.0)).with_pixels_per_point(ppp).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(session, crate::Services::default())
    });
    let ctx = h.ctx.clone();
    let (req, _rx) = crate::control::ControlRequest::new(
        "ui.set",
        json!({"theme": "promedium", "dockWidth": dock_width, "dockTabs": {"layers": tab}, "dock": {"collapsed": ["color", "properties", "history", "navigator"]}}),
    );
    crate::control::handle(h.state_mut(), &ctx, &req);
    h.run_steps(8);
    h
}

fn click_with(h: &mut Harness<'_, PhotocraftApp>, at: Pos2, modifiers: Modifiers) {
    h.hover_at(at);
    h.event(egui::Event::ModifiersChanged(modifiers));
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers });
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers });
    h.run_steps(1);
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

fn layer(h: &Harness<'_, PhotocraftApp>, id: u64) -> photocraft_doc::Layer {
    let st = h.state().session.active().unwrap();
    st.doc.layer(photocraft_doc::LayerId(id)).unwrap().clone()
}

#[test]
fn the_chain_toggles_linking_and_it_survives_psd() {
    let (s, masked, _) = session();
    let mut h = harness(s, 0, 1.0, 290.0);
    let p = h.get_by_label("Unlink layer mask of Masked").rect().center();
    click_with(&mut h, p, Modifiers::NONE);
    assert!(!layer(&h, masked).mask.unwrap().linked);
    let p = h.get_by_label("Unlink vector mask of Masked").rect().center();
    click_with(&mut h, p, Modifiers::NONE);
    assert!(!layer(&h, masked).vector_mask.unwrap().linked);
    let doc = (*h.state().session.active().unwrap().doc).clone();
    let bytes = photocraft_io::export(&doc, "m.psd", &Default::default()).unwrap().bytes;
    let back = photocraft_io::import("m.psd", &bytes).unwrap().document;
    let l = back.walk().into_iter().map(|(_, _, l)| l.clone()).find(|l| l.name == "Masked").unwrap();
    assert!(!l.mask.unwrap().linked && !l.vector_mask.unwrap().linked, "unlinked state round-trips PSD");
    // Clicking the (now empty) chain slot links again.
    let p = h.get_by_label("Link layer mask of Masked").rect().center();
    click_with(&mut h, p, Modifiers::NONE);
    assert!(layer(&h, masked).mask.unwrap().linked);
}

#[test]
fn shift_click_disables_a_mask() {
    let (s, masked, _) = session();
    let mut h = harness(s, 0, 1.0, 290.0);
    for kind in [MaskKind::Pixel, MaskKind::Vector] {
        let r = mask_rect(&h, masked, kind);
        click_with(&mut h, r.center(), Modifiers::SHIFT);
        h.run_steps(2);
    }
    let l = layer(&h, masked);
    assert!(!l.mask.unwrap().enabled && !l.vector_mask.unwrap().enabled);
}

#[test]
fn vector_thumbnail_is_white_inside_grey_outside_at_the_document_aspect() {
    let (s, masked, _) = session();
    let doc = s.active().unwrap().doc.clone();
    let vm = doc.layer(photocraft_doc::LayerId(masked)).unwrap().vector_mask.clone().unwrap();
    let img = vector_thumb_image(&doc, &vm, 64);
    let at = |x: usize, y: usize| img.pixels[y * 64 + x];
    // 200×100 doc in 64 px: 0.32 scale, rows 16..48. The mask covers x 20..120, y 10..90.
    assert_eq!(at(20, 32), egui::Color32::from_gray(255), "inside reveals");
    assert_eq!(at(55, 32), egui::Color32::from_gray(128), "outside hides");
    assert_eq!(at(20, 4), egui::Color32::TRANSPARENT, "letterbox");
}

#[test]
fn channels_list_the_layer_mask_and_keep_the_aspect() {
    let (s, masked, _) = session();
    let mut h = harness(s, 1, 1.0, 290.0);
    let rows = crate::channels_panel::recorded(&h.ctx);
    let names: Vec<_> = rows.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["RGB", "Red", "Green", "Blue", "Masked Mask"], "the mask is a temporary row below the colour channels");
    for (n, r) in &rows {
        assert!((r.width() / r.height() - 2.0).abs() < 0.05, "{n}: 2:1 thumbnail, got {r:?}");
    }
    assert!(!h.state().ui.mask_target);
    let p = h.get_by_label("Masked Mask").rect().center();
    click_with(&mut h, p, Modifiers::NONE);
    assert!(h.state().ui.mask_target, "clicking the mask row targets the mask");
    assert_eq!(crate::canvas::paint_target(h.state()), json!("mask"));
    h.run_steps(2);
    let p = h.get_by_label("RGB").rect().center();
    click_with(&mut h, p, Modifiers::NONE);
    assert!(!h.state().ui.mask_target, "the composite targets the pixels again");
    // A layer without a mask has no mask row.
    h.state_mut().run("layer.layerMask.delete", json!({"layer": masked})).unwrap();
    h.run_steps(3);
    assert!(crate::channels_panel::recorded(&h.ctx).iter().all(|(n, _)| !n.ends_with("Mask")));
}

#[test]
fn paths_list_the_layer_path_above_a_bottom_footer() {
    let (s, masked, shape) = session();
    let mut h = harness(s, 2, 1.0, 290.0);
    let row = h.get_by_label("Masked Vector Mask").rect();
    let footer = crate::vector_ui::paths_footer(&h.ctx).expect("footer drawn");
    assert!(footer.top() > row.bottom(), "buttons below the rows");
    let group = crate::panel_docking::probe::group_rect(h.state(), &h.ctx, "paths").unwrap();
    assert!(group.bottom() - footer.bottom() < 16.0, "footer at the panel bottom: footer {footer:?}, group {group:?}");
    assert!(footer.bottom() - row.bottom() > 100.0, "the footer is pinned, not right below the rows");
    h.state_mut().run("layer.select", json!({"layer": shape})).unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("Badge Shape Path").is_some());
    assert!(h.query_by_label("Masked Vector Mask").is_none(), "only the selected layer's path");
    // Selecting the temporary row targets the layer's path: Load as selection uses it.
    let p = h.get_by_label("Badge Shape Path").rect().center();
    click_with(&mut h, p, Modifiers::NONE);
    assert_eq!(h.state().ui.selected_path.as_deref(), Some("layer"));
    // Footer: fill, stroke, load as selection, … (26 pt buttons, 2 pt apart).
    let footer = crate::vector_ui::paths_footer(&h.ctx).unwrap();
    let p = pos2(footer.left() + 2.0 * 28.0 + 13.0, footer.center().y);
    click_with(&mut h, p, Modifiers::NONE);
    let sel = h.state().session.active().unwrap().doc.selection.clone().expect("selection from the shape path");
    let b = sel.content_bounds();
    assert!(b.x0.abs_diff(140) <= 1 && b.width().abs_diff(40) <= 1, "{b:?}");
    let _ = masked;
}

#[test]
fn panels_never_panic_on_odd_documents() {
    // Rows render for a 1×1 and a very wide document, and the fitted thumbnails stay finite.
    for (w, hgt) in [(1, 1), (2000, 3)] {
        for tab in 0..3 {
            let mut s = photocraft_engine::Session::new();
            s.execute("file.new", json!({"width": w, "height": hgt})).unwrap();
            s.execute("layer.new.layer", json!({})).unwrap();
            s.execute("layer.layerMask.hideAll", json!({})).unwrap();
            s.execute("layer.vectorMask.revealAll", json!({})).unwrap();
            let _ = harness(s, tab, 1.0, 250.0);
        }
    }
    let (r, uv) = crate::channels_panel::fit_thumb(Rect::from_min_size(pos2(0.0, 0.0), vec2(28.0, 28.0)), 0, 0);
    assert!(r.is_finite() && uv.is_finite() && r.width() > 0.0);
}

// ---------------------------------------------------------------------------------------------
// #196: ⌥-click mask view, ⌘-click vector mask to select, the vector mask as target.

/// The session above with a black dot painted into the layer mask at (50, 50).
fn dotted() -> (photocraft_engine::Session, u64, u64) {
    let (mut s, masked, shape) = session();
    s.execute("paint.stroke", json!({"points": [[50, 50]], "size": 20, "hardness": 1.0, "color": "#000000", "target": "mask"})).unwrap();
    (s, masked, shape)
}

fn name(h: &Harness<'_, PhotocraftApp>, id: u64) -> String {
    layer(h, id).name
}

fn mask_rect(h: &Harness<'_, PhotocraftApp>, id: u64, kind: MaskKind) -> Rect {
    let part = if kind == MaskKind::Pixel { LayerPart::LayerMask } else { LayerPart::VectorMask };
    crate::layers_panel_nodes::thumb(h, &name(h, id), part)
}

fn layer_thumb_center(h: &Harness<'_, PhotocraftApp>, id: u64) -> Pos2 {
    crate::layers_panel_nodes::thumb(h, &name(h, id), LayerPart::Pixels).center()
}

fn mask_view(h: &Harness<'_, PhotocraftApp>) -> serde_json::Value {
    photocraft_engine::inspect::document(h.state().session.active().unwrap())["layerMaskView"].clone()
}

fn canvas_px(h: &Harness<'_, PhotocraftApp>) -> Option<Vec<egui::Color32>> {
    let st = h.state().session.active().unwrap();
    let look = crate::channel_view::Look::of(&h.state().session);
    crate::channel_view::render(&st.doc, &st.channel_view, photocraft_geom::Rect::new(0, 0, 200, 100), 1, look, st.active_layer)
}

#[test]
fn alt_click_views_the_mask_in_gray_and_the_layer_thumbnail_returns() {
    for ppp in [1.0, 2.0] {
        let (s, masked, _) = dotted();
        let mut h = harness(s, 0, ppp, 290.0);
        let doc = h.state().session.active().unwrap().doc.clone();
        let r = mask_rect(&h, masked, MaskKind::Pixel);
        click_with(&mut h, r.center(), Modifiers::ALT);
        assert_eq!(mask_view(&h), json!({"layer": masked, "mode": "gray"}), "@{ppp}x");
        assert!(h.state().ui.mask_target, "viewing the mask targets it");
        assert_eq!(crate::canvas::paint_target(h.state()), json!("mask"), "painting paints the mask");
        let px = canvas_px(&h).expect("the canvas shows the mask");
        assert_eq!(px[50 * 200 + 50], egui::Color32::BLACK, "hidden reads black");
        assert_eq!(px[10 * 200 + 150], egui::Color32::WHITE, "revealed reads white");
        // ⌥-click again: back to the composite.
        click_with(&mut h, r.center(), Modifiers::ALT);
        assert_eq!(mask_view(&h), serde_json::Value::Null);
        assert!(canvas_px(&h).is_none());
        // Into mask view again, then out by clicking the layer thumbnail.
        click_with(&mut h, r.center(), Modifiers::ALT);
        assert_eq!(mask_view(&h)["mode"], "gray");
        let p = layer_thumb_center(&h, masked);
        click_with(&mut h, p, Modifiers::NONE);
        assert_eq!(mask_view(&h), serde_json::Value::Null, "@{ppp}x the layer thumbnail returns");
        assert!(!h.state().ui.mask_target, "and targets the pixels");
        assert!(std::sync::Arc::ptr_eq(&doc, &h.state().session.active().unwrap().doc), "viewing never touched the document");
    }
}

#[test]
fn shift_alt_click_shows_a_rubylith_and_the_channels_eye_toggles_it() {
    let (s, masked, _) = dotted();
    let mut h = harness(s, 0, 1.0, 290.0);
    let r = mask_rect(&h, masked, MaskKind::Pixel);
    click_with(&mut h, r.center(), Modifiers::SHIFT | Modifiers::ALT);
    assert_eq!(mask_view(&h), json!({"layer": masked, "mode": "overlay"}));
    let px = canvas_px(&h).unwrap();
    assert_eq!(px[50 * 200 + 50], egui::Color32::from_rgba_premultiplied(128, 0, 0, 128), "50% red over hidden areas");
    assert_eq!(px[10 * 200 + 150], egui::Color32::TRANSPARENT, "the composite shows through elsewhere");
    assert!(layer(&h, masked).mask.unwrap().enabled, "⇧⌥ doesn't disable the mask");
    // Channels: the mask row's eye shows and hides the overlay.
    let ctx = h.ctx.clone();
    let (req, _rx) = crate::control::ControlRequest::new("ui.set", json!({"dockTabs": {"layers": 1}}));
    crate::control::handle(h.state_mut(), &ctx, &req);
    h.run_steps(4);
    let p = h.get_by_label("Visibility Masked Mask").rect().center();
    click_with(&mut h, p, Modifiers::NONE);
    assert_eq!(mask_view(&h), serde_json::Value::Null, "the eye hides the overlay");
    let p = h.get_by_label("Visibility Masked Mask").rect().center();
    click_with(&mut h, p, Modifiers::NONE);
    assert_eq!(mask_view(&h)["mode"], "overlay", "and shows it again");
    // In gray view the composite's eye brings the composite back under the overlay.
    h.state_mut().run("view.layerMask", json!({"mode": "gray"})).unwrap();
    h.run_steps(3);
    let p = h.get_by_label("Visibility RGB").rect().center();
    click_with(&mut h, p, Modifiers::NONE);
    assert_eq!(mask_view(&h)["mode"], "overlay");
}

#[test]
fn command_click_a_vector_mask_loads_its_path_as_a_selection() {
    let (s, masked, _) = session();
    let mut h = harness(s, 0, 1.0, 290.0);
    let r = mask_rect(&h, masked, MaskKind::Vector);
    let sel = |h: &Harness<'_, PhotocraftApp>| h.state().session.active().unwrap().doc.selection.clone();
    click_with(&mut h, r.center(), Modifiers::COMMAND);
    let b = sel(&h).expect("⌘-click selects the path").content_bounds();
    assert_eq!((b.x0, b.y0, b.width(), b.height()), (20, 10, 100, 80));
    assert!(!h.state().ui.vector_mask_target, "⌘-click doesn't retarget");
    // ⌘⇧ add a 4×4 square, ⌘⌥ subtract, ⌘⇧⌥ intersect.
    h.state_mut().run("select.rect", json!({"x": 150, "y": 50, "width": 4, "height": 4})).unwrap();
    click_with(&mut h, r.center(), Modifiers::COMMAND | Modifiers::SHIFT);
    let s1 = sel(&h).unwrap();
    assert!(s1.sample_channel(60, 50, 0) > 0.99 && s1.sample_channel(151, 51, 0) > 0.99, "added");
    h.state_mut().run("select.all", json!({})).unwrap();
    click_with(&mut h, r.center(), Modifiers::COMMAND | Modifiers::ALT);
    let s2 = sel(&h).unwrap();
    assert!(s2.sample_channel(60, 50, 0) < 0.01 && s2.sample_channel(150, 50, 0) > 0.99, "subtracted");
    h.state_mut().run("select.rect", json!({"x": 100, "y": 0, "width": 100, "height": 100})).unwrap();
    click_with(&mut h, r.center(), Modifiers::COMMAND | Modifiers::SHIFT | Modifiers::ALT);
    let b = sel(&h).unwrap().content_bounds();
    assert_eq!((b.x0, b.y0, b.width(), b.height()), (100, 10, 20, 80), "intersected");
}

#[test]
fn clicking_the_vector_mask_targets_it_for_the_path_tools() {
    let (s, masked, _) = session();
    let mut h = harness(s, 0, 1.0, 290.0);
    let r = mask_rect(&h, masked, MaskKind::Vector);
    click_with(&mut h, r.center(), Modifiers::NONE);
    assert!(h.state().ui.vector_mask_target && !h.state().ui.mask_target);
    assert_eq!(h.state().ui.selected_path.as_deref(), Some("layer"), "the Paths panel selects its path");
    assert_eq!(crate::canvas::paint_target(h.state()), json!("pixels"));
    // Path Selection moves the vector mask.
    crate::vector_ui::path_selection_finish(h.state_mut(), [30.0, 30.0], [35.0, 32.0]);
    let k = layer(&h, masked).vector_mask.unwrap().path.subpaths[0].knots[0].anchor;
    assert_eq!((k.x, k.y), (25.0, 12.0));
    // The Pen adds a subpath to it.
    let owner = crate::vector_ui::capture_pen_owner(h.state());
    h.state_mut().ui.pen =
        Some(crate::vector_ui::PenPath { knots: vec![[[150.0, 10.0]; 3], [[190.0, 10.0]; 3], [[190.0, 40.0]; 3]], owner, ..Default::default() });
    crate::vector_ui::pen_commit(h.state_mut(), true);
    assert_eq!(layer(&h, masked).vector_mask.unwrap().path.subpaths.len(), 2);
    assert!(h.state().session.active().unwrap().doc.work_path.is_none(), "not a work path");
    h.run_steps(2);
    // The pixel mask and the layer thumbnail take the target back.
    let m = mask_rect(&h, masked, MaskKind::Pixel);
    click_with(&mut h, m.center(), Modifiers::NONE);
    assert!(h.state().ui.mask_target && !h.state().ui.vector_mask_target);
    click_with(&mut h, r.center(), Modifiers::NONE);
    let p = layer_thumb_center(&h, masked);
    click_with(&mut h, p, Modifiers::NONE);
    assert!(!h.state().ui.mask_target && !h.state().ui.vector_mask_target);
    // Deleting the vector mask drops the target.
    click_with(&mut h, r.center(), Modifiers::NONE);
    assert!(h.state().ui.vector_mask_target);
    h.state_mut().run("layer.vectorMask.delete", json!({"layer": masked})).unwrap();
    h.run_steps(2);
    assert!(!h.state().ui.vector_mask_target);
}

#[test]
fn a_targeted_vector_mask_flips_its_path_not_the_layer_pixels() {
    let (s, masked, _) = session();
    let mut h = harness(s, 0, 1.0, 290.0);
    let r = mask_rect(&h, masked, MaskKind::Vector);
    click_with(&mut h, r.center(), Modifiers::NONE);
    assert!(h.state().ui.vector_mask_target);
    let before = layer(&h, masked);
    h.state_mut().run("edit.transform.flipHorizontal", json!({})).unwrap();
    let after = layer(&h, masked);
    let path = after.vector_mask.as_ref().unwrap().path.clone();
    let k = &path.subpaths[0].knots;
    assert_eq!((k[0].anchor.x, k[0].anchor.y), (120.0, 10.0), "mirrored about the path's bounds (x = 140)");
    assert_eq!(after.surface().unwrap().content_bounds(), before.surface().unwrap().content_bounds(), "the pixels are untouched");
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(layer(&h, masked).vector_mask.unwrap().path, before.vector_mask.unwrap().path, "undo restores the path");
}

/// #12283: the panel's vector-mask target must not override an alpha channel or Quick Mask (the
/// engine routes those), and an explicit target in the params always wins.
#[test]
fn a_targeted_vector_mask_yields_to_alpha_quick_mask_and_explicit_targets() {
    let (s, masked, _) = session();
    let mut h = harness(s, 0, 1.0, 290.0);
    let r = mask_rect(&h, masked, MaskKind::Vector);
    click_with(&mut h, r.center(), Modifiers::NONE);
    assert!(h.state().ui.vector_mask_target && h.state().vector_mask_targeted());
    let target = |h: &Harness<'_, PhotocraftApp>, p: serde_json::Value| h.state().with_mask_target("edit.transform.flipHorizontal", p);
    assert_eq!(target(&h, json!({}))["target"], "vectorMask");
    assert_eq!(target(&h, json!({"target": "pixels"}))["target"], "pixels", "an explicit target wins");
    let before = layer(&h, masked).vector_mask.unwrap().path;

    // Photoshop targets a new alpha channel on its own; the panel still says vector mask.
    h.state_mut().run("channel.new", json!({"name": "Alpha", "fill": "white"})).unwrap();
    assert!(h.state().ui.vector_mask_target, "the panel keeps its vector-mask flag");
    assert_eq!(h.state().session.active().unwrap().channel_view.target, photocraft_engine::channel_cmds::ChannelTarget::Alpha(0));
    assert!(target(&h, json!({})).get("target").is_none(), "the alpha channel wins");
    h.state_mut().run("edit.transform.flipHorizontal", json!({})).unwrap();
    assert_eq!(layer(&h, masked).vector_mask.unwrap().path, before, "the vector mask stays put");

    // Quick Mask mode wins too, even with the stale panel flag.
    h.state_mut().run("select.editInQuickMaskMode", json!({"on": true})).unwrap();
    assert!(h.state().ui.vector_mask_target && !h.state().vector_mask_targeted());
    assert!(target(&h, json!({})).get("target").is_none(), "the Quick Mask wins");
    h.state_mut().run("edit.transform.flipHorizontal", json!({})).unwrap();
    assert_eq!(layer(&h, masked).vector_mask.unwrap().path, before, "the vector mask stays put");
}

#[test]
fn mask_view_gestures_fail_gracefully_without_a_mask() {
    let (s, masked, shape) = session();
    let mut h = harness(s, 0, 1.0, 290.0);
    // ⌥-click on a vector mask is not a mask view.
    let p = mask_rect(&h, masked, MaskKind::Vector).center();
    click_with(&mut h, p, Modifiers::ALT);
    assert_eq!(mask_view(&h), serde_json::Value::Null);
    // A layer without a layer mask: the command fails, nothing changes.
    assert!(h.state_mut().run("view.layerMask", json!({"layer": shape, "mode": "gray"})).is_err());
    assert_eq!(mask_view(&h), serde_json::Value::Null);
    h.run_steps(2);
    // A stale view (layer gone) draws nothing instead of panicking.
    h.state_mut().session.active_mut().unwrap().channel_view.layer_mask = Some(photocraft_engine::mask_view_cmds::LayerMaskView {
        layer: photocraft_doc::LayerId(u64::MAX),
        mode: photocraft_engine::mask_view_cmds::MaskViewMode::Gray,
        filter: false,
    });
    assert!(canvas_px(&h).is_none());
    h.run_steps(2);
    // The mask goes while it is shown: the view ends.
    h.state_mut().run("view.layerMask", json!({"layer": masked, "mode": "gray"})).unwrap();
    h.state_mut().run("layer.layerMask.delete", json!({"layer": masked})).unwrap();
    h.run_steps(2);
    assert_eq!(mask_view(&h), serde_json::Value::Null);
    assert_eq!(crate::canvas::paint_target(h.state()), json!("pixels"));
}

/// #1765: `\` shows the active layer's mask as a rubylith and hides it again, as in Photoshop;
/// the mask stays the paint target meanwhile. On a layer without a mask it does nothing, and a
/// focused text field keeps the key.
#[test]
fn backslash_toggles_the_rubylith() {
    let (s, masked, shape) = dotted();
    let mut h = harness(s, 0, 1.0, 290.0);
    let doc = h.state().session.active().unwrap().doc.clone();
    h.key_press(egui::Key::Backslash);
    h.run_steps(2);
    assert_eq!(mask_view(&h), json!({"layer": masked, "mode": "overlay"}), "\\ shows the overlay");
    assert_eq!(crate::canvas::paint_target(h.state()), json!("mask"), "painting paints the mask");
    let px = canvas_px(&h).unwrap();
    assert_eq!(px[50 * 200 + 50], egui::Color32::from_rgba_premultiplied(128, 0, 0, 128), "50% red over hidden areas");
    // Layer Mask Display Options: opaque green over what the mask hides instead.
    h.state_mut().run("channel.options", json!({"channel": "mask", "color": "#00ff00", "opacity": 100})).unwrap();
    assert_eq!(canvas_px(&h).unwrap()[50 * 200 + 50], egui::Color32::from_rgb(0, 255, 0));
    h.key_press(egui::Key::Backslash);
    h.run_steps(2);
    assert_eq!(mask_view(&h), serde_json::Value::Null, "\\ again hides it");
    assert!(std::sync::Arc::ptr_eq(&doc, &h.state().session.active().unwrap().doc), "a view toggle, not an edit");
    // A layer without a layer mask: nothing happens.
    h.state_mut().run("layer.select", json!({"layer": shape})).unwrap();
    h.run_steps(2);
    h.key_press(egui::Key::Backslash);
    h.run_steps(2);
    assert_eq!(mask_view(&h), serde_json::Value::Null);
    let sc = crate::shortcuts::parse("\\").unwrap();
    assert!(!crate::shortcut_dispatch::Focus::Text.allows(&sc), "typing \\ in a text field stays text");
}

/// #780: clicking the mask thumbnail targets the mask, and ⌘I (Image › Adjustments › Invert)
/// then inverts the mask instead of the layer; on an adjustment layer the menu item is live.
#[test]
fn command_i_inverts_the_targeted_mask() {
    let (s, masked, _) = dotted();
    let mut h = harness(s, 0, 1.0, 290.0);
    let pixels = layer(&h, masked).content;
    let r = mask_rect(&h, masked, MaskKind::Pixel);
    click_with(&mut h, r.center(), Modifiers::NONE);
    assert!(h.state().ui.mask_target);
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::I);
    h.run_steps(2);
    let l = layer(&h, masked);
    let mask = &l.mask.as_ref().unwrap().surface;
    assert_eq!((mask.sample_channel(50, 50, 0), mask.sample_channel(150, 10, 0)), (1.0, 0.0), "the mask inverted");
    assert!(l.content == pixels, "the layer's pixels are untouched");
    // An adjustment layer's mask: with no pixels of its own, Invert edits the mask whether the
    // panel targets the mask or the icon thumbnail (#4298).
    let app = h.state_mut();
    app.run("layer.newAdjustmentLayer.curves", json!({})).unwrap();
    app.run("layer.layerMask.revealAll", json!({})).unwrap();
    app.ui.mask_target = false;
    assert!(crate::menus::is_enabled(app, "image.adjustments.invert"));
    app.ui.mask_target = true;
    assert!(crate::menus::is_enabled(app, "image.adjustments.invert"));
    app.run("image.adjustments.invert", json!({})).unwrap();
    let st = app.session.active().unwrap();
    let mask = &st.doc.layer(st.active_layer.unwrap()).unwrap().mask.as_ref().unwrap().surface;
    assert_eq!(mask.sample_channel(10, 10, 0), 0.0, "reveal all → hide all");
}

/// #2420: Selecting a layer mask in the Layers panel toggles the color swatches
/// to the mask's colors (white/black by default), and deselecting toggles back.
#[test]
fn selecting_mask_thumbnail_toggles_swatches_to_mask_colors_and_back() {
    const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
    const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
    const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

    let (mut s, masked, _) = dotted();
    s.execute("tools.setColors", json!({"foreground": "#ff0000", "background": "#0000ff"})).unwrap();
    let mut h = harness(s, 0, 1.0, 290.0);

    // Initial state: layer pixels targeted, red foreground, blue background.
    assert!(!h.state().ui.mask_target);
    assert_eq!(h.state().session.tools.foreground, RED);
    assert_eq!(h.state().session.tools.background, BLUE);

    // 1. Click mask thumbnail in Layers panel -> targets mask, swatches toggle to mask colors.
    let r_mask = mask_rect(&h, masked, MaskKind::Pixel);
    click_with(&mut h, r_mask.center(), Modifiers::NONE);
    assert!(h.state().ui.mask_target, "mask thumbnail clicked: mask is targeted");
    assert_eq!(h.state().session.tools.foreground, WHITE);
    assert_eq!(h.state().session.tools.background, BLACK);

    // 2. Click layer thumbnail -> deselects mask, swatches toggle back to pixel colors.
    let r_layer = layer_thumb_center(&h, masked);
    click_with(&mut h, r_layer, Modifiers::NONE);
    assert!(!h.state().ui.mask_target, "layer thumbnail clicked: pixels are targeted");
    assert_eq!(h.state().session.tools.foreground, RED);
    assert_eq!(h.state().session.tools.background, BLUE);

    // 3. Click mask thumbnail again -> swatches toggle back to mask colors.
    click_with(&mut h, r_mask.center(), Modifiers::NONE);
    assert!(h.state().ui.mask_target);
    assert_eq!(h.state().session.tools.foreground, WHITE);
    assert_eq!(h.state().session.tools.background, BLACK);

    // 4. Changing mask colors preserves them when switching away and back.
    h.state_mut().run("tools.setColors", json!({"foreground": "#808080"})).unwrap();
    let grey = h.state().session.tools.foreground;
    assert_eq!(grey, [128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0, 1.0]);

    click_with(&mut h, r_layer, Modifiers::NONE);
    assert_eq!(h.state().session.tools.foreground, RED, "pixel colors restored");

    click_with(&mut h, r_mask.center(), Modifiers::NONE);
    assert_eq!(h.state().session.tools.foreground, grey, "mask colors restored");

    // 5. Creating a new adjustment layer targets its mask and toggles to mask colors.
    click_with(&mut h, r_layer, Modifiers::NONE);
    assert_eq!(h.state().session.tools.foreground, RED);
    h.state_mut().run("layer.newAdjustmentLayer.curves", json!({})).unwrap();
    assert!(h.state().ui.mask_target, "new adjustment layer targets its mask");
    assert_eq!(h.state().session.tools.foreground, grey);
}

// #3369: a new adjustment or fill layer targets its mask, but Properties shows its settings
// until the mask thumbnail is clicked, as in Photoshop.
#[test]
fn a_new_adjustment_or_fill_layer_shows_its_settings_until_the_mask_thumbnail_is_clicked() {
    for (command, settings) in [("layer.newAdjustmentLayer.curves", Some("Reset to defaults")), ("layer.newFillLayer.solidColor", None)] {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": 200, "height": 100})).unwrap();
        s.execute("layer.new.layer", json!({})).unwrap();
        let mut h = harness(s, 0, 1.0, 290.0);
        let ctx = h.ctx.clone();
        let (req, _rx) = crate::control::ControlRequest::new("ui.set", json!({"dock": {"collapsed": ["color", "history", "navigator"]}}));
        crate::control::handle(h.state_mut(), &ctx, &req);
        h.state_mut().run(command, json!({})).unwrap();
        h.run_steps(6);
        let id = h.state().session.active().unwrap().active_layer.unwrap().0;
        assert!(h.state().ui.mask_target, "{command}: the new mask is targeted");
        assert_eq!(crate::canvas::paint_target(h.state()), json!("mask"), "{command}: painting edits the mask");
        assert!(h.query_all_by_label("Density").next().is_none(), "{command}: Properties shows the layer, not its mask");
        if let Some(label) = settings {
            assert!(h.query_all_by_label(label).next().is_some(), "{command}: its settings show");
        }
        // Clicking the row (not a thumbnail) keeps the settings.
        let row = crate::layers_panel_nodes::row(&h, &name(&h, id)).center();
        click_with(&mut h, row, Modifiers::NONE);
        assert!(h.state().ui.mask_target);
        assert!(h.query_all_by_label("Density").next().is_none(), "{command}: a row click keeps the settings");
        // The mask thumbnail switches Properties to the mask.
        let r = mask_rect(&h, id, MaskKind::Pixel);
        click_with(&mut h, r.center(), Modifiers::NONE);
        h.run_steps(2);
        assert!(h.state().ui.mask_target);
        assert!(h.query_all_by_label("Density").next().is_some(), "{command}: the mask thumbnail shows the mask controls");
        if let Some(label) = settings {
            assert!(h.query_all_by_label(label).next().is_none());
        }
    }
}

// #4028: with Adjustments showing in a collapsed Properties group, a new adjustment layer brings
// Properties to the front and expands it, as in Photoshop, so its settings can be edited at once.
#[test]
fn a_new_adjustment_layer_brings_properties_to_the_front() {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 200, "height": 100})).unwrap();
    let mut h = harness(s, 0, 1.0, 290.0);
    let ctx = h.ctx.clone();
    let (req, _rx) = crate::control::ControlRequest::new("ui.set", json!({"dockTabs": {"properties": 1}}));
    crate::control::handle(h.state_mut(), &ctx, &req);
    h.run_steps(2);
    assert!(h.state().ui.dock.is_collapsed(crate::dock::Group::Properties));
    h.state_mut().run("layer.newAdjustmentLayer.curves", json!({})).unwrap();
    h.run_steps(6);
    assert_eq!(h.state().ui.dock_tabs.properties, 0, "Properties is the group's tab");
    assert!(!h.state().ui.dock.is_collapsed(crate::dock::Group::Properties), "the group is expanded");
    assert!(h.query_all_by_label("Reset to defaults").next().is_some(), "the Curves settings show");
}

#[test]
fn selecting_an_adjustment_layer_from_another_layer_targets_its_mask() {
    // The reference application: an adjustment layer picked by its name targets its mask; a
    // masked pixel layer targets its contents; the content thumbnail still targets contents.
    for command in ["layer.newAdjustmentLayer.hueSaturation", "layer.newAdjustmentLayer.curves"] {
        for ppp in [1.0, 2.0] {
            let (mut s, masked, other) = session();
            s.execute(command, json!({})).unwrap();
            let id = s.active().unwrap().active_layer.unwrap().0;
            let path = json!({"subpaths": [{"knots": [[20, 10], [120, 10], [120, 90], [20, 90]]}]});
            s.execute("layer.vectorMask.add", json!({"layer": id, "path": path})).unwrap();
            let mut h = harness(s, 0, ppp, 360.0);
            assert!(layer(&h, id).mask.is_some(), "{command}: the adjustment layer has a mask");
            let name_of = |h: &Harness<'_, PhotocraftApp>, l: u64| crate::layers_panel_nodes::row(h, &name(h, l)).center();
            // Each mask thumbnail targets its own mask.
            for kind in [MaskKind::Pixel, MaskKind::Vector] {
                let at = mask_rect(&h, id, kind).center();
                click_with(&mut h, at, Modifiers::NONE);
                assert_eq!(h.state().ui.mask_target, kind == MaskKind::Pixel, "{command} @{ppp}x {kind:?}");
                assert_eq!(h.state().ui.vector_mask_target, kind == MaskKind::Vector, "{command} @{ppp}x {kind:?}");
            }
            let at = name_of(&h, other);
            click_with(&mut h, at, Modifiers::NONE);
            let at = name_of(&h, id);
            click_with(&mut h, at, Modifiers::NONE);
            assert_eq!(h.state().session.active().unwrap().active_layer.unwrap().0, id);
            assert!(h.state().ui.mask_target && !h.state().ui.vector_mask_target, "{command} @{ppp}x: a name click targets the mask");
            assert_eq!(crate::canvas::paint_target(h.state()), json!("mask"));
            // Fill goes to the mask and leaves the adjustment as it is.
            let content = layer(&h, id).content;
            h.state_mut().run("edit.fill", json!({"contents": "black"})).unwrap();
            assert_eq!(layer(&h, id).content, content, "{command} @{ppp}x: filling must preserve the adjustment");
            assert_eq!(layer(&h, id).mask.unwrap().surface.sample_channel(50, 50, 0), 0.0, "{command} @{ppp}x: Fill edits the mask");
            // The content thumbnail targets the contents.
            let thumb = layer_thumb_center(&h, id);
            click_with(&mut h, thumb, Modifiers::NONE);
            assert!(!h.state().ui.mask_target && !h.state().ui.vector_mask_target, "{command} @{ppp}x: the content thumbnail targets contents");
            // A masked pixel layer picked by its name targets its contents.
            let at = name_of(&h, masked);
            click_with(&mut h, at, Modifiers::NONE);
            assert_eq!(h.state().session.active().unwrap().active_layer.unwrap().0, masked);
            assert!(!h.state().ui.mask_target && !h.state().ui.vector_mask_target, "{command} @{ppp}x: a pixel layer targets its contents");
        }
    }
}

/// #10485: dragging the layer mask's thumbnail onto a group's row moves the mask to the group, and
/// ⌥ copies it, as in Photoshop. It had no effect.
#[test]
fn dragging_the_mask_thumbnail_onto_a_group_moves_or_copies_it() {
    for copy in [false, true] {
        let (mut s, masked, shape) = session();
        let group = s.execute("layer.groupLayers", json!({"layer": shape, "name": "Grp"})).unwrap()["layer"].as_u64().unwrap();
        s.execute("layer.select", json!({"layer": masked})).unwrap();
        let mut h = harness(s, 0, 1.0, 290.0);
        let from = mask_rect(&h, masked, MaskKind::Pixel).center();
        let to = crate::layers_panel_nodes::row(&h, "Grp").center();
        let modifiers = if copy { Modifiers::ALT } else { Modifiers::NONE };
        h.hover_at(from);
        h.event(egui::Event::ModifiersChanged(modifiers));
        h.run_steps(1);
        h.event(egui::Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers });
        h.run_steps(1);
        for k in 1..=8 {
            h.event(egui::Event::PointerMoved(from + (to - from) * (k as f32 / 8.0)));
            h.run_steps(1);
        }
        h.event(egui::Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers });
        h.run_steps(3);
        assert!(layer(&h, group).mask.is_some(), "copy: {copy}: the group has the mask");
        assert_eq!(layer(&h, masked).mask.is_some(), copy, "copy: {copy}: the original stays only with ⌥");
    }
}

/// #10485: a layer mask's thumbnail dropped on a row that has a layer mask already (with or
/// without ⌥), or back on its own row, does nothing: no command runs, so no error, no history step,
/// and both masks stay as they were. Photoshop asks before replacing a mask; the panel doesn't offer
/// the drop at all.
#[test]
fn dropping_a_mask_thumbnail_on_a_masked_row_or_its_own_row_does_nothing() {
    for (target, copy) in [("Badge", false), ("Badge", true), ("Masked", false), ("Masked", true)] {
        let (mut s, masked, shape) = session();
        s.execute("layer.layerMask.hideAll", json!({"layer": shape})).unwrap();
        s.execute("layer.select", json!({"layer": masked})).unwrap();
        let mut h = harness(s, 0, 1.0, 290.0);
        let doc = h.state().session.active().unwrap().doc.clone();
        let history = h.state().session.active().unwrap().history.entries();
        let (mask_masked, mask_shape) = (layer(&h, masked).mask, layer(&h, shape).mask);
        let from = mask_rect(&h, masked, MaskKind::Pixel).center();
        let to = crate::layers_panel_nodes::row(&h, target).center();
        let modifiers = if copy { Modifiers::ALT } else { Modifiers::NONE };
        h.hover_at(from);
        h.event(egui::Event::ModifiersChanged(modifiers));
        h.run_steps(1);
        h.event(egui::Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers });
        h.run_steps(1);
        // Out of the thumbnail and back onto the target, so the drop on its own row is a drag.
        let away = from + vec2(0.0, 40.0);
        for k in 1..=4 {
            h.event(egui::Event::PointerMoved(from + (away - from) * (k as f32 / 4.0)));
            h.run_steps(1);
        }
        for k in 1..=8 {
            h.event(egui::Event::PointerMoved(away + (to - away) * (k as f32 / 8.0)));
            h.run_steps(1);
        }
        h.event(egui::Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers });
        h.run_steps(3);
        let case = format!("onto {target}, copy: {copy}");
        assert!(!h.state().ui.status_error, "{case}: no refused command: {}", h.state().ui.status);
        assert_eq!(layer(&h, masked).mask, mask_masked, "{case}: the dragged mask stays");
        assert_eq!(layer(&h, shape).mask, mask_shape, "{case}: the target keeps its own mask");
        let st = h.state().session.active().unwrap();
        assert!(st.doc == doc, "{case}: the document is unchanged");
        assert_eq!(st.history.entries(), history, "{case}: no history step");
    }
}
