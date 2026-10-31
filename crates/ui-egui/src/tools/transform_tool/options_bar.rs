use super::begin::{enter_warp, leave_warp, warps_type};
use super::commit::{cancel, commit};
use super::pointer::{apply_state, box_state};
use super::warp::edit_session_warp;
use super::{BezierMesh, PhotocraftApp, SplitTool, Stroke, TransformSession, Warp, WarpStyle, json, vec2};
use craft::draw::ui::{BarGroups, FieldLabel, SizeUnit, TransformField, TransformFieldLabels, TransformFields, TransformRow};
use craft::ui::canvas::quad::SKEW_LIMIT;
use craft::ui::canvas::{RulerUnits, Ticks};
use craft::ui::controls::{ControlSize, IconButton, NumberField};
use craft::ui::edit::EditEvent;
use craft::ui::icons::Icon;
use craft::ui::localized::{LocaleNumbers, Unit};
use craft::ui::patterns::{CellWidth, OptionsBar};

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
fn start_rect(t: &TransformSession) -> craft::ui::canvas::kurbo::Rect {
    craft::ui::canvas::kurbo::Rect::new(t.rect[0], t.rect[1], t.rect[2], t.rect[3])
}

/// W and H (%), the angle (°) and the horizontal skew (°) the options bar shows for the box
/// ([`BoxState::parts`]: a mirrored box reads as a negative W or H).
pub(super) fn readout(t: &TransformSession) -> (f64, f64, f64, f64) {
    let p = box_state(t).parts(start_rect(t), t.skew_v);
    (p.w * 100.0, p.h * 100.0, p.angle, p.skew_h)
}

/// Width of the mode, cancel and commit cluster kept on the right of the options bar.
const ACTIONS_W: f32 = 140.0;

/// Width of the action buttons when they follow the Warp fields (one more than the box bar: Reset).
const WARP_ACTIONS_W: f32 = 176.0;

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
    bar_fields(app, ui, t, BarGroups::ALL);
}

/// The Free Transform fields of the options bar (the groups of `groups`; Content-Aware Scale
/// shows the reference point, X/Y and W/H): the library's [`TransformFields`] over the session's
/// box. W and H show the unit picked from their right-click menu, and every field takes the units
/// a number may carry when typed (`40px`, `2in`, `50%`, #6284) and arithmetic.
pub(crate) fn bar_fields(app: &mut PhotocraftApp, ui: &mut egui::Ui, t: &TransformSession, groups: BarGroups) {
    let u = size_unit(app);
    let (state, rect) = (box_state(t), start_rect(t));
    let numbers = LocaleNumbers::of(ui).as_ref().clone().ungrouped();
    let mut pixels = RulerUnits::pixels(numbers.clone());
    pixels.decimals = 2;
    // Percent is a length unit too, scaled per axis so 100 is the box's original side: a typed
    // length converts to a percentage that keeps two decimals (the library's own percent size
    // rounds to one, which would move "5cm" by up to 0.05 % of the side).
    let per_doc =
        |side: f64| if u.unit == photocraft_engine::prefs::Unit::Percent { 100.0 / side.abs().max(1e-9) } else { u.unit.from_px(1.0, u.dpi, 1.0, u.ppi) };
    let (unit, decimals) = if u.unit == photocraft_engine::prefs::Unit::Percent {
        (Unit::new("{n}%"), 2)
    } else {
        (Unit::new(&format!("{{n}} {}", u.unit.suffix())), u.unit.decimals())
    };
    let side_units =
        |side: f64| RulerUnits { per_doc: per_doc(side), ticks: Ticks::Decimal { whole: false }, numbers: numbers.clone(), unit: unit.clone(), decimals };
    let size = SizeUnit::Length { x: side_units(t.rect[2] - t.rect[0]), y: side_units(t.rect[3] - t.rect[1]) };
    let labels = TransformFieldLabels {
        x: FieldLabel::new(tl!("X"), tl!("X")),
        y: FieldLabel::new(tl!("Y"), tl!("Y")),
        w: FieldLabel::new(tl!("W"), tl!("Width")),
        h: FieldLabel::new(tl!("H"), tl!("Height")),
        link: tl!("Maintain aspect ratio").to_string(),
        angle: FieldLabel::new("", tl!("Rotate")),
        skew_h: FieldLabel::new(tl!("H"), tl!("Set horizontal skew")),
        skew_v: FieldLabel::new(tl!("V"), tl!("Set vertical skew")),
        percent: Unit::new("{n}%"),
        degrees: Unit::new("{n}°"),
    };
    // A typed length converts at the document's resolution; W and H also take a percentage of the
    // box's original side.
    let (side_w, side_h) = (t.rect[2] - t.rect[0], t.rect[3] - t.rect[1]);
    let parser = |field: TransformField, text: &str, _: craft::draw::ui::ParseContext| {
        use photocraft_engine::prefs::Unit as Len;
        let (unit, suffix, extent) = match field {
            TransformField::X | TransformField::Y => (Some(Len::Pixels), "px", None),
            TransformField::W => (Some(u.unit), u.unit.suffix(), Some(side_w)),
            TransformField::H => (Some(u.unit), u.unit.suffix(), Some(side_h)),
            _ => (None, "°", None),
        };
        let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        crate::numeric_expression::parse_with_units(&text, Some(crate::numeric_expression::Units { unit, suffix, dpi: u.dpi, ppi: u.ppi, extent }))
    };
    let entries = crate::rulers::unit_entries(u.unit);
    let mut picked = None;
    let mut linked = !app.ui.transform_unlinked;
    let mut interp = t.interpolation.clone();
    let opts = [
        ("bicubic".to_string(), "Bicubic".to_string()),
        ("bilinear".to_string(), "Bilinear".to_string()),
        ("nearest".to_string(), "Nearest Neighbor".to_string()),
    ];
    let edit = TransformFields::new("transform-fields", &state, rect, labels)
        .units(&pixels, &pixels)
        .size_unit(size)
        .linked(&mut linked)
        .skew_v(t.skew_v)
        .size_menu(&entries, &mut picked)
        .interpolation(tl!("Interpolation"), &mut interp, &opts)
        .parser(parser)
        .groups(groups)
        .show_bar(ui);
    if let Some(i) = picked {
        set_size_unit(app, crate::rulers::unit_name_at(i));
    }
    app.ui.transform_unlinked = !linked;
    if interp != t.interpolation
        && let Some(s) = app.ui.transform.as_mut()
    {
        s.interpolation = interp;
    }
    // A cancelled gesture puts the box back as it was when the gesture began.
    let start = egui::Id::NULL.with("transform-bar-start");
    match edit.event {
        Some(EditEvent::Begin { value, .. }) => {
            ui.data_mut(|d| d.insert_temp(start, (t.quad, t.pivot, t.skew_v)));
            if let Some(row) = value {
                apply_row(app, &row);
            }
        }
        Some(EditEvent::Update { value, .. } | EditEvent::Set { value, .. }) => apply_row(app, &value),
        Some(EditEvent::Commit { value, changed: true, .. }) => apply_row(app, &value),
        Some(EditEvent::Cancel { .. }) => {
            if let (Some((quad, pivot, skew_v)), Some(s)) = (ui.data(|d| d.get_temp::<([[f64; 2]; 4], [f64; 2], f64)>(start)), app.ui.transform.as_mut()) {
                (s.quad, s.pivot, s.skew_v) = (quad, pivot, skew_v);
            }
        }
        _ => {}
    }
}

/// The box and V skew a field edit gave, applied to the session.
fn apply_row(app: &mut PhotocraftApp, row: &TransformRow) {
    let Some(s) = app.ui.transform.as_mut() else { return };
    if row.state.quad.iter().all(|p| p.is_finite()) && row.state.pivot.is_finite() {
        apply_state(s, &row.state);
        s.skew_v = row.skew_v.clamp(-SKEW_LIMIT, SKEW_LIMIT);
    }
}

/// A caption and a percentage field of the Warp bar. Returns the value this frame's edit gave: a
/// typed, scrubbed or stepped value, or the value a cancelled gesture began with.
fn warp_cell(ui: &mut egui::Ui, id: &str, caption: &str, name: &str, value: f32) -> Option<f32> {
    let numbers = LocaleNumbers::of(ui).as_ref().clone().ungrouped();
    let start = egui::Id::NULL.with(("transform-cell-start", id));
    let edit = OptionsBar::cell(ui, caption, CellWidth::Md, |ui, caption| {
        NumberField::new(id, name, f64::from(value), &numbers)
            .range(-100.0..=100.0)
            .decimals(0..=1)
            .unit(Unit::new("{n}%"))
            .or_parse(|text| crate::numeric_expression::parse(&text.split_whitespace().collect::<String>()))
            .size(ControlSize::Sm)
            .scrub_by(caption)
            .show(ui)
    });
    match edit.event {
        Some(EditEvent::Begin { value: first, .. }) => {
            ui.data_mut(|d| d.insert_temp(start, value));
            first.map(|v| v as f32)
        }
        Some(EditEvent::Update { value, .. } | EditEvent::Set { value, .. }) => Some(value as f32),
        Some(EditEvent::Commit { value, changed: true, .. }) => Some(value as f32),
        Some(EditEvent::Cancel { .. }) => ui.data(|d| d.get_temp::<f32>(start)),
        _ => None,
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
            if let Some(f) = warp_cell(ui, id, label, tl!(label), *v as f32)
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
    ui.painter().rect_stroke(box_r, craft::ui::theme::scale::Radius::R2.px(), stroke, egui::StrokeKind::Inside);
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
    ui.add_space(craft::ui::theme::scale::Space::S8.px());
    // Cancel and Commit keep the library order at the end of the bar; Reset, the divider and the mode switch follow them.
    let style = OptionsBar::style(&<craft::theme::Tokens as craft::ui::theme::TokensOf>::of(ui), false);
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
    let next = box_state(s).scaled_about_pivot(kx, ky);
    apply_state(s, &next);
}

pub(super) fn rotate_about_pivot(app: &mut PhotocraftApp, da: f64) {
    let Some(s) = app.ui.transform.as_mut() else { return };
    let next = box_state(s).turned_about_pivot(da);
    apply_state(s, &next);
}
