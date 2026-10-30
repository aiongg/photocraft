//! PhotoCraft's menus as a craft-menu model, and the rows the library's menu bar draws.
//!
//! [`menu_model`] turns the flat rows of [`menu_items`](super::menu_items) into a
//! [`craft_menu::MenuBar`] in English, with the roles the Mac layout needs. [`ui_menus`] converts
//! that model into the library bar's [`craft_ui::menu::MenuItem`] rows in the UI language, adding
//! what craft-menu's items do not carry: Edit › Menus tints and the row tooltips.

use std::collections::HashMap;

use craft_keymap::{Chord, CommandRef, Platform};
use craft_menu::{ItemRole, MenuRole, Node, Row};
use craft_ui::menu::{MenuEntry, MenuItem as UiItem};

use super::items::{MenuItem, menu_tint};
use super::state::translated_label;
use super::{DOCUMENT_ITEM, TOP_MENUS};
use crate::i18n::{Lang, tr};

/// Menu rows the library bar draws: one submenu row per top-level menu.
pub(crate) type UiMenus = Vec<UiItem<CommandRef>>;

/// The model of the menus, and the tints the model has no field for.
pub struct Model {
    pub bar: craft_menu::MenuBar,
    /// Edit › Menus colour of a command, by command id.
    pub tints: HashMap<String, egui::Color32>,
}

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

/// Index of the Help menu in [`TOP_MENUS`]: the menu that holds the search field.
pub(crate) const HELP: usize = 9;

/// The menus as craft-menu's tree, in English. A shortcut that is not a craft-keymap chord is left
/// off the row.
pub fn menu_model(items: &[MenuItem]) -> Model {
    let mut tints = HashMap::new();
    let rows: Vec<Row> = items
        .iter()
        .map(|it| {
            if let Some(color) = it.color.as_deref().and_then(menu_tint) {
                tints.insert(it.id.clone(), color);
            }
            Row {
                path: it.path.clone(),
                command: CommandRef::new(it.id.as_str()),
                label: it.label.clone(),
                shortcut: it.shortcut.as_deref().and_then(|s| s.parse::<Chord>().ok()),
                enabled: it.enabled,
                kind: it.checked.map_or(craft_menu::ItemKind::Plain, craft_menu::ItemKind::Check),
            }
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
    Model { bar, tints }
}

/// The platform whose shortcut names the bar writes.
pub(crate) fn platform(ctx: &egui::Context) -> Platform {
    match ctx.os() {
        egui::os::OperatingSystem::Mac | egui::os::OperatingSystem::IOS => Platform::Mac,
        egui::os::OperatingSystem::Windows => Platform::Windows,
        _ => Platform::Linux,
    }
}

/// The model's menus as library bar rows, in `lang`. A label or title that differs from its English
/// one carries the English as well, which the bar's search matches and its mnemonics use for a
/// label with no Latin letter.
pub(crate) fn ui_menus(model: &Model, lang: Lang, platform: Platform) -> UiMenus {
    let cx = Cx { lang, platform, tints: &model.tints };
    model
        .bar
        .menus
        .iter()
        .map(|menu| {
            let mut entries = cx.entries(&menu.children);
            if entries.is_empty() {
                entries.push(UiItem::new(tr(lang, "(coming soon)").to_string(), CommandRef::new("menu.empty")).enabled(false).into());
            }
            let mut row = cx.submenu(&menu.title, entries);
            if let Some((_, letter)) = TOP_PREFERRED.iter().find(|(t, _)| *t == menu.title) {
                row = row.mnemonic(*letter);
            }
            row
        })
        .collect()
}

struct Cx<'a> {
    lang: Lang,
    platform: Platform,
    tints: &'a HashMap<String, egui::Color32>,
}

impl Cx<'_> {
    fn submenu(&self, name: &str, entries: Vec<MenuEntry<CommandRef>>) -> UiItem<CommandRef> {
        let shown = tr(self.lang, name).to_string();
        let english = (shown != name).then(|| name.to_string());
        let row = UiItem::submenu(shown, entries);
        match english {
            Some(english) => row.english(english),
            None => row,
        }
    }

    fn entries(&self, nodes: &[Node]) -> Vec<MenuEntry<CommandRef>> {
        nodes
            .iter()
            .filter_map(|n| match n {
                Node::Item(it) => Some(self.item(it).into()),
                Node::Separator => Some(MenuEntry::Separator),
                Node::Submenu { label, children, .. } => Some(self.submenu(label, self.entries(children)).into()),
                Node::Standard { .. } => None,
            })
            .collect()
    }

    fn item(&self, it: &craft_menu::Item) -> UiItem<CommandRef> {
        let id = it.command.id.as_str();
        // A document's name is shown as it is.
        let shown = if id.starts_with(DOCUMENT_ITEM) { it.label.clone() } else { translated_label(self.lang, id, &it.label) };
        let mut row = UiItem::new(shown.clone(), it.command.clone()).enabled(it.enabled);
        if let craft_menu::ItemKind::Check(on) | craft_menu::ItemKind::Radio(on) = it.kind {
            row = row.check(on);
        }
        if let Some(chord) = &it.shortcut {
            row = row.shortcut(craft_keymap::ui::kbd(chord, self.platform));
        }
        if shown != it.label && !id.starts_with(DOCUMENT_ITEM) {
            row = row.english(it.label.clone());
        }
        if let Some(color) = self.tints.get(id) {
            row = row.tint(*color);
        }
        match id {
            "image.mode.bits8" | "image.mode.bits16" => row.tooltip(tr(self.lang, "Integer").to_string()),
            "image.mode.bits32" => row.tooltip(tr(self.lang, "Floating point").to_string()),
            _ => row,
        }
    }
}
