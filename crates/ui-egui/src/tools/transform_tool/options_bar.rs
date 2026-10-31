use super::begin::{enter_warp, leave_warp, warps_type};
use super::commit::{cancel, commit};
use super::pointer::{apply_state, box_state, point_on_quad};
use super::warp::edit_session_warp;
use super::{BezierMesh, Homography, PhotocraftApp, SplitTool, Stroke, TransformSession, Warp, WarpStyle, json, vec2};
use craft_ui::canvas::ReferencePoint;
use craft_ui::canvas::quad::{Anchor9, Parts as BoxParts, SKEW_LIMIT};
use craft_ui::controls::{ControlSize, IconButton, NumberField};
use craft_ui::edit::EditEvent;
use craft_ui::icons::Icon;
use craft_ui::localized::{LocaleNumbers, Unit};
use craft_ui::patterns::{CellWidth, OptionsBar};
use std::ops::RangeInclusive;

/// How much the box enlarges its pixels: the longest edge relative to the original (picks the
/// preview's texture level and filter).
pub(super) fn quad_scale(t: &TransformSession) -> f32 {
    let (w0, h0) = ((t.rect[2] - t.rect[0]).max(1e-9), (t.rect[3] - t.rect[1]).max(1e-9));
    let len = |a: [f64; 2], b: [f64; 2]| (b[0] - a[0]).hypot(b[1] - a[1]);
    let q = &t.quad;
    let s = (len(q[0], q[1]) / w0).max(len(q[3], q[2]) / w0).max(len(q[0], q[3]) / h0).max(len(q[1], q[2]) / h0);
    if s.is_finite() { s as f32 } else { 1.0 }
}

/// The session's box before the transform, as the library's box maths takes it.
fn start_rect(t: &TransformSession) -> craft_ui::canvas::kurbo::Rect {
    craft_ui::canvas::kurbo::Rect::new(t.rect[0], t.rect[1], t.rect[2], t.rect[3])
}

/// W and H (%), the angle (°) and the horizontal skew (°) the options bar shows for the box
/// ([`BoxState::parts`]: a mirrored box reads as a negative W or H).
pub(super) fn readout(t: &TransformSession) -> (f64, f64, f64, f64) {
    let p = box_state(t).parts(start_rect(t), t.skew_v);
    (p.w * 100.0, p.h * 100.0, p.angle, p.skew_h)
}

/// Sets the box's H and V skew (degrees), keeping its scale, angle and reference point: the
/// options bar's skew fields.
fn skew_about_pivot(app: &mut PhotocraftApp, skew_h: f64, skew_v: f64) {
    let Some(s) = app.ui.transform.as_mut() else { return };
    let (state, rect) = (box_state(s), start_rect(s));
    let skew_v = skew_v.clamp(-SKEW_LIMIT, SKEW_LIMIT);
    let to = BoxParts { skew_h, skew_v, ..state.parts(rect, s.skew_v) };
    let next = state.with_parts(rect, s.skew_v, to);
    if next.quad.iter().all(|p| p.is_finite()) {
        apply_state(s, &next);
        s.skew_v = skew_v;
    }
}

/// `v`, kept off zero with its sign: a divisor for a W or H scale that may be negative.
fn nonzero(v: f64) -> f64 {
    if v.abs() < 1e-9 { 1e-9f64.copysign(v) } else { v }
}

/// Width of the mode, cancel and commit cluster kept on the right of the options bar.
const ACTIONS_W: f32 = 140.0;

/// Width of the action buttons when they follow the Warp fields (one more than the box bar: Reset).
const WARP_ACTIONS_W: f32 = 176.0;

/// Reach of the reference point X/Y fields: the largest document side (`image.canvasSize`'s limit).
pub(super) const POSITION_LIMIT_PX: f32 = 300_000.0;

/// Reach of the W/H fields. The engine takes any scale; this lets a 30 px layer span the largest
/// document (#1267).
pub(super) const SCALE_LIMIT_PCT: f32 = 1_000_000.0;

/// The W or H field's limit for a box side of `side` px: as far as the result stays within the
/// largest document side, so a typed scale can't ask for an image too large to allocate.
pub(super) fn scale_limit_pct(side: f64) -> f32 {
    let side = side.abs();
    if !side.is_finite() || side < 1e-6 {
        return SCALE_LIMIT_PCT;
    }
    ((f64::from(POSITION_LIMIT_PX) / side * 100.0) as f32).clamp(100.0, SCALE_LIMIT_PCT)
}

/// Where Free Transform remembers the unit of its W and H fields, picked by right-clicking either
/// of them as in Photoshop (Percent until another is picked).
pub(super) const SIZE_UNIT_KEY: &str = "edit.freeTransform";

/// The W/H unit, with the document's resolution and the points per inch it converts with.
pub(super) fn size_unit(app: &PhotocraftApp) -> crate::layer_props_ui::LenUnit {
    use photocraft_engine::prefs::Unit;
    let unit = app.session.prefs().dialogs.get(SIZE_UNIT_KEY).and_then(|v| v.get("sizeUnit")).and_then(serde_json::Value::as_str).and_then(Unit::parse);
    crate::layer_props_ui::LenUnit { unit: unit.unwrap_or(Unit::Percent), ..crate::layer_props_ui::LenUnit::of(app) }
}

pub(super) fn set_size_unit(app: &mut PhotocraftApp, name: &str) {
    app.session.prefs.edit(|p| {
        let entry = p.dialogs.entry(SIZE_UNIT_KEY.to_string()).or_insert_with(|| json!({}));
        if !entry.is_object() {
            *entry = json!({});
        }
        entry["sizeUnit"] = json!(name);
    });
}

/// A W or H value shown in `u`: the box side `side` px wide at the original size, scaled by `pct`.
/// Percent stays the scale itself (100 % is the original size), lengths are the side's length.
pub(super) fn size_shown(u: &crate::layer_props_ui::LenUnit, pct: f64, side: f64) -> f32 {
    u.shown(pct / 100.0 * side, side)
}

/// The scale (%) a W or H value typed in `u` asks for, for a box side `side` px wide originally.
pub(super) fn size_pct(u: &crate::layer_props_ui::LenUnit, v: f32, side: f64) -> f64 {
    let side = side.abs().max(1e-9);
    u.px(v, side) / side * 100.0
}

/// Options bar while transforming: the fields (reference point X/Y, W/H %, angle, interpolation,
/// or the warp controls), then the mode switch, cancel and commit right after them. A bar too
/// narrow for both pins the buttons to the right edge and clips the fields, so Commit and Cancel
/// are always reachable.
pub fn options_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let Some(t) = app.ui.transform.clone() else { return };
    let bar = ui.available_rect_before_wrap();
    let warping = t.warp.is_some();
    let buttons_w = if warping { WARP_ACTIONS_W } else { ACTIONS_W };
    let split = (bar.right() - buttons_w).max(bar.left());
    let fields = egui::Rect::from_min_max(bar.min, egui::pos2(split, bar.bottom()));
    let mut row = ui.new_child(egui::UiBuilder::new().id_salt("transform-fields").max_rect(fields).layout(egui::Layout::left_to_right(egui::Align::Center)));
    row.set_clip_rect(fields);
    if let Some(w) = t.warp.clone() {
        warp_fields(app, &mut row, &w);
    } else if t.content_aware.is_some() {
        crate::cas_transform::fields(app, &mut row, &t);
    } else {
        transform_fields(app, &mut row, &t);
    }
    OptionsBar::divider(&mut row);
    let start = row.min_rect().right().min(split);
    let actions = egui::Rect::from_min_max(egui::pos2(start, bar.top()), egui::pos2(start + buttons_w, bar.bottom()));
    let mut btns = ui.new_child(egui::UiBuilder::new().id_salt("transform-actions").max_rect(actions).layout(egui::Layout::right_to_left(egui::Align::Center)));
    transform_actions(app, &mut btns, warping, t.content_aware.is_none());
}

fn transform_fields(app: &mut PhotocraftApp, ui: &mut egui::Ui, t: &TransformSession) {
    let (_, _, angle, skew_h) = readout(t);
    position_size_fields(app, ui, t);
    OptionsBar::divider(ui);
    let degrees = Unit::new("{n}°");
    let turn = Cell {
        id: "transform-angle",
        caption: None,
        name: tl!("Rotate"),
        range: -180.0..=180.0,
        unit: degrees.clone(),
        width: CellWidth::Sm,
        decimals: 0..=2,
        icon: Some(Icon::RotateCw),
    };
    if let Some(a) = number_cell(ui, &turn, angle as f32) {
        rotate_about_pivot(app, (f64::from(a) - angle).to_radians());
    }
    // Photoshop's H and V skew fields: the box's sides lean by H, its top and bottom by V.
    let (mut h, mut v) = (skew_h as f32, t.skew_v as f32);
    let lim = -SKEW_LIMIT as f32..=SKEW_LIMIT as f32;
    let skew = |id, caption, name| Cell {
        id,
        caption: Some(caption),
        name,
        range: lim.clone(),
        unit: degrees.clone(),
        width: CellWidth::Sm,
        decimals: 0..=2,
        icon: None,
    };
    let mut skewed = false;
    if let Some(x) = number_cell(ui, &skew("transform-skew-h", "H:", tl!("Set horizontal skew")), h) {
        (h, skewed) = (x, true);
    }
    if let Some(x) = number_cell(ui, &skew("transform-skew-v", "V:", tl!("Set vertical skew")), v) {
        (v, skewed) = (x, true);
    }
    if skewed {
        skew_about_pivot(app, f64::from(h), f64::from(v));
    }
    OptionsBar::divider(ui);
    let mut interp = t.interpolation.clone();
    let opts = [("bicubic".to_string(), "Bicubic"), ("bilinear".to_string(), "Bilinear"), ("nearest".to_string(), "Nearest Neighbor")];
    if crate::panels::select_cell(ui, Some("Interpolation:"), "transform-interp", "Interpolation", &mut interp, &opts, false)
        && let Some(s) = app.ui.transform.as_mut()
    {
        s.interpolation = interp;
    }
}

/// One number cell of the strip: its caption (or a scrub icon), accessible name and limits.
struct Cell<'a> {
    id: &'a str,
    caption: Option<&'a str>,
    name: &'a str,
    range: RangeInclusive<f32>,
    unit: Unit,
    width: CellWidth,
    decimals: RangeInclusive<usize>,
    /// Scrubs the field instead of a caption.
    icon: Option<Icon>,
}

/// A caption and its number field. Returns the value this frame's edit gave: a typed, scrubbed or
/// stepped value, or the value a cancelled gesture began with.
fn number_cell(ui: &mut egui::Ui, cell: &Cell<'_>, value: f32) -> Option<f32> {
    let icon = cell.icon;
    let numbers = LocaleNumbers::of(ui);
    let start = egui::Id::NULL.with(("transform-cell-start", cell.id));
    let show = |ui: &mut egui::Ui, caption: Option<&egui::Response>| {
        let mut field = NumberField::new(cell.id, cell.name, f64::from(value), &numbers)
            .range(cell.range.clone())
            .decimals(cell.decimals.clone())
            .unit(cell.unit.clone())
            .or_parse(|text| crate::numeric_expression::parse(&text.split_whitespace().collect::<String>()))
            .size(ControlSize::Sm);
        if let Some(c) = caption {
            field = field.scrub_by(c);
        }
        if let Some(icon) = icon {
            field = field.scrub_icon(icon);
        }
        field.show(ui)
    };
    let edit = match cell.caption {
        Some(caption) => OptionsBar::cell(ui, &format!("{}", tl!(caption)), cell.width, |ui, c| show(ui, Some(c))),
        None => OptionsBar::cell(ui, "", cell.width, |ui, _| show(ui, None)),
    };
    match edit.event {
        Some(EditEvent::Begin { .. }) => {
            ui.data_mut(|d| d.insert_temp(start, value));
            None
        }
        Some(EditEvent::Update { value, .. } | EditEvent::Set { value, .. }) => Some(value as f32),
        Some(EditEvent::Commit { value, changed: true, .. }) => Some(value as f32),
        Some(EditEvent::Cancel { .. }) => ui.data(|d| d.get_temp::<f32>(start)),
        _ => None,
    }
}

pub(super) fn matching_anchor(quad: [[f64; 2]; 4], pivot: [f64; 2]) -> Option<usize> {
    (0..9).find(|&i| {
        let p = point_on_quad(quad, i);
        (p[0] - pivot[0]).abs() <= 0.05 && (p[1] - pivot[1]).abs() <= 0.05
    })
}

/// The reference point's X/Y and the W/H scale fields (with the aspect link) of the options bar.
pub(crate) fn position_size_fields(app: &mut PhotocraftApp, ui: &mut egui::Ui, t: &TransformSession) {
    let (sx, sy, _, _) = readout(t);
    let mut pivot = t.pivot;
    let picked = matching_anchor(t.quad, t.pivot).map(|i| Anchor9::ALL[i]);
    let pick = ReferencePoint::new("transform-reference-point", picked).label(tl!("Reference point location")).show(ui);
    if let Some(EditEvent::Set { value: Some(anchor), .. }) = pick.event {
        pivot = point_on_quad(t.quad, anchor as usize);
        if let Some(s) = app.ui.transform.as_mut() {
            s.pivot = pivot;
        }
    }
    let px_unit = Unit::new("{n} px");
    let position = |id, caption, name| Cell {
        id,
        caption: Some(caption),
        name,
        range: -POSITION_LIMIT_PX..=POSITION_LIMIT_PX,
        unit: px_unit.clone(),
        width: CellWidth::Md,
        decimals: 0..=2,
        icon: None,
    };
    let (mut px, mut py) = (pivot[0] as f32, pivot[1] as f32);
    let (mut moved, edited_x) = (false, number_cell(ui, &position("transform-x", "X:", "X"), px));
    if let Some(x) = edited_x {
        (px, moved) = (x, true);
    }
    if let Some(y) = number_cell(ui, &position("transform-y", "Y:", "Y"), py) {
        (py, moved) = (y, true);
    }
    if moved && let Some(s) = app.ui.transform.as_mut() {
        let (dx, dy) = (px as f64 - s.pivot[0], py as f64 - s.pivot[1]);
        s.quad = s.quad.map(|q| [q[0] + dx, q[1] + dy]);
        s.pivot = [px as f64, py as f64];
    }
    OptionsBar::divider(ui);
    // W and H in the unit picked from their right-click menu: the scale in percent, or the box's
    // length (Photoshop's units, at the document's resolution).
    let u = size_unit(app);
    let (side_w, side_h) = (t.rect[2] - t.rect[0], t.rect[3] - t.rect[1]);
    let (wlim, hlim) = (scale_limit_pct(side_w), scale_limit_pct(side_h));
    let (wlim_u, hlim_u) = (size_shown(&u, f64::from(wlim), side_w), size_shown(&u, f64::from(hlim), side_h));
    let (mut w, mut h) = (size_shown(&u, sx, side_w), size_shown(&u, sy, side_h));
    let (suffix, places) = (u.unit.suffix(), u.unit.decimals() as u32);
    // Lengths need more room than a percentage ("2116.67 mm").
    let size_w = if u.unit == photocraft_engine::prefs::Unit::Percent { 66.0 } else { 84.0 };
    // Whatever their unit, W and H take any other typed in (`500 px` or `2 in` in percent, `50%`
    // in a length), converted at the document's resolution against the box's original side (#6284).
    let units = crate::widgets::FieldUnits { unit: u.unit, dpi: u.dpi, ppi: u.ppi };
    crate::panels::opt_label(ui, "W");
    let rw = crate::widgets::value_field_extent(ui, &mut w, -wlim_u..=wlim_u, suffix, size_w, places, units, side_w).accessible_name(tl!("Width"));
    let link = !app.ui.transform_unlinked;
    let mut linked = link;
    ui.add(IconButton::new(if link { Icon::Link } else { Icon::Unlink }, tl!("Maintain aspect ratio")).size(ControlSize::Sm).toggle(&mut linked));
    app.ui.transform_unlinked = !linked;
    crate::panels::opt_label(ui, "H");
    let rh = crate::widgets::value_field_extent(ui, &mut h, -hlim_u..=hlim_u, suffix, size_w, places, units, side_h).accessible_name(tl!("Height"));
    for r in [&rw, &rh] {
        r.context_menu(|ui| {
            if let Some(name) = crate::rulers::unit_menu(ui, u.unit) {
                set_size_unit(app, name);
            }
        });
    }
    let (w, h) = (size_pct(&u, w, side_w) as f32, size_pct(&u, h, side_h) as f32);
    if rw.changed() || rh.changed() {
        let (kx, ky) = if link {
            let k = if rw.changed() { w as f64 / nonzero(sx) } else { h as f64 / nonzero(sy) };
            // Linked, the other side follows: keep it within its own limit too.
            let k = k.min(f64::from(wlim) / sx.abs().max(1e-9)).min(f64::from(hlim) / sy.abs().max(1e-9));
            (k, k)
        } else {
            (w as f64 / nonzero(sx), h as f64 / nonzero(sy))
        };
        scale_about_pivot(app, kx, ky);
    }
}

/// Options bar fields in Warp mode: style, bend, the split icons and the grid menu.
fn warp_fields(app: &mut PhotocraftApp, ui: &mut egui::Ui, w: &Warp) {
    // Type takes only the styles (#3004): Split and Grid are greyed, and there is no Custom.
    let on_type = warps_type(app);
    // Split, Grid, then the warp style with its bend fields.
    OptionsBar::group(ui, !on_type, |ui| warp_mesh_fields(app, ui, w));
    OptionsBar::divider(ui);
    let mut style = w.style;
    let opts: Vec<(WarpStyle, &str)> = WarpStyle::all().filter(|s| !(on_type && *s == WarpStyle::Custom)).map(|s| (s, s.label())).collect();
    if crate::panels::select_cell(ui, Some("Warp:"), "warp-style", "Warp", &mut style, &opts, false)
        && let Some(t) = app.ui.transform.as_mut()
    {
        let b = t.rect;
        t.warp = Some(match style {
            WarpStyle::Custom => Warp::custom(w.to_mesh(1, 1), b),
            WarpStyle::None if on_type => Warp::none(b),
            WarpStyle::None => Warp::custom(BezierMesh::identity(b, 1, 1), b),
            s => Warp { style: s, bend: if w.style.is_preset() { w.bend } else { 50.0 }, ..Warp::none(b) },
        });
    }
    // Orientation and the bend fields are always there, and only act on a preset style.
    let preset = w.style.is_preset();
    OptionsBar::group(ui, preset, |ui| {
        let orient = ui.add(IconButton::new(Icon::SwapHorizontal, tl!("Vertical")).size(ControlSize::Md).selected(w.vertical));
        orient.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, preset, "Change warp orientation"));
        if orient.clicked()
            && let Some(Some(cur)) = app.ui.transform.as_mut().map(|t| t.warp.as_mut())
        {
            cur.vertical = !cur.vertical;
        }
        let fields: [(&str, &str, f64); 3] = [("warp-bend", "Bend:", w.bend), ("warp-h", "H:", w.h_distort), ("warp-v", "V:", w.v_distort)];
        for (k, (id, label, v)) in fields.iter().enumerate() {
            let cell = Cell {
                id,
                caption: Some(label),
                name: tl!(label),
                range: -100.0..=100.0,
                unit: Unit::new("{n}%"),
                width: CellWidth::Md,
                decimals: 0..=1,
                icon: None,
            };
            if let Some(f) = number_cell(ui, &cell, *v as f32)
                && let Some(Some(cur)) = app.ui.transform.as_mut().map(|t| t.warp.as_mut())
            {
                match k {
                    0 => cur.bend = f64::from(f),
                    1 => cur.h_distort = f64::from(f),
                    _ => cur.v_distort = f64::from(f),
                }
            }
        }
    });
}

/// The Split icons and the Grid menu of the Warp options bar.
fn warp_mesh_fields(app: &mut PhotocraftApp, ui: &mut egui::Ui, w: &Warp) {
    crate::panels::opt_label(ui, "Split");
    let armed = app.transform_preview.as_ref().and_then(|p| p.split_tool);
    for (tool, tip) in [
        (SplitTool::Cross, tl!("Split Warp Crosswise")),
        (SplitTool::Vertical, tl!("Split Warp Vertically")),
        (SplitTool::Horizontal, tl!("Split Warp Horizontally")),
    ] {
        let on = armed == Some(tool);
        if split_icon(ui, tool.icon(), tip, on).clicked()
            && let Some(pv) = app.transform_preview.as_mut()
        {
            pv.split_tool = if on { None } else { Some(tool) };
            pv.split_placing = false;
            pv.split_pointer = None;
            pv.split_press = None;
        }
    }
    OptionsBar::divider(ui);
    let mesh = w.mesh.clone().unwrap_or_else(|| BezierMesh::identity(w.bounds, 1, 1));
    let mut grid = warp_grid_id(&mesh).to_string();
    let opts = [
        ("custom".to_string(), "Custom"),
        ("default".to_string(), "Default"),
        ("3".to_string(), "3 x 3"),
        ("4".to_string(), "4 x 4"),
        ("5".to_string(), "5 x 5"),
    ];
    if crate::panels::select_cell(ui, Some("Grid:"), "warp-grid", "Grid", &mut grid, &opts, false)
        && let Some(n) = match grid.as_str() {
            "default" => Some(1),
            "3" => Some(3),
            "4" => Some(4),
            "5" => Some(5),
            _ => None,
        }
    {
        let _ = edit_session_warp(app, "edit.transform.warpGrid", &json!({ "size": n }));
    }
}

/// Back to the untouched box: the default one-patch grid over the box, no bend.
pub fn reset_warp(app: &mut PhotocraftApp) {
    let Some(t) = app.ui.transform.as_mut() else { return };
    if t.warp.is_none() {
        return;
    }
    let b = t.rect;
    t.warp = Some(Warp::custom(BezierMesh::identity(b, 1, 1), b));
    if warps_type(app)
        && let Some(t) = app.ui.transform.as_mut()
    {
        t.warp = Some(Warp::none(b));
    }
    if let Some(pv) = app.transform_preview.as_mut() {
        pv.warp_drag = None;
        pv.split_tool = None;
        pv.split_pointer = None;
        pv.split_placing = false;
        pv.split_press = None;
    }
}

/// Grid menu readout: Default is 1×1, the numbered entries are uniform n×n patches, and anything
/// split by hand reads as Custom.
pub(super) fn warp_grid_id(mesh: &BezierMesh) -> &'static str {
    let cols = mesh.us.len().saturating_sub(1);
    let rows = mesh.vs.len().saturating_sub(1);
    let even = |k: &[f64]| {
        let n = k.len().saturating_sub(1);
        n > 0 && k.iter().enumerate().all(|(i, v)| (v - i as f64 / n as f64).abs() < 1e-4)
    };
    if cols == rows && even(&mesh.us) && even(&mesh.vs) {
        match cols {
            1 => "default",
            3 => "3",
            4 => "4",
            5 => "5",
            _ => "custom",
        }
    } else {
        "custom"
    }
}

/// One of the three Split icons. `on` is the armed tool: the line then follows the pointer.
fn split_icon(ui: &mut egui::Ui, kind: &str, tip: &str, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::click());
    let ink = crate::icons::button_chrome(ui, rect, on, resp.hovered());
    let c = rect.center();
    let stroke = Stroke::new(1.15, ink);
    let box_r = egui::Rect::from_center_size(c, vec2(13.0, 13.0));
    ui.painter().rect_stroke(box_r, craft_ui::theme::scale::Radius::R2.px(), stroke, egui::StrokeKind::Inside);
    let h = 5.2;
    match kind {
        "vertical" => {
            ui.painter().line_segment([c - vec2(0.0, h), c + vec2(0.0, h)], stroke);
        }
        "horizontal" => {
            ui.painter().line_segment([c - vec2(h, 0.0), c + vec2(h, 0.0)], stroke);
        }
        _ => {
            ui.painter().line_segment([c - vec2(h, 0.0), c + vec2(h, 0.0)], stroke);
            ui.painter().line_segment([c - vec2(0.0, h), c + vec2(0.0, h)], stroke);
        }
    }
    if kind == "cross" {
        ui.painter().circle_filled(c, 1.5, ink);
    } else {
        ui.painter().circle_stroke(c, 2.1, stroke);
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
    resp.on_hover_text(tip)
}

/// Warp switch, cancel and commit, kept on the right of the options bar.
/// `mode_switch`: the Free Transform / Warp switch (not for Content-Aware Scale, which only scales).
fn transform_actions(app: &mut PhotocraftApp, ui: &mut egui::Ui, warping: bool, mode_switch: bool) {
    ui.add_space(craft_ui::theme::scale::Space::S8.px());
    // Cancel and Commit keep the library order at the end of the bar; Reset, the divider and the mode switch follow them.
    let style = OptionsBar::style(&<craft_theme::Tokens as craft_ui::theme::TokensOf>::of(ui), false);
    let pair = 2.0 * style.button + ui.spacing().item_spacing.x;
    let done = ui
        .allocate_ui_with_layout(egui::vec2(pair, ui.available_height()), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            OptionsBar::commit_cancel(ui, tl!("Commit transform"), tl!("Cancel transform"))
        })
        .inner;
    if done.commit {
        commit(app);
    }
    if done.cancel {
        cancel(app);
    }
    if warping {
        let reset = ui.add(IconButton::new(Icon::Undo, tl!("Reset")).size(ControlSize::Md));
        reset.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Reset warp"));
        if reset.clicked() {
            reset_warp(app);
        }
    }
    if !mode_switch {
        return;
    }
    OptionsBar::divider(ui);
    let mode = ui.add(IconButton::new(Icon::Grid, tl!("Switch between free transform and warp modes")).size(ControlSize::Md).selected(warping));
    mode.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Switch between free transform and warp modes"));
    if mode.clicked() {
        if warping {
            leave_warp(app);
        } else {
            enter_warp(app, &ui.ctx().clone());
        }
    }
}

/// Scale the box along its own axes about the reference point. A negative factor mirrors that
/// side; both negative turn the box by a half turn.
pub(super) fn scale_about_pivot(app: &mut PhotocraftApp, kx: f64, ky: f64) {
    let Some(s) = app.ui.transform.as_mut() else { return };
    let Some(h) = Homography::rect_to_quad([0.0, 0.0, 1.0, 1.0], s.quad) else { return };
    let Some(inv) = h.inverse() else { return };
    let (pu, pv) = inv.apply(s.pivot[0], s.pivot[1]);
    let m = |u: f64, v: f64| {
        let (x, y) = h.apply(pu + (u - pu) * kx, pv + (v - pv) * ky);
        [x, y]
    };
    s.quad = [m(0.0, 0.0), m(1.0, 0.0), m(1.0, 1.0), m(0.0, 1.0)];
}

pub(super) fn rotate_about_pivot(app: &mut PhotocraftApp, da: f64) {
    let Some(s) = app.ui.transform.as_mut() else { return };
    let (sn, cs) = da.sin_cos();
    let c = s.pivot;
    s.quad = s.quad.map(|q| {
        let (x, y) = (q[0] - c[0], q[1] - c[1]);
        [c[0] + x * cs - y * sn, c[1] + x * sn + y * cs]
    });
}
