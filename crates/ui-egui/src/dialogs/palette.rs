//! ⌘F command palette: search over every menu command and tool, on craft-ui's `CommandPalette`.

use craft_ui::behavior::PaletteEntry;
use craft_ui::controls::Kbd;
use craft_ui::patterns::{CommandPalette, PaletteLabels};

use crate::PhotocraftApp;

/// How many recently run commands the palette remembers.
const RECENT_CAP: usize = 8;

/// The id of the palette's recently-run list (egui temp data: session-scoped).
fn recent_id() -> egui::Id {
    egui::Id::unique("palette-recent")
}

/// Remember `id` as the most recently run command: deduplicated, newest first, capped.
pub(crate) fn note_recent(ctx: &egui::Context, id: &str) {
    ctx.data_mut(|d| {
        let mut list: Vec<String> = d.get_temp(recent_id()).unwrap_or_default();
        push_recent(&mut list, id, RECENT_CAP);
        d.insert_temp(recent_id(), list);
    });
}

/// The palette's recently run commands, newest first (for tests).
#[cfg(test)]
pub(crate) fn recent_ids(ctx: &egui::Context) -> Vec<String> {
    ctx.data(|d| d.get_temp(recent_id()).unwrap_or_default())
}

/// [`note_recent`]'s core: move `id` to the front, dropping the oldest past `cap`.
fn push_recent(list: &mut Vec<String>, id: &str, cap: usize) {
    list.retain(|x| x != id);
    list.insert(0, id.to_string());
    list.truncate(cap);
}

/// The palette's entries: every menu command (separators are not commands: they would match on
/// their menu path and list as blank rows, #2479) and every tool. The command is the menu id, or
/// `tool:<Tool>`.
fn entries(app: &PhotocraftApp, lang: crate::i18n::Lang) -> Vec<PaletteEntry<String>> {
    let mut out: Vec<PaletteEntry<String>> = crate::menus::menu_items(app)
        .into_iter()
        .filter(|m| m.label != "---")
        .map(|m| {
            let label = crate::i18n::tr_id(lang, &m.id, &m.label).trim_end_matches('…').to_string();
            let mut e = PaletteEntry::new(m.id.clone(), label)
                .english(m.label.clone())
                .path(m.path.iter().map(|p| crate::i18n::tr(lang, p)))
                .path_english(m.path.iter().cloned())
                .enabled(m.enabled);
            if let Some(k) = m.shortcut.as_deref().and_then(crate::shortcuts::parse) {
                e = e.shortcut(Kbd::new(&k));
            }
            e
        })
        .collect();
    for tool in crate::state::Tool::ALL {
        out.push(
            PaletteEntry::new(format!("tool:{tool:?}"), tl!(tool.label()))
                .english(tool.label())
                .kind(crate::i18n::tr(lang, "Tool"))
                .shortcut(Kbd::keys(&[tool.key().to_string()])),
        );
    }
    out
}

/// The palette's texts in the UI language.
fn labels() -> PaletteLabels {
    let mut l = PaletteLabels::default();
    l.placeholder = tl!("Search commands, tools and panels…").into();
    l.empty = tl!("No matching commands").into();
    l.hint = crate::i18n::fmt(tl!("↑↓ navigate   {key} run   esc close"), &[("key", &crate::shortcuts::pretty("Enter"))]);
    l
}

pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !app.ui.palette_open {
        return;
    }
    let lang = crate::i18n::Lang::from_pref(&app.session.prefs().interface.language);
    let entries = entries(app, lang);
    let mut recents: Vec<String> = ctx.data(|d| d.get_temp(recent_id()).unwrap_or_default());
    let mut open = true;
    let chosen = CommandPalette::new("palette", &entries).labels(labels()).recents(&mut recents).show(ctx, &mut open).map(|e| e.command().clone());
    ctx.data_mut(|d| d.insert_temp(recent_id(), recents));
    app.ui.palette_open = open;
    let Some(id) = chosen else { return };
    if let Some(tool) = id.strip_prefix("tool:") {
        if let Some(t) = crate::state::Tool::from_name(tool) {
            app.ui.tool = t;
        }
    } else if let Err(e) = crate::menus::invoke(app, ctx, &id, serde_json::json!({})) {
        app.ui.status = e;
    }
}

#[cfg(test)]
mod tests {
    use egui::{Event, Key, Modifiers, vec2};
    use egui_kittest::Harness;
    use serde_json::json;

    use super::push_recent;
    use crate::PhotocraftApp;

    #[test]
    fn open_palette_interrupts_the_ime_only_when_it_takes_focus() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        // Fonts bind on the next pass; this one opens nothing.
        ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
        app.ui.palette_open = true;
        // The field takes focus on the first pass and owns the IME from the second on.
        let interrupts: Vec<Option<bool>> = (0..4)
            .map(|_| {
                let mut out = ctx.run_ui(Default::default(), |ui| super::show(&mut app, ui.ctx()));
                out.textures_delta.clear();
                out.platform_output.ime.map(|ime| ime.should_interrupt_composition)
            })
            .collect();
        // The box takes focus on its second pass (the first sizes it), interrupting once; after that the
        // field owns the input method and is never asked again.
        assert_eq!(interrupts, [None, Some(true), Some(false), Some(false)]);
    }

    #[test]
    fn push_recent_moves_to_the_front_and_caps() {
        let mut l = Vec::new();
        push_recent(&mut l, "b", 3);
        push_recent(&mut l, "a", 3);
        push_recent(&mut l, "c", 3);
        assert_eq!(l, ["c", "a", "b"]);
        push_recent(&mut l, "a", 3);
        assert_eq!(l, ["a", "c", "b"], "re-running a command moves it to the front");
        push_recent(&mut l, "d", 3);
        assert_eq!(l, ["d", "a", "c"], "the oldest falls off the cap");
    }

    #[test]
    fn the_palette_records_and_replays_recent_commands_on_an_empty_query() {
        let mut h = Harness::builder().with_size(vec2(1200.0, 800.0)).with_max_steps(64).build_eframe(|cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default())
        });
        h.state_mut().run("file.new", json!({"width": 64, "height": 64})).unwrap();
        h.state_mut().sync_views();
        let ctx = h.ctx.clone();
        // Run "Keyboard Shortcuts" through the palette: it becomes the recent command.
        crate::menus::invoke(h.state_mut(), &ctx, "edit.search", json!({})).unwrap();
        h.run_steps(2);
        h.event(Event::Text("keyboard shortcuts".into()));
        h.run_steps(2);
        h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
        h.run_steps(3);
        assert_eq!(super::recent_ids(&ctx).len(), 1, "the command the palette ran is remembered");
        // Close its dialog, reopen the palette with an empty query and press Enter: the recent
        // command is the first hit, so the same dialog opens again.
        let dialog = h
            .state()
            .ui
            .dialogs
            .iter()
            .find(|d| d.fields.get("__prefsui").and_then(|v| v.as_str()) == Some("shortcuts"))
            .map(|d| d.id)
            .expect("the shortcuts dialog");
        h.state_mut().ui.close_dialog(dialog);
        h.run_steps(2);
        crate::menus::invoke(h.state_mut(), &ctx, "edit.search", json!({})).unwrap();
        h.run_steps(2);
        // The first Enter is still held: a repeat of it would not submit.
        h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
        h.run_steps(1);
        h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
        h.run_steps(3);
        assert!(
            h.state().ui.dialogs.iter().any(|d| d.fields.get("__prefsui").and_then(|v| v.as_str()) == Some("shortcuts")),
            "the empty-query palette runs the recent command"
        );
        assert_eq!(super::recent_ids(&ctx).len(), 1, "re-running it moves it to the front, no duplicate");
    }

    /// #2479: menu separators matched on their menu path ("f" → File, Filter) and filled the
    /// palette with blank "---" rows.
    #[test]
    fn separators_are_not_palette_entries() {
        let app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        assert!(crate::menus::menu_items(&app).iter().any(|m| m.label == "---"), "the menus have separators to filter");
        let entries = super::entries(&app, crate::i18n::Lang::EN);
        assert!(!entries.is_empty());
        assert!(entries.iter().all(|e| e.command() != "---" && e.label() != "---"));
    }

    /// #536: Edit › Search → "Keyboard Shortcuts" must open the dialog, with and without a document.
    #[test]
    fn palette_opens_the_keyboard_shortcuts_dialog() {
        for with_doc in [false, true] {
            let mut h = Harness::builder().with_size(vec2(1200.0, 800.0)).with_max_steps(64).build_eframe(|cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default())
            });
            if with_doc {
                h.state_mut().run("file.new", json!({"width": 64, "height": 64})).unwrap();
                h.state_mut().sync_views();
            }
            let ctx = h.ctx.clone();
            crate::menus::invoke(h.state_mut(), &ctx, "edit.search", json!({})).unwrap();
            h.run_steps(2);
            assert!(h.state().ui.palette_open, "Edit > Search opens the palette (with_doc={with_doc})");
            h.event(Event::Text("keyboard shortcuts".into()));
            h.run_steps(2);
            h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
            h.run_steps(3);
            let app = h.state();
            assert!(!app.ui.palette_open, "Enter closes the palette (with_doc={with_doc})");
            let dialog = app
                .ui
                .dialogs
                .iter()
                .find(|d| d.fields.get("__prefsui").and_then(|v| v.as_str()) == Some("shortcuts"))
                .map(|d| d.id)
                .unwrap_or_else(|| panic!("#536: the Keyboard Shortcuts dialog did not open (with_doc={with_doc})"));
            // The dialog keeps its own Enter: a second press is its OK and closes it (after the first
            // press is released: a key repeat doesn't answer a dialog).
            h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
            h.run_steps(1);
            h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
            h.run_steps(3);
            assert!(h.state().ui.dialogs.iter().all(|d| d.id != dialog), "a second Enter closes the dialog as its OK (with_doc={with_doc})");
        }
    }
}
