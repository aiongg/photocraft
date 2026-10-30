//! Indexed Color count: an integer field over a slider with the common palette sizes marked.

use super::{Response, Ui, pos2};
use craft_theme::{ControlSize, TypeRole};
use craft_ui::composites::PropertyRow;
use craft_ui::controls::{NumberField, Slider};
use craft_ui::edit::EditEvent;
use craft_ui::localized::LocaleNumbers;
use craft_ui::primitives::{Content, Text};
use craft_ui::theme::{Tokens as LibTokens, TokensOf as _};

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
    let marks = STOPS.map(f64::from);
    let slider = Slider::new(("color-count-slider", name), name, f64::from(*count), 2.0..=256.0).step(1.0).marks(&marks).width(ui.available_width()).show(ui);
    if let Some(EditEvent::Update { value, .. } | EditEvent::Set { value, .. } | EditEvent::Commit { value, .. }) = slider.event {
        *count = (value.round() as u32).clamp(2, 256);
    }
    let mut response = slider.response.clone();
    // The library draws the marks as ticks; the sizes are written under them.
    let tokens = LibTokens::of(ui);
    let style = Slider::style(&tokens, ControlSize::Md);
    let format = Text::format(&tokens, TypeRole::Caption, Content::Faint);
    let rect = response.rect;
    let (x0, x1) = (rect.left() + style.thumb_width / 2.0, rect.right() - style.thumb_width / 2.0);
    let top = rect.center().y + style.track_height / 2.0 + 2.0 * style.mark;
    for stop in STOPS {
        let x = x0 + (stop - 2) as f32 / 254.0 * (x1 - x0);
        let galley = ui.painter().layout_no_wrap(stop.to_string(), format.font_id.clone(), format.color);
        let left = if stop == 256 { x - galley.size().x } else { x - galley.size().x / 2.0 };
        ui.painter().galley(pos2(left, top), galley, format.color);
    }
    ui.add_space(top + 12.0 - rect.bottom());
    if *count != before {
        response.mark_changed();
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::vec2;
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
            let style = Slider::style(&craft_ui::theme::current(&harness.ctx), ControlSize::Md);
            let (x0, x1) = (rect.left() + style.thumb_width / 2.0, rect.right() - style.thumb_width / 2.0);
            let at = |value: f32| pos2(x0 + (value - 2.0) / 254.0 * (x1 - x0), rect.center().y);
            let point = pos2(at(count as f32).x - 2.0, rect.center().y);
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
