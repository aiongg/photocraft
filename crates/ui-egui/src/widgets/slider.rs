use super::{Color32, Pos2, Response, Ui, Vec2};
use craft_ui::composites::SliderRow;
use craft_ui::controls::{ControlSize, Slider};
use craft_ui::edit::{EditEvent, EditResponse};
use craft_ui::localized::{LocaleNumbers, Unit};

/// Thin-track slider with a round knob over `*value`. `gradient` paints the track (e.g. hue
/// spectrum). The response is changed on every update the user makes.
pub fn slider(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, gradient: Option<&[Color32]>) -> Response {
    slider_named(ui, tl!("Slider"), value, range, gradient)
}

/// [`slider`] named `label` for screen readers and automation.
pub fn slider_named(ui: &mut Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, gradient: Option<&[Color32]>) -> Response {
    let salt = ui.next_auto_id();
    let (lo, hi) = (*range.start(), *range.end());
    let mut slider = Slider::new(salt, label, held(ui, salt).unwrap_or(f64::from(*value)), f64::from(lo)..=f64::from(hi))
        .size(ControlSize::Sm)
        .width(ui.available_width().max(60.0));
    if let Some(stops) = gradient.filter(|g| g.len() >= 2) {
        slider = slider.track_colors(stops.iter().copied());
    }
    let edit = slider.show(ui);
    let mut response = edit.response.clone();
    if apply(ui, salt, &edit, value, lo, hi) {
        response.mark_changed();
    }
    response
}

/// Photoshop's gestures on a slider row beyond click and drag, as measured on Color Balance in
/// Photoshop 25.4 (Image › Adjustments dialog and the adjustment layer's Properties). Shift, Ctrl
/// and Alt change nothing about clicks, double-clicks or drags; the wheel takes ten steps with
/// Shift. (Up/Down in the field are every number field's.)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RowGestures {
    /// A double-click on the slider (knob or track; not the label or field) sets this value.
    pub reset: Option<f32>,
    /// Each wheel notch over the slider moves the value by this step (up = higher; ×10 with
    /// Shift). Photoshop does this in dialogs, not in the Properties panel.
    pub wheel_step: Option<f32>,
    /// A curved track: the knob at fraction f of the track is `lo + (hi - lo) * f^k`, so small
    /// values get fine steps (Photoshop's mask Feather, #6545). Typed, scrubbed and stepped
    /// values stay linear; only the track maps through the curve.
    pub curve: Option<f32>,
}

/// Writes the value an edit event reports into `value` (clamped to `lo..=hi`); a cancelled
/// gesture restores the value it began with. Whether the value changed.
fn apply(ui: &Ui, salt: egui::Id, edit: &EditResponse<f64>, value: &mut f32, lo: f32, hi: f32) -> bool {
    apply_event(ui, salt, edit.event.as_ref(), value, lo, hi)
}

/// The value a gesture under way last reported. The library cancels a gesture whose value the
/// host changed, and callers round, exponentiate or clamp what they store, so the control is
/// given this value until the gesture ends.
fn held(ui: &Ui, salt: egui::Id) -> Option<f64> {
    ui.data(|d| d.get_temp::<f64>(salt.with("pc-held")))
}

fn apply_event(ui: &Ui, salt: egui::Id, event: Option<&EditEvent<f64>>, value: &mut f32, lo: f32, hi: f32) -> bool {
    let before = salt.with("pc-before");
    let held_id = salt.with("pc-held");
    match event {
        Some(EditEvent::Update { value: v, .. }) => {
            ui.data_mut(|d| d.insert_temp(held_id, *v));
        }
        Some(_) => drop(ui.data_mut(|d| d.remove_temp::<f64>(held_id))),
        None => {}
    }
    let (min, max) = (lo.min(hi), lo.max(hi));
    match event {
        Some(EditEvent::Begin { .. }) => {
            let start = *value;
            ui.data_mut(|d| d.insert_temp(before, start));
            false
        }
        Some(EditEvent::Update { value: v, .. } | EditEvent::Set { value: v, .. } | EditEvent::Commit { value: v, changed: true, .. }) => {
            *value = (*v as f32).clamp(min, max);
            true
        }
        Some(EditEvent::Cancel { .. }) => match ui.data(|d| d.get_temp::<f32>(before)) {
            Some(old) => {
                *value = old;
                true
            }
            None => false,
        },
        _ => false,
    }
}

/// Wheel notches of widget `id` while `hovered`, a notch with Shift counting `shift_notches`.
pub(crate) fn wheel_notches_of(ui: &Ui, id: egui::Id, hovered: bool, shift_notches: f32) -> f32 {
    if !hovered {
        return 0.0;
    }
    let wheel: Vec<(egui::MouseWheelUnit, Vec2, bool)> = ui.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::MouseWheel { unit, delta, modifiers, .. } => Some((*unit, *delta, modifiers.shift)),
                _ => None,
            })
            .collect()
    });
    count_notches(ui, id.with("wheel-acc"), &wheel, shift_notches)
}

/// Whole notches in `wheel` (unit, delta, Shift held), a notch with Shift counting
/// `shift_notches`; fractions carry over in egui memory under `acc_id`.
pub(crate) fn count_notches(ui: &Ui, acc_id: egui::Id, wheel: &[(egui::MouseWheelUnit, Vec2, bool)], shift_notches: f32) -> f32 {
    let mut acc: f32 = ui.data(|d| d.get_temp(acc_id)).unwrap_or(0.0);
    let per_line = ui.ctx().options(|o| o.input_options.line_scroll_speed);
    let per_line = if per_line.is_finite() && per_line > 0.0 { per_line } else { 40.0 };
    let mut notches = 0.0;
    for &(unit, delta, shift) in wheel {
        // Some systems turn Shift+wheel into a horizontal scroll.
        let d = if delta.y != 0.0 { delta.y } else { delta.x };
        acc += match unit {
            egui::MouseWheelUnit::Point => d / per_line,
            _ => d,
        };
        let whole = acc.trunc();
        acc -= whole;
        notches += if shift { whole * shift_notches } else { whole };
    }
    if !acc.is_finite() {
        acc = 0.0;
    }
    ui.data_mut(|d| d.insert_temp(acc_id, acc));
    if notches.is_finite() { notches } else { 0.0 }
}

/// Windows' default double-click time (Photoshop's; egui's own default is 0.3 s).
pub(super) const DOUBLE_CLICK_S: f64 = 0.5;

/// Finger jitter between taps easily exceeds egui's 6 px `max_click_dist`; 64 px is about two-thirds of an inch at 96 DPI (roughly a fingertip pad). 32 px still rejected casual Framework double-taps; Windows' mouse SM_CXDOUBLECLK is only 4 px and is not used for touch.
const TOUCH_MAX_CLICK_DIST: f32 = 64.0;

/// True when this frame's pointer work came from a touch contact (still down, or a Touch event including End on the click frame).
fn pointer_from_touch(ui: &Ui) -> bool {
    ui.input(|i| i.any_touches() || i.events.iter().any(|e| matches!(e, egui::Event::Touch { .. })))
}

/// Whether this click on `resp` completes a double-click / double-tap, counted the Windows way (Photoshop): a click soon after a lone click near the same spot; the click after a double starts over.
///
/// Time window is 0.5 s (Windows / Photoshop; egui's default is 0.3 s). Mouse uses egui's `max_click_dist` (6 px); touch uses [`TOUCH_MAX_CLICK_DIST`] so finger jitter between taps still counts. Call on every click for that widget so the first tap is remembered.
pub fn second_click(ui: &Ui, resp: &Response) -> bool {
    if !resp.clicked() {
        return false;
    }
    let key = resp.id.with("pc-last-click");
    // Prefer the response's hit pos: raw `pointer.interact_pos` can be unset on a release-only frame.
    let pos = resp.interact_pointer_pos().or_else(|| ui.input(|i| i.pointer.interact_pos()));
    let time = ui.input(|i| i.time);
    let touch = pointer_from_touch(ui);
    let mouse_dist = ui.ctx().options(|o| o.input_options.max_click_dist);
    let last: Option<(f64, Pos2, bool)> = ui.data(|d| d.get_temp(key));
    // Looser distance if either tap in the pair was touch (End clears `any_touches` but leaves Touch in `events`).
    let dist = if touch || last.is_some_and(|(_, _, t)| t) { TOUCH_MAX_CLICK_DIST.max(mouse_dist) } else { mouse_dist };
    let double = match (last, pos) {
        (Some((t, p, _)), Some(q)) => time - t < DOUBLE_CLICK_S && p.distance(q) < dist,
        _ => false,
    };
    ui.data_mut(|d| {
        if let (false, Some(q)) = (double, pos) {
            d.insert_temp(key, (time, q, touch));
        } else {
            d.remove::<(f64, Pos2, bool)>(key);
        }
    });
    double
}

/// Labelled slider row: `Label ........ [value field]` above a full-width thin slider.
pub fn slider_row(ui: &mut Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, suffix: &str, gradient: Option<&[Color32]>) -> Response {
    slider_row_with(ui, label, value, range, suffix, gradient, RowGestures::default())
}

/// [`slider_row`] with Photoshop's extra gestures ([`RowGestures`]).
pub fn slider_row_with(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    gradient: Option<&[Color32]>,
    g: RowGestures,
) -> Response {
    slider_row_prec(ui, label, value, range, suffix, gradient, g, None)
}

/// [`slider_row_with`] whose field shows `decimals` places instead of the one (or two, for a
/// small range) it picks itself: Exposure's ±20 stops show two, its Offset four (#4164).
#[allow(clippy::too_many_arguments)]
pub fn slider_row_prec(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    gradient: Option<&[Color32]>,
    g: RowGestures,
    decimals: Option<u32>,
) -> Response {
    let numbers = LocaleNumbers::of(ui);
    // Rows repeat labels, so the row's own position names its gestures.
    let salt = ui.next_auto_id();
    let (lo, hi) = (*range.start(), *range.end());
    let mut row = SliderRow::new(salt, tl!(label), held(ui, salt).unwrap_or(f64::from(*value)), f64::from(lo)..=f64::from(hi), &numbers)
        .wheel(g.wheel_step.is_some())
        // `RowGestures::wheel_step` is the slider's step per notch; the library's default is a hundredth of the range.
        .unit(if suffix.is_empty() { Unit::default() } else { Unit::new(&format!("{{n}}{suffix}")) });
    if let Some(step) = g.wheel_step {
        row = row.wheel_step(f64::from(step));
    }
    if let Some(n) = decimals {
        row = row.decimals(n as usize);
    }
    if let Some(reset) = g.reset {
        row = row.default_value(f64::from(reset));
    }
    if let Some(k) = g.curve {
        row = row.curve(f64::from(k));
    }
    if let Some(stops) = gradient.filter(|g| g.len() >= 2) {
        row = row.track_colors(stops.iter().copied());
    }
    let shown = row.show(ui);
    let mut r = shown.response.clone();
    let mut changed = false;
    for event in [shown.field.event.as_ref(), shown.slider.event.as_ref()] {
        changed |= apply_event(ui, salt, event, value, lo, hi);
    }
    if changed {
        r.mark_changed();
    }
    // A scrub of the field or its label is one drag, like the slider's, for history.
    use egui::response::Flags;
    r.flags |= shown.field.response.flags & (Flags::DRAG_STARTED | Flags::DRAGGED | Flags::DRAG_STOPPED);
    r
}
