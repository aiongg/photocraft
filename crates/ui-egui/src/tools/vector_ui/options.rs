use craft::behavior::EditEvent;
use craft::color::ui::Paint;
use craft::draw::ui::pen::PathOptionsPopover;
use craft::draw::ui::shape::{self as fw, Arrowheads, ArrowheadsPopover, ShapePanel, StrokeCells, VectorMode, VectorModeSelect};
use craft::theme::Srgba;
use craft::ui::buttons::Button;
use craft::ui::choice::Checkbox;
use craft::ui::contract::{Bind, Props as _, Show as _, ShowContainer as _, clicked};
use craft::ui::display::{Divider, Text};
use craft::ui::layout_components::Row;
use craft::ui::runtime::env::{Label, UnitKind};
use craft::ui::text::NumberField;

use super::path_menus::{path_alignment_menu, path_arrangement_menu, path_operations_menu};
use super::path_ops::active_path_name;
use super::shapes::is_shape_tool;
use super::{LayerContent, PhotocraftApp, Tool, Value, copy_arrow_to_tool_defaults, json};
use crate::history_keys;

// ---------------------------------------------------------------------------------------------
// Options bars

/// The popover flags of the bar, kept by the framework per viewer.
#[derive(Clone, Copy, Default)]
struct BarOpen {
    path_options: bool,
    arrows: bool,
}

/// A label from a translated string.
fn label(text: &str) -> Label {
    Label::from(text.to_string())
}

/// The document's pixels per inch, which sizes lengths typed in inches or points.
fn document_ppi(app: &PhotocraftApp) -> f64 {
    app.session.active().map_or(72.0, |st| f64::from(st.doc.resolution_dpi))
}

/// A line of muted help text in the bar.
fn hint(ui: &mut egui::Ui, text: &str) {
    Text::new(label(text)).show(ui);
}

const PEN_MODES: [(VectorMode, bool); 3] = [(VectorMode::Shape, true), (VectorMode::Path, true), (VectorMode::Pixels, false)];
const OTHER_PEN_MODES: [(VectorMode, bool); 2] = [(VectorMode::Path, true), (VectorMode::Shape, true)];
const SHAPE_TOOL_MODES: [(VectorMode, bool); 2] = [(VectorMode::Shape, true), (VectorMode::Path, true)];
const SHAPE_ONLY: [(VectorMode, bool); 1] = [(VectorMode::Shape, true)];

/// Options bar for vector tools; false for other tools.
pub fn options_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui, tool: Tool) -> bool {
    if !(is_shape_tool(tool)
        || matches!(
            tool,
            Tool::Pen | Tool::FreeformPen | Tool::CurvaturePen | Tool::AddAnchorPoint | Tool::DeleteAnchorPoint | Tool::PathSelection | Tool::DirectSelection
        ))
    {
        return false;
    }
    Row::new().show(ui, |ui, cx| {
        // PLAN: assumed the bar's popover flags live in the body's shown-memory slot (`BodyCx::memory.shown`); the contract lists no public accessor.
        let open = cx.memory.shown::<BarOpen>(cx.id);
        bar(app, ui, tool, open);
    });
    true
}

/// The cells of the bar for `tool`.
fn bar(app: &mut PhotocraftApp, ui: &mut egui::Ui, tool: Tool, open: &mut BarOpen) {
    let target = selected_shape(&app.session);
    // Direct Selection shares the Path Selection bar (#13219).
    if matches!(tool, Tool::PathSelection | Tool::DirectSelection) && !crate::direct_select::active_paths(app).is_empty() {
        path_operations_menu(app, ui);
        Divider::vertical().show(ui);
    }
<<<<<<< HEAD
    if tool == Tool::PathSelection && target.is_none() {
        hint(ui, if app.ui.vector_mask_target { "Drag to move the targeted vector mask" } else { tl!("Drag to move the selected path") });
        return;
    }
    if tool == Tool::DirectSelection && target.is_none() {
        let (shift, alt) = (crate::shortcuts::pretty("Shift"), crate::shortcuts::pretty("Alt"));
        hint(
            ui,
            &crate::i18n::fmt(
                tl!("Click: select anchor · Drag: move anchor, handle or segment · {shift}-click: add · {alt}-click: whole subpath"),
                &[("shift", &shift), ("alt", &alt)],
            ),
        );
        return;
=======
    let o = &mut app.ui.tool_options;
    if matches!(tool, Tool::PathSelection | Tool::DirectSelection) && target.is_none() {
        if tool == Tool::PathSelection {
            lbl(ui, if app.ui.vector_mask_target { "Drag to move the targeted vector mask" } else { tl!("Drag to move the selected path") });
        }
        return true;
>>>>>>> upstream/main
    }
    if tool == Tool::AddAnchorPoint {
        hint(ui, tl!("No options for the Add Anchor Point Tool."));
        return;
    }
    if tool == Tool::DeleteAnchorPoint {
        hint(ui, tl!("No options for the Delete Anchor Point Tool."));
        return;
    }
    if matches!(tool, Tool::Pen | Tool::FreeformPen | Tool::CurvaturePen) {
        pen_bar(app, ui, tool, open);
        return;
    }
    if is_shape_tool(tool) {
<<<<<<< HEAD
        // Shape draws a shape layer, Path adds the outline to the work path.
        VectorModeSelect::new(Bind::bound(&mut app.ui.tool_options.shape_mode), &SHAPE_TOOL_MODES).show(ui);
=======
        // Shape draws a shape layer, Path adds the outline to the work path (#9453).
        let opts = [("shape".to_string(), tl!("Shape")), ("path".to_string(), tl!("Path"))];
        crate::widgets::dropdown(ui, "shape-mode", tl!("Mode"), &mut app.ui.tool_options.shape_mode, &opts, 80.0);
        if draws_path(&app.ui.tool_options) {
            crate::widgets::vline(ui, 22.0);
            make_buttons(app, ui);
        }
>>>>>>> upstream/main
    } else {
        // Path Selection and Direct Selection edit the selected shape layer: the Shape tools'
        // Path mode isn't theirs to show or change.
        let mut mode = VectorMode::Shape;
        VectorModeSelect::new(Bind::bound(&mut mode), &SHAPE_ONLY).show(ui);
    }
    Divider::vertical().show(ui);
    // The Shape tools' Path mode hides Fill and Stroke (an outline has no paint); the tool's
    // geometry, the Line's arrowheads included, stays.
    let paint = !(is_shape_tool(tool) && draws_path(&app.ui.tool_options));
    crate::shape_options::options_bar(app, ui, tool, paint);
    if is_shape_tool(tool) {
        // Path operations, alignment and arrangement, in Shape and Path mode.
        Divider::vertical().show(ui);
        path_operations_menu(app, ui);
        path_alignment_menu(app, ui);
        path_arrangement_menu(app, ui);
    }
<<<<<<< HEAD
    if tool == Tool::Line {
        Divider::vertical().show(ui);
        let ppi = document_ppi(app);
        let o = &mut app.ui.tool_options;
        let mut arrows = Arrowheads::new();
        arrows.start = o.arrow_start;
        arrows.end = o.arrow_end;
        arrows.width = f64::from(o.arrow_width);
        arrows.length = f64::from(o.arrow_length);
        arrows.concavity = f64::from(o.arrow_concavity);
        let out = ArrowheadsPopover::new(Bind::given(arrows), Bind::bound(&mut open.arrows), ppi).show(ui);
        if let Some(a) = out.edits.iter().rev().find_map(EditEvent::value) {
            o.arrow_start = a.start;
            o.arrow_end = a.end;
            o.arrow_width = a.width as f32;
            o.arrow_length = a.length as f32;
            o.arrow_concavity = a.concavity as f32;
=======
    if tool == Tool::Rectangle {
        crate::widgets::vline(ui, 22.0);
        rectangle_gear(&mut app.ui.tool_options, ui);
    }
    // Photoshop-style arrowhead settings at the right of Line options.
    if tool == Tool::Line {
        crate::widgets::vline(ui, 22.0);
        let mut concavity_alert = false;
        // Stays open on clicks inside, like the Pen's gear: a click into a field to type a value
        // would otherwise close the menu before the value was entered (#13258).
        let config = egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
        let gear = egui::Button::image(crate::icons::image("settings", 18.0, t.icon));
        let menu = egui::containers::menu::MenuButton::from_button(gear).config(config);
        let (gear, _) = menu.ui(ui, |ui| {
            let o = &mut app.ui.tool_options;
            ui.set_min_width(170.0);
            ui.label(tl!("Arrowheads"));
            ui.separator();
            ui.horizontal(|ui| {
                ui.checkbox(&mut o.arrow_start, tl!("Start"));
                ui.checkbox(&mut o.arrow_end, tl!("End"));
            });
            ui.horizontal(|ui| {
                ui.label(tl!("Width"));
                ui.add(egui::DragValue::new(&mut o.arrow_width).range(1.0..=4096.0).suffix(" px"));
            });
            ui.horizontal(|ui| {
                ui.label(tl!("Length"));
                ui.add(egui::DragValue::new(&mut o.arrow_length).range(1.0..=4096.0).suffix(" px"));
            });
            ui.horizontal(|ui| {
                let label = tl!("Concavity");
                ui.label(label);
                concavity_alert = crate::widgets::value_field_whole_alerting(ui, label, &mut o.arrow_concavity, CONCAVITY_RANGE, "%", 52.0).1;
            });
        });
        gear.on_hover_text(tl!("Arrowhead options"));
        if concavity_alert {
            crate::panels::options_bar::range_alert(&mut app.ui, CONCAVITY_RANGE_ALERT);
>>>>>>> upstream/main
        }
    }
}

<<<<<<< HEAD
/// The Pen, Freeform Pen and Curvature Pen bar.
fn pen_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui, tool: Tool, open: &mut BarOpen) {
    // Photoshop's Pen lists Shape, Path and a greyed Pixels; the other pens keep theirs.
    let offered: &[(VectorMode, bool)] = if tool == Tool::Pen { &PEN_MODES } else { &OTHER_PEN_MODES };
    VectorModeSelect::new(Bind::bound(&mut app.ui.tool_options.vector_mode), offered).show(ui);
    Divider::vertical().show(ui);
    if app.ui.tool_options.vector_mode == VectorMode::Shape {
        let ppi = document_ppi(app);
        let o = &mut app.ui.tool_options;
        let look = fw::ShapeAppearance::new(Paint::None, Paint::None, f64::from(o.stroke_width)).stroke_style((&o.shape_stroke).into());
        let events = StrokeCells::new(&look, ppi).corners(true).stroke_open(Bind::bound(&mut app.ui.shape_stroke_open)).show(ui);
        for event in &events {
            match event.value() {
                Some(fw::ShapeChange::StrokeWidth(w)) => o.stroke_width = *w as f32,
                Some(fw::ShapeChange::StrokeStyle { style, width, .. }) => {
                    o.shape_stroke = crate::shape_stroke_ui::StrokeOptions::from_style(style);
                    if let Some(w) = width {
                        o.stroke_width = *w as f32;
                    }
                }
                _ => {}
            }
        }
    }
    Divider::vertical().show(ui);
    path_operations_menu(app, ui);
    let o = &mut app.ui.tool_options;
    if tool == Tool::Pen {
        Divider::vertical().show(ui);
        // The gear's popover replaces the Rubber Band item (tools-03 D3).
        // PLAN: `PathOptionsOutput::pick_color` (key PATH_OPTIONS_COLOR_KEY) is answered by the owner of the colour picker dialog.
        let _ = PathOptionsPopover::new(label(tl!("Set additional pen and path options")), Bind::bound(&mut o.path_options)).show(ui, &mut open.path_options);
        Checkbox::new(Bind::bound(&mut o.pen_auto_add_delete)).label(label(tl!("Auto Add/Delete"))).show(ui);
    }
    let enter = crate::shortcuts::pretty("Enter");
    match tool {
        Tool::FreeformPen => {
            Divider::vertical().show(ui);
            let field =
                NumberField::new(label(tl!("Curve Fit:")), Bind::given(Some(f64::from(o.curve_fit)))).range(0.5..=10.0).unit_kind(UnitKind::Pixels).show(ui);
            if let Some(Some(v)) = field.value().copied() {
                o.curve_fit = v as f32;
            }
            let alt = crate::shortcuts::pretty("Alt");
            hint(ui, &crate::i18n::fmt(tl!("Drag: draw freehand · {alt}-click: straight segment · Release on the start point: close"), &[("alt", &alt)]));
        }
        Tool::CurvaturePen => hint(
            ui,
            &crate::i18n::fmt(
                tl!("Click: smooth point · Double-click: corner · Drag a point: move · Click first point: close · {key} finish"),
                &[("key", &enter)],
            ),
        ),
        _ => hint(
            ui,
            &crate::i18n::fmt(
                tl!("Click: corner · Drag: smooth · {alt}-drag: break handle · Click or drag first point: close · {key} finish · Esc cancel"),
                &[("alt", &crate::shortcuts::pretty("Alt")), ("key", &enter)],
            ),
        ),
    }
    if app.ui.tool_options.vector_mode == VectorMode::Path {
        Divider::vertical().show(ui);
        make_buttons(app, ui);
    }
}

/// Path mode's "Make:" row, as in Photoshop's options bar: Selection…, Mask and Shape, from the
/// path being drawn or the active path; all greyed without one.
=======
/// The Rectangle tool's gear, after the path arrangement as in the reference application
/// (#13343): Unconstrained (the default), Square, Fixed Size and Proportional, each size's W and H
/// greyed unless its radio is chosen, and From Center. The settings are tool options; Escape
/// closes the popup and keeps them. The Path Options at its top are #9971's.
fn rectangle_gear(o: &mut crate::state::ToolOptions, ui: &mut egui::Ui) {
    let gear = crate::icons::button_with_icon_size(ui, "settings", 28.0, 20.0, false, tl!("Set additional shape and path options"));
    egui::Popup::menu(&gear).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_min_width(220.0);
        ui.radio_value(&mut o.rect_constraint, "unconstrained".to_string(), tl!("Unconstrained"));
        ui.radio_value(&mut o.rect_constraint, "square".to_string(), tl!("Square"));
        // An empty field is 0: Fixed Size starts empty.
        let sizes = [
            ("fixedSize", tl!("Fixed Size"), [tl!("Fixed Size W:"), tl!("Fixed Size H:")], "px", 0, 300_000.0),
            ("proportional", tl!("Proportional"), [tl!("Proportional W:"), tl!("Proportional H:")], "", 3, 99_999.0),
        ];
        for (key, label, names, suffix, decimals, max) in sizes {
            ui.horizontal(|ui| {
                ui.radio_value(&mut o.rect_constraint, key.to_string(), label);
                let on = o.rect_constraint == key;
                let (w, h) =
                    if key == "fixedSize" { (&mut o.rect_fixed_width, &mut o.rect_fixed_height) } else { (&mut o.rect_ratio_width, &mut o.rect_ratio_height) };
                ui.add_enabled_ui(on, |ui| {
                    for (value, name, caption) in [(w, names[0], tl!("W:")), (h, names[1], tl!("H:"))] {
                        ui.label(caption);
                        crate::widgets::value_field_prec_blank(ui, value, 0.0..=max, suffix, 72.0, decimals).accessible_name(name);
                    }
                });
            });
        }
        ui.checkbox(&mut o.rect_from_center, tl!("From Center"));
    });
}

/// Arrowhead Concavity in percent of the head's length, as the reference takes it (#13258).
pub(crate) const CONCAVITY_RANGE: std::ops::RangeInclusive<f32> = -50.0..=50.0;

/// The reference's alert for a Concavity typed past [`CONCAVITY_RANGE`] (#13258).
pub(crate) const CONCAVITY_RANGE_ALERT: &str = "An integer between -50% and 50% is required. Closest value inserted.";

/// Path mode's "Make:" row, as in Photoshop's options bar: Selection… (#5703), Mask (#12904) and
/// Shape (#10934), from the path being drawn or the active path; all greyed without one.
>>>>>>> upstream/main
fn make_buttons(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    hint(ui, tl!("Make:"));
    let pending = app.ui.pen.as_ref().is_some_and(|p| p.draft.knots.len() >= 2);
    let has_path = pending || active_path_name(app).is_some();
    // A draft that finishes into the targeted vector mask has no path of its own to make a mask
    // from, as in the canvas menu's Create Vector Mask.
    let has_mask_path = has_path && !(pending && super::path_ops::targeted_vector_mask(app).is_some());
    // Make: Shape turns the path into a shape layer with the shape fill and stroke.
    let has_shape_path = pending || super::path_ops::shape_source_path(app).is_some();
    let selection = clicked(&Button::new(label(tl!("Selection…"))).enabled(has_path).show(ui));
    let mask = clicked(&Button::new(label(tl!("Mask"))).enabled(has_mask_path).show(ui));
    let shape = clicked(&Button::new(label(tl!("Shape"))).enabled(has_shape_path).show(ui));
    let made = if selection {
        crate::canvas_tool_menu::pen_dialogs::make_selection(app)
    } else if mask {
        make_mask(app)
    } else if shape {
        super::make_shape(app).map(|_| ())
    } else {
        Ok(())
    };
    if let Err(e) = made {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

/// Make: Mask: a vector mask on the active layer from the path being drawn (finished
/// first, as the canvas menu's Create Vector Mask does) or the active path. The command records
/// the History step Add Vector Mask and turns the Background into a normal layer in that edit.
fn make_mask(app: &mut PhotocraftApp) -> Result<(), String> {
    if app.ui.pen.as_ref().is_some_and(|p| p.draft.knots.len() >= 2) && !super::pen::pen_commit(app, false) {
        return Err(app.ui.status.clone());
    }
    let name =
        active_path_name(app).ok_or_else(|| tl!("No path to make a mask from: draw one with the Pen tool or select one in the Paths panel.").to_string())?;
    app.run("layer.vectorMask.fromPath", json!({"name": name})).map(|_| ())
}

/// Whether the Shape tools are in Path mode: a drag adds its outline to the work path.
pub(crate) fn draws_path(o: &crate::state::ToolOptions) -> bool {
    o.shape_mode == VectorMode::Path
}

/// Whether the active layer supplies the selected-shape options.
pub(super) fn selected_shape(session: &photocraft_engine::Session) -> Option<photocraft_doc::LayerId> {
    let st = session.active()?;
    let layer = st.active_layer?;
    matches!(st.doc.layer(layer)?.content, LayerContent::Shape(_)).then_some(layer)
}

// ---------------------------------------------------------------------------------------------
// Properties (shape layers)

pub(super) fn solid_paint(c: [f32; 4]) -> photocraft_doc::Fill {
    photocraft_doc::Fill::Solid(photocraft_doc::Color::rgba(c[0], c[1], c[2], c[3]))
}

pub(crate) fn enabled_stroke_width(width: f32) -> f32 {
    if width.is_finite() && width > 0.0 { width } else { 3.0 }
}

/// One corner of the live rectangle, preserving the other three radii.
/// The engine takes the full [top-left, top-right, bottom-right, bottom-left] array.
pub(super) fn corner_radii_patch(radii: [f64; 4], corner: usize, radius: f32) -> Value {
    let mut next = radii;
    if let Some(r) = next.get_mut(corner) {
        *r = f64::from(radius.max(0.0));
    }
    json!({"radii": next})
}

/// The framework's live shape for the layer's.
fn live_of(live: &photocraft_doc::vector::LiveShape) -> fw::LiveShape {
    use photocraft_doc::vector::LiveShape as Doc;
    match live {
        Doc::Rect { rect, radii } => fw::LiveShape::rect(rect[2], rect[3], *radii),
        Doc::Ellipse { rect } => fw::LiveShape::ellipse(rect[2], rect[3]),
        Doc::Polygon { rect, sides, star_ratio, corner_radius, .. } => fw::LiveShape::polygon(rect[2], rect[3], *corner_radius, *sides, *star_ratio),
        Doc::Line { weight, .. } => fw::LiveShape::line(*weight),
        Doc::ArrowLine { weight, start, end, width, length, concavity, .. } => {
            let mut arrows = Arrowheads::new();
            arrows.start = *start;
            arrows.end = *end;
            arrows.width = *width;
            arrows.length = *length;
            arrows.concavity = *concavity;
            fw::LiveShape::arrow_line(*weight, arrows)
        }
    }
}

/// One field's edit as a `shape.edit` patch, with the colour a solid pick adds to the Recently Used Colors and the
/// gradient a gradient pick adds to the Gradient tab's Recent row.
struct Edit {
    field: &'static str,
    patch: Value,
    recent: Option<[f32; 3]>,
    gradient: Option<photocraft_doc::Fill>,
}

/// The gradient a chip's `paint` stands for over `base`; `None` for a paint that is no gradient.
fn picked_gradient(paint: &Paint, base: &photocraft_doc::Fill) -> Option<photocraft_doc::Fill> {
    match paint {
        Paint::Gradient(preview) => Some(crate::shape_paint_ui::gradient_fill(preview, base)),
        _ => None,
    }
}

/// The `shape.edit` patch of `change` on the layer `sh`; `None` when the change is no edit. `gradients` are the fill's
/// and the stroke's [`ChipLibrary::gradient`](crate::shape_paint_ui::ChipLibrary::gradient), which a gradient paint
/// is laid over.
fn edit_of(change: &fw::ShapeChange, sh: &photocraft_doc::ShapeLayer, gradients: &[photocraft_doc::Fill; 2]) -> Option<Edit> {
    let plain = |field, patch| Some(Edit { field, patch, recent: None, gradient: None });
    let width_now = sh.stroke.as_ref().map_or(0.0, |s| s.width);
    match change {
        fw::ShapeChange::Fill(paint) => Some(Edit {
            field: "fill",
            patch: json!({"fill": crate::shape_paint_ui::fill_param(paint, &gradients[0])}),
            recent: crate::shape_paint_ui::solid_rgb(paint),
            gradient: picked_gradient(paint, &gradients[0]),
        }),
        fw::ShapeChange::Stroke { paint, width, .. } => {
            let color = crate::shape_paint_ui::fill_param(paint, &gradients[1]);
            let stroke = if color.is_null() {
                Value::Null
            } else {
                json!({"color": color, "width": width.unwrap_or_else(|| f64::from(enabled_stroke_width(width_now)))})
            };
            Some(Edit {
                field: "stroke",
                patch: json!({"stroke": stroke}),
                recent: crate::shape_paint_ui::solid_rgb(paint),
                gradient: picked_gradient(paint, &gradients[1]),
            })
        }
        fw::ShapeChange::StrokeWidth(w) => {
            // A step that clamps at 0 on a shape with no stroke is no change.
            if sh.stroke.is_none() && *w <= 0.0 {
                return None;
            }
            plain("stroke-w", json!({"stroke": {"width": w}}))
        }
        fw::ShapeChange::StrokeStyle { style, width, .. } => {
            let mut stroke = crate::shape_stroke_ui::StrokeOptions::from_style(style).params();
            if let Some(w) = width {
                stroke["width"] = json!(w);
            }
            plain("style", json!({"stroke": stroke}))
        }
        fw::ShapeChange::Size { width, height, .. } => {
            let rect = match sh.live.as_ref()? {
                photocraft_doc::vector::LiveShape::Rect { rect, .. }
                | photocraft_doc::vector::LiveShape::Ellipse { rect }
                | photocraft_doc::vector::LiveShape::Polygon { rect, .. } => *rect,
                _ => return None,
            };
            plain("wh", json!({"rect": [rect[0], rect[1], width.round(), height.round()]}))
        }
        fw::ShapeChange::Radii(new) => {
            let Some(photocraft_doc::vector::LiveShape::Rect { radii, .. }) = sh.live.as_ref() else { return None };
            let mut next = *radii;
            let mut patch = json!({"radii": next});
            for (corner, radius) in new.iter().enumerate() {
                if (radius - next[corner]).abs() > f64::EPSILON {
                    patch = corner_radii_patch(next, corner, *radius as f32);
                    next[corner] = f64::from((*radius as f32).max(0.0));
                }
            }
            plain("radii", patch)
        }
        fw::ShapeChange::CornerRadius(r) => plain("corner-radius", json!({"cornerRadius": r})),
        fw::ShapeChange::Sides(n) => plain("sides", json!({"sides": n})),
        fw::ShapeChange::StarRatio(f) => plain("star", json!({"starRatio": f})),
        fw::ShapeChange::Weight(w) => plain("weight", json!({"weight": w})),
        fw::ShapeChange::Arrowheads(a) => {
            plain("arrow", json!({"arrowStart": a.start, "arrowEnd": a.end, "arrowWidth": a.width, "arrowLength": a.length, "arrowConcavity": a.concavity}))
        }
        _ => None,
    }
}

/// Properties panel for a shape layer: Appearance (fill, stroke) and live shape geometry.
///
/// Each pass reads the layer fresh and applies only the events the panel reports in that pass, so a
/// second pass in the same frame applies nothing the first already did.
pub fn shape_properties(app: &mut PhotocraftApp, ui: &mut egui::Ui, id: photocraft_doc::LayerId) {
    let Some(st) = app.session.active() else { return };
    let doc = st.doc.id;
    let locks = st.doc.effective_locks(id);
    let locked = locks.all || locks.pixels;
    let ppi = f64::from(st.doc.resolution_dpi);
    let Some(sh) = st.doc.layer(id).and_then(|l| match &l.content {
        LayerContent::Shape(sh) => Some(sh.clone()),
        _ => None,
    }) else {
        return;
    };
<<<<<<< HEAD
    let (fg, bg) = (app.session.tools.foreground, app.session.tools.background);
    let appearance = crate::shape_paint_ui::appearance_of(&sh, locked);
    let shown = sh.live.as_ref().map_or_else(|| fw::LiveShape::line(1.0), live_of);
    let target = (doc.0, id.0);
    let was_linked = app.ui.arrow_default == Some(target);
    let mut linked = was_linked;
    let recent_colors: Vec<Srgba> = crate::paint_picker::recent_colors(&app.session).into_iter().map(|c| Srgba::new(c[0], c[1], c[2], 1.0)).collect();
    let groups = crate::shape_gradient_ui::ShapeGradients::groups_of(&app.session.presets.gradients, fg, bg);
    let gradients = crate::shape_gradient_ui::ShapeGradients { groups, current: &app.session.presets.gradient, recent: &mut app.ui.recent_gradients, fg, bg };
    let current = [sh.fill.clone(), sh.stroke.as_ref().map(|s| s.paint.clone())];
    let mut library = crate::shape_paint_ui::ChipLibrary::new(
        &recent_colors,
        &app.session.presets.swatches,
        &mut app.ui.paint_folders,
        gradients,
        &mut app.ui.shape_gradient_state,
        current,
    );
    let output = ShapePanel::new(label(tl!("Shape properties")), &shown, &appearance, ppi)
        .sections(true, sh.live.is_some())
        .stroke_open(Bind::bound(&mut app.ui.shape_stroke_open))
        .chips(|which, chip, chip_ui| crate::shape_paint_ui::decorate_chip(chip, which, &mut library, chip_ui))
        .fallbacks(Srgba::new(fg[0], fg[1], fg[2], fg[3]), Srgba::new(bg[0], bg[1], bg[2], bg[3]))
        .arrow_default(Bind::bound(&mut linked))
        .show(ui);
    let fw::ShapePanelOutput { events, pick_color, .. } = output;
    let paint_gradients = [library.gradient(fw::ShapePaint::Fill), library.gradient(fw::ShapePaint::Stroke)];

    let mut applied = false;
=======
    let t = Tokens::get(ui.ctx());
    let mut edit: Option<Value> = None;
    let mut concavity_alert = false;
    // A swatch picked in a Fill/Stroke popover joins the Recently Used Colors once the edit took it.
>>>>>>> upstream/main
    let mut recent: Option<[f32; 3]> = None;
    let mut remembered: Vec<photocraft_doc::Fill> = Vec::new();
    let prefix = |field: &str| format!("shape-{doc:?}-{}-{field}", id.0);
    for event in &events {
        let Some(change) = event.value() else { continue };
        let Some(edit) = edit_of(change, &sh, &paint_gradients) else { continue };
        let mut patch = edit.patch;
        patch["layer"] = json!(id.0);
        if let Some(key) = history_keys::coalesce(&prefix(edit.field), event) {
            patch["coalesce"] = json!(key);
        }
<<<<<<< HEAD
        match app.run("shape.edit", patch) {
=======
        for (is_stroke, change) in picks {
            match change {
                PaintChange::OpenPicker => {
                    let which = if is_stroke { crate::shape_fill_ui::Paint::Stroke } else { crate::shape_fill_ui::Paint::Fill };
                    crate::shape_fill_ui::open_paint(app, which, Some(id), false, fg);
                }
                PaintChange::Color(c) => recent = Some([c[0], c[1], c[2]]),
                PaintChange::None | PaintChange::Gradient(_) => {}
            }
        }
    }
    if let Some(live) = &sh.live
        && crate::props_layout::section(ui, "liveShape", tl!("Shape"))
    {
        let num = |ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>, unit: &str| -> bool {
            ui.label(egui::RichText::new(tl!(&label)).color(t.text_dim).size(12.0));
            crate::widgets::value_field(ui, v, range, unit, 64.0).accessible_name(tl!(&label)).changed()
        };
        match live {
            photocraft_doc::vector::LiveShape::Rect { rect, .. }
            | photocraft_doc::vector::LiveShape::Ellipse { rect }
            | photocraft_doc::vector::LiveShape::Polygon { rect, .. }
            | photocraft_doc::vector::LiveShape::Triangle { rect, .. } => {
                ui.horizontal(|ui| {
                    let (mut w, mut h) = (rect[2] as f32, rect[3] as f32);
                    let cw = num(ui, "W", &mut w, 1.0..=300000.0, "px");
                    let ch = num(ui, "H", &mut h, 1.0..=300000.0, "px");
                    if cw || ch {
                        edit = Some(json!({"rect": [rect[0], rect[1], w.round(), h.round()], "coalesce": key("wh")}));
                    }
                });
            }
            photocraft_doc::vector::LiveShape::Line { .. } | photocraft_doc::vector::LiveShape::ArrowLine { .. } => {}
        }
        ui.vertical(|ui| match live {
            photocraft_doc::vector::LiveShape::Rect { radii, .. } => {
                // Keep the uniform workflow available after independent corner edits too.
                ui.horizontal(|ui| {
                    let mut all = radii[0] as f32;
                    if num(ui, tl!("Corner radius"), &mut all, 0.0..=100000.0, "px") {
                        edit = Some(json!({"radii": all, "coalesce": key("radius-all")}));
                    }
                });
                // Separate rows also fit the narrow Properties dock. The labels use the
                // localized corner names, rather than untranslated abbreviations.
                for (index, label) in [(0, tl!("Top Left")), (1, tl!("Top Right")), (3, tl!("Bottom Left")), (2, tl!("Bottom Right"))] {
                    ui.horizontal(|ui| {
                        let mut radius = radii[index] as f32;
                        if num(ui, label, &mut radius, 0.0..=100000.0, "px") {
                            let mut patch = corner_radii_patch(*radii, index, radius);
                            patch["coalesce"] = json!(key(&format!("radius-{index}")));
                            edit = Some(patch);
                        }
                    });
                }
            }
            photocraft_doc::vector::LiveShape::Polygon { sides, star_ratio, corner_radius, .. } => {
                // One radius rounds every vertex of a triangle or polygon, or a star's outer points, as in Photoshop.
                ui.horizontal(|ui| {
                    let mut r = *corner_radius as f32;
                    if num(ui, tl!("Corner radius"), &mut r, 0.0..=100000.0, "px") {
                        edit = Some(json!({"cornerRadius": r, "coalesce": key("corner-radius")}));
                    }
                });
                ui.horizontal(|ui| {
                    let mut n = *sides as f32;
                    if num(ui, tl!("Sides"), &mut n, 3.0..=100.0, "") {
                        edit = Some(json!({"sides": n.round() as u32, "coalesce": key("sides")}));
                    }
                    let mut sr = (*star_ratio * 100.0) as f32;
                    if num(ui, tl!("Star ratio"), &mut sr, 1.0..=100.0, "%") {
                        edit = Some(json!({"starRatio": sr as f64 / 100.0, "coalesce": key("star")}));
                    }
                })
                .inner
            }
            photocraft_doc::vector::LiveShape::Triangle { corner_radius, .. } => {
                // One radius rounds all three corners of a triangle; it has no sides or star ratio.
                ui.horizontal(|ui| {
                    let mut r = *corner_radius as f32;
                    if num(ui, tl!("Corner radius"), &mut r, 0.0..=100000.0, "px") {
                        edit = Some(json!({"cornerRadius": r, "coalesce": key("corner-radius")}));
                    }
                })
                .inner
            }
            photocraft_doc::vector::LiveShape::ArrowLine { weight, start, end, width, length, concavity, .. } => {
                // This is a selected, saved Arrow; unlike the tool's defaults, edits update its geometry.
                ui.horizontal(|ui| {
                    let mut value = *start;
                    if ui.checkbox(&mut value, tl!("Start")).changed() {
                        edit = Some(json!({"arrowStart": value, "coalesce": key("arrow-start")}));
                    }
                    let mut value = *end;
                    if ui.checkbox(&mut value, tl!("End")).changed() {
                        edit = Some(json!({"arrowEnd": value, "coalesce": key("arrow-end")}));
                    }
                });
                ui.horizontal(|ui| {
                    let mut v = *width as f32;
                    if num(ui, "Width", &mut v, 1.0..=4096.0, "px") {
                        edit = Some(json!({"arrowWidth": v, "coalesce": key("arrow-width")}));
                    }
                });
                ui.horizontal(|ui| {
                    let mut v = *length as f32;
                    if num(ui, "Length", &mut v, 1.0..=4096.0, "px") {
                        edit = Some(json!({"arrowLength": v, "coalesce": key("arrow-length")}));
                    }
                });
                ui.horizontal(|ui| {
                    let mut v = *weight as f32;
                    if num(ui, "Weight", &mut v, 1.0..=10000.0, "px") {
                        edit = Some(json!({"weight": v, "coalesce": key("arrow-weight")}));
                    }
                });
                ui.horizontal(|ui| {
                    let label = tl!("Concavity");
                    ui.label(egui::RichText::new(label).color(t.text_dim).size(12.0));
                    let mut v = *concavity as f32;
                    let (r, out_of_range) = crate::widgets::value_field_whole_alerting(ui, label, &mut v, CONCAVITY_RANGE, "%", 64.0);
                    concavity_alert |= out_of_range;
                    if r.changed() {
                        edit = Some(json!({"arrowConcavity": v, "coalesce": key("arrow-concavity")}));
                    }
                });

                // Explicitly adopt this arrow's values as defaults for new arrows.
                let target = app.session.active().map(|st| (st.doc.id.0, id.0));
                let mut linked = ui.ctx().data(|data| {
                    let state = data.get_temp::<ArrowDefaultLink>(arrow_default_link_id()).unwrap_or_default();
                    state.target == target && state.enabled
                });

                if ui.checkbox(&mut linked, tl!("Set as default")).on_hover_text(tl!("Use these settings for new arrows")).changed() {
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(arrow_default_link_id(), ArrowDefaultLink { target, enabled: linked });
                    });
                    if linked {
                        copy_arrow_to_tool_defaults(app, id);
                    }
                }
            }
            photocraft_doc::vector::LiveShape::Line { weight, .. } => {
                ui.horizontal(|ui| {
                    let mut w = *weight as f32;
                    if num(ui, tl!("Weight"), &mut w, 1.0..=10000.0, "px") {
                        edit = Some(json!({"weight": w, "coalesce": key("weight")}));
                    }
                })
                .inner
            }
            _ => {}
        });
    }
    if let Some(mut p) = edit {
        p["layer"] = json!(id.0);

        match app.run("shape.edit", p) {
>>>>>>> upstream/main
            Ok(_) => {
                applied = true;
                recent = edit.recent.or(recent);
                if !matches!(event, EditEvent::Update { .. } | EditEvent::Cancel { .. }) {
                    remembered.extend(edit.gradient);
                }
            }
            Err(e) => {
                app.ui.status = e;
                app.ui.status_error = true;
            }
        }
    }
    if applied && let Some(rgb) = recent {
        // A colour picked in a Fill/Stroke popover joins the Recently Used Colors once the edit took it.
        crate::paint_picker::remember(app, rgb);
    }
    for g in &remembered {
        crate::shape_gradient_ui::remember_recent(&mut app.ui.recent_gradients, g);
    }
    if linked != was_linked {
        app.ui.arrow_default = linked.then_some(target);
        if linked {
            copy_arrow_to_tool_defaults(app, id);
        }
    } else if applied && linked {
        // Only a successful Properties edit updates new-arrow defaults, and only while the box is checked.
        copy_arrow_to_tool_defaults(app, id);
    }
    // The chip's Color Picker… opens the app's picker for that paint; it starts from the layer's paint, so the request's colour is not used.
    if let Some((which, _request)) = pick_color {
        let which = match which {
            fw::ShapePaint::Fill => crate::shape_fill_ui::Paint::Fill,
            _ => crate::shape_fill_ui::Paint::Stroke,
        };
        let _ = crate::shape_fill_ui::open_paint(app, which, Some(id), false, fg);
    }
    if concavity_alert {
        crate::panels::options_bar::range_alert(&mut app.ui, CONCAVITY_RANGE_ALERT);
    }
}

#[cfg(test)]
#[path = "line_arrow_concavity_field_tests.rs"]
mod line_arrow_concavity_field_tests;

#[cfg(test)]
#[path = "pen_make_mask_tests.rs"]
mod pen_make_mask_tests;
#[cfg(test)]
#[path = "rectangle_path_make_tests.rs"]
mod rectangle_path_make_tests;
