//! The library `GradientEditor` over a gradient fill: the bar, its markers and its fields row.
//!
//! The library edits whole stop lists; the engine takes one `gradient.fill.stop` (or
//! `gradient.presets.stop`) edit per undo step. [`stops_of`] numbers the stops by their sorted
//! index (the tag travels with a stop through moves and reorders) and [`command`] reads the edit
//! back from the tags.

use craft_theme::Srgba;
use craft_ui::composites::gradient::{ColorStop, GradientEditor, GradientLabels, GradientStops, OpacityStop, StopRow, StopRows};
use craft_ui::edit::EditEvent;
use craft_ui::localized::LocaleNumbers;
use egui::Ui;
use photocraft_doc::Fill;
use serde_json::{Value, json};

/// What the bar asks the caller to do this frame.
pub(super) enum BarAction {
    /// A gesture's value, previewed through the canvas path until it commits.
    Preview(Value),
    /// A finished edit: one command, one history step.
    Commit(Value),
    /// A gesture ended or was cancelled with nothing to apply.
    Clear,
    /// Edit the colour of colour stop `i` (double-click, Enter on it, or the Color well).
    Color(usize),
}

/// The bar's value for `fill`: sorted stops tagged `index + 1`, the opacity row always explicit
/// (the engine's empty list is two 100 % stops).
pub(super) fn stops_of(fill: &Fill) -> GradientStops {
    let Fill::Gradient { stops, opacity_stops, midpoints, .. } = fill else { return GradientStops::default() };
    let mut sorted: Vec<(usize, &(f32, photocraft_color::Color))> = stops.iter().enumerate().collect();
    sorted.sort_by(|a, b| a.1.0.total_cmp(&b.1.0));
    let color = sorted
        .iter()
        .enumerate()
        .map(|(i, (original, (location, color)))| {
            let [r, g, b] = color.to_rgb();
            ColorStop { location: *location, color: Srgba::new(r, g, b, 1.0), midpoint: midpoints.get(*original).copied().unwrap_or(0.5), tag: i as u64 + 1 }
        })
        .collect();
    let mut opacity: Vec<(f32, f32)> = if opacity_stops.is_empty() { vec![(0.0, 1.0), (1.0, 1.0)] } else { opacity_stops.clone() };
    opacity.sort_by(|a, b| a.0.total_cmp(&b.0));
    let opacity = opacity
        .iter()
        .enumerate()
        .map(|(i, (location, opacity))| OpacityStop { location: *location, opacity: *opacity, midpoint: 0.5, tag: i as u64 + 1 })
        .collect();
    GradientStops { color, opacity }
}

/// The engine edit that turns `old` (from [`stops_of`]) into `new`, or `None` when they agree or
/// the change is one the engine has no edit for (an opacity midpoint).
pub(super) fn command(old: &GradientStops, new: &GradientStops) -> Option<Value> {
    row_command(old, new, StopRow::Color).or_else(|| row_command(old, new, StopRow::Opacity))
}

fn row_command(old: &GradientStops, new: &GradientStops, row: StopRow) -> Option<Value> {
    // (tag, location, value, midpoint) per stop; the value is the colour or the opacity.
    let flat = |s: &GradientStops| -> Vec<(u64, f32, Srgba, f32)> {
        match row {
            StopRow::Color => s.color.iter().map(|c| (c.tag, c.location, c.color, c.midpoint)).collect(),
            StopRow::Opacity => s.opacity.iter().map(|o| (o.tag, o.location, Srgba::new(o.opacity, 0.0, 0.0, 1.0), o.midpoint)).collect(),
        }
    };
    let (before, after) = (flat(old), flat(new));
    let old_of = |tag: u64| before.iter().position(|s| tag > 0 && s.0 == tag);
    let with_kind = |mut p: Value| {
        if row == StopRow::Opacity {
            p["kind"] = json!("opacity");
        }
        p
    };
    let percent = |v: f32| f64::from(v) * 100.0;
    if after.len() == before.len() + 1 {
        // The added stop: no tag, or a copy whose tag is also its source's and whose place differs.
        let added = after.iter().find(|s| old_of(s.0).is_none()).or_else(|| after.iter().find(|s| old_of(s.0).is_some_and(|i| before[i].1 != s.1)))?;
        let mut p = json!({"action": "add", "location": added.1});
        match row {
            StopRow::Color if added.0 > 0 => p["color"] = json!([added.2.r, added.2.g, added.2.b]),
            StopRow::Opacity => p["opacity"] = json!(percent(added.2.r)),
            StopRow::Color => {}
        }
        return Some(with_kind(p));
    }
    if after.len() + 1 == before.len() {
        let gone = before.iter().find(|b| !after.iter().any(|a| a.0 == b.0))?;
        let index = before.iter().position(|b| b.0 == gone.0)?;
        return Some(with_kind(json!({"action": "delete", "index": index})));
    }
    if after.len() != before.len() {
        return None;
    }
    for stop in &after {
        let Some(index) = old_of(stop.0) else { continue };
        let was = &before[index];
        if was.1 != stop.1 {
            return Some(with_kind(json!({"action": "move", "index": index, "location": stop.1})));
        }
        if was.2 != stop.2 {
            return Some(match row {
                StopRow::Color => json!({"action": "color", "index": index, "color": [stop.2.r, stop.2.g, stop.2.b]}),
                StopRow::Opacity => json!({"action": "opacity", "kind": "opacity", "index": index, "opacity": percent(stop.2.r)}),
            });
        }
        if row == StopRow::Color && was.3 != stop.3 {
            return Some(json!({"action": "midpoint", "index": index, "location": stop.3}));
        }
    }
    None
}

fn labels() -> GradientLabels {
    GradientLabels {
        color: tl!("Color").to_string(),
        opacity: tl!("Opacity").to_string(),
        location: tl!("Location").to_string(),
        midpoint_field: tl!("Midpoint").to_string(),
        delete: tl!("Delete").to_string(),
        ..GradientLabels::default()
    }
}

/// A gesture's value, kept with the stops it started from.
#[derive(Clone)]
struct Draft {
    base: GradientStops,
    value: GradientStops,
}

/// Shows the bar and fields for `fill` under `key`. `midpoints` shows the colour midpoint
/// diamonds (a layer's; presets keep theirs centred).
pub(super) fn show(ui: &mut Ui, key: egui::Id, fill: &Fill, midpoints: bool) -> Option<BarAction> {
    let numbers = LocaleNumbers::of(ui);
    let base = stops_of(fill);
    let draft_key = key.with("draft");
    // A draft made against other stops (an undo, or another edit) is stale.
    let draft = ui.data(|d| d.get_temp::<Draft>(draft_key)).filter(|d| d.base == base);
    let shown = draft.as_ref().map_or(&base, |d| &d.value);
    let response = GradientEditor::new(key, tl!("Gradient stops"), shown, &numbers).rows(StopRows::Split).midpoints(midpoints).labels(labels()).show(ui);
    let action = match response.edit.event {
        Some(EditEvent::Update { value, .. }) => {
            let preview = command(&base, &value);
            ui.data_mut(|d| d.insert_temp(draft_key, Draft { base: base.clone(), value }));
            preview.map(BarAction::Preview)
        }
        Some(EditEvent::Commit { value, changed, .. }) => {
            ui.data_mut(|d| d.remove::<Draft>(draft_key));
            Some(command(&base, &value).filter(|_| changed).map_or(BarAction::Clear, BarAction::Commit))
        }
        Some(EditEvent::Set { value, .. }) => {
            ui.data_mut(|d| d.remove::<Draft>(draft_key));
            command(&base, &value).map(BarAction::Commit)
        }
        Some(EditEvent::Cancel { .. }) => {
            ui.data_mut(|d| d.remove::<Draft>(draft_key));
            Some(BarAction::Clear)
        }
        _ => None,
    };
    action.or(response.color_requested.map(BarAction::Color))
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_color::Color;
    use photocraft_doc::GradientStyle;

    fn fill() -> Fill {
        let mut f = Fill::gradient(vec![(0.0, Color::BLACK), (0.5, Color::WHITE), (1.0, Color::BLACK)], 0.0, 1.0, GradientStyle::Linear, false);
        if let Fill::Gradient { opacity_stops, .. } = &mut f {
            *opacity_stops = vec![(0.0, 1.0), (1.0, 0.5)];
        }
        f
    }

    #[test]
    fn edits_read_back_as_the_engine_commands() {
        let old = stops_of(&fill());
        let mut moved = old.clone();
        moved.color[0].location = 0.75;
        moved.color.sort_by(|a, b| a.location.total_cmp(&b.location));
        assert_eq!(command(&old, &moved), Some(json!({"action": "move", "index": 0, "location": 0.75})));
        let mut opacity = old.clone();
        opacity.opacity[1].opacity = 0.25;
        assert_eq!(command(&old, &opacity), Some(json!({"action": "opacity", "kind": "opacity", "index": 1, "opacity": 25.0})));
        let mut added = old.clone();
        added.opacity.insert(1, OpacityStop::new(0.4, 0.8));
        assert_eq!(command(&old, &added).unwrap()["action"], "add");
        let mut deleted = old.clone();
        deleted.color.remove(1);
        assert_eq!(command(&old, &deleted), Some(json!({"action": "delete", "index": 1})));
        let mut midpoint = old.clone();
        midpoint.color[1].midpoint = 0.25;
        assert_eq!(command(&old, &midpoint), Some(json!({"action": "midpoint", "index": 1, "location": 0.25})));
        assert_eq!(command(&old, &old), None);
    }
}
