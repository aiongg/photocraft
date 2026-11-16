//! Layers panel Smart Filters rows (#4271) and the Delete key with a filter row selected (#4751).
//! The click, drop and double-click rows are `panels/layers_events/tests.rs`'s.

<<<<<<< HEAD
use egui::Pos2;
use egui::accesskit::Role;
=======
use egui::accesskit::{Role, Toggled};
use egui::{Modifiers, Pos2, pos2};
>>>>>>> upstream/main
use egui_kittest::kittest::{NodeT, Queryable};
use photocraft_doc::{LayerContent, LayerId};
use serde_json::json;

use crate::PhotocraftApp;

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
        PhotocraftApp::with_context(cc.egui_ctx.clone(), s, crate::Services::default())
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

fn click_at(h: &mut egui_kittest::Harness<'static, PhotocraftApp>, p: Pos2) {
    h.hover_at(p);
    h.run_steps(1);
    h.drag_at(p);
    h.run_steps(1);
    h.drop_at(p);
    h.run_steps(4);
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
    crate::panel_docking::show_panel(h.state_mut(), "layers", Some(false)).unwrap();
    h.run_steps(4);
    assert!(h.query_all_by_label("Gaussian Blur").next().is_none(), "the panel is closed");
    h.key_press(egui::Key::Delete);
    h.run_steps(4);
    assert!(h.state().session.active().unwrap().doc.layer(LayerId(a)).is_none(), "Delete reached Smart A itself");
}
<<<<<<< HEAD
=======

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

/// 40 filler layers and the smart object "Smart A" (Gaussian Blur and Offset), at the top of the
/// stack when `smart_on_top` and at its bottom otherwise, in a Layers panel short enough that the
/// list scrolls: (harness, A).
fn scrolling_panel(smart_on_top: bool) -> (egui_kittest::Harness<'static, PhotocraftApp>, u64) {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 16, "height": 16})).unwrap();
    let fillers = |s: &mut photocraft_engine::Session| {
        for i in 0..40 {
            s.execute("layer.new.layer", json!({"name": format!("Filler {i}")})).unwrap();
        }
    };
    if smart_on_top {
        fillers(&mut s);
    }
    s.execute("layer.new.layer", json!({})).unwrap();
    s.execute("edit.fill", json!({"contents": "black"})).unwrap();
    let a = s.execute("layer.smartObjects.convertToSmartObject", json!({})).unwrap()["layer"].as_u64().unwrap();
    s.execute("layer.setProps", json!({"layer": a, "name": "Smart A"})).unwrap();
    s.execute("filter.blur.gaussianBlur", json!({"radius": 1})).unwrap();
    s.execute("filter.other.offset", json!({"horizontal": 2})).unwrap();
    if !smart_on_top {
        fillers(&mut s);
    }
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 600.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(s, crate::Services::default())
    });
    let ctx = h.ctx.clone();
    let (req, _rx) = crate::control::ControlRequest::new(
        "ui.set",
        json!({"theme": "promedium", "dockWidth": 290.0, "dockTabs": {"layers": 0}, "dock": {"collapsed": ["color", "properties", "history", "navigator"]}}),
    );
    crate::control::handle(h.state_mut(), &ctx, &req);
    h.run_steps(8);
    (h, a)
}

/// Scroll the Layers list by `steps` mouse wheel events of `delta` (negative scrolls down), with
/// the pointer at `over`, a point inside the list.
fn scroll_list(h: &mut egui_kittest::Harness<'static, PhotocraftApp>, over: Pos2, steps: usize, unit: egui::MouseWheelUnit, delta: f32) {
    h.hover_at(over);
    h.run_steps(2);
    for _ in 0..steps {
        h.event(egui::Event::MouseWheel {
            source: egui::MouseWheelSource::Unknown,
            unit,
            delta: egui::vec2(0.0, delta),
            phase: egui::TouchPhase::Move,
            modifiers: Default::default(),
        });
        h.run_steps(1);
    }
    h.run_steps(10);
}

fn scroll_to_bottom(h: &mut egui_kittest::Harness<'static, PhotocraftApp>, over: Pos2) {
    scroll_list(h, over, 80, egui::MouseWheelUnit::Line, -1.0);
}

fn scroll_to_top(h: &mut egui_kittest::Harness<'static, PhotocraftApp>, over: Pos2) {
    scroll_list(h, over, 80, egui::MouseWheelUnit::Line, 1.0);
}

/// The Layers rows' name button labelled `label`, if drawn.
fn button(h: &egui_kittest::Harness<'static, PhotocraftApp>, label: &str) -> Option<egui::Rect> {
    h.query_all_by_label(label).find(|n| n.accesskit_node().role() == Role::Button).map(|n| n.rect())
}

/// Every text painted in the last frame.
fn painted_text(h: &egui_kittest::Harness<'static, PhotocraftApp>) -> Vec<String> {
    fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(t) => out.push(t.galley.text().to_string()),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &h.output().shapes {
        collect(&clipped.shape, &mut out);
    }
    out
}

/// #13118: a filter row scrolled out of the Layers list paints no name and carries no button, so
/// a long list of smart filters doesn't lay out every row's widgets each frame. It does once
/// scrolled into view.
#[test]
fn an_off_screen_filter_row_is_not_painted() {
    let (mut h, _) = scrolling_panel(false);
    let window = h.ctx.content_rect();
    // The smart object is the bottom layer: 40 rows lie above it, far below the window's end.
    let filler = h.get_by_label("Filler 0").rect();
    assert!(filler.top() > window.bottom(), "the filters' rows are below the list's visible part: {filler:?} vs {window:?}");
    assert!(button(&h, "Gaussian Blur").is_none(), "no button for a row below the list");
    assert!(!painted_text(&h).iter().any(|t| t.contains("Gaussian Blur")), "no text for a row below the list");
    let top = h.get_by_label("Filler 39").rect().center();
    scroll_to_bottom(&mut h, top);
    let fill = h.get_all_by_label("Fill").next().unwrap().rect();
    let row = button(&h, "Gaussian Blur").expect("the row is drawn once scrolled into view");
    assert!(row.center().y > fill.bottom() && row.center().y < window.bottom(), "the row shows in the list: {row:?}");
    assert!(painted_text(&h).iter().any(|t| t.contains("Gaussian Blur")), "the row's name is painted in view");
}

/// #13118: a filter selected stays selected while its row is scrolled out of view.
#[test]
fn a_selected_filter_stays_selected_when_scrolled_away() {
    let (mut h, _) = scrolling_panel(false);
    let top = h.get_by_label("Filler 39").rect().center();
    scroll_to_bottom(&mut h, top);
    let blur = button(&h, "Gaussian Blur").expect("the row shows").center();
    click_at(&mut h, blur);
    assert!(selected(&h, "Gaussian Blur"), "the click selects the filter");
    scroll_to_top(&mut h, top);
    let window = h.ctx.content_rect();
    assert!(h.get_by_label("Filler 0").rect().top() > window.bottom(), "the filter's row scrolled away");
    h.run_steps(4);
    scroll_to_bottom(&mut h, top);
    assert!(selected(&h, "Gaussian Blur"), "still selected after scrolling away and back");
}

/// #13118: a filter dropped over the panel's header doesn't reorder against the row of its own
/// smart object scrolled out of view underneath it. The rows take drops only where the list
/// shows them.
#[test]
fn a_filter_dropped_over_the_header_skips_rows_scrolled_out_of_view() {
    let (mut h, a) = scrolling_panel(true);
    let (blur, offset) = ("filter.blur.gaussianBlur", "filter.other.offset");
    assert_eq!(filters(&h, a), [blur, offset]);
    let fill = h.get_all_by_label("Fill").next().unwrap().rect();
    // Offset (the upper row) lies SUB_ROW_H-plus above Gaussian Blur's.
    let (blur_row, offset_row) = (button(&h, "Gaussian Blur").unwrap(), button(&h, "Offset").unwrap());
    let gap = offset_row.center().y - blur_row.center().y;
    assert!(gap < 0.0, "Offset is above Gaussian Blur: {gap}");
    // Scroll a little at a time until Offset's row is just above the list while Gaussian Blur's
    // is still mostly in it.
    let hover = h.get_by_label("Filler 39").rect().center();
    let mut blur_center = None;
    for _ in 0..400 {
        scroll_list(&mut h, hover, 1, egui::MouseWheelUnit::Point, -2.0);
        let Some(row) = button(&h, "Gaussian Blur") else { break };
        let (c, above) = (row.center(), row.center().y + gap);
        if above < fill.bottom() - 1.0 {
            if c.y > fill.bottom() + 6.0 {
                blur_center = Some(c);
            }
            break;
        }
    }
    let from = blur_center.expect("Gaussian Blur's row is in the list with Offset's scrolled out above it");
    // Offset's row, hidden: the point in its upper half, which a drop would reorder against.
    let over = pos2(from.x, from.y + gap - 6.0);
    assert!(over.y < fill.bottom(), "the drop point is outside the list: {over:?} vs {fill:?}");
    let doc = h.state().session.active().unwrap().doc.clone();
    h.hover_at(from);
    h.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    // Down through the list, clear of its edges (which scroll it), then up over the header and
    // released there at once.
    let inside = pos2(from.x, from.y + 60.0);
    for k in 1..=8 {
        h.event(egui::Event::PointerMoved(from + (inside - from) * (k as f32 / 8.0)));
        h.run_steps(1);
    }
    h.event(egui::Event::PointerMoved(over));
    h.event(egui::Event::PointerButton { pos: over, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
    assert_eq!(filters(&h, a), [blur, offset], "the row under the header took no drop");
    let st = h.state().session.active().unwrap();
    assert!(st.doc == doc, "the document is unchanged");
    assert!(!h.state().ui.status_error, "{}", h.state().ui.status);
}
>>>>>>> upstream/main
