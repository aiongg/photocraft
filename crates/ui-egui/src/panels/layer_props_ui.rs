//! Properties panel for pixel (and other bounded) layers, Photoshop 2026 style: a Transform section
//! (W/H/X/Y of the layer content, editable), Align and Distribute, the kind's sections and Quick Actions. Edits dispatch
//! `edit.transform`, `layer.translate`, `layer.align.*` and selection commands.

use craft::behavior::geometry::quad::BoxState;
use craft::behavior::kurbo::{Point, Rect, Size};
use craft::behavior::{ColumnWidth, EditEvent, EditEvents, Mixed, RulerUnits, Ticks};
use craft::draw::ui::{SizeUnit, TransformFields, TransformGroups, TransformRow};
use craft::ui::bars::AlignBar;
use craft::ui::buttons::Button;
use craft::ui::canvas::RulerText;
use craft::ui::contract::{Bind, Props as _, Show as _, ShowContainer as _, Sizing};
use craft::ui::layout_components::Grid;
use craft::ui::runtime::commands::CommandRef;
use craft::ui::sections::Section;
use photocraft_doc::{Layer, LayerContent, LayerId};
use photocraft_engine::prefs::Unit;
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::prefs::unit_kind;

/// The panel width, in points, from which Quick Actions take two columns.
const TWO_COLUMNS_FROM: f32 = 360.0;
/// Quick Actions in two columns that share the width.
const TWO_COLUMNS: &[ColumnWidth] = &[ColumnWidth::fill(1.0), ColumnWidth::fill(1.0)];
/// Quick Actions in one column.
const ONE_COLUMN: &[ColumnWidth] = &[ColumnWidth::fill(1.0)];

/// `edit.transform` params that scale the content box `b` = [x0, y0, x1, y1] to `w` x `h`, keeping
/// the top-left corner (Photoshop's Properties W/H fields), or `layer.translate` params for X/Y.
/// `tol` is the smallest change, in px, that counts as an edit (see [`edit_tolerance`]).
pub fn transform_params(layer: u64, b: [f64; 4], w: Option<f32>, h: Option<f32>, linked: bool, tol: f32) -> Option<Value> {
    // Keep the transform geometry in f64. Rounding the linked W/H ratio and
    // quad corners to f32 can introduce a tiny anisotropic scale, which makes
    // a diagonal live ArrowLine lose its editable parameters after resizing.
    let (bw, bh) = (b[2] - b[0], b[3] - b[1]);
    if !bw.is_finite() || !bh.is_finite() || bw <= 0.0 || bh <= 0.0 {
        return None;
    }
    let (mut nw, mut nh) = (w.map(f64::from).unwrap_or(bw).max(1.0), h.map(f64::from).unwrap_or(bh).max(1.0));
    if linked {
        if w.is_some() {
            nh = (nw * bh / bw).max(1.0);
        } else if h.is_some() {
            nw = (nh * bw / bh).max(1.0);
        }
    }
    if (nw - bw).abs() < f64::from(tol) && (nh - bh).abs() < f64::from(tol) {
        return None;
    }
    let (x0, y0) = (b[0], b[1]);
    let quad = [[x0, y0], [x0 + nw, y0], [x0 + nw, y0 + nh], [x0, y0 + nh]];
    Some(json!({"layer": layer, "rect": b, "quad": quad}))
}

/// The smallest W/H change, in px, that counts as an edit. A pixel frame is whole pixels: under
/// half a pixel is noise. A shape's path is fractional and is scaled to exactly the typed size,
/// so anything past the field's own rounding (half its last shown decimal) is an edit: 400.25 px
/// typed as 400 applies, while focusing and leaving the field (which re-reads its rounded text)
/// doesn't. `step_px` is one shown decimal step of the ruler unit, in px.
fn edit_tolerance(layer: &Layer, step_px: f64) -> f32 {
    match &layer.content {
        LayerContent::Shape(sh) if !sh.path.inverted => (step_px as f32 + 1e-3).min(0.5),
        _ => 0.5,
    }
}

/// The box the Transform fields show and edit, [x0, y0, x1, y1]: the layer's pixel frame `px`,
/// or for a shape its path's geometry, as Photoshop's Properties shows it, without the pixels its
/// stroke adds around it; W/H then scale the path to exactly the typed size.
fn frame(layer: &Layer, px: photocraft_geom::Rect) -> [f64; 4] {
    let path = match &layer.content {
        LayerContent::Shape(sh) if !sh.path.inverted => sh.path.control_bounds().filter(|c| c.2 > c.0 && c.3 > c.1),
        _ => None,
    };
    path.map_or([f64::from(px.x0), f64::from(px.y0), f64::from(px.x1), f64::from(px.y1)], |c| [c.0, c.1, c.2, c.3])
}

/// The Units & Rulers unit, the document's resolution and the points per inch: what a length shown in
/// the rulers' unit needs.
fn ruler_unit(app: &PhotocraftApp) -> (Unit, f64, f64) {
    let ur = &app.session.prefs().units_and_rulers;
    let dpi = app.session.active().map_or(72.0, |s| f64::from(s.doc.resolution_dpi));
    (ur.rulers, dpi, ur.point_size.per_inch())
}

/// The framework's text for lengths in `unit` along an axis `extent` px long (the 100% of percent).
fn ruler_text(unit: Unit, dpi: f64, ppi: f64, extent: f64) -> RulerText {
    RulerText::new(RulerUnits::new(unit.from_px(1.0, dpi, extent, ppi), Ticks::decimal(unit == Unit::Pixels)), unit_kind(unit))
}

/// The width and height of a row's box.
fn box_size(row: &TransformRow) -> (f64, f64) {
    let q = &row.state.quad;
    ((q[1] - q[0]).hypot(), (q[3] - q[0]).hypot())
}

/// Runs a command; a refusal goes to the status line.
fn run(app: &mut PhotocraftApp, id: &str, params: Value) {
    if let Err(e) = app.run(id, params) {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

/// The layer's Transform box as it is now, read again after an edit of the same pass.
fn current_frame(app: &mut PhotocraftApp, id: LayerId) -> Option<[f64; 4]> {
    let doc = app.session.active()?.doc.clone();
    let layer = doc.layer(id)?;
    let surface = layer.surface()?;
    Some(frame(layer, app.layer_frame_bounds(layer, surface)))
}

/// Applies the edits the Transform fields committed in this pass: a W or H change past `tol` is one
/// `edit.transform`, an X or Y change one `layer.translate`. A commit that left the box where it was
/// (a click with no change, a value the field rounds back to the shown one) changes nothing.
fn apply_transform(app: &mut PhotocraftApp, layer: &Layer, linked: bool, tol: (f32, f32), events: &EditEvents<TransformRow>) {
    for event in events.iter() {
        let (EditEvent::Commit { before: Mixed::Same(before), after, .. } | EditEvent::Set { before: Mixed::Same(before), after, .. }) = event else {
            continue;
        };
        let (bw, bh) = box_size(before);
        let (aw, ah) = box_size(after);
        let w_changed = (aw - bw).abs() >= f64::from(tol.0);
        let h_changed = (ah - bh).abs() >= f64::from(tol.1);
        if w_changed || h_changed {
            // Linked, the typed side carries the other one along.
            let w = w_changed.then_some(aw as f32);
            let h = (h_changed && !(linked && w.is_some())).then_some(ah as f32);
            let tolerance = match (w, h) {
                (Some(_), None) => tol.0,
                (None, Some(_)) => tol.1,
                _ => tol.0.min(tol.1),
            };
            if let Some(bb) = current_frame(app, layer.id)
                && let Some(params) = transform_params(layer.id.0, bb, w, h, linked, tolerance)
            {
                run(app, "edit.transform", params);
            }
        }
        let (dx, dy) = ((after.state.pivot.x - before.state.pivot.x).round() as i32, (after.state.pivot.y - before.state.pivot.y).round() as i32);
        if dx != 0 || dy != 0 {
            let params = crate::move_ui::pattern_wrap(app, json!({"layer": layer.id.0, "dx": dx, "dy": dy}));
            run(app, "layer.translate", params);
        }
    }
}

/// The Transform section: X, Y, W and H with the aspect link, in the Units & Rulers unit. The reference
/// point is the box's top-left corner, so W and H scale from it and X and Y are its position.
///
/// The edits are the ones the fields report in this pass, applied once after the section has shown. A
/// pass that reruns inside the frame gets no input events, so it applies nothing a second time.
fn transform_section(app: &mut PhotocraftApp, ui: &mut egui::Ui, layer: &Layer, bb: [f64; 4]) {
    let (unit, dpi, ppi) = ruler_unit(app);
    let (dw, dh) = app.session.active().map_or((1.0, 1.0), |s| (f64::from(s.doc.size.width), f64::from(s.doc.size.height)));
    let step = |extent: f64| (unit.to_px(0.05, dpi, extent, ppi) - unit.to_px(0.0, dpi, extent, ppi)).abs();
    let tol = (edit_tolerance(layer, step(dw)), edit_tolerance(layer, step(dh)));
    let (x_text, y_text) = (ruler_text(unit, dpi, ppi, dw), ruler_text(unit, dpi, ppi, dh));
    // `TransformGroups` is non-exhaustive: start from none and switch the shown groups on.
    let mut groups = TransformGroups::NONE;
    groups.position = true;
    groups.size = true;
    let rect = Rect::new(bb[0], bb[1], bb[2], bb[3]);
    let corners = [Point::new(bb[0], bb[1]), Point::new(bb[2], bb[1]), Point::new(bb[2], bb[3]), Point::new(bb[0], bb[3])];
    let row = TransformRow::new(BoxState::new(corners, corners[0]), 0.0);
    let shown = Section::new(tl!("Transform")).show(ui, |ui, _| {
        TransformFields::new(tl!("Transform"), Bind::given(row), rect, Bind::bound(&mut app.ui.properties_linked))
            .groups(groups)
            .document(Size::new(dw, dh))
            .units(x_text, y_text)
            .size_unit(SizeUnit::length(x_text, y_text))
            .show(ui)
    });
    let Some(out) = shown.inner else { return };
    // The bound link already holds this pass's toggle; a toggle carries no edit.
    let linked = app.ui.properties_linked;
    apply_transform(app, layer, linked, tol, &out.events);
}

/// Properties body for a non-adjustment layer: Transform, Align and Distribute, the kind's own
/// sections (Character/Paragraph/Type Options for type, Appearance/Shape for shapes), then Quick
/// Actions, all with the same collapsible headers (#155).
pub fn properties(app: &mut PhotocraftApp, ui: &mut egui::Ui, layer: &Layer) {
    if let Some(s) = layer.surface() {
        let bb = frame(layer, app.layer_frame_bounds(layer, s));
        transform_section(app, ui, layer, bb);
    }
    // Gradient fills: their gradient first, as in Photoshop's Properties panel.
    if matches!(layer.content, LayerContent::Fill(photocraft_doc::Fill::Gradient { .. })) {
        crate::gradient_ui::properties(app, ui, layer);
    } else {
        align_and_distribute(ui);
    }
    match &layer.content {
        LayerContent::Text(_) => crate::type_tool::type_properties(app, ui),
        LayerContent::Shape(_) => {
            crate::vector_ui::shape_properties(app, ui, layer.id);
            crate::mask_props_ui::shape_properties(app, ui, layer);
        }
        _ => {}
    }
    // Re-read: a section above may have changed the layer (e.g. point to paragraph type).
    let content = app.session.active().and_then(|s| s.doc.layer(layer.id)).map_or_else(|| layer.content.clone(), |l| l.content.clone());
    quick_actions_section(app, ui, &content);
}

/// The Align and Distribute section: the framework's align bar. The align commands are app commands
/// that `crate::align_bar` states and runs, so the section only shows the bar.
fn align_and_distribute(ui: &mut egui::Ui) {
    Section::new(tl!("Align and Distribute")).show(ui, |ui, _| {
        AlignBar::new().show(ui);
    });
}

/// Several layers selected with the Background active: Photoshop's Properties keeps Align and
/// Distribute for the selection there, above the Document properties (#4608).
pub fn selection_align(ui: &mut egui::Ui) {
    align_and_distribute(ui);
}

/// The Quick Actions a layer kind offers, as (label, command id), Photoshop-style: pixel layers get
/// subject selection, type layers its conversions, shapes and smart objects their own.
pub fn quick_actions(content: &LayerContent) -> &'static [(&'static str, &'static str)] {
    match content {
        LayerContent::Raster(_) => &[("Remove Background", "layer.removeBackground"), ("Select Subject", "select.subject")],
        LayerContent::Text(_) => &[
            ("Convert to Shape", "type.convertToShape"),
            ("Convert to Paragraph Text", "type.convertToParagraphText"),
            ("Convert to Point Text", "type.convertToPointText"),
            ("Create Warped Text", "type.warpText"),
            ("Rasterize Type", "type.rasterizeTypeLayer"),
        ],
        LayerContent::Shape(_) => &[("Rasterize Shape", "layer.rasterize.shape"), ("Convert to Smart Object", "layer.smartObjects.convertToSmartObject")],
        LayerContent::Smart(_) => &[
            ("Edit Contents", "layer.smartObjects.editContents"),
            ("Convert to Layers", "layer.smartObjects.convertToLayers"),
            ("Rasterize Smart Object", "layer.rasterize.smartObject"),
        ],
        LayerContent::Group(_) => &[("Ungroup Layers", "layer.ungroupLayers"), ("Convert to Smart Object", "layer.smartObjects.convertToSmartObject")],
        LayerContent::Fill(_) => &[("Rasterize Fill Content", "layer.rasterize.fillContent")],
        LayerContent::Adjustment(_) => &[],
    }
}

/// The Quick Actions shown right now: the kind's list, limited to commands that exist and are
/// enabled (Convert to Paragraph Text only for point type, and vice versa).
pub fn visible_quick_actions(app: &PhotocraftApp, content: &LayerContent) -> Vec<(&'static str, &'static str)> {
    quick_actions(content).iter().copied().filter(|(_, id)| crate::menus::is_live(id) && crate::menus::is_enabled(app, id)).collect()
}

/// The Quick Actions section: one command button per shown action, two columns when the panel is
/// wide enough. A click fires the command, which the menus run (the ones with dialogs open them).
fn quick_actions_section(app: &PhotocraftApp, ui: &mut egui::Ui, content: &LayerContent) {
    let actions = visible_quick_actions(app, content);
    if actions.is_empty() {
        return;
    }
    Section::new(tl!("Quick Actions")).show(ui, |ui, _| {
        let columns = if ui.available_width() >= TWO_COLUMNS_FROM { TWO_COLUMNS } else { ONE_COLUMN };
        Grid::new(columns).label(tl!("Quick Actions")).sizing(Sizing::FillWidth).show(ui, |ui, _| {
            for (_, id) in &actions {
                Button::command(CommandRef::new(*id)).sizing(Sizing::FillWidth).show(ui);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document with one layer of each kind; returns the session and the ids by kind.
    fn kinds() -> (photocraft_engine::Session, Vec<(&'static str, u64)>) {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": 400, "height": 300})).unwrap();
        let id = |v: serde_json::Value| v["layer"].as_u64().unwrap();
        let pixel = id(s.execute("layer.new.layer", json!({"name": "Paint"})).unwrap());
        s.execute("select.rect", json!({"x": 10, "y": 10, "width": 100, "height": 60})).unwrap();
        s.execute("edit.fill", json!({"contents": "color", "color": "#336699"})).unwrap();
        s.execute("select.deselect", json!({})).unwrap();
        let shape = id(s.execute("shape.create", json!({"kind": "rect", "rect": [20, 30, 80, 40], "fill": "#ff0000"})).unwrap());
        let ty = id(s.execute("type.create", json!({"text": "Headline", "size": 48, "x": 40, "y": 200})).unwrap());
        (s, vec![("pixel", pixel), ("shape", shape), ("type", ty)])
    }

    fn layer_of(app: &PhotocraftApp, id: u64) -> Layer {
        app.session.active().unwrap().doc.layer(LayerId(id)).unwrap().clone()
    }

    /// Every kind's Quick Actions are live commands (a renamed id would show a dead button), Select
    /// Subject and Remove Background are for pixel layers only, and the shown list drops disabled ones.
    #[test]
    fn every_quick_action_is_a_live_command_and_only_enabled_ones_show() {
        let (s, layers) = kinds();
        let mut app = PhotocraftApp::new(s, crate::Services::default());
        let ids = |layer: &Layer| quick_actions(&layer.content).iter().map(|(_, id)| *id).collect::<Vec<_>>();
        for (kind, id) in &layers {
            let layer = layer_of(&app, *id);
            for command in ids(&layer) {
                assert!(crate::menus::is_live(command), "{kind}: {command} is not a live command");
            }
            let pixel_only = ids(&layer).iter().any(|c| matches!(*c, "select.subject" | "layer.removeBackground"));
            assert_eq!(pixel_only, *kind == "pixel", "{kind}");
        }
<<<<<<< HEAD
        let pixel = layers.iter().find(|(k, _)| *k == "pixel").unwrap().1;
        app.run("layer.select", json!({"layer": pixel})).unwrap();
        let shown: Vec<&str> = visible_quick_actions(&app, &layer_of(&app, pixel).content).iter().map(|(_, id)| *id).collect();
        assert!(shown.contains(&"layer.removeBackground"), "{shown:?}");
        let ty = layers.iter().find(|(k, _)| *k == "type").unwrap().1;
        app.run("layer.select", json!({"layer": ty})).unwrap();
        let shown: Vec<&str> = visible_quick_actions(&app, &layer_of(&app, ty).content).iter().map(|(l, _)| *l).collect();
        assert_eq!(shown, ["Convert to Shape", "Convert to Paragraph Text", "Create Warped Text", "Rasterize Type"], "point type has no Convert to Point Text");
=======
    }

    /// #7314: in the Transform section W sits over X and H over Y, and each pair shares a centre.
    #[test]
    fn transform_labels_are_centred_on_each_other() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 120})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        app.run("select.rect", json!({"x": 10, "y": 10, "width": 100, "height": 60})).unwrap();
        app.run("edit.fill", json!({"contents": "color", "color": "#336699"})).unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(280.0, 900.0)).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                if ui.ctx().cumulative_pass_nr() == 0 {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::ProMedium);
                    return;
                }
                let state = app.session.active().unwrap();
                let layer = state.doc.layer(state.active_layer.unwrap()).unwrap().clone();
                properties(app, ui, &layer);
            },
            app,
        );
        h.run_steps(3);
        let centres: Vec<(String, f32)> = h
            .output()
            .shapes
            .iter()
            .filter_map(|s| if let egui::Shape::Text(t) = &s.shape { Some(t) } else { None })
            .filter(|t| t.galley.text().chars().count() == 1)
            .map(|t| (t.galley.text().to_string(), t.pos.x + t.galley.size().x * 0.5))
            .collect();
        let at = |l: &str| centres.iter().find(|(s, _)| s == l).map(|(_, x)| *x).unwrap_or_else(|| panic!("no {l} label in {centres:?}"));
        assert!((at("W") - at("X")).abs() < 0.01, "W and X: {centres:?}");
        assert!((at("H") - at("Y")).abs() < 0.01, "H and Y: {centres:?}");
    }

    /// #9898: wheeling over W applied each notch as its own Free Transform, resampling the layer
    /// again from the last result until a fast scroll destroyed it. A wheel gesture is one edit,
    /// applied when it stops, from the pixels it started with.
    #[test]
    fn wheeling_the_width_is_one_transform_when_the_wheel_stops() {
        use egui_kittest::kittest::Queryable;
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 120})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        app.run("select.rect", json!({"x": 10, "y": 10, "width": 100, "height": 60})).unwrap();
        app.run("edit.fill", json!({"contents": "color", "color": "#336699"})).unwrap();
        app.run("select.deselect", json!({})).unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(280.0, 900.0)).with_step_dt(0.05).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                if ui.ctx().cumulative_pass_nr() == 0 {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::ProMedium);
                    return;
                }
                let state = app.session.active().unwrap();
                let layer = state.doc.layer(state.active_layer.unwrap()).unwrap().clone();
                properties(app, ui, &layer);
            },
            app,
        );
        h.run_steps(3);
        let history = |h: &egui_kittest::Harness<'_, PhotocraftApp>| h.state().session.active().unwrap().history.past_len();
        let width = |h: &mut egui_kittest::Harness<'_, PhotocraftApp>| {
            let st = h.state().session.active().unwrap();
            let layer = st.doc.layer(st.active_layer.unwrap()).unwrap().clone();
            h.state_mut().layer_frame_bounds(&layer, layer.surface().unwrap()).width()
        };
        let before = history(&h);
        assert_eq!(width(&mut h), 100);
        let at = h.get_by_label("Width").rect().center();
        h.hover_at(at);
        h.run_steps(1);
        for _ in 0..8 {
            h.event(egui::Event::MouseWheel {
                source: egui::MouseWheelSource::Unknown,
                unit: egui::MouseWheelUnit::Line,
                delta: vec2(0.0, 1.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(1);
        }
        assert_eq!(history(&h), before, "nothing applied while the wheel turns");
        assert_eq!(width(&mut h), 100);
        h.run_steps(12);
        assert_eq!(history(&h), before + 1, "one edit for the whole gesture");
        assert_eq!(width(&mut h), 108);
    }

    /// A wheeled width still waiting for its gesture to end is dropped, not applied to another
    /// layer made active in the meantime (or to its own when it is made active again).
    #[test]
    fn a_wheeled_width_is_not_applied_to_another_layer() {
        use egui_kittest::kittest::Queryable;
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 120})).unwrap();
        let mut layers = Vec::new();
        for (x, w) in [(10, 100), (120, 40)] {
            app.run("layer.new.layer", json!({})).unwrap();
            app.run("select.rect", json!({"x": x, "y": 10, "width": w, "height": 40})).unwrap();
            app.run("edit.fill", json!({"contents": "color", "color": "#336699"})).unwrap();
            app.run("select.deselect", json!({})).unwrap();
            layers.push(app.session.active().unwrap().active_layer.unwrap());
        }
        app.run("layer.select", json!({"layer": layers[0].0})).unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(280.0, 900.0)).with_step_dt(0.05).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                if ui.ctx().cumulative_pass_nr() == 0 {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::ProMedium);
                    return;
                }
                let state = app.session.active().unwrap();
                let layer = state.doc.layer(state.active_layer.unwrap()).unwrap().clone();
                properties(app, ui, &layer);
            },
            app,
        );
        h.run_steps(3);
        let widths = |h: &mut egui_kittest::Harness<'_, PhotocraftApp>| {
            let doc = h.state().session.active().unwrap().doc.clone();
            layers
                .iter()
                .map(|id| doc.layer(*id).unwrap().clone())
                .map(|l| h.state_mut().layer_frame_bounds(&l, l.surface().unwrap()).width())
                .collect::<Vec<_>>()
        };
        assert_eq!(widths(&mut h), vec![100, 40]);
        let at = h.get_by_label("Width").rect().center();
        h.hover_at(at);
        h.run_steps(1);
        for _ in 0..3 {
            h.event(egui::Event::MouseWheel {
                source: egui::MouseWheelSource::Unknown,
                unit: egui::MouseWheelUnit::Line,
                delta: vec2(0.0, 1.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(1);
        }
        h.state_mut().run("layer.select", json!({"layer": layers[1].0})).unwrap();
        h.run_steps(12);
        assert_eq!(widths(&mut h), vec![100, 40], "the other layer took the first one's wheeled width");
        h.state_mut().run("layer.select", json!({"layer": layers[0].0})).unwrap();
        h.run_steps(12);
        assert_eq!(widths(&mut h), vec![100, 40], "the dropped width came back on its own layer");
    }

    /// A wheeled width whose field stops being drawn before the gesture ends (Transform
    /// collapsed) is dropped, not applied as a stale edit when the field shows again.
    #[test]
    fn a_wheeled_width_left_undrawn_is_dropped() {
        use egui_kittest::kittest::Queryable;
        let hide = egui::Id::unique("test-hide-layer-props");
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 120})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        app.run("select.rect", json!({"x": 10, "y": 10, "width": 100, "height": 60})).unwrap();
        app.run("edit.fill", json!({"contents": "color", "color": "#336699"})).unwrap();
        app.run("select.deselect", json!({})).unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(280.0, 900.0)).with_step_dt(0.05).build_ui_state(
            move |ui, app: &mut PhotocraftApp| {
                if ui.ctx().cumulative_pass_nr() == 0 {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::ProMedium);
                    return;
                }
                if ui.data(|d| d.get_temp::<bool>(hide)).unwrap_or(false) {
                    return;
                }
                let state = app.session.active().unwrap();
                let layer = state.doc.layer(state.active_layer.unwrap()).unwrap().clone();
                properties(app, ui, &layer);
            },
            app,
        );
        h.run_steps(3);
        let history = |h: &egui_kittest::Harness<'_, PhotocraftApp>| h.state().session.active().unwrap().history.past_len();
        let before = history(&h);
        let at = h.get_by_label("Width").rect().center();
        h.hover_at(at);
        h.run_steps(1);
        for _ in 0..3 {
            h.event(egui::Event::MouseWheel {
                source: egui::MouseWheelSource::Unknown,
                unit: egui::MouseWheelUnit::Line,
                delta: vec2(0.0, 1.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(1);
        }
        h.ctx.data_mut(|d| d.insert_temp(hide, true));
        h.run_steps(40);
        h.ctx.data_mut(|d| d.insert_temp(hide, false));
        h.run_steps(12);
        assert_eq!(history(&h), before, "the stale wheeled width was applied when the field showed again");
    }

    /// An Undo while a wheeled width waits drops it: applied afterwards it would redo the edit
    /// the Undo just took back, under a new history step.
    #[test]
    fn a_wheeled_width_is_dropped_by_an_undo_during_the_gesture() {
        use egui_kittest::kittest::Queryable;
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 120})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        app.run("select.rect", json!({"x": 10, "y": 10, "width": 100, "height": 60})).unwrap();
        app.run("edit.fill", json!({"contents": "color", "color": "#336699"})).unwrap();
        app.run("select.deselect", json!({})).unwrap();
        let width = |app: &mut PhotocraftApp| {
            let st = app.session.active().unwrap();
            let layer = st.doc.layer(st.active_layer.unwrap()).unwrap().clone();
            app.layer_frame_bounds(&layer, layer.surface().unwrap()).width()
        };
        let original = width(&mut app);
        let st = app.session.active().unwrap();
        let layer = st.doc.layer(st.active_layer.unwrap()).unwrap().clone();
        let bb = frame(&layer, app.layer_frame_bounds(&layer, layer.surface().unwrap()));
        let p = transform_params(layer.id.0, bb, Some((bb[2] - bb[0]) as f32 + 20.0), None, false, 0.5).unwrap();
        app.run("edit.transform", p).unwrap();
        assert!(width(&mut app) > original);
        let mut h = egui_kittest::Harness::builder().with_size(vec2(280.0, 900.0)).with_step_dt(0.05).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                if ui.ctx().cumulative_pass_nr() == 0 {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::ProMedium);
                    return;
                }
                let state = app.session.active().unwrap();
                let layer = state.doc.layer(state.active_layer.unwrap()).unwrap().clone();
                properties(app, ui, &layer);
            },
            app,
        );
        h.run_steps(3);
        let at = h.get_by_label("Width").rect().center();
        h.hover_at(at);
        h.run_steps(1);
        for _ in 0..3 {
            h.event(egui::Event::MouseWheel {
                source: egui::MouseWheelSource::Unknown,
                unit: egui::MouseWheelUnit::Line,
                delta: vec2(0.0, 1.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(1);
        }
        h.state_mut().run("edit.undo", json!({})).unwrap();
        let history = h.state().session.active().unwrap().history.past_len();
        h.run_steps(40);
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history, "the wheeled width was applied after the Undo");
        assert_eq!(width(h.state_mut()), original, "the Undo stands");
    }

    /// A wheeled width is dropped when a layer without these fields (a group) is shown, even for
    /// a moment: going back to the layer before the gesture ends must not apply it.
    #[test]
    fn a_wheeled_width_is_dropped_by_a_moment_on_a_group() {
        use egui_kittest::kittest::Queryable;
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 120})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        app.run("select.rect", json!({"x": 10, "y": 10, "width": 100, "height": 60})).unwrap();
        app.run("edit.fill", json!({"contents": "color", "color": "#336699"})).unwrap();
        app.run("select.deselect", json!({})).unwrap();
        let layer = app.session.active().unwrap().active_layer.unwrap();
        app.run("layer.new.group", json!({})).unwrap();
        let group = app.session.active().unwrap().active_layer.unwrap();
        app.run("layer.select", json!({"layer": layer.0})).unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(280.0, 900.0)).with_step_dt(0.05).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                if ui.ctx().cumulative_pass_nr() == 0 {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::ProMedium);
                    return;
                }
                let state = app.session.active().unwrap();
                let layer = state.doc.layer(state.active_layer.unwrap()).unwrap().clone();
                properties(app, ui, &layer);
            },
            app,
        );
        h.run_steps(3);
        let history = |h: &egui_kittest::Harness<'_, PhotocraftApp>| h.state().session.active().unwrap().history.past_len();
        let at = h.get_by_label("Width").rect().center();
        h.hover_at(at);
        h.run_steps(1);
        for _ in 0..3 {
            h.event(egui::Event::MouseWheel {
                source: egui::MouseWheelSource::Unknown,
                unit: egui::MouseWheelUnit::Line,
                delta: vec2(0.0, 1.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(1);
        }
        h.state_mut().run("layer.select", json!({"layer": group.0})).unwrap();
        h.run_steps(2);
        h.state_mut().run("layer.select", json!({"layer": layer.0})).unwrap();
        let before = history(&h);
        h.run_steps(40);
        assert_eq!(history(&h), before, "the wheeled width was applied after a moment on the group");
    }

    /// Moving straight from wheeling W to wheeling H is still W's gesture: W's value lands as
    /// soon as the wheel turns off it, and H doesn't step until a gesture of its own, so the
    /// two never apply from the same original box together.
    #[test]
    fn wheeling_width_then_height_applies_each_once() {
        use egui_kittest::kittest::Queryable;
        let setup = || {
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
            app.run("file.new", json!({"width": 200, "height": 120})).unwrap();
            app.run("layer.new.layer", json!({})).unwrap();
            app.run("select.rect", json!({"x": 10, "y": 10, "width": 100, "height": 50})).unwrap();
            app.run("edit.fill", json!({"contents": "color", "color": "#336699"})).unwrap();
            app.run("select.deselect", json!({})).unwrap();
            app
        };
        let app = setup();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(280.0, 900.0)).with_step_dt(0.05).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                if ui.ctx().cumulative_pass_nr() == 0 {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::ProMedium);
                    return;
                }
                let state = app.session.active().unwrap();
                let layer = state.doc.layer(state.active_layer.unwrap()).unwrap().clone();
                properties(app, ui, &layer);
            },
            app,
        );
        h.run_steps(3);
        let history = |h: &egui_kittest::Harness<'_, PhotocraftApp>| h.state().session.active().unwrap().history.past_len();
        let size = |h: &mut egui_kittest::Harness<'_, PhotocraftApp>| {
            let st = h.state().session.active().unwrap();
            let layer = st.doc.layer(st.active_layer.unwrap()).unwrap().clone();
            let r = h.state_mut().layer_frame_bounds(&layer, layer.surface().unwrap());
            (r.width(), r.height())
        };
        let before = history(&h);
        let notch = |h: &mut egui_kittest::Harness<'_, PhotocraftApp>| {
            h.event(egui::Event::MouseWheel {
                source: egui::MouseWheelSource::Unknown,
                unit: egui::MouseWheelUnit::Line,
                delta: vec2(0.0, 1.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(1);
        };
        let (w_at, h_at) = (h.get_by_label("Width").rect().center(), h.get_by_label("Height").rect().center());
        // What W's four notches alone make, and H's two after them, applied the same way on a copy
        // of the document.
        let (w_only, then_h) = {
            let mut app = setup();
            let st = app.session.active().unwrap();
            let layer = st.doc.layer(st.active_layer.unwrap()).unwrap().clone();
            let bb = frame(&layer, app.layer_frame_bounds(&layer, layer.surface().unwrap()));
            let p = transform_params(layer.id.0, bb, Some((bb[2] - bb[0]) as f32 + 4.0), None, true, 0.5).unwrap();
            app.run("edit.transform", p).unwrap();
            let st = app.session.active().unwrap();
            let layer = st.doc.layer(st.active_layer.unwrap()).unwrap().clone();
            let r = app.layer_frame_bounds(&layer, layer.surface().unwrap());
            let w_only = (r.width(), r.height());
            let bb = frame(&layer, r);
            let p = transform_params(layer.id.0, bb, None, Some((bb[3] - bb[1]) as f32 + 2.0), true, 0.5).unwrap();
            app.run("edit.transform", p).unwrap();
            let st = app.session.active().unwrap();
            let layer = st.doc.layer(st.active_layer.unwrap()).unwrap().clone();
            let r = app.layer_frame_bounds(&layer, layer.surface().unwrap());
            (w_only, (r.width(), r.height()))
        };
        h.hover_at(w_at);
        h.run_steps(1);
        for _ in 0..4 {
            notch(&mut h);
        }
        h.hover_at(h_at);
        h.run_steps(1);
        for _ in 0..4 {
            notch(&mut h);
        }
        h.run_steps(20);
        assert_eq!(history(&h), before + 1, "W applied once, H not stepped by W's gesture");
        assert_eq!(size(&mut h), w_only, "W's four notches, linked, and nothing from H");
        // A new gesture over H is H's own.
        for _ in 0..2 {
            notch(&mut h);
        }
        h.run_steps(20);
        assert_eq!(history(&h), before + 2);
        assert_eq!(size(&mut h), then_h, "H's two notches, linked");
    }

    /// Types each `(name, text)` of `edits` in turn into the Transform field named `name` of an
    /// 80 × 40 px rectangle shape at (10, 10) in a 200 × 120 document, with the rulers in `unit`,
    /// and returns its path's [x, y, w, h] (a shape's path scales exactly, where resampled pixels
    /// would blur its edge). Each edit is committed with Enter.
    fn type_into_transform_fields(unit: photocraft_engine::prefs::Unit, edits: &[(&str, &str)]) -> [f64; 4] {
        type_into_transform_fields_by(unit, edits, true)
    }

    /// [`type_into_transform_fields`], committing each edit with Enter when `enter`, or else by
    /// clicking into the next field (the last edit still with Enter).
    fn type_into_transform_fields_by(unit: photocraft_engine::prefs::Unit, edits: &[(&str, &str)], enter: bool) -> [f64; 4] {
        use egui_kittest::kittest::Queryable;
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.session.prefs.edit(|p| p.units_and_rulers.rulers = unit);
        app.run("file.new", json!({"width": 200, "height": 120})).unwrap();
        app.run("shape.create", json!({"kind": "rect", "rect": [10, 10, 80, 40], "fill": "#336699"})).unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(280.0, 900.0)).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                if ui.ctx().cumulative_pass_nr() == 0 {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::ProMedium);
                    return;
                }
                let state = app.session.active().unwrap();
                let layer = state.doc.layer(state.active_layer.unwrap()).unwrap().clone();
                properties(app, ui, &layer);
            },
            app,
        );
        h.run_steps(3);
        for (i, (name, text)) in edits.iter().enumerate() {
            h.get_by_label(name).click();
            h.run();
            for c in text.chars() {
                h.event(egui::Event::Text(c.to_string()));
                h.run();
            }
            if enter || i + 1 == edits.len() {
                h.key_press(egui::Key::Enter);
                h.run_steps(3);
            }
        }
        let st = h.state().session.active().unwrap();
        let LayerContent::Shape(sh) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!("still a shape") };
        let c = sh.path.control_bounds().unwrap();
        [c.0, c.1, c.2 - c.0, c.3 - c.1].map(|v| (v * 100.0).round() / 100.0)
    }

    /// #10784: a percentage typed into a W or H showing a length is that share of the layer's
    /// size, and into X or Y that share of the document, as in Photoshop; never that many pixels.
    #[test]
    fn transform_fields_take_a_typed_percentage() {
        use photocraft_engine::prefs::Unit;
        // Linked W/H: half the width halves the height too (typed live, `50` alone would be 50 px).
        assert_eq!(type_into_transform_fields(Unit::Pixels, &[("Width", "50%")]), [10.0, 10.0, 40.0, 20.0]);
        assert_eq!(type_into_transform_fields(Unit::Pixels, &[("Height", "150%")]), [10.0, 10.0, 120.0, 60.0]);
        assert_eq!(type_into_transform_fields(Unit::Pixels, &[("X", "50%")]), [100.0, 10.0, 80.0, 40.0]);
        assert_eq!(type_into_transform_fields(Unit::Pixels, &[("Y", "50%")]), [10.0, 60.0, 80.0, 40.0]);
        // A second edit takes its percentage of the size the first one left.
        assert_eq!(type_into_transform_fields(Unit::Pixels, &[("Width", "50%"), ("Width", "50%")]), [10.0, 10.0, 20.0, 10.0]);
        // Clicking into the linked H commits W as H takes the focus: H's `50%` is half the height
        // W left (#10802 review), and the same from H into W.
        assert_eq!(type_into_transform_fields_by(Unit::Pixels, &[("Width", "50%"), ("Height", "50%")], false), [10.0, 10.0, 20.0, 10.0]);
        assert_eq!(type_into_transform_fields_by(Unit::Pixels, &[("Height", "50%"), ("Width", "50%")], false), [10.0, 10.0, 20.0, 10.0]);
        // And with Enter between them.
        assert_eq!(type_into_transform_fields(Unit::Pixels, &[("Width", "50%"), ("Height", "50%")]), [10.0, 10.0, 20.0, 10.0]);
        // In inches too (72 dpi: 80 px is 1.11 in).
        assert_eq!(type_into_transform_fields(Unit::Inches, &[("Width", "50%")]), [10.0, 10.0, 40.0, 20.0]);
        // A plain number and another length are unchanged.
        assert_eq!(type_into_transform_fields(Unit::Pixels, &[("Width", "60")]), [10.0, 10.0, 60.0, 30.0]);
        assert_eq!(type_into_transform_fields(Unit::Pixels, &[("Width", "1in")]), [10.0, 10.0, 72.0, 36.0]);
        // Rulers in percent show W as a share of the 200 px document; a typed length is that length.
        assert_eq!(type_into_transform_fields(Unit::Percent, &[("Width", "40px")]), [10.0, 10.0, 40.0, 20.0]);
        assert_eq!(type_into_transform_fields(Unit::Percent, &[("Width", "25")]), [10.0, 10.0, 50.0, 25.0]);
    }

    #[test]
    fn transform_fields_show_and_take_the_ruler_unit() {
        use photocraft_engine::prefs::Unit;
        // 72 dpi: 1644 px = 58 cm; typing 60 cm sets 1700.8 px
        let u = LenUnit { unit: Unit::Centimeters, dpi: 72.0, ppi: 72.0 };
        assert!((u.shown(1644.0, 1.0) - 58.0).abs() < 0.01);
        assert!((u.px(60.0, 1.0) - 1700.787).abs() < 0.01);
        let px = LenUnit { unit: Unit::Pixels, dpi: 150.0, ppi: 72.0 };
        assert_eq!(px.shown(170.0, 1.0), 170.0);
        assert_eq!(px.px(170.0, 1.0), 170.0);
        let pc = LenUnit { unit: Unit::Percent, dpi: 72.0, ppi: 72.0 };
        assert_eq!(pc.shown(822.0, 1644.0), 50.0);
>>>>>>> upstream/main
    }

    #[test]
    fn width_edit_scales_from_the_top_left() {
        let p = transform_params(7, [10.0, 20.0, 110.0, 70.0], Some(200.0), None, true, 0.5).unwrap();
        assert_eq!(p["layer"], 7);
        assert_eq!(p["quad"], json!([[10.0, 20.0], [210.0, 20.0], [210.0, 120.0], [10.0, 120.0]]));
        let p = transform_params(7, [10.0, 20.0, 110.0, 70.0], None, Some(25.0), false, 0.5).unwrap();
        assert_eq!(p["quad"][2], json!([110.0, 45.0]));
        assert!(transform_params(7, [10.0, 20.0, 110.0, 70.0], Some(100.0), None, true, 0.5).is_none());
        assert!(transform_params(7, [0.0, 0.0, 0.0, 0.0], Some(5.0), None, true, 0.5).is_none());
    }

    #[test]
    fn transform_params_resize_a_layer_in_the_engine() {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": 200, "height": 200, "background": "transparent"})).unwrap();
        s.execute("select.rect", json!({"x": 10, "y": 20, "width": 100, "height": 50})).unwrap();
        s.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
        s.execute("select.deselect", json!({})).unwrap();
        let st = s.active().unwrap();
        let id = st.active_layer.unwrap();
        let p = transform_params(id.0, [10.0, 20.0, 110.0, 70.0], Some(50.0), None, true, 0.5).unwrap();
        s.execute("edit.transform", p).unwrap();
        let b = s.active().unwrap().doc.layer(id).unwrap().surface().unwrap().content_bounds();
        assert_eq!((b.x0, b.y0, b.width(), b.height()), (10, 20, 50, 25));
    }

    /// Linked W/H edits must generate a truly uniform affine scale, not one
    /// rounded independently on each axis to f32 precision.
    #[test]
    fn linked_transform_preserves_scale_precision() {
        let b = [203.6, 171.7, 712.4, 690.3];
        let p = transform_params(7, b, Some(600.0), None, true, 0.05).unwrap();
        let x0 = p["quad"][0][0].as_f64().unwrap();
        let y0 = p["quad"][0][1].as_f64().unwrap();
        let x1 = p["quad"][1][0].as_f64().unwrap();
        let y1 = p["quad"][2][1].as_f64().unwrap();
        let sx = (x1 - x0) / (b[2] - b[0]);
        let sy = (y1 - y0) / (b[3] - b[1]);
        assert!((sx - sy).abs() < 1e-12, "linked resize produced different scales: {sx} and {sy}");
    }

    /// A diagonal arrow must remain live after a proportional width edit in
    /// Properties, while its outline is transformed by the engine as usual.
    #[test]
    fn linked_properties_resize_keeps_diagonal_arrow_editable() {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": 1200, "height": 1200})).unwrap();
        s.execute(
            "shape.create",
            json!({
                "kind": "line",
                "from": [203.6, 171.7],
                "to": [712.4, 690.3],
                "weight": 20.0,
                "arrowStart": false,
                "arrowEnd": true,
                "arrowWidth": 55.0,
                "arrowLength": 85.0,
                "arrowConcavity": 50.0
            }),
        )
        .unwrap();
        let (id, b) = {
            let st = s.active().unwrap();
            let id = st.active_layer.unwrap();
            let layer = st.doc.layer(id).unwrap();
            let LayerContent::Shape(shape) = &layer.content else { panic!("expected shape layer") };
            assert!(matches!(shape.live.as_ref(), Some(photocraft_doc::vector::LiveShape::ArrowLine { .. })));
            let px = photocraft_compose::bounds::content_bounds(layer.surface().unwrap());
            (id, frame(layer, px))
        };
        let p = transform_params(id.0, b, Some(600.0), None, true, 0.05).unwrap();
        s.execute("edit.transform", p).unwrap();
        let layer = s.active().unwrap().doc.layer(id).unwrap();
        let LayerContent::Shape(shape) = &layer.content else { panic!("expected shape layer") };
        assert!(
            matches!(shape.live.as_ref(), Some(photocraft_doc::vector::LiveShape::ArrowLine { .. })),
            "proportional Properties resize must preserve live arrow settings"
        );

        // The next live-property edit must still be accepted after resizing.
        s.execute("shape.edit", json!({"layer": id.0, "arrowConcavity": 40.0})).unwrap();
        let layer = s.active().unwrap().doc.layer(id).unwrap();
        let LayerContent::Shape(shape) = &layer.content else { panic!("expected shape layer") };
        assert!(matches!(shape.live.as_ref(), Some(photocraft_doc::vector::LiveShape::ArrowLine { concavity, .. }) if (*concavity - 40.0).abs() < 1e-9));
    }

    /// A stroked shape's Transform fields show its path, not the stroke's pixels, and a W edit
    /// scales the path to that width, as in Photoshop.
    #[test]
    fn shape_fields_show_and_scale_the_path_without_its_stroke() {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": 800, "height": 600})).unwrap();
        s.execute("shape.create", json!({"kind": "rect", "rect": [200, 150, 400, 300], "fill": "#3366cc", "stroke": {"width": 15, "color": "#000000"}}))
            .unwrap();
        let st = s.active().unwrap();
        let id = st.active_layer.unwrap();
        let layer = st.doc.layer(id).unwrap();
        let px = photocraft_compose::bounds::content_bounds(layer.surface().unwrap());
        assert!(px.width() > 400, "the stroke's pixels reach past the path: {px:?}");
        let bb = frame(layer, px);
        assert_eq!(bb, [200.0, 150.0, 600.0, 450.0]);
        s.execute("edit.transform", transform_params(id.0, bb, Some(200.0), None, true, 0.5).unwrap()).unwrap();
        let LayerContent::Shape(sh) = &s.active().unwrap().doc.layer(id).unwrap().content else { panic!("still a shape") };
        let c = sh.path.control_bounds().unwrap();
        assert_eq!((c.0, c.1, c.2 - c.0, c.3 - c.1), (200.0, 150.0, 200.0, 150.0), "the path is exactly 200 × 150");
    }

    /// #9181 review: a shape's fractional path takes a W edit smaller than half a pixel (400.25
    /// px typed as 400), while a pixel layer still ignores sub-pixel noise.
    #[test]
    fn a_shape_path_takes_a_width_edit_under_half_a_pixel() {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": 800, "height": 600})).unwrap();
        s.execute("shape.create", json!({"kind": "rect", "rect": [200, 150, 400.25, 100], "fill": "#3366cc"})).unwrap();
        let st = s.active().unwrap();
        let id = st.active_layer.unwrap();
        let layer = st.doc.layer(id).unwrap();
        // One shown decimal step of the pixel ruler at 72 ppi.
        let step_px = 0.05;
        let bb = frame(layer, photocraft_compose::bounds::content_bounds(layer.surface().unwrap()));
        assert_eq!(bb[2] - bb[0], 400.25);
        let tol = edit_tolerance(layer, step_px);
        assert!(tol < 0.25, "{tol}");
        let p = transform_params(id.0, bb, Some(400.0), None, false, tol).expect("a quarter-pixel edit applies");
        s.execute("edit.transform", p).unwrap();
        let LayerContent::Shape(sh) = &s.active().unwrap().doc.layer(id).unwrap().content else { panic!("still a shape") };
        let c = sh.path.control_bounds().unwrap();
        assert!((c.2 - c.0 - 400.0).abs() < 1e-6, "the path is exactly 400 wide: {c:?}");
        // The field's own rounding (400.25 shown as 400.3) is not an edit.
        assert!(transform_params(id.0, bb, Some(400.3), None, false, tol).is_none());
        s.execute("layer.new.layer", json!({})).unwrap();
        let st = s.active().unwrap();
        assert_eq!(edit_tolerance(st.doc.layer(st.active_layer.unwrap()).unwrap(), step_px), 0.5, "a pixel layer keeps half a pixel");
    }
}
