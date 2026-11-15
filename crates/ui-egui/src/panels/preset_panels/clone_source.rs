//! The Clone Source panel: five source slots, the slot's offset, scale, rotation and flips, and the overlay options.
//! Every edit is a `cloneSource.*` engine command.

use craft::behavior::{EditEvent, GestureId, Mixed};
use craft::ui::buttons::{Button, ToggleButton};
use craft::ui::choice::Checkbox;
use craft::ui::contract::{Bind, Entry, Outcome as _, Props as _, Show as _, ShowContainer as _};
use craft::ui::display::Text;
use craft::ui::form::{FieldGroup, SliderField};
use craft::ui::pickers::Select;
use craft::ui::runtime::env::UnitKind;
use craft::ui::text::{LinkedFields, NumberField};
use serde_json::{Value, json};

use super::run;
use crate::PhotocraftApp;
use crate::state::Tool;

/// Runs `command`; a value edit of one gesture shares one undo step through the gesture's id.
fn edit(app: &mut PhotocraftApp, command: &str, mut params: Value, gesture: Option<GestureId>) {
    if let Some(id) = gesture {
        params["coalesce"] = json!(format!("clone-source-{}", id.0));
    }
    run(app, command, params);
}

/// A number field of `value`: the value an edit landed on with its gesture, `None` when nothing changed.
fn number(ui: &mut egui::Ui, label: &str, value: f64, range: std::ops::RangeInclusive<f64>, unit: UnitKind) -> Option<(f64, Option<GestureId>)> {
    let mut shown = Some(value);
    let out = NumberField::new(label, Bind::bound(&mut shown)).range(range).unit_kind(unit).show(ui);
    let landed = (*out.events.landed()?)?;
    ((landed - value).abs() > 1e-9).then(|| (landed, out.events.last().and_then(EditEvent::id)))
}

/// A toggle button of `value`: the new state when a click changed it.
fn toggle(ui: &mut egui::Ui, label: &str, value: bool) -> Option<bool> {
    let mut on = value;
    let out = ToggleButton::new(label, Bind::bound(&mut on)).show(ui);
    out.events.landed().copied().filter(|&now| now != value)
}

/// The `cloneSource` registry body.
pub fn panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let cs = app.session.presets.clone.clone();
    let slot = cs.active().clone();
    let mut select = None;
    ui.horizontal(|ui| {
        for i in 0..photocraft_engine::presets::clone_source::SLOTS {
            let mut on = cs.active == i;
            let out = ToggleButton::new(format!("{}", i + 1), Bind::bound(&mut on)).show(ui);
            if out.events.landed() == Some(&true) && cs.active != i {
                select = Some(i);
            }
        }
    });
    if let Some(index) = select {
        run(app, "cloneSource.select", json!({"index": index}));
    }
    let layer_name = slot.layer.and_then(|l| app.session.active().and_then(|d| d.doc.layer(photocraft_doc::LayerId(l)).map(|x| x.name.clone())));
    let source_line = match (slot.source, layer_name) {
        (Some(_), Some(l)) => format!("Source: {} : {l}", app.session.active().map(|d| d.doc.name.clone()).unwrap_or_default()),
        (Some(_), None) => "Source: set".to_string(),
        _ => crate::i18n::fmt(tl!("Source: not set ({key}-click with Clone Stamp)"), &[("key", &crate::shortcuts::pretty("Alt"))]),
    };
    Text::new(source_line).show(ui);

    let mut set: Option<(Value, Option<GestureId>)> = None;
    let off = slot.offset().unwrap_or([0.0, 0.0]);
    FieldGroup::new(tl!("Transform")).show(ui, |ui, _| {
        if slot.source.is_some() {
            if let Some((x, id)) = number(ui, "X", off[0], -30000.0..=30000.0, UnitKind::Pixels) {
                set = Some((json!({"offset": [x, off[1]]}), id));
            }
            if let Some((y, id)) = number(ui, "Y", off[1], -30000.0..=30000.0, UnitKind::Pixels) {
                set = Some((json!({"offset": [off[0], y]}), id));
            }
        }
        // The W/H chain: with it on, a new W or H scales the other by the same factor.
        let mut size = Mixed::each([Some(slot.scale[0]), Some(slot.scale[1])]);
        let mut linked = slot.linked;
        let out = LinkedFields::new(tl!("Size"), Bind::bound(&mut size), Bind::bound(&mut linked)).captions(["W", "H"]).range(-1000.0..=1000.0).show(ui);
        if let Some(new) = out.edit.events.landed() {
            let gesture = out.edit.events.last().and_then(EditEvent::id);
            if let Some(w) = new[0].same().copied().flatten().filter(|w| (w - slot.scale[0]).abs() > 1e-9) {
                set = Some((json!({"width": w}), gesture));
            } else if let Some(h) = new[1].same().copied().flatten().filter(|h| (h - slot.scale[1]).abs() > 1e-9) {
                set = Some((json!({"height": h}), gesture));
            }
        }
        if let Some(&now) = out.link.landed().filter(|&&now| now != slot.linked) {
            set = Some((json!({"linked": now}), None));
        }
        if let Some((angle, id)) = number(ui, "A", slot.rotation, -180.0..=180.0, UnitKind::Degrees) {
            set = Some((json!({"rotation": angle}), id));
        }
        if let Some(now) = toggle(ui, tl!("Flip Horizontal"), slot.flip_h) {
            set = Some((json!({"flipH": now}), None));
        }
        if let Some(now) = toggle(ui, tl!("Flip Vertical"), slot.flip_v) {
            set = Some((json!({"flipV": now}), None));
        }
    });
    if let Some((params, gesture)) = set {
        edit(app, "cloneSource.set", params, gesture);
    }
    if Button::new(tl!("Reset Transform")).show(ui).response().clicked() {
        run(app, "cloneSource.resetTransform", json!({}));
    }

    // Overlay options.
    let mut o = cs.overlay.clone();
    let before = o.clone();
    let mut opacity = o.opacity as f64;
    let mut gesture = None;
    Checkbox::new(Bind::bound(&mut o.show)).label(tl!("Show Overlay")).show(ui);
    let slider = SliderField::new(tl!("Opacity"), Bind::bound(&mut opacity), 0.0..=100.0).unit_kind(UnitKind::Percent).show(ui);
    if slider.events.landed().is_some() {
        gesture = slider.events.last().and_then(EditEvent::id);
        o.opacity = opacity as f32;
    }
    let modes = [("normal", tl!("Normal")), ("darken", tl!("Darken")), ("lighten", tl!("Lighten")), ("difference", tl!("Difference"))]
        .map(|(value, label)| Entry::new(value.to_string(), label));
    Select::new(tl!("Blend Mode"), Bind::bound(&mut o.blend), &modes).show(ui);
    Checkbox::new(Bind::bound(&mut o.clipped)).label(tl!("Clipped")).show(ui);
    Checkbox::new(Bind::bound(&mut o.auto_hide)).label(tl!("Auto Hide")).show(ui);
    Checkbox::new(Bind::bound(&mut o.invert)).label(tl!("Invert")).show(ui);
    if o != before {
        let params = json!({"show": o.show, "opacity": o.opacity, "clipped": o.clipped, "autoHide": o.auto_hide, "invert": o.invert, "blend": o.blend});
        edit(app, "cloneSource.overlay", params, gesture);
    }
}

/// Where the active clone source samples for document point `at` (for the canvas marker).
/// The first stroke anchors at its press, so the marker follows before the stroke commits.
pub fn clone_sample_point(app: &PhotocraftApp, at: Option<[f64; 2]>) -> Option<[f64; 2]> {
    use photocraft_engine::presets::clone_source::{Mapping, transform_matrix};
    let s = app.session.presets.clone.active();
    let src = s.source?;
    // PLAN: assumed `paint_tool::stroking(app) -> Option<&PaintStroke>` (cw.P9) with `PaintStroke { tool: Tool, start: [f64; 2], .. }`.
    let stroking = crate::canvas::paint_tool::stroking(app).filter(|stroke| matches!(stroke.tool, Tool::CloneStamp | Tool::Healing)).map(|stroke| stroke.start);
    let anchor = if app.ui.tool_options.clone_aligned { s.anchor.or(stroking) } else { stroking };
    match (anchor, at) {
        (Some(a), Some(h)) => {
            let m = Mapping { source: (src[0], src[1]), anchor: (a[0], a[1]), m: transform_matrix(s.scale, s.rotation, s.flip_h, s.flip_v) };
            let (x, y) = m.map(h[0], h[1]);
            Some([x, y])
        }
        _ => Some(src),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::testing;

    #[test]
    fn clone_marker_follows_the_engine_slot() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": 64, "height": 48})).unwrap();
        app.sync_views();
        assert_eq!(clone_sample_point(&app, Some([5.0, 5.0])), None);
        app.run("cloneSource.set", json!({"source": [10, 10]})).unwrap();
        assert_eq!(clone_sample_point(&app, Some([30.0, 30.0])), Some([10.0, 10.0]));
        app.ui.tool = Tool::CloneStamp;
        testing::frame(&mut app);
        testing::press(&mut app, [40.0, 40.0], egui::Modifiers::NONE);
        assert_eq!(clone_sample_point(&app, Some([50.0, 30.0])), Some([20.0, 0.0]));
        testing::release(&mut app, [40.0, 40.0], egui::Modifiers::NONE);
        app.session.presets.clone.active_mut().anchor = Some([20.0, 20.0]);
        assert_eq!(clone_sample_point(&app, Some([30.0, 30.0])), Some([20.0, 20.0]));
    }
}
