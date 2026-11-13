//! Window › Brush Settings (F5) and Window › Brushes: Photoshop's floating brush panel.
//!
//! Brush Settings: the section list with enable boxes on the left (clicking a controls section
//! shows and enables it; the last five rows only toggle), the selected section's controls on the right (see [`crate::brush_sections`]),
//! and a live stroke preview strip below, rendered with the real brush engine and cached on the
//! settings (it re-renders only when they change). Brushes: the presets in collapsible groups with
//! tip thumbnails and stroke previews, a size slider and a search field.
//!
//! The panel never writes the session brush itself: each frame's edits become one
//! `tools.setBrush` call carrying only the changed fields (Rule 1), so the journal, the control
//! channel and the MCP server see exactly what the panel did and can do the same. A drag's calls
//! share a `coalesce` key, so a whole gesture is one journal entry ([`commit_gesture`]).
//!
//! The lock beside a section name keeps that section's settings when another preset is picked
//! (`locks` in the brush, applied by the engine). The Brushes tab lives in [`crate::brushes_tab`].

use egui::{CornerRadius, Stroke};
use photocraft_engine::BrushSettings;
use photocraft_engine::paint;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::theme::Tokens;
use crate::{PhotocraftApp, icons, widgets};

pub use crate::brush_preview::{preview_pixels, preview_sig};
pub use crate::brush_sections::section_body;
pub use photocraft_engine::brush_cmds::brush_patch;

/// Sections in Photoshop's order. The bool says whether the section has an enable box.
pub const SECTIONS: [(&str, bool); 13] = [
    ("Brush Tip Shape", false),
    ("Shape Dynamics", true),
    ("Scattering", true),
    ("Texture", true),
    ("Dual Brush", true),
    ("Color Dynamics", true),
    ("Transfer", true),
    ("Brush Pose", true),
    ("Noise", true),
    ("Wet Edges", true),
    ("Build-up", true),
    ("Smoothing", true),
    ("Protect Texture", true),
];

/// The final five entries are switches, not sections with a properties pane.
pub(super) const CHECKBOX_ONLY_START: usize = 8;

/// Panel width.
pub(crate) const WIDTH: f32 = 540.0;

/// How the Brushes tab lists presets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrushesView {
    /// Rows: tip, size, stroke preview, name.
    #[default]
    List,
    /// Tip thumbnails with their sizes.
    Grid,
}

/// A rename in progress in the Brushes tab (a preset, or a group when `group` is set).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Renaming {
    pub group: bool,
    /// The preset (or group) being renamed.
    pub name: String,
    /// With `group`: the nested folder of group `name` being renamed (empty = the group itself).
    pub folder: Vec<String>,
    /// The text typed so far.
    pub text: String,
}

/// Brushes panel view state (serde, so the control channel can read and drive it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BrushesPanelState {
    /// Collapsed groups and folders: a group's label, or for a nested folder the group's label and
    /// the folder path joined with `/` ([`crate::brushes_tab::folder_view_key`]).
    pub collapsed: Vec<String>,
    /// Case-insensitive name filter.
    pub filter: String,
    pub view: BrushesView,
    pub renaming: Option<Renaming>,
    /// The Brush Preset picker's cards: show the preset's name. At least one of the three parts
    /// is always on, and the picker's cards never go empty (see [`crate::brush_picker`]).
    pub show_name: bool,
    /// The Brush Preset picker's cards: show the stroke preview.
    pub show_stroke: bool,
    /// The Brush Preset picker's cards: show the tip thumbnail.
    pub show_tip: bool,
    /// The Brush Preset picker's search field (its gear menu's Show Search Bar).
    pub show_search: bool,
    /// The Brush Preset picker's cards and the Brushes tab's grid cells and list rows: their size
    /// scale, from the picker's footer slider (`crate::brush_picker::body`) or the tab's preview-size
    /// slider. 1 is the standard size; the cards' width, height and
    /// insides multiply by it (`crate::brushes_tab`) — the height down to a 0.50 floor, the
    /// width all the way — the text holds its 1.0 size, and at or below the compact threshold
    /// the tips drop the size number under them and take their cell whole.
    pub scale: f32,
}

impl Default for BrushesPanelState {
    fn default() -> Self {
        Self {
            collapsed: Vec::new(),
            filter: String::new(),
            view: BrushesView::default(),
            renaming: None,
            show_name: true,
            show_stroke: true,
            show_tip: true,
            show_search: true,
            scale: 1.0,
        }
    }
}

/// The enable flag behind section `i` (None for Brush Tip Shape).
pub fn section_flag(b: &mut BrushSettings, i: usize) -> Option<&mut bool> {
    Some(match i {
        1 => &mut b.shape_dynamics.enabled,
        2 => &mut b.scattering.enabled,
        3 => &mut b.texture.enabled,
        4 => &mut b.dual_brush.enabled,
        5 => &mut b.color_dynamics.enabled,
        6 => &mut b.transfer.enabled,
        7 => &mut b.pose.enabled,
        8 => &mut b.noise,
        9 => &mut b.wet_edges,
        10 => &mut b.build_up,
        11 => &mut b.smoothing.enabled,
        12 => &mut b.protect_texture,
        _ => return None,
    })
}

/// The lock behind section `i` (every section but Brush Tip Shape has one, as in Photoshop).
pub fn section_lock(b: &mut BrushSettings, i: usize) -> Option<&mut bool> {
    let l = &mut b.locks;
    Some(match i {
        1 => &mut l.shape_dynamics,
        2 => &mut l.scattering,
        3 => &mut l.texture,
        4 => &mut l.dual_brush,
        5 => &mut l.color_dynamics,
        6 => &mut l.transfer,
        7 => &mut l.pose,
        8 => &mut l.noise,
        9 => &mut l.wet_edges,
        10 => &mut l.build_up,
        11 => &mut l.smoothing,
        12 => &mut l.protect_texture,
        _ => return None,
    })
}

/// Send the panel's edits (`before` → `after`) through `tools.setBrush`.
pub fn commit(app: &mut PhotocraftApp, before: &BrushSettings, after: &BrushSettings) {
    send(app, before, after, None);
}

fn send(app: &mut PhotocraftApp, before: &BrushSettings, after: &BrushSettings, key: Option<String>) -> bool {
    if before == after {
        return false;
    }
    let patch = brush_patch(before, after);
    if patch.as_object().is_some_and(Map::is_empty) {
        return false;
    }
    let mut p = json!({ "brush": patch });
    if let Some(k) = key {
        p["coalesce"] = json!(k);
    }
    if let Err(e) = app.run("tools.setBrush", p) {
        app.ui.status = e;
    }
    true
}

const GESTURE: &str = "brush-set-gesture";

/// The current brush-edit gesture number. A pointer press starts a new gesture (once per frame);
/// [`commit_gesture`] ends one after an edit made with the pointer up (a click, a typed value).
fn gesture(ctx: &egui::Context) -> u64 {
    let frame = ctx.cumulative_pass_nr();
    let pressed = ctx.input(|i| i.pointer.any_pressed());
    ctx.data_mut(|d| {
        let g = d.get_temp_mut_or_default::<(u64, u64)>(egui::Id::unique(GESTURE));
        if pressed && g.1 != frame {
            g.0 += 1;
            g.1 = frame;
        }
        g.0
    })
}

/// [`commit`] as part of a gesture: every call of one drag carries the same `coalesce` key, so
/// the engine journals the drag as a single `tools.setBrush` (Rule 1: one command per gesture).
/// Call it every frame (it tracks presses even when nothing changed).
pub fn commit_gesture(app: &mut PhotocraftApp, ctx: &egui::Context, before: &BrushSettings, after: &BrushSettings) {
    let g = gesture(ctx);
    if send(app, before, after, Some(format!("brush-ui:{g}"))) && !ctx.input(|i| i.pointer.any_down()) {
        ctx.data_mut(|d| d.get_temp_mut_or_default::<(u64, u64)>(egui::Id::unique(GESTURE)).0 += 1);
    }
}

/// Group label for presets saved without a group.
pub const UNGROUPED: &str = photocraft_engine::brush_key_cmds::UNGROUPED;

/// Preset indices in panel order: groups in order of first appearance.
pub fn grouped_presets(presets: &[paint::BrushPreset]) -> Vec<(String, Vec<usize>)> {
    let mut out: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, p) in presets.iter().enumerate() {
        let g = if p.group.is_empty() { UNGROUPED } else { p.group.as_str() };
        match out.iter_mut().find(|(n, _)| n == g) {
            Some((_, v)) => v.push(i),
            None => out.push((g.to_string(), vec![i])),
        }
    }
    out
}

/// A fresh "Brush N" name.
pub(crate) fn new_preset_name(presets: &[paint::BrushPreset]) -> String {
    (1..).map(|n| format!("Brush {n}")).find(|n| !presets.iter().any(|p| &p.name == n)).unwrap_or_else(|| "Brush".into())
}

pub(crate) fn run_or_status(app: &mut PhotocraftApp, id: &str, p: Value) {
    if let Err(e) = app.run(id, p) {
        app.ui.status = e;
    }
}

pub(crate) fn full_uv() -> egui::Rect {
    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0))
}

/// Clear brush controls: turns off every section except Smoothing and resets their options
/// to the defaults (Photoshop), keeping locked sections intact.
pub fn clear_brush_controls(current: &BrushSettings) -> BrushSettings {
    let mut b = current.clone();
    let d = BrushSettings::default();
    let l = &current.locks;
    if !l.shape_dynamics {
        b.shape_dynamics = d.shape_dynamics;
    }
    if !l.scattering {
        b.scattering = d.scattering;
    }
    if !l.texture {
        b.texture = d.texture;
    }
    if !l.dual_brush {
        b.dual_brush = d.dual_brush;
    }
    if !l.color_dynamics {
        b.color_dynamics = d.color_dynamics;
    }
    if !l.transfer {
        b.transfer = d.transfer;
    }
    if !l.pose {
        b.pose = d.pose;
    }
    if !l.noise {
        b.noise = d.noise;
    }
    if !l.wet_edges {
        b.wet_edges = d.wet_edges;
    }
    if !l.build_up {
        b.build_up = d.build_up;
        b.build_up_rate = d.build_up_rate;
    }
    if !l.protect_texture {
        b.protect_texture = d.protect_texture;
    }
    b
}

/// Copy the current brush's Texture settings to other painting tools that have Texture enabled.
pub fn copy_texture_to_other_tools(app: &mut PhotocraftApp) {
    let texture = app.session.tools.brush.texture.clone();
    for (_, brush) in &mut app.ui.tool_brushes {
        if brush.texture.enabled {
            brush.texture = texture.clone();
        }
    }
}

/// The Brush Settings panel menu actions (hamburger menu).
pub fn panel_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if ui.button(tl!("New Brush Preset…")).clicked() {
        let name = new_preset_name(&app.session.tools.presets);
        run_or_status(app, "brush.presets.save", json!({ "name": name }));
        if app.session.tools.presets.iter().any(|p| p.name == name) {
            app.ui.brushes_panel.renaming = Some(Renaming { group: false, name: name.clone(), folder: Vec::new(), text: String::new() });
            app.ui.brush_picker_list.renaming = Some(Renaming { group: false, name, folder: Vec::new(), text: String::new() });
        }
        ui.close();
    }
    ui.separator();
    if ui.button(tl!("Clear Brush Controls")).clicked() {
        let before = app.session.tools.brush.clone();
        let cleared = clear_brush_controls(&before);
        commit(app, &before, &cleared);
        ui.close();
    }
    let has_locks = app.session.tools.brush.locks != photocraft_paint::brush::SectionLocks::default();
    if ui.add_enabled(has_locks, egui::Button::new(tl!("Reset All Locked Settings"))).clicked() {
        let before = app.session.tools.brush.clone();
        let mut after = before.clone();
        after.locks = photocraft_paint::brush::SectionLocks::default();
        commit(app, &before, &after);
        ui.close();
    }
    let has_texture = app.session.tools.brush.texture.enabled;
    if ui.add_enabled(has_texture, egui::Button::new(tl!("Copy Texture to Other Tools"))).clicked() {
        copy_texture_to_other_tools(app);
        ui.close();
    }
}

pub fn window(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !app.ui.panels.brush_settings {
        return;
    }
    let t = Tokens::get(ctx);
    let frame = egui::Frame::NONE
        .fill(t.card)
        .stroke(Stroke::new(1.0, t.card_border))
        .corner_radius(CornerRadius::same(t.radius_lg as u8))
        .shadow(egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: t.shadow })
        .inner_margin(egui::Margin::same(10));
    let canvas = app.last_canvas_rect;
    let mut open = true;
    egui::Window::new(tl!("Brush Settings"))
        .id(egui::Id::unique("brush-settings"))
        .title_bar(false)
        .resizable(false)
        .frame(frame)
        .default_pos(egui::pos2((canvas.right() - WIDTH - 30.0).max(canvas.left() + 8.0), canvas.top() + 24.0))
        .show(ctx, |ui| {
            ui.set_width(WIDTH);
            ui.horizontal(|ui| {
                let mut tab = app.ui.brush_tab;
                for (i, name) in [tl!("Brush Settings"), tl!("Brushes")].iter().enumerate() {
                    if widgets::pill_tab(ui, name, tab == i).clicked() {
                        tab = i;
                    }
                }
                app.ui.brush_tab = tab;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::button(ui, "x", 20.0, false, tl!("Close")).clicked() {
                        open = false;
                    }
                    let menu_btn = widgets::panel_menu_button(ui);
                    egui::Popup::menu(&menu_btn).show(|ui| {
                        widgets::style_spectrum_popup_menu(ui);
                        ui.set_min_width(180.0);
                        panel_menu(app, ui);
                        ui.separator();
                        if ui.button(tl!("Close")).clicked() {
                            open = false;
                            ui.close();
                        }
                        if ui.button(tl!("Close Tab Group")).clicked() {
                            open = false;
                            ui.close();
                        }
                    });
                });
            });
            ui.add_space(6.0);
            widgets::hairline(ui);
            ui.add_space(6.0);
            if app.ui.brush_tab == 1 {
                crate::brushes_tab::show(app, ui);
            } else {
                super::brush_settings::brush_settings(app, ui);
            }
        });
    if !open {
        app.ui.panels.brush_settings = false;
    }
}

#[cfg(test)]
mod tests;
