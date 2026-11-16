//! Create Rectangle / Ellipse / Triangle / Polygon: a single click (no drag) with a Shape tool opens
//! the framework's `CreateShapeDialog` on the dialog host. Its OK draws the shape with the options
//! bar's fill and stroke, as a drag does. The dialog owns the fields, their validation and the
//! remembered values.

use craft::behavior::kurbo::Point;
use craft::draw::ui::shape_drag::{CreateShapeDialog, CreateShapeOptions, ShapeKind};
use craft::ui::dialog::{DialogId, DialogResult};
use craft::ui::patterns::form_dialogs::GuideUnits;

use crate::PhotocraftApp;

<<<<<<< HEAD
/// Opens the creation dialog of `kind` for a click at `at` (document px). `None` for a kind without
/// one (Line, Custom Shape) or a click that is not a point.
pub(crate) fn open(app: &mut PhotocraftApp, kind: ShapeKind, at: Point) -> Option<DialogId> {
    if !CreateShapeDialog::supports(kind) || !(at.x.is_finite() && at.y.is_finite()) {
        return None;
    }
=======
const MARK: &str = "__createShape";
/// Largest side the dialog accepts, in pixels (Photoshop's document limit).
const MAX_PX: f64 = 300_000.0;
/// The reference's alert for a Width or Height typed past `1..=MAX_PX` px (#13358): Perspective
/// Crop's text, with the limits in two decimals and the system's decimal separator ("1,00" there).
fn size_range_alert() -> String {
    let number = |v: f64| {
        let text = format!("{v:.2}");
        if crate::i18n::decimal_separator() == Some(',') { text.replace('.', ",") } else { text }
    };
    let (lo, hi) = (number(1.0), number(MAX_PX));
    crate::i18n::fmt(tl!(crate::tools::crop_size::LENGTH_RANGE_ALERT), &[("min", &lo), ("max", &hi), ("unit", tl!("Pixels"))])
}
/// Set in the dialog's fields by [`body`] when a typed size was out of range; [`open_owed_alert`] takes it.
const ALERT_OWED: &str = "__sizeRangeAlert";
/// Remembered values (document pixels; sides and radii come from the options bar).
const KEYS: [&str; 4] = ["width", "height", "fromCenter", "starRatio"];

pub fn owns(fields: &Map<String, Value>) -> bool {
    fields.contains_key(MARK)
}

/// The tool's name in the dialog fields and the title, for the tools that have a dialog.
fn kind_of(tool: Tool) -> Option<(&'static str, &'static str)> {
    Some(match tool {
        Tool::Rectangle => ("rectangle", "Create Rectangle"),
        Tool::EllipseShape => ("ellipse", "Create Ellipse"),
        Tool::Triangle => ("triangle", "Create Triangle"),
        Tool::Polygon => ("polygon", "Create Polygon"),
        _ => return None,
    })
}

fn pref_key(kind: &str) -> String {
    format!("shape.create.{kind}")
}

fn valid(key: &str, v: &Value) -> bool {
    match key {
        "width" | "height" => v.as_f64().is_some_and(|x| x.is_finite() && (1.0..=MAX_PX).contains(&x)),
        "starRatio" => v.as_f64().is_some_and(|x| x.is_finite() && (1.0..=100.0).contains(&x)),
        _ => v.is_boolean(),
    }
}

/// True for a press and release that never left `start` by more than a few screen pixels.
pub fn is_click(start: [f64; 2], points: &[[f64; 3]], zoom: f32) -> bool {
    within_click_slop(start, points, zoom, 3.0)
}

pub(crate) fn is_tool_click(tool: Tool, options: &crate::state::ToolOptions, start: [f64; 2], points: &[[f64; 3]], zoom: f32) -> bool {
    if tool == Tool::Rectangle && options.shape_mode == "shape" { within_click_slop(start, points, zoom, 1.0) } else { is_click(start, points, zoom) }
}

fn within_click_slop(start: [f64; 2], points: &[[f64; 3]], zoom: f32, pixels: f64) -> bool {
    let slop = pixels / f64::from(zoom.max(0.01));
    points.iter().all(|p| (p[0] - start[0]).hypot(p[1] - start[1]) <= slop)
}

/// A Shape-mode Rectangle click inside the active live rectangle leaves the shape alone (#13340).
pub(crate) fn click_inside_active_rectangle(app: &PhotocraftApp, tool: Tool, at: [f64; 2]) -> bool {
    use photocraft_doc::{LayerContent, LiveShape};

    if tool != Tool::Rectangle || app.ui.tool_options.shape_mode != "shape" {
        return false;
    }
    let Some(layer) = app.session.active().and_then(|st| st.active_layer.and_then(|id| st.doc.layer(id))) else { return false };
    let LayerContent::Shape(shape) = &layer.content else { return false };
    let Some(LiveShape::Rect { rect: [x, y, w, h], radii }) = shape.live else { return false };
    let (left, top) = (x.min(x + w), y.min(y + h));
    let (right, bottom) = (x.max(x + w), y.max(y + h));
    let [px, py] = at;
    if !((left..=right).contains(&px) && (top..=bottom).contains(&py)) {
        return false;
    }
    let (l, r, t, b) = (px - left, right - px, py - top, bottom - py);
    [(l, t), (r, t), (r, b), (l, b)]
        .into_iter()
        .zip(photocraft_vector::shapes::clamp_radii(w.abs(), h.abs(), radii))
        .all(|((dx, dy), radius)| dx >= radius || dy >= radius || (radius - dx).hypot(radius - dy) <= radius)
}

/// Open the creation dialog of `tool` for a click at `at` (document px). `None` for a tool without
/// one (Line, Custom Shape).
pub fn open(app: &mut PhotocraftApp, tool: Tool, at: [f64; 2]) -> Option<u64> {
    let (kind, label) = kind_of(tool)?;
    if !(at[0].is_finite() && at[1].is_finite()) {
        return None;
    }
    let o = &app.ui.tool_options;
    let r = f64::from(o.corner_radius.max(0.0));
    let sides = o.polygon_sides.clamp(3, 100);
    let mut f = Map::new();
    f.insert(MARK.into(), json!(kind));
    f.insert("__command".into(), json!("shape.create"));
    f.insert("__label".into(), json!(label));
    f.insert("__at".into(), json!([at[0].round(), at[1].round()]));
    f.insert("width".into(), json!(100.0));
    f.insert("height".into(), json!(100.0));
    f.insert("fromCenter".into(), json!(false));
    match kind {
        "rectangle" => {
            f.insert("radii".into(), json!([r, r, r, r]));
            f.insert("linkRadii".into(), json!(true));
        }
        "polygon" => {
            f.insert("sides".into(), json!(sides));
            f.insert("starRatio".into(), json!(100.0));
        }
        _ => {}
    }
    if let Some(Value::Object(saved)) = app.session.prefs().dialogs.get(&pref_key(kind)) {
        for key in KEYS {
            if f.contains_key(key)
                && let Some(v) = saved.get(key).filter(|v| valid(key, v))
            {
                f.insert(key.into(), v.clone());
            }
        }
    }
    Some(app.ui.open_dialog(DialogKind::Command, f))
}

fn num(f: &Map<String, Value>, key: &str) -> Option<f64> {
    f.get(key).and_then(Value::as_f64).filter(|v| v.is_finite())
}

/// `shape.create` geometry for the dialog's values (no fill or stroke).
pub fn geometry(f: &Map<String, Value>) -> Result<Value, String> {
    let kind = f.get(MARK).and_then(Value::as_str).ok_or("not a Create Shape dialog")?;
    let at = f.get("__at").and_then(Value::as_array).ok_or("the dialog has no position")?;
    let (Some(x), Some(y)) = (at.first().and_then(Value::as_f64), at.get(1).and_then(Value::as_f64)) else {
        return Err("the dialog has no position".into());
    };
    let side = |key: &str, label: &str| -> Result<f64, String> {
        num(f, key).filter(|v| (1.0..=MAX_PX).contains(v)).ok_or_else(|| format!("{label} must be between 1 and {MAX_PX} pixels"))
    };
    let (w, h) = (side("width", "Width")?, side("height", "Height")?);
    let center = f.get("fromCenter").and_then(Value::as_bool).unwrap_or(false);
    if !(x.abs() <= MAX_PX && y.abs() <= MAX_PX) {
        return Err("the shape's position is out of range".into());
    }
    let (x0, y0) = if center { (x - w / 2.0, y - h / 2.0) } else { (x, y) };
    let rect = json!([x0, y0, w, h]);
    Ok(match kind {
        "rectangle" => {
            let mut radii = [0.0; 4];
            if let Some(a) = f.get("radii").and_then(Value::as_array) {
                for (r, v) in radii.iter_mut().zip(a) {
                    *r = v.as_f64().filter(|v| v.is_finite()).unwrap_or(0.0).clamp(0.0, w.min(h) / 2.0);
                }
            }
            if radii.iter().any(|r| *r > 0.0) { json!({"kind": "roundedRect", "rect": rect, "radii": radii}) } else { json!({"kind": "rect", "rect": rect}) }
        }
        "ellipse" => json!({"kind": "ellipse", "rect": rect}),
        "triangle" => json!({"kind": "triangle", "rect": rect, "historyName": "Triangle Tool"}),
        "polygon" => {
            let sides = num(f, "sides").map_or(5.0, |s| s.round().clamp(3.0, 100.0));
            let ratio = num(f, "starRatio").map_or(100.0, |r| r.clamp(1.0, 100.0));
            json!({"kind": "polygon", "rect": rect, "sides": sides, "starRatio": ratio / 100.0})
        }
        other => return Err(format!("unknown shape `{other}`")),
    })
}

/// OK: create the shape and remember the values for the next click.
pub fn confirm(app: &mut PhotocraftApp, f: &Map<String, Value>) -> Result<Value, String> {
    let result = geometry(f).and_then(|g| crate::vector_ui::create_shape(app, g));
    match &result {
        Ok(_) => {
            if let Some(kind) = f.get(MARK).and_then(Value::as_str) {
                let saved: Map<String, Value> = KEYS.iter().filter_map(|k| f.get(*k).map(|v| (k.to_string(), v.clone()))).collect();
                app.session.prefs.edit(|p| p.dialogs.insert(pref_key(kind), Value::Object(saved)));
            }
        }
        Err(e) => {
            app.ui.status = e.clone();
            app.ui.status_error = true;
        }
    }
    result
}

/// Returns whether a size warning is owed, blocking creation until it has been acknowledged.
pub fn open_owed_alert(app: &mut PhotocraftApp, ctx: &egui::Context, f: &mut Map<String, Value>) -> bool {
    if !f.contains_key(ALERT_OWED) {
        return false;
    }
    // A mouse press leaves the field before its button answers on release. Keep the warning
    // pending until then, so Cancel can discard it instead of opening an orphaned alert.
    if !ctx.input(|i| i.pointer.any_down()) {
        f.remove(ALERT_OWED);
        crate::panels::options_bar::range_alert(&mut app.ui, &size_range_alert());
    }
    true
}

/// Label column bounds for [`crate::widgets::label_column_width`]: the English labels' width, and a cap.
const LABEL_MIN_W: f32 = 96.0;
const LABEL_MAX_W: f32 = 200.0;
/// Every label in the column, measured in the current language.
const LABELS: [&str; 5] = ["Width:", "Height:", "Radius:", "Sides:", "Star Ratio:"];

/// The dialog's rows (Width and Height, then Radius, or Sides and Star Ratio for a polygon),
/// their labels in one column measured over [`LABELS`] so a longer translation never overruns.
pub fn body(app: &PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    ui.spacing_mut().item_spacing.y = 6.0;
    let kind = f.get(MARK).and_then(Value::as_str).unwrap_or("").to_string();
    let label_w = crate::widgets::label_column_width(ui, &LABELS.map(crate::i18n::t), LABEL_MIN_W, LABEL_MAX_W);
    let label = |ui: &mut egui::Ui, text: &str| {
        crate::widgets::column_label(ui, text, label_w);
    };
    // Width and height each in its own unit (percent of the document's side): the ruler unit, or
    // the one picked from that field's right-click menu, as in Photoshop (#7786).
>>>>>>> upstream/main
    let ur = &app.session.prefs().units_and_rulers;
    let ruler = crate::sizing::pref_unit(ur.rulers);
    let points = f64::from(ur.point_size.per_inch());
    let (dpi, size) = app
        .session
        .active()
        .map_or((72.0, [1.0, 1.0]), |d| (f64::from(d.doc.resolution_dpi.max(1.0)), [f64::from(d.doc.size.width), f64::from(d.doc.size.height)]));
<<<<<<< HEAD
    let o = &app.ui.tool_options;
    let radius = f64::from(o.corner_radius.max(0.0));
    let sides = o.polygon_sides.clamp(3, 100);
    let units = GuideUnits::new(ruler, size[0], size[1], dpi).points_per_inch(points);
    // The options bar has no star ratio: a polygon opens as a plain polygon.
    let options = CreateShapeOptions::initial(kind, at).units(units).tool(radius, sides, 1.0);
    Some(app.framework.dialogs.open(CreateShapeDialog::new(options)))
}

/// Draws the shape of a Create Shape dialog that finished OK this pass.
pub(crate) fn take(app: &mut PhotocraftApp, _ctx: &egui::Context) {
    let Some(DialogResult::Ok(shape)) = app.framework.dialogs.take::<CreateShapeDialog>() else { return };

    if let Err(e) = crate::vector_ui::create_geometry(app, &shape.geometry, shape.kind, None) {
        app.ui.status = e;
        app.ui.status_error = true;
=======
    for (key, unit_key, text, extent) in [("width", "widthUnit", tl!("Width:"), size[0]), ("height", "heightUnit", tl!("Height:"), size[1])] {
        let unit = f.get(unit_key).and_then(Value::as_str).and_then(Unit::parse).unwrap_or(rulers);
        ui.horizontal(|ui| {
            label(ui, text);
            let px = num(f, key).unwrap_or(100.0);
            let to_unit = |px: f64| unit.from_px(px, dpi, extent, ppi) as f32;
            let mut v = to_unit(px);
            let places = unit.decimals() as u32;
            let range = to_unit(1.0)..=to_unit(MAX_PX);
            // Create Triangle's pixel fields say when a typed size was out of range (#13358); the other
            // units and the other shapes' dialogs keep clamping silently.
            let (resp, out_of_range) = if unit == Unit::Pixels && kind == "triangle" {
                crate::widgets::value_field_alerting_prec(ui, text, &mut v, range, unit.suffix(), 110.0, places)
            } else {
                (crate::widgets::value_field_prec(ui, &mut v, range, unit.suffix(), 110.0, places), false)
            };
            let resp = resp.accessible_name(text);
            if out_of_range {
                f.insert(ALERT_OWED.into(), Value::Bool(true));
            }
            if resp.changed() {
                let px = unit.to_px(f64::from(v), dpi, extent, ppi).clamp(1.0, MAX_PX);
                f.insert(key.into(), json!((px * 1000.0).round() / 1000.0));
            }
            // Right-click: the length units for this field (the sizes stay in pixels).
            resp.context_menu(|ui| {
                if let Some(name) = crate::rulers::unit_menu_of(ui, unit, &crate::rulers::LENGTH_UNITS) {
                    f.insert(unit_key.into(), json!(name));
                }
            });
        });
>>>>>>> upstream/main
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod create_triangle_range_alert_tests;
