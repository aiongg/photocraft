//! Preferences › Tools › Pressure Curve: the pen's pressure response, edited as points on a curve
//! (`tools.pressureCurve`, read by the stylus through `photocraft_engine::prefs::PressureCurve`).
//! Brush Settings uses the same editor for each dynamic's own curve ([`edit_points`]).
//! The graph is the library's `CurveEditor`; the presets sit beside it.

use craft_ui::composites::{Curve, CurveEditor, CurveLabels};
use craft_ui::controls::{Button, ButtonVariant, ControlSize};
use craft_ui::edit::EditEvent;
use craft_ui::localized::LocaleNumbers;
use photocraft_engine::prefs::PressureCurve;
use serde_json::{Value, json};

/// The editor's top level of input and output: whole percent.
const LEVELS: f32 = 100.0;
/// Salt of the Preferences page's editor.
const SALT: &str = "pressure-curve";
/// Side of the graph, in points.
const SIDE: f32 = 150.0;

/// Preset curves: Linear, Soft (light pressure already gives a lot) and Hard (it takes a firm
/// press to reach full pressure).
const PRESETS: [(&str, &[[f32; 2]]); 3] =
    [("Linear", &[[0.0, 0.0], [1.0, 1.0]]), ("Soft", &[[0.0, 0.0], [0.3, 0.6], [1.0, 1.0]]), ("Hard", &[[0.0, 0.0], [0.6, 0.3], [1.0, 1.0]])];

/// The points a preferences value holds (anything malformed is skipped; `PressureCurve` sanitises
/// the rest).
pub(crate) fn points_of(v: &Value) -> Vec<[f32; 2]> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    let p = p.as_array()?;
                    Some([p.first()?.as_f64()? as f32, p.get(1)?.as_f64()? as f32])
                })
                .collect()
        })
        .unwrap_or_default()
}

fn value_of(points: &[[f32; 2]]) -> Value {
    // Two decimals: the stored file stays readable and a drag doesn't write float noise.
    json!(points.iter().map(|p| [round2(p[0]), round2(p[1])]).collect::<Vec<_>>())
}

fn round2(x: f32) -> f64 {
    (f64::from(x.clamp(0.0, 1.0)) * 100.0).round() / 100.0
}

/// The editor. Returns the new value when the curve changed.
pub(crate) fn editor(ui: &mut egui::Ui, value: &Value) -> Option<Value> {
    let mut points = points_of(value);
    let changed = edit_points(ui, egui::Id::unique(SALT), &mut points);
    changed.then(|| value_of(&points))
}

/// The editor over a list of points (fewer than two reads as linear), as the library's
/// [`CurveEditor`] over 0..=100 levels, with the presets beside it. `id` salts its state. Returns
/// whether the points changed; they are rounded to two decimals then.
pub(crate) fn edit_points(ui: &mut egui::Ui, id: egui::Id, points: &mut Vec<[f32; 2]>) -> bool {
    if points.len() < 2 {
        *points = vec![[0.0, 0.0], [1.0, 1.0]];
    }
    let mut changed = false;
    let numbers = LocaleNumbers::of(ui);
    let sampler = |pts: &[[f32; 2]]| -> Vec<f32> {
        let unit: Vec<[f32; 2]> = pts.iter().map(|p| [p[0] / LEVELS, p[1] / LEVELS]).collect();
        let curve = PressureCurve::new(&unit);
        (0..=LEVELS as u32).map(|i| curve.eval(i as f32 / LEVELS) * LEVELS).collect()
    };
    // The square with the presets beside it, so the control fits the preferences page's height.
    ui.horizontal_top(|ui| {
        let curve = Curve::Points { points: points.iter().map(|p| [p[0] * LEVELS, p[1] * LEVELS]).collect(), corners: Vec::new() };
        let shown = CurveEditor::new(id, tl!("Pressure curve"), &curve, &numbers)
            .levels(LEVELS as u32)
            .max_points(PressureCurve::MAX_POINTS)
            .sampler(&sampler)
            .side(SIDE)
            .fields(false)
            .labels(CurveLabels {
                input: tl!("Input").into(),
                output: tl!("Output").into(),
                point: tl!("Point {i} of {n}").into(),
                point_value: tl!("Input {x}, Output {y}").into(),
                corner: tl!("Corner").into(),
                corner_value: tl!("Input {x}, Output {y}, corner").into(),
                graph_value: tl!("{channel}, {n} points").into(),
                graph_points: tl!("{n} points").into(),
                graph_drawn: tl!("{channel}, drawn").into(),
                drawn: tl!("Drawn").into(),
                percent: tl!("{n}%").into(),
            })
            .show(ui);
        let before = id.with("before");
        match shown.event {
            Some(EditEvent::Begin { .. }) => {
                ui.data_mut(|d| d.insert_temp(before, points.clone()));
            }
            Some(EditEvent::Update { value, .. } | EditEvent::Set { value, .. }) => changed |= take(points, &value),
            Some(EditEvent::Commit { value, changed: true, .. }) => changed |= take(points, &value),
            Some(EditEvent::Cancel { .. }) => {
                if let Some(was) = ui.data(|d| d.get_temp::<Vec<[f32; 2]>>(before)) {
                    changed |= *points != was;
                    *points = was;
                }
            }
            _ => {}
        }
        ui.vertical(|ui| {
            for (name, preset) in PRESETS {
                if ui.add(Button::new(tl!(name)).variant(ButtonVariant::Secondary).size(ControlSize::Sm)).clicked() {
                    *points = preset.to_vec();
                    changed = true;
                }
            }
        });
    });
    if changed {
        // Two decimals: the stored value stays readable and a drag doesn't write float noise.
        for p in points.iter_mut() {
            *p = [round2(p[0]) as f32, round2(p[1]) as f32];
        }
    }
    changed
}

/// Sets `points` from the library's curve (levels back to 0..1); whether they differ.
fn take(points: &mut Vec<[f32; 2]>, curve: &Curve) -> bool {
    let Curve::Points { points: levels, .. } = curve else { return false };
    let next: Vec<[f32; 2]> = levels.iter().map(|p| [p[0] / LEVELS, p[1] / LEVELS]).collect();
    let differs = next != *points;
    *points = next;
    differs
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn harness(start: Value) -> Harness<'static, Value> {
        let mut h = Harness::builder().with_size(egui::vec2(300.0, 260.0)).build_ui_state(
            |ui, v: &mut Value| {
                if let Some(next) = editor(ui, v) {
                    *v = next;
                }
            },
            start,
        );
        h.run_steps(2);
        h
    }

    #[test]
    fn presets_replace_the_curve() {
        let mut h = harness(json!([[0.0, 0.0], [1.0, 1.0]]));
        h.get_by_label("Soft").click();
        h.run_steps(2);
        assert_eq!(points_of(h.state()), vec![[0.0, 0.0], [0.3, 0.6], [1.0, 1.0]]);
        h.get_by_label("Linear").click();
        h.run_steps(2);
        assert_eq!(points_of(h.state()), vec![[0.0, 0.0], [1.0, 1.0]]);
    }

    #[test]
    fn malformed_stored_values_still_edit() {
        let mut h = harness(json!([[0.5], "x", [2.0, -1.0]]));
        h.get_by_label("Hard").click();
        h.run_steps(2);
        assert_eq!(points_of(h.state()), vec![[0.0, 0.0], [0.6, 0.3], [1.0, 1.0]]);
        assert!(points_of(&json!("nope")).is_empty());
    }
}
