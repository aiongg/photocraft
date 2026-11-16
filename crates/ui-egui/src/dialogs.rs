//! The hosted-dialog glue: one pass of the framework's dialog host per frame ([`run`]), the
//! results it hands back ([`take_results`]: parameter dialogs through [`params`], prompts through
//! [`PROMPTS`], every other family through [`TAKERS`]), the command opener
//! ([`open_command_dialog`]) and the one-time move of the dialogs' remembered values
//! ([`move_memories`]). Each dialog family owns its module below; this file only routes to it.

use craft::host::Host;
use craft::settings::Remembered;
use craft::ui::dialog::{DialogId, DialogResult, HeldOk, PromptAnswer, StandardPrompt};
use photocraft_doc::DocId;
use photocraft_engine::prefs::Preferences;
use serde_json::Value;

use crate::PhotocraftApp;

/// What a family of the hosted dialogs does with each result, and who owns which dialog key.
pub(crate) mod params {
    use std::path::{Path, PathBuf};

    use craft::host::{FileFilter, PickId, PickRequest};
    use craft::ui::contract::Bind;
    use craft::ui::dialog::{DialogId, DialogResult, StackDialog, dialog_id};
    use craft::ui::patterns::form_dialogs::{DialogFields, FieldValue, HostedParamDialog, Param, ParamDialog, ParamResult};
    use craft::ui::runtime::env::Label;
    use craft::ui::runtime::memory::SaveKey;
    use craft::ui::runtime::requests::{PathKind, PathRequest};
    use photocraft_doc::DocId;
    use serde_json::Value;

    use super::*;
    use crate::canvas::document_display::CanvasHost;

    /// One family of parameter dialogs: the dialogs whose key it owns, and what it does with their answers.
    #[derive(Clone, Copy)]
    pub(crate) struct ParamFamily {
        /// Whether the family opens the dialog with this key. The first family that owns a key wins.
        pub owns: fn(&str) -> bool,
        /// What OK does: runs the family's command with the confirmed values. The family reads its per-open state with
        /// [`captured`].
        pub run: fn(&mut PhotocraftApp, &egui::Context, ParamResult),
        /// Judges a held OK before the dialog closes; a refusal shows under the rows and keeps the dialog open.
        pub check: Option<fn(&mut PhotocraftApp, &ParamResult) -> Result<(), Label>>,
        /// The files of a folder, for a files row of the family's dialogs.
        pub listing: Option<fn(&PhotocraftApp, &Path) -> Vec<PathBuf>>,
        /// A footer button the dialog reported (the button's id comes last).
        pub footer: Option<fn(&mut PhotocraftApp, &egui::Context, DialogId, &str)>,
        /// What a cancelled dialog of the family leaves to undo (a live preview, a guide edit).
        pub cancel: Option<fn(&mut PhotocraftApp, &egui::Context, DialogId)>,
        /// The document a dialog is about, read from its captured value: the dialog closes with the document.
        pub document: Option<fn(&Value) -> Option<DocId>>,
    }

    impl ParamFamily {
        /// A family with only the two required hooks.
        pub(crate) const fn new(owns: fn(&str) -> bool, run: fn(&mut PhotocraftApp, &egui::Context, ParamResult)) -> Self {
            ParamFamily { owns, run, check: None, listing: None, footer: None, cancel: None, document: None }
        }
    }

    /// Every family of parameter dialogs, first match wins; the generic filter family owns what is left.
    pub(crate) const PARAM_FAMILIES: &[ParamFamily] = &[
        ParamFamily { check: Some(fill_ui::check), ..ParamFamily::new(fill_ui::owns, fill_ui::run) },
        ParamFamily { check: Some(stroke_ui::check), ..ParamFamily::new(stroke_ui::owns, stroke_ui::run) },
        ParamFamily {
            check: Some(smart_filter_dialog::check),
            cancel: Some(filter_dialog::cancel),
            document: Some(smart_filter_dialog::document),
            ..ParamFamily::new(smart_filter_dialog::owns, smart_filter_dialog::run)
        },
        ParamFamily::new(new_layer_ui::owns, new_layer_ui::run),
        ParamFamily::new(new_group_ui::owns, new_group_ui::run),
        ParamFamily { check: Some(duplicate_layer_ui::check), ..ParamFamily::new(duplicate_layer_ui::owns, duplicate_layer_ui::run) },
        ParamFamily::new(snapshot_dialog::owns, snapshot_dialog::run),
        ParamFamily::new(history_options_ui::owns, history_options_ui::run),
        ParamFamily::new(brush_preset_dialog::owns, brush_preset_dialog::run),
        ParamFamily { check: Some(equalize_ui::check), ..ParamFamily::new(equalize_ui::owns, equalize_ui::run) },
        ParamFamily::new(channel_options_ui::owns, channel_options_ui::run),
        ParamFamily { check: Some(merge_channels_ui::check), ..ParamFamily::new(merge_channels_ui::owns, merge_channels_ui::run) },
        ParamFamily {
            check: Some(filter_dialog::check),
            footer: Some(color_table_ui::footer_action),
            cancel: Some(filter_dialog::cancel),
            ..ParamFamily::new(|key| key == "image.mode.colorTable", filter_dialog::run)
        },
        ParamFamily { cancel: Some(filter_dialog::cancel), ..ParamFamily::new(file_forms::owns, filter_dialog::run) },
        ParamFamily { listing: Some(stack_dialog::listing), ..ParamFamily::new(stack_dialog::owns, stack_dialog::run) },
        ParamFamily {
            check: Some(crate::canvas_tool_menu::pen_dialogs::check),
            ..ParamFamily::new(crate::canvas_tool_menu::pen_dialogs::owns, crate::canvas_tool_menu::pen_dialogs::run)
        },
        ParamFamily::new(crate::analysis::dialogs::owns, crate::analysis::dialogs::run),
        ParamFamily::new(crate::layers_panel_ui::owns, crate::layers_panel_ui::run),
        ParamFamily { cancel: Some(filter_dialog::cancel), ..ParamFamily::new(crate::view_cmds::dialogs::is_form, filter_dialog::run) },
        crate::prefs_ui::PRESETS_FAMILY,
        ParamFamily { check: Some(crate::crop_presets::check), ..ParamFamily::new(crate::crop_presets::owns, crate::crop_presets::run) },
        crate::actions::dialogs::NEW_SET_FAMILY,
        crate::actions::dialogs::INSERT_STOP_FAMILY,
        crate::actions::playback::FAMILY,
        crate::actions::menu_item::FAMILY,
        crate::smart_svg_ui::family(),
        ParamFamily { check: Some(crate::workspace_ui::check_param), ..ParamFamily::new(crate::workspace_ui::owns_param, crate::workspace_ui::run_param) },
        ParamFamily { check: Some(filter_dialog::check), cancel: Some(filter_dialog::cancel), ..ParamFamily::new(filter_dialog::owns, filter_dialog::run) },
    ];

    /// The family that owns the dialog `key`.
    fn family_for(key: &str) -> Option<&'static ParamFamily> {
        PARAM_FAMILIES.iter().find(|family| (family.owns)(key))
    }

    /// The scope a family's dialog remembers its values under, and the scope [`move_memories`] moves the old value to.
    pub(super) fn memory_scope(key: &str) -> String {
        format!("photocraft.{key}")
    }

    /// Opens the parameter dialog `key` over `fields`, built by `build`. The dialog edits its own history (Cmd+Z steps
    /// its values), remembers its values under [`memory_scope`] and lets its colour rows sample the active document's
    /// canvas. `captured` is the per-open state the family needs when the answer comes back (Smart Filter's target,
    /// Duplicate's layer; `Value::Null` for none): it stays in the app until [`take`] has run the family on the result or
    /// the dialog was cancelled, and the family reads it with [`captured`].
    pub(crate) fn open_param(
        app: &mut PhotocraftApp,
        key: &str,
        fields: DialogFields,
        captured: Value,
        build: impl for<'b> Fn(&'b mut DialogFields) -> ParamDialog<'b> + Send + 'static,
    ) -> DialogId {
        open_with(app, key, fields, captured, Some(SaveKey::named(memory_scope(key))), build)
    }

    /// [`open_param`] for a dialog that remembers nothing (a spec dialog, a smart edit).
    pub(crate) fn open_param_unsaved(
        app: &mut PhotocraftApp,
        key: &str,
        fields: DialogFields,
        captured: Value,
        build: impl for<'b> Fn(&'b mut DialogFields) -> ParamDialog<'b> + Send + 'static,
    ) -> DialogId {
        open_with(app, key, fields, captured, None, build)
    }

    /// The one body of the two openers: `save` is the remember key, when the dialog has one.
    fn open_with(
        app: &mut PhotocraftApp,
        key: &str,
        fields: DialogFields,
        captured: Value,
        save: Option<SaveKey>,
        build: impl for<'b> Fn(&'b mut DialogFields) -> ParamDialog<'b> + Send + 'static,
    ) -> DialogId {
        let canvas = app.session.active_index().map(|idx| CanvasHost::main(idx).canvas_id());
        let dialog = HostedParamDialog::new(app.ctx(), key, fields, move |fields| {
            let mut dialog = build(fields).history(true);
            if let Some(save) = &save {
                dialog = dialog.remember(save.clone());
            }
            match canvas {
                Some(canvas) => dialog.canvas(canvas),
                None => dialog,
            }
        });
        // A family without a check closes at OK; one with a check holds OK until `answer_held` judged it.
        let dialog = if family_for(key).is_some_and(|family| family.check.is_some()) { dialog.held_ok() } else { dialog };
        app.framework.captured.insert(key.to_owned(), captured);
        app.framework.dialogs.open(dialog)
    }

    /// The per-open state the dialog `key` was opened with, `Value::Null` when it had none.
    pub(crate) fn captured(app: &PhotocraftApp, key: &str) -> Value {
        app.framework.captured.get(key).cloned().unwrap_or(Value::Null)
    }

<<<<<<< HEAD
    /// Runs the families on the parameter dialogs that finished, then answers the held OKs and the requests the open
    /// dialogs filed. A result is taken once, so a second pass of a frame finds nothing to run.
    pub(crate) fn take(app: &mut PhotocraftApp, ctx: &egui::Context) {
        while let Some((result, key)) = app.framework.dialogs.take_keyed::<HostedParamDialog>() {
            let key = key.unwrap_or_default();
            let family = family_for(&key);
            let id = match &result {
                DialogResult::Ok(result) => result.id,
                _ => dialog_id(key.as_str()),
            };
            match result {
                DialogResult::Ok(result) => {
                    if let Some(family) = family {
                        (family.run)(app, ctx, result);
                    }
                }
                _ => {
                    if let Some(cancel) = family.and_then(|family| family.cancel) {
                        cancel(app, ctx, id);
=======
/// Whether another dialog is drawn above `live` (egui's layer order, back to front).
fn older_dialog_above(ctx: &egui::Context, live: egui::LayerId, dialogs: &[Dialog]) -> bool {
    let others: Vec<egui::LayerId> =
        dialogs.iter().map(|d| egui::LayerId::new(egui::Order::Foreground, egui::Id::unique(("dialog", d.id)))).filter(|l| *l != live).collect();
    ctx.memory(|m| m.layer_ids().skip_while(|l| *l != live).skip(1).any(|l| others.contains(&l)))
}

/// The size a resizable dialog opens at, or `None` for a dialog sized by its contents.
fn resizable_dialog(fields: &serde_json::Map<String, Value>) -> Option<egui::Vec2> {
    if crate::prefs_ui::is_preferences(fields) {
        Some(crate::prefs_ui::PREFERENCES_SIZE)
    } else if fields.contains_key("__web") {
        Some(egui::vec2(960.0, 900.0))
    } else if is_curves(fields) {
        Some(CURVES_SIZE)
    } else {
        None
    }
}

/// Whether `fields` are the Image › Adjustments › Curves dialog's.
fn is_curves(fields: &serde_json::Map<String, Value>) -> bool {
    fields.get("__adjust").and_then(Value::as_str) == Some("curves")
}

/// The size the Curves dialog first opens at: the 300-point graph it had before it could be
/// resized (#5113); it grows to fit its controls below the graph.
const CURVES_SIZE: egui::Vec2 = egui::vec2(300.0, 420.0);
/// The smallest a resizable dialog is dragged to: Curves keeps a usable graph (#8984), the others
/// their two-column layouts.
fn resizable_min(fields: &serde_json::Map<String, Value>) -> egui::Vec2 {
    if is_curves(fields) { egui::vec2(300.0, 420.0) } else { egui::vec2(560.0, 400.0) }
}

/// Where the size the user last gave a resizable dialog is kept (#8984), beside its place.
fn size_id(place: egui::Id) -> egui::Id {
    place.with("size")
}

/// The largest a resizable dialog of `fields` with smallest size `min` is dragged to: the window
/// less a margin, and for Curves no wider than its graph can grow (#8984), so no blank column.
fn resizable_max(ctx: &egui::Context, fields: &serde_json::Map<String, Value>, min: egui::Vec2) -> egui::Vec2 {
    let mut max = (ctx.content_rect().size() - egui::vec2(40.0, 40.0)).max(min);
    if is_curves(fields) {
        max.x = max.x.min(crate::adjust_editors::curve_side_max(ctx)).max(min.x);
    }
    max
}

fn show_modal<T>(
    modal: egui::Modal,
    ctx: &egui::Context,
    id: egui::Id,
    resizable: Option<(egui::Vec2, egui::Vec2, egui::Vec2)>,
    fixed: Option<egui::Vec2>,
    interactive: bool,
    contents: impl FnOnce(&mut egui::Ui) -> T,
) -> egui::ModalResponse<T> {
    modal.show(ctx, |ui| {
        // Disable the entire older modal, including its resize grip, not just its body.
        if !interactive {
            ui.disable();
        }
        if let Some(size) = fixed {
            egui::Resize::default().id(id.with("resize")).with_stroke(false).fixed_size(size).show(ui, |ui| {
                ui.set_clip_rect(ui.clip_rect().expand(8.0));
                contents(ui)
            })
        } else if let Some((size, min, max)) = resizable {
            // No stroke: Resize outlines its contents 2 points out, an inner box inside the
            // dialog's frame that left the title and buttons flush against it (#8199).
            egui::Resize::default().id(id.with("resize")).with_stroke(false).default_size(size.min(max)).min_size(min).max_size(max).show(ui, contents)
        } else {
            contents(ui)
        }
    })
}

/// Whether dialogs have a close box (×) in their title bar (#5496). Photoshop's modal dialogs have
/// one on Windows, where clicking it is Cancel, and none on macOS; Linux and the web follow Windows.
pub(crate) const CLOSE_BOX: bool = !cfg!(target_os = "macos");

/// The side of the close box's square, in points.
const CLOSE_BOX_SIZE: f32 = 20.0;

/// The close box's square: flush with the title band's right end `right`, centred on the title
/// row at height `centre_y`.
fn close_box_rect(right: f32, centre_y: f32) -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(right - CLOSE_BOX_SIZE, centre_y - CLOSE_BOX_SIZE / 2.0), egui::vec2(CLOSE_BOX_SIZE, CLOSE_BOX_SIZE))
}

/// Draws a title bar's close box in `rect`, the same × icon button the floating panels close with,
/// and says whether it was clicked. Its own child `Ui`, so the title row's layout is unchanged.
pub(crate) fn close_box(ui: &mut egui::Ui, rect: egui::Rect) -> bool {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::right_to_left(egui::Align::Center)));
    crate::icons::button(&mut child, "x", CLOSE_BOX_SIZE, false, tl!("Close")).clicked()
}

/// The dialog's contents under the title rule, picked by its kind and the marker field that routes
/// it. Returns `Some(true)` / `Some(false)` when the body itself answered OK / Cancel.
/// The body of a command dialog: the first handler whose marker the fields carry. The prompts and
/// small editors come first; the rest are in `command_body_more`.
/// The delete confirmations: a layer from the Layers panel's trash, a note (#11788), or all notes
/// (#11791).
fn delete_prompt(fields: &Fields) -> bool {
    crate::delete_layer_prompt::owns(fields)
        || crate::analysis_ui::owns_delete_note(fields)
        || crate::analysis_ui::owns_clear_notes(fields)
        || live_shape_prompt::owns(fields)
}

fn delete_prompt_body(ui: &mut egui::Ui, fields: &mut Fields) {
    if crate::analysis_ui::owns_clear_notes(fields) {
        crate::analysis_ui::clear_notes_body(ui);
    } else if crate::analysis_ui::owns_delete_note(fields) {
        crate::analysis_ui::delete_note_body(ui, fields);
    } else if live_shape_prompt::owns(fields) {
        live_shape_prompt::body(ui, fields);
    } else {
        crate::delete_layer_prompt::body(ui, fields);
    }
}

fn command_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, d: &Dialog, fields: &mut Fields, id: egui::Id, interactive: bool) -> Option<bool> {
    let mut outcome: Option<bool> = None;
    match d.kind {
        _ if crate::open_prompt::owns_memory(fields) => crate::open_prompt::memory_body(ui, fields),
        _ if crate::open_prompt::owns_damage(fields) => crate::open_prompt::damage_body(ui, fields),
        _ if crate::guide_layout_ui::owns(fields) => outcome = crate::guide_layout_ui::body(app, ui, fields, interactive),
        _ if crate::brush_preset_dialog::owns(fields) => outcome = crate::brush_preset_dialog::body(app, ui, fields, interactive),
        _ if crate::fill_ui::owns(fields) => crate::fill_ui::body(app, ui, fields),
        _ if crate::pattern_fill_ui::owns(fields) => crate::pattern_fill_ui::body(app, ui, fields),
        _ if crate::stroke_ui::owns(fields) => crate::stroke_ui::body(ui, fields),
        _ if crate::new_group_ui::owns(fields) => crate::new_group_ui::body(ui, fields),
        _ if crate::snapshot_dialog::owns(fields) => crate::snapshot_dialog::body(ui, fields),
        _ if crate::channel_options_ui::owns(fields) => crate::channel_options_ui::body(ui, fields),
        _ if crate::channel_options_ui::owns_new_spot(fields) => crate::channel_options_ui::new_spot_body(ui, fields),
        _ if crate::shape_dialog::owns(fields) => crate::shape_dialog::body(app, ui, fields),
        _ if delete_prompt(fields) => delete_prompt_body(ui, fields),
        _ if crate::disk_change_prompt::owns(fields) => crate::disk_change_prompt::body(ui, fields),
        _ if crate::replace_prompt::owns(fields) => crate::replace_prompt::body(ui, fields),
        _ if crate::preset_group_prompt::owns(fields) => crate::preset_group_prompt::body(ui, fields),
        _ if crate::actions::owns_stop_message(fields) => crate::actions::stop_message_body(ui, fields),
        _ if crate::actions::owns_insert_stop(fields) => crate::actions::insert_stop_body(ui, fields),
        _ if crate::actions::menu_item::owns(fields) => crate::actions::menu_item::body(ui, fields),
        _ if crate::actions::owns_new_set(fields) => crate::actions::new_set_body(ui, fields),
        _ if crate::actions::playback::owns(fields) => crate::actions::playback::body(ui, fields),
        _ if crate::new_layer_ui::owns(fields) => crate::new_layer_ui::body(ui, fields),
        _ if crate::mask_trash_prompt::owns(fields) => crate::mask_trash_prompt::body(ui, fields),
        _ if crate::rasterize_prompt::owns(fields) => crate::rasterize_prompt::body(ui, fields),
        _ if crate::duplicate_layer_ui::owns(fields) => crate::duplicate_layer_ui::body(app, ui, fields),
        _ if crate::stack_dialog::owns(fields) => crate::stack_dialog::body(app, ui, fields, d.id),
        _ if crate::tool_reset::owns(fields) => crate::tool_reset::body(ui),
        _ if crate::crop_presets::owns(fields) => crate::crop_presets::body(app, ui, fields),
        _ => return command_body_more(app, ui, d, fields, id),
    }
    outcome
}

/// The rest of the command dialogs: files, colour, export, sizing, filters and forms.
fn command_body_more(app: &mut PhotocraftApp, ui: &mut egui::Ui, d: &Dialog, fields: &mut Fields, id: egui::Id) -> Option<bool> {
    let mut outcome: Option<bool> = None;
    match d.kind {
        _ if crate::actions::library::owns(fields) => crate::actions::library::body(app, ui, d.id),
        _ if crate::variables_ui::owns(fields) => crate::variables_ui::body(app, ui, fields),
        _ if merge_channels_ui::owns(fields) => merge_channels_ui::body(ui, fields),
        _ if crate::file_ui::owns(fields) => crate::file_ui::body(app, ui, fields, d.id),
        _ if crate::color_picker_ui::owns(fields) => {
            outcome = crate::color_picker_ui::body(ui, fields);
            crate::color_picker_ui::take_add_swatch(app, fields);
            if let Some(web) = fields.get("__webOnly").and_then(Value::as_bool) {
                app.ui.color_panel.web_only = web;
            }
        }
        _ if crate::color_range_ui::owns(fields) => crate::color_range_ui::body(app, ui, fields),
        _ if crate::prefs_ui::owns(fields) => crate::prefs_ui::body(app, ui, fields, d.id),
        _ if fields.contains_key("__export") => crate::export_dialog::body(app, ui, fields),
        // Image Size and Canvas Size open on Width, selected (#2329).
        _ if fields.contains_key("__sizing") => first_field_focused(ui, id, |ui| crate::sizing::body(ui, fields)),
        _ if crate::smart_filter_dialog::owns(fields) => crate::smart_filter_dialog::body(app, ui, fields),
        _ if crate::adjust_dialog::owns(fields) => crate::adjust_dialog::body(app, ui, fields),
        _ if crate::blur_pins::owns(fields) => crate::blur_pins::body(ui, fields),
        _ if crate::canvas_tool_menu::owns_make_selection(fields) => {
            first_field_focused(ui, id, |ui| crate::canvas_tool_menu::make_selection_body(app, ui, fields))
        }
        _ if fields.contains_key("__filter") => filter_body(app, ui, d, fields, id),
        _ if crate::new_guide_ui::owns(fields) => crate::new_guide_ui::body(ui, fields),
        _ if crate::equalize_ui::owns(fields) => crate::equalize_ui::body(ui, fields),
        _ if fields.contains_key("__form") => {
            if fields.get("__command").and_then(Value::as_str) == Some("file.fileInfo") {
                let room = (ui.ctx().content_rect().height() - DIALOG_CHROME).max(120.0);
                egui::ScrollArea::vertical().id_salt(("file-info-body", d.id)).max_height(room).show(ui, |ui| crate::view_cmds::form_body(ui, fields));
            } else {
                crate::view_cmds::form_body(ui, fields);
            }
        }
        _ => {}
    }
    outcome
}

fn dialog_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, d: &Dialog, fields: &mut Fields, id: egui::Id, interactive: bool) -> Option<bool> {
    let mut outcome: Option<bool> = None;
    match d.kind {
        DialogKind::NewDocument => {
            if crate::new_doc_ui::body(app, ui, fields) {
                outcome = Some(true);
            }
        }
        DialogKind::About if fields.get("systemInfo").and_then(Value::as_bool) == Some(true) => {
            let lines = crate::gpu_status::system_info_ui(app);
            for l in &lines {
                ui.add(egui::Label::new(egui::RichText::new(l).font(crate::theme::mono(12.0))).selectable(true));
            }
            ui.add_space(8.0);
            if crate::widgets::secondary_button(ui, tl!("Copy"), 84.0).clicked() {
                ui.ctx().copy_text(crate::gpu_status::system_info(app).join("\n"));
            }
        }
        DialogKind::About => {
            // Tabs About · Contributors · Models. The
            // tab is a dialog field, so automation can switch it with `ui.dialog.set`.
            let tab = about_tab(fields);
            let mut chosen = tab;
            ui.horizontal(|ui| {
                for (key, label) in [("about", tl!("About")), ("contributors", tl!("Contributors")), ("models", tl!("Models"))] {
                    if crate::widgets::pill_tab(ui, label, tab == key).clicked() {
                        chosen = key;
>>>>>>> upstream/main
                    }
                }
            }
            // The dialog is gone: its eyedropper and its file pickers have nobody to answer.
            canvas_input::disarm_dropper(app);
            drop_picks(app, ctx, id);
            app.framework.captured.remove(&key);
        }
        answer_held(app);
        answer_requests(app, ctx);
    }

    /// The ids of the open hosted parameter dialogs.
    fn hosted(app: &PhotocraftApp) -> Vec<DialogId> {
        let dialogs = &app.framework.dialogs;
        dialogs.stack().ids().filter(|id| dialogs.get_by_id::<HostedParamDialog>(*id).is_some()).collect()
    }
<<<<<<< HEAD

    fn dialog_mut(app: &mut PhotocraftApp, id: DialogId) -> Option<&mut HostedParamDialog> {
        app.framework.dialogs.get_mut_by_id::<HostedParamDialog>(id)
    }

    fn key_of(app: &PhotocraftApp, id: DialogId) -> Option<String> {
        app.framework.dialogs.get_by_id::<HostedParamDialog>(id).and_then(|dialog| StackDialog::key(dialog).map(str::to_owned))
    }

    /// Judges the OK each held dialog filed with its family's `check` (no check accepts) and answers the dialog.
    fn answer_held(app: &mut PhotocraftApp) {
        for id in hosted(app) {
            let Some(result) = dialog_mut(app, id).and_then(HostedParamDialog::take_ok) else { continue };
            let answer = match family_for(&result.key).and_then(|family| family.check) {
                Some(check) => check(app, &result),
                None => Ok(()),
=======
    // Long parameter lists (Flame, Lighting Effects) scroll, so the title and the
    // OK / Cancel buttons stay inside a small window. The preview window below the
    // parameters reserves its own space (#3769).
    let room = (ui.ctx().content_rect().height() - DIALOG_CHROME - crate::filter_preview_ui::reserved_height(fields)).max(120.0);
    let body = |ui: &mut egui::Ui, fields: &mut serde_json::Map<String, Value>| {
        egui::ScrollArea::vertical().id_salt(id.with("body")).max_height(room).show(ui, |ui| {
            if crate::bitmap_dialog::owns(fields) {
                crate::bitmap_dialog::body(app, ui, fields);
            } else {
                crate::filter_dialog::body(ui, fields);
            }
        });
        // The in-dialog preview window sits under the scrolling parameters (#3769),
        // so it never shifts them.
        crate::filter_preview_ui::window(app, d.id, ui, fields);
    };
    // Select and Mask leaves the keyboard to its view keys (F, X, O, V…).
    if crate::select_mask_view::owns(fields) {
        body(ui, fields);
    } else {
        first_field_focused(ui, id, |ui| body(ui, fields));
    }
    if let Some(Value::String(msg)) = fields.remove("__rangeAlert") {
        crate::panels::options_bar::range_alert(&mut app.ui, &msg);
    }
}

/// The OK / Cancel row (with Apply, Alternate or Reset where the dialog has them), drawn in the
/// right-to-left layout. Returns what the buttons (or Enter) answered and whether Apply was hit.
/// `popup_blocks_enter` is true when a web dialog has a popup open, which keeps Enter for the popup.
fn button_row(
    app: &mut PhotocraftApp,
    ui: &mut egui::Ui,
    d: &Dialog,
    fields: &mut Fields,
    id: egui::Id,
    interactive: bool,
    popup_blocks_enter: bool,
) -> (Option<bool>, bool) {
    let mut outcome: Option<bool> = None;
    let mut apply_requested = false;
    if d.kind == DialogKind::Command && crate::actions::owns_stop_message(fields) {
        // A Stop step's message: Continue and Stop, or Stop alone.
        outcome = crate::actions::stop_message_buttons(ui, fields, interactive);
    } else if matches!(d.kind, DialogKind::About | DialogKind::Error) {
        // ↩ is OK too, as on every other dialog (#4872), from the frame after the
        // alert appeared: the ↩ that raised it (committing a locked layer's Free
        // Transform, say) must not close it again at once.
        let seen = id.with("alert-seen");
        let shown_before = ui.ctx().data(|m| m.get_temp::<bool>(seen).unwrap_or(false));
        ui.ctx().data_mut(|m| m.insert_temp(seen, true));
        let enter = interactive && shown_before && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if dialog_buttons(ui, &[DialogButton::new(ButtonRole::Default, tl!("OK"), 84.0)]).is_some() || enter {
            outcome = Some(false);
        }
    } else {
        let ok_label = ok_label(d);
        // Dialogs that need an input first (a preset name, a selection) grey OK out.
        let can_ok = crate::crop_presets::can_confirm(app, fields)
            && crate::stack_dialog::can_confirm(fields)
            && merge_channels_ui::can_confirm(fields)
            && (!crate::actions::library::owns(fields) || crate::actions::library::can_confirm(app, d.id));
        let ok = DialogButton::new(ButtonRole::Default, ok_label, 84.0).enabled(can_ok);
        // Photoshop: holding Alt turns Cancel into Reset (the dialog stays open).
        // Dialogs opt in via `dialog_alt_reset::supported` (per marker).
        let reset = ui.input(|i| i.modifiers.alt) && crate::dialog_alt_reset::supported(fields);
        let cancel_label = if reset {
            tl!("Reset")
        } else if d.kind == DialogKind::NewDocument {
            tl!("Close")
        } else if crate::analysis_ui::owns_delete_note(fields) || live_shape_prompt::owns(fields) {
            tl!("No")
        } else {
            tl!("Cancel")
        };
        let cancel = DialogButton::new(ButtonRole::Cancel, cancel_label, 84.0);
        let clicked = if crate::actions::library::owns(fields) {
            crate::actions::library::buttons(app, ui, d.id)
        } else if d.kind == DialogKind::Command && crate::prefs_ui::is_preferences(fields) {
            let changed = crate::prefs_ui::preferences_changed(app, fields);
            dialog_buttons(ui, &[ok, cancel, DialogButton::new(ButtonRole::Apply, tl!("Apply"), 84.0).enabled(changed)])
        } else if crate::mask_trash_prompt::owns(fields) && !crate::mask_trash_prompt::is_vector(fields) {
            // Photoshop: Apply / Cancel / Delete. Delete answers with the choice set.
            let apply = DialogButton::new(ButtonRole::Default, tl!("Apply"), 84.0).enabled(crate::mask_trash_prompt::can_apply(fields));
            let delete = DialogButton::new(ButtonRole::Alternate, tl!("Delete"), 84.0);
            let hit = dialog_buttons(ui, &[apply, cancel, delete]);
            if hit == Some(ButtonRole::Alternate) {
                fields.insert("choice".into(), json!("delete"));
            } else if hit == Some(ButtonRole::Default) {
                fields.insert("choice".into(), json!("apply"));
            }
            hit
        } else if crate::rasterize_prompt::is_filter(fields) {
            // Photoshop: Convert to Smart Object / Rasterize / Cancel (#4303).
            let rasterize = DialogButton::new(ButtonRole::Default, tl!("Rasterize"), 84.0);
            let hit = if crate::rasterize_prompt::offers_smart_object(fields) {
                let convert = DialogButton::new(ButtonRole::Alternate, tl!("Convert to Smart Object"), 160.0);
                dialog_buttons(ui, &[rasterize, cancel, convert])
            } else {
                dialog_buttons(ui, &[rasterize, cancel])
>>>>>>> upstream/main
            };
            if let Some(dialog) = dialog_mut(app, id) {
                dialog.answer_ok(answer);
            }
        }
    }

    /// The file pickers started for a dialog and not answered yet, in the frame's memory: a pick answers on a later pass.
    #[derive(Clone, Default)]
    struct Picks(Vec<(PickId, DialogId, String)>);

    fn picks_id() -> egui::Id {
        egui::Id::new("pc-param-picks")
    }

    /// Answers the dialogs' requests of this pass: colours through a Color Picker child, paths through the host's file
    /// service, the eyedropper through the canvas, listings and footer buttons through the family. Each request is filed
    /// by the pass that took the input, so a second pass of the frame answers nothing twice.
    pub(crate) fn answer_requests(app: &mut PhotocraftApp, ctx: &egui::Context) {
        settle_picks(app, ctx);
        for id in hosted(app) {
            let key = key_of(app, id);
            let Some(dialog) = dialog_mut(app, id) else { continue };
            let (colour, path, dropper, listing, footer) =
                (dialog.pick_color(), dialog.pick_path(), dialog.take_eyedropper(), dialog.take_listing(), dialog.take_footer_action());
            if let Some(request) = colour {
                color_picker_ui::open_for_request(app, id, request);
            }
            if let Some((row, colour)) = color_picker_ui::answer_for(app, id)
                && let Some(dialog) = dialog_mut(app, id)
            {
                dialog.answer_color(row, colour);
            }
            if let Some(request) = path {
                start_pick(app, ctx, id, request);
            }
            if let Some(request) = dropper {
                canvas_input::arm_dropper(app, id, request.key);
            }
            let family = key.as_deref().and_then(family_for);
            if let (Some(request), Some(listing)) = (listing, family.and_then(|family| family.listing)) {
                let files = listing(app, &request.folder);
                if let Some(dialog) = dialog_mut(app, id) {
                    dialog.answer_listing(request.key, files);
                }
            }
            if let (Some(action), Some(footer)) = (footer, family.and_then(|family| family.footer)) {
                footer(app, ctx, id, &action);
            }
<<<<<<< HEAD
=======
            hit
        } else {
            dialog_buttons(ui, &[ok, cancel])
        };
        // Enter is the default button, except where a popup takes it or Export As waits for a click.
        let enter_confirms = can_ok && !popup_blocks_enter && crate::export_dialog::enter_confirms(fields);
        match clicked {
            Some(ButtonRole::Cancel) if reset => crate::dialog_alt_reset::apply(app, ui.ctx(), fields),
            Some(ButtonRole::Cancel) => outcome = Some(false),
            Some(ButtonRole::Apply) => apply_requested = true,
            Some(_) => outcome = Some(true),
            None if interactive && enter_confirms && ui.input(|i| i.key_pressed(egui::Key::Enter)) => outcome = Some(true),
            None => {}
>>>>>>> upstream/main
        }
    }

<<<<<<< HEAD
    /// Starts the picker a path row asked for, in the folder the row's current path names.
    fn start_pick(app: &mut PhotocraftApp, ctx: &egui::Context, id: DialogId, request: PathRequest) {
        const PURPOSE: &str = "dialog";
        let current = app.framework.dialogs.get_by_id::<HostedParamDialog>(id).and_then(|dialog| match dialog.fields().get(&request.key) {
            Some(FieldValue::Text(text)) if !text.is_empty() => Some(PathBuf::from(text)),
            _ => None,
        });
        let start = current.and_then(|path| if path.is_dir() { Some(path) } else { path.parent().map(Path::to_path_buf) }).filter(|dir| dir.is_dir());
        let mut pick = match &request.kind {
            PathKind::SaveFile => {
                let mut pick = PickRequest::save(PURPOSE);
                for filter in &request.filters {
                    let extensions: Vec<&str> = filter.extensions.iter().map(String::as_str).collect();
                    pick.filters.push(FileFilter::new(filter.name.clone(), &extensions));
=======
/// The default button's label: what OK means for this dialog.
fn ok_label(d: &Dialog) -> &'static str {
    if image_processor_ui::owns(&d.fields) {
        tl!("Run")
    } else if d.kind == DialogKind::NewDocument {
        tl!("Create")
    } else if crate::open_prompt::owns_memory(&d.fields) {
        tl!("Open anyway")
    } else if crate::open_prompt::owns_damage(&d.fields) {
        tl!("Continue")
    } else if crate::delete_layer_prompt::owns(&d.fields) {
        tl!("Delete")
    } else if crate::analysis_ui::owns_delete_note(&d.fields) || live_shape_prompt::owns(&d.fields) {
        tl!("Yes")
    } else if crate::disk_change_prompt::owns(&d.fields) || crate::replace_prompt::owns(&d.fields) {
        tl!("Replace")
    } else if d.fields.contains_key("__export") {
        tl!("Export")
    } else {
        crate::file_ui::ok_label(&d.fields).unwrap_or(tl!("OK"))
    }
}

/// The widest a dialog grows when its body names no width of its own.
fn default_max_width(d: &Dialog, compact: bool, curves: bool, about_tabs: bool, available: f32) -> f32 {
    if d.kind == DialogKind::NewDocument {
        800.0
    } else if about_tabs {
        700.0
    } else if d.kind == DialogKind::LayerStyle || d.fields.contains_key("__export") || crate::color_picker_ui::owns(&d.fields) {
        600.0
    } else if compact {
        260.0
    } else if curves {
        available.max(300.0)
    } else {
        440.0
    }
}

pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if crate::guide_edit::sync(app) {
        ctx.request_repaint();
    }
    crate::layer_style::color_picker::prune(app);
    crate::type_panels_ui::color_picker::prune(app);
    crate::edit_history::track_dialogs(app, ctx);
    let dialogs = app.ui.dialogs.clone();
    let top = dialogs.last().map(|d| d.id);
    let mut shown = Vec::new();
    for d in dialogs {
        let lang = if crate::prefs_ui::is_preferences(&d.fields) {
            crate::i18n::Lang::from_pref(d.fields.get("values").and_then(|v| v.pointer("/interface/language")).and_then(Value::as_str).unwrap_or("auto"))
        } else {
            crate::i18n::current()
        };
        let _language = crate::i18n::language_scope(lang);
        let interactive = top == Some(d.id);
        let mut fields = d.fields.clone();
        let mut outcome: Option<bool> = None; // Some(true)=OK, Some(false)=Cancel
        let mut apply_requested = false;
        let title = display_title(&d);
        let id = egui::Id::unique(("dialog", d.id));
        // Opens where the same dialog was last left (centred the first time), then its top-left
        // stays put (offset from the window's top-left, moved by dragging the title bar; view
        // state only, so egui memory): a dialog whose body grows, like Layer Style switching
        // effects, extends down and right instead of re-centring.
        let pinned: Option<egui::Vec2> = ctx.data(|m| m.get_temp(id));
        let place = place_id(&d);
        let start = pinned.or_else(|| ctx.data_mut(|m| m.get_persisted::<egui::Vec2>(place))).or_else(|| crate::blur_pins::first_place(&d.fields, ctx));
        let mut drag = egui::Vec2::ZERO;
        let mut close_clicked = false;
        let mut sizing = false;
        let mut processor_alert = None;
        let mut fit_image_alert = None;
        let area = match start {
            Some(offset) => egui::Modal::default_area(id).anchor(egui::Align2::LEFT_TOP, offset),
            None => egui::Modal::default_area(id),
        };
        // Only the last dialog takes input, and egui only lets the modal drawn on top take input,
        // so the last one must be drawn above the others. egui can raise an older one (it raises an
        // area that is pressed or wasn't drawn the frame before), which left a greyed-out dialog
        // in front and the live one unreachable behind it. Raise the live one only then: every
        // frame would also cover its own popups.
        if interactive && older_dialog_above(ctx, area.layer(), &app.ui.dialogs) {
            ctx.move_to_top(area.layer());
        }
        let color_picker = crate::color_picker_ui::owns(&fields);
        let mut modal = egui::Modal::new(id).area(area).backdrop_color(egui::Color32::TRANSPARENT);
        if color_picker {
            modal = modal.frame(egui::Frame::popup(&ctx.global_style()).inner_margin(16));
        }
        // Tab / ⇧Tab walk the topmost dialog's text fields, not every widget (field_tab.rs).
        let tab = if top == Some(d.id) && !egui::Popup::is_any_open(ctx) { crate::field_tab::take_step(ctx) } else { 0 };
        // Photoshop doesn't dim the window behind dialogs: previews must be judged at true contrast.
        let web = fields.contains_key("__web");
        let web_popup_open = web && egui::Popup::is_any_open(ctx);
        let resizable = resizable_dialog(&fields);
        // Curves reopens at the size it was left at (#8984).
        let remembered = if is_curves(&fields) { ctx.data_mut(|m| m.get_persisted::<egui::Vec2>(size_id(place))) } else { None };
        let resize = resizable.map(|size| {
            let min = resizable_min(&fields);
            (remembered.unwrap_or(size), min, resizable_max(ctx, &fields, min))
        });
        let mut given = None;
        let fixed = (d.kind == DialogKind::LayerStyle)
            .then(|| egui::vec2(800.0, 550.0).min((ctx.content_rect().size() - egui::vec2(40.0, 40.0)).max(egui::vec2(640.0, 360.0))));
        let modal = show_modal(modal, ctx, id, resize, fixed, interactive, |ui| {
            sizing = ui.is_sizing_pass();
            let available = ui.available_size();
            given = Some(available);
            // A dialog of a few short fields (`__compact`, Fit Image) is narrower (#4673): the title
            // rule and the button row fill the width they are given, so it needs its own maximum too.
            let compact = d.fields.contains_key("__compact");
            // The curve editor's graph is as wide as the dialog lets it be (#5113, #8984).
            let curves = is_curves(&fields);
            if !compact {
                ui.set_min_width(if curves { 300.0 } else { 380.0 });
            }
            // A resizable dialog's contents fill its resizable area (#7420), the others the window.
            let room = if resizable.is_some() { available.x } else { (ctx.content_rect().width() - 48.0).max(380.0) };
            let wide = crate::prefs_ui::width(&d.fields, room);
            if let Some(w) = wide {
                ui.set_min_width(w.min(460.0));
            }
            if d.kind == DialogKind::NewDocument {
                ui.set_min_width(800.0);
            }
            // The About window: room for the contributor table, the same width on every tab.
            let about_tabs = d.kind == DialogKind::About && d.fields.get("systemInfo").and_then(Value::as_bool) != Some(true);
            if about_tabs {
                ui.set_min_width(700.0);
            }
            ui.set_max_width(wide.unwrap_or_else(|| default_max_width(&d, compact, curves, about_tabs, available.x)));
            if web || fixed.is_some() {
                ui.set_max_width(available.x);
                ui.set_min_size(available);
            } else if let Some(w) = crate::file_ui::dialog_width(&d.fields) {
                ui.set_min_width(w);
                ui.set_max_width(w);
            }
            // Duplicate Layer… is a fixed small grid (labels + 260 pt fields ≈ 352 pt):
            // the generic 380 pt minimum left a dead strip on its right.
            if crate::duplicate_layer_ui::owns(&d.fields) {
                ui.set_min_width(360.0);
                ui.set_max_width(360.0);
            }
            if color_picker {
                ui.set_width(crate::color_picker_ui::content_width(ui));
            }
            let label = egui::Label::new(egui::RichText::new(&title).font(crate::theme::semibold(15.0))).selectable(false);
            let t = if crate::color_picker_ui::owns(&fields) { ui.add_sized(egui::vec2(ui.available_width(), 22.0), label).rect } else { ui.add(label).rect };
            // The whole title band drags (#1921): the full width plus a little of the popup's
            // padding around it and the gap above the hairline, but none of the controls below.
            // The close box (#5496) sits at the band's right end and is left out of the drag area:
            // egui would let a drag that starts on a button move the dialog.
            let close_rect = CLOSE_BOX.then(|| close_box_rect(ui.max_rect().right(), t.center().y));
            let bar_right = close_rect.map_or(ui.max_rect().right() + 7.0, |r| r.left());
            let bar = egui::Rect::from_min_max(t.min - egui::vec2(7.0, 7.0), egui::pos2(bar_right, t.bottom() + 4.0));
            drag = ui.interact(bar, id.with("title"), egui::Sense::drag()).drag_delta();
            if let Some(r) = close_rect {
                close_clicked = close_box(ui, r);
            }
            ui.add_space(4.0);
            crate::widgets::hairline(ui);
            ui.add_space(8.0);
            crate::field_tab::begin(ui.ctx());
            if image_processor_ui::owns(&fields) {
                processor_alert = image_processor_ui::body(ui, &mut fields);
            } else if fit_image_range::owns(&fields) {
                fit_image_alert = fit_image_range::body(ui, &mut fields);
            } else {
                outcome = dialog_body(app, ui, &d, &mut fields, id, interactive);
            }
            crate::field_tab::end(ui.ctx(), tab);
            if let Some(index) = fields.remove("__scratchBrowse").and_then(|v| v.as_u64()) {
                let dialog_id = d.id;
                let result = app.ask_folder(move |app, path| {
                    let dialog = app.ui.dialog_mut(dialog_id).ok_or("the dialog was closed")?;
                    let disk = dialog
                        .fields
                        .get_mut("values")
                        .and_then(|v| v.pointer_mut(&format!("/scratchDisks/disks/{index}/path")))
                        .ok_or("the scratch disk was removed")?;
                    if disk.as_str().is_some_and(|s| s.trim() != "(system temp)") {
                        *disk = json!(path);
                    }
                    Ok(Value::Null)
                });
                if let Err(error) = result
                    && error != crate::file_dialog::CANCELLED
                {
                    app.ui.status = error;
                }
            }
            if let Some(Value::String(key)) = fields.remove(BROWSE_FIELD) {
                browse_field(app, d.id, &fields, &key);
            }
            if d.kind == DialogKind::LayerStyle || crate::guide_layout_ui::owns(&fields) || crate::brush_preset_dialog::owns(&fields) {
                // These dialogs own their right-side actions and validate Enter themselves.
            } else if crate::color_picker_ui::owns(&fields) {
                if interactive && outcome.is_none() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    outcome = Some(true);
>>>>>>> upstream/main
                }
                if let Some(name) = &request.file_name {
                    pick = pick.file_name(name.clone());
                }
                pick
            }
            PathKind::OpenFile { extensions, .. } => {
                let mut pick = PickRequest::open(PURPOSE);
                if !extensions.is_empty() {
                    let refs: Vec<&str> = extensions.iter().map(String::as_str).collect();
                    pick = pick.filter(extensions.join(", "), &refs);
                }
                pick
            }
            _ => PickRequest::folder(PURPOSE),
        };
        if let Some(dir) = start {
            pick = pick.directory(dir);
        }
        let ticket = app.framework.host().files().pick(pick);
        ctx.data_mut(|data| data.get_temp_mut_or_default::<Picks>(picks_id()).0.push((ticket, id, request.key)));
    }

    /// Hands each answered pick to its dialog. A cancelled or unavailable picker answers nothing; a pick of a dialog that
    /// closed is forgotten.
    fn settle_picks(app: &mut PhotocraftApp, ctx: &egui::Context) {
        let waiting = ctx.data_mut(|data| std::mem::take(&mut data.get_temp_mut_or_default::<Picks>(picks_id()).0));
        if waiting.is_empty() {
            return;
        }
        let host = app.framework.host().clone();
        let mut still = Vec::new();
        for (ticket, id, key) in waiting {
            if dialog_mut(app, id).is_none() {
                host.files().forget(ticket);
                continue;
            }
            match host.files().take_answer(ticket) {
                None => still.push((ticket, id, key)),
                Some(picked) => {
                    if let (Some(path), Some(dialog)) = (picked.first_path(), dialog_mut(app, id)) {
                        dialog.answer_path(key, path);
                    }
                }
            }
        }
        ctx.data_mut(|data| {
            data.get_temp_mut_or_default::<Picks>(picks_id()).0.splice(0..0, still);
        });
<<<<<<< HEAD
=======
        // The picker has a larger frame margin; include that whole frame in the canvas hit guard.
        shown.push(if color_picker { modal.response.rect } else { modal.inner });
        // The size Curves was given (dragged or grown to fit) is the one it reopens at (#8984).
        // A size at the window's limit is the window's, not the user's: a large remembered size
        // squeezed into a small window is kept for a larger one.
        if let (false, true, Some(size), Some((opened, _, max))) = (sizing, is_curves(&fields), given.filter(|g| g.is_finite()), resize)
            && size != opened.min(max)
            && size.x < max.x - 0.5
            && size.y < max.y - 0.5
            && remembered != Some(size)
        {
            ctx.data_mut(|m| m.insert_persisted(size_id(place), size));
        }
        // Pin once laid out at its real size (the first frame is an invisible sizing pass).
        if !sizing && (web || fixed.is_some() || pinned.is_none() || drag != egui::Vec2::ZERO) {
            let screen = ctx.content_rect();
            // Keep the whole dialog (and so its title bar) on screen.
            let room = (screen.size() - modal.response.rect.size()).max(egui::Vec2::ZERO);
            let offset = (pinned.unwrap_or(modal.response.rect.min - screen.min) + drag).clamp(egui::Vec2::ZERO, room);
            ctx.data_mut(|m| m.insert_temp(id, offset));
            // A dialog the user moved reopens there (Photoshop remembers each dialog's place).
            if drag != egui::Vec2::ZERO {
                ctx.data_mut(|m| m.insert_persisted(place, offset));
            }
        }
        // Esc and the title bar's close box cancel (topmost dialog, no popup open for Esc). A click
        // outside does nothing: Photoshop keeps the dialog, and the pointer may be panning or
        // zooming the canvas under it.
        if interactive
            && outcome.is_none()
            && (close_clicked
                || modal.response.should_close()
                || (modal.is_top_modal && !modal.any_popup_open && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))))
        {
            outcome = Some(false);
        }
        outcome = image_processor_ui::finish(&mut app.ui, interactive, processor_alert, outcome);
        outcome = fit_image_range::finish(&mut app.ui, interactive, fit_image_alert, outcome);
        // A size warning must precede creation, while Cancel discards the edit and its warning.
        // Wait until the buttons have answered so the click that left a field cannot do both.
        if crate::shape_dialog::owns(&fields) && outcome != Some(false) && crate::shape_dialog::open_owed_alert(app, ctx, &mut fields) {
            outcome = None;
        }
        if let Some(dm) = app.ui.dialog_mut(d.id) {
            dm.fields = fields;
        }
        // A colour swatch (Edit › Fill, Edit › Stroke) opens the Color Picker over the dialog.
        if outcome.is_none() {
            crate::color_picker_ui::open_requested(app, d.id);
        }
        if apply_requested && outcome.is_none() {
            let _ = crate::prefs_ui::apply(app, d.id);
        }
        match outcome {
            Some(true) => {
                if let Err(error) = confirm(app, d.id) {
                    app.ui.status = error;
                }
            }
            Some(false) => {
                let _ = cancel(app, d.id);
            }
            None => {
                if let Err(error) = crate::layer_style::color_picker::take_request(app, d.id) {
                    app.ui.status = error;
                }
            }
        }
>>>>>>> upstream/main
    }

    /// Forgets the picks of the dialog `id`, which closed.
    fn drop_picks(app: &mut PhotocraftApp, ctx: &egui::Context, id: DialogId) {
        let waiting = ctx.data_mut(|data| std::mem::take(&mut data.get_temp_mut_or_default::<Picks>(picks_id()).0));
        if waiting.is_empty() {
            return;
        }
        let host = app.framework.host().clone();
        let (closed, kept): (Vec<_>, Vec<_>) = waiting.into_iter().partition(|(_, dialog, _)| *dialog == id);
        for (ticket, ..) in closed {
            host.files().forget(ticket);
        }
        ctx.data_mut(|data| {
            data.get_temp_mut_or_default::<Picks>(picks_id()).0.splice(0..0, kept);
        });
    }

    /// A bare command form: a dialog without rows that the generic filter family runs.
    pub(super) fn open_bare(app: &mut PhotocraftApp, command: &str, label: &str) -> DialogId {
        let (key, title) = (command.to_owned(), label.to_owned());
        open_param(app, command, DialogFields::new(), Value::Null, move |fields| {
            ParamDialog::command(key.clone(), title.clone(), Vec::<Param>::new()).values(Bind::bound(fields))
        })
    }
}

/// One entry of [`PROMPTS`]: answers the prompt when the answer's tag is its own, and says whether it did.
pub(crate) type PromptRow = fn(&mut PhotocraftApp, &egui::Context, &mut PromptAnswer) -> bool;

<<<<<<< HEAD
/// The prompts' answers, one row per family: the first row whose tag type matches takes the answer (A11). A prompt
/// family gives an `answer` function, never a `take`: the host keeps one plain result per dialog type, so a second taker
/// would take another family's answer.
pub(crate) const PROMPTS: &[PromptRow] = &[
    crate::tools::tool_reset::answer,
    crate::analysis::note_alerts::answer,
    delete_layer_prompt::answer,
    mask_trash_prompt::answer,
    disk_change_prompt::answer,
    rasterize_prompt::answer,
    live_shape_prompt::answer,
    preset_group_prompt::answer,
    open_prompt::answer_prompt,
    alerts::answer,
    crate::jobs_ui::answer,
    crate::gpu_status::answer,
];

/// [`prompts_take`] over the rows `rows`.
fn take_prompts_from(app: &mut PhotocraftApp, ctx: &egui::Context, rows: &[PromptRow]) {
    while let Some(result) = app.framework.dialogs.take::<StandardPrompt>() {
        // Only an `Ok` carries an answer: a host Cancel (a replaced prompt, a parent closed over it) reaches no row.
        let DialogResult::Ok(mut answer) = result else { continue };
        for row in rows {
            if row(app, ctx, &mut answer) {
                break;
            }
=======
/// The About tab to show: the `tab` field when it names one, otherwise "about".
fn about_tab(fields: &serde_json::Map<String, Value>) -> &'static str {
    let want = fields.get("tab").and_then(Value::as_str).unwrap_or("");
    ABOUT_TABS.iter().copied().find(|t| *t == want).unwrap_or("about")
}

/// One About tab's contents: the credits lists, or the product blurb and links.
fn about_tab_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, tab: &str) {
    match tab {
        "contributors" => crate::credits::contributors_ui(app, ui),
        "models" => crate::credits::models_ui(ui),
        _ => {
            ui.label(tl!("PhotoCraft — an open-source, native image editor written in Rust."));
            ui.label(crate::i18n::fmt(tl!("Version {version}"), &[("version", &photocraft_engine::build_info::long_version())]));
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                crate::links::discord_button(app, ui, 220.0);
                ui.add_space(8.0);
                crate::links::link_row(app, ui);
            });
            ui.add_space(10.0);
            ui.weak("egui · wgpu · photocraft-engine");
        }
    }
}

/// The dialog title as shown: [`title`] in the UI language.
fn display_title(d: &Dialog) -> String {
    match d.fields.get("__colorPicker").and_then(Value::as_str) {
        Some("foreground") => return crate::color_picker_ui::fixed_title("Color Picker (Foreground Color)", "Foreground Color"),
        Some("background") => return crate::color_picker_ui::fixed_title("Color Picker (Background Color)", "Background Color"),
        _ => {}
    }
    if crate::smart_filter_dialog::owns(&d.fields) {
        return crate::smart_filter_dialog::title(&d.fields);
    }
    match d.kind {
        DialogKind::Command => {
            let label = d.fields.get("__label").and_then(Value::as_str).unwrap_or("Command");
            // Dialog labels are catalogued with their "…" ("Export As…"); some commands omit it.
            let with_dots = format!("{}…", label.trim_end_matches('…'));
            let shown = if crate::i18n::has(crate::i18n::current(), label) { tl!(label) } else { tl!(&with_dots) };
            shown.trim_end_matches('…').to_string()
        }
        _ => tl!(&title(d)).to_string(),
    }
}

pub fn title(d: &Dialog) -> String {
    match d.kind {
        DialogKind::NewDocument => "New Document".into(),
        DialogKind::About if d.fields.get("systemInfo").and_then(Value::as_bool) == Some(true) => "System Info".into(),
        DialogKind::About => "About PhotoCraft".into(),
        DialogKind::LayerStyle => "Layer Style".into(),
        DialogKind::Command => d.fields.get("__label").and_then(Value::as_str).unwrap_or("Command").trim_end_matches('…').to_string(),
        DialogKind::Error => "Error".into(),
    }
}

type Fields = serde_json::Map<String, Value>;

/// The markers [`confirm`] and [`cancel`] choose a dialog's handler by: keys the code opening a
/// dialog puts in its fields. Keep in step with the arms of `confirm`.
const ROUTES: &[fn(&Fields) -> bool] = &[
    crate::guide_layout_ui::owns,
    crate::brush_preset_dialog::owns,
    crate::fill_ui::owns,
    crate::pattern_fill_ui::owns,
    crate::stroke_ui::owns,
    crate::channel_options_ui::owns,
    crate::channel_options_ui::owns_new_spot,
    merge_channels_ui::owns,
    crate::shape_dialog::owns,
    crate::delete_layer_prompt::owns,
    crate::analysis_ui::owns_delete_note,
    crate::analysis_ui::owns_clear_notes,
    live_shape_prompt::owns,
    crate::disk_change_prompt::owns,
    crate::actions::owns_stop_message,
    crate::actions::owns_insert_stop,
    crate::actions::menu_item::owns,
    crate::actions::record_again::owns,
    crate::actions::owns_new_set,
    crate::actions::playback::owns,
    crate::new_layer_ui::owns,
    crate::new_group_ui::owns,
    crate::mask_trash_prompt::owns,
    crate::rasterize_prompt::owns,
    crate::duplicate_layer_ui::owns,
    crate::stack_dialog::owns,
    crate::tool_reset::owns,
    crate::crop_presets::owns,
    crate::variables_ui::owns,
    crate::file_ui::owns,
    crate::new_guide_ui::owns,
    crate::equalize_ui::owns,
    crate::color_picker_ui::owns,
    crate::color_range_ui::owns,
    crate::prefs_ui::owns,
    crate::open_prompt::owns,
    crate::replace_prompt::owns,
    crate::actions::library::owns,
    crate::preset_group_prompt::owns,
    crate::layer_style::color_picker::owns,
    |f| f.contains_key("__export"),
    |f| f.contains_key("__smartFilter"),
    |f| f.contains_key(crate::actions::RESUME),
];

/// Whether `key` is reserved to its dialog. Automation may change private fields but never
/// add a route marker: that could select a handler its command was never judged for.
pub fn is_route_marker(key: &str) -> bool {
    let probe = Fields::from_iter([(key.to_string(), Value::Bool(true))]);
    key == crate::filter_dialog::MATCH_COLOR_DESTINATION || ROUTES.iter().any(|owns| owns(&probe))
}

/// The file command a dialog's handler stands for when it reads or writes files itself, outside
/// the engine (a disk-change save, Export As, Save for Web, Print), so automation is judged for
/// that command rather than for the dialog's `__command`.
fn file_command(fields: &Fields) -> Option<&'static str> {
    if crate::disk_change_prompt::owns(fields) {
        Some("file.save")
    } else if fields.contains_key("__export") {
        Some("file.export.exportAs")
    } else if fields.contains_key("__web") {
        Some("file.export.saveForWebLegacy")
    } else if fields.contains_key("__print") {
        Some("file.print")
    } else {
        None
    }
}

/// Batch and the other folder runs (Image Processor, …) go on past a file that fails and list it
/// in their result's `errors`; the status bar and a notice say which files failed and why, so a run
/// that fails on every file isn't silent (#10682). Each file is named by its whole path: files of
/// the same name in different subfolders are told apart.
fn report_file_errors(app: &mut PhotocraftApp, d: &Dialog, r: &Value) {
    let Some(errors) = r.get("errors").and_then(Value::as_array).filter(|e| !e.is_empty()) else { return };
    let lines: Vec<String> = errors
        .iter()
        .map(|e| {
            let file = e.get("file").and_then(Value::as_str).unwrap_or_default();
            format!("{file}: {}", e.get("error").and_then(Value::as_str).unwrap_or_default())
        })
        .collect();
    let label = d.fields.get("__label").and_then(Value::as_str).unwrap_or("Command");
    let what = crate::i18n::t(label).trim_end_matches('…').to_string();
    crate::notices::file_failures(app, &what, &lines);
}

/// Confirm a command dialog no specific handler owns: run its `__command` with the dialog's
/// parameters, then settle the filter preview and the playing action.
fn confirm_generic_command(app: &mut PhotocraftApp, mut d: Dialog) -> Result<Value, String> {
    crate::filter_dialog::refresh_match_color_sources(&app.session, &mut d.fields);
    let Some(cmd) = d.fields.get("__command").and_then(Value::as_str).map(str::to_string) else {
        app.filter_preview = None;
        crate::actions::abandoned(app, &d.fields);
        return Err("dialog has no command".into());
    };
    let mut params = crate::filter_dialog::params_of(&d.fields);
    let exact = if cmd == "file.automate.batch" {
        crate::actions::batch_dialog_params(app, d.id, d.fields.get(crate::actions::BATCH_OCCURRENCE), &mut params)
    } else {
        Ok(())
    };
    if let Err(error) = exact.and_then(|()| crate::actions::authorize_panel_preview(app, d.id, &d.fields, &cmd, &params)) {
        app.filter_preview = None;
        crate::actions::abandoned(app, &d.fields);
        return Err(error);
    }
    if crate::filter_dialog::needs_displace_map(&cmd, &params) {
        app.filter_preview = None;
        let result = crate::filter_dialog::ask_displace_map(app, cmd, d.fields.clone());
        if result.is_err() {
            crate::actions::abandoned(app, &d.fields);
        } else {
            // Cancel and an unavailable picker finish before the Displace callback
            // runs. Observe the whole file-dialog result, including those exits.
            app.after_file_dialog(move |app, result| {
                if result.is_err() {
                    crate::actions::abandoned(app, &d.fields);
                }
                result
            });
>>>>>>> upstream/main
        }
    }
}

/// Hands each prompt answer to the family it is for.
pub(crate) fn prompts_take(app: &mut PhotocraftApp, ctx: &egui::Context) {
    take_prompts_from(app, ctx, PROMPTS);
}

/// The families that take their own results (one `take(app, ctx)` each), in order.
pub(crate) const TAKERS: &[fn(&mut PhotocraftApp, &egui::Context)] = &[
    alerts::take,
    discard_ui::take,
    open_prompt::take,
    credits::take,
    adjust_dialog::take,
    layer_style::take,
    color_picker_ui::take,
    color_range_ui::take,
    select_mask_ui::take,
    caf_ui::take,
    liquify_ui::take,
    gallery_ui::take,
    wide_angle_ui::take,
    vp_ui::take,
    camera_raw_ui::take,
    new_doc_ui::take,
    sizing::take,
    shape_dialog::take,
    guide_layout_ui::take,
    new_guide_ui::take,
    export_dialog::take,
    file_ui::take,
    format_prompts::take,
    pattern_fill_ui::take,
    crate::workspace_ui::take,
    crate::kys_import::take,
    crate::prefs_ui::take_presets,
    crate::layers_panel_ui::take,
    crate::slice_ui::take,
    crate::crop_presets::take,
    crate::variables_ui::take,
    crate::actions::dialogs::take_stops,
];

/// Runs the answers of every dialog that finished: parameter dialogs, prompts, then each family's own results. It runs
/// after every host pass and at once after [`confirm`], [`cancel`] and an automation edit, so an answer acts inside the
/// request that caused it.
pub(crate) fn take_results(app: &mut PhotocraftApp, ctx: &egui::Context) {
    params::take(app, ctx);
    prompts_take(app, ctx);
    for taker in TAKERS {
        taker(app, ctx);
    }
}

/// One pass of the dialogs: the guide edit follows its dialog, the palette shows, the filter preview is fed, the
/// framework's host runs every open dialog, and the results are taken. A frame that runs a second pass runs this again;
/// the host applies only a pass's own input and a result is taken once, so nothing acts twice.
pub fn run(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if crate::guide_edit::sync(app) {
        ui.ctx().request_repaint();
    }
    palette::show(app, ui);
    filter_preview_ui::feed(app, ui.ctx());
    app.framework.dialogs.run(ui);
    take_results(app, &ui.ctx().clone());
}

/// Answers dialog `id` with OK, as its default button would, and runs what OK does. A held OK is answered at once by the
/// family that holds it: an accepted one closes the dialog in this call, a refusal comes back as the error text, and one
/// that waits on a pick (Export's save dialog) is answered by the pick.
pub(crate) fn confirm(app: &mut PhotocraftApp, ctx: &egui::Context, id: DialogId) -> Result<(), String> {
    if app.framework.dialogs.get_dyn(id).is_none() {
        return Err(format!("no dialog {}", id.value()));
    }
    // Automation confirming a dialog that stands for a command is judged as that command, whichever way the answer
    // arrives (a control request or synthetic input).
    if app.automation_input
        && let Some(authorize) = app.services.automation_command.as_ref()
        && let Some((command, params)) = crate::control::dialogs::hosted_command(app, id)
    {
        authorize(&command, &params)?;
    }
    // Synthetic input answering Save on the unsaved-changes prompt is judged as `file.save`, as a control answer is.
    if app.automation_input && crate::control::dialogs::is_save_answer(app, id) {
        crate::discard_ui::authorize_save(app, false)?;
    }
    if !app.framework.dialogs.confirm_by_id(ctx, id) {
        if app.framework.dialogs.held_state(id) != Some(HeldOk::Waiting) {
            take_results(app, ctx);
            return Err(format!("dialog {} cannot be confirmed now", id.value()));
        }
        // The family that holds the OK answers it among the results.
        take_results(app, ctx);
        match app.framework.dialogs.held_state(id) {
            Some(HeldOk::Accepted) => {
                app.framework.dialogs.confirm_by_id(ctx, id);
            }
            Some(HeldOk::Refused(reason)) => return Err(reason.as_str().to_owned()),
            _ => {}
        }
    }
    take_results(app, ctx);
    Ok(())
}

/// Cancels dialog `id`, as Cancel or Escape would, and runs what a cancel leaves to do.
pub(crate) fn cancel(app: &mut PhotocraftApp, ctx: &egui::Context, id: DialogId) -> Result<(), String> {
    // A prompt answers Escape with its escape button, as an answer its family takes; the host's own cancel is a result
    // that reaches no family (it stands for a replaced prompt).
    let escape = app.framework.dialogs.get_dyn_mut(id).and_then(|dialog| dialog.downcast_mut::<StandardPrompt>()).and_then(|prompt| {
        let choice = prompt.escape_choice()?;
        prompt.set_choice(choice);
        Some(())
    });
    if escape.is_some() && app.framework.dialogs.confirm_by_id(ctx, id) {
        take_results(app, ctx);
        return Ok(());
    }
    if !app.framework.dialogs.cancel_by_id(ctx, id) {
        return Err(format!("no dialog {}", id.value()));
    }
    take_results(app, ctx);
    Ok(())
}

<<<<<<< HEAD
/// Cancels every open parameter dialog whose family reports one of the `closed` documents from the dialog's captured
/// value: a dialog about a document does not outlive it.
pub(crate) fn cancel_for_closed(app: &mut PhotocraftApp, ctx: &egui::Context, closed: &[DocId]) {
    let doomed: Vec<DialogId> = app
        .framework
        .dialogs
        .open_dialogs()
        .into_iter()
        .filter_map(|open| {
            let key = open.key?;
            let document = params::PARAM_FAMILIES.iter().find(|family| (family.owns)(&key))?.document?;
            let doc = document(app.framework.captured.get(&key)?)?;
            closed.contains(&doc).then_some(open.id)
        })
        .collect();
    if doomed.is_empty() {
        return;
=======
/// The rest of the command dialogs: files, colour, preferences and export, then any other command.
fn confirm_command_more(app: &mut PhotocraftApp, d: Dialog) -> Result<Value, String> {
    match d.kind {
        _ if crate::actions::library::owns(&d.fields) => crate::actions::library::confirm(app, d.id),
        _ if merge_channels_ui::owns(&d.fields) => merge_channels_ui::confirm(app, &d.fields),
        _ if crate::analysis_ui::owns_delete_note(&d.fields) => crate::analysis_ui::confirm_delete_note(app, &d.fields),
        _ if crate::analysis_ui::owns_clear_notes(&d.fields) => crate::analysis_ui::confirm_clear_notes(app, &d.fields),
        _ if live_shape_prompt::owns(&d.fields) => live_shape_prompt::confirm(app, &d.fields),
        _ if crate::file_ui::owns(&d.fields) => crate::file_ui::confirm(app, &d.fields),
        _ if crate::new_guide_ui::owns(&d.fields) => crate::new_guide_ui::confirm(app, &d.fields),
        _ if crate::color_picker_ui::owns(&d.fields) => {
            // OK sets the target as it was before the live preview showed the colour in it.
            crate::color_picker_ui::live_preview::end(app, d.id);
            crate::color_picker_ui::confirm(app, &d.fields)
        }
        _ if crate::color_range_ui::owns(&d.fields) => crate::color_range_ui::confirm(app, &d.fields),
        _ if crate::prefs_ui::owns(&d.fields) => crate::prefs_ui::confirm(app, &d.fields),
        _ if d.fields.contains_key("__export") => crate::export_dialog::confirm(app, &d.fields),
        _ => confirm_generic_command(app, d),
>>>>>>> upstream/main
    }
    for id in doomed {
        app.framework.dialogs.cancel_by_id(ctx, id);
    }
    take_results(app, ctx);
}

<<<<<<< HEAD
/// Open the parameter dialog of `command`: the guide layout, Stroke, Equalize (when it asks), New Group, Color Range,
/// Slice options and Divide, the adjustment editor for `image.adjustments.*`, the schema dialog for filters, otherwise a
/// bare command form. The error is an opener's refusal text.
pub fn open_command_dialog(app: &mut PhotocraftApp, command: &str, label: &str) -> Result<DialogId, String> {
=======
pub fn confirm(app: &mut PhotocraftApp, id: u64) -> Result<Value, String> {
    confirm_preflight(app, id)?;
    image_processor_ui::preflight(&mut app.ui, id)?;
    fit_image_range::preflight(&mut app.ui, id)?;
    let d = app.ui.close_dialog(id).ok_or_else(|| format!("no dialog {id}"))?;
    match d.kind {
        DialogKind::NewDocument => {
            let r = app.run("file.new", crate::new_doc_ui::command_params(&d.fields));
            if let Some(i) = app.session.active_index() {
                app.ui.views[i].fit_pending = true;
            }
            if r.is_ok() {
                crate::new_doc_ui::pixel_aspect::apply(app, &d.fields);
                crate::remember_new_document(app, &d.fields);
                // File › New › Recent. Best effort: the document exists either way.
                let _ = app.run("document.recent.add", json!({"settings": crate::new_doc_ui::document_settings(&d.fields)}));
            }
            r
        }
        DialogKind::Command => confirm_command(app, d),
        DialogKind::LayerStyle => crate::layer_style::confirm(app, &d.fields),
        DialogKind::About => Ok(Value::Null),
        DialogKind::Error => {
            dont_show_again::closed(app, &d);
            Ok(Value::Null)
        }
    }
}

/// Cancel a dialog and its dependent Layer Style picker, without applying edits.
pub fn cancel(app: &mut PhotocraftApp, id: u64) -> Result<Value, String> {
    let closed = app.ui.close_dialog(id).ok_or_else(|| format!("no dialog {id}"))?;
    dont_show_again::closed(app, &closed);
    crate::guide_edit::cancel(app, id);
    // A Color Picker puts back what its live preview showed in its target.
    crate::color_picker_ui::live_preview::end(app, id);
    // A field Color Picker of this dialog has no target left: it closes with the dialog.
    // A picker with a live target goes by the parent recorded when it opened; `__dialog` only
    // names the parent of one without (e.g. its parent was closed before it could preview).
    let mut pickers = crate::color_picker_ui::live_preview::pickers_of(app, id);
    pickers.extend(
        app.ui
            .dialogs
            .iter()
            .filter(|d| crate::color_picker_ui::owns(&d.fields) && d.fields.get("__dialog").and_then(Value::as_u64) == Some(id))
            .filter(|d| !crate::color_picker_ui::live_preview::previews(app, d.id))
            .map(|d| d.id),
    );
    for picker in &pickers {
        crate::color_picker_ui::live_preview::end(app, *picker);
    }
    app.ui.dialogs.retain(|d| !pickers.contains(&d.id));
    crate::actions::record_again::cancel(app, id);
    crate::actions::cancel_batch_dialog(app, id);
    crate::actions::menu_item::cancel(app, id);
    crate::actions::stopped(app, &closed.fields);
    crate::actions::library::cancel(app, id);
    if crate::guide_layout_ui::owns(&closed.fields) {
        app.ui.dialogs.retain(|child| child.fields.get("__dialog").and_then(Value::as_u64) != Some(id));
    }
    // Cancel on a profile prompt opens neither its file nor the ones still to come.
    if crate::open_prompt::owns_memory(&closed.fields) {
        crate::open_prompt::answer_memory(app, false)?;
    } else if crate::open_prompt::owns_damage(&closed.fields) {
        crate::open_prompt::answer_damage(app, false)?;
    } else if crate::open_prompt::owns(&closed.fields) {
        crate::open_prompt::answer(app, None)?;
    }
    // Cancel on the question whether to replace a file writes nothing.
    if crate::replace_prompt::owns(&closed.fields) {
        crate::replace_prompt::cancel(app)?;
    }
    // Cancel on the group-name question imports nothing.
    if crate::preset_group_prompt::owns(&closed.fields) {
        crate::preset_group_prompt::cancel(app);
    }
    app.ui.dialogs.retain(|d| !crate::layer_style::color_picker::child_of(&d.fields, id));
    app.filter_preview = None;
    app.color_range = None;
    crate::type_panels_ui::color_picker::prune(app);
    Ok(Value::Null)
}

/// Open the parameter dialog of `command`: the Equalize choices when there is a selection, the
/// adjustment editor for `image.adjustments.*`, the schema dialog for filters, the Color Range
/// dialog for `select.colorRange`, otherwise a bare confirm dialog.
pub fn open_command_dialog(app: &mut PhotocraftApp, command: &str, label: &str) -> u64 {
>>>>>>> upstream/main
    if command == "view.newGuideLayout" {
        return guide_layout_ui::open(app).ok_or_else(|| "no document".to_string());
    }
    if command == stroke_ui::COMMAND {
        return Ok(stroke_ui::open(app));
    }
    if command == equalize_ui::COMMAND && equalize_ui::asks(app) {
        return Ok(equalize_ui::open(app));
    }
    if new_group_ui::fronts(command) {
        return Ok(new_group_ui::open(app, command));
    }
    if command == color_range_ui::COMMAND {
        return color_range_ui::open(app);
    }
    if command == "slice.set" {
        return crate::slice_ui::open_options(app);
    }
    if command == "slice.divide" {
        return crate::slice_ui::open_divide(app);
    }
    if adjust_dialog::has_dialog(command) {
        return adjust_dialog::open(app, command);
    }
    if filter_dialog::has_dialog(command) {
        return filter_dialog::open(app, command);
    }
    Ok(params::open_bare(app, command, label))
}

/// Preference entries the engine reads itself, by key: the engine has no settings store, so they cannot move.
const ENGINE_READ: &[&str] = &["ui.layersPanelOptions", "file.print", "file.export.saveForWebLegacy"];

/// The "Don't show again" entries (`{"skip": true}`), by key, as their prompt modules read them. Told apart by key, not
/// by the shape of the value.
fn dont_show_again_keys() -> [&'static str; 4] {
    [delete_layer_prompt::PREF, live_shape_prompt::PREF, crate::analysis::note_alerts::DELETE_NOTE_PREF, crate::retouch_ui::CAM_NO_PIXELS_PREF]
}

/// Where the remembered value of the preference entry `key` goes: `None` keeps it in `Preferences.dialogs`, else the
/// scope it moves to. Order: keep, then route, then the generic `photocraft.<key>`.
fn memory_destination(key: &str) -> Option<String> {
    if ENGINE_READ.contains(&key) || dont_show_again_keys().contains(&key) {
        return None;
    }
    // The keys the app passes to a workspace or dialog (`select.refineEdge.workspace`, `edit.contentAwareFill.workspace`,
    // `filter.liquify`, `view.newGuide`, `file.export.exportAs`, `file.jpegOptions`) follow the generic naming, so only a
    // key a library crate names differently routes.
    let routed = match key {
        "filter.cameraRaw.scope" => Some("craft_imaging.raw_develop.scope".to_owned()),
        _ => key
            .strip_prefix("image.adjustments.")
            .map(|kind| format!("photocraft.adjust.last.{kind}"))
            .or_else(|| key.strip_prefix("shape.create.").map(|kind| format!("craft_draw.shape.create.{kind}"))),
    };
    Some(routed.unwrap_or_else(|| params::memory_scope(key)))
}

/// `value` without its nulls: a missing key reads as the dialog's default, and TOML has no null.
fn without_nulls(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(map.into_iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k, without_nulls(v))).collect()),
        Value::Array(items) => Value::Array(items.into_iter().filter(|v| !v.is_null()).map(without_nulls).collect()),
        other => other,
    }
}

/// Fill and Stroke remembered their blend mode as a `mode_key` string ("colorBurn"); the dialogs keep the layers panel's key.
fn convert_blend_mode(key: &str, value: &mut Value) {
    if !matches!(key, "edit.fill" | "edit.stroke") {
        return;
    }
    let Some(mode) = value.get_mut("mode") else { return };
    if let Some(blend) = mode.as_str().and_then(crate::panels::layers_tree::blend_of) {
        *mode = Value::String(crate::panels::layers_tree::blend_key(blend));
    }
}

/// Moves the dialogs' last-used values out of `Preferences.dialogs` into the remembered section of `host`, once, before
/// the first frame. Entries the engine reads and the don't-show-again entries stay; a second run finds nothing to move
/// because every moved entry left `Preferences.dialogs`. An entry that has no table form stays too.
pub(crate) fn move_memories(prefs: &mut Preferences, host: &Host) {
    let keys: Vec<String> = prefs.dialogs.keys().cloned().collect();
    for key in keys {
        let Some(scope) = memory_destination(&key) else { continue };
        let Some(old) = prefs.dialogs.get(&key) else { continue };
        let mut value = without_nulls(old.clone());
        convert_blend_mode(&key, &mut value);
        let Ok(table) = toml::Table::try_from(&value) else { continue };
        Remembered::<toml::Table>::new(scope).store(host, &table);
        prefs.dialogs.remove(&key);
    }
}

pub mod adjust_dialog;
pub mod adjust_editors;
pub mod adjust_preview;
pub mod alerts;
#[cfg(test)]
mod arbitrary_ruler_prefill_tests;
pub(crate) mod bitmap_dialog;
pub mod blend_options;
pub mod blur_pins;
pub mod brush_preset_dialog;
pub mod caf_ui;
pub(super) mod camera_raw_detail_ui;
pub(super) mod camera_raw_gpu;
pub(super) mod camera_raw_scope_ui;
pub mod camera_raw_ui;
pub(crate) mod canvas_input;
pub mod channel_options_ui;
pub(crate) mod color_picker_preview;
pub mod color_picker_ui;
pub mod color_range_ui;
pub mod color_table_ui;
pub mod count_group_name_ui;
pub mod credits;
pub mod dds_options_ui;
pub mod delete_layer_prompt;
pub mod discard_ui;
pub mod disk_change_prompt;
pub mod distort_ui;
pub mod duplicate_layer_ui;
pub mod equalize_ui;
pub mod export_dialog;
<<<<<<< HEAD
pub mod file_forms;
=======
pub(crate) mod fade_modes;
>>>>>>> upstream/main
pub mod file_ui;
pub mod fill_ui;
pub mod filter_dialog;
pub(super) mod filter_preview_ui;
#[cfg(not(target_arch = "wasm32"))]
pub(super) mod filter_preview_worker;
<<<<<<< HEAD
pub(crate) mod format_prompts;
=======
mod fit_image_range;
#[cfg(test)]
mod fit_image_range_tests;
>>>>>>> upstream/main
pub mod gallery_ui;
pub(crate) mod guide_edit;
pub mod guide_layout_ui;
pub mod history_options_ui;
mod image_processor_ui;
pub mod jpeg_options_ui;
pub mod layer_picker;
pub mod layer_style;
pub mod liquify_ui;
pub mod live_shape_prompt;
pub mod mask_trash_prompt;
pub mod merge_channels_ui;
pub mod new_doc_ui;
pub mod new_group_ui;
pub mod new_guide_ui;
pub mod new_layer_ui;
pub mod open_prompt;
pub mod palette;
#[cfg(test)]
mod paths_export_default_tests;
#[cfg(test)]
mod paths_export_document_bounds_tests;
pub(crate) mod pattern_fill_ui;
pub mod preset_group_prompt;
pub mod rasterize_prompt;
pub mod screen_picker;
pub mod select_mask_ui;
pub mod shape_dialog;
pub(crate) mod shape_fill_ui;
pub(crate) mod shared_picture;
pub(super) mod sizing;
pub mod smart_filter_dialog;
pub mod snapshot_dialog;
pub(crate) mod solid_fill_ui;
pub mod stack_dialog;
pub mod stroke_ui;
pub mod targa_options_ui;
#[cfg(test)]
mod tests;
pub mod tiff_options_ui;
pub mod vp_ui;
pub mod wide_angle_ui;

/// Confirm and cancel a dialog from a test, on the app's own context.
#[cfg(test)]
pub(crate) mod testing {
    use craft::ui::dialog::DialogId;

    use crate::PhotocraftApp;

    /// Answers dialog `id` with OK and takes the results.
    pub(crate) fn confirm(app: &mut PhotocraftApp, id: DialogId) -> Result<(), String> {
        let ctx = app.ctx().clone();
        super::confirm(app, &ctx, id)
    }

    /// Cancels dialog `id` and takes the results.
    pub(crate) fn cancel(app: &mut PhotocraftApp, id: DialogId) -> Result<(), String> {
        let ctx = app.ctx().clone();
        super::cancel(app, &ctx, id)
    }

    /// Runs one pass of the dialog host, as a frame does. A hosted parameter form loads its remembered values and
    /// defaults when it first shows, so its fields read empty until a pass has run.
    pub(crate) fn show(app: &mut PhotocraftApp) {
        let ctx = app.ctx().clone();
        crate::testing::run_ui(&ctx, Default::default(), |ui| super::run(app, ui));
    }
}
