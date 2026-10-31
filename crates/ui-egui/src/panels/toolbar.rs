use super::{Color32, PhotocraftApp, Tool, icons, json};
use craft_theme::Srgba;
use craft_ui::composites::{PairWell, SwatchPair, Well};
use craft_ui::controls::IconButton;
use craft_ui::icons::Icon;
use craft_ui::localized::LocaleNumbers;
use craft_ui::patterns::{FlyoutButton, FlyoutItem, LastUsed, RailTool, ToolRail, ToolSlot};
use craft_ui::theme::{ToEgui, Tokens, TokensOf};

// ----------------------------------------------------------------------------- toolbar

/// Photoshop's toolbar: sections of slots; each slot is a flyout group whose button shows the
/// group's last-used tool. The rail draws the slots as one column; a slot's id is its index in
/// the flattened list.
pub(super) const TOOL_SECTIONS: &[&[&[Tool]]] = &[
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
        &[Tool::Remove, Tool::SpotHealing, Tool::Healing, Tool::Patch, Tool::ContentAwareMove, Tool::RedEye],
        &[Tool::Brush, Tool::Pencil, Tool::MixerBrush, Tool::ColorReplacement],
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
        &[Tool::Rectangle, Tool::EllipseShape, Tool::Triangle, Tool::Polygon, Tool::Line, Tool::CustomShape],
    ],
    &[&[Tool::Hand, Tool::RotateView], &[Tool::Zoom]],
];

/// [`TOOL_SECTIONS`] as the rail's slots, each numbered by its index in the flattened list.
fn tool_slots(app: &PhotocraftApp) -> Vec<ToolSlot<Tool>> {
    TOOL_SECTIONS
        .iter()
        .flat_map(|section| section.iter())
        .enumerate()
        .map(|(index, slot)| {
            let tools = slot
                .iter()
                .map(|&tool| {
                    let item = RailTool::new(tool, icons::tool_library_icon(tool), tl!(tool.label()));
                    match crate::tool_shortcuts::shortcut(app, tool) {
                        Some(sc) => item.shortcut(crate::shortcuts::pretty(&sc)),
                        None => item,
                    }
                })
                .collect();
            ToolSlot::new(index as u32, tools)
        })
        .collect()
}

fn last_used_id() -> egui::Id {
    egui::Id::unique("toolbar-last-used")
}

/// Photoshop's toolbar on craft-ui's `ToolRail`: the tool slots with their flyouts, the column
/// toggle and Edit Toolbar, and under them the colour chips, Quick Mask and Screen Mode.
/// `edit.toolbar` takes the tools hidden in Edit > Toolbar as `Tool` names.
pub fn toolbar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let double = app.ui.panels.toolbar_double;
    let hidden: Vec<Tool> = app.session.prefs().toolbar.hidden.iter().filter_map(|name| Tool::from_name(name)).collect();
    let slots = tool_slots(app);
    let current = app.ui.tool;
    let mut last_used: LastUsed<Tool> = ui.data(|d| d.get_temp(last_used_id())).unwrap_or_default();
    let out = ToolRail::new("toolbar")
        .columns(if double { 2 } else { 1 })
        .column_toggle(tl!("Toolbar"))
        .edit_toolbar(tl!("Edit Toolbar…"))
        .hidden(&hidden)
        .label(tl!("Tools"))
        .show(ui, &slots, &current, &mut last_used, |ui, footer| footer_controls(app, ui, footer.columns));
    ui.data_mut(|d| d.insert_temp(last_used_id(), last_used));
    if let Some(tool) = out.select {
        crate::tool_memory::select(app, tool);
    }
    // Double-clicking the Hand tool fits the image on screen, Zoom shows it at 100% (Photoshop).
    // With no document open there is nothing to fit.
    match out.double_clicked {
        Some(tool @ Tool::Hand) => {
            crate::tool_memory::select(app, tool);
            let _ = app.run("view.fitOnScreen", json!({}));
        }
        Some(tool @ Tool::Zoom) => {
            crate::tool_memory::select(app, tool);
            let _ = app.run("view.actualPixels", json!({}));
        }
        _ => {}
    }
    if out.toggle_columns {
        app.ui.panels.toolbar_double = !double;
    }
    if out.edit_toolbar {
        let ctx = ui.ctx().clone();
        let _ = crate::menus::invoke(app, &ctx, "edit.toolbar", json!({}));
    }
}

/// The rail's footer: the colour chips, then Quick Mask and Screen Mode, side by side in two
/// columns.
fn footer_controls(app: &mut PhotocraftApp, ui: &mut egui::Ui, columns: usize) {
    let ctx = ui.ctx().clone();
    let tokens = Tokens::of(ui);
    let button = FlyoutButton::<&str>::style(&tokens).icon_button;
    let rail_fill = tokens.component.rail.fill.to_egui();
    drop(tokens);
    ui.add_space(craft_ui::theme::scale::Space::S8.px());
    color_pair(app, ui, rail_fill);
    ui.add_space(craft_ui::theme::scale::Space::S8.px());
    let layout = match (columns > 1, craft_ui::direction::is_rtl(&ctx)) {
        (false, _) => egui::Layout::top_down(egui::Align::Min),
        (true, false) => egui::Layout::left_to_right(egui::Align::Min),
        (true, true) => egui::Layout::right_to_left(egui::Align::Min),
    };
    ui.with_layout(layout, |ui| {
        let quick_mask = app.session.active().is_some_and(|s| s.doc.quick_mask.is_some());
        let label = if quick_mask { tl!("Edit in Standard Mode  (Q)") } else { tl!("Edit in Quick Mask Mode  (Q)") };
        if ui.add(IconButton::new(Icon::MarqueeRect, label).with_style(button).selected(quick_mask)).clicked() {
            let _ = crate::menus::invoke(app, &ctx, "select.editInQuickMaskMode", json!({}));
        }
        screen_mode_button(app, ui);
    });
}

/// The Screen Mode button: a click cycles the mode, a right click or a long press lists the three
/// modes with the current one marked.
fn screen_mode_button(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let modes = [
        ("view.screenMode.standard", Icon::AppWindow, tl!("Standard Screen Mode")),
        ("view.screenMode.fullScreenWithMenuBar", Icon::Monitor, tl!("Full Screen Mode With Menu Bar")),
        ("view.screenMode.fullScreen", Icon::Maximize2, tl!("Full Screen Mode")),
    ];
    let items: Vec<FlyoutItem<&str>> = modes.iter().map(|&(id, icon, label)| FlyoutItem::new(id, icon, label)).collect();
    let current = modes.iter().map(|m| m.0).find(|id| crate::view_cmds::checked(app, id).unwrap_or(false)).unwrap_or(modes[0].0);
    let out = FlyoutButton::new("screen-mode", Icon::AppWindow, tl!("Change Screen Mode  (F)")).items(&items).current(&current).hold_selects(false).show(ui);
    if let Some(id) = out.chosen {
        let _ = crate::menus::invoke(app, &ctx, id, json!({}));
    } else if out.clicked {
        let _ = crate::menus::invoke(app, &ctx, "view.screenMode.cycle", json!({}));
    }
}

/// Photoshop's foreground / background colour chips on craft-ui's `SwatchPair`: the Default Colors
/// (D) and Switch Colors (X) icons above them. A click on a chip opens PhotoCraft's Color Picker
/// for that colour; the foreground stays in front. `rail_fill` is the gap between the chips.
pub(super) fn color_pair(app: &mut PhotocraftApp, ui: &mut egui::Ui, rail_fill: Color32) {
    let numbers = LocaleNumbers::of(ui);
    let srgba = |c: [f32; 4]| Srgba::new(c[0], c[1], c[2], c[3]);
    let tools = &app.session.tools;
    let wells = [PairWell::new(tl!("Set foreground color"), srgba(tools.foreground)), PairWell::new(tl!("Set background color"), srgba(tools.background))];
    let out = SwatchPair::new("color-chips", tl!("Colors"), wells, &numbers)
        .popover(false)
        .click_activates(false)
        .active(Well::Primary)
        .backdrop(rail_fill)
        .swap_button(tl!("Swap colours (X)"))
        .default_button(tl!("Default colours (D)"))
        .show(ui);
    match out.clicked {
        Some(Well::Primary) => {
            let _ = crate::color_picker_ui::open(app, "foreground");
        }
        Some(Well::Secondary) => {
            let _ = crate::color_picker_ui::open(app, "background");
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
