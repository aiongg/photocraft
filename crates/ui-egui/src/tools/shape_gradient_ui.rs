//! The shape Fill / Stroke swatches' Gradient tab, as Photoshop's popover: the recently used
//! gradients and the preset library, a stops strip (click to add a stop, drag one to move it or
//! off the strip to delete it, drag a midpoint), the selected stop's colour, opacity and
//! location, and the gradient's style, angle, scale, reverse and Align with layer. Stop edits use
//! the Gradient Editor's grammar ([`apply_stop`]); the tab returns the edited gradient and the
//! swatch's caller applies it to the next shape or as one `shape.edit`.

use craft_theme::Srgba;
use craft_ui::composites::ColorPickerLayout;
use craft_ui::composites::gradient::{GradientEditor, GradientGroup, GradientItem, GradientPalette, GradientRef, GradientStops, StopRef};
use craft_ui::composites::ColorPicker;
use craft_ui::localized::LocaleNumbers;
use craft_ui::theme::ToEgui;
use photocraft_doc::{Fill, GradientStyle};
use photocraft_engine::gradient_fill_cmds::apply_stop;
use photocraft_engine::presets::Group;
use photocraft_engine::presets::gradients::GradientPreset;
use serde_json::{Value, json};

use crate::gradient_ui::stops_bar::{self, BarAction};
use crate::shape_paint_ui::rgba_of;

/// Width of the tab's rows (the strip, the swatch grids).
const WIDTH: f32 = 228.0;
/// How many gradients the Recent row keeps.
const RECENT: usize = 8;

/// What a swatch's Gradient tab offers: the recently used gradients, the preset library, and the
/// Gradient tool's current gradient (what a colour switches to), with the tool colours that
/// resolve their Foreground / Background stops.
pub(crate) struct GradientChoices<'a> {
    pub presets: &'a [Group<GradientPreset>],
    pub current: &'a GradientPreset,
    /// Newest first ([`GradientChoices::remember`]).
    pub recent: &'a mut Vec<Fill>,
    pub fg: [f32; 4],
    pub bg: [f32; 4],
}

impl GradientChoices<'_> {
    /// Photoshop's shape gradient default: the current gradient, linear at 90°, 100%.
    pub(crate) fn default_fill(&self) -> Fill {
        let mut fill = Fill::gradient(Vec::new(), 90.0, 1.0, GradientStyle::Linear, false);
        take_preset(&mut fill, self.current, self.fg, self.bg);
        fill
    }

    /// Puts `gradient`'s stops at the front of the Recent row; a gradient already there moves up.
    pub(crate) fn remember(&mut self, gradient: &Fill) {
        let Fill::Gradient { .. } = gradient else { return };
        self.recent.retain(|f| !same_stops(f, gradient));
        self.recent.insert(0, gradient.clone());
        self.recent.truncate(RECENT);
    }
}

/// Whether two gradients have the same colour stops, opacity stops and midpoints.
fn same_stops(a: &Fill, b: &Fill) -> bool {
    match (a, b) {
        (Fill::Gradient { stops: s1, opacity_stops: o1, midpoints: m1, .. }, Fill::Gradient { stops: s2, opacity_stops: o2, midpoints: m2, .. }) => {
            s1 == s2 && o1 == o2 && m1 == m2
        }
        _ => false,
    }
}

/// `fill` takes `preset`'s colour and opacity stops (live colours resolved now), as a Gradient
/// Fill layer made from it holds them; its old midpoints belonged to the old stops.
pub(crate) fn take_preset(fill: &mut Fill, preset: &GradientPreset, fg: [f32; 4], bg: [f32; 4]) {
    if let Fill::Gradient { stops, opacity_stops, midpoints, .. } = fill {
        (*stops, *opacity_stops) = preset.fill_stops(fg, bg);
        midpoints.clear();
    }
}

/// `fill` takes `from`'s colour stops, opacity stops and midpoints (a Recent pick).
fn take_stops(fill: &mut Fill, from: &Fill) {
    if let (Fill::Gradient { stops, opacity_stops, midpoints, .. }, Fill::Gradient { stops: s, opacity_stops: o, midpoints: m, .. }) = (fill, from) {
        (*stops, *opacity_stops, *midpoints) = (s.clone(), o.clone(), m.clone());
    }
}

/// The Gradient tab of the swatch `id`. Returns the edited gradient.
pub(crate) fn gradient_tab(ui: &mut egui::Ui, fill: &Fill, choices: &GradientChoices, id: egui::Id) -> Option<Fill> {
    let Fill::Gradient { angle, scale, style, reverse, align, .. } = fill else { return None };
    let t = craft_ui::theme::current(ui.ctx());
    let mut out: Option<Fill> = None;
    // The library palette: the recent gradients, then the preset library, as the bar shows them.
    let recent: Vec<GradientStops> = choices.recent.iter().map(stops_bar::stops_of).collect();
    let groups: Vec<GradientGroup> = choices
        .presets
        .iter()
        .map(|g| GradientGroup::new(g.name.clone(), g.items.iter().map(|p| GradientItem::new(p.name.clone(), preset_stops(p, choices.fg, choices.bg))).collect()))
        .collect();
    let picked = ui
        .scope(|ui| {
            ui.set_width(WIDTH);
            GradientPalette::new(id.with("gradient-presets"), tl!("Gradient presets"), &groups)
                .recent(&recent, tl!("Recent"))
                .max_height(120.0)
                .show(ui)
        })
        .inner;
    match picked.event.as_ref().and_then(|e| e.value()) {
        Some(GradientRef::Recent(i)) => {
            if let Some(from) = choices.recent.get(*i) {
                take_stops(edited(&mut out, fill), from);
            }
        }
        Some(GradientRef::Item { group, item }) => {
            if let Some(preset) = choices.presets.get(*group).and_then(|g| g.items.get(*item)) {
                take_preset(edited(&mut out, fill), preset, choices.fg, choices.bg);
            }
        }
        None => {}
    }
    if out.is_some() {
        // New stops: any gesture draft, picker and selection belonged to the old ones.
        stops_bar::clear_draft(ui, bar_key(id));
        ui.data_mut(|d| d.remove::<usize>(picker_key(id)));
        GradientEditor::select(ui.ctx(), GradientEditor::id_in(ui, bar_key(id)), Some(StopRef::Color(0)));
    }
    ui.add_space(craft_ui::theme::scale::Space::S6.px());
    if let Some(f) = stops_editor(ui, out.as_ref().unwrap_or(fill), id, choices.fg, choices.bg) {
        out = Some(f);
    }
    ui.add_space(t.space.gap.stack);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (s, icon, tip) in [
            (GradientStyle::Linear, "gradient", tl!("Linear Gradient")),
            (GradientStyle::Radial, "circle", tl!("Radial Gradient")),
            (GradientStyle::Angle, "rotate-cw", tl!("Angle Gradient")),
            (GradientStyle::Reflected, "arrow-left-right", tl!("Reflected Gradient")),
            (GradientStyle::Diamond, "diamond", tl!("Diamond Gradient")),
        ] {
            if crate::icons::button(ui, icon, 22.0, *style == s, tip).clicked()
                && *style != s
                && let Fill::Gradient { style, .. } = edited(&mut out, fill)
            {
                *style = s;
            }
        }
    });
    let label = |ui: &mut egui::Ui, s: &str| ui.label(egui::RichText::new(s).color(t.color.text.muted.to_egui()).size(12.0));
    ui.horizontal(|ui| {
        label(ui, tl!("Angle"));
        let mut a = *angle;
        if crate::widgets::value_field(ui, &mut a, -180.0..=180.0, "°", 52.0).changed()
            && let Fill::Gradient { angle, .. } = edited(&mut out, fill)
        {
            *angle = a;
        }
        label(ui, tl!("Scale"));
        let mut sc = *scale * 100.0;
        if crate::widgets::value_field(ui, &mut sc, 10.0..=150.0, "%", 52.0).changed()
            && let Fill::Gradient { scale, .. } = edited(&mut out, fill)
        {
            *scale = sc / 100.0;
        }
    });
    ui.horizontal(|ui| {
        let mut rev = *reverse;
        if craft_ui::controls::Checkbox::bool(tl!("Reverse"), &mut rev).show(ui).response.changed()
            && let Fill::Gradient { reverse, .. } = edited(&mut out, fill)
        {
            *reverse = rev;
        }
        let mut al = *align;
        if craft_ui::controls::Checkbox::bool(tl!("Align with layer"), &mut al).show(ui).response.changed()
            && let Fill::Gradient { align, .. } = edited(&mut out, fill)
        {
            *align = al;
        }
    });
    out
}

/// The gradient being edited this frame: `fill` until a control changes it.
fn edited<'a>(out: &'a mut Option<Fill>, fill: &Fill) -> &'a mut Fill {
    out.get_or_insert_with(|| fill.clone())
}


/// The preset's stops as the bar shows them, the tool colours resolved.
fn preset_stops(preset: &GradientPreset, fg: [f32; 4], bg: [f32; 4]) -> GradientStops {
    let mut fill = Fill::gradient(Vec::new(), 0.0, 1.0, GradientStyle::Linear, false);
    take_preset(&mut fill, preset, fg, bg);
    stops_bar::stops_of(&fill)
}

fn bar_key(id: egui::Id) -> egui::Id {
    id.with("stops-bar")
}

/// Where swatch `id` keeps the colour stop whose inline picker is open.
fn picker_key(id: egui::Id) -> egui::Id {
    id.with("stop-picker-open")
}

/// Where swatch `id` keeps the gradient a bar gesture started from.
fn gesture_key(id: egui::Id) -> egui::Id {
    id.with("stops-gesture")
}

fn apply(fill: &Fill, edit: &Value, fg: [f32; 4], bg: [f32; 4]) -> Option<Fill> {
    apply_stop(fill, edit, fg, bg).ok()
}

/// The stops bar, its fields row and the open colour stop's inline picker. The tab previews each
/// gesture step on the shape, so the bar's commands address the gradient the gesture started from.
fn stops_editor(ui: &mut egui::Ui, fill: &Fill, id: egui::Id, fg: [f32; 4], bg: [f32; 4]) -> Option<Fill> {
    let gesture = gesture_key(id);
    let start = |ui: &egui::Ui| ui.data(|d| d.get_temp::<Fill>(gesture));
    let mut out = None;
    match stops_bar::show(ui, bar_key(id), fill, true, true) {
        Some(BarAction::Preview(p)) => {
            let base = start(ui).unwrap_or_else(|| fill.clone());
            ui.data_mut(|d| d.insert_temp(gesture, base.clone()));
            out = apply(&base, &p, fg, bg);
        }
        Some(BarAction::Commit(p)) => {
            let base = start(ui).unwrap_or_else(|| fill.clone());
            ui.data_mut(|d| d.remove::<Fill>(gesture));
            out = apply(&base, &p, fg, bg);
        }
        Some(BarAction::Clear) => {
            let base = start(ui);
            ui.data_mut(|d| d.remove::<Fill>(gesture));
            out = base;
        }
        Some(BarAction::Color(i)) => {
            ui.data_mut(|d| {
                if d.get_temp::<usize>(picker_key(id)) == Some(i) {
                    d.remove::<usize>(picker_key(id));
                } else {
                    d.insert_temp(picker_key(id), i);
                }
            });
        }
        None => {}
    }
    let shown = out.as_ref().unwrap_or(fill);
    if let Some(i) = ui.data(|d| d.get_temp::<usize>(picker_key(id)))
        && let Some(edit) = stop_picker(ui, shown, id, i)
        && let Some(f) = apply(shown, &edit, fg, bg)
    {
        out = Some(f);
    }
    out
}

/// The inline picker of colour stop `i` (sorted order). The stop's colour keeps its own alpha;
/// transparency belongs to the opacity stops.
fn stop_picker(ui: &mut egui::Ui, fill: &Fill, id: egui::Id, i: usize) -> Option<Value> {
    let Fill::Gradient { stops, .. } = fill else { return None };
    let mut sorted = stops.clone();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let rgba = rgba_of(&sorted.get(i)?.1);
    let numbers = LocaleNumbers::of(ui);
    let response = ColorPicker::new(id.with(("stop-picker", i)), tl!("Color"), Srgba::new(rgba[0], rgba[1], rgba[2], 1.0), &numbers)
        .layout(ColorPickerLayout::Compact)
        .text(|key| tl!(key).to_string())
        .show(ui);
    let picked = response.edit.event.as_ref().and_then(|e| e.value())?;
    let c = picked.color;
    ([c.r, c.g, c.b].iter().zip(rgba).any(|(a, b)| (a - b).abs() > 0.000001)).then(|| json!({"action": "color", "index": i, "color": [c.r, c.g, c.b, rgba[3]]}))
}
