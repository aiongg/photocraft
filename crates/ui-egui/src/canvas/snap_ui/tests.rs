use super::*;
use crate::canvas::{ToolEvent, tool_event};
use craft::behavior::geometry::SnapLine;
use serde_json::json;

fn app_with_box() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
    app.sync_views();
    app.ui.views[0].zoom = 1.0;
    app.ui.views[0].fit_pending = false;
    // Two boxes: a target at (300..350, 200..260) and the moving one at (40..90, 40..80).
    app.run("layer.new.layer", json!({"name": "target"})).unwrap();
    app.run("select.rect", json!({"x": 300, "y": 200, "width": 50, "height": 60})).unwrap();
    app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
    app.run("layer.new.layer", json!({"name": "mover"})).unwrap();
    app.run("select.rect", json!({"x": 40, "y": 40, "width": 50, "height": 40})).unwrap();
    app.run("edit.fill", json!({"color": "#0000ff"})).unwrap();
    app.run("select.deselect", json!({})).unwrap();
    app
}

fn mover_bounds(app: &PhotocraftApp) -> photocraft_geom::Rect {
    let st = app.session.active().unwrap();
    st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap().content_bounds()
}

/// The target of kind `kind` at `at` on `along` that a position within `tol` of `at + 0.5` snaps to.
fn near(app: &PhotocraftApp, exclude: &[LayerId], along: Axis, at: f64) -> Option<(SnapKind, f64)> {
    targets(app, 0, exclude).nearest(along, at + 0.5, 1.0).map(|t| (t.kind, t.position))
}

fn has_line(lines: &[SnapLine], position: f64) -> bool {
    lines.iter().any(|l| l.along == Axis::Y && l.position == position)
}

/// Drags the Move tool from the mover's body to `to` and returns the snap lines of the last move.
fn move_drag(app: &mut PhotocraftApp, to: [f64; 2], m: egui::Modifiers) -> Vec<SnapLine> {
    tool_event(app, ToolEvent::Down { x: 60.0, y: 60.0, pressure: 1.0 }, m);
    let lines = tool_event(app, ToolEvent::Move { x: to[0], y: to[1], pressure: 1.0 }, m).snapped;
    tool_event(app, ToolEvent::Up { x: to[0], y: to[1] }, m);
    lines
}

/// #7538: the moving rect is the floating piece's outline at its offset, not the marquee round it.
#[test]
fn moving_selected_pixels_snaps_their_bounds_not_the_marquee() {
    for alt in [false, true] {
        let mut app = app_with_box();
        app.ui.tool = Tool::Move;
        app.run("select.rect", json!({"x": 20, "y": 20, "width": 100, "height": 90})).unwrap();
        let m = egui::Modifiers { alt, ..Default::default() };
        // 263 px right puts the piece's left edge 3 px from the target's (the marquee's 20 px off).
        let lines = move_drag(&mut app, [323.0, 100.0], m);
        assert!(has_line(&lines, 300.0), "alt {alt}: a line at the target's edge: {lines:?}");
        app.run("select.drop", json!({})).unwrap();
        let sel = app.session.active().unwrap().doc.selection.as_ref().map(|s| s.content_bounds().x0);
        assert_eq!(sel, Some(300), "alt {alt}: snapped to the target's edge");
    }
}

/// Other layers' edges are targets and smart kinds; the moving set is not.
#[test]
fn move_tool_snaps_to_other_layer_edges_with_smart_guides() {
    let mut app = app_with_box();
    app.ui.tool = Tool::Move;
    app.ui.view.show.smart_guides = true;
    app.ui.tool_options.move_auto_select = false;
    let mover = app.session.active().unwrap().active_layer.unwrap();
    assert!(near(&app, &[mover], Axis::Y, 40.0).is_none(), "the moving layer's own edge is no target");
    assert_eq!(near(&app, &[], Axis::Y, 40.0), Some((SnapKind::LayerEdge, 40.0)));
    let lines = move_drag(&mut app, [323.0, 100.0], egui::Modifiers::NONE);
    assert!(lines.iter().any(|l| l.position == 300.0 && l.kind.is_smart()), "{lines:?}");
    assert_eq!(mover_bounds(&app).x0, 300);
}

/// The auto-selected layer is the moving set; an empty layer gives no box and no snap.
#[test]
fn move_tool_box_and_snapping_follow_the_auto_selected_layer() {
    for before in ["target", "empty"] {
        let mut app = app_with_box();
        let mover = app.session.active().unwrap().active_layer.unwrap();
        if before == "empty" {
            app.run("layer.new.layer", json!({"name": "empty"})).unwrap();
        } else {
            let st = app.session.active().unwrap();
            let target = st.doc.walk().into_iter().find(|(_, _, l)| l.name == "target").unwrap().2.id;
            app.run("layer.select", json!({"layer": target.0})).unwrap();
        }
        app.ui.tool = Tool::Move;
        app.ui.tool_options.move_auto_select = true;
        let lines = move_drag(&mut app, [323.0, 100.0], egui::Modifiers::NONE);
        assert_eq!(app.session.active().unwrap().active_layer, Some(mover), "{before}: Auto-Select picked the mover");
        assert!(has_line(&lines, 300.0), "{before}: {lines:?}");
        assert_eq!(mover_bounds(&app).x0, 300, "{before}: snapped to the target's edge");
    }
}

/// A ruler-dragged guide snaps to layer edges: layers are targets for guides.
#[test]
fn guides_snap_to_layer_edges() {
    let app = app_with_box();
    assert_eq!(near(&app, &[], Axis::Y, 300.0), Some((SnapKind::LayerEdge, 300.0)));
    assert_eq!(near(&app, &[], Axis::X, 200.0), Some((SnapKind::LayerEdge, 200.0)));
}

/// The target cache follows the Snap To options and the document, not only edits.
#[test]
fn guide_targets_follow_snap_to_options() {
    let mut app = app_with_box();
    let first = targets(&app, 0, &[]);
    assert!(Arc::ptr_eq(&first, &targets(&app, 0, &[])), "nothing changed: the same targets");
    assert!(near(&app, &[], Axis::Y, 300.0).is_some());
    app.ui.view.snap_to.layers = false;
    let second = targets(&app, 0, &[]);
    assert!(!Arc::ptr_eq(&first, &second), "a Snap To option rebuilds");
    assert!(near(&app, &[], Axis::Y, 300.0).is_none(), "layers off: no layer targets");
    app.run("layer.new.layer", json!({})).unwrap();
    assert!(!Arc::ptr_eq(&second, &targets(&app, 0, &[])), "a document revision rebuilds");
}

/// The percentage grid snaps at separate intervals per axis.
#[test]
fn percentage_grid_snaps_at_separate_axis_intervals() {
    let mut app = app_with_box();
    app.ui.view.snap_to.grid = true;
    app.ui.extras.grid = true;
    let (_, step) = grid(&app, app.session.active().unwrap());
    let kind_at = |along: Axis, v: f64| targets(&app, 0, &[]).nearest(along, v + 0.25, 0.5).map(|t| (t.kind, t.position));
    assert_eq!(kind_at(Axis::Y, step.x), Some((SnapKind::Grid, step.x)));
    assert_eq!(kind_at(Axis::X, step.y), Some((SnapKind::Grid, step.y)));
}

/// Grid targets come from Guides, Grid & Slices, and the snap carries the options.
#[test]
fn grid_snapping_uses_preferences() {
    let mut app = app_with_box();
    assert!(!snap(&app, 0).options().grid);
    app.ui.view.snap_to.grid = true;
    app.ui.extras.grid = true;
    assert!(snap(&app, 0).options().grid);
    let before = grid(&app, app.session.active().unwrap()).1;
    app.run("prefs.set", json!({"path": "guidesGridAndSlices.subdivisions", "value": 8})).unwrap();
    let after = grid(&app, app.session.active().unwrap()).1;
    assert!(after.x < before.x, "more subdivisions, a finer grid: {before:?} {after:?}");
}

/// The targets of a document: edges, centre, guides, grid, layers minus the moving set, an artboard,
/// the selection, and pixels past the canvas.
#[test]
fn snap_targets_follow_the_document() {
    let mut app = app_with_box();
    let mover = app.session.active().unwrap().active_layer.unwrap();
    // Document edges and centre.
    assert_eq!(near(&app, &[], Axis::Y, 0.0), Some((SnapKind::Bounds, 0.0)));
    assert_eq!(near(&app, &[], Axis::Y, 200.0), Some((SnapKind::Center, 200.0)));
    // Layers, minus the moving set.
    assert_eq!(near(&app, &[], Axis::Y, 350.0), Some((SnapKind::LayerEdge, 350.0)));
    assert!(near(&app, &[mover], Axis::Y, 90.0).is_none());
    // Guides, only while Snap To › Guides is on.
    app.run("view.newGuide", json!({"orientation": "vertical", "position": 321})).unwrap();
    assert_eq!(near(&app, &[], Axis::Y, 321.0), Some((SnapKind::Guide, 321.0)));
    app.ui.view.snap_to.guides = false;
    assert!(near(&app, &[], Axis::Y, 321.0).is_none());
    // The selection's edges: not a target while the Move tool would carry its pixels (#6290).
    app.ui.tool = Tool::RectMarquee;
    app.run("select.rect", json!({"x": 120, "y": 100, "width": 60, "height": 50})).unwrap();
    assert_eq!(near(&app, &[], Axis::Y, 120.0), Some((SnapKind::Selection, 120.0)));
    app.run("select.deselect", json!({})).unwrap();
    // An artboard offers its own edges and centre.
    app.session
        .edit("artboard", |doc, _| {
            let mut board = photocraft_doc::Layer::group("Artboard 1", vec![]);
            if let photocraft_doc::LayerContent::Group(g) = &mut board.content {
                g.artboard = Some(photocraft_doc::Artboard {
                    rect: photocraft_geom::Rect::new(20, 110, 120, 190),
                    background: Default::default(),
                    preset: String::new(),
                });
            }
            doc.insert_above(None, board);
            Ok(())
        })
        .unwrap();
    assert_eq!(near(&app, &[], Axis::Y, 120.0), Some((SnapKind::LayerEdge, 120.0)));
    assert_eq!(near(&app, &[], Axis::X, 150.0), Some((SnapKind::LayerCenter, 150.0)));
    // A layer whose pixels reach past the canvas offers its whole rect (#1919).
    app.run("layer.new.layer", json!({"name": "off"})).unwrap();
    let off = app.session.active().unwrap().active_layer.unwrap();
    app.session
        .edit("paint past the edge", |doc, _| {
            doc.layer_mut(off).unwrap().surface_mut().unwrap().fill_rect(photocraft_geom::Rect::new(-40, -20, 160, 80), &[1.0, 0.0, 0.0, 1.0]);
            Ok(())
        })
        .unwrap();
    for (along, at) in [(Axis::Y, -40.0), (Axis::X, -20.0), (Axis::Y, 160.0), (Axis::X, 80.0)] {
        assert_eq!(near(&app, &[], along, at), Some((SnapKind::LayerEdge, at)), "{along:?} {at}");
    }
}
