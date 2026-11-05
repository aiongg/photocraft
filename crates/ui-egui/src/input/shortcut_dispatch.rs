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

/// Every key binding, most modifiers first (so ⇧⌘Z wins over ⌘Z), each key owned by one command:
/// command and shell shortcuts, then the menu catalogue's for live items without their own,
/// then Edit › Keyboard Shortcuts assignments to any other menu item, then the commands'
/// alternate keys (#3665: F3 is Copy as well as ⌘C, see
/// [`photocraft_engine::prefs::ALTERNATE_SHORTCUTS`]), which a main key of another command wins. D and X
/// (`tools.defaultColors` / `tools.swapColors`) and the fill keys are commands like any other, so
/// their overrides apply. Held temporary tools (Space…) are not here: see [`crate::hold_keys`].
pub fn bindings(app: &PhotocraftApp) -> Vec<(String, KeyboardShortcut)> {
    let keymap = app.keymap();
    let km = &*keymap;
    // Mains in catalog order, then every command's alternates, the user's ahead of the defaults.
    // A key the user assigned in Edit › Keyboard Shortcuts belongs to that command, ahead of any
    // default that still names it (#1272): OK clears the old owner's row, but that row can be
    // another id for the same item, or a default the dialog doesn't list.
    let ids: Vec<&str> = km.ids().filter(|id| km.fires(id)).collect();
    let mains = ids.iter().filter_map(|id| Some((*id, km.main(id)?, km.main_overridden(id))));
    let alternates = ids.iter().flat_map(|id| km.alternates(id).into_iter().map(move |k| (*id, k, km.alternates_overridden(id))));
    let mut candidates: Vec<(&str, String, bool)> = mains.chain(alternates).collect();
    candidates.sort_by_key(|(_, _, user)| !user);
    let mut all: Vec<(String, KeyboardShortcut)> = Vec::new();
    for (id, sc, _) in candidates {
        let Some(sc) = parse(&sc) else { continue };
        if !all.iter().any(|(_, b)| *b == sc) {
            all.push((id.to_string(), sc));
        }
    }
    all.sort_by_key(|(_, sc)| std::cmp::Reverse(sc.modifiers.shift as u8 + sc.modifiers.alt as u8 + sc.modifiers.command as u8));
    all
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

/// Who has the keyboard now: [`KeyRouter::owner`] with the app's gate. A layer rename keeps its
/// keys even in the frame egui drops its focus (Esc, Tab), so it counts as a text field.
pub fn owner(app: &PhotocraftApp, ctx: &egui::Context) -> KeyOwner {
    match KeyRouter::owner(ctx, &key_gate(app)) {
        KeyOwner::App | KeyOwner::Widget | KeyOwner::Popover if crate::layer_row_ui::rename_active(ctx) => KeyOwner::Text,
        owner => owner,
    }
}

/// Whether ⌫ / Delete belong to the shortcuts: no text field has them and no focused control
/// claimed them (the Curves graph deletes its selected point).
pub fn delete_keys_free(app: &PhotocraftApp, ctx: &egui::Context) -> bool {
    let claimed = |key| craft::ui::behavior::KeyClaim::claimed(ctx, &KeyboardShortcut::new(egui::Modifiers::NONE, key));
    app.ui.text_edit.is_none() && owner(app, ctx) != KeyOwner::Text && !claimed(Key::Delete) && !claimed(Key::Backspace)
}

/// May `sc`, bound to command `id`, fire under `owner`: [`KeyRouter::allows`].
pub(crate) fn allows(app: &PhotocraftApp, ctx: &egui::Context, owner: KeyOwner, id: &str, sc: &KeyboardShortcut) -> bool {
    use craft::keymap::{Chord, CommandRef, egui::platform};
    let Some(key) = craft::keymap::Key::from_egui(sc.logical_key) else { return false };
    let mods = craft::keymap::Mods::from_egui(&sc.modifiers, platform(ctx));
    KeyRouter::allows(ctx, owner, &key_gate(app), &Chord { mods, key }, &CommandRef::new(id))
}

/// Keys whose shortcuts follow the key's position, as in Photoshop: ⇧⌘. is ⇧⌘> on a US layout,
/// but types `:` on Nordic and German ones, so its character would never match. Only ⌘ shortcuts
/// follow the position: a plain `,` or `;` binding (brush hardness on AZERTY, where `;` sits on the
/// US comma key) must match the character typed.
const POSITIONAL: [Key; 2] = [Key::Period, Key::Comma];

/// Dispatch this frame's shortcut presses in the order they arrived (⌘Z then ⌘S undoes, then
/// saves the undone state; #440), consuming each. A press matching several bindings goes to the
/// first in table order (⇧⌘Z before ⌘Z). Stops after a shortcut that hands the keyboard to
/// someone else (opens a dialog, a prompt, a menu or inline type), leaving the later presses to
/// it. While typing (inline type), clipboard and select-all shortcuts belong to the text.
/// Returns true when a shortcut was dispatched.
pub fn dispatch_pressed(app: &mut PhotocraftApp, ctx: &egui::Context, editing: bool) -> bool {
    // Building the table walks the registry: only do it when a key went down.
    if !ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))) {
        return false;
    }
    const TEXT_OWNED: [&str; 6] = ["edit.copy", "edit.cut", "edit.paste", "edit.copyMerged", "select.all", "edit.pasteSpecial.pasteInPlace"];
    let key_owner = owner(app, ctx);
    let table: Vec<(String, KeyboardShortcut)> =
        bindings(app).into_iter().filter(|(id, sc)| allows(app, ctx, key_owner, id, sc) && !(editing && TEXT_OWNED.contains(&id.as_str()))).collect();
    let command = |e: &egui::Event| match e {
        egui::Event::Key { key, physical_key, pressed: true, modifiers, .. } => {
            let by_position = physical_key
                .filter(|p| POSITIONAL.contains(p))
                .and_then(|p| table.iter().find(|(_, sc)| sc.modifiers.command && sc.logical_key == p && key_matches(sc, p, *modifiers)));
            let bound =
                by_position.or_else(|| table.iter().find(|(_, sc)| key_matches(sc, *key, *modifiers))).map(|(id, _)| (id.clone(), *key == Key::Tab, false));
            // Photoshop: ⌥ added to an adjustment's own key opens it at its last settings (⌥⌘L,
            // ⌥⌘M, #8900), unless that key is bound to something else.
            bound.or_else(|| {
                let plain = egui::Modifiers { alt: false, ..*modifiers };
                let last = modifiers.alt.then(|| table.iter().find(|(id, sc)| crate::adjust_dialog::has_dialog(id) && key_matches(sc, *key, plain))).flatten();
                last.map(|(id, _)| (id.clone(), false, true))
            })
        }
        _ => None,
    };
    let mut ran = false;
    loop {
        let next = ctx.input_mut(|i| {
            let (at, hit) = i.events.iter().enumerate().find_map(|(at, e)| Some((at, command(e)?)))?;
            i.events.remove(at);
            Some(hit)
        });
        let Some((id, tab, last)) = next else { break };
        if tab {
            // egui already read this Tab as "focus the next widget" (Photoshop's Tab hides the
            // panels instead, #1313): the shortcut took it, so cancel that move.
            ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
        }
        let id = clip_key_command(app, delete_key_command(app, id));
        let (gate, before) = (key_gate(app), owner(app, ctx));
        dispatch_as(app, ctx, &id, last);
        ran = true;
        if key_gate(app) != gate || owner(app, ctx) != before {
            break;
        }
    }
    ran
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
