//! Reveal the active layer in the Layers panel (#152), like Photoshop: whenever the active
//! layer changes, by whatever means (a click on the canvas with Auto-Select, Select › layer
//! commands, ⌥[ / ⌥], undo, automation), its closed parent groups open and the panel scrolls
//! just enough to show its row, unless the row is already in view. The panel does both
//! (`LayersPanel::reveal`); this module decides when, and whether the groups open.
//!
//! Only a change of the active layer reveals: closing the group that holds it, or scrolling
//! away, is left alone until the next change.

use craft::layers::ui::panel::Groups;
use photocraft_doc::LayerId;

use crate::PhotocraftApp;

/// Last active layer the panel saw: (document index, layer).
#[derive(Clone, Copy, PartialEq)]
struct Seen(usize, Option<LayerId>);

fn seen_id() -> egui::Id {
    egui::Id::unique("layers-reveal-seen")
}

/// Called once per Layers panel frame, before its rows are listed. When the active layer
/// changed since the last frame, returns it with what its closed parent groups do: the panel's
/// reveal for this pass. The first frame of a document only records it.
pub fn track(app: &mut PhotocraftApp, ctx: &egui::Context) -> Option<(LayerId, Groups)> {
    let index = app.session.active_index()?;
    let active = app.session.active()?.active_layer;
    let now = Seen(index, active);
    // The marker says the latest active-layer change was a Move-way pick of that layer: it holds only
    // while no later change happened (the engine's change count is unchanged), however many frames a
    // hidden panel missed, and whether or not the final layer is the same id.
    let changes = app.session.active_layer_changes();
    let pick = app.move_pick.take().filter(|&(id, at)| Some(id) == active && at == changes).map(|(id, _)| id);
    let before = ctx.data(|d| d.get_temp::<Seen>(seen_id()));
    ctx.data_mut(|d| d.insert_temp(seen_id(), now));
    match before {
        Some(Seen(i, prev)) if i == index && prev != active => {}
        _ => return None,
    }
    let id = active?;
    // Expand layer group on click (Move gear menu, #11381) governs a canvas click that picked this
    // layer the Move tool way (Auto-Select, or a ⌘-click from another tool); every other change
    // opens the groups. The row (or its outermost closed group's) still scrolls into view.
    let move_pick = pick == Some(id);
    let groups = if !move_pick || app.ui.tool_options.move_expand_group_on_click { Groups::Open } else { Groups::Keep };
    Some((id, groups))
}

#[cfg(test)]
mod tests;
