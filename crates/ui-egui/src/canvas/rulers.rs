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
use egui::{Color32, Rect, vec2};
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

/// A guide being dragged, mirrored from the guides' edit events for the Info panel and the tools
/// that stand aside while a guide moves: from a ruler (new) or an existing one (index).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GuideDrag {
    pub vertical: bool,
    pub index: Option<usize>,
    pub pos: f64,
}

/// Canvas area left after the rulers.
pub fn content_rect(app: &PhotocraftApp, rect: Rect) -> Rect {
    if app.ui.extras.rulers { Rect::from_min_max(rect.min + vec2(RULER, RULER), rect.max) } else { rect }
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

/// What the guides took of a pass.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GuidesTaken {
    /// The guides own the current press: the tool input leaves it alone.
    pub claimed: bool,
    /// A guide right-clicked (vertical?, index): the guide menu opens on it.
    pub context: Option<(bool, usize)>,
    /// The guides set the cursor (hover or drag): the tool cursors stand aside.
    pub cursor: bool,
}

/// The library's names for the guides shown: by orientation and place, so they survive a delete
/// before them (the selection names a guide the same way). Guides at one place are told apart by
/// their order. Each is (key, vertical, index).
fn guide_keys(guides: &photocraft_doc::Guides) -> Vec<(u64, bool, usize)> {
    use std::hash::{Hash, Hasher};
    let mut keys: Vec<(u64, bool, usize)> = Vec::new();
    for (vertical, positions) in [(true, &guides.vertical), (false, &guides.horizontal)] {
        for (index, position) in positions.iter().enumerate() {
            let same = positions[..index].iter().filter(|p| p.to_bits() == position.to_bits()).count();
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            (vertical, position.to_bits(), same).hash(&mut hasher);
            keys.push((hasher.finish(), vertical, index));
        }
    }
    keys
}

/// Whether guides take the pointer and keys with `tool` and `mods`: the Move tool, or ⌘ with a
/// selection tool or a tool on which ⌘ is the Move tool (#2690, #6288); shown, unlocked, and not
/// under a Free Transform box.
pub(crate) fn guides_take_pointer(app: &PhotocraftApp, tool: crate::state::Tool, mods: egui::Modifiers) -> bool {
    let visible = app.ui.extras.guides && app.ui.view.shows(app.ui.view.show.canvas_guides) && !app.ui.extras.lock_guides;
    visible && app.ui.transform.is_none() && (tool == crate::state::Tool::Move || crate::canvas::command_takes_guides(app, tool, mods))
}

/// The guides of `doc` (or the New Guide Layout preview) over the canvas, and the drags out of the
/// rulers (their id is [`guides_id`]). `interactive` lets the pointer and keys take them
/// ([`guides_take_pointer`]); the drag of an existing guide, its click (select, ⌥ turns it), a
/// press elsewhere (deselects), Delete, the arrow keys, the readout and the cursor are the
/// library's. Shown before the tool input, which skips a press the guides claimed.
#[allow(clippy::too_many_arguments)]
pub fn show_guides(
    app: &mut PhotocraftApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    response: &egui::Response,
    xf: &ViewXform,
    doc: &Document,
    idx: usize,
    interactive: bool,
    mods: egui::Modifiers,
) -> GuidesTaken {
    let g = &app.session.prefs().guides_grid_and_slices;
    let preview = crate::guide_layout_ui::preview(app, doc);
    let source = preview.as_ref().map(|(guides, _)| guides).unwrap_or(&doc.guides);
    let (base, style) =
        (preview.as_ref().map(|(_, color)| *color).unwrap_or_else(|| pref_color(&g.guide_color, Color32::from_rgb(74, 255, 255))), g.guide_style);
    let keys = guide_keys(source);
    let mut list = Vec::new();
    if (app.ui.extras.guides && app.ui.view.shows(app.ui.view.show.canvas_guides)) || preview.is_some() {
        for &(key, vertical, i) in &keys {
            let positions = if vertical { &source.vertical } else { &source.horizontal };
            let mut guide = Guide::new(key, if vertical { Orientation::Vertical } else { Orientation::Horizontal }, positions[i]);
            // A guide given a colour in New Guide keeps it; the rest use the preference colour
            // (or, while New Guide Layout previews, the colour it will set).
            guide.color = Some(source.color(vertical, positions[i]).map_or(base, |[r, g, b]| Color32::from_rgb(r, g, b)));
            guide.line = line_style(style);
            list.push(guide);
        }
    }
    let interactive = interactive && preview.is_none() && app.session.active_index() == Some(idx);
    let key_of = |vertical: bool, index: usize| keys.iter().find(|k| k.1 == vertical && k.2 == index).map(|k| k.0);
    let selected = crate::guide_select::selected(app).and_then(|(vertical, i)| key_of(vertical, i));
    let [ux, uy] = ruler_units(app, ui, doc);
    let view = xf.canvas_view();
    let origin = Point::new(doc.ruler_origin[0], doc.ruler_origin[1]);
    let cursor_before = ctx.output(|o| o.cursor_icon);
    let cell = RefCell::new(&mut *app);
    let out = Guides::new(guides_salt(idx), &list, &ux, &uy, GuideNames::of(ui))
        .selected(selected)
        .interactive(interactive)
        .modifiers(mods)
        .bounds(DocRect::new(0.0, 0.0, f64::from(doc.size.width), f64::from(doc.size.height)))
        .zero(origin)
        .readout("X:", "Y:")
        .snap(|orientation, position, _| crate::snap_ui::snap_guide(&mut cell.borrow_mut(), orientation == Orientation::Vertical, position))
        .show(ui, response, &view);
    drop(cell);
    let find = |key: u64| keys.iter().find(|k| k.0 == key).map(|k| (k.1, k.2));
    let name = |vertical: bool| if vertical { "vertical" } else { "horizontal" };
    let mut deleted = None;
    match out.event {
        // A drag out of a ruler is a new guide, added on its release (a drag released in the pass
        // it begins arrives as one `Set`).
        Some((None, EditEvent::Begin { value: Some(value), .. } | EditEvent::Update { value, .. })) => {
            if let Some(pos) = value.position {
                app.guide_drag = Some(GuideDrag { vertical: value.orientation == Orientation::Vertical, index: None, pos });
            }
        }
        Some((None, EditEvent::Commit { value, .. } | EditEvent::Set { value, .. })) => {
            app.guide_drag = None;
            if let Some(pos) = value.position {
                let vertical = value.orientation == Orientation::Vertical;
                let _ = app.run("view.newGuide", json!({"orientation": name(vertical), "position": (pos * 1000.0).round() / 1000.0}));
                app.ui.extras.guides = true;
            }
        }
        // A guide taken on the canvas: moved to where it is let go, or deleted (let go on a ruler
        // or off the canvas, or Delete); one history step each.
        Some((Some(key), event)) => {
            if let Some((vertical, index)) = find(key) {
                match event {
                    EditEvent::Begin { value: Some(value), .. } | EditEvent::Update { value, .. } => {
                        if let Some(pos) = value.position {
                            app.guide_drag = Some(GuideDrag { vertical, index: Some(index), pos });
                        }
                    }
                    EditEvent::Commit { value, changed, .. } => {
                        app.guide_drag = None;
                        match value.position {
                            Some(pos) if changed => {
                                let _ = app
                                    .run("view.moveGuide", json!({"orientation": name(vertical), "index": index, "position": (pos * 1000.0).round() / 1000.0}));
                            }
                            Some(_) => {}
                            None => deleted = app.run("view.deleteGuide", crate::guide_select::delete_params(vertical, index)).ok().map(|_| (vertical, index)),
                        }
                    }
                    EditEvent::Set { value, .. } => match value.position {
                        Some(pos) => {
                            let _ =
                                app.run("view.moveGuide", json!({"orientation": name(vertical), "index": index, "position": (pos * 1000.0).round() / 1000.0}));
                        }
                        None => deleted = app.run("view.deleteGuide", crate::guide_select::delete_params(vertical, index)).ok().map(|_| (vertical, index)),
                    },
                    _ => app.guide_drag = None,
                }
            }
        }
        Some((None, _)) => app.guide_drag = None,
        None => {}
    }
    // The guides' selection, by the names of the guides as they were: a guide after a deleted one
    // has moved up an index.
    match out.select {
        Some(None) => crate::guide_select::clear(app),
        Some(Some(key)) => {
            if let Some((vertical, index)) = find(key) {
                let index = match deleted {
                    Some((v, i)) if v == vertical && i < index => index - 1,
                    _ => index,
                };
                crate::guide_select::select(app, vertical, index);
            }
        }
        None => {}
    }
    // ⌥-click on a guide turns it, through the point clicked, as in Photoshop.
    if let Some((key, clicked_with, at)) = out.clicked
        && clicked_with.alt
        && !clicked_with.shift
        && !clicked_with.command
        && let Some((vertical, index)) = find(key)
    {
        crate::guide_select::flip(app, vertical, index, [at.x, at.y]);
    }
    // Double-clicking a guide opens Preferences › Guides, Grid & Slices, as in Photoshop.
    if out.double_clicked.is_some()
        && let Err(e) = crate::menus::invoke(app, ctx, "edit.preferences.guidesGridAndSlices", json!({}))
    {
        app.ui.status = e;
    }
    GuidesTaken { claimed: out.claimed, context: out.context.and_then(find), cursor: ctx.output(|o| o.cursor_icon) != cursor_before }
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
