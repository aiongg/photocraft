//! Key tables of the full-window dialogs (Content-Aware Fill, Select and Mask, Camera Raw's
//! scopes, Vanishing Point) as `craft_keymap` catalogs: one scope per dialog, run by
//! [`craft_keymap::egui::Dispatcher`]. Plain keys wait while a text field has the keyboard.
//! The dialogs' keys are fixed (not in Edit › Keyboard Shortcuts), so a table is built once.

use craft_keymap::egui::Dispatcher;
use craft_keymap::{Active, Bindable, Binding, Keymap, Platform, Scope, Scopes};

/// One dialog's keys: `(command, chord)` rows in the dialog's own scope.
pub struct DialogKeys {
    scope: &'static str,
    keymap: Keymap,
    repeat: bool,
}

impl DialogKeys {
    /// `rows` are `(command id, chord text)`. A row with an unreadable chord, or a scope name the
    /// library refuses, is left out.
    pub fn new(scope: &'static str, rows: &[(&str, String)]) -> DialogKeys {
        let home = Scope::new(scope).unwrap_or_else(Scope::app);
        let catalog = rows
            .iter()
            .map(|(id, chord)| {
                let bindable = Bindable::new(*id, *id, home.clone());
                match Binding::parse(chord) {
                    Ok(binding) => bindable.with_default(binding),
                    Err(_) => bindable,
                }
            })
            .collect();
        DialogKeys { scope, keymap: Keymap::new(catalog, Scopes::new().with_panel(scope), Platform::current()), repeat: true }
    }

    /// Whether a held key runs its command again on each repeat (on by default).
    pub fn with_repeat(mut self, on: bool) -> DialogKeys {
        self.repeat = on;
        self
    }

    /// The commands whose keys were pressed this frame, in press order. Each press is consumed.
    pub fn pressed(&self, ctx: &egui::Context) -> Vec<String> {
        let active = Active { panel: Some(self.scope), contexts: &[], typing: ctx.text_edit_focused() };
        ctx.input_mut(|i| Dispatcher::new().with_repeat(self.repeat).dispatch(i, &self.keymap, &active)).into_iter().map(|hit| hit.command.id).collect()
    }
}

/// The canonical text of an egui key as a chord with no modifiers (`[`, `Backspace`, `W`).
pub fn key_text(key: egui::Key) -> String {
    craft_keymap::Key::from_egui(key).map(|k| k.to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Key, Modifiers};

    fn run(keys: &DialogKeys, text_field: bool, events: Vec<Event>) -> (Vec<String>, usize) {
        let ctx = egui::Context::default();
        let mut found = Vec::new();
        let mut left = 0;
        let mut out = ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| {
            if text_field {
                let id = egui::Id::new("field");
                ui.memory_mut(|m| m.request_focus(id));
                ui.add(egui::TextEdit::singleline(&mut String::new()).id(id));
            }
            found = keys.pressed(ui.ctx());
            left = ui.input(|i| i.events.iter().filter(|e| matches!(e, Event::Key { .. })).count());
        });
        out.textures_delta.clear();
        (found, left)
    }

    fn press(key: Key, modifiers: Modifiers) -> Event {
        Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    #[test]
    fn a_press_runs_its_command_and_plain_keys_wait_for_a_text_field() {
        let keys = DialogKeys::new("Test", &[("hide", "Cmd+H".into()), ("brush", "B".into()), ("smaller", "[".into())]);
        let (found, left) = run(&keys, false, vec![press(Key::B, Modifiers::NONE), press(Key::OpenBracket, Modifiers::NONE)]);
        assert_eq!((found, left), (vec!["brush".to_string(), "smaller".to_string()], 0), "presses run in order and are consumed");
        let (found, _) = run(&keys, false, vec![press(Key::H, Modifiers::COMMAND | Modifiers::SHIFT)]);
        assert!(found.is_empty(), "modifiers must match exactly");
        let (found, left) = run(&keys, true, vec![press(Key::B, Modifiers::NONE), press(Key::H, Modifiers::COMMAND)]);
        assert_eq!((found, left), (vec!["hide".to_string()], 1), "the text field keeps B, the command chord still runs");
    }
}
