//! What a click or double-click on a Layers panel thumbnail runs in PhotoCraft: the panel reports
//! the part and modifiers (`ThumbClick`, `Activate`), PhotoCraft picks the command.

use craft::layers::ui::tree::LayerPart;
use egui::Modifiers;
use egui_kittest::Harness;
use photocraft_doc::Layer;
use photocraft_engine::Session;
use serde_json::json;

use crate::PhotocraftApp;
use crate::layers_panel_nodes::{click, double_click, layer_id, row, thumb};

type H = Harness<'static, PhotocraftApp>;

fn harness(s: Session) -> H {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_step_dt(1.0 / 60.0).with_max_steps(64).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(s, crate::Services::default())
    });
    let ctx = h.ctx.clone();
    let (r, _) = crate::control::ControlRequest::new("ui.set", json!({"dock": {"collapsed": ["color", "properties", "history", "navigator"]}}));
    crate::control::handle(h.state_mut(), &ctx, &r);
    h.run_steps(8);
    h
}

fn active(h: &H) -> Option<String> {
    let st = h.state().session.active().unwrap();
    st.active_layer.map(|id| st.doc.layer(id).unwrap().name.clone())
}

fn selected(h: &H) -> Vec<String> {
    let st = h.state().session.active().unwrap();
    st.selected_layers().into_iter().map(|id| st.doc.layer(id).unwrap().name.clone()).collect()
}

/// The selection's bounds as (x, y, width, height).
fn selection(h: &H) -> Option<(u32, u32, u32, u32)> {
    let b = h.state().session.active().unwrap().doc.selection.as_ref()?.content_bounds();
    Some((b.x0 as u32, b.y0 as u32, b.width(), b.height()))
}

/// 400 × 300: "Shape" (black pixels at 100, 50, 120 × 100), "Small" (black at 300, 200, 20 × 10),
/// "Badge" (a shape layer at 40, 20, 60 × 40) and "Top", active.
fn stack() -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 400, "height": 300})).unwrap();
    for (name, rect) in [("Shape", [100, 50, 120, 100]), ("Small", [300, 200, 20, 10])] {
        s.execute("layer.new.layer", json!({"name": name})).unwrap();
        s.execute("select.rect", json!({"x": rect[0], "y": rect[1], "width": rect[2], "height": rect[3]})).unwrap();
        s.execute("edit.fill", json!({"contents": "black"})).unwrap();
        s.execute("select.deselect", json!({})).unwrap();
    }
    s.execute("shape.create", json!({"kind": "rect", "rect": [40, 20, 60, 40], "fill": "#cc3333", "name": "Badge"})).unwrap();
    s.execute("layer.new.layer", json!({"name": "Top"})).unwrap();
    s
}

/// Cmd-click loads the thumbnail's layer as a selection without selecting the layer (a shape
/// layer's too, as one undo step; a hidden layer's too; with Extras hidden the new selection shows
/// its edges), except on the Background, which has no transparency: there it is a plain click,
/// which selects the Background alone. Alt-click fits the layer in the window.
#[test]
fn modifier_clicks_on_a_layer_thumbnail() {
    type Case = (&'static str, fn(&mut H), &'static str, Modifiers, fn(&mut H));
    let none: fn(&mut H) = |_| {};
    let cases: [Case; 7] = [
        ("Cmd loads the pixels", none, "Shape", Modifiers::COMMAND, |h| {
            assert_eq!(selection(h), Some((100, 50, 120, 100)));
            assert_eq!((active(h).as_deref(), selected(h)), (Some("Top"), vec!["Top".to_string()]), "the selected layer stays");
        }),
        ("Cmd loads a shape layer, as one step", none, "Badge", Modifiers::COMMAND, |h| {
            assert_eq!(selection(h), Some((40, 20, 60, 40)), "{}", h.state().ui.status);
            assert_eq!(active(h).as_deref(), Some("Top"));
            h.state_mut().run("edit.undo", json!({})).unwrap();
            assert_eq!(selection(h), None);
            h.state_mut().run("edit.redo", json!({})).unwrap();
            assert_eq!(selection(h), Some((40, 20, 60, 40)));
        }),
        (
            "Cmd loads a hidden layer",
            |h| {
                let id = layer_id(h.state(), "Shape").0;
                h.state_mut().run("layer.setProps", json!({"layer": id, "visible": false})).unwrap();
                h.run_steps(2);
            },
            "Shape",
            Modifiers::COMMAND,
            |h| assert_eq!(selection(h), Some((100, 50, 120, 100))),
        ),
        (
            "Cmd with Extras hidden shows the new selection's edges",
            |h| {
                let ctx = h.ctx.clone();
                crate::menus::invoke(h.state_mut(), &ctx, "view.extras", json!({})).unwrap();
                assert!(!h.state().ui.view.extras);
            },
            "Badge",
            Modifiers::COMMAND,
            |h| {
                assert_eq!(selection(h), Some((40, 20, 60, 40)));
                assert!(h.state().ui.view.shows(h.state().ui.view.show.selection_edges));
            },
        ),
        ("Cmd on the Background is a plain click", none, "Background", Modifiers::COMMAND, |h| {
            assert_eq!(selection(h), None);
            assert_eq!((active(h).as_deref(), selected(h)), (Some("Background"), vec!["Background".to_string()]));
        }),
        (
            "Cmd on the Background in a selection of two selects it alone",
            |h| {
                let id = layer_id(h.state(), "Background").0;
                h.state_mut().run("layer.select", json!({"layer": id, "mode": "add"})).unwrap();
                h.run_steps(2);
                assert_eq!(selected(h).len(), 2);
            },
            "Background",
            Modifiers::COMMAND,
            |h| assert_eq!(selected(h), ["Background"]),
        ),
        ("Alt fits the layer", none, "Small", Modifiers::ALT, |h| {
            assert_eq!(active(h).as_deref(), Some("Small"));
            assert_eq!(h.state().ui.views[0].center, [310.0, 205.0], "centred on the layer's content");
        }),
    ];
    for (case, before, name, mods, after) in cases {
        let mut h = harness(stack());
        before(&mut h);
        let zoom = h.state().ui.views[0].zoom;
        let at = thumb(&h, name, LayerPart::Pixels).center();
        click(&mut h, at, mods);
        eprintln!("{case}");
        after(&mut h);
        if mods == Modifiers::ALT {
            assert!(h.state().ui.views[0].zoom > zoom * 4.0, "zoomed in on 20 × 10 px");
        }
    }
}

/// `name` over the Background, with "Other" above it, active.
fn under_other(make: fn(&mut Session) -> u64) -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 640, "height": 480})).unwrap();
    let id = make(&mut s);
    s.execute("layer.setProps", json!({"layer": id, "name": "T"})).unwrap();
    s.execute("layer.new.layer", json!({"name": "Other"})).unwrap();
    s
}

fn made(v: serde_json::Value) -> u64 {
    v["layer"].as_u64().unwrap()
}

fn dialog_fields(h: &H) -> Option<serde_json::Map<String, serde_json::Value>> {
    h.state().ui.dialogs.last().map(|d| d.fields.clone())
}

fn properties_shown(h: &H) {
    assert!(h.state().ui.panels.properties, "Properties opened");
    assert!(!h.state().ui.dock.collapsed.contains(&crate::dock::Group::Properties), "Properties un-collapsed");
}

/// Where a double-click lands on the row of "T".
#[derive(Clone, Copy)]
enum At {
    Thumb(LayerPart),
    /// The middle of the row, on the name.
    Name,
    /// The row's end, past the name.
    End,
}

/// A double-click opens what the part shows: a type layer's thumbnail edits its text (its name
/// still renames), a mask's thumbnail its Properties (or Select and Mask, by preference), a fill or
/// shape layer's thumbnail its fill picker, an adjustment or gradient its Properties, a frame's
/// content a Smart Object's contents; elsewhere on the row, Layer Style.
#[test]
fn double_clicks_open_what_the_part_shows() {
    type Case = (&'static str, fn() -> Session, At, fn(&mut H));
    let cases: [Case; 14] = [
        (
            "type thumbnail: inline editing",
            || under_other(|s| made(s.execute("type.create", json!({"text": "Hello", "size": 48, "x": 100, "y": 180})).unwrap())),
            At::Thumb(LayerPart::Pixels),
            |h| {
                assert_eq!(h.state().ui.tool, crate::state::Tool::Type);
                let ed = h.state().ui.text_edit.as_ref().expect("inline text editor");
                assert_eq!(ed.layer, layer_id(h.state(), "T").0);
                assert!(h.state().ui.dialogs.is_empty(), "no Layer Style dialog");
            },
        ),
        (
            "type name: no text editing",
            || under_other(|s| made(s.execute("type.create", json!({"text": "Hello", "size": 48, "x": 100, "y": 180})).unwrap())),
            At::Name,
            |h| assert!(h.state().ui.text_edit.is_none() && h.state().ui.dialogs.is_empty()),
        ),
        (
            "layer mask, preference off: Properties",
            || {
                let mut s = under_other(|s| made(s.execute("layer.new.layer", json!({})).unwrap()));
                let id = s.active().unwrap().doc.walk().into_iter().find(|(_, _, l)| l.name == "T").unwrap().2.id.0;
                s.execute("layer.layerMask.revealAll", json!({"layer": id})).unwrap();
                s.execute("prefs.set", json!({"values": {"tools.doubleClickLayerMaskLaunchesSelectAndMask": false}})).unwrap();
                s
            },
            At::Thumb(LayerPart::LayerMask),
            |h| {
                assert!(h.state().ui.mask_target, "the mask is targeted");
                properties_shown(h);
                assert!(h.state().ui.dialogs.is_empty());
            },
        ),
        (
            "layer mask, preference on: Select and Mask",
            || {
                let mut s = under_other(|s| made(s.execute("layer.new.layer", json!({})).unwrap()));
                let id = s.active().unwrap().doc.walk().into_iter().find(|(_, _, l)| l.name == "T").unwrap().2.id.0;
                s.execute("layer.layerMask.revealAll", json!({"layer": id})).unwrap();
                s.execute("prefs.set", json!({"values": {"tools.doubleClickLayerMaskLaunchesSelectAndMask": true}})).unwrap();
                s
            },
            At::Thumb(LayerPart::LayerMask),
            |h| {
                assert!(h.state().ui.mask_target);
                let w = h.state().distort.describe()["selectAndMask"].clone();
                assert_eq!(w["settings"]["output"].as_str(), Some("layerMask"), "Select and Mask opens on the mask: {w}");
            },
        ),
        (
            "vector mask: Properties",
            || {
                let mut s = under_other(|s| made(s.execute("layer.new.layer", json!({})).unwrap()));
                let id = s.active().unwrap().doc.walk().into_iter().find(|(_, _, l)| l.name == "T").unwrap().2.id.0;
                s.execute("layer.vectorMask.revealAll", json!({"layer": id})).unwrap();
                s
            },
            At::Thumb(LayerPart::VectorMask),
            |h| {
                assert!(h.state().ui.vector_mask_target, "the vector mask is targeted");
                properties_shown(h);
            },
        ),
        (
            "pattern fill: the Pattern Fill dialog",
            || under_other(|s| made(s.execute(photocraft_engine::pattern_fill_cmds::CREATE, json!({"pattern": "Checkerboard"})).unwrap())),
            At::Thumb(LayerPart::Pixels),
            |h| {
                let f = dialog_fields(h).expect("a dialog opened");
                assert!(crate::dialogs::pattern_fill_ui::owns(&f));
                assert_eq!(f["__params"]["layer"], layer_id(h.state(), "T").0);
            },
        ),
        (
            "shape: the Color Picker on its fill (#2812)",
            || under_other(|s| made(s.execute("shape.create", json!({"rect": [8, 8, 40, 30], "fill": "#336699"})).unwrap())),
            At::Thumb(LayerPart::Pixels),
            |h| {
                assert_eq!(h.state().session.active().unwrap().active_layer, Some(layer_id(h.state(), "T")));
                let f = dialog_fields(h).filter(crate::dialogs::shape_fill_ui::owns).expect("the Color Picker opened on the shape fill");
                assert_eq!(f["color"], "#336699");
            },
        ),
        ("solid colour: its picker, the fill targeted", solid, At::Thumb(LayerPart::Pixels), |h| {
            assert!(!h.state().ui.mask_target && !h.state().ui.vector_mask_target);
            let f = dialog_fields(h).expect("Color Picker opened");
            assert!(crate::dialogs::solid_fill_ui::owns(&f));
            assert_eq!(f["__label"], "Color Picker (Solid Color)");
            assert_eq!(f["__params"]["layer"], layer_id(h.state(), "T").0);
        }),
        ("solid colour's mask: not the picker", solid, At::Thumb(LayerPart::LayerMask), |h| {
            assert!(!h.state().ui.dialogs.iter().any(|d| crate::dialogs::solid_fill_ui::owns(&d.fields)));
        }),
        ("the row's end: Layer Style", solid, At::End, |h| {
            assert_eq!(h.state().ui.dialogs.last().map(|d| d.kind), Some(crate::state::DialogKind::LayerStyle));
        }),
        (
            "gradient fill: Properties",
            || under_other(|s| made(s.execute("layer.newFillLayer.gradient", json!({})).unwrap())),
            At::Thumb(LayerPart::Pixels),
            |h| {
                assert!(h.state().ui.panels.properties);
                assert!(h.state().ui.dialogs.is_empty());
            },
        ),
        (
            "adjustment: Properties",
            || under_other(|s| made(s.execute("layer.newAdjustmentLayer.invert", json!({})).unwrap())),
            At::Thumb(LayerPart::Pixels),
            |h| {
                assert!(h.state().ui.panels.properties);
                assert!(h.state().ui.dialogs.is_empty());
            },
        ),
        ("frame holding a Smart Object: its contents open", framed, At::Thumb(LayerPart::FrameContent), |h| {
            assert_eq!(h.state().session.documents().len(), 2, "the Smart Object's contents open");
        }),
        ("frame holding pixels: nothing opens", framed_pixels, At::Thumb(LayerPart::FrameContent), |h| {
            assert_eq!(h.state().session.documents().len(), 1);
            assert!(!h.state().ui.status_error, "{}", h.state().ui.status);
        }),
    ];
    for (case, doc, at, after) in cases {
        let mut h = harness(doc());
        let p = match at {
            At::Thumb(part) => thumb(&h, "T", part).center(),
            At::Name => row(&h, "T").center(),
            At::End => row(&h, "T").right_center() - egui::vec2(35.0, 0.0),
        };
        double_click(&mut h, p);
        eprintln!("{case}");
        after(&mut h);
    }
}

fn solid() -> Session {
    under_other(|s| {
        let id = made(s.execute("layer.newFillLayer.solidColor", json!({"color": "#33669980"})).unwrap());
        s.execute("layer.layerMask.revealAll", json!({})).unwrap();
        id
    })
}

/// "T", a frame holding a Smart Object.
fn framed() -> Session {
    under_other(|s| {
        let photo = made(s.execute("layer.new.layer", json!({"name": "Photo"})).unwrap());
        s.execute("edit.fill", json!({"contents": "black"})).unwrap();
        made(s.execute("layer.new.frame", json!({"x": 10, "y": 10, "width": 100, "height": 80, "layer": photo})).unwrap())
    })
}

/// "T", a frame holding a pixel layer (as PSD files can hold), which has no contents to open.
fn framed_pixels() -> Session {
    let mut s = under_other(|s| made(s.execute("layer.new.frame", json!({"x": 200, "y": 10, "width": 100, "height": 80})).unwrap()));
    let frame = s.active().unwrap().doc.walk().into_iter().find(|(_, _, l)| l.name == "T").unwrap().2.id;
    s.edit("Pixels", |doc, _| {
        let l = Layer::raster("Pixels", doc.pixel_format());
        doc.layer_mut(frame).and_then(Layer::children_mut).unwrap().push(l);
        Ok(())
    })
    .unwrap();
    s
}
/// Edit Text (the type thumbnail's double-click) on no document or a non-type layer changes nothing.
#[test]
fn edit_type_command_rejects_missing_or_non_type_layers_without_changes() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    let ctx = egui::Context::default();
    for has_document in [false, true] {
        if has_document {
            app.run("file.new", json!({"width": 32, "height": 24})).unwrap();
        }
        let tool = app.ui.tool;
        assert!(!crate::menus::is_enabled(&app, "type.editText"));
        assert!(crate::menus::invoke(&mut app, &ctx, "type.editText", json!({})).is_err());
        assert_eq!(app.ui.tool, tool);
        assert!(app.ui.text_edit.is_none());
    }
}

/// On a masked artboard, as on any masked layer: Alt-clicking the mask thumbnail shows the mask,
/// and clicking the board's thumbnail ends the mask view and targets the board.
#[test]
fn a_masked_artboards_thumbnail_retargets_it_and_ends_the_mask_view() {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 200, "height": 100})).unwrap();
    s.execute("layer.new.layer", json!({"name": "Inside"})).unwrap();
    s.execute("edit.fill", json!({"contents": "color", "color": "#336699"})).unwrap();
    let board = made(s.execute("layer.new.artboardFromLayers", json!({"name": "Board"})).unwrap());
    s.execute("layer.layerMask.revealAll", json!({"layer": board})).unwrap();
    s.execute("layer.select", json!({"layer": board})).unwrap();
    let mut h = harness(s);
    let viewing = |h: &H| h.state().session.active().and_then(photocraft_engine::mask_view_cmds::current).is_some();
    let at = thumb(&h, "Board", LayerPart::LayerMask).center();
    click(&mut h, at, Modifiers::ALT);
    assert!(viewing(&h), "Alt-click shows the mask");
    let at = thumb(&h, "Board", LayerPart::Pixels).center();
    click(&mut h, at, Modifiers::NONE);
    assert!(!h.state().ui.mask_target, "the board is targeted again");
    assert!(!viewing(&h), "and the mask view ends");
}
