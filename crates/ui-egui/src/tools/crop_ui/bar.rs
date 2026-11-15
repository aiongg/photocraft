//! The Crop tool's options as the framework `CropBar`, and the preset list of the task bar. One handler turns the cells'
//! outputs into app calls.

use craft::behavior::geometry::CropBox;
use craft::draw::ui::crop::{CropBar, CropPresetList, CropRatioChange};
use craft::ui::dialog::{Alert, HostedAlert};
use craft::ui::runtime::env::Env;

use super::{document, handle_event, refit, reset, toggle_straighten, words};
use crate::PhotocraftApp;
use crate::state::Tool;

/// The Crop options inside the options bar's `Bar`.
pub(crate) fn bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let Some(document) = document(app) else { return };
    let mut settings = app.ui.tool_options.crop.clone();
    let users = crate::crop_presets::user_presets(app);
    let (armed, angle, modified, entry) = {
        let tool = &app.canvas_tools.crop.tool;
        (tool.straighten_armed(), tool.frame().map(|f| f.angle), tool.modified(), tool.shows_selection_entry(&settings))
    };
    let words = words(app);
    let out = CropBar::new(&mut settings, &document)
        .user_presets(&users)
        .straighten(armed)
        .angle(angle)
        .selection_entry(entry)
        .selection_chosen(entry)
        .can_reset(modified)
        .help(words.fill_help.clone())
        .show(ui);
    if out.settings_changed || out.ratio.is_some() || out.selection {
        app.ui.tool_options.crop = settings.clone();
    }
    preset_outputs(app, out.ratio, out.new_preset, out.delete_preset, out.selection);
    if out.straighten {
        toggle_straighten(app);
    }
    if let Some(angle) = out.angle
        && let Some(frame) = app.canvas_tools.crop.tool.frame()
    {
        let event = app.canvas_tools.crop.tool.crop_to(CropBox::new(frame.rect, angle));
        handle_event(app, event);
    }
    if out.pick_color.is_some() {
        let rgb = settings.shield.custom_color.map(|v| f32::from(v) / 255.0);
        crate::color_picker_ui::open_for_target(app, "cropShield", "Color Picker (Crop Shield Color)", rgb);
    }
    if out.reset {
        reset(app);
    }
    if out.help {
        let message = format!(
            "{} {}\n{} {}",
            words.fill_help_background.as_str(),
            words.fill_help_background_text.as_str(),
            words.fill_help_content_aware.as_str(),
            words.fill_help_content_aware_text.as_str()
        );
        let alert = Alert::new(words.fill_help_title.clone(), message);
        let _ = app.framework.dialogs.open_unless_open(HostedAlert::new(alert));
    }
    if let Some(range) = out.out_of_range {
        let alert = Alert::range(tl!("PhotoCraft").to_owned(), &range, &Env::of(ui));
        // An `Err` is the alert that was not opened because one is already up.
        let _ = app.framework.dialogs.open_unless_open(HostedAlert::new(alert));
    }
}

/// The task bar's preset list.
pub(crate) fn preset_list(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let Some(document) = document(app) else { return };
    let mut settings = app.ui.tool_options.crop.clone();
    let users = crate::crop_presets::user_presets(app);
    let entry = app.canvas_tools.crop.tool.shows_selection_entry(&settings);
    let out = CropPresetList::new(&mut settings, &document).user_presets(&users).selection_entry(entry).selection_chosen(entry).show(ui);
    if out.ratio.is_some() || out.selection {
        app.ui.tool_options.crop = settings;
    }
    preset_outputs(app, out.ratio, out.new_preset, out.delete_preset, out.selection);
}

/// What a preset pick asks of the app, for the bar and the task bar alike.
fn preset_outputs(app: &mut PhotocraftApp, ratio: Option<CropRatioChange>, new_preset: bool, delete_preset: bool, selection: bool) {
    if let Some(change) = ratio {
        refit(app, change.editing);
    }
    if selection {
        let event = app.canvas_tools.crop.tool.take_selection_frame();
        handle_event(app, event);
    }
    if new_preset {
        crate::crop_presets::open_new(app);
    }
    if delete_preset {
        crate::crop_presets::open_delete(app);
    }
}

/// The ✓ and ⊘ cells show while a crop is pending or the frame was edited.
pub(crate) fn commits_shown(app: &PhotocraftApp) -> bool {
    app.ui.tool == Tool::Crop && (super::pending(app) || app.canvas_tools.crop.tool.modified())
}

/// ✓ crops, ⊘ discards.
pub(crate) fn committed(app: &mut PhotocraftApp, apply: bool) {
    if apply {
        super::apply(app);
    } else {
        super::discard(app);
    }
}

