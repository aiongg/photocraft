//! Channels panel › Channel Options… (double-click an alpha or spot channel's thumbnail, or the
//! Quick Mask row): name, Color Indicates Masked Areas / Selected Areas / Spot Color, the overlay
//! or ink colour, and opacity (solidity for spot channels). OK runs the engine's
//! `channel.options`, which agents can also call directly with params (#3761).
//!
//! On the layer mask (double-click its Channels row, or Mask Options… on its Layers thumbnail)
//! it is Photoshop's Layer Mask Display Options: just the rubylith's colour and opacity.

use craft::ui::composites::FormGrid;
use craft::ui::controls::RadioGroup;
use craft::ui::primitives::Divider;
use photocraft_doc::ColorIndicates;
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;

pub const COMMAND: &str = "channel.options";
const MARK: &str = "__channelOptions";

pub fn owns(fields: &Map<String, Value>) -> bool {
    fields.contains_key(MARK)
}

fn hex(rgb: [f32; 3]) -> String {
    crate::color_picker_ui::hex(rgb)
}

/// Whether `f` is Layer Mask Display Options rather than a channel's options.
fn layer_mask(f: &Map<String, Value>) -> bool {
    f.get("channel").and_then(Value::as_str) == Some("mask")
}

/// The dialog's fields for `channel` (an alpha/spot channel index, `"quickMask"`, or `"mask"` for
/// Layer Mask Display Options), or `None` when there's no document or no such channel.
pub fn fields(app: &PhotocraftApp, channel: &Value) -> Option<Map<String, Value>> {
    let doc = &app.session.active()?.doc;
    let kind = |i: ColorIndicates| if i == ColorIndicates::SelectedAreas { "selected" } else { "masked" };
    if channel.as_str() == Some("mask") {
        let o = &app.session.prefs().layer_mask_overlay;
        let mut f = Map::new();
        f.insert(MARK.into(), json!(true));
        f.insert("__command".into(), json!(COMMAND));
        f.insert("__label".into(), json!("Layer Mask Display Options…"));
        f.insert("channel".into(), json!("mask"));
        f.insert("color".into(), json!(hex(o.rgb())));
        f.insert("opacity".into(), json!((o.alpha() * 100.0).round()));
        return Some(f);
    }
    let (name, indicates, color, percent) = if channel.as_str() == Some("quickMask") {
        let o = app.session.quick_mask_options;
        (None, kind(o.indicates), o.color, o.opacity)
    } else {
        let ch = doc.channels.get(usize::try_from(channel.as_u64()?).ok()?)?;
        match ch.spot {
            Some((ink, solidity)) => (Some(ch.name.clone()), "spot", ink, solidity),
            None => (Some(ch.name.clone()), kind(ch.indicates), ch.color, ch.opacity),
        }
    };
    let mut f = Map::new();
    f.insert(MARK.into(), json!(true));
    f.insert("__command".into(), json!(COMMAND));
    f.insert("__label".into(), json!(if name.is_some() { "Channel Options…" } else { "Quick Mask Options…" }));
    f.insert("channel".into(), channel.clone());
    if let Some(n) = name {
        f.insert("name".into(), json!(n));
    }
    f.insert("indicates".into(), json!(indicates));
    f.insert("color".into(), json!(hex(color.to_rgb())));
    f.insert("opacity".into(), json!((percent.clamp(0.0, 1.0) * 100.0).round()));
    Some(f)
}

/// Opens Channel Options on `channel`, or `None` if it doesn't exist.
pub fn open(app: &mut PhotocraftApp, channel: &Value) -> Option<u64> {
    let f = fields(app, channel)?;
    Some(app.ui.open_dialog(crate::state::DialogKind::Command, f))
}

/// The `channel.options` params for the dialog's fields: spot channels take `solidity`, the
/// others `opacity`.
pub fn params(f: &Map<String, Value>) -> Value {
    let spot = f.get("indicates").and_then(Value::as_str) == Some("spot");
    let mut p = Map::new();
    for key in ["channel", "name", "indicates", "color"] {
        if let Some(v) = f.get(key) {
            p.insert(key.into(), v.clone());
        }
    }
    if let Some(v) = f.get("opacity") {
        p.insert(if spot { "solidity" } else { "opacity" }.into(), v.clone());
    }
    Value::Object(p)
}

pub fn confirm(app: &mut PhotocraftApp, f: &Map<String, Value>) -> Result<Value, String> {
    app.run(COMMAND, params(f))
}

pub fn body(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    ui.spacing_mut().item_spacing.y = 6.0;
    if layer_mask(f) {
        return mask_body(ui, f);
    }
    let quick = f.get("channel").and_then(Value::as_str) == Some("quickMask");
    name_row(ui, f);
    craft::ui::composites::SectionLabel::new(tl!("Color Indicates:")).show(ui);
    Divider::horizontal().show(ui);
    let mut indicates = f.get("indicates").and_then(Value::as_str).unwrap_or("masked").to_string();
    let before = indicates.clone();
    ui.indent("channel-options-indicates", |ui| {
        let mut group = RadioGroup::new("channel-options-indicates", tl!("Color Indicates:"), &mut indicates)
            .item("masked".to_string(), tl!("Masked Areas"))
            .item("selected".to_string(), tl!("Selected Areas"));
        // Quick Mask can't be a spot channel.
        if !quick {
            group = group.item("spot".to_string(), tl!("Spot Color"));
        }
        _ = group.show(ui);
    });
    if indicates != before {
        f.insert("indicates".into(), json!(indicates));
    }
    let spot = indicates == "spot";
    ui.add_space(craft::ui::theme::scale::Space::S4.px());
    craft::ui::composites::SectionLabel::new(tl!("Color")).show(ui);
    Divider::horizontal().show(ui);
    let what = if spot { tl!("Spot Color") } else { tl!("Channel Color") };
    let caption = if spot { tl!("Solidity:") } else { tl!("Opacity:") };
    FormGrid::new("channel-options-color").show(ui, |form| {
        form.unlabelled(|ui| color_and_amount(ui, f, ("color", what), ("opacity", caption), 50.0));
    });
}

/// A colour swatch that opens the Color Picker for `f[color.0]`, then a captioned percentage field
/// over `f[amount.0]`. Each pair is the field's key and its name.
fn color_and_amount(ui: &mut egui::Ui, f: &mut Map<String, Value>, color: (&str, &str), amount: (&str, &str), default: f32) -> egui::Response {
    ui.horizontal(|ui| {
        let current = f.get(color.0).and_then(Value::as_str).and_then(crate::color_picker_ui::parse_hex).unwrap_or([1.0, 0.0, 0.0]);
        let c = current.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
        let swatch = crate::widgets::color_swatch_button(ui, egui::Color32::from_rgb(c[0], c[1], c[2]), color.1);
        if swatch.clicked() {
            crate::color_picker_ui::request(f, &crate::color_picker_ui::title_for(color.1));
        }
        ui.add_space(craft::ui::theme::scale::Space::S12.px());
        ui.label(amount.1);
        let mut pct = f.get(amount.0).and_then(Value::as_f64).unwrap_or(f64::from(default)) as f32;
        if crate::widgets::dialog_number(ui, ("channel-options", amount.0), amount.1, &mut pct, 0.0..=100.0, "{n}%", 0) {
            f.insert(amount.0.into(), json!(pct.round()));
        }
        swatch
    })
    .inner
}

/// The Name row shared by Channel Options and New Spot Channel, when the fields carry a name.
fn name_row(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let Some(mut name) = f.get("name").and_then(Value::as_str).map(str::to_string) else { return };
    FormGrid::new("channel-options-name").show(ui, |form| {
        form.row(tl!("Name:"), |ui, _| {
            let edit = crate::widgets::dialog_text(ui, "channel-options-name", tl!("Name"), &mut name);
            if edit.changed {
                f.insert("name".into(), json!(name));
            }
            edit.response
        });
    });
}

/// Layer Mask Display Options: Color and Opacity, as Photoshop's dialog has them.
fn mask_body(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    FormGrid::new("channel-options-mask").show(ui, |form| {
        form.row(tl!("Color:"), |ui, _| color_and_amount(ui, f, ("color", tl!("Overlay Color")), ("opacity", tl!("Opacity:")), 50.0));
    });
}

// ---------------------------------------------------------------------------------------------
// New Spot Channel…: Name and Ink Characteristics (Color, Solidity). OK runs `channel.newSpot`
// (from the selection, as Photoshop does) and remembers the ink and solidity for next time.

pub const NEW_SPOT: &str = "channel.newSpot";
const NEW_SPOT_MARK: &str = "__newSpot";

pub fn owns_new_spot(fields: &Map<String, Value>) -> bool {
    fields.contains_key(NEW_SPOT_MARK)
}

/// The New Spot Channel fields: the next "Spot Color N" name and the last ink and solidity
/// (red and 0% at first), or `None` without a document.
pub fn new_spot_fields(app: &PhotocraftApp) -> Option<Map<String, Value>> {
    let doc = &app.session.active()?.doc;
    let saved = app.session.prefs().dialogs.get(NEW_SPOT).cloned().unwrap_or_default();
    let color = saved.get("color").and_then(Value::as_str).filter(|h| crate::color_picker_ui::parse_hex(h).is_some()).unwrap_or("#ff0000");
    let solidity = saved.get("solidity").and_then(Value::as_f64).filter(|v| (0.0..=100.0).contains(v)).unwrap_or(0.0);
    let mut f = Map::new();
    f.insert(NEW_SPOT_MARK.into(), json!(true));
    f.insert("__command".into(), json!(NEW_SPOT));
    f.insert("__label".into(), json!("New Spot Channel…"));
    f.insert("name".into(), json!(photocraft_engine::channel_cmds::next_alpha_name(doc, "Spot Color")));
    f.insert("color".into(), json!(color));
    f.insert("solidity".into(), json!(solidity));
    Some(f)
}

/// Opens New Spot Channel, or `None` without a document.
pub fn open_new_spot(app: &mut PhotocraftApp) -> Option<u64> {
    let f = new_spot_fields(app)?;
    Some(app.ui.open_dialog(crate::state::DialogKind::Command, f))
}

pub fn new_spot_params(f: &Map<String, Value>) -> Value {
    let mut p = Map::new();
    for key in ["name", "color", "solidity"] {
        if let Some(v) = f.get(key) {
            p.insert(key.into(), v.clone());
        }
    }
    Value::Object(p)
}

pub fn confirm_new_spot(app: &mut PhotocraftApp, f: &Map<String, Value>) -> Result<Value, String> {
    let p = new_spot_params(f);
    let result = app.run(NEW_SPOT, p.clone());
    if result.is_ok() {
        let remembered = json!({ "color": p.get("color"), "solidity": p.get("solidity") });
        app.session.prefs.edit(|prefs| prefs.dialogs.insert(NEW_SPOT.into(), remembered));
    }
    result
}

pub fn new_spot_body(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    ui.spacing_mut().item_spacing.y = 6.0;
    name_row(ui, f);
    craft::ui::composites::SectionLabel::new(tl!("Ink Characteristics")).show(ui);
    Divider::horizontal().show(ui);
    FormGrid::new("channel-options-ink").show(ui, |form| {
        form.row(tl!("Color:"), |ui, _| color_and_amount(ui, f, ("color", tl!("Spot Color")), ("solidity", tl!("Solidity:")), 0.0));
    });
}

#[cfg(test)]
mod tests;
