//! Keyboard shortcut table and dispatch (#127): every shortcut the menus display fires.
//!
//! The menus show Photoshop's default shortcuts from [`crate::menu_catalog::CATALOG`], while
//! commands also declare their own (`CommandSpec::shortcut`, [`crate::menus::UI_COMMANDS`]).
//! The binding table merges all three, with Edit › Keyboard Shortcuts overrides on top, so a
//! shortcut printed next to a live menu item always reaches that item. A shortcut whose command
//! is disabled in the current state reports why on the status bar instead of doing nothing
//! silently, as the greyed menu item shows.

use egui::{Key, KeyboardShortcut};
use serde_json::json;

use crate::PhotocraftApp;
use craft::keymap::Active;

use crate::input::shortcut_keymap::ShortcutKeymap;
use crate::shortcuts::{key_matches, parse};

/// What a dispatched shortcut did (kept for tests and `ui.inspect`-style debugging).
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Ran,
    Disabled(String),
    Failed(String),
}

fn log_id() -> egui::Id {
    egui::Id::unique("pc-shortcut-log")
}

fn dry_run_id() -> egui::Id {
    egui::Id::unique("pc-shortcut-dry-run")
}

/// Tests: resolve and check shortcuts without running them (File › Exit, Open, browser links…).
pub fn set_dry_run(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(dry_run_id(), on));
}

/// The shortcuts dispatched since the last call (command id and outcome), oldest first.
pub fn take_log(ctx: &egui::Context) -> Vec<(String, Outcome)> {
    ctx.data_mut(|d| d.remove_temp::<Vec<(String, Outcome)>>(log_id())).unwrap_or_default()
}

/// Every key binding the dispatch runs: [`crate::shortcut_keymap::ShortcutKeymap::table`].
pub fn bindings(app: &PhotocraftApp) -> Vec<(String, KeyboardShortcut)> {
    app.keymap().table().iter().filter_map(|(id, text)| Some((id.clone(), parse(text)?))).collect()
}

pub use craft::keymap::ui::KeyOwner;
use craft::keymap::ui::{KeyGate, KeyRouter, NAVIGATION_COMMANDS};

/// What the app knows about its own modal state this frame: a foreground save or Camera Raw
/// blocks the window, and an app dialog or the unsaved-changes prompt is open. Only the navigation commands run under a dialog.
pub fn key_gate(app: &PhotocraftApp) -> KeyGate<'static> {
    KeyGate {
        blocked: app.camera_raw.is_some() || crate::jobs_ui::foreground_save(app).is_some(),
        dialog: app.ui.any_dialog_open() || app.discard.is_some(),
        recording: false,
        in_dialogs: &NAVIGATION_COMMANDS,
    }
}

/// Who has the keyboard now: [`KeyRouter::owner`] with the app's gate.
pub fn owner(app: &PhotocraftApp, ctx: &egui::Context) -> KeyOwner {
    KeyRouter::owner(ctx, &key_gate(app))
}

/// Whether a focused control claimed ⌫ / Delete (the Curves graph deletes its selected point), so
/// the shortcuts and the object keys leave them alone.
pub fn delete_claimed(ctx: &egui::Context) -> bool {
    let claimed = |key| craft::ui::behavior::KeyClaim::claimed(ctx, &KeyboardShortcut::new(egui::Modifiers::NONE, key));
    claimed(Key::Delete) || claimed(Key::Backspace)
}

/// [`KeyRouter::allows`] for the shortcut `sc` bound to command `id`, for tests that ask what an
/// owner leaves to the shortcuts.
#[cfg(test)]
pub(crate) fn allows(app: &PhotocraftApp, ctx: &egui::Context, owner: KeyOwner, id: &str, sc: &KeyboardShortcut) -> bool {
    use craft::keymap::{Chord, CommandRef, Key, Mods, egui::platform};
    let Some(key) = Key::from_egui(sc.logical_key) else { return false };
    let chord = Chord { mods: Mods::from_egui(&sc.modifiers, platform(ctx)), key };
    KeyRouter::allows(ctx, owner, &key_gate(app), &chord, &CommandRef::new(id))
}

/// The shortcut presses the dispatch hands to [`KeyRouter::route`] while inline type is being
/// edited: the clipboard and select-all commands belong to the text.
const TEXT_OWNED: [&str; 6] = ["edit.copy", "edit.cut", "edit.paste", "edit.copyMerged", "select.all", "edit.pasteSpecial.pasteInPlace"];

/// Dispatch this frame's shortcut presses in the order they arrived (⌘Z then ⌘S undoes, then
/// saves the undone state; #440): [`KeyRouter::route`] over [`ShortcutKeymap::dispatch`], which
/// removes each press it runs and stops after a shortcut that hands the keyboard to someone else
/// (opens a dialog, a prompt, a menu or inline type), leaving the later presses to it. While
/// typing (inline type), clipboard and select-all shortcuts belong to the text. Returns true when
/// a shortcut was dispatched.
pub fn dispatch_pressed(app: &mut PhotocraftApp, ctx: &egui::Context, editing: bool) -> bool {
    // Building the table walks the registry: only do it when a key went down.
    let pressed = |e: &egui::Event| matches!(e, egui::Event::Key { pressed: true, .. } | egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_));
    if !ctx.input(|i| i.events.iter().any(pressed)) {
        return false;
    }
    let keymap = app.keymap();
    let mut router = std::mem::take(&mut app.key_router);
    let ran = std::cell::Cell::new(false);
    let cell = std::cell::RefCell::new(&mut *app);
    let active = Active { typing: editing, ..Active::NONE };
    router.route(ctx, keymap.dispatch(), &active, || key_gate(&cell.borrow()), |hit| {
        let id = hit.command.id;
        if editing && TEXT_OWNED.contains(&id.as_str()) {
            return;
        }
        let mut app = cell.borrow_mut();
        if hit.trigger.chord.key == craft::keymap::Key::Tab {
            // egui already read this Tab as "focus the next widget" (Photoshop's Tab hides the
            // panels instead, #1313): the shortcut took it, so cancel that move.
            ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
        }
        let id = clip_key_command(&app, delete_key_command(&app, id));
        dispatch_as(&mut app, ctx, &id, false);
        ran.set(true);
    });
    let mut ran = ran.get();
    ran |= dispatch_last_settings(&mut cell.into_inner(), ctx, &keymap);
    app.key_router = router;
    ran
}

/// Photoshop: ⌥ added to an adjustment's own key opens it at its last settings (⌥⌘L, ⌥⌘M,
/// #8900), unless that key is bound to something else. The keymap has no binding for the ⌥ chord,
/// so its press is still in egui's input after the routing.
fn dispatch_last_settings(app: &mut PhotocraftApp, ctx: &egui::Context, keymap: &ShortcutKeymap) -> bool {
    use craft::keymap::{Chord, CommandRef, Mods, egui::platform};
    let events = ctx.input(|i| i.events.clone());
    for (at, event) in events.iter().enumerate() {
        let egui::Event::Key { key, pressed: true, modifiers, .. } = event else { continue };
        if !modifiers.alt {
            continue;
        }
        let plain = egui::Modifiers { alt: false, ..*modifiers };
        let Some(found) = keymap
            .table()
            .iter()
            .find(|(id, text)| crate::adjust_dialog::has_dialog(id) && parse(text).is_some_and(|sc| key_matches(&sc, *key, plain)))
        else {
            continue;
        };
        let Some(ckey) = craft::keymap::Key::from_egui(*key) else { continue };
        let chord = Chord { mods: Mods::from_egui(modifiers, platform(ctx)), key: ckey };
        let gate = key_gate(app);
        if !KeyRouter::allows(ctx, KeyRouter::owner(ctx, &gate), &gate, &chord, &CommandRef::new(&found.0)) {
            continue;
        }
        ctx.input_mut(|i| i.events.remove(at));
        dispatch_as(app, ctx, &found.0, true);
        return true;
    }
    false
}

/// Clear's key (Delete / Backspace) with nothing selected deletes the selected layers, of any
/// kind, as in Photoshop (#1077): Edit › Clear only clears pixel layers, so on an adjustment, fill,
/// type or shape layer the key did nothing. A selection, a single channel or Quick Mask keeps Clear.
/// With the active layer's mask targeted the key edits the mask, never the layer's pixels: a
/// selection is filled with the background colour in the mask (Photoshop's Delete on a mask), and
/// with no selection the mask itself is deleted, as Layer › Layer Mask › Delete, unless several
/// layers are selected: then they are deleted.
pub(crate) fn delete_key_command(app: &PhotocraftApp, id: String) -> String {
    if id != "edit.clear" {
        return id;
    }
    let Some(st) = app.session.active() else { return id };
    // A targeted filter mask (FILE-215-3) takes a selection's Delete as a layer mask does: the
    // selection fills with the background colour there. With no selection Photoshop deletes the
    // layer, as below.
    if app.filter_mask_targeted() && st.doc.selection.is_some() {
        return "edit.fillBackground".into();
    }
    let composite = st.channel_view.target == photocraft_engine::channel_cmds::ChannelTarget::Composite && st.doc.quick_mask.is_none();
    let has_mask = st.active_layer.and_then(|l| st.doc.layer(l)).is_some_and(|l| l.mask.is_some());
    let colors_off = st.channel_view.visible_colors(photocraft_engine::channel_cmds::color_count(&st.doc)) == 0;
    let alpha_on = (0..st.doc.channels.len()).any(|i| st.channel_view.alpha_shown(i));
    let solo_mask = colors_off && has_mask && !alpha_on && st.doc.quick_mask.is_none();
    let layer_mask = st.channel_view.target == photocraft_engine::channel_cmds::ChannelTarget::LayerMask;
    let mask = (app.ui.mask_target || solo_mask || layer_mask) && has_mask;
    // Several selected layers outrank the active one's targeted mask (#4025): a new adjustment
    // layer targets its mask, so the first press used to delete only that mask.
    let several = st.selected_layers().len() > 1;
    if mask && (composite || layer_mask) && !(several && composite && st.doc.selection.is_none()) {
        let cmd = if st.doc.selection.is_some() { "edit.fillBackground" } else { "layer.layerMask.delete" };
        return cmd.into();
    }
    if st.doc.selection.is_some() || !composite {
        return id;
    }
    "layer.delete".into()
}

/// Create Clipping Mask's key (⌥⌘G) releases when there is nothing left to clip, as in Photoshop,
/// whose Layer menu has one item that reads Create or Release Clipping Mask with the same
/// shortcut: pressing it again takes the clip off. That is when the layers Create Clipping Mask
/// would clip are all clipped already: the selected layer, or with several selected, every one
/// above the lowest.
pub(crate) fn clip_key_releases(app: &PhotocraftApp) -> bool {
    let Some(st) = app.session.active() else { return false };
    let selected = st.selected_layers();
    let clips = match selected.as_slice() {
        [] => return false,
        [one] => std::slice::from_ref(one),
        [_lowest, above @ ..] => above,
    };
    clips.iter().all(|id| st.doc.layer(*id).is_some_and(|l| l.clipped))
}

fn clip_key_command(app: &PhotocraftApp, id: String) -> String {
    if id == "layer.createClippingMask" && clip_key_releases(app) { "layer.releaseClippingMask".into() } else { id }
}

/// Why `id` can't run now (the engine's reason when it has one).
pub fn disabled_reason(app: &PhotocraftApp, id: &str) -> String {
    photocraft_engine::commands::find(id).and_then(|_| app.session.disabled_reason_with(id, &app.with_mask_target(id, serde_json::Value::Null))).unwrap_or_else(
        || match id {
            "tools.decreaseBrushHardness" | "tools.increaseBrushHardness" => "the current tool has no brush tip".into(),
            _ if app.session.active().is_none() => "no document open".into(),
            _ => "not available in the current state".into(),
        },
    )
}

fn label(id: &str) -> String {
    crate::menu_catalog::CATALOG
        .iter()
        .find(|c| c.3 == id)
        .map(|c| c.1)
        .or_else(|| crate::menus::UI_COMMANDS.iter().find(|c| c.0 == id).map(|c| c.1))
        .or_else(|| photocraft_engine::commands::find(id).map(|c| c.label))
        .or_else(|| crate::tool_shortcuts::tool(id).map(|tool| tool.label()))
        .unwrap_or(id)
        .trim_end_matches('…')
        .to_string()
}

/// Run a shortcut's command like its menu item (dialogs included), or say why it can't run.
pub fn dispatch(app: &mut PhotocraftApp, ctx: &egui::Context, id: &str) {
    dispatch_as(app, ctx, id, false);
}

/// [`dispatch`]; with `last`, an adjustment dialog opens at its last confirmed settings.
fn dispatch_as(app: &mut PhotocraftApp, ctx: &egui::Context, id: &str, last: bool) {
    let action = id.strip_prefix(crate::actions::SHORTCUT_PREFIX);
    let command_id = if action.is_some() { "actions.play" } else { id };
    let outcome = if !crate::menus::is_enabled(app, command_id) {
        let why = disabled_reason(app, command_id);
        let template = "{label} is not available: {reason}";
        app.ui.status = crate::i18n::fmt(tl!(template), &[("label", crate::i18n::tr_id(crate::i18n::current(), id, &label(id))), ("reason", tl!(&why))]);
        app.ui.status_error = true;
        Outcome::Disabled(why)
    } else if ctx.data(|d| d.get_temp::<bool>(dry_run_id())).unwrap_or(false) {
        Outcome::Ran
    } else {
        let r = if let Some(name) = action {
            // Use the normal command path: synthetic input and every nested action
            // step must still pass the automation authorizer.
            crate::menus::invoke(app, ctx, "actions.play", json!({"action": name})).and_then(|v| {
                crate::actions::report_play(app, &v);
                if v.get("failed").is_some_and(|f| f.is_object()) { Err(app.ui.status.clone()) } else { Ok(v) }
            })
        } else if crate::adjust_dialog::has_dialog(id) {
            if last {
                crate::adjust_dialog::open_last(app, id);
            } else {
                crate::adjust_dialog::open(app, id);
            }
            Ok(serde_json::Value::Null)
        } else {
            crate::menus::invoke(app, ctx, id, json!({}))
        };
        match r {
            Ok(_) => Outcome::Ran,
            Err(e) => {
                app.ui.status = e.clone();
                app.ui.status_error = true;
                Outcome::Failed(e)
            }
        }
    };
    ctx.data_mut(|d| {
        let log = d.get_temp_mut_or_default::<Vec<(String, Outcome)>>(log_id());
        if log.len() >= 64 {
            log.remove(0);
        }
        log.push((id.to_string(), outcome));
    });
}

#[cfg(test)]
mod tests;
