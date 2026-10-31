//! Dialogs that the library's `DialogHost` shows beside the legacy `UiState::dialogs` list.
//!
//! The host and the legacy loop in [`super::show`] draw against one `DialogStack`, so modality and
//! top-ness are common. Each hosted dialog has a [`Hosted`] record under the u64 id automation
//! uses; the library host knows dialogs only by type, so the record is how `ui.dialog.*` and the
//! cancel cleanup find one.

use craft::ui::dialog::{DialogFields, DialogHost, Param};
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
        let mut fields = match serde_json::to_value(self.hosted_fields(id)?) {
            Ok(Value::Object(map)) => map,
            _ => Fields::new(),
        };
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
    use craft::ui::dialog::{CommandForm, ParamDialog};
    if app.ui.hosted_fields(id).is_none() {
        return Err(format!("no dialog {id}"));
    }
    match app.ui.hosted(id).map(|h| h.kind) {
        Some(HostedKind::Param) => app.ui.host.cancel::<ParamDialog>(),
        Some(HostedKind::Form) => app.ui.host.cancel::<CommandForm>(),
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
}
