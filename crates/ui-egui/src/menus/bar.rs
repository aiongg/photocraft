use craft::menu::{Labeled, MenuBarUi};

use super::PhotocraftApp;
use super::items;
use super::menu_gate;

/// Mnemonic letters of the top-level menus (File F, Edit E, …). A title takes its letter only when
/// no earlier title took it.
pub(super) const TOP_PREFERRED: [(&str, char); 10] =
    [("File", 'F'), ("Edit", 'E'), ("Image", 'I'), ("Layer", 'L'), ("Type", 'Y'), ("Select", 'S'), ("Filter", 'T'), ("View", 'V'), ("Window", 'W'), ("Help", 'H')];

/// Draws the in-window menu bar over the one menu model. Picks are fired commands from
/// `FireSource::Menu`, run by the frame's command drain; the bar's keys wait while a dialog or the
/// shortcut recorder has the keyboard.
pub fn menu_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let (gate, _) = menu_gate(app);
    let ctx = ui.ctx().clone();
    let model = items::model(app, &ctx);
    let english = |labeled: Labeled<'_>| match labeled {
        Labeled::Command(command) => model.english_label(command),
        Labeled::Title(title) => model.english_title(title),
        _ => None,
    };
    MenuBarUi::new(&model.bar).english(&english).mnemonics(&TOP_PREFERRED).help_search(true).keyboard(!(gate.modal || gate.recording_shortcut)).show(ui);
}
