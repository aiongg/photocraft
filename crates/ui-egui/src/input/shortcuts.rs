//! Keyboard shortcuts: the notation and lookup of a command's keys, the clipboard key fix, and the
//! per-pass entry that runs the commands the runtime's keys fired.

use craft::keymap::{Chord, Platform};
use egui::{Key, KeyboardShortcut, Modifiers};
use serde_json::json;

use crate::PhotocraftApp;
use crate::state::Tool;

/// Parse `Cmd+Shift+N` style strings. `Cmd` maps to ⌘ on macOS and Ctrl elsewhere.
pub fn parse(s: &str) -> Option<KeyboardShortcut> {
    Chord::parse(s).ok().map(|c| c.to_egui_shortcut())
}

/// Human-readable form for menus, tooltips and inline hints (`Cmd+Shift+N` → `⌘⇧N` on
/// macOS, `Ctrl+Shift+N` elsewhere). Works for lone keys too: `pretty("Alt")` is `⌥`/`Alt`,
/// `pretty("Enter")` is `↩`/`Enter`.
pub fn pretty(s: &str) -> String {
    match Chord::parse(s) {
        Ok(chord) => chord.display(Platform::current()),
        Err(_) => lone_key(s),
    }
}

/// A key text that is no chord (`Alt`, `Enter` as the options bar writes them), in the platform's symbols.
fn lone_key(s: &str) -> String {
    let mac = cfg!(target_os = "macos");
    s.split('+')
        .map(|p| match (p, mac) {
            ("Cmd", true) => "⌘".to_string(),
            ("Cmd", false) => "Ctrl".to_string(),
            ("Shift", true) => "⇧".to_string(),
            ("Alt", true) => "⌥".to_string(),
            ("Enter", true) => "↩".to_string(),
            ("Delete", true) => "⌫".to_string(),
            (other, _) => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(if mac { "" } else { "+" })
}

/// The command's effective shortcut in the platform's notation (`Ctrl+N`), or `None` when
/// the command has no binding (the user removed it in Edit › Keyboard Shortcuts).
pub fn shortcut_label(app: &PhotocraftApp, command: &str) -> Option<String> {
    effective_shortcut(app, command, default_shortcut(command).as_deref()).map(|s| pretty(&s))
}

/// The command's effective shortcut as a chord, for framework components that show or bind it.
pub fn chord(app: &PhotocraftApp, command: &str) -> Option<craft::keymap::Chord> {
    effective_shortcut(app, command, default_shortcut(command).as_deref()).and_then(|s| craft::keymap::Chord::parse(&s).ok())
}

/// `label` (English, shown in the UI language) followed by the command's effective shortcut
/// (`New document…     Ctrl+N`); bare `label` when the binding was removed.
pub fn command_label(app: &PhotocraftApp, label: &str, command: &str) -> String {
    let label = tl!(label);
    shortcut_label(app, command).map_or_else(|| label.to_string(), |sc| format!("{label}     {sc}"))
}

/// `label` (English, shown in the UI language) with the command's effective shortcut in
/// parentheses (`Rulers  (Ctrl+R)`); bare `label` when the binding was removed.
pub fn tip_label(app: &PhotocraftApp, label: &str, command: &str) -> String {
    let label = tl!(label);
    shortcut_label(app, command).map_or_else(|| label.to_string(), |sc| format!("{label}  ({sc})"))
}

/// The shortcut in effect for a command: the user's override, else `default`.
pub fn effective_shortcut(app: &PhotocraftApp, id: &str, default: Option<&str>) -> Option<String> {
    app.session.prefs().shortcut(id, default).map(str::to_string)
}

/// A command's further keys in effect besides its main one (#3665): the user's, else
/// Photoshop's (F3 for Copy, Shift+F7 for Inverse…). Menus show only the main key.
pub fn effective_alternates(app: &PhotocraftApp, id: &str) -> Vec<String> {
    let main = effective_shortcut(app, id, default_shortcut(id).as_deref());
    app.session.prefs().alternates(id, main.as_deref())
}

/// Default (unmodified) shortcut of a command id.
pub fn default_shortcut(id: &str) -> Option<String> {
    crate::menus::UI_COMMANDS
        .iter()
        .find(|c| c.0 == id)
        .and_then(|c| c.3)
        .or_else(|| photocraft_engine::commands::find(id).and_then(|c| c.shortcut))
        .or_else(|| crate::menu_catalog::CATALOG.iter().find(|c| c.3 == id).and_then(|c| c.2))
        .or_else(|| photocraft_engine::prefs::TEMPORARY_TOOLS.iter().find(|t| t.0 == id).map(|t| t.2))
        .map(str::to_string)
        .or_else(|| crate::tool_shortcuts::tool(id).and_then(crate::tool_shortcuts::default))
}

/// View navigation that Photoshop keeps live while a dialog is open (so the image can be judged
/// at any zoom): Zoom In / Out, Fit on Screen, 100%.
pub const NAV_COMMANDS: [&str; 4] = ["view.zoomIn", "view.zoomOut", "view.fitOnScreen", "view.actualPixels"];

/// The key press a clipboard event stands for outside text fields, with `mods` held: Copy, Cut
/// and Paste are ⌘C, ⌘X and ⌘V (with ⇧/⌥ as held). On Windows egui-winit also sends Cut for
/// Shift+Delete, which is Edit › Fill here (#530): a Cut with ⇧ and without Ctrl is that key.
fn clipboard_press(e: &egui::Event, mods: Modifiers, is_windows: bool) -> Option<(Key, Modifiers)> {
    let held = if mods.command { mods } else { Modifiers::COMMAND };
    match e {
        egui::Event::Copy => Some((Key::C, held)),
        egui::Event::Cut if is_windows && mods.shift && !mods.command => Some((Key::Delete, mods)),
        egui::Event::Cut => Some((Key::X, held)),
        egui::Event::Paste(_) => Some((Key::V, held)),
        // egui 0.37 emits this when the clipboard holds an image and no text. Treat it as the
        // paste shortcut; the command reads the OS clipboard once through the existing import
        // path, never a second time from this event.
        egui::Event::PasteImage(_) => Some((Key::V, held)),
        _ => None,
    }
}

/// Text is selected in a label (an error notice's message, say): dragged across, double- or
/// triple-clicked or shift-clicked, as egui selects it. ⌘C then copies that text, as in a text
/// field, rather than running Edit › Copy on the layer (#7807). A plain click on a label leaves an
/// empty selection behind, which must not take ⌘C from the document. Read once a frame, from the
/// frame before.
fn label_text_selected(ctx: &egui::Context) -> bool {
    let id = egui::Id::unique("pc-label-text-selected");
    let any = ctx.plugin_opt::<egui::text_selection::LabelSelectionState>().is_some_and(|p| p.lock().has_selection());
    let (selects, pressed) = ctx.input(|i| {
        let p = &i.pointer;
        let selects = (p.primary_down() && p.is_decidedly_dragging())
            || p.button_double_clicked(egui::PointerButton::Primary)
            || p.button_triple_clicked(egui::PointerButton::Primary)
            || (p.primary_pressed() && i.modifiers.shift);
        (selects, p.primary_pressed())
    });
    let selected = any && (selects || (!pressed && ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false)));
    ctx.data_mut(|d| d.insert_temp(id, selected));
    selected
}

/// ⌘C, as egui-winit's `Event::Copy` or as a key press (the web, the Mac menu's key equivalent),
/// without ⇧ (Copy Merged) or ⌥.
fn plain_copy(e: &egui::Event, mods: Modifiers) -> bool {
    match e {
        egui::Event::Copy => !mods.shift && !mods.alt,
        egui::Event::Key { key: Key::C, pressed: true, modifiers: m, .. } => m.command && !m.shift && !m.alt,
        _ => false,
    }
}

/// egui-winit turns ⌘C / ⌘X / ⌘V into `Event::Copy` / `Cut` / `Paste` and drops the key press, and
/// drops ⌘V entirely when the clipboard holds no text (an image). Outside text fields, give the
/// pixel commands their key presses back: Copy/Cut/Paste become ⌘C/⌘X/⌘V presses (with ⇧/⌥ as
/// held, so ⇧⌘C is Copy Merged), and a ⌘V release without a ⌘V press (a paste event, or the Mac
/// menu's key equivalent) becomes a ⌘V press.
///
/// The web build gets both: eframe forwards the ⌘V key-down, then the browser's `paste` event
/// (the key-down's default action) arrives as `Event::Paste` in the same frame. A clipboard event
/// right after its own ⌘ key press is that press, not a second one (#2638).
///
/// With an image on the clipboard the V release is all that reaches us, and ⌘ is often let go
/// first: a V release whose press never arrived, after ⌘ was held and before any other key, is
/// that swallowed ⌘V too (#6329). Releasing in either order then pastes.
///
/// Ctrl+Insert copies and Shift+Insert pastes: egui-winit maps them on Windows (a Shift+Insert
/// release without a paste event pastes the image, like ⌘V), and we map them here on Linux.
///
/// With text selected in a label ([`label_text_selected`]) ⌘C copies the text; ⌘X and ⌘V stay
/// the document's.
pub fn clipboard_keys(ctx: &egui::Context, typing: bool, raw: &mut egui::RawInput) {
    let text_selected = label_text_selected(ctx);
    let press = |key, modifiers| egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers };
    if cfg!(target_os = "linux") {
        raw.events.retain_mut(|e| {
            let egui::Event::Key { key: Key::Insert, pressed: true, modifiers: m, .. } = *e else { return true };
            match (m.ctrl, m.shift) {
                (true, false) => *e = egui::Event::Copy,
                // Text fields need the clipboard text: egui-winit reads it and sends a Paste event.
                (false, true) if typing => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                    return false;
                }
                (false, true) => *e = press(Key::V, Modifiers::COMMAND),
                _ => {}
            }
            true
        });
    }
    if typing {
        // A text field copies and pastes on egui's Copy / Cut / Paste events, which egui-winit
        // makes from the window's key and so never sends as key presses. A ⌘C / ⌘X / ⌘V press
        // here is the Mac menu's key equivalent (AppKit takes the key while Edit › Copy, Cut or
        // Paste is enabled), which a text field ignores: pasting text worked only while
        // PhotoCraft's own clipboard was empty. Give those presses back as the events; Paste asks
        // the window for the clipboard text, which arrives as a Paste event next frame.
        raw.events.retain_mut(|e| {
            let egui::Event::Key { key, pressed: true, modifiers: m, .. } = *e else { return true };
            if !m.command || m.alt || m.shift {
                return true;
            }
            match key {
                Key::C => *e = egui::Event::Copy,
                Key::X => *e = egui::Event::Cut,
                Key::V => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                    return false;
                }
                _ => {}
            }
            true
        });
        return;
    }
    let seen = egui::Id::unique("pc-paste-key-seen");
    // A V press without ⌘ that reached us (the Move tool's key): its release is not a paste.
    let plain_v = egui::Id::unique("pc-plain-v-down");
    // ⌘ was held since the last key press without it: a swallowed ⌘V can be under way.
    let command_lately = egui::Id::unique("pc-paste-command-lately");
    // The modifiers held at each event: last frame's, updated as the events report changes.
    let mut mods = ctx.input(|i| i.modifiers);
    if mods.command {
        ctx.data_mut(|d| d.insert_temp(command_lately, true));
    }
    let mut out = Vec::with_capacity(raw.events.len());
    // The last ⌘C/⌘X/⌘V press this frame whose clipboard event hasn't arrived (yet).
    let mut unpaired: Option<Key> = None;
    for e in raw.events.drain(..) {
        if let egui::Event::ModifiersChanged(m) | egui::Event::Key { modifiers: m, .. } = &e {
            mods = *m;
            if mods.command {
                ctx.data_mut(|d| d.insert_temp(command_lately, true));
            }
        }
        if text_selected && plain_copy(&e, mods) {
            out.push(egui::Event::Copy);
            continue;
        }
        if let Some((key, held)) = clipboard_press(&e, mods, cfg!(target_os = "windows")) {
            if key == Key::V {
                ctx.data_mut(|d| {
                    d.insert_temp(seen, true);
                    d.remove_temp::<bool>(plain_v);
                });
            }
            if unpaired.take_if(|k| *k == key).is_none() {
                out.push(press(key, held));
            }
            continue;
        }
        match &e {
            // A ⌘C/⌘X/⌘V press egui-winit didn't swallow (the web, the Mac menu's key equivalent,
            // Shift+Insert on Linux): its clipboard event and, for ⌘V, its release must not run
            // the command a second time (#2638, #1638).
            egui::Event::Key { key: key @ (Key::C | Key::X | Key::V), pressed: true, modifiers, .. } if modifiers.command => {
                unpaired = Some(*key);
                if *key == Key::V {
                    ctx.data_mut(|d| {
                        d.insert_temp(seen, true);
                        d.remove_temp::<bool>(plain_v);
                    });
                }
            }
            egui::Event::Key { key, pressed: true, modifiers, .. } if !modifiers.command => {
                ctx.data_mut(|d| {
                    d.remove_temp::<bool>(command_lately);
                    if *key == Key::V {
                        d.insert_temp(plain_v, true);
                    }
                });
            }
            // A paste key's release ends that press, with or without ⌘ still held (else a stale
            // mark would swallow the next image paste); without a press it is the paste itself.
            egui::Event::Key { key: key @ (Key::V | Key::Insert), pressed: false, modifiers, .. } => {
                let had_press = ctx.data_mut(|d| d.remove_temp::<bool>(seen)).unwrap_or(false);
                let had_plain_press = *key == Key::V && ctx.data_mut(|d| d.remove_temp::<bool>(plain_v)).unwrap_or(false);
                // Taken: one swallowed ⌘V is one paste.
                let lately = *key == Key::V && ctx.data_mut(|d| d.remove_temp::<bool>(command_lately)).unwrap_or(false);
                let held = match key {
                    Key::V if modifiers.command => Some(*modifiers),
                    // ⌘ let go before V: the press egui-winit swallowed was ⌘V (#6329). The web
                    // always forwards the key-down, so there a lone release is no paste.
                    Key::V if lately && !had_plain_press && !cfg!(target_arch = "wasm32") => Some(Modifiers::COMMAND),
                    Key::Insert if cfg!(target_os = "windows") && modifiers.shift && !modifiers.ctrl => Some(Modifiers::COMMAND),
                    _ => None,
                };
                if let Some(held) = held
                    && !had_press
                {
                    out.push(press(Key::V, held));
                }
            }
            _ => {}
        }
        out.push(e);
    }
    raw.events = out;
}

/// The frame's key call, once per pass: runs the commands the runtime's keys fired in the pass before
/// ([`crate::shortcut_dispatch::run_fired`]), then the held-key work of the app; Escape is [`escape_keys`]'s. The fired list is taken,
/// so a second pass in one frame runs only what its own input fired. The main window runs no shortcuts while the
/// screen color picker is open.
pub fn handle(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let fired = std::mem::take(&mut app.framework.fired);
    if crate::screen_picker::busy(app) {
        return;
    }
    crate::shortcut_dispatch::run_fired(app, ctx, &fired);
    // A held tool letter released: a temporary tool gives the previous one back (#2672).
    crate::spring_tools::tick(app, ctx);
    // However the tool was picked (toolbar, flyout, shortcut, automation), its group's key and its
    // toolbar slot bring it back later (#2608). A temporary tool is no choice, so it isn't noted.
    app.ui.remember_group_tool(crate::tool_memory::selected(app));
<<<<<<< HEAD
=======
    crate::canvas::polygon_cancel_stale(app);
    // Camera Raw is modal like Photoshop's filter dialog: no application shortcut (Save, Undo,
    // tools) runs beneath it, and it handles its own keys (Y, U, O, S). Unlike the other dialogs
    // below it covers the canvas, so canvas zoom keys are blocked too.
    if app.camera_raw.is_some() {
        return false;
    }
    // A foreground save (Save in Background off, #6207) blocks the app until the file is written;
    // only Esc reaches it, to cancel while it still encodes (see `jobs_ui::tick`).
    if crate::jobs_ui::foreground_save(app).is_some() {
        return false;
    }
    // An open menu owns the keyboard (arrows, ↩, Esc), like a native menu; so does the menu bar
    // once Alt or F10 put the keyboard on it (#4988), and an open dropdown list, whose letters
    // pick an option rather than a tool (#1481). A command shortcut (⌘D, F7…) takes the keyboard
    // off the bar first and runs, as with a native menu bar (#6573), and closes an open menu or
    // list (or a panel's options menu, any popup) and runs, as Photoshop's menus do (#9065).
    crate::menu_keys::leave_for_shortcut(ctx);
    if app.ui.dialogs.is_empty() && app.discard.is_none() && crate::type_tool::commit_shortcut(app, ctx) {
        return false;
    }
    crate::menu_keys::close_for_shortcut(ctx);
    if crate::menu_nav::is_open(ctx) || crate::menu_keys::is_focused(ctx) || crate::widgets::dropdown_list_open(ctx) {
        return false;
    }
    // A Keyboard Shortcuts field waiting for keys records the key pressed, whatever it is: no
    // shortcut runs first, not even the view keys that work under dialogs (#8965: ⌘0, ⌘+ zoomed
    // instead of being recorded).
    if crate::prefs_ui::capturing_shortcut(app) {
        return false;
    }
    // ⌘Z in a filter or adjustment dialog steps the dialog's settings, not the document (#4697).
    crate::edit_history::keys(app, ctx);
    // Stroke Options owns plain navigation/activation keys, including while its child
    // dropdown is open. This runs before Popup::show registers the current pass, so use
    // the editor's explicit state; otherwise Escape deselects or commits a pending Pen path.
    // Modified commands such as Undo still follow the ordinary dispatch below.
    if app.ui.stroke_editor.is_some() && !ctx.input(|i| i.modifiers.command || i.modifiers.ctrl) {
        return false;
    }
    // So does the right-click menu of text being edited: Esc closes only it, and keys typed while
    // it is open don't reach the text beneath. The crop menu's likewise: Esc and ↵ don't cancel or
    // commit the crop under it (#6561).
    if app.ui.canvas_tool_menu.as_ref().is_some_and(|m| m.text || m.crop.is_some()) {
        return false;
    }
    // The Space-Hand view menu (the menu of a view tool opened over another tool, #11271) takes a
    // plain Esc and ↵ too: they don't cancel or commit a pending crop under it. Esc closes the
    // menu here and both keys are withheld from the dispatch below; every other key, modified ↵
    // and tool keys included, dispatches as usual.
    if app.ui.canvas_tool_menu.as_ref().is_some_and(|m| m.under.is_some() && crate::canvas_tool_menu::is_view_tool(m.tool)) {
        let plain = |e: &egui::Event, k: egui::Key| matches!(e, egui::Event::Key { key, pressed: true, modifiers, .. } if *key == k && modifiers.is_none());
        let (esc, enter) = ctx.input(|i| (i.events.iter().any(|e| plain(e, egui::Key::Escape)), i.events.iter().any(|e| plain(e, egui::Key::Enter))));
        if esc {
            app.ui.canvas_tool_menu = None;
        }
        if esc || enter {
            ctx.input_mut(|i| i.events.retain(|e| !plain(e, egui::Key::Escape) && !plain(e, egui::Key::Enter)));
        }
    }
    let symmetry_editing = app.ui.symmetry_transform.is_some();
    crate::symmetry_ui::track(app, ctx);
    if symmetry_editing && (app.ui.symmetry_transform.is_none() || egui::Popup::is_any_open(ctx)) {
        return false;
    }
    // Before any key handler: per-layout symbol keys and the bracket fallback (#3102, #3109).
    if !ctx.text_edit_focused() {
        crate::layout_keys::apply(ctx);
    }
    // Liquify is a full-window custom dialog with focusable sliders. egui can therefore claim
    // keyboard input before the distortion-mode handler below runs. Give Liquify's local
    // shortcuts first refusal (Undo, brush size, tool keys), but never steal keys from text edits.
    crate::distort_ui::cancel_stale(app);
    if app.distort.liquify.is_some() {
        // Undo and redo still apply while a numeric field (Size, Pressure, …) has focus.
        // Other Liquify keys must not steal characters from that field (#4027).
        if ctx.text_edit_focused() {
            crate::liquify_ui::history_keys(app, ctx);
        } else {
            crate::liquify_ui::keys(app, ctx);
        }
    }
    // The Filter Gallery's preview takes the zoom keys from the canvas behind it.
    if app.distort.gallery.is_some() && !ctx.text_edit_focused() {
        crate::gallery_ui::keys(app, ctx);
    }
    // The Content-Aware Fill workspace likewise.
    if app.distort.caf.is_some() && !ctx.text_edit_focused() {
        crate::caf_ui::keys(app, ctx);
    }
    // And Select and Mask's tools, views and brush size.
    if app.distort.select_mask.is_some() && !ctx.text_edit_focused() {
        crate::select_mask_ui::keys(app, ctx);
    }
    true
>>>>>>> upstream/main
}

/// Esc and the view keys. The UI pass runs it last (`ui_body`), so the canvas's
/// sessions and tools, the dialogs and every other UI that takes Esc have had it first, and only what they left
/// reaches the window's intents. The main window takes none while the screen color picker is open, and a foreground
/// save (Save in Background off, #6207) blocks the app until the file is written: only Esc reaches it, to cancel
/// while it still encodes (see `jobs_ui::tick`).
pub fn escape_keys(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if crate::screen_picker::busy(app) || crate::jobs_ui::foreground_save(app).is_some() {
        return;
    }
    handle_escape_and_view_keys(app, ctx);
}

<<<<<<< HEAD
/// Esc (leave full screen, else deselect), the Delete Anchor Point Esc stages, and the `+` / `−` path operations.
/// Runs last, so every state that owns Esc has already had its chance.
=======
/// Pen path keys (#1466, #1769), the work-path delete key (#5702), and in-progress lasso state.
/// Returns true when one of them consumed the key.
fn handle_pen_and_work_path_keys(app: &mut PhotocraftApp, ctx: &egui::Context, focus: crate::shortcut_dispatch::Focus) -> bool {
    // A Pen path's points are uncommitted gesture state, not History entries (#1466).
    // Intercept Cmd/Ctrl+Z before the normal Edit › Undo shortcut, as well as unmodified
    // Backspace/Delete. Once the last anchor is removed, regular Undo works again.
    // A failed draft retained after changing tools does not own the new tool's keys.
    if app.ui.pen.as_ref().is_some_and(|pen| pen.owns_undo(app.ui.tool) && !pen.knots.is_empty()) {
        let mods = ctx.input(|i| i.modifiers);
        // Not while a Free Transform or Crop box owns Undo: Edit › Undo steps the box.
        let command_undo = !crate::vector_ui::box_steps(app, "edit.undo")
            && effective_shortcut(app, "edit.undo", default_shortcut("edit.undo").as_deref())
                .and_then(|shortcut| parse(&shortcut))
                .is_some_and(|shortcut| consume(ctx, &shortcut));
        let remove = !mods.command
            && !mods.ctrl
            && !mods.shift
            && !mods.alt
            && ctx.input_mut(|i| i.consume_key(mods, Key::Backspace) || i.consume_key(mods, Key::Delete));
        if command_undo || remove {
            crate::vector_ui::pen_undo_last_point(app);
            return true;
        }
    }
    // Pen path in progress: ↩ and Esc both end it as an open path and keep it, as in Photoshop (#1769);
    // ↩ also deselects the Work Path. When the document refuses it, ↩ keeps the draft to commit
    // again; Esc keeps it as the Work Path.
    if app.ui.pen.is_some() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
        crate::vector_ui::pen_enter(app);
        return true;
    }
    if app.ui.pen.is_some() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        crate::vector_ui::pen_end(app);
        return true;
    }
    // A shape box handle drag: Esc drops it, the shapes keep their size (#11107).
    if app.ui.shape_box.is_some() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        crate::shape_bounds_box::cancel(app);
        return true;
    }
    // An anchor selected with Direct Selection or the Add / Delete Anchor Point tool: ⌫/Delete
    // removes it and its two segments (#12703, #13203); a segment selected with Direct Selection
    // goes on its own (#13203). As in Photoshop, never the active layer.
    if focus != crate::shortcut_dispatch::Focus::DeletingWidget
        && let Some((target, cmd, mut params)) = crate::vector_ui::delete_key_deletes_anchor(app)
            .map(|(target, [s, k])| (target, "path.deleteAnchor", json!({"subpath": s, "knot": k, "segments": true})))
            .or_else(|| {
                crate::vector_ui::delete_key_deletes_segment(app).map(|(target, [s, k])| (target, "path.deleteSegment", json!({"subpath": s, "knot": k})))
            })
    {
        let mods = ctx.input(|i| i.modifiers);
        if !mods.command && !mods.ctrl && !mods.shift && !mods.alt && ctx.input_mut(|i| i.consume_key(mods, Key::Backspace) || i.consume_key(mods, Key::Delete))
        {
            if let (Some(p), Some(t)) = (params.as_object_mut(), target.params(app).as_object()) {
                p.extend(t.clone());
            }
            match app.run(cmd, params) {
                // The anchors shift: the selection would point at the wrong ones.
                Ok(_) => app.ui.direct_selection = crate::direct_select::DirectSelection::default(),
                Err(e) => {
                    app.ui.status = e;
                    app.ui.status_error = true;
                }
            }
            return true;
        }
    }
    // A finished work path shown by the Pen or Path Selection (#5702), or the path Direct
    // Selection targets with nothing selected on it (#13203): ⌫/Delete deletes it, as in
    // Photoshop, never Edit › Clear on the pixels under it or the active layer.
    if focus != crate::shortcut_dispatch::Focus::DeletingWidget
        && let Some((cmd, params)) = crate::vector_ui::delete_key_deletes_path(app)
    {
        let mods = ctx.input(|i| i.modifiers);
        if !mods.command && !mods.ctrl && !mods.shift && !mods.alt && ctx.input_mut(|i| i.consume_key(mods, Key::Backspace) || i.consume_key(mods, Key::Delete))
        {
            if let Err(e) = app.run(cmd, params) {
                app.ui.status = e;
                app.ui.status_error = true;
            }
            return true;
        }
    }
    false
}

/// Magnetic Lasso, lasso vertices, guides, slices and smart-filter rows: each owns ⌫/Delete (and ↩/Esc)
/// in its own context, so none of them reaches Edit › Clear. Returns true when one consumed the key.
fn handle_object_tool_keys(app: &mut PhotocraftApp, ctx: &egui::Context) -> bool {
    // Magnetic Lasso: ↩ closes, Esc cancels, ⌫ removes a fastening point (not Edit › Clear),
    // [ ] change the detection width (not the brush size).
    if crate::magnetic_lasso_ui::keys(app, ctx) || handle_lasso_vertex_keys(app, ctx) {
        return true;
    }
    // A selected guide: ⌫/Delete removes it (`guide_select`), never Edit › Clear's pixels.
    if crate::guide_select::keys(app, ctx) {
        return true;
    }
    // A slice selected with Slice Select: ⌫/Delete deletes the slice, not the layer (#7862).
    if crate::slice_ui::keys(app, ctx) {
        return true;
    }
    // A note selected with the Note tool: ⌫/Delete asks to delete the note, not the layer (#11788).
    if crate::analysis_ui::note_keys(app, ctx) {
        return true;
    }
    // A selected smart filter row: ⌫/Delete removes the filter, not its smart object (#4751).
    crate::smart_ui::keys(app, ctx)
}

/// Registry shortcuts, inline type, and the crop-tool and single-key tool shortcuts. Returns true
/// when one consumed the key.
fn handle_registry_and_tool_keys(app: &mut PhotocraftApp, ctx: &egui::Context, focus: crate::shortcut_dispatch::Focus) -> bool {
    // Space+F (⇧Space+F backwards) cycles the pasteboard colour, as in Photoshop: F while the
    // Hand key is held, before F's own Cycle Screen Mode.
    if !matches!(focus, crate::shortcut_dispatch::Focus::Text) && app.ui.text_edit.is_none() && crate::hold_keys::reposition_held(app, ctx) {
        let back = ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::F));
        if back || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F)) {
            if let Err(e) = crate::canvas_tool_menu::cycle_canvas_color(app, back) {
                app.ui.status = e;
            }
            return true;
        }
    }
    // Inline type editing eats text and navigation keys; ⌘-shortcuts still reach the menus.
    let editing = crate::type_tool::handle_keys(app, ctx);
    // ⌥⇧⌘N: a new layer at once, without the New Layer dialog ⇧⌘N opens (#4794).
    if new_layer_without_dialog(app, ctx) {
        return true;
    }
    if app.ui.type_path_drag.is_some() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        crate::type_path_placement::cancel(app);
        return true;
    }
    // Registry, UI and menu-catalogue shortcuts (see [`crate::shortcut_dispatch::bindings`]).
    if crate::shortcut_dispatch::dispatch_pressed(app, ctx, focus, editing) {
        return true;
    }
    if editing {
        return true;
    }
    // Crop tool showing a box: O cycles its overlay, ⇧O the overlay's orientation (#1919).
    // ... and H toggles Show Cropped Area, and I fills W x H x Resolution from the document (#2443).
    if crate::crop_overlay::keys(app, ctx) || crate::crop_shield::keys(app, ctx) || crate::crop_size::keys(app, ctx) {
        return true;
    }
    // Single-key tools (no modifiers). D and X (`tools.defaultColors` / `tools.swapColors`) and the
    // brush keys [ ] ⇧[ ⇧] (`tools.decreaseBrushSize`…) are commands, dispatched above with any
    // Keyboard Shortcuts override.
    let pressed = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
    // Remove tool with strokes waiting (Remove after each stroke off): ↩ applies them, Esc drops them.
    if !app.remove_pending.is_empty() {
        if pressed(Key::Enter) {
            crate::retouch_ui::remove_pending_commit(app);
            return true;
        }
        if pressed(Key::Escape) {
            crate::retouch_ui::remove_pending_cancel(app);
            return true;
        }
    }
    if app.ui.frame_tool.drag.is_some() && pressed(Key::Escape) {
        crate::frame_tool::escape(app);
        return true;
    }
    // Enter / Escape commit or cancel in-progress tool state (polygonal lasso, crop).
    if crate::lasso_ui::active(app) {
        let mods = ctx.input(|i| i.modifiers);
        if ctx.input_mut(|i| i.consume_key(mods, Key::Escape)) {
            app.drag = None;
            return true;
        }
        if ctx.input_mut(|i| i.consume_key(mods, Key::Enter)) {
            crate::lasso_ui::commit(app);
            return true;
        }
    }
    // An open options bar popup (the Crop Ratio list, the overlay and gear menus) takes the keys
    // first: Esc closes it, and neither key reaches the crop beneath (#7386). Its id is kept in
    // memory from the last frame; `ctx.any_popup_open` only sees this frame's, drawn later.
    if (!app.ui.polygon.is_empty() || app.ui.crop_rect.is_some()) && !egui::Popup::is_any_open(ctx) && !crate::panels::options_bar::fill_help_takes_keys(ctx) {
        if pressed(Key::Enter) {
            if !app.ui.polygon.is_empty() {
                crate::canvas::commit_polygon(app);
            } else {
                crate::canvas::commit_crop(app);
            }
            return true;
        }
        // In a full screen mode, the Crop tool's untouched default frame (Straighten off) has nothing to cancel: cancelling only
        // makes the next one, so Esc goes on to leave full screen (#7315). A gesture on it is still cancelled (#9275).
        let untouched = app.ui.view.screen_mode != "standard"
            && app.ui.polygon.is_empty()
            && app.crop.default_frame
            && app.crop.drag.is_none()
            && !crate::crop_straighten::mode(app);
        if !untouched && pressed(Key::Escape) {
            app.ui.polygon.clear();
            app.ui.polygon_mode.clear();
            app.ui.polygon_doc = None;
            crate::crop_ui::dismiss(app);
            return true;
        }
    }
    false
}

/// Esc (leave full screen, else deselect), the tool-letter keys, Page/Home/End scrolling and the
/// opacity keys. Runs last, so every state that owns Esc has already had its chance.
>>>>>>> upstream/main
fn handle_escape_and_view_keys(app: &mut PhotocraftApp, ctx: &egui::Context) {
    // A Properties Levels/Curves eyedropper: Esc disarms it.
    if !app.dialog_open() && crate::layer_picker::armed(app).is_some() && crate::window_keys::take_escape(ctx) {
        crate::layer_picker::cancel(app);
        return;
    }
    // Esc with no active operation leaves a full screen mode, else deselects (#2674), like ⌘D.
    // The key is never taken from a state that handles Esc itself: a gesture on the canvas (a
    // marquee being drawn, a Direct Selection drag on an anchor, whose release is still to come,
    // #13072), inline type, the palette, the canvas menu, the brush picker, Rotate View, any dialog
    // and any open popup (flyouts and menus included).
    let esc_taken = crate::canvas::tools::in_gesture(app)
        || crate::direct_select::editing(app)
        || app.ui.text_edit.is_some()
        || app.ui.palette.open
        || app.ui.canvas_tool_menu.is_some()
        || app.ui.brush_picker.is_some()
        || app.ui.tool == Tool::RotateView
<<<<<<< HEAD
        || app.dialog_open()
        || ctx.any_popup_open();
=======
        || app.wide_angle.is_some()
        || app.save_options_open()
        || app.smart_svg.modal_open()
        || app.distort.gallery.is_some()
        || ctx.any_popup_open()
        || ctx.data(|d| d.get_temp::<(egui::Id, egui::Rect)>(egui::Id::unique("tool-flyout")).is_some())
        || crate::panels::toolbar::more_flyout_open(ctx);
    // Active operations and popups have already had priority. Reset a rotated view before
    // idle Escape actions such as leaving full screen or deselecting; Rotate View handles its
    // own Escape later, where it also clears an in-progress rotation drag.
    if !esc_taken
        && let Some(view) = app.session.active_index().and_then(|i| app.ui.views.get_mut(i))
        && view.rotation != 0.0
        && pressed(Key::Escape)
    {
        view.rotation = 0.0;
        return;
    }
>>>>>>> upstream/main
    // Both full screen modes (with and without the menu bar) return to Standard, as in Photoshop
    // (#4116), through the menu command so the window and the Screen Mode check follow.
    if !esc_taken && app.ui.view.screen_mode != craft::ui::shell::ScreenMode::Standard && crate::window_keys::take_escape(ctx) {
        if let Err(e) = crate::menus::invoke(app, ctx, "view.screenMode.standard", json!({})) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
        return;
    }
    if !esc_taken && app.session.active().is_some_and(|st| st.doc.selection.is_some()) && crate::window_keys::take_escape(ctx) {
        if let Err(e) = app.run("select.deselect", json!({})) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
        return;
    }
    // Delete Anchor Point (#12773): Esc first hides the anchors of the saved path targeted in the
    // Paths panel, then deselects that path, so clicks edit nothing. The first Esc leaves the path
    // targeted whether or not an anchor was selected (#13072), and clears only anchors selected on
    // that path, never another path's.
    if !esc_taken
        && app.ui.tool == Tool::DeleteAnchorPoint
        && let Some((index, _)) = crate::vector_ui::selected_saved_path_index(app)
    {
        let ds = &app.ui.direct_selection;
        let on_this_path = ds.target == Some(crate::direct_select::PathRef::Saved(index));
        let selected = on_this_path && (!ds.anchors.is_empty() || ds.segment.is_some());
        if crate::window_keys::take_escape(ctx) {
            // The stage belongs to this path: choosing another one starts at the first Esc again.
            if selected || app.ui.direct_selection.escape_hidden != Some(index) {
                if selected {
                    app.ui.direct_selection.anchors.clear();
                    app.ui.direct_selection.segment = None;
                }
                app.ui.direct_selection.escape_hidden = Some(index);
            } else {
                app.ui.selected_path = None;
                app.ui.direct_selection.escape_hidden = None;
            }
            return;
        }
    }
<<<<<<< HEAD
    // + / − pick a Path Operation with the Pen tools and Path Selection (`path_components`).
    crate::path_components::handle_keys(app, ctx);
=======
    // Path Selection (#13287): with components of the saved path targeted in the Paths panel
    // selected, Esc deselects the components and keeps the path targeted and drawn, while a plain
    // Enter deselects the components and also untargets the path (its Paths panel row and outline
    // go). Neither adds a History step. Free Transform Path and ⌘↩ (Make Selection) took their
    // keys earlier; with no component selected both keys keep today's behaviour. Not while the
    // path's name is typed in the panel. Only the bare keys: `consume_key` would also take them with
    // ⇧ or ⌥, which were not measured.
    let bare = |k: Key| {
        ctx.input_mut(|i| {
            let n = i.events.len();
            i.events.retain(|e| !matches!(e, egui::Event::Key { key, pressed: true, modifiers, .. } if *key == k && modifiers.is_none()));
            i.events.len() != n
        })
    };
    if !esc_taken
        && app.ui.tool == Tool::PathSelection
        && !crate::vector_ui::path_renaming(app, ctx)
        && let Some((index, _)) = crate::vector_ui::selected_saved_path_index(app)
        && let Some((crate::direct_select::PathRef::Saved(i), ..)) = crate::path_components::selected(app)
        && i == index
    {
        if bare(Key::Escape) {
            crate::path_components::deselect_components(app);
            return;
        }
        if bare(Key::Enter) {
            crate::path_components::deselect_components(app);
            if let Some(name) = app.ui.selected_path.clone() {
                crate::vector_ui::deselect_loaded_path(app, &name);
            }
            return;
        }
    }
    // + / − pick a Path Operation with the Pen tools and Path Selection (`path_components`)…
    crate::path_components::handle_keys(app, ctx);
    // … and otherwise ⇧+ / ⇧− step the blend mode and ⇧⌥ letters pick one (`blend_keys`). Before the
    // tool letters: `consume_key` ignores extra ⇧ and ⌥, so ⇧⌥M would otherwise pick a tool.
    crate::blend_keys::handle(app, ctx);
    // Tool keys; pressing the key of the current group cycles within it. With Preferences ›
    // Tools › Use Shift Key for Tool Switch, only ⇧+key cycles and the plain key keeps the
    // group's current tool. From another group, the key picks the group's last-used tool.
    // A held tool letter is a spring-loaded tool (#2672), not a burst of presses: its key
    // repeats are dropped rather than cycling the group.
    ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Key { key, pressed: true, repeat: true, .. } if crate::spring_tools::is_tool_key(*key))));
    if select_tool_by_key(app, ctx, &pressed) {
        return;
    }
    // Page Up / Page Down (⌘ sideways, ⇧ a small step), Home and End scroll the canvas (#8057).
    if crate::page_keys::handle(app, ctx) {
        return;
    }
    // 1–0 set opacity, ⇧1–0 flow or fill (`opacity_keys`).
    crate::opacity_keys::handle(app, ctx);
}

/// Selects a tool from its letter key. Entering a group uses its remembered eligible tool or the
/// first eligible tool in toolbar order. Pressing the key of the current group cycles within it.
/// With the Shift-switch preference, only ⇧+key cycles. Returns true when a tool key was handled.
fn select_tool_by_key(app: &mut PhotocraftApp, ctx: &egui::Context, pressed: &impl Fn(Key) -> bool) -> bool {
    let shift_switch = app.session.prefs().tools.use_shift_key_for_tool_switch;
    for t in Tool::ALL {
        if !crate::tool_shortcuts::uses_default(app, t) {
            continue;
        }
        let Some(k) = crate::spring_tools::key_of(t.key()) else { continue };
        let cycle_shift = shift_switch && ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, k));
        if cycle_shift || pressed(k) {
            let before = app.ui.tool;
            let mut group: Vec<Tool> = Tool::ALL.iter().copied().filter(|x| x.key() == t.key() && crate::tool_shortcuts::uses_default(app, *x)).collect();
            // Shift+key cycles in the toolbar layout's order, across slots (Edit › Toolbar).
            if cycle_shift {
                let order = crate::panels::toolbar::layout_order(&app.session.prefs().toolbar);
                group.sort_by_key(|x| order.iter().position(|t| t == x).unwrap_or(usize::MAX));
            }
            let next_tool = match group.iter().position(|x| *x == app.ui.tool) {
                Some(i) if shift_switch && !cycle_shift => group[i],
                Some(i) => group[(i + 1) % group.len()],
                None => app
                    .ui
                    .group_tools
                    .iter()
                    .copied()
                    .find(|last| group.contains(last))
                    .unwrap_or_else(|| group.iter().copied().min_by_key(|tool| crate::panels::toolbar::toolbar_position(*tool)).unwrap_or(t)),
            };
            if before != next_tool {
                crate::type_path_placement::cancel(app);
            }
            app.ui.tool = next_tool;
            crate::tool_memory::ended_by_tool(app);
            // `consume_key` ignores extra ⇧ and ⌥: a ⇧/⌥ switch is never spring-loaded.
            let (plain, now) = ctx.input(|i| (!cycle_shift && !i.modifiers.shift && !i.modifiers.alt, i.time));
            crate::spring_tools::pressed(app, t.key(), before, plain, now);
            if app.ui.spring.is_none() {
                // Permanent key choices own the tool; held-key switches keep the pen marker.
                app.stylus.tool_before_eraser = None;
            }
            return true;
        }
    }
    false
>>>>>>> upstream/main
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "tool_key_default_tests.rs"]
mod tool_key_default_tests;
