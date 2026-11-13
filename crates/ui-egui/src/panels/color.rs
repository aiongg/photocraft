//! The Color panel on the framework: `ColorPanel` over the tool colours, its edits run as
//! `tools.setColors` (one history step per gesture) and recolour what is selected (`apply_color`).

use craft::behavior::EditEvent;
use craft::color::ui::{ColorPanel, ColorPanelCommand, ColorPanelMenu, PanelTarget, PickedColor};
use craft::theme::Srgba;
use craft::ui::contract::Bind;
use craft::ui::menu::PanelRow;
use craft::ui::runtime::commands::CommandSet as _;
use serde_json::json;

use super::PhotocraftApp;

/// The Color tab: the panel in the view its panel menu picked (#6000).
pub(crate) fn panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let colors = [srgba(app.session.tools.foreground), srgba(app.session.tools.background)];
    let dynamic = app.session.prefs().interface.dynamic_color_sliders;
    let out = ColorPanel::new(Bind::bound(&mut app.ui.color_panel.panel), colors).dynamic_tracks(dynamic).show(ui);
    for (target, event) in out.edits {
        set_color(app, target, &event);
    }
    if let Some(target) = out.pick_color.and_then(|request| PanelTarget::from_key(&request.key)) {
        crate::color_picker_ui::open(app, target_name(target));
    }
    if let Some(target) = out.add_to_swatches {
        let color = colors[usize::from(target == PanelTarget::Background)];
        if let Err(e) = app.run("swatches.add", json!({"color": color.to_hex()})) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
    }
}

/// One edit of a colour: `tools.setColors` under the gesture's coalesce key, so a drag or a
/// typed value is one history step; a new foreground recolours what is selected.
fn set_color(app: &mut PhotocraftApp, target: PanelTarget, event: &EditEvent<PickedColor>) {
    let Some(picked) = event.landed() else { return };
    let key = match event.id() {
        Some(id) => format!("color-panel-{}", id.0),
        None => format!("color-panel-set-{}", app.ui.alloc_id()),
    };
    let c = picked.color;
    let mut params = json!({"coalesce": key});
    params[target_name(target)] = json!([c.r, c.g, c.b, c.a]);
    if let Err(e) = app.run("tools.setColors", params) {
        app.ui.status = e;
        app.ui.status_error = true;
        return;
    }
    if target == PanelTarget::Foreground {
        crate::apply_color::foreground_changed(app, Some(key));
    }
}

fn target_name(target: PanelTarget) -> &'static str {
    match target {
        PanelTarget::Background => "background",
        _ => "foreground",
    }
}

/// A tool colour (sRGB-encoded RGBA) as the framework's colour.
pub(crate) fn srgba(c: [f32; 4]) -> Srgba {
    Srgba::new(c[0], c[1], c[2], c[3])
}

/// The Color panel's panel menu (the dock's menus are still drawn by PhotoCraft): the rows,
/// checks and commands of `ColorPanelMenu`.
pub(crate) fn panel_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let defs = ColorPanelCommand::defs().unwrap_or_default();
    let states = ColorPanelMenu::states(&app.ui.color_panel.panel);
    for row in ColorPanelMenu::rows() {
        let PanelRow::Command(command) = row else {
            ui.separator();
            continue;
        };
        let Some(id) = ColorPanelCommand::from_ref(&command) else { continue };
        let label = defs.iter().find(|d| d.command == command).map_or("", |d| d.label.as_str());
        let checked = states.iter().find(|(c, _)| *c == id).and_then(|(_, s)| s.checked);
        let text = match checked {
            Some(on) => format!("{}{}", if on { "✔ " } else { "   " }, tl!(label)),
            None => tl!(label).to_string(),
        };
        if ui.button(text).clicked() {
            if let Some(text) = run_menu_command(app, id) {
                ui.ctx().copy_text(text);
            }
            ui.close();
        }
    }
}

/// Runs a Color panel menu command on the panel state; Copy Color as HTML and Copy Color's Hex
/// Code return the clipboard text of the edited colour.
pub(crate) fn run_menu_command(app: &mut PhotocraftApp, command: ColorPanelCommand) -> Option<String> {
    let tools = &app.session.tools;
    let edited = srgba(if app.ui.color_panel.panel.background { tools.background } else { tools.foreground });
    ColorPanelMenu::run_command(&mut app.ui.color_panel.panel, command, edited)
}
