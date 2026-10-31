//! Indexed Color count: an integer field over a slider with the common palette sizes marked.

use super::{Response, Ui};
use craft::theme::ControlSize;
use craft::ui::composites::PropertyRow;
use craft::ui::controls::{NumberField, Slider};
use craft::ui::edit::EditEvent;
use craft::ui::localized::LocaleNumbers;

const STOPS: [u32; 6] = [8, 16, 32, 64, 128, 256];

/// A count in 2..=256. Direct entry allows every integer; slider gestures snap to the marks.
pub fn color_count_row(ui: &mut Ui, label: &str, count: &mut u32) -> Response {
    *count = (*count).clamp(2, 256);
    let before = *count;
    let numbers = LocaleNumbers::of(ui);
    let name = tl!(label);
    let field = PropertyRow::word(name)
        .show(ui, |ui, _| {
            NumberField::new(("color-count-field", name), name, f64::from(*count), &numbers)
                .range(2.0..=256.0)
                .decimals(0..=0)
                .or_parse(super::parse_num)
                .size(ControlSize::Md)
                .show(ui)
        })
        .inner;
    if let Some(EditEvent::Update { value, .. } | EditEvent::Set { value, .. } | EditEvent::Commit { value, .. }) = field.event {
        *count = (value.round() as u32).clamp(2, 256);
    }
    let slider = Slider::new(("color-count-slider", name), name, f64::from(*count), 2.0..=256.0)
        .step(1.0)
        .marks_labelled(STOPS.map(|stop| (f64::from(stop), stop.to_string())))
        .width(ui.available_width())
        .show(ui);
    if let Some(EditEvent::Update { value, .. } | EditEvent::Set { value, .. } | EditEvent::Commit { value, .. }) = slider.event {
        *count = (value.round() as u32).clamp(2, 256);
    }
    let mut response = slider.response.clone();
    if *count != before {
        response.mark_changed();
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::{pos2, vec2};
    use egui_kittest::{Harness, kittest::Queryable};

    fn harness(start: u32) -> Harness<'static, u32> {
        let mut harness = Harness::builder().with_size(vec2(320.0, 100.0)).build_ui_state(
            |ui, count: &mut u32| {
                color_count_row(ui, "Colors", count);
            },
            start,
        );
        crate::PhotocraftApp::setup_context(&harness.ctx, Default::default());
        harness.run_steps(3);
        harness
    }

    #[test]
    fn typed_color_counts_allow_arbitrary_integers_and_arithmetic() {
        for (text, expected) in [("17", 17), ("256", 256), ("(8+8)*2", 32), ("2^8", 256), ("1e2", 100)] {
            let mut harness = harness(64);
            harness.get_by_role(egui::accesskit::Role::SpinButton).click();
            harness.run();
            harness.event(egui::Event::Text(text.into()));
            harness.key_press(egui::Key::Enter);
            harness.run_steps(3);
            assert_eq!(*harness.state(), expected, "{text}");
        }
    }

    #[test]
    fn slider_clicks_snap_to_every_marker_but_allow_intermediate_counts() {
        for count in STOPS {
            let mut harness = harness(2);
            let rect = harness.get_by_role(egui::accesskit::Role::Slider).rect();
            let style = Slider::style(&craft::ui::theme::current(&harness.ctx), ControlSize::Md);
            let (x0, x1) = (rect.left() + style.thumb_width / 2.0, rect.right() - style.thumb_width / 2.0);
            let at = |value: f32| pos2(x0 + (value - 2.0) / 254.0 * (x1 - x0), (rect.top() + style.height / 2.0));
            let point = pos2(at(count as f32).x - 2.0, (rect.top() + style.height / 2.0));
            harness.event(egui::Event::PointerMoved(point));
            for pressed in [true, false] {
                harness.event(egui::Event::PointerButton { pos: point, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
                harness.run_steps(2);
            }
            assert_eq!(*harness.state(), count);
            harness.key_press(egui::Key::ArrowRight);
            harness.run_steps(2);
            assert_eq!(*harness.state(), (count + 1).min(256));
        }
    }
}
