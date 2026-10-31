//! Dialogs that the library's `DialogHost` shows beside the legacy `UiState::dialogs` list.
//!
//! The host and the legacy loop in [`super::show`] draw against one `DialogStack`, so modality and
//! top-ness are common. Each hosted dialog has a [`Hosted`] record under the u64 id automation
//! uses; the library host knows dialogs only by type, so the record is how `ui.dialog.*` and the
//! cancel cleanup find one.

use craft::ui::dialog::{CommandForm, DialogFields, DialogHost, DialogResult, FieldValue, Param, ParamDialog, ParamKind, Width};
use serde_json::Value;
use std::ops::{Deref, DerefMut};

use crate::PhotocraftApp;

type Fields = serde_json::Map<String, Value>;

/// Which library dialog type a record points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostedKind {
    Param,
    Form,
}

/// One open hosted dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct Hosted {
    /// The id automation addresses the dialog by (`UiState::alloc_id`).
    pub id: u64,
    pub kind: HostedKind,
    /// The library dialog's own key (`ParamDialog::key`, `CommandForm::key`): the host finds the
    /// dialog by type, so a lookup checks the key before it trusts the type.
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

    /// The library fields of hosted dialog `id`, read from the host.
    pub fn hosted_fields(&self, id: u64) -> Option<&DialogFields> {
        let record = self.hosted(id)?;
        match record.kind {
            HostedKind::Param => self.host.get::<craft::ui::dialog::ParamDialog>().filter(|d| d.key() == record.key).map(|d| d.fields()),
            HostedKind::Form => self.host.get::<craft::ui::dialog::CommandForm>().filter(|d| d.key() == record.key).map(|d| d.fields()),
        }
    }

    /// The JSON fields of dialog `id`: the legacy list first, then the hosted registry.
    pub fn dialog_fields(&self, id: u64) -> Option<Fields> {
        if let Some(d) = self.dialogs.iter().find(|d| d.id == id) {
            return Some(d.fields.clone());
        }
        let record = self.hosted(id)?;
        let mut fields = fields_json(self.hosted_fields(id)?);
        fields.extend(record.extra.clone());
        fields.extend(record.private.clone());
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
        ParamKind::Range { .. } | ParamKind::Angle { .. } => value.as_f64().filter(|n| n.is_finite()).map(FieldValue::Number),
        ParamKind::Int { .. } => {
            value.as_i64().or_else(|| value.as_f64().filter(|n| n.fract() == 0.0 && n.abs() < 9.0e15).map(|n| n as i64)).map(FieldValue::Int)
        }
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
    let Some(param) = record.params.iter().find(|p| p.key == field) else {
        record.extra.insert(field.to_owned(), value);
        return Ok(());
    };
    let value = field_from_json(&param.kind, &value).map_err(|e| format!("`{field}`: {e}"))?;
    let (kind, key) = (record.kind, record.key.clone());
    let fields = match kind {
        HostedKind::Param => app.ui.host.get_mut::<ParamDialog>().filter(|d| d.key() == key).map(|d| d.fields_mut()),
        HostedKind::Form => app.ui.host.get_mut::<CommandForm>().filter(|d| d.key() == key).map(|d| d.fields_mut()),
    };
    fields.ok_or_else(|| format!("no dialog {id}"))?.set(field, value);
    Ok(())
}

/// Opens a form for `command`: the command runs with the fields at OK. A form already open is
/// replaced (the host holds one `CommandForm`), with its cancel cleanup.
#[allow(clippy::too_many_arguments)]
pub fn open_form(
    app: &mut PhotocraftApp,
    command: &str,
    label: &str,
    params: Vec<Param>,
    values: DialogFields,
    extra: Fields,
    private: Fields,
    width: Width,
) -> u64 {
    if let Some(old) = app.ui.hosted.iter().find(|h| h.kind == HostedKind::Form).map(|h| h.id) {
        app.ui.host.cancel::<CommandForm>();
        app.ui.host.take::<CommandForm>();
        cancelled(app, old);
    }
    let id = app.ui.alloc_id();
    let title = label.trim_end_matches('…').to_owned();
    let form = CommandForm::new(command, crate::dialogs::display_label(label), params.clone()).values(values).width(width);
    app.ui.hosted.push(Hosted { id, kind: HostedKind::Form, key: command.to_owned(), params, title, extra, private });
    app.ui.host.open(form);
    id
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

/// `ui.dialog.confirm` for a hosted dialog. The library has no programmatic OK, so this closes the
/// dialog as a cancel and runs its command with the fields it holds.
pub fn confirm(app: &mut PhotocraftApp, id: u64) -> Result<Value, String> {
    let params = fields_json(app.ui.hosted_fields(id).ok_or_else(|| format!("no dialog {id}"))?);
    let at = app.ui.hosted.iter().position(|h| h.id == id).ok_or_else(|| format!("no dialog {id}"))?;
    let record = app.ui.hosted.remove(at);
    match record.kind {
        HostedKind::Param => {
            app.ui.host.cancel::<ParamDialog>();
            app.ui.host.take::<ParamDialog>();
        }
        HostedKind::Form => {
            app.ui.host.cancel::<CommandForm>();
            app.ui.host.take::<CommandForm>();
        }
    }
    run_form(app, record, params)
}

/// The form's answer, after `DialogHost::show`: OK runs the command, Cancel runs the cleanup.
pub fn collect(app: &mut PhotocraftApp) {
    let Some(result) = app.ui.host.take::<CommandForm>() else { return };
    let Some(record) = app.ui.hosted.iter().find(|h| h.kind == HostedKind::Form).cloned() else { return };
    match result {
        DialogResult::Ok(answer) if answer.key == record.key => {
            app.ui.hosted.retain(|h| h.id != record.id);
            if let Err(error) = run_form(app, record, fields_json(&answer.fields)) {
                app.ui.status = error;
            }
        }
        _ => {
            cancelled(app, record.id);
        }
    }
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
pub fn cancel(app: &mut PhotocraftApp, id: u64) -> Result<Value, String> {
    if app.ui.hosted_fields(id).is_none() {
        return Err(format!("no dialog {id}"));
    }
    match app.ui.hosted(id).map(|h| h.kind) {
        Some(HostedKind::Param) => app.ui.host.cancel::<ParamDialog>() && app.ui.host.take::<ParamDialog>().is_some(),
        Some(HostedKind::Form) => app.ui.host.cancel::<CommandForm>() && app.ui.host.take::<CommandForm>().is_some(),
        None => false,
    };
    cancelled(app, id);
    Ok(Value::Null)
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
}
