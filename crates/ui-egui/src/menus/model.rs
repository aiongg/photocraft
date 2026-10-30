//! PhotoCraft's menus as a craft-menu model.
//!
//! [`menu_model`] turns the flat rows of [`menu_items`](super::menu_items) into a
//! [`craft_menu::MenuBar`] in English, with the roles the Mac layout needs, the Edit › Menus
//! tints and the row tooltips. [`translated`] writes it in the UI language for the in-window bar.

use std::collections::HashMap;

use craft_keymap::{Chord, CommandRef};
use craft_menu::{Item, ItemRole, MenuRole, Node, Row};

use super::items::{MenuItem, menu_tint};
use super::state::translated_label;
use super::{DOCUMENT_ITEM, TOP_MENUS};
use crate::i18n::{Lang, tr};

/// Photoshop's mnemonics for the top menus (Windows), used where the shown label has the letter.
const TOP_PREFERRED: [(&str, char); 10] = [
    ("File", 'F'),
    ("Edit", 'E'),
    ("Image", 'I'),
    ("Layer", 'L'),
    ("Type", 'Y'),
    ("Select", 'S'),
    ("Filter", 'T'),
    ("View", 'V'),
    ("Window", 'W'),
    ("Help", 'H'),
];

/// The menus as craft-menu's tree, in English. A shortcut that is not a craft-keymap chord is left
/// off the row.
pub fn menu_model(items: &[MenuItem]) -> craft_menu::MenuBar {
    let by_id: HashMap<&str, &MenuItem> = items.iter().map(|it| (it.id.as_str(), it)).collect();
    let rows: Vec<Row> = items
        .iter()
        .map(|it| Row {
            path: it.path.clone(),
            command: CommandRef::new(it.id.as_str()),
            label: it.label.clone(),
            shortcut: it.shortcut.as_deref().and_then(|s| s.parse::<Chord>().ok()),
            enabled: it.enabled,
            kind: it.checked.map_or(craft_menu::ItemKind::Plain, craft_menu::ItemKind::Check),
        })
        .collect();
    let mut bar = craft_menu::from_items(&TOP_MENUS, &rows);
    for (title, role) in [("File", MenuRole::File), ("Edit", MenuRole::Edit), ("View", MenuRole::View), ("Window", MenuRole::Window), ("Help", MenuRole::Help)]
    {
        bar.tag_menu(title, role);
    }
    bar.tag_item(&"help.about".into(), ItemRole::About);
    bar.tag_item(&"file.exit".into(), ItemRole::Quit);
    bar.tag_submenu(MenuRole::Edit, "Preferences", ItemRole::Settings);
    bar.tag_submenu(MenuRole::Window, "Theme", ItemRole::Appearance);
    bar.tag_item(&"window.theme.toggle".into(), ItemRole::Appearance);
    fn decorate(nodes: &mut [Node], by_id: &HashMap<&str, &MenuItem>) {
        for n in nodes {
            match n {
                Node::Item(item) => {
                    if let Some(color) = by_id.get(item.command.id.as_str()).and_then(|it| it.color.as_deref()).and_then(menu_tint) {
                        item.tint = Some(color.into());
                    }
                    item.tooltip = tooltip(&item.command.id).map(str::to_string);
                }
                Node::Submenu { children, .. } => decorate(children, by_id),
                _ => {}
            }
        }
    }
    for menu in &mut bar.menus {
        decorate(&mut menu.children, &by_id);
    }
    bar
}

/// The English tooltip of a row (translated when the bar is written in the UI language).
fn tooltip(id: &str) -> Option<&'static str> {
    match id {
        "image.mode.bits8" | "image.mode.bits16" => Some("Integer"),
        "image.mode.bits32" => Some("Floating point"),
        _ => None,
    }
}

/// The English names behind a translated bar, for the bar's mnemonics and Help search.
#[derive(Default)]
pub(crate) struct English {
    commands: HashMap<String, String>,
    titles: HashMap<String, String>,
}

impl English {
    pub(crate) fn of(&self, labeled: craft_menu::Labeled<'_>) -> Option<String> {
        match labeled {
            craft_menu::Labeled::Command(c) => self.commands.get(&c.id).cloned(),
            craft_menu::Labeled::Title(t) => self.titles.get(t).cloned(),
        }
    }
}

/// The model written in `lang`, with the English names of what differs. A menu left with no rows
/// shows a disabled "(coming soon)".
pub(crate) fn translated(model: &craft_menu::MenuBar, lang: Lang) -> (craft_menu::MenuBar, English) {
    let mut bar = model.clone();
    let mut english = English::default();
    fn walk(nodes: &mut [Node], lang: Lang, english: &mut English) {
        for n in nodes {
            match n {
                Node::Item(it) => {
                    // A document's name is shown as it is.
                    if it.command.id.starts_with(DOCUMENT_ITEM) {
                        continue;
                    }
                    let shown = translated_label(lang, &it.command.id, &it.label);
                    if shown != it.label {
                        english.commands.insert(it.command.id.clone(), std::mem::replace(&mut it.label, shown));
                    }
                    if let Some(tip) = it.tooltip.take() {
                        it.tooltip = Some(tr(lang, &tip).to_string());
                    }
                }
                Node::Submenu { label, children, .. } => {
                    let shown = tr(lang, label).to_string();
                    if shown != *label {
                        english.titles.insert(shown.clone(), std::mem::replace(label, shown));
                    }
                    walk(children, lang, english);
                }
                _ => {}
            }
        }
    }
    for menu in &mut bar.menus {
        let shown = tr(lang, &menu.title).to_string();
        if shown != menu.title {
            english.titles.insert(shown.clone(), std::mem::replace(&mut menu.title, shown));
        }
        if menu.children.is_empty() {
            menu.children.push(Item::new("menu.empty", tr(lang, "(coming soon)")).enabled(false).into());
        }
        walk(&mut menu.children, lang, &mut english);
    }
    (bar, english)
}

/// The preferred mnemonics of the top menus, by title as shown.
pub(crate) fn top_mnemonics(lang: Lang) -> Vec<(String, char)> {
    TOP_PREFERRED.iter().map(|(t, c)| (tr(lang, t).to_string(), *c)).collect()
}
