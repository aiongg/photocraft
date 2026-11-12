//! Layers panel Smart Filters rows (#4271): a click selects a filter (and its smart object), a
//! drag onto another smart object moves it there, ⌥-drag copies it, and a drag onto Delete
//! removes it, as in Photoshop.

use egui::accesskit::Role;
use egui::{Modifiers, Pos2};
use egui_kittest::kittest::{NodeT, Queryable};
use photocraft_doc::{LayerContent, LayerId};
use serde_json::json;

use crate::PhotocraftApp;
use crate::canvas::ToolEvent;

/// Smart objects "Smart A" (Gaussian Blur and Offset) and "Smart B" (no filters), and a pixel
/// layer "Pixels" on top: (harness, A, B, pixels).
fn panel() -> (egui_kittest::Harness<'static, PhotocraftApp>, u64, u64, u64) {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 16, "height": 16})).unwrap();
    let smart = |s: &mut photocraft_engine::Session, name: &str| {
        s.execute("layer.new.layer", json!({})).unwrap();
        s.execute("edit.fill", json!({"contents": "black"})).unwrap();
        let id = s.execute("layer.smartObjects.convertToSmartObject", json!({})).unwrap()["layer"].as_u64().unwrap();
        s.execute("layer.setProps", json!({"layer": id, "name": name})).unwrap();
        id
    };
    let a = smart(&mut s, "Smart A");
    s.execute("filter.blur.gaussianBlur", json!({"radius": 1})).unwrap();
    s.execute("filter.other.offset", json!({"horizontal": 2})).unwrap();
    let b = smart(&mut s, "Smart B");
    let c = s.execute("layer.new.layer", json!({})).unwrap()["layer"].as_u64().unwrap();
    s.execute("layer.setProps", json!({"layer": c, "name": "Pixels"})).unwrap();
    // 60 fps steps, so a double-click's two clicks fall inside egui's 0.3 s window.
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 900.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(s, crate::Services::default())
    });
    h.run_steps(4);
    (h, a, b, c)
}

fn filters(h: &egui_kittest::Harness<'static, PhotocraftApp>, id: u64) -> Vec<String> {
    match &h.state().session.active().unwrap().doc.layer(LayerId(id)).unwrap().content {
        LayerContent::Smart(sm) => sm.smart_filters.iter().map(|f| f.command.clone()).collect(),
        _ => Vec::new(),
    }
}

/// The centre of the Layers panel row named `label`, or of the footer button.
fn at(h: &egui_kittest::Harness<'static, PhotocraftApp>, label: &str) -> Pos2 {
    if crate::layers_panel_nodes::row_drawn(h, label) { crate::layers_panel_nodes::row(h, label).center() } else { h.get_by_label(label).rect().center() }
}

fn selected(h: &egui_kittest::Harness<'static, PhotocraftApp>, label: &str) -> bool {
    h.get_by_role_and_label(Role::TreeItem, label).accesskit_node().is_selected() == Some(true)
}

/// Press at `from`, move to `to` in steps and release there, with `modifiers` held on release.
fn drag(h: &mut egui_kittest::Harness<'static, PhotocraftApp>, from: Pos2, to: Pos2, modifiers: Modifiers) {
    h.hover_at(from);
    h.run_steps(1);
    h.drag_at(from);
    h.run_steps(1);
    for i in 1..=6 {
        h.hover_at(from + (to - from) * (i as f32 / 6.0));
        h.run_steps(1);
    }
    h.event(egui::Event::ModifiersChanged(modifiers));
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers });
    h.run_steps(4);
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

#[test]
fn a_click_selects_the_filter_and_its_smart_object() {
    let (mut h, a, _, c) = panel();
    assert_eq!(h.state().session.active().unwrap().active_layer, Some(LayerId(c)));
    let blur = at(&h, "Gaussian Blur");
    click_at(&mut h, blur);
    h.run_steps(4);
    assert_eq!(h.state().session.active().unwrap().active_layer, Some(LayerId(a)), "the smart object becomes active");
    assert!(selected(&h, "Gaussian Blur"));
    assert!(!selected(&h, "Offset"));
    // Another layer: the filter is no longer shown selected.
    let pixels = at(&h, "Pixels");
    h.hover_at(pixels);
    h.run_steps(1);
    h.drag_at(pixels);
    h.run_steps(1);
    h.drop_at(pixels);
    h.run_steps(4);
    assert_eq!(h.state().session.active().unwrap().active_layer, Some(LayerId(c)));
    assert!(!selected(&h, "Gaussian Blur"));
}

/// The report's case: the Camera Raw Filter is ⌥-dragged to another smart object with its
/// settings, and double-clicking the copy opens Camera Raw on that smart object's filter.
#[test]
fn a_camera_raw_filter_copies_and_its_copy_opens_on_the_target() {
    let (mut h, a, b, _) = panel();
    h.state_mut().run("layer.select", json!({"layer": a})).unwrap();
    h.state_mut().run("filter.cameraRaw", json!({"exposure": 1.0, "contrast": 20})).unwrap();
    h.run_steps(4);
    let (from, to) = (at(&h, "RAW Develop Filter"), at(&h, "Smart B"));
    drag(&mut h, from, to, Modifiers::ALT);
    let raw = |h: &egui_kittest::Harness<'static, PhotocraftApp>, id: u64| match &h.state().session.active().unwrap().doc.layer(LayerId(id)).unwrap().content {
        LayerContent::Smart(sm) => sm.smart_filters.iter().find(|f| f.command == "filter.cameraRaw").map(|f| f.params.clone()),
        _ => None,
    };
    assert_eq!(raw(&h, b), raw(&h, a), "copied with its settings");
    assert_eq!(raw(&h, b).unwrap()["exposure"], 1.0);
    // B is above A in the panel, so its copy is the upper of the two Camera Raw rows.
    let copy = h
        .query_all_by_label("RAW Develop Filter")
        .filter(|n| n.accesskit_node().role() == Role::TreeItem)
        .map(|n| n.rect().center())
        .min_by(|p, q| p.y.total_cmp(&q.y))
        .unwrap();
    for _ in 0..2 {
        h.hover_at(copy);
        h.drag_at(copy);
        h.run_steps(1);
        h.drop_at(copy);
        h.run_steps(1);
    }
    h.run_steps(3);
    let dialog = h.state().camera_raw.as_ref().expect("RAW Develop opened");
    assert_eq!(dialog.layer, LayerId(b));
}

fn click_at(h: &mut egui_kittest::Harness<'static, PhotocraftApp>, p: Pos2) {
    h.hover_at(p);
    h.run_steps(1);
    h.drag_at(p);
    h.run_steps(1);
    h.drop_at(p);
    h.run_steps(4);
}

/// #4751: with a filter's row selected, clicking Delete removes that filter, not the smart object.
#[test]
fn clicking_delete_removes_the_selected_filter() {
    let (mut h, a, _, _) = panel();
    let blur = at(&h, "Gaussian Blur");
    click_at(&mut h, blur);
    let trash = at(&h, "Delete layer");
    click_at(&mut h, trash);
    assert_eq!(filters(&h, a), ["filter.other.offset"]);
    assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 4, "no layer was deleted");
    // The deleted row's selection doesn't carry over to Offset, now at its index.
    click_at(&mut h, trash);
    assert_eq!(filters(&h, a), ["filter.other.offset"]);
}

/// Clicking the smart object's own row leaves its filters: Delete then deletes the layer as usual.
#[test]
fn clicking_the_smart_object_row_deselects_its_filter() {
    let (mut h, a, _, _) = panel();
    let offset = at(&h, "Offset");
    click_at(&mut h, offset);
    let row = at(&h, "Smart A");
    click_at(&mut h, row);
    assert_eq!(h.state().session.active().unwrap().active_layer, Some(LayerId(a)));
    assert!(!selected(&h, "Offset"));
    let trash = at(&h, "Delete layer");
    click_at(&mut h, trash);
    assert_eq!(filters(&h, a).len(), 2, "the filters stay");
}

/// #8764: the bottom layer's filter rows sit where the empty space below the last row begins, so
/// that space's deselect-all click (#8100) must start under them: a click on a filter of a new
/// document's only layer, converted for Smart Filters, selects the filter.
#[test]
fn a_click_selects_a_filter_of_the_bottom_layer() {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 16, "height": 16})).unwrap();
    let id = s.execute("filter.convertForSmartFilters", json!({})).unwrap()["layer"].as_u64().unwrap();
    s.execute("filter.blur.gaussianBlur", json!({"radius": 1})).unwrap();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 900.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(s, crate::Services::default())
    });
    h.run_steps(4);
    let blur = at(&h, "Gaussian Blur");
    click_at(&mut h, blur);
    let st = h.state().session.active().unwrap();
    assert_eq!(st.active_layer, Some(LayerId(id)), "the smart object stays active");
    assert_eq!(st.selected_layers(), [LayerId(id)], "the click did not deselect the layers");
    assert!(selected(&h, "Gaussian Blur"));
}

/// A double-click whose two clicks land in one frame, as a real mouse delivers them when a frame
/// takes longer than the gap between the clicks (`Harness::event` would run a frame per event).
fn double_click_in_one_frame(h: &mut egui_kittest::Harness<'static, PhotocraftApp>, p: Pos2) {
    h.hover_at(p);
    h.run_steps(1);
    for pressed in [true, false, true, false] {
        h.input_mut().events.push(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(4);
}

/// #7153: a double-click on a filter of a smart object that isn't the active layer also selects
/// it. That selection must not land after the editor captured its target, or OK refuses the edit
/// ("The document or Smart Filter changed").
#[test]
fn double_clicking_a_filter_of_an_inactive_smart_object_edits_it() {
    let (mut h, a, _, c) = panel();
    assert_eq!(h.state().session.active().unwrap().active_layer, Some(LayerId(c)));
    let blur = at(&h, "Gaussian Blur");
    double_click_in_one_frame(&mut h, blur);
    assert_eq!(h.state().session.active().unwrap().active_layer, Some(LayerId(a)), "the smart object becomes active");
    let d = h.state_mut().ui.dialogs.last_mut().expect("the filter's dialog opened");
    d.fields.insert("radius".into(), json!(3.0));
    let dialog = d.id;
    crate::dialogs::confirm(h.state_mut(), dialog).unwrap();
    assert!(!h.state().ui.status_error, "{}", h.state().ui.status);
    let LayerContent::Smart(sm) = &h.state().session.active().unwrap().doc.layer(LayerId(a)).unwrap().content else { panic!("smart object") };
    assert_eq!(sm.smart_filters.len(), 2, "edited in place, not added");
    assert_eq!(sm.smart_filters[0].params["radius"], json!(3.0));
}

/// The same for Liquify, whose workspace takes the strokes with it: a refused OK lost them.
#[test]
fn double_clicking_the_liquify_filter_of_an_inactive_smart_object_edits_it() {
    let (mut h, a, _, c) = panel();
    let stroke = json!({"tool": "forwardWarp", "size": 8, "points": [[4.0, 8.0, 1.0], [8.0, 8.0, 1.0]]});
    h.state_mut().run("layer.select", json!({"layer": a})).unwrap();
    h.state_mut().run("filter.liquify", json!({"strokes": [stroke], "meshSize": 3.0})).unwrap();
    h.state_mut().run("layer.select", json!({"layer": c})).unwrap();
    h.run_steps(4);
    let liquify = at(&h, "Liquify");
    double_click_in_one_frame(&mut h, liquify);
    assert!(h.state().distort.liquify.is_some(), "the Liquify workspace opens");
    for ev in [ToolEvent::Down { x: 6.0, y: 4.0, pressure: 1.0 }, ToolEvent::Move { x: 10.0, y: 4.0, pressure: 1.0 }, ToolEvent::Up { x: 10.0, y: 4.0 }] {
        crate::liquify_ui::pointer(h.state_mut(), ev, Modifiers::NONE);
    }
    h.run_steps(2);
    crate::liquify_ui::control(h.state_mut(), &json!({"commit": true})).unwrap();
    assert!(!h.state().ui.status_error, "{}", h.state().ui.status);
    let LayerContent::Smart(sm) = &h.state().session.active().unwrap().doc.layer(LayerId(a)).unwrap().content else { panic!("smart object") };
    let liquify = sm.smart_filters.iter().find(|f| f.command == photocraft_engine::distort_cmds::LIQUIFY).unwrap();
    assert_eq!(liquify.params["strokes"].as_array().map(Vec::len), Some(2), "the new stroke was kept");
}

/// A filter dragged onto another smart object moves there (Alt copies it), onto a pixel layer
/// nothing happens, onto Delete it is removed (the Smart Filters row removes them all); within its
/// own stack a drop on a row's upper half lands above it, on the lower half below it, on the Smart
/// Filters row on top, each one undo step, the moved filter still selected (#4271).
#[test]
fn dragging_filters() {
    let (blur, offset) = ("filter.blur.gaussianBlur", "filter.other.offset");
    let half = |p: Pos2, up: bool| p + egui::vec2(0.0, if up { -6.0 } else { 6.0 });
    type Case<'a> = (&'a str, &'a str, &'a str, Option<bool>, Modifiers, &'a [&'a str], &'a [&'a str]);
    let cases: [Case; 8] = [
        ("Alt onto another smart object copies", "Gaussian Blur", "Smart B", None, Modifiers::ALT, &[blur, offset], &[blur]),
        ("onto another smart object moves", "Offset", "Smart B", None, Modifiers::NONE, &[blur], &[offset]),
        ("onto a pixel layer: nothing", "Gaussian Blur", "Pixels", None, Modifiers::NONE, &[blur, offset], &[]),
        ("onto Delete", "Offset", "Delete layer", None, Modifiers::NONE, &[blur], &[]),
        ("the Smart Filters row onto Delete", "Smart Filters", "Delete layer", None, Modifiers::NONE, &[], &[]),
        ("onto the upper half of the filter above: above it", "Gaussian Blur", "Offset", Some(true), Modifiers::NONE, &[offset, blur], &[]),
        ("onto the lower half of the filter below: below it", "Offset", "Gaussian Blur", Some(false), Modifiers::NONE, &[offset, blur], &[]),
        ("onto the Smart Filters row: on top", "Gaussian Blur", "Smart Filters", None, Modifiers::NONE, &[offset, blur], &[]),
    ];
    for (case, from, to, side, mods, on_a, on_b) in cases {
        let (mut h, a, b, _) = panel();
        let steps = h.state().session.active().unwrap().history.past_len();
        let target = side.map_or_else(|| at(&h, to), |up| half(at(&h, to), up));
        let from_at = at(&h, from);
        drag(&mut h, from_at, target, mods);
        assert_eq!(filters(&h, a), on_a.to_vec(), "{case}: Smart A");
        assert_eq!(filters(&h, b), on_b.to_vec(), "{case}: Smart B");
        assert!(!h.state().ui.status_error, "{case}: {}", h.state().ui.status);
        assert_eq!(h.state().session.active().unwrap().doc.layers.len(), 4, "{case}: no layer was deleted");
        let changed = on_a.len() != 2 || !on_b.is_empty() || on_a[0] != blur;
        assert_eq!(h.state().session.active().unwrap().history.past_len(), steps + usize::from(changed), "{case}: one undo step");
        if side.is_some() {
            assert!(selected(&h, from), "{case}: the moved filter stays selected");
        }
    }
}

/// With a filter's row selected, Delete or Backspace removes that filter, not the smart object;
/// on the Smart Filters row it clears them all (#4751). Where the key belongs to something else (a
/// rename field, Quick Mask) the filter stays; where the row is filtered out, which deselects it,
/// Delete deletes the active layer as usual.
#[test]
fn the_delete_key_and_a_selected_filter() {
    type Case = (&'static str, &'static str, fn(&mut egui_kittest::Harness<'static, PhotocraftApp>, u64), egui::Key, usize, bool);
    let none: fn(&mut egui_kittest::Harness<'static, PhotocraftApp>, u64) = |_, _| {};
    let cases: [Case; 5] = [
        ("Delete removes the selected filter", "Gaussian Blur", none, egui::Key::Delete, 1, false),
        ("Backspace on the Smart Filters row clears them", "Smart Filters", none, egui::Key::Backspace, 0, false),
        (
            "a rename field keeps the key",
            "Gaussian Blur",
            |h, a| {
                h.state_mut().layers_rename = Some(LayerId(a));
                h.run_steps(4);
                assert!(selected(h, "Gaussian Blur"), "the filter is still selected while renaming");
            },
            egui::Key::Delete,
            2,
            false,
        ),
        (
            "Quick Mask keeps the key for Edit › Clear on the mask",
            "Gaussian Blur",
            |h, _| {
                h.state_mut().run("select.editInQuickMaskMode", json!({"on": true})).unwrap();
                assert!(h.state().session.active().unwrap().doc.selection.is_none());
                h.run_steps(4);
                assert!(selected(h, "Gaussian Blur"), "the filter is still selected in Quick Mask");
            },
            egui::Key::Delete,
            2,
            false,
        ),
        (
            "a filter row filtered out is deselected: Delete deletes the layer",
            "Gaussian Blur",
            |h, a| {
                let ctx = h.ctx.clone();
                for name in ["Pixels", ""] {
                    let (r, _) = crate::control::ControlRequest::new("ui.set", json!({"layerFilter": {"by": "name", "name": name}}));
                    crate::control::handle(h.state_mut(), &ctx, &r);
                    h.run_steps(4);
                }
                assert!(!selected(h, "Gaussian Blur"), "shown again, the row is no longer selected");
                h.state_mut().session.execute("layer.select", json!({"layer": a})).unwrap();
                h.run_steps(4);
                assert!(!selected(h, "Gaussian Blur"), "selecting the layer does not bring the filter row back");
            },
            egui::Key::Delete,
            0,
            true,
        ),
    ];
    for (case, pick, before, key, left, deleted) in cases {
        let (mut h, a, _, _) = panel();
        let p = at(&h, pick);
        click_at(&mut h, p);
        before(&mut h, a);
        h.key_press(key);
        h.run_steps(4);
        let gone = h.state().session.active().unwrap().doc.layer(LayerId(a)).is_none();
        assert_eq!(gone, deleted, "{case}: the smart object deleted");
        if !gone {
            assert_eq!(filters(&h, a).len(), left, "{case}: filters left");
        }
    }
}

// Fails: nothing tells PhotoCraft that the selected filter row is off screen with the Layers panel
// closed (`layers_tree::listed_filter` reads the filter, not the panel), so Delete removes the filter.
/// With the Layers panel closed the selected filter row isn't on screen, so Delete deletes the
/// active layer as usual.
#[test]
fn with_the_layers_panel_closed_delete_leaves_the_filter() {
    let (mut h, a, _, _) = panel();
    let p = at(&h, "Gaussian Blur");
    click_at(&mut h, p);
    let ctx = h.ctx.clone();
    crate::menus::invoke(h.state_mut(), &ctx, "window.toggle.layers", json!({})).unwrap();
    h.run_steps(4);
    assert!(h.query_all_by_label("Gaussian Blur").next().is_none(), "the panel is closed");
    h.key_press(egui::Key::Delete);
    h.run_steps(4);
    assert!(h.state().session.active().unwrap().doc.layer(LayerId(a)).is_none(), "Delete reached Smart A itself");
}
