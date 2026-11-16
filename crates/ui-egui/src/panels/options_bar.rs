//! The options bar: the framework's `Bar` with the Home button, the active tool's icon and the members of the active
//! tool. The tools' own members live with their tools; this file orders them and hosts the bar's commit and cancel
//! buttons.

use std::ops::RangeInclusive;

use craft::behavior::{EditEvents, IconId};
use craft::ui::bars::{Bar, BarHome, BarKind, BarOutput, BarTool};
use craft::ui::choice::Checkbox;
use craft::ui::contract::{Bind, Entry, Props as _, Show as _, ShowContainer as _};
use craft::ui::display::Divider;
use craft::ui::form::Field;
use craft::ui::menu::Menu;
use craft::ui::runtime::env::{Label, WheelScope, with_env};
use craft::ui::text::NumberField;
use photocraft_paint::BrushSettings;

use super::{PhotocraftApp, Tool, Value, icons, json};
use paint::PaintMode;

pub(crate) mod fill;
mod navigate;
mod paint;
pub(crate) mod selection;

/// The right-click menu of the tool icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Reset {
    /// Reset Tool.
    Tool,
    /// Reset All Tools.
    All,
}

impl Reset {
    /// The menu command the row runs.
    fn command(self) -> &'static str {
        match self {
            Reset::Tool => "tool.reset",
            Reset::All => "tool.resetAll",
        }
    }
}

/// What the bar's commit and cancel buttons end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum End {
    /// The Transform Symmetry session.
    Symmetry,
    /// The live crop.
    Crop,
    /// The Perspective Crop's pending crop.
    PerspectiveCrop,
    /// The Remove or Content-Aware Move edit that waits for an answer.
    Retouch,
    /// The text being typed.
    Type,
}

/// The commit and cancel buttons of the bar and what they end.
struct Ending {
    end: End,
    commit: Label,
    cancel: Label,
}

/// Whether `tool` is one of the four type tools.
fn is_type_tool(tool: Tool) -> bool {
    matches!(tool, Tool::Type | Tool::VerticalType | Tool::HorizontalTypeMask | Tool::VerticalTypeMask)
}

/// The edit the bar's commit and cancel buttons belong to, if one is waiting. A bar carries at most one pair.
fn ending(app: &PhotocraftApp, tool: Tool) -> Option<Ending> {
    if app.ui.transform.is_some() {
        return None;
    }
    let pair = |end, commit: &str, cancel: &str| Some(Ending { end, commit: commit.to_owned().into(), cancel: cancel.to_owned().into() });
    if crate::symmetry_ui::is_open(app) {
        return pair(End::Symmetry, tl!("Commit"), tl!("Cancel"));
    }
    match tool {
        Tool::Crop if crate::crop_ui::bar::commits_shown(app) => pair(End::Crop, tl!("Commit current crop operation"), tl!("Cancel current crop operation")),
        Tool::PerspectiveCrop if crate::perspective_crop_ui::commit_shown(app) => pair(End::PerspectiveCrop, tl!("Commit"), tl!("Cancel")),
        tool if is_type_tool(tool) && app.ui.text_edit.is_some() => pair(End::Type, tl!("Commit any current edits"), tl!("Cancel any current edits")),
        tool => crate::retouch_ui::bar::pending(app, tool).map(|p| Ending { end: End::Retouch, commit: Label::from(p.commit), cancel: Label::from(p.cancel) }),
    }
}

/// The options bar of the active tool. It is the same arrangement in every theme.
///
/// Each pass applies the commit or cancel its own bar took; a second pass of a frame applies none of the first pass's.
pub fn options_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let tool = app.ui.tool;
    let ending = ending(app, tool);
    let mut bar = Bar::new(tl!("Options").to_owned(), BarKind::Options);
    if let Some(ending) = &ending {
        bar = bar.commit_cancel(ending.commit.clone(), ending.cancel.clone());
    }
    // A value control of the bar takes the wheel without a click, as the Mode dropdown and the fields did.
    let shown = bar.show(ui, |ui, _| with_env(ui, |set| set.wheel = Some(WheelScope::Free), |ui| body(app, ui)));
    if let Some(ending) = ending {
        answer(app, ui, tool, ending.end, &shown.output);
    }
}

/// Runs the ending the bar's commit or cancel button asked for.
fn answer(app: &mut PhotocraftApp, ui: &egui::Ui, tool: Tool, end: End, out: &BarOutput) {
    match end {
        End::Symmetry => {
            if out.committed {
                crate::symmetry_ui::commit(app);
            } else if out.cancelled {
                crate::symmetry_ui::cancel(app);
            }
        }
        End::Crop => {
            if out.committed || out.cancelled {
                crate::crop_ui::bar::committed(app, out.committed);
            }
        }
        End::PerspectiveCrop => crate::perspective_crop_ui::committed(app, out),
        End::Retouch => crate::retouch_ui::bar::answer(app, tool, out.committed, out.cancelled),
        End::Type => {
            if out.committed {
                let mods = ui.input(|i| i.modifiers);
                crate::type_tool::commit_with(app, mods);
            } else if out.cancelled {
                crate::type_tool::cancel(app);
            }
        }
    }
}

/// The bar's members in order: Home, the tool icon, then the active tool's options.
fn body(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    lead(app, ui);
    if app.ui.transform.is_some() {
        crate::transform_tool::options_bar(app, ui);
        return;
    }
    if crate::symmetry_ui::is_open(app) {
        crate::symmetry_ui::bar_members(ui);
        return;
    }
    if crate::distort_ui::options_bar(app, ui) {
        return;
    }
    let tool = app.ui.tool;
    // The bar shows the active tool's brush: switch it in before drawing, so a tool change swaps the brush with it
    // (Photoshop keeps one per tool).
    crate::paint_mouse::sync_tool_brush(app);
    if (tool.is_brushlike() && !matches!(tool, Tool::Brush | Tool::Pencil | Tool::MixerBrush | Tool::Eraser)) || tool == Tool::QuickSelection {
        crate::brush_picker::show_field(ui, app, true);
        crate::brush_picker::settings_toggle(ui, app);
        Divider::vertical().show(ui);
    }
    if matches!(tool, Tool::CloneStamp | Tool::PatternStamp | Tool::HistoryBrush | Tool::Healing | Tool::ArtHistoryBrush) {
        stamp_members(app, ui, tool);
        Divider::vertical().show(ui);
    }
    if crate::eraser_ui::options_bar(ui, app, tool)
        || crate::retouch_ui::bar::options_bar(ui, app, tool)
        || crate::vector_ui::options_bar(app, ui, tool)
        || crate::analysis::bars::options(app, ui, tool)
        || crate::slice_ui::options_bar(ui, app, tool)
        || crate::frame_tool::options_bar(app, ui, tool)
        || crate::artboard_tool::options_bar(app, ui, tool)
    {
        return;
    }
<<<<<<< HEAD
    match tool {
        Tool::Crop => crate::crop_ui::bar::bar(app, ui),
        Tool::PerspectiveCrop => crate::perspective_crop_ui::options(app, ui),
        Tool::Gradient => crate::gradient_ui::bar(app, ui),
        tool if is_type_tool(tool) => crate::type_tool::options_bar(app, ui),
=======
}

// ----------------------------------------------------------------------------- options bar

/// Options bar height: Preferences ▸ Workspace ▸ Enable Narrow Options Bar trims it, as in
/// Photoshop, leaving the document more room.
pub(super) fn options_bar_height(pro: bool, narrow: bool) -> f32 {
    match (pro, narrow) {
        (true, false) => 36.0,
        (true, true) => 30.0,
        (false, false) => 42.0,
        (false, true) => 34.0,
    }
}

pub fn options_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    crate::vector_ui::track_arrow_default_selection(app, ui.ctx());
    crate::eraser_ui::alert_for_a_field_gone(app, ui.ctx());
    let t = Tokens::get(ui.ctx());
    let narrow = app.session.prefs().workspace.narrow_options_bar();
    egui::Panel::top("options_bar")
        .exact_size(options_bar_height(t.pro, narrow))
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(10, 0)))
        .show(ui, |ui| {
            ui.style_mut().spacing.scroll = egui::style::ScrollStyle { bar_width: 4.0, ..egui::style::ScrollStyle::floating() };
            egui::ScrollArea::horizontal().id_salt("tool-options").auto_shrink([false, false]).show(ui, |ui| {
                // Font ink can extend beyond its advance width at fractional positions.
                // Include its gutter in the container layout, including right-aligned actions.
                egui::Frame::NONE.inner_margin(egui::Margin { right: 2, ..Default::default() }).show(ui, |ui| {
                    crate::direction::horizontal_centered(ui, |ui| {
                        if t.pro {
                            crate::chrome_ui::home_button(app, ui);
                            widgets::vline(ui, 22.0);
                        }
                        // The tool preset picker is greyed while warping, as in the reference (#12503).
                        let warping = app.ui.transform.as_ref().is_some_and(|s| s.warp.is_some());
                        let tool_icon = ui
                            .add_enabled_ui(!warping, |ui| {
                                icons::button(ui, icons::tool_icon(app.ui.tool), if t.pro { 26.0 } else { 28.0 }, !t.pro, tl!(app.ui.tool.label()))
                            })
                            .inner;
                        // Photoshop: right-click the tool icon for Reset Tool / Reset All Tools (#2825).
                        let mut reset = None;
                        tool_icon.context_menu(|ui| {
                            if ui.button(tl!("Reset Tool")).clicked() {
                                reset = Some("tool.reset");
                                ui.close();
                            }
                            if ui.button(tl!("Reset All Tools")).clicked() {
                                reset = Some("tool.resetAll");
                                ui.close();
                            }
                        });
                        if let Some(id) = reset
                            && let Err(e) = crate::menus::invoke(app, ui.ctx(), id, json!({}))
                        {
                            app.ui.status = e;
                        }
                        if t.pro {
                            let (r, _) = ui.allocate_exact_size(vec2(10.0, 20.0), Sense::hover());
                            icons::paint(ui, r, "chevron-down", 10.0, t.text_faint);
                        }
                        widgets::vline(ui, 22.0);
                        if app.ui.transform.is_some() {
                            crate::transform_tool::options_bar(app, ui);
                            return;
                        }
                        if crate::symmetry_ui::options_bar(app, ui) || crate::distort_ui::options_bar(app, ui) {
                            return;
                        }
                        let tool = app.ui.tool;
                        // The options bar shows the active tool's brush: switch it in before drawing the
                        // chip, so a tool change swaps the brush with it (Photoshop keeps one per tool).
                        crate::paint_mouse::sync_tool_brush(app);
                        // Brush edits here go through `tools.setBrush`, one journal entry per gesture (Rule 1).
                        if (tool.is_brushlike() && !matches!(tool, Tool::Brush | Tool::Pencil | Tool::MixerBrush | Tool::Eraser))
                            || tool == Tool::QuickSelection
                        {
                            brush_preset_chip(ui, &app.session.tools.brush, &mut app.ui);
                            crate::brush_picker::settings_toggle(app, ui);
                            widgets::vline(ui, 22.0);
                        }
                        if matches!(tool, Tool::CloneStamp | Tool::PatternStamp | Tool::HistoryBrush | Tool::Healing | Tool::ArtHistoryBrush) {
                            let before = app.session.tools.brush.clone();
                            let mut brush = before.clone();
                            // Mode is the tool's own brush's, kept per tool like its size.
                            if tool == Tool::PatternStamp {
                                // Its list has Behind, greyed on the Background (#12879).
                                opt_label(ui, tl!("Mode"));
                                let behind = crate::brush_mode_menu::enabled(app, crate::brush_mode_menu::Item::Behind);
                                // An Indexed Color document names Normal "Threshold" (#12887); the other
                                // entries weren't measured there.
                                let threshold = app.session.active().is_some_and(|st| st.doc.mode == photocraft_doc::ColorMode::Indexed);
                                super::brush_controls::pattern_stamp_mode_dropdown(ui, "retouch-mode", &mut brush, behind, threshold);
                            } else if let Some(modes) = crate::blend_keys::tool_modes(tool) {
                                opt_label(ui, tl!("Mode"));
                                let mode = crate::blend_keys::brush_tool_mode(tool, brush.mode);
                                // Indexed Color names Normal "Threshold"; the other modes weren't measured.
                                let threshold = crate::retouch_ui::clone_stamp_indexed(app);
                                if let Some(Some(m)) = tool_mode_dropdown(ui, "retouch-mode", &modes, Some(mode), threshold) {
                                    brush.mode = m;
                                }
                            }
                            if matches!(tool, Tool::CloneStamp | Tool::PatternStamp | Tool::HistoryBrush) {
                                let width = if t.pro { 62.0 } else { 66.0 };
                                // The Pattern Stamp's are whole percents as measured; the Brush's tooltips (#12616).
                                let pattern = tool == Tool::PatternStamp;
                                let field = if pattern { whole_percent_field } else { percent_field };
                                let opacity = field(ui, tl!("Opacity"), &mut brush.opacity, 1.0..=100.0, width);
                                let flow = field(ui, tl!("Flow"), &mut brush.flow, 1.0..=100.0, width);
                                if pattern {
                                    opacity.on_hover_text(tl!("Set opacity for stroke"));
                                    flow.on_hover_text(tl!("Set flow rate for stroke"));
                                }
                            }
                            // Art History has no Mode (the engine paints Normal) and no Flow.
                            if tool == Tool::ArtHistoryBrush {
                                let width = if t.pro { 62.0 } else { 66.0 };
                                percent_field(ui, tl!("Opacity"), &mut brush.opacity, 1.0..=100.0, width);
                            }
                            crate::brush_panel::commit_gesture(app, ui.ctx(), &before, &brush);
                            widgets::vline(ui, 22.0);
                        }
                        if crate::eraser_ui::options_bar(app, ui, tool)
                            || crate::retouch_ui::options_bar(app, ui, tool)
                            || crate::vector_ui::options_bar(app, ui, tool)
                            || crate::analysis_ui::options_bar(app, ui, tool)
                            || crate::slice_ui::options_bar(app, ui, tool)
                            || crate::frame_tool::options_bar(app, ui, tool)
                            || crate::artboard_tool::options_bar(app, ui, tool)
                        {
                            return;
                        }
                        let brush_before = app.session.tools.brush.clone();
                        let mut brush = brush_before.clone();
                        let b = &mut brush;
                        tool_specific_options(app, ui, t, tool, b);
                        // Painting symmetry is independent of visual theme. Keep the options-bar
                        // action available for Brush, Pencil and Eraser in every palette (#2659).
                        if matches!(tool, Tool::Brush | Tool::Pencil | Tool::Eraser) {
                            crate::symmetry_ui::menu(app, ui);
                        }
                        crate::brush_panel::commit_gesture(app, ui.ctx(), &brush_before, &brush);
                    });
                    reveal_focused_control(ui);
                });
            });
        });
}

/// The per-tool options of the bar: the brush, mixer, marquee, pencil and view controls.
/// Brush family options share the same per-tool settings model.
fn brush_specific_options(app: &mut PhotocraftApp, ui: &mut egui::Ui, t: Tokens, b: &mut BrushSettings) {
    match app.ui.tool {
        Tool::Brush | Tool::Eraser if t.pro => {
            // A Block-mode Eraser has a fixed size, opacity and flow (Photoshop, #2770).
            let block = crate::eraser_ui::block_mode(app, app.ui.tool);
            // The Brush's own tooltips (#11813); the Eraser keeps its texts.
            let brush = app.ui.tool == Tool::Brush;
            let tip = |r: egui::Response, text: &str| if brush { r.on_hover_text(text) } else { r };
            ui.add_enabled_ui(!block, |ui| {
                brush_preset_chip(ui, b, &mut app.ui);
                crate::brush_picker::settings_toggle(app, ui);
            });
            widgets::vline(ui, 22.0);
            // The Eraser has no blend mode: its Mode is Brush, Pencil or Block (#2662).
            if app.ui.tool == Tool::Eraser {
                crate::eraser_ui::mode_dropdown(ui, &mut app.ui.tool_options.eraser_mode);
            } else {
                opt_label(ui, tl!("Mode"));
                let held = app.held_erase;
                paint_mode_dropdown(ui, "brush-mode", b, &mut app.ui.tool_options.brush_clear, held);
                // The dropdown's button, by the id its combo box takes from "brush-mode".
                if let Some(mode) = ui.ctx().read_response(ui.make_persistent_id(egui::IdSalt::new("brush-mode"))) {
                    tip(mode, tl!("Painting Mode"));
                }
            }
            ui.add_enabled_ui(!block, |ui| {
                // A whole number from 1 to 100, as in Photoshop: 0 paints nothing (#11812). The
                // Eraser's says so when a typed value was past that (#11938).
                if brush {
                    tip(whole_percent_field(ui, tl!("Opacity"), &mut b.opacity, 1.0..=100.0, 62.0), tl!("Set opacity for stroke"));
                } else if eraser_percent(ui, tl!("Opacity"), &mut b.opacity) {
                    range_alert(&mut app.ui, ONE_TO_100);
                }
                let pressure = pressure_button(ui, b.pressure_opacity, tl!("Always use pressure for opacity"), brush);
                if tip(pressure, tl!("Always use Pressure for Opacity. When off, Brush Preset controls pressure")).clicked() {
                    b.pressure_opacity = !b.pressure_opacity;
                }
            });
            // A Pencil- or Block-mode Eraser is always full flow, without build-up (Photoshop).
            ui.add_enabled_ui(!block && !crate::eraser_ui::pencil_mode(app, app.ui.tool), |ui| {
                if brush {
                    tip(percent_field(ui, tl!("Flow"), &mut b.flow, 1.0..=100.0, 62.0), tl!("Set flow rate for stroke"));
                } else if eraser_percent(ui, tl!("Flow"), &mut b.flow) {
                    range_alert(&mut app.ui, ONE_TO_100);
                }
                let airbrush = icons::button(ui, "sparkles", 24.0, b.build_up, tl!("Enable airbrush-style build-up effects"));
                if crate::brush_picker::named(airbrush, "Enable airbrush-style build-up effects").clicked() {
                    b.build_up = !b.build_up;
                }
            });
            opt_label(ui, tl!("Smoothing"));
            if brush {
                tip(smoothing_field(ui, b, 58.0), tl!("Set smoothing for stroke\nUse higher values to reduce the shakiness of brush strokes"));
            } else if super::brush_controls::smoothing_field_alerting(ui, b, 58.0).1 {
                range_alert(&mut app.ui, ZERO_TO_100);
            }
            smoothing_options(ui, b, brush);
            if app.ui.tool == Tool::Eraser {
                widgets::checkbox(ui, &mut app.ui.tool_options.erase_to_history, tl!("Erase to History"));
            }
            widgets::vline(ui, 22.0);
            let pressure = pressure_button(ui, b.pressure_size, tl!("Always use pressure for size"), brush);
            if tip(pressure, tl!("Always use Pressure for Size. When off, Brush Preset controls pressure")).clicked() {
                b.pressure_size = !b.pressure_size;
            }
        }
        // Pencil: Photoshop's options (no hardness or flow: the pencil is always hard).
        Tool::Pencil => {
            brush_preset_chip(ui, b, &mut app.ui);
            crate::brush_picker::settings_toggle(app, ui);
            widgets::vline(ui, 22.0);
            if !t.pro {
                opt_label(ui, tl!("Size"));
                widgets::value_field(ui, &mut b.size, 1.0..=photocraft_engine::paint::MAX_BRUSH_SIZE, "px", 76.0);
                widgets::vline(ui, 22.0);
            }
            opt_label(ui, tl!("Mode"));
            let held = app.held_erase;
            paint_mode_dropdown(ui, "pencil-mode", b, &mut app.ui.tool_options.pencil_clear, held);
            whole_percent_field(ui, tl!("Opacity"), &mut b.opacity, 1.0..=100.0, if t.pro { 62.0 } else { 66.0 });
            if t.pro {
                // Between Opacity and Smoothing, as measured (#11962).
                let pressure = pressure_button(ui, b.pressure_opacity, tl!("Always use pressure for opacity"), true);
                if pressure.on_hover_text(tl!("Always use Pressure for Opacity. When off, Brush Preset controls pressure")).clicked() {
                    b.pressure_opacity = !b.pressure_opacity;
                }
            }
            opt_label(ui, tl!("Smoothing"));
            smoothing_field(ui, b, if t.pro { 58.0 } else { 66.0 });
            widgets::vline(ui, 22.0);
            widgets::checkbox(ui, &mut app.ui.tool_options.pencil_auto_erase, tl!("Auto Erase"))
                .on_hover_text(tl!("Draws background color over foreground colors"));
            widgets::vline(ui, 22.0);
            if t.pro {
                // Between Auto Erase and the Symmetry button the bar adds after this arm (#11962).
                let pressure = pressure_button(ui, b.pressure_size, tl!("Always use pressure for size"), true);
                if pressure.on_hover_text(tl!("Always use Pressure for Size. When off, Brush Preset controls pressure")).clicked() {
                    b.pressure_size = !b.pressure_size;
                }
            } else {
                // The other themes show the Brush's labelled switches; the Pencil gets them too.
                widgets::toggle(ui, &mut b.pressure_size, tl!("Pressure for Size"))
                    .on_hover_text(tl!("Always use Pressure for Size. When off, Brush Preset controls pressure"));
                widgets::toggle(ui, &mut b.pressure_opacity, tl!("Pressure for Opacity"))
                    .on_hover_text(tl!("Always use Pressure for Opacity. When off, Brush Preset controls pressure"));
            }
        }
        Tool::Brush | Tool::Eraser => {
            brush_preset_chip(ui, b, &mut app.ui);
            crate::brush_picker::settings_toggle(app, ui);
            widgets::vline(ui, 22.0);
            // A Block-mode Eraser has a fixed size, opacity and flow (Photoshop, #2770).
            let block = crate::eraser_ui::block_mode(app, app.ui.tool);
            ui.add_enabled_ui(!block, |ui| {
                opt_label(ui, tl!("Size"));
                widgets::value_field(ui, &mut b.size, 1.0..=photocraft_engine::paint::MAX_BRUSH_SIZE, "px", 76.0);
            });
            // A Pencil-mode Eraser is always hard and at full flow (Photoshop).
            let fixed = block || crate::eraser_ui::pencil_mode(app, app.ui.tool);
            // Hardness stays with Size, so Opacity follows Mode as in Photoshop and the Pro bar (#9784).
            ui.add_enabled_ui(!fixed, |ui| percent_field(ui, tl!("Hardness"), &mut b.hardness, 0.0..=100.0, 66.0));
            widgets::vline(ui, 22.0);
            // The Brush's own tooltips here too (#11813); the Eraser keeps its texts.
            let brush = app.ui.tool == Tool::Brush;
            let tip = |r: egui::Response, text: &str| if brush { r.on_hover_text(text) } else { r };
            if app.ui.tool == Tool::Eraser {
                crate::eraser_ui::mode_dropdown(ui, &mut app.ui.tool_options.eraser_mode);
            } else {
                opt_label(ui, tl!("Mode"));
                let held = app.held_erase;
                paint_mode_dropdown(ui, "brush-mode", b, &mut app.ui.tool_options.brush_clear, held);
                if let Some(mode) = ui.ctx().read_response(ui.make_persistent_id(egui::IdSalt::new("brush-mode"))) {
                    tip(mode, tl!("Painting Mode"));
                }
            }
            let opacity = ui.add_enabled_ui(!block, |ui| whole_percent_field(ui, tl!("Opacity"), &mut b.opacity, 1.0..=100.0, 66.0)).inner;
            tip(opacity, tl!("Set opacity for stroke"));
            let flow = ui.add_enabled_ui(!fixed, |ui| percent_field(ui, tl!("Flow"), &mut b.flow, 1.0..=100.0, 66.0)).inner;
            tip(flow, tl!("Set flow rate for stroke"));
            opt_label(ui, tl!("Smoothing"));
            tip(smoothing_field(ui, b, 66.0), tl!("Set smoothing for stroke\nUse higher values to reduce the shakiness of brush strokes"));
            if app.ui.tool == Tool::Eraser {
                widgets::checkbox(ui, &mut app.ui.tool_options.erase_to_history, tl!("Erase to History"));
            }
            widgets::vline(ui, 22.0);
            tip(
                widgets::toggle(ui, &mut b.pressure_size, tl!("Pressure for Size")),
                tl!("Always use Pressure for Size. When off, Brush Preset controls pressure"),
            );
            tip(
                widgets::toggle(ui, &mut b.pressure_opacity, tl!("Pressure for Opacity")),
                tl!("Always use Pressure for Opacity. When off, Brush Preset controls pressure"),
            );
        }
        Tool::MixerBrush => {
            brush_preset_chip(ui, b, &mut app.ui);
            crate::brush_picker::settings_toggle(app, ui);
            widgets::vline(ui, 22.0);
            mixer_load_swatch(app, ui);
            let m = &mut b.mixer;
            for (icon, on, tip) in [
                ("brush", &mut m.load_after_stroke, tl!("Load the brush after each stroke")),
                ("brush-cleaning", &mut m.clean_after_stroke, tl!("Clean the brush after each stroke")),
            ] {
                if icons::button(ui, icon, 24.0, *on, tip).clicked() {
                    *on = !*on;
                }
            }
            if let Some(r) = mixer_combination_dropdown(ui, &mut app.ui.tool_options.mixer_combination, &mut b.mixer) {
                r.on_hover_text(tl!("Useful mixer brush combinations"));
            }
            widgets::vline(ui, 22.0);
            for (label, value) in
                [(tl!("Wet"), &mut b.mixer.wet), (tl!("Load"), &mut b.mixer.load), (tl!("Mix"), &mut b.mixer.mix), (tl!("Flow"), &mut b.mixer.flow)]
            {
                percent_field(ui, label, value, 0.0..=100.0, 62.0);
            }
            widgets::vline(ui, 22.0);
            widgets::checkbox(ui, &mut b.mixer.sample_all_layers, tl!("Sample All Layers"));
        }
>>>>>>> upstream/main
        _ => {}
    }
    let _ = paint::members(app, ui, tool) || selection::members(app, ui, tool);
    fill::members(app, ui);
    navigate::members(app, ui);
}

/// The Home button and the tool's icon with its Reset Tool and Reset All Tools menu.
fn lead(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let documents = app.session.documents().len();
    let auto = app.session.prefs().general.auto_show_home_screen;
    let on = app.ui.chrome.shows_home(documents, auto);
    // With no documents and auto-show on, Home can't be dismissed (there is nothing behind it).
    let home = BarHome::new(tl!("Home").to_owned(), Bind::given(on)).enabled(documents > 0 || !auto).show(ui);
    if home.events.value().is_some() {
        app.ui.chrome.home = if on { None } else { Some(documents) };
    }
    // The tool picker is greyed while warping, as in the reference.
    let warping = app.ui.transform.as_ref().is_some_and(|t| t.session.warp().is_some());
    let tool = BarTool::new(icons::tool_icon(app.ui.tool), tl!(app.ui.tool.label()).to_owned()).enabled(!warping).show(ui);
    // Photoshop: right-click the tool icon for Reset Tool / Reset All Tools.
    let rows = [Entry::new(Reset::Tool, tl!("Reset Tool").to_owned()), Entry::new(Reset::All, tl!("Reset All Tools").to_owned())];
    if let Some(reset) = Menu::context(tl!("Tool").to_owned(), &tool.response).pick(ui, &rows).picked
        && let Err(e) = crate::menus::invoke(app, ui.ctx(), reset.command(), json!({}))
    {
        app.ui.status = e;
    }
}

<<<<<<< HEAD
/// The Mode, Opacity and Flow of the stamp tools, Healing and Art History: the tool's own brush's, kept per tool like its
/// size. Art History has no Mode (the engine paints Normal) and no Flow.
fn stamp_members(app: &mut PhotocraftApp, ui: &mut egui::Ui, tool: Tool) {
    let brush = app.session.tools.brush.clone();
    // Pattern Stamp's list has Behind, greyed on the Background.
    let entries = fill::mode_entries(tool, crate::brush_mode_menu::enabled(app, crate::brush_mode_menu::Item::Behind));
    if !entries.is_empty() {
        let current =
            if tool == Tool::PatternStamp && brush.behind { PaintMode::Behind } else { PaintMode::Blend(crate::blend_keys::brush_tool_mode(tool, brush.mode)) };
        let picked = labelled(ui, tl!("Mode"), |ui| fill::mode_select(Bind::given(current), &entries).show(ui));
        if let Some(mode) = picked.and_then(|shown| shown.events.value().cloned()) {
            let mut after = brush.clone();
            match mode {
                PaintMode::Blend(blend) => {
                    after.mode = blend;
                    if tool == Tool::PatternStamp {
                        after.behind = false;
=======
/// Selection and paint tools expose their controls as a family of options rows.
fn selection_specific_options(app: &mut PhotocraftApp, ui: &mut egui::Ui, tool: Tool) {
    match app.ui.tool {
        Tool::RectMarquee | Tool::EllipseMarquee | Tool::SingleRowMarquee | Tool::SingleColumnMarquee => {
            marquee_options(app, ui);
        }
        Tool::Lasso | Tool::PolygonLasso | Tool::MagneticLasso => {
            lasso_options(app, ui);
        }
        Tool::MagicWand => {
            selection_mode_buttons(&mut app.ui.selection_mode, ui);
            widgets::vline(ui, 22.0);
            opt_label(ui, tl!("Tolerance"));
            // The wand reads a whole-number tolerance (#11786): the field rounds a typed decimal
            // and never keeps a fraction, so it shows the number the wand uses.
            widgets::value_field_int(ui, &mut app.ui.tool_options.tolerance, 0.0..=255.0, "", 58.0);
            widgets::checkbox(ui, &mut app.ui.tool_options.anti_alias, tl!("Anti-alias"));
            widgets::checkbox(ui, &mut app.ui.tool_options.contiguous, tl!("Contiguous"));
            widgets::checkbox(ui, &mut app.ui.tool_options.sample_all.magic_wand, tl!("Sample All Layers"));
            widgets::vline(ui, 22.0);
            if widgets::secondary_button(ui, tl!("Select Subject"), 0.0).clicked() {
                let r = app.run("select.subject", json!({}));
                crate::retouch_ui::report_smart_result(app, r, tl!("No subject found — try Object Selection or the Lasso"));
            }
            select_and_mask_button(app, ui);
        }
        Tool::PaintBucket => {
            opt_label(ui, tl!("Fill"));
            let mut src = u8::from(app.ui.tool_options.bucket_fill_pattern);
            if widgets::dropdown(ui, "bucket-src", tl!("Source"), &mut src, &[(0u8, tl!("Foreground")), (1, tl!("Pattern"))], 100.0) {
                app.ui.tool_options.bucket_fill_pattern = src == 1;
            }
            opt_label(ui, tl!("Mode"));
            if let Some(modes) = crate::blend_keys::tool_modes(tool) {
                let o = &mut app.ui.tool_options;
                let current = if o.bucket_clear { None } else { Some(o.bucket_blend_mode) };
                if let Some(m) = tool_mode_dropdown(ui, "bucket-mode", &modes, current, false) {
                    o.bucket_clear = m.is_none();
                    if let Some(m) = m {
                        o.bucket_blend_mode = m;
>>>>>>> upstream/main
                    }
                }
                PaintMode::Behind => after.behind = true,
                PaintMode::Clear => {}
            }
            // A pick is one step of its own.
            crate::brush_panel::commit_keyed(app, &brush, &after, None);
        }
    }
    if matches!(tool, Tool::CloneStamp | Tool::HistoryBrush | Tool::ArtHistoryBrush) {
        let opacity = percent_field(ui, tl!("Opacity"), brush.opacity, 1.0..=100.0);
        edit_brush(app, &opacity, |b, v| b.opacity = v);
    }
    if matches!(tool, Tool::CloneStamp | Tool::HistoryBrush) {
        let flow = percent_field(ui, tl!("Flow"), brush.flow, 1.0..=100.0);
        edit_brush(app, &flow, |b, v| b.flow = v);
    }
}

/// Sends each event of a brush field to the engine, a scrub as one history step (`history_keys::coalesce`). The value
/// each event carries goes onto the brush as it is now, so events of one pass apply in order.
fn edit_brush(app: &mut PhotocraftApp, events: &EditEvents<f32>, set: impl Fn(&mut BrushSettings, f32)) {
    for event in events.iter() {
        let Some(value) = event.landed() else { continue };
        let before = app.session.tools.brush.clone();
        let mut after = before.clone();
        set(&mut after, *value);
        crate::brush_panel::commit_keyed(app, &before, &after, crate::history_keys::coalesce("brush", event));
    }
}

/// `add` captioned with `label` as a field: the label sits beside the control and a drag on it scrubs a number field.
/// `None` when the body did not run. `add` can run twice in a pass (the measuring run, then the real one).
pub(super) fn labelled<R>(ui: &mut egui::Ui, label: &str, mut add: impl FnMut(&mut egui::Ui) -> R) -> Option<R> {
    Field::new(label.to_owned()).show(ui, |ui, _| add(ui)).inner
}

/// A check box named `label` over `value`.
pub(super) fn check(ui: &mut egui::Ui, label: &str, value: &mut bool) {
    Checkbox::new(Bind::bound(value)).label(label.to_owned()).show(ui);
}

/// A number field named `label` over `value` (in `unit` when it is not empty), limited to `range`. The events of this
/// pass carry the edited value; an emptied field gives none.
pub(super) fn number_field(ui: &mut egui::Ui, label: &str, value: f32, range: RangeInclusive<f64>, unit: &'static str) -> EditEvents<f32> {
    let events = labelled(ui, label, |ui| {
        let field = NumberField::new(label.to_owned(), Bind::given(Some(f64::from(value)))).range(range.clone()).scrub_by_label();
        let field = if unit.is_empty() { field } else { field.unit(unit) };
        field.show(ui).events.clone()
    });
    events.unwrap_or_default().filter_map(|v| v.map(|v| v as f32))
}

/// A percent field over a fraction `fraction` (0.5 shows as 50 %); its events carry fractions. `range` is in percent.
pub(super) fn percent_field(ui: &mut egui::Ui, label: &str, fraction: f32, range: RangeInclusive<f64>) -> EditEvents<f32> {
    number_field(ui, label, fraction * 100.0, range, "%").map(|percent| percent / 100.0)
}

/// The `layer.setProps` edit of a Layers panel Opacity or Fill (`key`) field at `pct` percent. A drag of the pop-up
/// slider carries its history step's `coalesce` key, so it is one step, as in Photoshop.
pub(super) fn pct_action(target: &Value, key: &str, pct: f32, coalesce: Option<String>) -> (String, Value) {
    let mut p = target.clone();
    p[key] = json!(pct / 100.0);
    if let Some(key) = coalesce {
        p["coalesce"] = json!(key);
    }
    ("layer.setProps".into(), p)
}
<<<<<<< HEAD
=======

/// Photoshop's scrubby Opacity/Fill labels: drag horizontally to change the percentage without
/// hitting the small number field. The unique drag ID coalesces every frame into one undo step.
pub(super) fn scrub_pct_label(ui: &mut egui::Ui, text: &str, percent: &mut f32, max_w: f32) -> widgets::PopupFieldResponse {
    // Elide to the room left in the row (full text on hover): a longer translation shrinks here
    // instead of running over the fixed controls beside it (#3580).
    let resp = widgets::elided_label_sensed(ui, text, max_w, Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    let key = resp.id.with("percent-label-drag");
    if resp.drag_started() {
        let stamp = ui.ctx().cumulative_pass_nr();
        ui.data_mut(|d| d.insert_temp(key, (*percent, stamp)));
    }
    let mut out = widgets::PopupFieldResponse::default();
    if resp.dragged()
        && let Some((start, stamp)) = ui.data(|d| d.get_temp::<(f32, u64)>(key))
        && let (Some(origin), Some(pos)) = (ui.input(|i| i.pointer.press_origin()), resp.interact_pointer_pos())
    {
        // Match DragValue's 0.5 percentage-point per screen point; do not jump on press.
        let next = (start + (pos.x - origin.x) * 0.5).clamp(0.0, 100.0);
        if next.is_finite() && next != *percent {
            *percent = next;
            out.changed = true;
            out.drag = Some(stamp);
        }
    }
    if resp.drag_stopped() {
        ui.data_mut(|d| d.remove::<(f32, u64)>(key));
    }
    out
}

/// Width of `s` as [`label`] draws it (body text).
pub(super) fn body_text_width(ui: &egui::Ui, s: &str) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    ui.painter().layout_no_wrap(s.to_string(), font, Color32::WHITE).size().x
}

/// Options-bar label: Photoshop writes "Size:" with a colon.
/// An options-bar label; one right before a value field scrubs it when dragged (Photoshop).
pub(crate) fn opt_label(ui: &mut egui::Ui, s: &str) {
    let t = Tokens::get(ui.ctx());
    let text = if t.pro && !s.ends_with(':') { format!("{s}:") } else { s.to_string() };
    let label = ui.add(egui::Label::new(RichText::new(tl!(&text)).color(t.text_dim)).selectable(false));
    widgets::scrub_option_label(ui, &label);
}

/// The largest Fixed Size side, in pixels.
pub(crate) const FIXED_SIZE_MAX_PX: f64 = 300_000.0;

/// The resolution Fixed Size's units convert at (the active document's, 72 ppi without one) and
/// the document's size in pixels, which a percentage is of. The unit is the caller's.
fn fixed_size_units(app: &PhotocraftApp) -> (widgets::FieldUnits, [f64; 2]) {
    let ppi = app.session.prefs().units_and_rulers.point_size.per_inch();
    let (dpi, size) = app
        .session
        .active()
        .map_or((72.0, [1.0, 1.0]), |d| (f64::from(d.doc.resolution_dpi.max(1.0)), [f64::from(d.doc.size.width), f64::from(d.doc.size.height)]));
    (widgets::FieldUnits { unit: photocraft_engine::prefs::Unit::Pixels, dpi, ppi }, size)
}

/// One of Fixed Size's Width and Height (#7910): `px` shown in `units.unit` (a typed unit converts),
/// still stored in pixels. A right-click lists the units, as on the rulers; returns the one picked.
fn fixed_size_field(ui: &mut egui::Ui, px: &mut f32, units: widgets::FieldUnits, extent: f64) -> Option<&'static str> {
    let widgets::FieldUnits { unit, dpi, ppi } = units;
    let to_unit = |px: f64| unit.from_px(px, dpi, extent, ppi) as f32;
    let before = to_unit(f64::from(*px));
    let mut v = before;
    let places = unit.decimals() as u32;
    let resp = widgets::value_field_extent(ui, &mut v, to_unit(1.0)..=to_unit(FIXED_SIZE_MAX_PX), unit.suffix(), 62.0, places, units, extent);
    // Leaving the field re-reads the rounded text it showed (100 px at 72 ppi reads back as
    // 1.389 in); that isn't an edit, so the pixels stay exact.
    let shown = |x: f32| (f64::from(x) * 10f64.powi(places as i32)).round();
    if resp.changed() && shown(v) != shown(before) {
        *px = unit.to_px(f64::from(v), dpi, extent, ppi).clamp(1.0, FIXED_SIZE_MAX_PX) as f32;
    }
    let mut picked = None;
    resp.context_menu(|ui| picked = crate::rulers::unit_menu(ui, unit));
    picked
}

fn percent_field(ui: &mut egui::Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, width: f32) -> egui::Response {
    opt_label(ui, label);
    let mut percent = *value * 100.0;
    let r = widgets::value_field_named(ui, label, &mut percent, range, "%", width);
    if r.changed() {
        *value = percent / 100.0;
    }
    r
}

/// A pressure toggle named `name` (for screen readers and tests); its tooltip is `name`, unless
/// `own_tip` leaves the tooltip to the caller (the Brush's own texts, #11813).
fn pressure_button(ui: &mut egui::Ui, on: bool, name: &str, own_tip: bool) -> egui::Response {
    icons::button_with_icon_size_tip(ui, "circle-dot", 24.0, (24.0f32 * 0.52).round(), on, name, !own_tip)
}

/// The reference's alerts for an Eraser Opacity or Flow, and Smoothing, typed out of range (#11938).
pub const ONE_TO_100: &str = "An integer between 1% and 100% is required. Closest value inserted.";
pub const ZERO_TO_100: &str = "An integer between 0% and 100% is required. Closest value inserted.";

/// The Eraser's Opacity or Flow: whole percents 1..100 that say when a typed value was past them.
fn eraser_percent(ui: &mut egui::Ui, label: &str, value: &mut f32) -> bool {
    opt_label(ui, label);
    let mut percent = *value * 100.0;
    let (r, alert) = widgets::value_field_whole_alerting(ui, label, &mut percent, 1.0..=100.0, "%", 62.0);
    if r.changed() {
        *value = percent / 100.0;
    }
    alert
}

/// Shows one of the range alerts as a modal with OK, once even if more than one pass asks.
pub(crate) fn range_alert(ui_state: &mut crate::state::UiState, text: &str) {
    let message = tl!(text).to_owned();
    let shown =
        ui_state.dialogs.iter().any(|d| d.kind == crate::state::DialogKind::Error && d.fields.get("message").and_then(|m| m.as_str()) == Some(&message));
    if !shown {
        ui_state.open_dialog(crate::state::DialogKind::Error, serde_json::json!({ "message": message }).as_object().cloned().unwrap_or_default());
    }
}

/// [`percent_field`] that keeps whole percents (`33.7` becomes 34 %). A value set elsewhere (a
/// `tools.setBrush` fraction, an older saved 0) shows as the whole percent in range.
fn whole_percent_field(ui: &mut egui::Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, width: f32) -> egui::Response {
    opt_label(ui, label);
    let mut percent = (*value * 100.0).round().max(*range.start()).min(*range.end());
    let r = widgets::value_field_named(ui, label, &mut percent, range.clone(), "%", width);
    if r.changed() {
        *value = percent.round().max(*range.start()).min(*range.end()) / 100.0;
    }
    r
}

fn hint(ui: &mut egui::Ui, s: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(s).color(t.text_faint));
}

#[cfg(test)]
#[path = "../eraser_range_alert_tests.rs"]
mod eraser_range_alert_tests;

#[cfg(test)]
#[path = "../blur_strength_alert_tests.rs"]
mod blur_strength_alert_tests;

#[cfg(test)]
#[path = "../sharpen_strength_alert_tests.rs"]
mod sharpen_strength_alert_tests;

#[cfg(test)]
#[path = "../smudge_strength_alert_tests.rs"]
mod smudge_strength_alert_tests;

#[cfg(test)]
#[path = "../pattern_stamp_opacity_flow_tests.rs"]
mod pattern_stamp_opacity_flow_tests;
>>>>>>> upstream/main
