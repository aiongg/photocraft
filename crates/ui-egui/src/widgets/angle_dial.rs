//! Angle dial row: `Label (dial) [field°]` for a direction in document angles (counter-clockwise
//! from three o'clock, y up), on the library `AngleDial`. Drop Shadow's light (#882), Motion Blur
//! and the other direction angles of the filter dialogs (#3152) share it.

use super::{Response, Ui};
use craft_ui::controls::AngleDial;
use craft_ui::edit::EditEvent;
use craft_ui::localized::LocaleNumbers;
use craft_ui::primitives::{Content, Text};

/// `Label (dial) [field°]`. The dial points anywhere in -180..=180 and the result is clamped to
/// `range`; the field accepts the whole of `range` (Motion Blur takes -360..=360, so a typed or
/// remembered 270° stays 270°). The response is changed on every update the user makes.
pub fn angle_dial_row(ui: &mut Ui, label: &str, angle: &mut f32, range: std::ops::RangeInclusive<f32>) -> Response {
    let numbers = LocaleNumbers::of(ui);
    // A malformed spec may reverse the range.
    let (lo, hi) = (f64::from(*range.start()).min(f64::from(*range.end())), f64::from(*range.start()).max(f64::from(*range.end())));
    ui.horizontal(|ui| {
        ui.add(Text::new(tl!(label)).content(Content::Muted));
        let edit = AngleDial::new(("angle-dial", label), tl!(label), f64::from(*angle), &numbers)
            .range(lo..=hi)
            // A range under one turn (a line's -90..=90) wraps; a full turn wraps by default, and a wider range stops at its ends.
            .wrap(hi - lo < 360.0 - 1e-9)
            .show(ui);
        let mut response = edit.response.clone();
        if let Some(EditEvent::Update { value, .. } | EditEvent::Set { value, .. } | EditEvent::Commit { value, changed: true, .. }) = edit.event {
            *angle = value as f32;
            response.mark_changed();
        }
        response
    })
    .inner
}
