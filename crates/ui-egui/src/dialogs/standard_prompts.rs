//! The confirmations on the library's [`StandardPrompt`]: the disk-change and replace-file
//! questions. Each open prompt has a [`PromptRecord`] under the u64 id automation uses; the
//! record keeps the fields the legacy dialog had (`ui.inspect` and `ui.dialog.set` read and write
//! them) and the prompt carries the id as its tag, so an answer finds its record.
//!
//! The host holds one `StandardPrompt` at a time: opening a second replaces the first, whose
//! record is then swept as cancelled (gap AB).

use craft::ui::dialog::{DialogId, DialogResult, PromptChoice, StackDialog, StandardPrompt};
use serde_json::{Map, Value};

use crate::PhotocraftApp;

type Fields = Map<String, Value>;

/// One open prompt.
#[derive(Clone, Debug, PartialEq)]
pub struct PromptRecord {
    /// The id automation addresses the prompt by (`UiState::alloc_id`).
    pub id: u64,
    /// The library dialog's id, which finds it in the host.
    pub dialog: DialogId,
    /// The title `ui.inspect` lists.
    pub title: String,
    pub fields: Fields,
}

impl crate::state::UiState {
    pub fn prompt(&self, id: u64) -> Option<&PromptRecord> {
        self.prompts.iter().find(|p| p.id == id)
    }

    /// The open prompt whose fields carry `marker`.
    pub fn prompt_with(&self, owns: fn(&Fields) -> bool) -> Option<&PromptRecord> {
        self.prompts.iter().find(|p| owns(&p.fields) && self.host.get_dyn(p.dialog).is_some())
    }
}

/// Opens `prompt` with `fields` as the record's fields; `None` when a prompt is open already.
pub fn open(app: &mut PhotocraftApp, prompt: StandardPrompt, title: &str, fields: Fields) -> Option<u64> {
    if app.ui.host.is_open::<StandardPrompt>() {
        return None;
    }
    let id = app.ui.alloc_id();
    let dialog = StackDialog::id(&prompt);
    app.ui.host.open(prompt.tag(id));
    app.ui.prompts.push(PromptRecord { id, dialog, title: title.to_owned(), fields });
    Some(id)
}

/// Runs what the answer means for the prompt `record` was: `proceed` confirms, anything else cancels.
fn run(app: &mut PhotocraftApp, record: PromptRecord, proceed: bool) -> Result<Value, String> {
    let fields = &record.fields;
    if proceed {
        if crate::disk_change_prompt::owns(fields) {
            crate::disk_change_prompt::confirm(app, fields)
        } else if crate::delete_layer_prompt::owns(fields) {
            crate::delete_layer_prompt::confirm(app, fields)
        } else {
            crate::replace_prompt::confirm(app)
        }
    } else {
        crate::actions::stopped(app, fields);
        if crate::replace_prompt::owns(fields) { crate::replace_prompt::cancel(app) } else { Ok(Value::Null) }
    }
}

/// Closes prompt `id` in the host without an answer and takes its record.
fn close(app: &mut PhotocraftApp, ctx: &egui::Context, id: u64) -> Result<PromptRecord, String> {
    let at = app.ui.prompts.iter().position(|p| p.id == id).ok_or_else(|| format!("no dialog {id}"))?;
    let record = app.ui.prompts.remove(at);
    app.ui.host.cancel_by_id::<StandardPrompt>(ctx, record.dialog);
    app.ui.host.take_by_id::<StandardPrompt>(record.dialog);
    Ok(record)
}

/// `ui.dialog.confirm`: the default button's answer.
pub fn confirm(app: &mut PhotocraftApp, ctx: &egui::Context, id: u64) -> Result<Value, String> {
    let record = app.ui.prompt(id).ok_or_else(|| format!("no dialog {id}"))?;
    // A control request installs the session gate; synthetic input sets `automation_input`.
    // Either way the handler's own file access is judged before it runs, as its command would be.
    if (app.automation_input || app.session.authorize.is_some())
        && let Some(command) = crate::dialogs::file_command(&record.fields)
        && let Some(authorize) = app.services.automation_command.as_ref()
    {
        authorize(command, &Value::Object(record.fields.clone()))?;
    }
    // The host answers with the default button, which stores "Don't show again" through the Host.
    crate::host_settings::sync(app, ctx);
    let dialog = app.ui.prompt(id).ok_or_else(|| format!("no dialog {id}"))?.dialog;
    if app.ui.host.confirm_by_id::<StandardPrompt>(ctx, dialog) {
        crate::host_settings::sync(app, ctx);
        app.ui.host.take_by_id::<StandardPrompt>(dialog);
        let at = app.ui.prompts.iter().position(|p| p.id == id).ok_or_else(|| format!("no dialog {id}"))?;
        let record = app.ui.prompts.remove(at);
        return run(app, record, true);
    }
    let record = close(app, ctx, id)?;
    run(app, record, true)
}

/// `ui.dialog.cancel`.
pub fn cancel(app: &mut PhotocraftApp, ctx: &egui::Context, id: u64) -> Result<Value, String> {
    let record = close(app, ctx, id)?;
    run(app, record, false)
}

/// `ui.dialog.set`: a prompt's fields are its question's data; the markers stay.
pub fn set_field(app: &mut PhotocraftApp, id: u64, field: &str, value: Value) -> Result<(), String> {
    let record = app.ui.prompts.iter_mut().find(|p| p.id == id).ok_or_else(|| format!("no dialog {id}"))?;
    if field == crate::actions::RESUME {
        return Err("the paused action token is read-only".into());
    }
    if !record.fields.contains_key(field) && crate::dialogs::is_route_marker(field) {
        return Err(format!("`{field}` is set only by the dialog it belongs to"));
    }
    if field == "skip" && crate::delete_layer_prompt::owns(&record.fields) {
        let skip = value.as_bool().ok_or("`skip` takes true or false")?;
        let dialog = record.dialog;
        record.fields.insert(field.to_owned(), value);
        if let Some(prompt) = app.ui.host.get_mut_by_id::<StandardPrompt>(dialog) {
            prompt.set_skip(skip);
        }
        return Ok(());
    }
    record.fields.insert(field.to_owned(), value);
    Ok(())
}

/// The prompts' answers, after `DialogHost::show`: Proceed runs the confirm, any other answer the
/// cancel. A record whose dialog is gone without an answer (replaced by another prompt) counts as
/// cancelled.
pub fn collect(app: &mut PhotocraftApp) {
    let mut errors = Vec::new();
    while let Some(result) = app.ui.host.take::<StandardPrompt>() {
        let DialogResult::Ok(mut answer) = result else { continue };
        let Some(record) = answer.take_tag::<u64>().and_then(|id| app.ui.prompts.iter().position(|p| p.id == id)).map(|at| app.ui.prompts.remove(at)) else {
            continue;
        };
        if let Err(error) = run(app, record, answer.choice == PromptChoice::Proceed) {
            errors.push(error);
        }
    }
    sweep(app);
    if let Some(error) = errors.pop() {
        app.ui.status = error;
    }
}

/// Runs the cancel of every record whose prompt is no longer in the host.
pub fn sweep(app: &mut PhotocraftApp) {
    let gone: Vec<u64> = app.ui.prompts.iter().filter(|p| app.ui.host.get_dyn(p.dialog).is_none()).map(|p| p.id).collect();
    for id in gone {
        if let Some(at) = app.ui.prompts.iter().position(|p| p.id == id) {
            let record = app.ui.prompts.remove(at);
            let _ = run(app, record, false);
        }
    }
}
