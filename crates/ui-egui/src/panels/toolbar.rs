use super::{Color32, PhotocraftApp, Rect, Sense, StrokeKind, Tool, c32, icons, json, pos2, vec2};
use craft_ui::controls::IconButton;
use craft_ui::icons::Icon;
use craft_ui::patterns::{FlyoutButton, FlyoutItem, LastUsed, RailTool, ToolRail, ToolSlot};
use craft_ui::theme::{StrokeExt, ToEgui, Tokens, TokensOf};

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
    color_chips(app, ui, rail_fill);
    ui.add_space(craft_ui::theme::scale::Space::S8.px());
    let layout = match (columns > 1, crate::direction::is_rtl()) {
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

/// Colour chips: chip size and the background chip's offset (points).
const CHIP: (f32, f32) = (18.0, 10.0);

/// Height of the default-colours and swap icons over the chips.
const CHIP_ICON: f32 = 13.0;

/// Height of the colour chips block.
pub(super) fn chips_height() -> f32 {
    CHIP_ICON + 3.0 + CHIP.0 + CHIP.1
}

/// Photoshop's foreground / background colour chips: the Default Colors (D) and Switch Colors (X)
/// icons above them, the foreground chip over the background one, centred in `ui`'s width.
/// `rail_fill` is the gap between the chips.
pub(super) fn color_chips(app: &mut PhotocraftApp, ui: &mut egui::Ui, rail_fill: Color32) {
    let t = super::Tokens::get(ui.ctx());
    let (chip, step) = CHIP;
    let group = chip + step;
    let w = ui.available_width().max(group);
    let (rect, _) = ui.allocate_exact_size(vec2(w, chips_height()), Sense::hover());
    let left = (rect.left() + (w - group) / 2.0).round();
    let fg = Rect::from_min_size(pos2(left, rect.top() + CHIP_ICON + 3.0), vec2(chip, chip));
    let bg = fg.translate(vec2(step, step));
    let radius = t.radius_sm.min(3.0);
    let p = ui.painter();
    let frame = |r: Rect| {
        // A light line inside a dark one, so any colour reads against the toolbar.
        p.rect_stroke(r, radius, craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.text.gamma_multiply(0.9)), StrokeKind::Inside);
        p.rect_stroke(r, radius, craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.field_border), StrokeKind::Outside);
    };
    p.rect_filled(bg, radius, c32(app.session.tools.background));
    frame(bg);
    p.rect_filled(fg.expand(2.0), radius + 2.0, rail_fill);
    p.rect_filled(fg, radius, c32(app.session.tools.foreground));
    frame(fg);
    // Photoshop: clicking a chip opens the Color Picker for that colour.
    let bg_resp = ui.interact(bg, ui.scope_id().with("bgchip"), Sense::click());
    let fg_resp = ui.interact(fg, ui.scope_id().with("fgchip"), Sense::click());
    if fg_resp.on_hover_text(tl!("Set foreground color")).clicked() {
        crate::color_picker_ui::open(app, "foreground");
    } else if bg_resp.on_hover_text(tl!("Set background color")).clicked() {
        crate::color_picker_ui::open(app, "background");
    }
    // Default Colors at the left, Switch Colors at the right, over the chips.
    let icon = |x: f32| Rect::from_min_size(pos2(x, rect.top()), vec2(CHIP_ICON, CHIP_ICON));
    let (dr, sr) = (icon(left), icon(left + group - CHIP_ICON));
    let d = ui.interact(dr, ui.scope_id().with("default-colors"), Sense::click());
    let s = ui.interact(sr, ui.scope_id().with("swap-colors"), Sense::click());
    let p = ui.painter();
    for (r, resp) in [(dr, &d), (sr, &s)] {
        if resp.hovered() {
            p.rect_filled(r.expand(2.0), radius, t.hover);
        }
    }
    let ink = |resp: &egui::Response| if resp.hovered() { t.text } else { t.icon };
    // Switch Colors: the compact clockwise bend has left- and down-pointing heads.
    // swap_arrow.rs owns its geometry and the pixel-level render regression test.
    crate::swap_arrow::paint(p, sr.min, ink(&s));
    // Default Colors: a small black chip over a small white one.
    let small = 7.0;
    let (b, w) = (Rect::from_min_size(dr.min + vec2(1.0, 1.0), vec2(small, small)), Rect::from_min_size(dr.min + vec2(5.0, 5.0), vec2(small, small)));
    p.rect_filled(w, craft_ui::theme::scale::Radius::R2.px(), Color32::WHITE);
    p.rect_stroke(w, craft_ui::theme::scale::Radius::R2.px(), craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), ink(&d)), StrokeKind::Inside);
    p.rect_filled(b.expand(1.0), 1.5, rail_fill);
    p.rect_filled(b, craft_ui::theme::scale::Radius::R2.px(), Color32::BLACK);
    p.rect_stroke(b, craft_ui::theme::scale::Radius::R2.px(), craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), ink(&d)), StrokeKind::Inside);
    d.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("Default colours (D)")));
    s.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("Swap colours (X)")));
    if s.on_hover_text(tl!("Swap colours (X)")).clicked() {
        chip_command(app, "tools.swapColors");
    }
    if d.on_hover_text(tl!("Default colours (D)")).clicked() {
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
