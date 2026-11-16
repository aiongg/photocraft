use super::{PhotocraftApp, Tool, json, vec2};
use craft::behavior::IconId;
use craft::color::ui::{PairWell, SwatchPair, Well};
use craft::ui::buttons::ToggleButton;
use craft::ui::contract::{Bind, Gap, PaneEdge, Show as _, ShowContainer as _, clicked};
use craft::ui::layout_components::{Row, Stack};
use craft::ui::rails::{DrawerGroup, LastUsed, ToolRail, ToolSlot, ToolbarDrawer, ToolbarEdit, apply_toolbar_edit};
use craft::ui::runtime::env::Label;
use craft::ui::shell::ScreenModeButton;
use photocraft_engine::prefs::ToolbarCustomization;
use std::num::NonZeroU8;

use super::tool_commands;
use crate::state::ToolbarSide;

#[cfg(test)]
#[path = "toolbar_tab_reveal_tests.rs"]
mod toolbar_tab_reveal_tests;

// ----------------------------------------------------------------------------- toolbar

/// Tool groups separated by hairlines, Photoshop order.
/// Photoshop toolbar: divider-separated sections of slots; each slot is a flyout group whose button
/// shows the group's last-used tool.
pub(crate) const TOOL_SECTIONS: &[&[&[Tool]]] = &[
    &[&[Tool::Move, Tool::Artboard]],
    &[
        &[Tool::RectMarquee, Tool::EllipseMarquee, Tool::SingleRowMarquee, Tool::SingleColumnMarquee],
        &[Tool::Lasso, Tool::PolygonLasso, Tool::MagneticLasso],
        &[Tool::ObjectSelection, Tool::QuickSelection, Tool::MagicWand],
        &[Tool::Crop, Tool::PerspectiveCrop, Tool::Slice, Tool::SliceSelect],
        &[Tool::Frame],
        &[Tool::Eyedropper, Tool::ColorSampler, Tool::Ruler, Tool::Note, Tool::Count],
    ],
    &[
        &[Tool::SpotHealing, Tool::Remove, Tool::Healing, Tool::Patch, Tool::ContentAwareMove, Tool::RedEye],
        &[Tool::Brush, Tool::Pencil, Tool::ColorReplacement, Tool::MixerBrush],
        &[Tool::CloneStamp, Tool::PatternStamp],
        &[Tool::HistoryBrush, Tool::ArtHistoryBrush],
        &[Tool::Eraser, Tool::BackgroundEraser, Tool::MagicEraser],
        &[Tool::Gradient, Tool::PaintBucket],
        &[Tool::Blur, Tool::Sharpen, Tool::Smudge],
        &[Tool::Dodge, Tool::Burn, Tool::Sponge],
    ],
    &[
        &[Tool::Pen, Tool::FreeformPen, Tool::CurvaturePen, Tool::AddAnchorPoint, Tool::DeleteAnchorPoint, Tool::ConvertPoint],
        &[Tool::Type, Tool::VerticalType, Tool::VerticalTypeMask, Tool::HorizontalTypeMask],
        &[Tool::PathSelection, Tool::DirectSelection],
        &[Tool::Rectangle, Tool::EllipseShape, Tool::Triangle, Tool::Polygon, Tool::Star, Tool::Line, Tool::CustomShape],
    ],
    &[&[Tool::Hand, Tool::RotateView], &[Tool::Zoom]],
];

/// Position of `tool` in [`TOOL_SECTIONS`] read top to bottom, slot by slot. Tools outside the
/// toolbar sort last. Shift+letter orders a key's group by this so it steps like the flyout.
pub(crate) fn toolbar_position(tool: Tool) -> usize {
    TOOL_SECTIONS.iter().flat_map(|section| section.iter()).flat_map(|slot| slot.iter()).position(|&t| t == tool).unwrap_or(usize::MAX)
}

<<<<<<< HEAD
/// Opens the Edit Toolbar drawer beside the rail. The rail's "…" button and the `edit.toolbar` command both come here.
pub(crate) fn open_drawer(app: &mut PhotocraftApp) {
    app.ui.toolbar_drawer = true;
}

/// The tool each slot shows when the current tool is not in it: the most recent tool of `group_tools` (most recent first)
/// that the slot holds.
fn last_used(slots: &[ToolSlot<Tool>], group_tools: &[Tool]) -> LastUsed<Tool> {
    let mut last = LastUsed::default();
    for slot in slots {
        if let Some(tool) = group_tools.iter().find(|tool| slot.tools.iter().any(|held| held.id == **tool)) {
            last.set(slot.id, *tool);
=======
/// The toolbar's group layout from Edit › Toolbar (`edit.toolbar`), as tools: the slots' groups
/// in order, then the Extra Tools groups. Names were resolved by the engine
/// (`ToolbarCustomization::layout`), which writes `Tool` debug names.
pub(crate) fn tool_layout(custom: &photocraft_engine::prefs::ToolbarCustomization) -> (Vec<Vec<Tool>>, Vec<Vec<Tool>>) {
    let layout = custom.layout();
    let tools = |groups: Vec<Vec<String>>| -> Vec<Vec<Tool>> {
        groups.iter().map(|g| g.iter().filter_map(|n| Tool::from_name(n)).collect::<Vec<Tool>>()).filter(|g| !g.is_empty()).collect()
    };
    (tools(layout.toolbar), tools(layout.extras))
}

/// Every tool in layout order: the slots' groups, then the extras. Shift+letter steps through a
/// key's tools in this order, across slots.
pub(crate) fn layout_order(custom: &photocraft_engine::prefs::ToolbarCustomization) -> Vec<Tool> {
    let (toolbar, extras) = tool_layout(custom);
    toolbar.into_iter().chain(extras).flatten().collect()
}

/// The default slot (section, index in [`TOOL_SECTIONS`] read slot by slot) that holds `tool`.
fn home_slot(tool: Tool) -> Option<(usize, usize)> {
    let mut index = 0usize;
    for (si, section) in TOOL_SECTIONS.iter().enumerate() {
        for slot in section.iter() {
            if slot.contains(&tool) {
                return Some((si, index));
            }
            index += 1;
        }
    }
    None
}

/// The slots the toolbar draws: one per toolbar group of the stored layout, in order, each
/// listing its group's tools in order. Consecutive groups whose first tools come from the same
/// default section share a section, so the default layout keeps its dividers. Each slot carries
/// the index of its first tool's default slot in `TOOL_SECTIONS` (or one past the defaults when
/// that index is taken), so its flyout stays put when other slots are hidden or moved.
pub(crate) fn layout_sections(custom: &photocraft_engine::prefs::ToolbarCustomization) -> Vec<Vec<(usize, Vec<Tool>)>> {
    let (groups, _) = tool_layout(custom);
    let defaults: usize = TOOL_SECTIONS.iter().map(|section| section.len()).sum();
    let mut used: Vec<usize> = Vec::new();
    let mut sections: Vec<Vec<(usize, Vec<Tool>)>> = Vec::new();
    let mut last_section = None;
    for (i, group) in groups.into_iter().enumerate() {
        let home = group.first().copied().and_then(home_slot);
        let index = match home {
            Some((_, slot)) if !used.contains(&slot) => slot,
            _ => defaults + i,
        };
        used.push(index);
        let section = home.map(|(si, _)| si);
        if sections.is_empty() || section != last_section {
            sections.push(Vec::new());
        }
        last_section = section;
        if let Some(current) = sections.last_mut() {
            current.push((index, group));
>>>>>>> upstream/main
        }
    }
    last
}

<<<<<<< HEAD
/// The rail's edge for the physical side the toolbar sits on: the rail names its edge by the interface direction.
fn rail_edge(side: ToolbarSide) -> PaneEdge {
    if (side.resolve() == ToolbarSide::Right) != crate::direction::is_rtl() { PaneEdge::End } else { PaneEdge::Start }
=======
/// [`layout_sections`] for the default layout without the tools hidden in Edit › Toolbar
/// (`hidden` holds tool names: the dialog writes `Tool` debug names, `edit.toolbar` takes any
/// name `Tool::from_name` reads). A slot keeps only its visible tools, so its flyout lists just
/// those; a slot with none left is dropped, and so is a section with no slots.
#[cfg(test)]
pub(super) fn visible_sections(hidden: &[String]) -> Vec<Vec<(usize, Vec<Tool>)>> {
    layout_sections(&photocraft_engine::prefs::ToolbarCustomization { hidden: hidden.to_vec(), ..Default::default() })
}

/// A primary-button gesture started on this tool; open only after it leaves its hit area.
/// Using pointer origin rather than `Response::dragged` also works for click-only icon buttons.
pub(super) fn tool_flyout_drag_open(button: Rect, origin: Option<Pos2>, pointer: Option<Pos2>, primary_down: bool) -> bool {
    primary_down && origin.is_some_and(|p| button.contains(p)) && pointer.is_some_and(|p| !button.contains(p))
>>>>>>> upstream/main
}

/// The tool palette: a `ToolRail` over the slots, with the Quick Mask and Screen Mode buttons and the colour chips in
/// its footer, and, while the Edit Toolbar drawer is open, the drawer beside it. The caller places it on the side
/// `panels.toolbar_side` gives (AppShell's tools slot).
///
/// A frame can run this twice. Every outcome comes from the pass's own widgets (a click, a drop), and the slots, hidden
/// tools and remembered tools are read again at the start of each pass, so a second pass applies only its own input.
pub fn toolbar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let slots = tool_commands::slots(&app.session.prefs().toolbar);
    let hidden = tool_commands::hidden_tools(&app.session.prefs().toolbar.hidden);
    let remembered = last_used(&slots, &app.ui.group_tools);
    let current = app.ui.tool;
    let columns = if app.ui.panels.toolbar_double { 2 } else { 1 };
    let side = app.ui.panels.toolbar_side.resolve();
    let edge = rail_edge(side);
    let other_side = side.other();
    let move_label = if other_side == ToolbarSide::Right { tl!("Move Toolbar to the Right") } else { tl!("Move Toolbar to the Left") };

    let mut rail = ToolRail::new(Label::Empty, &slots, &current, &remembered)
        .columns(NonZeroU8::new(columns).unwrap_or(NonZeroU8::MIN))
        .edge(edge)
        .column_toggle(tl!("Toolbar").to_string())
        .side_toggle(move_label.to_string())
        .edit_toolbar(tl!("Edit Toolbar…").to_string())
        .hidden(&hidden);
    // The drawer's open flag is the one source of "editing".
    if app.ui.toolbar_drawer {
        rail = rail.editing();
    }
    let out = rail.show_rail(ui, |ui, footer| {
        let modes = |ui: &mut egui::Ui, app: &mut PhotocraftApp| {
            ToggleButton::command("select.editInQuickMaskMode").icon(IconId::new("square-dashed")).icon_only().show(ui);
            ScreenModeButton::new(app.ui.view.screen_mode).show(ui);
        };
        if footer.columns > 1 {
            Row::new().gap(Gap::Tight).show(ui, |ui, _| modes(ui, app));
        } else {
            Stack::new().gap(Gap::Tight).show(ui, |ui, _| modes(ui, app));
        }
        let column = ui.available_width();
        color_chips(app, ui, column);
    });

    if let Some(tool) = out.select {
        crate::tool_memory::select(app, tool);
    }
    // A double click on Hand fits the image on screen, on Zoom shows it at 100% (Photoshop); with no document open there
    // is nothing to do. The framework reports no double click after a modified click.
    match out.double_clicked {
        Some(Tool::Hand) => {
            let _ = crate::menus::invoke(app, &ctx, "view.fitOnScreen", json!({}));
        }
        Some(Tool::Zoom) => {
            let _ = crate::menus::invoke(app, &ctx, "view.actualPixels", json!({}));
        }
        _ => {}
    }
    if out.toggle_columns {
        app.ui.panels.toolbar_double = !app.ui.panels.toolbar_double;
    }
    if out.toggle_side {
        app.ui.panels.toolbar_side = other_side;
        // Persist the side as a discrete change; the dock may not be drawn this frame.
        crate::dock::persist(app);
    }
    if let Some(next) = &out.last_used {
        for slot in &slots {
            if let Some(tool) = next.get(slot.id)
                && remembered.get(slot.id) != Some(tool)
            {
                app.ui.remember_group_tool(*tool);
            }
        }
    }
    if out.edit_toolbar.as_ref().is_some_and(clicked) {
        open_drawer(app);
    }
    if app.ui.toolbar_drawer {
        show_drawer(app, ui, &ctx, &slots, edge, &out);
    }
}

/// The Edit Toolbar drawer beside the rail: its tool list, and the arrangement edits saved through `edit.toolbar`.
fn show_drawer(
    app: &mut PhotocraftApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    slots: &[ToolSlot<Tool>],
    edge: PaneEdge,
    rail: &craft::ui::rails::ToolRailOutput<Tool>,
) {
    // Every tool is listed in toolbar order; the drawer dims the ones the toolbar holds.
    let mut all: Vec<Tool> = Tool::ALL.to_vec();
    all.sort_by_key(|tool| toolbar_position(*tool));
    let groups = [DrawerGroup::new(tl!("All Tools").to_string(), all.into_iter().map(tool_commands::rail_tool).collect())];
    let mut drawer = ToolbarDrawer::new(rail.response.rect, &groups, slots, rail.dropped).edge(edge);
    if let Some(opener) = &rail.edit_toolbar {
        drawer = drawer.opened_by(opener);
    }
    let out = drawer.show(ui);
    if let Some(tool) = out.select {
        crate::tool_memory::select(app, tool);
    }
    if !out.edits.is_empty() {
        // `DropAt::NewSlot { before }` counts the slots as the rail was given them, the same list the edits apply to.
        let defaults = tool_commands::slots(&ToolbarCustomization::default());
        let mut arranged = slots.to_vec();
        for edit in &out.edits {
            arranged = apply_toolbar_edit(&arranged, edit, &defaults);
        }
        let params =
            if matches!(out.edits.last(), Some(ToolbarEdit::Reset)) { json!({"reset": true}) } else { json!({"slots": tool_commands::saved_slots(&arranged)}) };
        if let Err(e) = crate::menus::invoke(app, ctx, "edit.toolbar", params) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
    }
    if out.closed {
        app.ui.toolbar_drawer = false;
    }
}

<<<<<<< HEAD
/// Photoshop's foreground / background colour chips: `SwatchPair` in host-picker mode, the
/// foreground in front, Default Colors (D) and Switch Colors (X) over them, centred in the tool
/// column.
fn chip_pair<'a>(app: &PhotocraftApp, active: &'a mut Well) -> SwatchPair<'a> {
    let tools = &app.session.tools;
    let wells = [
        PairWell::new(tl!("Set foreground color"), crate::panels::color::srgba(tools.foreground)),
        PairWell::new(tl!("Set background color"), crate::panels::color::srgba(tools.background)),
    ];
    SwatchPair::new(tl!("Colors"), wells, Bind::bound(active))
        .popover(false)
        .click_activates(false)
        .default_button(tl!("Default Foreground and Background Colors (D)"))
        .swap_button(tl!("Switch Foreground and Background Colors (X)"))
}

/// The colour chips: a click on a chip opens the Color Picker for that colour (Photoshop).
pub(super) fn color_chips(app: &mut PhotocraftApp, ui: &mut egui::Ui, column: f32) {
    let mut active = Well::Primary;
    let layout = egui::Layout::top_down(egui::Align::Center);
    let out = ui.allocate_ui_with_layout(vec2(column, 0.0), layout, |ui| chip_pair(app, &mut active).show(ui)).inner;
    match out.clicked {
        Some(Well::Secondary) => {
            crate::color_picker_ui::open(app, "background");
=======
/// The tool palette panel, on the side `panels.toolbar_side` resolves to, with its width
/// toggle and Move Toolbar header buttons.
pub fn toolbar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let (_, bx) = toolbar_columns(t.pro);
    let m = if t.pro { 5i8 } else { 7i8 };
    let sections = layout_sections(&app.session.prefs().toolbar);
    // Two columns when the header chevron asks for them; a column that doesn't fit scrolls.
    let double = app.ui.panels.toolbar_double;
    let w = toolbar_width(t.pro, double);
    let side = app.ui.panels.toolbar_side.resolve();
    let panel = if side == ToolbarSide::Right { egui::Panel::right("toolbar") } else { egui::Panel::left("toolbar") };
    panel.resizable(false).exact_size(w).frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(m, 8))).show(ui, |ui| {
        // Toolbar width is a user preference, not a Pro-only feature. The two header buttons share
        // the narrow toolbar column (14-point targets in every theme, including RTL layouts), so
        // their tooltips carry the full names.
        ui.allocate_ui_with_layout(vec2(bx, 14.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let (cr, resp) = ui.allocate_exact_size(vec2(bx.min(14.0), 14.0), Sense::click());
            let (shrink, grow) = if side == ToolbarSide::Right { ("chevrons-right", "chevrons-left") } else { ("chevrons-left", "chevrons-right") };
            icons::paint(ui, cr, if double { shrink } else { grow }, 11.0, if resp.hovered() { t.text } else { t.text_faint });
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("Toolbar")));
            if resp.clicked() {
                app.ui.panels.toolbar_double = !double;
            }
            let other = side.other();
            let (dock_tip, dock_icon) = match other {
                ToolbarSide::Right => (tl!("Move Toolbar to the Right"), "panel-right"),
                _ => (tl!("Move Toolbar to the Left"), "panel-left"),
            };
            let (dr, dock) = ui.allocate_exact_size(vec2((bx - 16.0).max(14.0), 14.0), Sense::click());
            icons::paint(ui, dr, dock_icon, 11.0, if dock.hovered() { t.text } else { t.text_faint });
            dock.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, dock_tip));
            if dock.on_hover_text(dock_tip).clicked() {
                app.ui.panels.toolbar_side = other;
                // Persist the side as a discrete change; the dock may not be drawn this frame.
                crate::dock::persist_panels(app, ui.ctx());
            }
        });
        ui.add_space(4.0);
        // Subtle violet wash at the bottom of the toolbar.
        let full = ui.max_rect();
        if !t.bevel && !t.pro && t.dark() {
            let mut mesh = egui::Mesh::default();
            let r = Rect::from_min_max(pos2(full.left() - 7.0, full.bottom() - 260.0), pos2(full.right() + 7.0, full.bottom() + 8.0));
            let top = Color32::TRANSPARENT;
            let bottom = Color32::from_rgba_unmultiplied(90, 70, 190, 34);
            mesh.colored_vertex(r.left_top(), top);
            mesh.colored_vertex(r.right_top(), top);
            mesh.colored_vertex(r.right_bottom(), bottom);
            mesh.colored_vertex(r.left_bottom(), bottom);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            ui.painter().add(mesh);
>>>>>>> upstream/main
        }
        Some(_) => {
            crate::color_picker_ui::open(app, "foreground");
        }
        None => {}
    }
    if out.swap {
        chip_command(app, "tools.swapColors");
    }
    if out.defaults {
        chip_command(app, "tools.defaultColors");
    }
}

/// Run Switch Colors or Default Colors from the chips. As in Photoshop, characters highlighted
/// in the type layer being edited take the new foreground colour, like picking one (#5690).
fn chip_command(app: &mut PhotocraftApp, id: &str) {
    if app.run(id, json!({})).is_ok() {
        crate::type_tool::foreground_changed(app);
    }
}
