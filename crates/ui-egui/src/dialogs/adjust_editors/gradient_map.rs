use super::{Edit, EditorCx, GradientPreset, Group, PhotocraftApp, Value, adjust_params, flag, json, label, rgb_of, round, vec2};
use crate::gradient_ui::stops_bar::{self, BarOptions, StopsEvent};
use craft::draw::ui::gradient::{ColorStop, GradientRamp, GradientStops, StopRows};
use craft::theme::Srgba;
use craft::ui::controls::Select;
use craft::ui::edit::EditEvent;
use craft::ui::menu::{MenuEntry, MenuItem};
use craft::ui::theme::{Tokens as LibTokens, TokensOf as _};

// ---------------------------------------------------------------------------------------------
// Gradient Map

type Stop = (f32, [f32; 3]);

fn read_stops(v: &Value) -> Vec<Stop> {
    let mut s: Vec<Stop> = v
        .get("stops")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| {
                    let p = s.as_array()?;
                    let t = p.first()?.as_f64()? as f32;
                    let c = rgb_of(&json!({"c": p.get(1)?.clone()}), "c", [0.0; 3]);
                    Some((t.clamp(0.0, 1.0), c))
                })
                .collect()
        })
        .unwrap_or_default();
    if s.len() < 2 {
        s = vec![(0.0, [0.0; 3]), (1.0, [1.0; 3])];
    }
    s
}

fn write_stops(v: &mut Value, s: &[Stop]) {
    let mut s = s.to_vec();
    s.sort_by(|a, b| a.0.total_cmp(&b.0));
    v["stops"] = json!(s.iter().map(|(t, c)| json!([round(*t, 0.001), adjust_params::hex(*c)])).collect::<Vec<_>>());
}

fn sample_stops(s: &[Stop], t: f32) -> [f32; 3] {
    let mut sorted = s.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let Some(first) = sorted.first() else { return [t; 3] };
    if t <= first.0 {
        return first.1;
    }
    for w in sorted.windows(2) {
        if t <= w[1].0 {
            let k = if w[1].0 > w[0].0 { (t - w[0].0) / (w[1].0 - w[0].0) } else { 0.0 };
            return std::array::from_fn(|i| w[0].1[i] + (w[1].1[i] - w[0].1[i]) * k);
        }
    }
    sorted.last().map_or([t; 3], |l| l.1)
}

/// The bar's value for `stops`: colour stops only, tagged by their sorted place.
fn bar_stops(stops: &[Stop]) -> GradientStops {
    let mut sorted = stops.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    GradientStops::new(
        sorted
            .iter()
            .enumerate()
            .map(|(i, (at, c))| ColorStop { location: *at, color: Srgba::new(c[0], c[1], c[2], 1.0), midpoint: 0.5, tag: i as u64 + 1 })
            .collect(),
    )
}

fn stops_of_bar(bar: &GradientStops) -> Vec<Stop> {
    bar.color.iter().map(|s| (s.location, [s.color.r, s.color.g, s.color.b])).collect()
}

/// The stops of a preset with the tool colours resolved; a map has no transparency, so they show opaque.
fn preset_colors(p: &GradientPreset, fg: [f32; 4], bg: [f32; 4]) -> Vec<(f32, [f32; 4])> {
    p.resolve(fg, bg).into_iter().map(|(at, c)| (at, [c[0], c[1], c[2], 1.0])).collect()
}

pub(super) fn gradient_map(ui: &mut egui::Ui, v: &mut Value, cx: &EditorCx) -> Edit {
    let mut e = Edit::default();
    let mut stops = read_stops(v);
    // Photoshop's gradient picker: the same library as the Gradient tool and Gradients panel
    // (built-in groups and the user's own), Foreground/Background resolved to the current colours.
    let [fg, bg] = cx.swatches.map(|c| [c[0], c[1], c[2], 1.0]);
    let bar_key = cx.mem.with("gm-bar");
    ui.horizontal(|ui| {
        label(ui, tl!("Preset:"));
        let tokens = LibTokens::of(ui);
        let mut current = (usize::MAX, 0usize);
        let mut select = Select::new(cx.mem.with("gm-preset"), tl!("Preset:"), &mut current).placeholder(tl!("Custom")).row_content(
            vec2(48.0, 16.0),
            |painter, rect, &(g, i)| {
                let Some(p) = cx.gradients.get(g).and_then(|group| group.items.get(i)) else { return };
                let colors: Vec<(f32, Srgba)> = preset_colors(p, fg, bg).into_iter().map(|(at, c)| (at, Srgba::new(c[0], c[1], c[2], c[3]))).collect();
                GradientRamp::paint_colors(painter, rect, &colors, &tokens);
            },
        );
        for (g, group) in cx.gradients.iter().enumerate() {
            select = select.group(group.name.clone());
            for (i, p) in group.items.iter().enumerate() {
                select = select.option((g, i), p.name.clone());
            }
        }
        let entries: Vec<MenuEntry<usize>> = vec![MenuItem::new(tl!("Import .GRD"), 0).into()];
        let mut menu_pick = None;
        let width = ui.available_width().min(190.0);
        let response = ui.allocate_ui_with_layout(vec2(width, 0.0), egui::Layout::top_down_justified(egui::Align::Min), |ui| {
            select.header_menu(tl!("Gradient Preset Options"), &entries, &mut menu_pick).show(ui)
        });
        if menu_pick.is_some() {
            e.import_gradients = true;
        }
        let picked = match response.inner.event {
            Some(EditEvent::Commit { value, changed: true, .. } | EditEvent::Set { value, .. }) => Some(value),
            _ => None,
        };
        if let Some((g, i)) = picked
            && let Some(p) = cx.gradients.get(g).and_then(|group| group.items.get(i))
        {
            stops = preset_colors(p, fg, bg).into_iter().map(|(at, c)| (at, [c[0], c[1], c[2]])).collect();
            stops_bar::clear_draft(ui, bar_key);
            write_stops(v, &stops);
            e.add(Edit::discrete(true));
        }
    });
    // The library bar: click adds, drag moves, a tear-off deletes; its fields row sets the
    // selected stop's colour (in its popover) and location, and deletes it.
    let mut changed = false;
    let options = BarOptions { stops: (2, 64), color_popover: true, live: true, ..BarOptions::new(StopRows::ColorOnly, false) };
    match stops_bar::edit_stops(ui, bar_key, &bar_stops(&stops), options) {
        Some(StopsEvent::Preview { value, .. }) => {
            stops = stops_of_bar(&value);
            changed = true;
            e.changed = true;
        }
        Some(StopsEvent::Commit { value, changed: true, .. }) => {
            stops = stops_of_bar(&value);
            changed = true;
            e.add(Edit::discrete(true));
        }
        Some(StopsEvent::Set { value, .. }) => {
            stops = stops_of_bar(&value);
            changed = true;
            e.add(Edit::discrete(true));
        }
        Some(StopsEvent::Cancel { base }) => {
            stops = stops_of_bar(&base);
            changed = true;
            e.changed = true;
        }
        _ => {}
    }
    if stops.len() < 64 && ui.add(craft::ui::controls::Button::new(tl!("Add Stop")).variant(craft::ui::controls::ButtonVariant::Secondary)).clicked() {
        // Halfway along the widest gap, in the gradient's colour there.
        let mut sorted = stops.clone();
        sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
        let at = sorted.windows(2).max_by(|a, b| (a[1].0 - a[0].0).total_cmp(&(b[1].0 - b[0].0))).map_or(0.5, |w| (w[0].0 + w[1].0) / 2.0);
        stops.push((at, sample_stops(&stops, at)));
        changed = true;
        e.add(Edit::discrete(true));
    }
    if changed {
        write_stops(v, &stops);
        e.changed = true;
    }
    ui.horizontal(|ui| {
        for (key, text) in [("reverse", tl!("Reverse")), ("dither", tl!("Dither"))] {
            let mut b = flag(v, key, false);
            if craft::ui::controls::Checkbox::bool(text, &mut b).show(ui).response.changed() {
                v[key] = json!(b);
                e.add(Edit::discrete(true));
            }
        }
    });
    e
}

// ---------------------------------------------------------------------------------------------
// Hosts

/// Whether a document of this mode edits Levels/Curves through a single Gray channel.
pub fn is_gray(mode: photocraft_doc::ColorMode) -> bool {
    matches!(mode, photocraft_doc::ColorMode::Grayscale | photocraft_doc::ColorMode::Duotone | photocraft_doc::ColorMode::Bitmap)
}

/// The gradient preset library for a `kind` editor's picker (Gradient Map only; empty otherwise).
pub fn gradient_presets(app: &PhotocraftApp, kind: &str) -> Vec<Group<GradientPreset>> {
    if kind == "gradientMap" { app.session.presets.gradients.clone() } else { Vec::new() }
}

pub fn swatches(app: &PhotocraftApp) -> [[f32; 3]; 2] {
    let c = |x: [f32; 4]| [x[0], x[1], x[2]];
    [c(app.session.tools.foreground), c(app.session.tools.background)]
}
