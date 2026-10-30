use craft_keymap::CommandRef;
use craft_ui::menu::{MenuBar, MenuBarLabels};

use super::invoke::invoke;
use super::items::{ItemCache, menu_items};
use super::model::{HELP, menu_model, platform, ui_menus};
use super::{PhotocraftApp, Value, json};

/// Draws the menu bar with the library's; returns the inner edge of the menu titles, facing the
/// rest of the title bar (the right edge of the last title, or its left edge in a right-to-left
/// UI, where File is at the right). The bar takes only the width its titles need.
pub fn menu_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) -> f32 {
    let ctx = ui.ctx().clone();
    let lang = crate::i18n::current();
    let cache = fresh_cache(app, ui);
    let labels = MenuBarLabels {
        // Not "⋯" (U+22EF): the shell's fonts have no glyph for it.
        more: "…".into(),
        all: crate::i18n::tr(lang, "Menu").into(),
        search: crate::i18n::tr(lang, "Search menus").into(),
        clear: crate::i18n::tr(lang, "Clear").into(),
        no_matches: crate::i18n::tr(lang, "No matching commands").into(),
        scroll_up: crate::i18n::tr(lang, "Scroll menu up").into(),
        scroll_down: crate::i18n::tr(lang, "Scroll menu down").into(),
    };
    // Option is a typing key on a Mac, so the bar has no mnemonics there.
    craft_ui::menu::set_mnemonics(&ctx, Some(!platform(&ctx).is_mac()));
    let shown = MenuBar::new(&cache.menus)
        .id(egui::Id::unique("photocraft-menu-bar"))
        .shrink_to_fit(f32::INFINITY)
        .labels(&labels)
        .help_search(HELP)
        .text_editing(ctx.text_edit_focused())
        .show(ui);
    let titles = shown.titles;
    app.menu_cache = Some(cache);
    if let Some(command) = shown.chosen {
        run(app, &ctx, command);
    }
    if crate::direction::is_rtl() { titles.left() } else { titles.right() }
}

/// The cached menu rows, built again when the menus may have changed and someone can see them:
/// a menu is open or the keyboard is on the bar, the pointer is over the bar, or Alt or F10 was
/// pressed, or the language changed. Building them asks every command whether it can run, which on a document with
/// thousands of layers costs far more than a frame, so an idle bar keeps what it has. A frame
/// with input while a menu is open rebuilds, which covers state the hash does not list.
fn fresh_cache(app: &mut PhotocraftApp, ui: &egui::Ui) -> ItemCache {
    let ctx = ui.ctx();
    let lang = crate::i18n::current();
    let kept = app.menu_cache.take();
    let open = craft_ui::menu::bar::is_open(ctx) || craft_ui::menu::bar::is_focused(ctx);
    let keys =
        ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, key, modifiers, .. } if modifiers.alt || *key == egui::Key::F10)));
    let hovered = ctx.pointer_hover_pos().is_some_and(|p| ui.max_rect().contains(p));
    let input = crate::native_menu::had_input(ctx);
    let state = crate::native_menu::state_hash(app, None);
    let language = lang.code().to_string();
    match kept {
        Some(c) if c.language == language && !((open || hovered || keys) && (c.state != state || (open && input))) => c,
        _ => {
            let model = menu_model(&menu_items(app));
            ItemCache { state, language, menus: ui_menus(&model, lang, platform(ctx)) }
        }
    }
}

/// Runs the command a menu row chose.
fn run(app: &mut PhotocraftApp, ctx: &egui::Context, command: CommandRef) {
    let id = command.id;
    let alt = ctx.input(|i| i.modifiers.alt);
    // ⌥ while choosing an Image › Adjustments item opens it at its last settings (#8900).
    let r = if alt && crate::adjust_dialog::has_dialog(&id) {
        let opened = crate::adjust_dialog::open_last(app, &id).map(|_| Value::Null).ok_or_else(|| "no document".to_string());
        // Edit › Search's recents, as `invoke` notes a plain click.
        if opened.is_ok() {
            crate::palette::note_recent(ctx, &id);
        }
        opened
    } else {
        invoke(app, ctx, &alt_click(id, alt), json!({}))
    };
    if let Err(e) = r {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

/// The command a menu click runs: ⌥ + Merge Down / Merge Layers / Merge Visible keep the
/// originals, running Stamp Down / Stamp Visible instead (#217).
pub fn alt_click(id: String, alt: bool) -> String {
    match photocraft_engine::stamp_cmds::alt_variant(&id) {
        Some(stamp) if alt => stamp.to_string(),
        _ => id,
    }
}
