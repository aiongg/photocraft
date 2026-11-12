use super::*;
use serde_json::json;

#[test]
fn mask_toggle_label_follows_history_and_selected_layer() {
    // English in every build; French too when the craft-i18n catalogs are embedded.
    let langs: Vec<_> = std::iter::once(crate::i18n::Lang::EN)
        .chain(crate::i18n::needs_catalogs().then(|| crate::i18n::Lang::from_code("fr").expect("fr registered")))
        .collect();
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 10, "height": 10})).unwrap();
    s.execute("layer.new.layer", json!({})).unwrap();
    let mut app = crate::PhotocraftApp::new(s, Default::default());
    let toggle = "layer.layerMask.enabled";
    let check = |app: &crate::PhotocraftApp, label: Option<&str>| {
        let st = app.session.active().unwrap();
        let l = st.doc.layer(st.active_layer.unwrap()).unwrap();
        let entry = entries(l, false, st.doc.selection.is_some()).into_iter().flatten().find(|e| e.1 == toggle);
        assert_eq!(entry.map(|e| e.0), label);
        // A selection has no single mask to toggle: its menu never offers the item.
        assert!(entries(l, true, st.doc.selection.is_some()).into_iter().flatten().all(|e| e.1 != toggle));
        let item = crate::menus::menu_items(app).into_iter().find(|i| i.id == toggle).unwrap();
        assert_eq!(item.enabled, label.is_some());
        assert_eq!(item.label, label.unwrap_or("Enable Layer Mask"));
        for &lang in &langs {
            let layout = crate::native_menu::photocraft_layout(&crate::menus::menu_items(app), lang, lang.code());
            let native = layout.bar.find(toggle).unwrap();
            assert_eq!(native.label, crate::i18n::tr(lang, &item.label));
            assert_eq!(native.enabled, item.enabled);
        }
    };
    check(&app, None);
    app.run("layer.layerMask.revealAll", json!({})).unwrap();
    let masked = app.session.active().unwrap().active_layer.unwrap();
    check(&app, Some("Disable Layer Mask"));
    app.run(toggle, json!({})).unwrap();
    check(&app, Some("Enable Layer Mask"));
    app.run("edit.undo", json!({})).unwrap();
    check(&app, Some("Disable Layer Mask"));
    app.run("edit.redo", json!({})).unwrap();
    check(&app, Some("Enable Layer Mask"));
    app.run("layer.new.layer", json!({})).unwrap();
    check(&app, None);
    app.run("layer.layerMask.revealAll", json!({})).unwrap();
    check(&app, Some("Disable Layer Mask"));
    app.run("layer.select", json!({"layer": masked.0, "mode": "add"})).unwrap();
    check(&app, Some("Enable Layer Mask"));
    app.run("select.rect", json!({"x": 0, "y": 0, "width": 4, "height": 4})).unwrap();
    check(&app, Some("Enable Layer Mask"));
    app.session.active_mut().unwrap().active_layer = None;
    assert!(!crate::menus::is_enabled(&app, toggle));
    let empty = crate::PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    assert!(!crate::menus::is_enabled(&empty, toggle));
}

/// With several layers selected there is no single mask to toggle, apply or delete: the menu
/// offers Add Layer Mask only, whatever the clicked layer has, and does not hit the active layer.
#[test]
fn multi_selection_menu_has_no_per_layer_mask_items() {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 10, "height": 10})).unwrap();
    s.execute("layer.new.layer", json!({})).unwrap();
    s.execute("layer.layerMask.revealAll", json!({})).unwrap();
    let st = s.active().unwrap();
    let masked = st.doc.layer(st.active_layer.unwrap()).unwrap().clone();
    let ids = entries(&masked, true, false).into_iter().flatten().map(|e| e.1).collect::<Vec<_>>();
    assert!(ids.contains(&"layer.layerMask.revealAll"));
    for single in ["layer.layerMask.enabled", "layer.layerMask.apply", "layer.layerMask.delete"] {
        assert!(!ids.contains(&single), "{single} acts on one layer, not on a selection");
    }
}

#[test]
fn edit_contents_is_available_only_for_smart_objects() {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 32, "height": 24})).unwrap();
    s.execute("layer.new.layer", json!({"name": "Source"})).unwrap();
    let layer = |s: &photocraft_engine::Session| {
        let st = s.active().unwrap();
        st.doc.layer(st.active_layer.unwrap()).unwrap().clone()
    };
    assert!(!entries(&layer(&s), false, false).iter().flatten().any(|e| e.1 == "layer.smartObjects.editContents"));
    s.execute("paint.stroke", json!({"points": [[12, 12]], "size": 8})).unwrap();
    s.execute("layer.smartObjects.convertToSmartObject", json!({})).unwrap();
    let ids: Vec<_> = entries(&layer(&s), false, false).into_iter().flatten().map(|e| e.1).collect();
    let edit = ids.iter().position(|id| *id == "layer.smartObjects.editContents").expect("Edit Contents entry");
    assert_eq!(
        ids[edit + 1..edit + 6],
        [
            "layer.smartObjects.resetTransform",
            "layer.smartObjects.replaceContents",
            "layer.smartObjects.relinkToFile",
            "layer.smartObjects.convertToLayers",
            "layer.rasterize.smartObject"
        ],
        "Reset Transform follows Edit Contents, as in the reference flyout"
    );
}

/// A type layer's menu offers Photoshop's outline conversions after Rasterize Type, and Convert
/// to Shape from it turns the text into a shape layer.
#[test]
fn type_layer_menu_converts_to_shape() {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 64, "height": 48})).unwrap();
    s.execute("type.create", json!({"text": "Hi", "size": 24, "x": 8, "y": 32})).unwrap();
    let layer = |s: &photocraft_engine::Session| {
        let st = s.active().unwrap();
        st.doc.layer(st.active_layer.unwrap()).unwrap().clone()
    };
    let ids: Vec<_> = entries(&layer(&s), false, false).into_iter().flatten().map(|e| e.1).collect();
    let r = ids.iter().position(|id| *id == "layer.rasterize.type").expect("Rasterize Type entry");
    assert_eq!(ids[r + 1..r + 3], ["type.createWorkPath", "type.convertToShape"]);
    assert!(!entries(&layer(&s), true, false).iter().flatten().any(|e| e.1 == "type.convertToShape"), "the selection menu keeps one Rasterize item");
    s.execute("type.convertToShape", json!({})).unwrap();
    assert!(matches!(layer(&s).content, LayerContent::Shape(_)));
}

/// Right-clicking inside a multi-layer selection offers one Rasterize item for the whole
/// selection, whatever kind the clicked layer is, and no single-layer Smart Object items.
#[test]
fn multi_selection_menu_rasterizes_the_selection() {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 32, "height": 24})).unwrap();
    s.execute("layer.new.layer", json!({"name": "Source"})).unwrap();
    s.execute("paint.stroke", json!({"points": [[12, 12]], "size": 8})).unwrap();
    s.execute("layer.smartObjects.convertToSmartObject", json!({})).unwrap();
    let st = s.active().unwrap();
    let smart = st.doc.layer(st.active_layer.unwrap()).unwrap().clone();
    let ids = |multi| entries(&smart, multi, false).into_iter().flatten().map(|e| e.1).collect::<Vec<_>>();
    assert!(ids(false).contains(&"layer.smartObjects.editContents"), "a single smart object keeps its items");
    assert!(ids(false).contains(&"layer.smartObjects.resetTransform"), "and its Reset Transform");
    let multi = ids(true);
    assert!(multi.contains(&"layer.rasterize.layer"));
    for single in [
        "layer.rasterize.smartObject",
        "layer.rasterize.shape",
        "layer.smartObjects.editContents",
        "layer.smartObjects.resetTransform",
        "layer.smartObjects.replaceContents",
        "layer.smartObjects.relinkToFile",
        "layer.smartObjects.convertToLayers",
    ] {
        assert!(!multi.contains(&single), "{single} acts on one layer, not on a selection");
    }
    let label = entries(&smart, true, false).into_iter().flatten().find(|e| e.1 == "layer.rasterize.layer").unwrap().0;
    assert_eq!(label, "Rasterize Layers");
}

/// Every entry of the multi-layer menu is either known to act on the selection or greyed by
/// [`single_layer_only`]; a new single-layer command added to the menu must be classified.
#[test]
fn single_layer_only_covers_the_per_layer_commands() {
    for id in
        ["layer.layerStyle.blendingOptions", "layer.layerStyle.copyLayerStyle", "layer.layerMask.enabled", "layer.layerMask.apply", "layer.layerMask.delete"]
    {
        assert!(single_layer_only(id), "{id}");
    }
    for id in [
        "layer.quickExportAsPng",
        "layer.exportAs",
        "layer.duplicate",
        "layer.delete",
        "layer.new.groupFromLayers",
        "layer.rasterize.layer",
        "layer.smartObjects.convertToSmartObject",
        "layer.createClippingMask",
        "layer.linkLayers",
        "layer.mergeLayers",
        "layer.layerMask.revealAll",
        "layer.layerMask.hideSelection",
        "layer.layerStyle.pasteLayerStyle",
        "layer.layerStyle.clear",
    ] {
        assert!(!single_layer_only(id), "{id}");
    }
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 10, "height": 10})).unwrap();
    s.execute("layer.new.layer", json!({})).unwrap();
    s.execute("layer.layerMask.revealAll", json!({})).unwrap();
    let st = s.active().unwrap();
    let masked = st.doc.layer(st.active_layer.unwrap()).unwrap().clone();
    let ids: Vec<_> = entries(&masked, true, false).into_iter().flatten().map(|e| e.1).collect();
    assert!(ids.contains(&"layer.layerMask.revealAll") && !ids.contains(&"layer.layerMask.apply"), "one stable mask item for several layers");
}

#[test]
fn entries_follow_layer_state() {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 10, "height": 10})).unwrap();
    s.execute("layer.new.layer", json!({})).unwrap();
    let st = s.active().unwrap();
    let l = st.doc.layer(st.active_layer.unwrap()).unwrap().clone();
    let ids: Vec<&str> = entries(&l, false, false).into_iter().flatten().map(|e| e.1).collect();
    assert_eq!(ids.first(), Some(&"layer.layerStyle.blendingOptions"));
    assert!(ids.contains(&"layer.mergeDown") && !ids.contains(&"layer.mergeLayers"));
    assert!(ids.contains(&"layer.layerMask.revealAll"));
    let ids: Vec<&str> = entries(&l, false, true).into_iter().flatten().map(|e| e.1).collect();
    assert!(ids.contains(&"layer.layerMask.revealSelection") && !ids.contains(&"layer.layerMask.revealAll"));
    let ids: Vec<&str> = entries(&l, true, false).into_iter().flatten().map(|e| e.1).collect();
    assert!(ids.contains(&"layer.mergeLayers"));
}

/// #3832: a group's right-click menu and the Layer menu offer Photoshop's Merge Group, which
/// merges the group itself (not into the layer below), and it is enabled.
#[test]
fn a_group_offers_merge_group() {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 10, "height": 10})).unwrap();
    s.execute("layer.new.layer", json!({})).unwrap();
    s.execute("layer.groupLayers", json!({"name": "G"})).unwrap();
    let st = s.active().unwrap();
    let g = st.doc.layer(st.active_layer.unwrap()).unwrap().clone();
    let e: Vec<(&str, &str)> = entries(&g, false, false).into_iter().flatten().collect();
    assert!(e.contains(&("Merge Group", "layer.mergeGroup")), "{e:?}");
    assert!(!e.iter().any(|x| x.0 == "Merge Down"));
    let mut app = crate::PhotocraftApp::new(s, Default::default());
    app.sync_views();
    let items = crate::menus::menu_items(&app);
    let item = items.iter().find(|i| i.id == "layer.mergeLayers").unwrap();
    assert_eq!(item.label, "Merge Group");
    assert!(item.enabled, "Merge Group runs on a group");
    // A plain layer keeps Merge Layers.
    app.run("layer.new.layer", json!({})).unwrap();
    let items = crate::menus::menu_items(&app);
    assert_eq!(items.iter().find(|i| i.id == "layer.mergeLayers").unwrap().label, "Merge Layers");
}

#[test]
fn add_mask_uses_the_selection() {
    assert_eq!(add_mask_command(false, false), "layer.layerMask.revealAll");
    assert_eq!(add_mask_command(false, true), "layer.layerMask.hideAll");
    assert_eq!(add_mask_command(true, true), "layer.layerMask.hideSelection");
    // With a selection the new mask is the selection: inside revealed, outside hidden.
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 10, "height": 10})).unwrap();
    s.execute("layer.new.layer", json!({})).unwrap();
    s.execute("select.rect", json!({"x": 0, "y": 0, "width": 4, "height": 10})).unwrap();
    s.execute(add_mask_command(true, false), json!({})).unwrap();
    let st = s.active().unwrap();
    let mask = &st.doc.layer(st.active_layer.unwrap()).unwrap().mask.as_ref().unwrap().surface;
    assert!(mask.pixel(1, 5)[0] > 0.99 && mask.pixel(8, 5)[0] < 0.01);
}

/// #2075: clicking the mask button again (with or without ⌥) used to replace the layer's
/// mask, discarding what was painted into it. Like Photoshop, it now adds a vector mask, and
/// does nothing once the layer has both.
#[test]
fn mask_button_never_replaces_a_mask() {
    for alt in [false, true] {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": 10, "height": 10})).unwrap();
        s.execute("layer.new.layer", json!({})).unwrap();
        let layer = |s: &photocraft_engine::Session| {
            let st = s.active().unwrap();
            st.doc.layer(st.active_layer.unwrap()).unwrap().clone()
        };
        let first = mask_button_command(Some(&layer(&s)), false, alt).unwrap();
        assert_eq!(first, if alt { "layer.layerMask.hideAll" } else { "layer.layerMask.revealAll" });
        s.execute(first, json!({})).unwrap();
        // Paint a stroke into the mask.
        s.edit("paint mask", |doc, active| {
            let id = (*active).ok_or(photocraft_engine::EngineError::NoDocument)?;
            let m = doc.layer_mut(id).and_then(|l| l.mask.as_mut()).ok_or(photocraft_engine::EngineError::NoDocument)?;
            m.surface.fill_rect(photocraft_geom::Rect::new(2, 2, 5, 5), &[0.5]);
            Ok(())
        })
        .unwrap();
        let painted = layer(&s).mask.unwrap().surface.pixel(3, 3)[0];
        assert!((painted - 0.5).abs() < 0.01);
        // Second click: a vector mask (⌥ hides all), the layer mask is untouched.
        let second = mask_button_command(Some(&layer(&s)), false, alt).unwrap();
        assert_eq!(second, if alt { "layer.vectorMask.hideAll" } else { "layer.vectorMask.revealAll" });
        s.execute(second, json!({})).unwrap();
        let l = layer(&s);
        assert!(l.vector_mask.is_some(), "alt={alt}");
        let mask = l.mask.as_ref().unwrap();
        assert!((mask.surface.pixel(3, 3)[0] - painted).abs() < 1e-6, "alt={alt}: the painted mask was replaced");
        // Third click: nothing left to add.
        assert_eq!(mask_button_command(Some(&l), false, alt), None);
        assert_eq!(mask_button_command(Some(&l), true, alt), None);
    }
    // No layer: nothing to do.
    assert_eq!(mask_button_command(None, false, false), None);
    // A shape layer's path is already its vector mask: after the layer mask, nothing more.
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 10, "height": 10})).unwrap();
    s.execute("shape.create", json!({"kind": "rect", "rect": [0, 0, 5, 5]})).unwrap();
    s.execute("layer.layerMask.revealAll", json!({})).unwrap();
    let st = s.active().unwrap();
    let l = st.doc.layer(st.active_layer.unwrap()).unwrap();
    assert_eq!(mask_button_command(Some(l), false, false), None);
}

#[test]
fn every_entry_is_a_known_command() {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 10, "height": 10})).unwrap();
    let l = s.active().unwrap().doc.layers[0].clone();
    for (_, id) in entries(&l, false, true).into_iter().chain(entries(&l, false, false)).chain(entries(&l, true, false)).chain(effects_entries()).flatten() {
        assert!(crate::menu_catalog::CATALOG.iter().any(|m| m.3 == id) || photocraft_engine::commands::find(id).is_some(), "{id}");
    }
}

mod panel_menus {
    //! Right-clicks in the Layers panel: the panel opens the menu PhotoCraft builds for the row or
    //! thumbnail under the pointer, and PhotoCraft runs the picked item on the clicked layer (#4575,
    //! #12324), or on the whole selection when the clicked row is one of several selected rows.

    use craft::layers::ui::tree::LayerPart;
    use egui::{Event, Modifiers, PointerButton, Pos2};
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};
    use photocraft_doc::{LabelColor, Layer, LayerContent};
    use photocraft_engine::Session;
    use serde_json::json;

    use crate::PhotocraftApp;
    use crate::layers_panel_nodes::{layer_id, row, thumb};

    type H = Harness<'static, PhotocraftApp>;

    fn harness(s: Session) -> H {
        let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_step_dt(1.0 / 60.0).with_max_steps(64).build_eframe(|cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            PhotocraftApp::new(s, crate::Services::default())
        });
        let ctx = h.ctx.clone();
        let (req, _rx) = crate::control::ControlRequest::new("ui.set", json!({"dock": {"collapsed": ["color", "properties", "history", "navigator"]}}));
        crate::control::handle(h.state_mut(), &ctx, &req);
        h.run_steps(8);
        h
    }

    fn press(h: &mut H, at: Pos2, button: PointerButton) {
        h.hover_at(at);
        h.step();
        for pressed in [true, false] {
            h.event(Event::PointerButton { pos: at, button, pressed, modifiers: Modifiers::NONE });
            h.step();
        }
        h.run_steps(3);
    }

    fn pick(h: &mut H, item: &str) {
        let at = h.get_by_label(item).rect().center();
        press(h, at, PointerButton::Primary);
    }

    fn layer(h: &H, name: &str) -> Layer {
        h.state().session.active().unwrap().doc.layer(layer_id(h.state(), name)).unwrap().clone()
    }

    fn active(h: &H) -> String {
        let st = h.state().session.active().unwrap();
        st.doc.layer(st.active_layer.unwrap()).unwrap().name.clone()
    }

    /// "Painted" (pixels at 60..100 × 0..60), "Masked" (a mask revealing 60..100) and a Smart
    /// Object "Smart" under "Other", active, with a selection of 0..20 that overlaps neither.
    fn stack() -> Session {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 100, "height": 60})).unwrap();
        s.execute("layer.new.layer", json!({"name": "Smart"})).unwrap();
        s.execute("paint.stroke", json!({"points": [[10, 10]], "size": 8})).unwrap();
        let smart = s.execute("layer.smartObjects.convertToSmartObject", json!({})).unwrap()["layer"].as_u64().unwrap();
        s.execute("layer.setProps", json!({"layer": smart, "name": "Smart"})).unwrap();
        s.execute("layer.new.layer", json!({"name": "Painted"})).unwrap();
        s.execute("select.rect", json!({"x": 60, "y": 0, "width": 40, "height": 60})).unwrap();
        s.execute("edit.fill", json!({"color": "#b04020"})).unwrap();
        s.execute("layer.new.layer", json!({"name": "Masked"})).unwrap();
        s.execute("layer.layerMask.revealSelection", json!({})).unwrap();
        s.execute("layer.new.layer", json!({"name": "Other"})).unwrap();
        s.execute("layer.layerMask.revealAll", json!({})).unwrap();
        s.execute("select.rect", json!({"x": 0, "y": 0, "width": 20, "height": 60})).unwrap();
        s
    }

    /// A type layer "Hi", a shape "Ellipse" and a shape "Rect", with "Hi" and "Ellipse" selected.
    fn two_selected() -> Session {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 640, "height": 480})).unwrap();
        let text = s.execute("type.create", json!({"text": "Hi", "size": 24, "x": 20, "y": 60})).unwrap()["layer"].as_u64().unwrap();
        s.execute("layer.setProps", json!({"layer": text, "name": "Hi"})).unwrap();
        let shape = s.execute("shape.create", json!({"kind": "ellipse", "rect": [100, 100, 200, 160], "name": "Ellipse"})).unwrap()["layer"].as_u64().unwrap();
        s.execute("shape.create", json!({"kind": "rect", "rect": [300, 100, 380, 160], "name": "Rect"})).unwrap();
        s.execute("layer.select", json!({"layer": text})).unwrap();
        s.execute("layer.select", json!({"layer": shape, "mode": "add"})).unwrap();
        s
    }

    fn selected(h: &H, x: i32, y: i32) -> bool {
        h.state().session.active().unwrap().doc.selection.as_ref().is_some_and(|s| s.sample_channel(x, y, 0) > 0.99)
    }

    #[derive(Clone, Copy)]
    enum On {
        Row(&'static str),
        Thumb(&'static str, LayerPart),
        Eye(&'static str),
    }

    #[test]
    #[ignore = "craft: the row menu runs past the window's bottom edge (Label Color at y 965 in a 900-pt window)"]
    fn context_menu_picks_act_on_the_clicked_row() {
        type Case = (&'static str, fn() -> Session, On, &'static [&'static str], fn(&mut H));
        let cases: [Case; 9] = [
            ("Smart Object thumbnail › Edit Contents", stack, On::Thumb("Smart", LayerPart::Pixels), &["Edit Contents"], |h| {
                assert_eq!(h.state().session.active_index(), Some(1), "the contents tab opens");
                h.state_mut().session.set_active(0);
                assert_eq!(active(h), "Smart");
            }),
            ("pixel thumbnail › Select Pixels", stack, On::Thumb("Painted", LayerPart::Pixels), &["Select Pixels"], |h| {
                assert!(selected(h, 80, 30), "the clicked layer's pixels are selected");
                assert!(!selected(h, 10, 30), "the old selection is replaced");
            }),
            ("layer mask thumbnail › Add Mask to Selection", stack, On::Thumb("Masked", LayerPart::LayerMask), &["Add Mask to Selection"], |h| {
                assert_eq!(active(h), "Masked");
                assert!(selected(h, 10, 30), "the selection is kept");
                assert!(selected(h, 80, 30), "the clicked layer's mask is added");
                assert!(!selected(h, 40, 30), "nothing else is selected");
            }),
            ("layer mask thumbnail › Mask Options…", stack, On::Thumb("Masked", LayerPart::LayerMask), &["Mask Options…"], |h| {
                let d = h.state().ui.dialogs.last().expect("Mask Options… opens a dialog");
                assert!(crate::channel_options_ui::owns(&d.fields) && d.fields["channel"] == json!("mask"), "{:?}", d.fields);
            }),
            ("a row › Disable Layer Mask: the clicked layer's mask, not the active one's", stack, On::Row("Masked"), &["Disable Layer Mask"], |h| {
                assert_eq!(active(h), "Masked");
                assert!(!layer(h, "Masked").mask.unwrap().enabled);
                assert!(layer(h, "Other").mask.unwrap().enabled);
            }),
            ("one of two selected rows › Rasterize Layers", two_selected, On::Row("Hi"), &["Rasterize Layers"], |h| {
                let raster = |name| matches!(layer(h, name).content, LayerContent::Raster(_));
                assert!(raster("Hi") && raster("Ellipse"), "both selected layers are rasterized");
                assert!(!raster("Rect"), "an unselected layer is left alone");
                assert_eq!(h.state().session.active().unwrap().selected_layers().len(), 2, "the selection survives");
            }),
            ("one of two selected rows › Group from Layers…", two_selected, On::Row("Hi"), &["Group from Layers…"], |h| {
                let (id, fields) = h.state().ui.dialogs.last().map(|d| (d.id, d.fields.clone())).expect("the New Group dialog opened");
                assert_eq!(fields["__command"], "layer.new.groupFromLayers");
                crate::dialogs::confirm(h.state_mut(), id).unwrap();
                h.run_steps(2);
                let doc = &h.state().session.active().unwrap().doc;
                let group = doc.walk().into_iter().map(|(_, _, l)| l).find(|l| l.is_group()).expect("a group");
                let names: Vec<&str> = group.children().unwrap().iter().map(|l| l.name.as_str()).collect();
                assert_eq!(names, ["Hi", "Ellipse"], "both selected layers, in order");
            }),
            ("one of two selected rows › Label Color › Blue colours both", two_selected, On::Row("Hi"), &["Label Color", "Blue"], |h| {
                assert_eq!((layer(h, "Hi").label, layer(h, "Ellipse").label, layer(h, "Rect").label), (LabelColor::Blue, LabelColor::Blue, LabelColor::None));
                assert_eq!(h.state().session.active().unwrap().selected_layers().len(), 2);
            }),
            ("the eye › a colour, without hiding the layer", stack, On::Eye("Painted"), &["Blue"], |h| {
                let l = layer(h, "Painted");
                assert!(l.visible);
                assert_eq!((l.label, layer(h, "Other").label), (LabelColor::Blue, LabelColor::None));
            }),
        ];
        for (case, doc, on, items, after) in cases {
            let mut h = harness(doc());
            let at = match on {
                On::Row(name) => row(&h, name).center(),
                On::Thumb(name, part) => thumb(&h, name, part).center(),
                On::Eye(name) => crate::layers_panel_nodes::part(&h, &format!("Visibility {name}")).center(),
            };
            press(&mut h, at, PointerButton::Secondary);
            // A submenu opens under the pointer; the last item is the pick.
            for (i, item) in items.iter().enumerate() {
                if i + 1 < items.len() {
                    let at = h.get_by_label(item).rect().center();
                    assert!(h.ctx.content_rect().contains(at), "{case}: {item} at {at:?} is in the window");
                    h.hover_at(at);
                    h.run_steps(4);
                } else {
                    pick(&mut h, item);
                }
            }
            eprintln!("{case}");
            after(&mut h);
        }
    }

    /// The menus PhotoCraft builds for what was clicked: a thumbnail's (the Background's has no
    /// pixels to load, so it keeps the layer menu), a mask's, and the selection's, which greys what
    /// acts on one layer and leaves out the single-layer mask and Smart Object items.
    #[test]
    fn each_part_gets_its_own_menu() {
        type Case = (&'static str, fn() -> Session, On, &'static [&'static str], &'static [&'static str], &'static [&'static str]);
        let cases: [Case; 4] = [
            (
                "a pixel thumbnail",
                stack,
                On::Thumb("Painted", LayerPart::Pixels),
                &["Select Pixels", "Small Thumbnails", "Clip Thumbnails to Layer Bounds"],
                &[],
                &["Blending Options…"],
            ),
            ("the Background's thumbnail: the layer menu", stack, On::Thumb("Background", LayerPart::Pixels), &[], &[], &["Select Pixels", "Small Thumbnails"]),
            (
                "a layer mask thumbnail",
                stack,
                On::Thumb("Masked", LayerPart::LayerMask),
                &[
                    "Disable Layer Mask",
                    "Delete Layer Mask",
                    "Apply Layer Mask",
                    "Subtract Mask from Selection",
                    "Intersect Mask with Selection",
                    "Select and Mask…",
                ],
                &[],
                &["Blending Options…"],
            ),
            (
                "one of two selected rows",
                two_selected,
                On::Row("Hi"),
                &[
                    "Export As…",
                    "Add Layer Mask",
                    "Duplicate Layers…",
                    "Delete Layers",
                    "Convert to Smart Object",
                    "Rasterize Layers",
                    "Link Layers",
                    "Merge Layers",
                ],
                &["Blending Options…", "Copy Layer Style", "Paste Layer Style", "Clear Layer Style"],
                &["Disable Layer Mask", "Apply Layer Mask", "Delete Layer Mask", "Edit Contents", "Convert to Layers"],
            ),
        ];
        for (case, doc, on, enabled, disabled, absent) in cases {
            let mut h = harness(doc());
            let at = match on {
                On::Row(name) => row(&h, name).center(),
                On::Thumb(name, part) => thumb(&h, name, part).center(),
                On::Eye(_) => unreachable!(),
            };
            press(&mut h, at, PointerButton::Secondary);
            for label in enabled.iter() {
                assert!(!h.get_by_label(label).accesskit_node().is_disabled(), "{case}: {label} is offered");
            }
            for label in disabled.iter() {
                assert!(h.get_by_label(label).accesskit_node().is_disabled(), "{case}: {label} is greyed");
            }
            for label in absent.iter() {
                assert!(h.query_by_label(label).is_none(), "{case}: {label} is not offered");
            }
        }
    }
}
