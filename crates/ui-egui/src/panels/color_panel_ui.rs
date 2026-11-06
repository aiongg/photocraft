//! Photoshop's Color panel, the library's [`ColorPanel`] on the app's tool colours: the panel menu
//! picks the mode and ramp (Hue Cube, Brightness Cube, Color Wheel and the slider sets over the
//! ramp), and an edit recolours what is selected as the tool in hand says (`apply_color`).

use craft::color::ui::{ColorPanel, ColorPanelResponse, PanelTarget};
use craft::theme::Srgba;
use craft::ui::localized::LocaleNumbers;
use craft::ui::menu::MenuEntry;
use serde_json::json;

use crate::PhotocraftApp;
use crate::color_picker_ui::{hex, srgba};

/// The panel menu's and the dock's id of the panel.
const ID_SALT: &str = "color-panel";

/// The Color tab.
pub(crate) fn panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let numbers = LocaleNumbers::of(ui);
    let colors = [tool_color(app.session.tools.foreground), tool_color(app.session.tools.background)];
    let dynamic = app.session.prefs().interface.dynamic_color_sliders;
    let response = ColorPanel::new(ID_SALT, &mut app.ui.color_panel, colors, &numbers).dynamic_tracks(dynamic).show(ui);
    apply(app, ui.ctx(), response);
    crate::apply_color::track_focus(ui.ctx());
}

fn tool_color(c: [f32; 4]) -> Srgba {
    srgba([c[0], c[1], c[2]])
}

/// The colour the panel edits.
#[cfg(test)]
pub(crate) fn active(app: &PhotocraftApp) -> [f32; 3] {
    let c = if app.ui.color_panel.background { app.session.tools.background } else { app.session.tools.foreground };
    [c[0], c[1], c[2]].map(|x| x.clamp(0.0, 1.0))
}

/// What the panel returned: the edits become tool colours, a click on the edited well opens the
/// Color Picker on it, and the wheel's button adds the colour to the swatches.
fn apply(app: &mut PhotocraftApp, ctx: &egui::Context, response: ColorPanelResponse) {
    for (target, event) in &response.edits {
        if let Some(picked) = event.value() {
            set_color(app, ctx, *target == PanelTarget::Background, [picked.color.r, picked.color.g, picked.color.b]);
        }
    }
    if let Some(request) = response.pick_color
        && let Some(target) = PanelTarget::from_key(&request.key)
    {
        crate::color_picker_ui::open(app, if target == PanelTarget::Background { "background" } else { "foreground" });
    }
    if let Some(target) = response.add_to_swatches {
        let c = if target == PanelTarget::Background { app.session.tools.background } else { app.session.tools.foreground };
        if let Err(e) = app.run("swatches.add", json!({"color": hex([c[0], c[1], c[2]])})) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
    }
}

/// Set the background colour, or the foreground and recolour what is selected as the tool in hand
/// says (`apply_color`); the picks of one drag or typed value are one history step. A held,
/// unmoved press repeats its pick: selected type is restyled on a change only.
pub(crate) fn set_color(app: &mut PhotocraftApp, ctx: &egui::Context, background: bool, rgb: [f32; 3]) {
    let c = [rgb[0], rgb[1], rgb[2], 1.0];
    if background {
        app.session.tools.background = c;
    } else if app.session.tools.foreground != c {
        app.session.tools.foreground = c;
        crate::apply_color::foreground_changed(app, crate::apply_color::gesture(ctx));
    }
}

/// The Color panel's panel menu rows, the library's ([`ColorPanel::panel_menu`]) in the legacy
/// panel-menu hook with a check column. The dock's `DockContent::panel_menu_entries` would show
/// them in the shared menu, but `DockArea::show_customized` does not pass that hook on (gap DW).
pub(crate) fn panel_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let entries = ColorPanel::panel_menu(ui.ctx(), ID_SALT, &app.ui.color_panel);
    for entry in entries {
        match entry {
            MenuEntry::Separator => {
                ui.separator();
            }
            MenuEntry::Item(item) => {
                let Some(command) = item.command().cloned() else { continue };
                let mark = if item.item_kind().checked() == Some(true) { "✔ " } else { "   " };
                if ui.button(format!("{mark}{}", item.label())).clicked() {
                    let c = if app.ui.color_panel.background { app.session.tools.background } else { app.session.tools.foreground };
                    ColorPanel::run_command(ui.ctx(), ID_SALT, &mut app.ui.color_panel, &command, tool_color(c));
                    ui.close();
                }
            }
            _ => {}
        }
    }
}

/// An older Color panel state kept the Color Picker's Only Web Colors as `webOnly` (`web_only`
/// before the camelCase rename). The panel's own web settings (Make Ramp Web Safe, the Web Color
/// sliders) are separate, so the value goes to the picker's memory (`colorPicker.memory`,
/// `webColors`), once: a picker that already remembers its choice keeps it.
pub(crate) fn carry_web_only(app: &mut PhotocraftApp, saved: &serde_json::Value) {
    let Some(on) = saved.get("webOnly").or_else(|| saved.get("web_only")).and_then(serde_json::Value::as_bool) else { return };
    let mut memory = crate::host_settings::recall(app, "colorPicker.memory").unwrap_or_else(|| json!({}));
    let Some(map) = memory.as_object_mut() else { return };
    if map.contains_key("webColors") {
        return;
    }
    map.insert("webColors".into(), json!(on));
    crate::host_settings::remember(app, "colorPicker.memory", &memory);
}
