//! The Layers panel's rows and their named parts in a kittest harness, for tests that drive the
//! real panel (craft-layers' `LayersPanel`): a row is named after its layer, its parts after the
//! panel's words (`Visibility {row}`, `Unlock {row}`, `Collapse effects {layer}`, …).

use egui::accesskit::Role;
use egui::{Event, Modifiers, PointerButton, Pos2, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

/// The rect of the row of the layer, effect or Smart Filter named `name`.
pub(crate) fn row<S>(h: &Harness<'_, S>, name: &str) -> Rect {
    h.query_all_by_label(name)
        .find(|n| matches!(n.accesskit_node().role(), Role::TreeItem | Role::ListItem | Role::ListBoxOption))
        .unwrap_or_else(|| panic!("no Layers panel row named {name:?}"))
        .rect()
}

/// A drag from `from` to `to` with `mods` held: press, a move past the drag threshold, the move
/// to `to`, release there.
pub(crate) fn drag<S>(h: &mut Harness<'_, S>, from: Pos2, to: Pos2, mods: Modifiers) {
    h.event(Event::ModifiersChanged(mods));
    h.event(Event::PointerMoved(from));
    h.step();
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: mods });
    h.step();
    for at in [from + egui::vec2(0.0, 8.0), to, to] {
        h.event(Event::PointerMoved(at));
        h.step();
    }
    h.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: mods });
    h.step();
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(3);
}
