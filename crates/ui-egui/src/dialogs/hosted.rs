//! Dialogs that the library's `DialogHost` shows beside the legacy `UiState::dialogs` list.
//!
//! The host and the legacy loop in [`super::show`] draw against one `DialogStack`, so modality and
//! top-ness are common. Each hosted dialog has a [`Hosted`] record under the u64 id automation
//! uses; the record's key gives the library's dialog id, which finds the dialog in the host. The
//! record also holds what the library dialog does not: the `__` keys, the cancel cleanup's input.

use craft::ui::dialog::{Body, CommandForm, DialogFields, DialogHost, FieldValue, Param, ParamDialog, ParamKind, PathKind, PathRequest, Width, dialog_id};
use serde_json::{Value, json};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

use crate::PhotocraftApp;

type Fields = serde_json::Map<String, Value>;

/// Which library dialog type a record's dialog is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostedKind {
    /// A [`CommandForm`] (`dialogs::hosted::open_form`).
    Form,
    /// A [`ParamDialog`] (`filter_dialog::open_hosted`).
    Params,
}

/// State the footer hook sets and the app reads after the dialog drew (the hook cannot reach the
/// app). Equal to every other, like [`HostCell`].
#[derive(Clone, Debug, Default)]
pub struct Pending<T>(pub Arc<Mutex<T>>);

impl<T> PartialEq for Pending<T> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// One open hosted dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct Hosted {
    pub kind: HostedKind,
    /// The id automation addresses the dialog by (`UiState::alloc_id`).
    pub id: u64,
    /// The library dialog's own key (`CommandForm::key`). The host knows a dialog by
    /// `dialog_id(key)`, so this is also how a record finds its dialog.
    pub key: String,
    pub params: Vec<Param>,
    /// The dialog's title as `dialogs::title` gives it (untranslated, no trailing "…").
    pub title: String,
    /// Fields the dialog does not draw (a list a form carries to its command, or a key a script
    /// added): they ride along into the command's parameters.
    pub extra: Fields,
    /// The `__` keys of the legacy fields (`__command`, `__label`, `__spec`, `__docs`, the
    /// paused-action RESUME token).
    pub private: Fields,
    /// Parameters that hold an open document's index (shown as a menu of the documents' names).
    pub documents: Vec<String>,
    /// Parameters whose opening value was a JSON integer: a slider's f64 reads back as one.
    pub whole: Vec<String>,
    /// The Color Table footer's pressed button.
    pub table: Pending<Option<crate::color_table_ui::Action>>,
    /// The textures of the preview pane (filter dialogs).
    pub preview: crate::filter_preview_ui::PaneCache,
}

/// `DialogHost` as a `UiState` field: not saved, and a clone starts empty (open dialogs belong to
/// one session's UI, like the legacy list's egui state).
#[derive(Debug, Default)]
pub struct HostCell(pub DialogHost);

impl Clone for HostCell {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PartialEq for HostCell {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Deref for HostCell {
    type Target = DialogHost;
    fn deref(&self) -> &DialogHost {
        &self.0
    }
}

impl DerefMut for HostCell {
    fn deref_mut(&mut self) -> &mut DialogHost {
        &mut self.0
    }
}

impl crate::state::UiState {
    /// The record of hosted dialog `id`.
    pub fn hosted(&self, id: u64) -> Option<&Hosted> {
        self.hosted.iter().find(|h| h.id == id)
    }

    /// The library form of hosted dialog `id`, found in the host by its dialog id.
    pub fn hosted_form(&self, id: u64) -> Option<&CommandForm> {
        let record = self.hosted(id).filter(|h| h.kind == HostedKind::Form)?;
        self.host.get_by_id::<CommandForm>(dialog_id(record.key.as_str()))
    }

    /// The library parameter dialog of hosted dialog `id`.
    pub fn hosted_params(&self, id: u64) -> Option<&ParamDialog> {
        let record = self.hosted(id).filter(|h| h.kind == HostedKind::Params)?;
        self.host.get_by_id::<ParamDialog>(dialog_id(record.key.as_str()))
    }

    /// The library fields of hosted dialog `id`, read from the host.
    pub fn hosted_fields(&self, id: u64) -> Option<&DialogFields> {
        match self.hosted(id)?.kind {
            HostedKind::Form => self.hosted_form(id).map(CommandForm::fields),
            HostedKind::Params => self.hosted_params(id).map(ParamDialog::fields),
        }
    }

    pub(crate) fn hosted_fields_mut(&mut self, id: u64) -> Option<&mut DialogFields> {
        let record = self.hosted(id)?;
        let (kind, did) = (record.kind, dialog_id(record.key.as_str()));
        let dialog = self.host.get_dyn_mut(did)?;
        match kind {
            HostedKind::Form => dialog.downcast_mut::<CommandForm>().map(CommandForm::fields_mut),
            HostedKind::Params => dialog.downcast_mut::<ParamDialog>().map(ParamDialog::fields_mut),
        }
    }

    /// The JSON fields of dialog `id`: the legacy list first, then the hosted registry.
    pub fn dialog_fields(&self, id: u64) -> Option<Fields> {
        if let Some(d) = self.dialogs.iter().find(|d| d.id == id) {
            return Some(d.fields.clone());
        }
        let record = self.hosted(id)?;
        let library = self.hosted_fields(id)?;
        let mut fields = match record.kind {
            HostedKind::Form => fields_json(library),
            HostedKind::Params => crate::filter_dialog::Wire::of(record).json(library),
        };
        fields.extend(record.extra.clone());
        fields.extend(record.private.clone());
        // The Preview check box is the library's: it reads as the dialog shows it (on and not held
        // to compare). `ui.dialog.set` of `__preview` false also turns it off: the library has no
        // setter for the check box (gap L).
        if record.kind == HostedKind::Params && fields.contains_key("__preview") {
            let wanted = record.private.get("__preview").and_then(Value::as_bool) != Some(false);
            let on = self.hosted_params(id).is_some_and(ParamDialog::preview_on);
            fields.insert("__preview".into(), json!(on && wanted));
        }
        Some(fields)
    }

    /// Whether dialog `id` is open, legacy or hosted.
    pub fn dialog_open(&self, id: u64) -> bool {
        self.dialogs.iter().any(|d| d.id == id) || self.hosted(id).is_some()
    }

    /// Whether a hosted dialog is open (or a startup modal waits).
    pub fn hosted_open(&self) -> bool {
        !self.host.is_empty()
    }

    /// Whether any dialog, legacy or hosted, is open: the app's input gates wait meanwhile.
    pub fn any_dialog_open(&self) -> bool {
        !self.dialogs.is_empty() || self.hosted_open()
    }

    /// The legacy dialog that takes the input, if no hosted dialog is above it. A legacy dialog the
    /// stack has not seen yet was opened after everything on it.
    pub fn legacy_top(&self) -> Option<&crate::state::Dialog> {
        let last = self.dialogs.last()?;
        let id = craft::ui::dialog::dialog_id(last.id);
        let stack = self.host.stack();
        (!stack.contains(id) || stack.is_top(id)).then_some(last)
    }
}

/// The library fields as the JSON map automation reads and commands take.
pub fn fields_json(fields: &DialogFields) -> Fields {
    fields.iter().filter_map(|(key, value)| Some((key.to_owned(), serde_json::to_value(value).ok()?))).collect()
}

/// A JSON value as the field value `kind` holds, or why it does not fit.
pub fn field_from_json(kind: &ParamKind, value: &Value) -> Result<FieldValue, String> {
    let parsed = match kind {
        // The engine clamps a slider's value, so a number outside the slider's range is kept.
        ParamKind::Range { .. } | ParamKind::Number { .. } | ParamKind::Angle { .. } => value.as_f64().filter(|n| n.is_finite()).map(FieldValue::Number),
        ParamKind::Int { .. } => {
            value.as_i64().or_else(|| value.as_f64().filter(|n| n.fract() == 0.0 && n.abs() < 9.0e15).map(|n| n as i64)).map(FieldValue::Int)
        }
        // The host's own row decides what it keeps under its key.
        ParamKind::Custom(_) => serde_json::from_value::<FieldValue>(value.clone()).ok(),
        _ => serde_json::from_value::<FieldValue>(value.clone()).ok().filter(|v| kind.fits(v)),
    };
    parsed.ok_or_else(|| format!("{value} is not a value this field takes"))
}

/// `ui.dialog.set` for a hosted dialog: a drawn field takes the value by its kind, a `__` key
/// goes to the private record, any other key rides along to the command.
pub fn set_field(app: &mut PhotocraftApp, id: u64, field: &str, value: Value) -> Result<(), String> {
    let at = app.ui.hosted.iter().position(|h| h.id == id).ok_or_else(|| format!("no dialog {id}"))?;
    let record = &mut app.ui.hosted[at];
    if field == crate::actions::RESUME {
        return Err("the paused action token is read-only".into());
    }
    if field.starts_with("__") {
        if !record.private.contains_key(field) && crate::dialogs::is_route_marker(field) {
            return Err(format!("`{field}` is set only by the dialog it belongs to"));
        }
        record.private.insert(field.to_owned(), value);
        return Ok(());
    }
    let (kind, drawn) = (record.kind, record.params.iter().any(|p| p.key == field));
    if kind == HostedKind::Params {
        // Rows the host draws keep their values under keys of their own (`centerY`): any key the
        // dialog already holds is a field.
        let known = drawn || app.ui.hosted_fields(id).is_some_and(|f| f.contains(field));
        if known {
            let wire = crate::filter_dialog::Wire::of(&app.ui.hosted[at]);
            let fields = app.ui.hosted_fields(id).cloned().ok_or_else(|| format!("no dialog {id}"))?;
            let mut next = fields;
            wire.set(&mut next, field, &value).map_err(|e| format!("`{field}`: {e}"))?;
            *app.ui.hosted_fields_mut(id).ok_or_else(|| format!("no dialog {id}"))? = next;
            let whole = &mut app.ui.hosted[at].whole;
            whole.retain(|w| w != field);
            if value.is_i64() || value.is_u64() {
                whole.push(field.to_owned());
            }
            return Ok(());
        }
        app.ui.hosted[at].extra.insert(field.to_owned(), value);
        return Ok(());
    }
    let Some(param) = record.params.iter().find(|p| p.key == field) else {
        record.extra.insert(field.to_owned(), value);
        return Ok(());
    };
    let value = field_from_json(&param.kind, &value).map_err(|e| format!("`{field}`: {e}"))?;
    app.ui.hosted_fields_mut(id).ok_or_else(|| format!("no dialog {id}"))?.set(field, value);
    Ok(())
}

/// Sets `values` in dialog `id`, hosted or legacy, as a script's `ui.dialog.set` would, except that
/// a `__` key is written to the dialog's private fields whatever it is. A value the dialog's row
/// cannot hold leaves the row as it was.
pub fn seed(app: &mut PhotocraftApp, id: u64, values: impl IntoIterator<Item = (String, Value)>) {
    if let Some(at) = app.ui.hosted.iter().position(|h| h.id == id) {
        for (key, value) in values {
            if key.starts_with("__") {
                app.ui.hosted[at].private.insert(key, value);
            } else {
                let _ = set_field(app, id, &key, value);
            }
        }
    } else if let Some(d) = app.ui.dialog_mut(id) {
        d.fields.extend(values);
    }
}

/// Removes the fields `keys` from dialog `id`, hosted or legacy: the command keeps its own
/// defaults for them.
pub fn remove_fields(app: &mut PhotocraftApp, id: u64, keys: &[&str]) {
    if let Some(record) = app.ui.hosted.iter_mut().find(|h| h.id == id) {
        for key in keys {
            record.extra.remove(*key);
        }
    }
    if let Some(fields) = app.ui.hosted_fields_mut(id) {
        for key in keys {
            fields.remove(key);
        }
    } else if let Some(d) = app.ui.dialog_mut(id) {
        for key in keys {
            d.fields.remove(*key);
        }
    }
}

/// What a form needs beyond its rows.
pub struct FormSpec {
    pub command: String,
    pub label: String,
    pub params: Vec<Param>,
    pub values: DialogFields,
    pub extra: Fields,
    pub private: Fields,
    pub width: Width,
    /// A read-only section under the rows (File Info's Camera Data).
    pub below: Option<Box<dyn Fn(&mut egui::Ui, &DialogFields) + Send>>,
}

/// Opens a form for its command: the command runs with the fields at OK. A form already open is
/// replaced (the host holds one `CommandForm`), with its cancel cleanup.
pub fn open_form(app: &mut PhotocraftApp, spec: FormSpec) -> u64 {
    let FormSpec { command, label, params, values, extra, private, width, below } = spec;
    let id = app.ui.alloc_id();
    let title = label.trim_end_matches('…').to_owned();
    let mut form = CommandForm::new(command.as_str(), crate::dialogs::display_label(&label), params.clone())
        .values(values)
        .width(width)
        .body(Body::Fit)
        // Cmd/Ctrl+Z steps the form's own settings (#4697), and the close box follows the platform.
        .history(true)
        .close_label(crate::dialogs::CLOSE_BOX.then(|| tl!("Close").to_string()));
    if let Some(below) = below {
        form = form.below(below);
    }
    // Opening over a form replaces it: its Cancel waits in the host with its key.
    app.ui.host.open(form);
    cancel_replaced::<CommandForm>(app);
    let record = Hosted {
        kind: HostedKind::Form,
        id,
        key: command,
        params,
        title,
        extra,
        private,
        documents: Vec::new(),
        whole: Vec::new(),
        table: Pending::default(),
        preview: Default::default(),
    };
    app.ui.hosted.push(record);
    id
}

/// Opens a parameter dialog, replacing the one open (the host holds one `ParamDialog`) with its
/// cancel cleanup. The caller has built the dialog and the record's fields.
pub fn open_params(app: &mut PhotocraftApp, dialog: ParamDialog, mut record: Hosted) -> u64 {
    let id = app.ui.alloc_id();
    record.id = id;
    record.kind = HostedKind::Params;
    app.ui.host.open(dialog);
    cancel_replaced::<ParamDialog>(app);
    app.ui.hosted.push(record);
    id
}

/// A dialog an `open` of the same type replaced says `Cancel` with its key: run its cleanup.
fn cancel_replaced<D: craft::ui::dialog::StackDialog>(app: &mut PhotocraftApp) {
    if let Some((_, Some(old))) = app.ui.host.take_keyed::<D>()
        && let Some(old) = app.ui.hosted.iter().find(|h| h.key == old).map(|h| h.id)
    {
        cancelled(app, old);
    }
}

/// The command a closed form runs: the dialog is gone, so the preview and the paused action settle
/// with the command's result.
fn run_form(app: &mut PhotocraftApp, record: Hosted, mut params: Fields) -> Result<Value, String> {
    params.extend(record.extra);
    app.filter_preview = None;
    let result = app.run(&record.key, Value::Object(params));
    if result.is_ok() {
        crate::actions::resume(app, &record.private)?;
    } else {
        crate::actions::abandoned(app, &record.private);
    }
    result
}

/// The command a closed parameter dialog runs, as a legacy filter dialog's OK does (the displace
/// map question, smart filter edits, remembered values, the paused action).
fn run_params(app: &mut PhotocraftApp, record: Hosted, answer: &DialogFields) -> Result<Value, String> {
    let mut fields = crate::filter_dialog::Wire::of(&record).json(answer);
    fields.extend(record.extra);
    fields.extend(record.private);
    let dialog = crate::state::Dialog { id: record.id, kind: crate::state::DialogKind::Command, fields };
    crate::dialogs::confirm_generic_command(app, dialog)
}

/// `ui.dialog.confirm` for a hosted dialog: OK as the default button answers it (refused while OK
/// is disabled), then the same path a click takes.
pub fn confirm(app: &mut PhotocraftApp, ctx: &egui::Context, id: u64) -> Result<Value, String> {
    let kind = app.ui.hosted(id).map(|h| h.kind).ok_or_else(|| format!("no dialog {id}"))?;
    if app.ui.hosted_fields(id).is_none() {
        return Err(format!("no dialog {id}"));
    }
    let confirmed = match kind {
        HostedKind::Form => app.ui.host.confirm::<CommandForm>(ctx),
        HostedKind::Params => app.ui.host.confirm::<ParamDialog>(ctx),
    };
    if !confirmed {
        return Err("OK is disabled: the dialog's values are not complete".into());
    }
    settle(app, kind).unwrap_or_else(|| Err(format!("no dialog {id}")))
}

/// The dialogs' answers, after `DialogHost::show`: OK runs the command, Cancel runs the cleanup.
pub fn collect(app: &mut PhotocraftApp) {
    browse(app);
    crate::filter_dialog::requests(app);
    for kind in [HostedKind::Form, HostedKind::Params] {
        if let Some(Err(error)) = settle(app, kind) {
            app.ui.status = error;
        }
    }
}

/// Takes the dialog's stored answer. OK runs the command and returns its result; Cancel runs the
/// cleanup of the record the answer's key names.
fn settle(app: &mut PhotocraftApp, kind: HostedKind) -> Option<Result<Value, String>> {
    let (result, key) = match kind {
        HostedKind::Form => {
            let (result, key) = app.ui.host.take_keyed::<CommandForm>()?;
            (result.ok().map(|answer| answer.fields), key)
        }
        HostedKind::Params => {
            let (result, key) = app.ui.host.take_keyed::<ParamDialog>()?;
            (result.ok().map(|answer| answer.fields), key)
        }
    };
    let record = app.ui.hosted.iter().find(|h| Some(&h.key) == key.as_ref() && h.kind == kind).cloned()?;
    match result {
        Some(answer) => {
            app.ui.hosted.retain(|h| h.id != record.id);
            Some(match kind {
                HostedKind::Form => run_form(app, record, fields_json(&answer)),
                HostedKind::Params => run_params(app, record, &answer),
            })
        }
        None => {
            cancelled(app, record.id);
            None
        }
    }
}

/// The Browse… button of a path row, once pressed: opens the file or folder dialog, and the answer
/// fills the row.
fn browse(app: &mut PhotocraftApp) {
    let pressed =
        if let Some(form) = app.ui.host.get_mut::<CommandForm>() { form.pick_path().map(|r| (r, form.fields().clone(), form.key().to_owned())) } else { None };
    let pressed = pressed.or_else(|| {
        let dialog = app.ui.host.get_mut::<ParamDialog>()?;
        dialog.pick_path().map(|r| (r, dialog.fields().clone(), dialog.key().to_owned()))
    });
    let Some((PathRequest { key: field, kind }, fields, key)) = pressed else { return };
    let current = fields.text(&field).unwrap_or_default().to_owned();
    let Some(id) = app.ui.hosted.iter().find(|h| h.key == key).map(|h| h.id) else { return };
    let field_for_ext = field.clone();
    let fill = move |app: &mut PhotocraftApp, path: String| fill_path(app, id, &field, path);
    let result = match kind {
        PathKind::Folder => app.ask_folder(fill),
        PathKind::OpenFile => {
            let initial_dir = crate::file_dialog::last_used_dir(&app.ui.recent_files);
            // The library's path request carries no file types (gap S): Install Plug-in… takes `.wasm`.
            let extensions = (key == "plugin.install" && field_for_ext == "path").then(|| vec!["wasm".to_string()]);
            let request = crate::file_dialog::FileDialogRequest::Open { multiple: false, initial_dir, extensions };
            app.ask_file(request, move |app, answer| match answer {
                crate::file_dialog::FileDialogAnswer::Paths(mut paths) if !paths.is_empty() => fill(app, paths.remove(0)),
                _ => Err("the file dialog gave an unexpected answer".into()),
            })
        }
        PathKind::SaveFile => app.ask_file(crate::file_dialog::FileDialogRequest::Save { suggested: current }, move |app, answer| match answer {
            crate::file_dialog::FileDialogAnswer::SaveTo(path) => fill(app, path),
            _ => Err("the file dialog gave an unexpected answer".into()),
        }),
    };
    crate::dialogs::report_browse(app, result);
}

fn fill_path(app: &mut PhotocraftApp, id: u64, key: &str, path: String) -> Result<Value, String> {
    let fields = app.ui.hosted_fields_mut(id).ok_or("the dialog was closed")?;
    fields.set(key, FieldValue::Text(path));
    Ok(Value::Null)
}

/// The colour a Color Picker child answers with, for the colour parameter or private key `key` of
/// hosted dialog `id` (`color_picker_ui::confirm`'s `field` target).
pub fn set_color(app: &mut PhotocraftApp, id: u64, key: &str, hex: &str) -> Result<(), String> {
    let rgb = crate::color_picker_ui::parse_hex(hex).ok_or("not a colour")?;
    if key.starts_with("__") {
        app.ui.hosted.iter_mut().find(|h| h.id == id).ok_or("the dialog this colour was for is closed")?.private.insert(key.to_owned(), json!(hex));
        return Ok(());
    }
    app.ui.hosted_fields_mut(id).ok_or("the dialog this colour was for is closed")?.set(key, FieldValue::Color(rgb));
    Ok(())
}

/// Runs PC's cancel cleanup for hosted dialog `id` and drops its record: the paused action it
/// belongs to stops and the filter preview goes. The caller has already closed the library dialog
/// (`host.cancel::<D>()`, or the replacement by an `open` of the same type).
pub fn cancelled(app: &mut PhotocraftApp, id: u64) -> Option<Hosted> {
    let at = app.ui.hosted.iter().position(|h| h.id == id)?;
    let record = app.ui.hosted.remove(at);
    crate::actions::stopped(app, &record.private);
    app.filter_preview = None;
    Some(record)
}

/// `ui.dialog.cancel` for a hosted dialog: closes the library dialog, then runs the cleanup.
pub fn cancel(app: &mut PhotocraftApp, ctx: &egui::Context, id: u64) -> Result<Value, String> {
    let kind = app.ui.hosted(id).map(|h| h.kind).ok_or_else(|| format!("no dialog {id}"))?;
    if app.ui.hosted_fields(id).is_none() {
        return Err(format!("no dialog {id}"));
    }
    match kind {
        HostedKind::Form => {
            app.ui.host.cancel::<CommandForm>(ctx);
            app.ui.host.take_keyed::<CommandForm>();
        }
        HostedKind::Params => {
            app.ui.host.cancel::<ParamDialog>(ctx);
            app.ui.host.take_keyed::<ParamDialog>();
        }
    }
    cancelled(app, id);
    Ok(Value::Null)
}

/// Test helpers for the open hosted filter dialog: set a field as automation would, read its JSON.
#[cfg(test)]
pub(crate) fn test_set(app: &mut PhotocraftApp, key: &str, value: Value) {
    let id = app.ui.hosted.last().expect("a hosted dialog is open").id;
    set_field(app, id, key, value).unwrap();
}

#[cfg(test)]
pub(crate) fn test_fields(app: &PhotocraftApp) -> Fields {
    let id = app.ui.hosted.last().expect("a hosted dialog is open").id;
    app.ui.dialog_fields(id).unwrap()
}

#[cfg(test)]
pub(crate) fn test_field(app: &PhotocraftApp, key: &str) -> Value {
    let id = app.ui.hosted.last().expect("a hosted dialog is open").id;
    app.ui.dialog_fields(id).unwrap().get(key).cloned().unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft::ui::dialog::{DialogId, DialogResult, DialogStack, StackDialog, dialog_id};
    use egui::{Modifiers, vec2};
    use egui_kittest::Harness;
    use serde_json::json;

    /// A hosted dialog that draws an alert.
    struct Probe;

    impl StackDialog for Probe {
        type Result = ();
        fn id(&self) -> DialogId {
            dialog_id("hosted-probe")
        }
        fn show(&mut self, ctx: &egui::Context, stack: &DialogStack) -> Option<DialogResult<()>> {
            craft::ui::dialog::Alert::new("hosted-probe", "Probe", "Probe").ok_label("OK").stack(stack).show(ctx).then_some(DialogResult::Cancel)
        }
    }

    fn harness() -> Harness<'static, PhotocraftApp> {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
        app.sync_views();
        app.ui.tool = crate::state::Tool::RectMarquee;
        let mut h = Harness::builder().with_size(vec2(1200.0, 800.0)).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                let ctx = ui.ctx().clone();
                if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                    return;
                }
                egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
                crate::dialogs::show(app, &ctx);
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
        h.run_steps(4);
        h
    }

    /// Drags a marquee across the canvas centre; whether it made a selection.
    fn strokes(h: &mut Harness<'static, PhotocraftApp>) -> bool {
        let c = h.state().last_canvas_rect.center();
        let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        h.event(egui::Event::PointerMoved(c));
        h.run_steps(1);
        h.event(button(c, true));
        let mut q = c;
        for i in 1..=8 {
            q = c + vec2(12.0 * i as f32, 8.0 * i as f32);
            h.event(egui::Event::PointerMoved(q));
            h.run_steps(1);
        }
        h.event(button(q, false));
        h.run_steps(3);
        h.state().session.active().unwrap().doc.selection.is_some()
    }

    #[test]
    fn a_hosted_dialog_blocks_the_canvas_and_stacks_with_the_legacy_ones() {
        let mut free = harness();
        assert!(free.state().session.active().unwrap().doc.selection.is_none());
        assert!(strokes(&mut free), "the marquee selects when no dialog is open");

        let mut h = harness();
        h.state_mut().ui.host.open(Probe);
        h.run_steps(2);
        assert!(h.state().ui.hosted_open() && h.state().ui.any_dialog_open());
        assert!(!strokes(&mut h), "a hosted dialog keeps the marquee off the canvas");

        // A legacy dialog opened over it is the stack top; the hosted one is below.
        let mut h = harness();
        h.state_mut().ui.host.open(Probe);
        h.run_steps(2);
        crate::dialogs::open_command_dialog(h.state_mut(), "image.adjustments.brightnessContrast", "Brightness/Contrast…");
        h.run_steps(3);
        let legacy = h.state().ui.dialogs.last().unwrap().id;
        assert_eq!(h.state().ui.host.stack().top(), Some(dialog_id(legacy)));
        assert!(h.state().ui.legacy_top().is_some());
        assert!(!h.state().ui.host.stack().is_top(Probe.id()));

        // The other way round the hosted dialog is on top, and the legacy one does not take
        // canvas input.
        h.state_mut().ui.host.open(Probe);
        h.run_steps(2);
        assert_eq!(h.state().ui.host.stack().top(), Some(Probe.id()));
        assert!(h.state().ui.legacy_top().is_none());

        // Closing the legacy dialog takes it off the shared stack.
        crate::dialogs::cancel(h.state_mut(), legacy).unwrap();
        h.run_steps(2);
        assert!(!h.state().ui.host.stack().contains(dialog_id(legacy)));
    }

    fn open_fit_image(h: &mut Harness<'static, PhotocraftApp>) -> u64 {
        let ctx = h.ctx.clone();
        let r = crate::menus::invoke(h.state_mut(), &ctx, "file.automate.fitImage", json!({})).unwrap();
        h.run_steps(3);
        r["dialog"].as_u64().unwrap()
    }

    #[test]
    fn a_form_is_modal_esc_cancels_it_and_ok_runs_its_command_once() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness();
        let id = open_fit_image(&mut h);
        assert!(h.state().ui.hosted(id).is_some() && h.state().ui.dialogs.is_empty());
        assert!(h.state().ui.any_dialog_open());
        assert!(!crate::menus::modal_allows(h.state(), "edit.fill"), "a menu command waits for the form");
        assert!(!strokes(&mut h), "the form keeps the marquee off the canvas");

        // Esc cancels: no command ran and the record is gone.
        let revision = h.state().session.active().unwrap().revision;
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert!(h.state().ui.hosted(id).is_none() && !h.state().ui.any_dialog_open());
        assert_eq!(h.state().session.active().unwrap().revision, revision);
        assert_eq!(h.state().session.active().unwrap().doc.size.width, 400);

        // OK runs Fit Image once with the edited value.
        let id = open_fit_image(&mut h);
        set_field(h.state_mut(), id, "width", json!(55)).unwrap();
        assert!(set_field(h.state_mut(), id, "width", json!("wide")).is_err(), "a value of the wrong shape is refused");
        h.run_steps(2);
        let past = h.state().session.active().unwrap().history.past_len();
        h.get_by_label("OK").click();
        h.run_steps(3);
        assert!(h.state().ui.hosted(id).is_none() && !h.state().ui.any_dialog_open());
        let st = h.state().session.active().unwrap();
        assert_eq!(st.doc.size.width, 55);
        assert_eq!(st.history.past_len(), past + 1);
    }

    #[test]
    fn automation_confirm_is_refused_while_ok_is_disabled() {
        let mut h = harness();
        let id = open_fit_image(&mut h);
        set_field(h.state_mut(), id, "width", json!(55)).unwrap();
        h.state_mut().ui.host.get_mut::<CommandForm>().unwrap().set_ok_enabled(false);
        let ctx = h.ctx.clone();
        let error = crate::dialogs::confirm_in(h.state_mut(), &ctx, id).unwrap_err();
        assert!(error.contains("OK is disabled"), "{error}");
        assert!(h.state().ui.hosted(id).is_some() && h.state().ui.host.is_open::<CommandForm>(), "the dialog stays open");
        assert_eq!(h.state().session.active().unwrap().doc.size.width, 400, "the command did not run");

        h.state_mut().ui.host.get_mut::<CommandForm>().unwrap().set_ok_enabled(true);
        crate::dialogs::confirm_in(h.state_mut(), &ctx, id).unwrap();
        assert!(h.state().ui.hosted(id).is_none() && !h.state().ui.any_dialog_open());
        assert_eq!(h.state().session.active().unwrap().doc.size.width, 55);
    }

    #[test]
    fn opening_a_form_over_another_cancels_the_first_by_its_key() {
        let mut h = harness();
        let first = open_fit_image(&mut h);
        let ctx = h.ctx.clone();
        let second = crate::menus::invoke(h.state_mut(), &ctx, "file.automate.conditionalModeChange", json!({})).unwrap()["dialog"].as_u64().unwrap();
        h.run_steps(2);
        assert!(h.state().ui.hosted(first).is_none(), "the replaced form's record is gone");
        assert!(h.state().ui.hosted(second).is_some() && h.state().ui.hosted_fields(second).is_some());
        assert!(h.state().ui.hosted_fields(first).is_none());
    }
}
