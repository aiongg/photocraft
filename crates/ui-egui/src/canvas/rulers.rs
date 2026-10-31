//! View extras: rulers (⌘R), grid (⌘'), guides (⌘;). Guides are dragged out of the rulers and
//! moved with the Move tool (drag one off the canvas or onto a ruler to delete it), all through
//! undoable engine commands (`view.newGuide` / `view.moveGuide` / `view.deleteGuide`).
//!
//! The rulers, the guides' look and the drags out of the rulers are craft_ui's `Rulers`,
//! `Guides` and `CanvasGrid`; this file maps PhotoCraft's preferences and commands onto them.

use std::cell::RefCell;

use craft::ui::canvas::kurbo::{Point, Rect as DocRect};
use craft::ui::canvas::{CanvasGrid, Guide, GuideLine, GuideNames, Guides, Orientation, RulerNames, RulerUnits, Rulers, Ticks};
use craft::ui::edit::EditEvent;
use craft::ui::localized::{Labels as _, LocaleNumbers};
use craft::ui::menu::{ContextMenu, OpenAt};
use egui::{Color32, Pos2, Rect, vec2};
use photocraft_doc::Document;
use photocraft_engine::prefs::{GuidesGridAndSlices, Unit};
use serde_json::json;

use crate::PhotocraftApp;
use crate::canvas::ViewXform;

/// Thickness of a ruler strip, in points: `ruler.size` (`Rulers::style`), which a test pins.
pub const RULER: f32 = 16.0;

/// A `#rrggbb` preference colour.
fn pref_color(s: &str, fallback: Color32) -> Color32 {
    photocraft_engine::prefs::parse_hex(s).map_or(fallback, |c| Color32::from_rgb(c[0], c[1], c[2]))
}

fn line_style(style: photocraft_engine::prefs::LineStyle) -> GuideLine {
    use photocraft_engine::prefs::LineStyle;
    match style {
        LineStyle::Lines => GuideLine::Solid,
        LineStyle::Dashed => GuideLine::Dashed,
        LineStyle::Dots => GuideLine::Dots,
    }
}

/// English display name for a length unit, as shown in Preferences › Units & Rulers.
fn unit_label(u: Unit) -> &'static str {
    match u {
        Unit::Pixels => "Pixels",
        Unit::Inches => "Inches",
        Unit::Centimeters => "Centimeters",
        Unit::Millimeters => "Millimeters",
        Unit::Points => "Points",
        Unit::Picas => "Picas",
        Unit::Percent => "Percent",
    }
}

/// Every length unit, in the Preferences › Units & Rulers order.
const UNITS: [Unit; 7] = [Unit::Pixels, Unit::Inches, Unit::Centimeters, Unit::Millimeters, Unit::Points, Unit::Picas, Unit::Percent];

/// The units Photoshop's Create Rectangle (and the other shape dialogs) offer on a right-click of
/// Width or Height: the lengths, without Percent.
pub(crate) const LENGTH_UNITS: [Unit; 6] = [Unit::Pixels, Unit::Inches, Unit::Centimeters, Unit::Millimeters, Unit::Points, Unit::Picas];

/// The ruler right-click menu: every unit of `units` with its (translated) label and whether it's
/// the one in effect. Picking one changes the ruler unit exactly as the preference pane does.
fn unit_menu_items(current: Unit, units: &[Unit]) -> Vec<(String, &'static str, bool)> {
    units.iter().map(|&u| (tl!(unit_label(u)).to_string(), u.name(), u == current)).collect()
}

/// The rows of a number field's unit menu: every unit, with `current` marked. A pick is an index
/// into the units; [`unit_name_at`] names it.
pub(crate) fn unit_entries(current: Unit) -> Vec<craft::ui::menu::MenuEntry<usize>> {
    UNITS.iter().enumerate().map(|(i, &u)| craft::ui::menu::MenuItem::new(tl!(unit_label(u)), i).radio(u == current).into()).collect()
}

/// The name of the unit at `index` of [`unit_entries`].
pub(crate) fn unit_name_at(index: usize) -> &'static str {
    UNITS[index].name()
}

/// The length units menu that right-clicking a ruler (or Free Transform's W and H fields) opens;
/// the chosen unit's name is returned (`None` if the menu is dismissed without a pick).
pub(crate) fn unit_menu(ui: &mut egui::Ui, current: Unit) -> Option<&'static str> {
    unit_menu_of(ui, current, &UNITS)
}

/// [`unit_menu`] with only `units` in it.
pub(crate) fn unit_menu_of(ui: &mut egui::Ui, current: Unit, units: &[Unit]) -> Option<&'static str> {
    crate::widgets::menu_scroll(ui, |ui| {
        ui.set_min_width(150.0);
        let mut chosen = None;
        for (label, name, checked) in unit_menu_items(current, units) {
            let label = if checked { format!("✓ {label}") } else { label };
            if ui.button(label).clicked() {
                ui.close();
                chosen = Some(name);
            }
        }
        chosen
    })
}

/// A guide being dragged: from a ruler (new) or an existing one (index).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GuideDrag {
    pub vertical: bool,
    pub index: Option<usize>,
    pub pos: f64,
    /// Where the pointer went down, on the guide's axis: released there, an existing guide is
    /// clicked (selected), not moved.
    pub press: f64,
    /// Whether the pointer was over a ruler when the guide was let go (#6286): a guide released
    /// there is removed, whatever its own axis reads.
    pub over_ruler: bool,
}

impl GuideDrag {
    /// An existing guide taken at document point `p`.
    pub fn existing(vertical: bool, index: usize, p: [f64; 2]) -> Self {
        let at = if vertical { p[0] } else { p[1] };
        Self { vertical, index: Some(index), pos: at, press: at, over_ruler: false }
    }
}

/// Canvas area left after the rulers.
pub fn content_rect(app: &PhotocraftApp, rect: Rect) -> Rect {
    if app.ui.extras.rulers { Rect::from_min_max(rect.min + vec2(RULER, RULER), rect.max) } else { rect }
}

/// Whether the screen point `p` is over a ruler, inside the full canvas rect `full`. A guide let go
/// there is removed, as in Photoshop, and either ruler removes it (#6286): the top ruler measures
/// document x and the left one y, so the ruler the guide was not dragged out of can lie over a
/// coordinate that is still inside the document on the guide's own axis.
pub(crate) fn dropped_on_a_ruler(app: &PhotocraftApp, full: Rect, p: Pos2) -> bool {
    if !app.ui.extras.rulers {
        return false;
    }
    // The rulers take the strips [`content_rect`] trims off the canvas rect, and nothing else: a
    // canvas point on the boundary is canvas.
    let content = content_rect(app, full);
    full.contains(p) && (p.x < content.min.x || p.y < content.min.y)
}

/// Grid spacing in document pixels. Percentages use each axis's own extent.
pub(crate) fn grid_major_px(g: &GuidesGridAndSlices, dpi: f64, size: [f64; 2], ppi: f64) -> [f64; 2] {
    size.map(|extent| g.major_px(dpi, extent, ppi))
}

/// View › Show › Grid, from Preferences › Guides, Grid & Slices (spacing, subdivisions, colour,
/// style). Photoshop's default is a gridline every inch with 4 subdivisions.
pub fn draw_grid(app: &PhotocraftApp, ui: &egui::Ui, xf: &ViewXform, doc: &Document) {
    let g = &app.session.prefs().guides_grid_and_slices;
    let ppi = app.session.prefs().units_and_rulers.point_size.per_inch();
    let (w, h) = (f64::from(doc.size.width), f64::from(doc.size.height));
    let major = grid_major_px(g, f64::from(doc.resolution_dpi.max(1.0)), [w, h], ppi);
    let line = match line_style(g.grid_style) {
        GuideLine::Solid => craft::ui::canvas::GridLine::Solid,
        GuideLine::Dashed => craft::ui::canvas::GridLine::Dashed,
        GuideLine::Dots => craft::ui::canvas::GridLine::Dots,
    };
    CanvasGrid::new("pc-grid")
        .spacing(craft::ui::canvas::kurbo::Vec2::new(major[0], major[1]))
        .subdivisions(g.subdivisions)
        .bounds(DocRect::new(0.0, 0.0, w, h))
        .color(Some(pref_color(&g.grid_color, Color32::from_gray(140))))
        .line(line)
        .show(ui, &xf.canvas_view());
}

/// The cursor over a guide: the arrow across it as the view shows it. A vertical guide moves left
/// and right in the document; a rotated view turns that, as it turns the guide (#3277).
pub fn guide_cursor(xf: &ViewXform, vertical: bool) -> egui::CursorIcon {
    crate::tool_cursor::resize(if vertical { [1.0, 0.0] } else { [0.0, 1.0] }, xf)
}

/// The library's key for guide `index` of an orientation: guides are named by place in the document.
fn guide_key(vertical: bool, index: usize) -> u64 {
    (index as u64) << 1 | u64::from(vertical)
}

/// The ruler unit of each axis (x, y) for `doc` from Preferences › Units & Rulers.
pub(crate) fn ruler_units(app: &PhotocraftApp, ui: &egui::Ui, doc: &Document) -> [RulerUnits; 2] {
    ruler_units_with(app, (*LocaleNumbers::of(ui)).clone(), doc)
}

/// [`ruler_units`] for code with no `Ui`, given the locale's numbers.
pub(crate) fn ruler_units_with(app: &PhotocraftApp, numbers: LocaleNumbers, doc: &Document) -> [RulerUnits; 2] {
    let ur = &app.session.prefs().units_and_rulers;
    let (unit, ppi) = (ur.rulers, ur.point_size.per_inch());
    let dpi = f64::from(doc.resolution_dpi.max(1.0));
    let numbers = numbers.ungrouped();
    [f64::from(doc.size.width), f64::from(doc.size.height)].map(|extent| RulerUnits {
        per_doc: 1.0 / unit.to_px(1.0, dpi, extent, ppi).max(1e-9),
        ticks: Ticks::Decimal { whole: unit == Unit::Pixels },
        numbers: numbers.clone(),
        unit: craft::ui::localized::Unit::new(&format!("{{n}} {}", unit.suffix())),
        decimals: unit.decimals(),
    })
}

/// The id of the guides of document `idx`, which the rulers hand a drag to.
pub fn guides_id(ui: &egui::Ui, idx: usize) -> egui::Id {
    ui.make_persistent_id(egui::IdSalt::new(guides_salt(idx)))
}

fn guides_salt(idx: usize) -> (&'static str, usize) {
    ("pc-guides", idx)
}

/// The guides of `doc` (or the New Guide Layout preview) over the canvas, and the drags out of the
/// rulers (their id is [`guides_id`]). The Move tool's own guide drags are moved by the
/// tool input, so the guides here only draw; a guide being moved shows where it is.
pub fn draw_guides(app: &mut PhotocraftApp, ui: &mut egui::Ui, response: &egui::Response, xf: &ViewXform, doc: &Document, idx: usize) {
    let drag = app.guide_drag;
    let g = &app.session.prefs().guides_grid_and_slices;
    let preview = crate::guide_layout_ui::preview(app, doc);
    let source = preview.as_ref().map(|(guides, _)| guides).unwrap_or(&doc.guides);
    let (base, style) =
        (preview.as_ref().map(|(_, color)| *color).unwrap_or_else(|| pref_color(&g.guide_color, Color32::from_rgb(74, 255, 255))), g.guide_style);
    let mut list = Vec::new();
    if (app.ui.extras.guides && app.ui.view.shows(app.ui.view.show.canvas_guides)) || preview.is_some() {
        for (vertical, positions) in [(true, &source.vertical), (false, &source.horizontal)] {
            for (i, &position) in positions.iter().enumerate() {
                // A guide the Move tool is carrying is drawn where the pointer has it.
                let position = match drag {
                    Some(d) if d.vertical == vertical && d.index == Some(i) => d.pos,
                    _ => position,
                };
                let mut guide = Guide::new(guide_key(vertical, i), if vertical { Orientation::Vertical } else { Orientation::Horizontal }, position);
                // A guide given a colour in New Guide keeps it; the rest use the preference colour
                // (or, while New Guide Layout previews, the colour it will set).
                guide.color = Some(source.color(vertical, positions[i]).map_or(base, |[r, g, b]| Color32::from_rgb(r, g, b)));
                guide.line = line_style(style);
                list.push(guide);
            }
        }
    }
    let selected = crate::guide_select::selected(app).map(|(vertical, i)| guide_key(vertical, i));
    let [ux, uy] = ruler_units(app, ui, doc);
    let view = xf.canvas_view();
    let origin = Point::new(doc.ruler_origin[0], doc.ruler_origin[1]);
    let cell = RefCell::new(&mut *app);
    let out = Guides::new(guides_salt(idx), &list, &ux, &uy, GuideNames::of(ui))
        .selected(selected)
        .bounds(DocRect::new(0.0, 0.0, f64::from(doc.size.width), f64::from(doc.size.height)))
        .zero(origin)
        .snap(|orientation, position, _| crate::snap_ui::snap_guide(&mut cell.borrow_mut(), orientation == Orientation::Vertical, position))
        .show(ui, response, &view);
    drop(cell);
    // A drag out of a ruler: mirrored into `guide_drag` for the Info panel, the readout and the
    // tools that stand aside while a guide moves, and applied on its release.
    match out.event {
        Some((None, EditEvent::Begin { value: Some(value), .. } | EditEvent::Update { value, .. })) => {
            if let Some(pos) = value.position {
                let vertical = value.orientation == Orientation::Vertical;
                app.guide_drag = Some(GuideDrag { vertical, index: None, pos, press: pos, over_ruler: false });
            }
        }
        // A drag released in the pass it begins arrives as one `Set`.
        Some((None, EditEvent::Commit { value, .. } | EditEvent::Set { value, .. })) => {
            app.guide_drag = None;
            if let Some(pos) = value.position {
                let vertical = value.orientation == Orientation::Vertical;
                let orientation = if vertical { "vertical" } else { "horizontal" };
                let _ = app.run("view.newGuide", json!({"orientation": orientation, "position": (pos * 1000.0).round() / 1000.0}));
                app.ui.extras.guides = true;
            }
        }
        Some((None, EditEvent::Cancel { .. })) => app.guide_drag = None,
        _ => {}
    }
}

/// The position readout of a guide being dragged: `X:` for a vertical guide, `Y:` for a horizontal
/// one, measured from the ruler zero point in the ruler unit. Pixels keep one decimal, as
/// Photoshop's "X: 103.2 px".
pub fn guide_drag_readout(app: &PhotocraftApp, d: GuideDrag) -> Option<(&'static str, String)> {
    let doc = &app.session.active()?.doc;
    let ur = &app.session.prefs().units_and_rulers;
    let axis = usize::from(!d.vertical);
    let px = d.pos - doc.ruler_origin.get(axis).copied().unwrap_or(0.0);
    let extent = if d.vertical { doc.size.width } else { doc.size.height } as f64;
    let value = match ur.rulers {
        Unit::Pixels => photocraft_engine::prefs::fmt_decimals(px, 1),
        _ => ur.format(px, f64::from(doc.resolution_dpi), extent),
    };
    Some((if d.vertical { "X:" } else { "Y:" }, format!("{value} {}", ur.rulers.suffix())))
}

/// Draw [`guide_drag_readout`] beside the pointer while a guide is dragged.
pub fn draw_guide_drag_readout(app: &PhotocraftApp, ctx: &egui::Context) {
    let (Some(d), Some(at)) = (app.guide_drag, ctx.pointer_latest_pos()) else { return };
    if let Some((label, value)) = guide_drag_readout(app, d) {
        crate::canvas::draw_value_readout(app, ctx, "guide-drag-readout", at, [label], [value]);
    }
}

/// Existing guide under a document point (within 4 screen px).
pub fn guide_at(app: &PhotocraftApp, x: f64, y: f64) -> Option<(bool, usize)> {
    // Hidden canvas guides are not interactive: the Move tool must reach the layer below.
    // Keep hit testing consistent with the visibility gate in canvas::draw.
    if !app.ui.extras.guides || !app.ui.view.shows(app.ui.view.show.canvas_guides) || app.ui.extras.lock_guides {
        return None;
    }
    let doc = &app.session.active()?.doc;
    let tol = 4.0 / app.point_zoom().max(0.01) as f64;
    for (i, g) in doc.guides.vertical.iter().enumerate() {
        if (*g - x).abs() <= tol {
            return Some((true, i));
        }
    }
    for (i, g) in doc.guides.horizontal.iter().enumerate() {
        if (*g - y).abs() <= tol {
            return Some((false, i));
        }
    }
    None
}

/// Finish a guide drag: create, move, or delete (dropped outside the canvas or on a ruler). An
/// existing guide released where it was pressed is a click: it's selected and stays put, as in
/// Photoshop; a moved one stays selected.
pub fn finish_drag(app: &mut PhotocraftApp, d: GuideDrag) {
    let Some(size) = app.session.active().map(|s| s.doc.size) else { return };
    // A drop on a ruler removes the guide even when the drag held its own axis, which otherwise
    // reads as a click: dragged sideways onto the left ruler, a horizontal guide never moves in y.
    if let Some(i) = d.index
        && !d.over_ruler
        && (d.pos - d.press).abs() * f64::from(app.point_zoom().max(0.01)) < 0.5
    {
        crate::guide_select::select(app, d.vertical, i);
        return;
    }
    let extent = if d.vertical { size.width } else { size.height } as f64;
    let orientation = if d.vertical { "vertical" } else { "horizontal" };
    let inside = !d.over_ruler && d.pos >= 0.0 && d.pos <= extent;
    let pos = (d.pos * 1000.0).round() / 1000.0;
    let _ = match (d.index, inside) {
        (None, true) => app.run("view.newGuide", json!({"orientation": orientation, "position": pos})),
        (Some(i), true) => app
            .run("view.moveGuide", json!({"orientation": orientation, "index": i, "position": pos}))
            .inspect(|_| crate::guide_select::select(app, d.vertical, i)),
        (Some(i), false) => app.run("view.deleteGuide", json!({"orientation": orientation, "index": i})),
        (None, false) => Ok(serde_json::Value::Null),
    };
    if d.index.is_none() && inside {
        app.ui.extras.guides = true;
    }
}
/// The rulers of `doc`'s view in `full` (the canvas rect before [`content_rect`]), counting from
/// its zero point. The active view's rulers take the pointer: a drag out of them is a new guide
/// (`guides`, the id [`draw_guides`] returned), one out of the corner moves the zero point (Shift
/// snaps to the ticks, else View › Snap To), its double-click puts it back, and a right-click
/// switches the unit. Other tiles only paint (#6017).
#[allow(clippy::too_many_arguments)]
pub fn show_rulers(
    app: &mut PhotocraftApp,
    ui: &mut egui::Ui,
    response: &egui::Response,
    full: Rect,
    xf: &ViewXform,
    doc: &Document,
    idx: usize,
    guides: Option<egui::Id>,
) {
    let [ux, uy] = ruler_units(app, ui, doc);
    let unit = app.session.prefs().units_and_rulers.rulers;
    let interactive = guides.is_some();
    let view = xf.canvas_view();
    let zero = Point::new(doc.ruler_origin[0], doc.ruler_origin[1]);
    let cell = RefCell::new(&mut *app);
    let out = Rulers::new(("pc-rulers", idx), &ux, &uy, RulerNames::of(ui))
        .zero(zero)
        .interactive(interactive)
        .snap(|orientation, position, _| crate::snap_ui::snap_guide(&mut cell.borrow_mut(), orientation == Orientation::Vertical, position))
        .show(ui, full, &view, guides);
    drop(cell);
    if !interactive {
        return;
    }
    if out.reset_origin {
        let _ = app.run("view.resetRulerOrigin", json!({}));
    }
    if let Some(EditEvent::Commit { value, changed: true, .. } | EditEvent::Set { value, .. }) = out.origin {
        let _ = app.run("view.setRulerOrigin", json!({"x": value.x, "y": value.y}));
    }
    // Right-click a ruler to switch the ruler unit.
    let open = out.context.map(|_| OpenAt::Pointer(ui.input(|i| i.pointer.interact_pos()).unwrap_or(full.min)));
    let entries = unit_entries(unit);
    if let Some(i) = ContextMenu::new(&entries).open_at(open).own_gestures(false).show(ui, response).chosen {
        let _ = app.run("prefs.set", json!({"path": "unitsAndRulers.rulers", "value": unit_name_at(i)}));
    }
}

#[cfg(test)]
mod tests;
