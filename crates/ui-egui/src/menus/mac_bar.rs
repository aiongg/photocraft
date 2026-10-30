//! The macOS menu bar: the same menus as the in-window bar ([`menu_model`]), laid out the way a Mac
//! app's are by craft-menu ([`craft_menu::mac_layout`]) and shown by its AppKit backend.
//!
//! What the layout does:
//! - an **app menu** named PhotoCraft: About (from Help), Settings (Edit › Preferences), Language,
//!   Appearance (Window › Theme), Services, Hide, Hide Others, Show All and Quit (File › Exit,
//!   still the app's own `file.exit` so the unsaved-changes prompt runs);
//! - **Window** gets Minimize, Zoom and Bring All to Front, with the open documents after it
//!   (where a Mac app lists its windows), and **Help** the system search field;
//! - **clashes** with the system's keys (⌘H, ⌘M, ⌘W, ⌘,) are resolved in PhotoCraft's favour, and
//!   reported; Hide and Minimize move to ⌃⌘H and ⌃⌘M, as Photoshop does.
//!
//! **Keys stay with egui.** AppKit runs a menu's key equivalents before the window sees the key,
//! which would skip PhotoCraft's key rules (text fields keep ⌘C/⌘V/⌘Z, dialogs and Camera Raw are
//! modal, Liquify has its own ⌘Z). So the backend reports a key equivalent as a key press, and
//! [`NativeMenu::raw_input`] feeds it to egui as the one it was ([`ClipboardKeys::Raw`]: the
//! clipboard keys too): the shortcut runs through [`crate::shortcuts::handle`] exactly as without
//! a native menu. A click is a command and runs like an in-window menu click.

use craft_keymap::CommandRef;
#[cfg(target_os = "macos")]
use craft_menu::{ClipboardKeys, NativeMenu};
use craft_menu::{Document, Item, ItemRole, Layout, LayoutOptions, Menu, MenuBar, Node};

use super::items::MenuItem;
use super::model::menu_model;
use super::state::{state_hash, translated_label};
use super::{DOCUMENT_ITEM, PhotocraftApp};
use crate::i18n::{Lang, tr};

pub const APP_NAME: &str = "PhotoCraft";

/// Command ids of the app menu's Language items: `app.language.<code>`, where `auto` follows the
/// system (the `interface.language` preference values).
pub const LANGUAGE_PREFIX: &str = "app.language.";

/// PhotoCraft's menus as a Mac menu bar, in the UI language; `language` is the
/// `interface.language` preference (`auto` or a code), checked in the app menu's Language.
pub fn mac_layout(items: &[MenuItem], lang: Lang, language: &str) -> Layout {
    let mut bar = menu_model(items);
    translate(&mut bar, lang);
    list_documents(&mut bar);
    // The layout moves Language into the app menu, after Settings, wherever it is put.
    if let Some(menu) = bar.menus.first_mut() {
        menu.children.push(language_node(language, lang));
    }
    let label = |english: &'static str| tr(lang, english).to_string();
    let options = LayoutOptions { label: &label, ..LayoutOptions::new(APP_NAME) };
    craft_menu::mac_layout(&bar, &options)
}

/// Language in the app menu: Auto, then every UI language in its own name, with the current
/// preference checked.
fn language_node(pref: &str, lang: Lang) -> Node {
    let item = |code: &str, label: &str| Node::Item(Item::new(CommandRef::new(format!("{LANGUAGE_PREFIX}{code}")), label).check(pref == code));
    let mut children = vec![item("auto", tr(lang, "Auto")), Node::Separator];
    children.extend(Lang::for_picker().into_iter().map(|l| item(l.code(), l.name())));
    Node::submenu(tr(lang, "Language"), children).role(ItemRole::Language)
}

/// The Window menu's document rows become the bar's open documents, which the layout lists last in
/// Window, the one in front selected.
fn list_documents(bar: &mut MenuBar) {
    let is_document = |n: &Node| matches!(n, Node::Item(it) if it.command.id.starts_with(DOCUMENT_ITEM));
    let mut documents = Vec::new();
    for menu in &mut bar.menus {
        let (docs, rest): (Vec<Node>, Vec<Node>) = std::mem::take(&mut menu.children).into_iter().partition(is_document);
        menu.children = rest;
        for node in docs {
            if let Node::Item(it) = node {
                let active = it.kind.checked() == Some(true);
                documents.push(Document::new(it.command, it.label).active(active));
            }
        }
    }
    bar.documents = documents;
}

/// Translate menu titles, submenus and items; the layout writes the labels of its own items.
fn translate(bar: &mut MenuBar, lang: Lang) {
    fn walk(nodes: &mut [Node], lang: Lang) {
        for n in nodes {
            match n {
                // A document's name is shown as it is.
                Node::Item(it) if !it.command.id.starts_with(DOCUMENT_ITEM) => it.label = translated_label(lang, &it.command.id, &it.label),
                Node::Submenu { label, children, .. } => {
                    *label = tr(lang, label).to_string();
                    walk(children, lang);
                }
                _ => {}
            }
        }
    }
    for Menu { title, children, .. } in &mut bar.menus {
        *title = tr(lang, title).to_string();
        walk(children, lang);
    }
}

/// Install the menu bar now (in eframe's creator closure, so winit's default menu doesn't stay).
/// `None` when AppKit refuses it: the app keeps its in-window menus.
#[cfg(target_os = "macos")]
pub fn install(ctx: &egui::Context, app: &PhotocraftApp) -> Option<NativeMenu> {
    let layout = layout_of(app);
    // Never crash for a menu: if AppKit refuses, keep the in-window menus.
    let built =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| craft_menu::mac::install_with(ctx, &layout.bar, craft_menu::mac::MacOptions::default())));
    match built {
        // PhotoCraft runs its clipboard shortcuts itself (`shortcuts::clipboard_keys`).
        Ok(menu) => menu.map(|m| m.clipboard_keys(ClipboardKeys::Raw)),
        Err(_) => {
            log::warn!("couldn't install the macOS menu bar; using the in-window menus");
            None
        }
    }
}

/// The app's menus laid out for the Mac bar: rows disabled by a modal dialog, Camera Raw or a
/// full-window workspace greyed (the rule of [`super::modal_allows`]).
fn layout_of(app: &PhotocraftApp) -> Layout {
    let mut items = super::menu_items(app);
    for it in &mut items {
        it.enabled &= super::modal_allows(app, &it.id);
    }
    mac_layout(&items, crate::i18n::current(), &app.session.prefs().interface.language)
}

/// Run the native menu clicks that arrived since the last frame, like in-window menu clicks.
pub fn run(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(menu) = app.services.native_menu.as_mut() else { return };
    // Minimize is run by the library.
    for command in menu.take_clicks(ctx) {
        // The rows are disabled while a dialog is open; a click that raced the update is dropped.
        if !super::modal_allows(app, &command.id) {
            continue;
        }
        if let Err(e) = super::invoke(app, ctx, &command.id, serde_json::json!({})) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
    }
}

/// Bring the native menu up to date when anything it shows may have changed: a command ran, the
/// document or selection changed, or the user clicked or pressed a key. Building the rows walks
/// every command's enabled state, so it doesn't run on frames that only animate or scroll.
pub fn sync(app: &mut PhotocraftApp, ctx: &egui::Context) {
    // Out of the app while the layout reads it.
    let Some(mut menu) = app.services.native_menu.take() else { return };
    let state = state_hash(app, craft_menu::had_input(ctx).then_some(app.frame));
    for clash in menu.sync_with(state, || layout_of(app)) {
        log::info!("menu: {:?} is {} on macOS and {} in PhotoCraft; {}", clash.shortcut, clash.system, clash.command.id, clash.resolution);
    }
    app.services.native_menu = Some(menu);
}

#[cfg(test)]
mod tests;
