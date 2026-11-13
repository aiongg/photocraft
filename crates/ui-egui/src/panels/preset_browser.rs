//! The Patterns and Styles panels and the pattern picker on the framework's `PresetBrowser` and `PresetPicker`. Rows
//! come from a panel's groups in the panel's folder order; the browser's events run as the panel's engine commands,
//! through one mapping for every preset panel (`events`).

use craft::ui::contract::{Bind, DropZone, Show as _};
use craft::ui::list::ThumbAspect;
use craft::ui::menu::PanelRow;
use craft::ui::patterns::presets::{
    DefaultOpen, PresetBrowser, PresetCommands, PresetEvent, PresetExport, PresetFileRequest, PresetFiles, PresetFooterEvent, PresetFooterOptions,
    PresetMenuAction, PresetPanelMenu, PresetPicker, PresetReorder, PresetRow, PresetState, PresetView,
};
use craft::ui::runtime::commands::CommandSet as _;
use craft::ui::primitives::ContentPainterCx;
use craft::ui::primitives::textures::TextureCache;
use craft::ui::tile_grid::TileZoom;
use egui::{Color32, ColorImage, Pos2, Rect, pos2};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::preset_panels::{PresetUi, canvas_drop_rect, run};
use crate::PhotocraftApp;

/// A row of a preset browser: a folder by name, or a preset by its key in the panel (a pattern id, a style name).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PresetKey {
    Group(String),
    Item(String),
}

/// What tells one preset panel from another: its key in `PresetUi`, its engine command prefix and its New button.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PresetPanel {
    /// The panel's key in `PresetUi` (`selected`, `group_order`, `browsers`).
    pub(crate) id: &'static str,
    /// The panel's name, in English.
    label: &'static str,
    /// The engine prefix of `{prefix}.edit` and `{prefix}.new`.
    pub(crate) prefix: &'static str,
    /// The params of `{prefix}.new`.
    new_params: fn() -> Value,
    /// A new preset opens its rename (patterns are named by their source and do not).
    rename_new: bool,
}

pub(crate) const PATTERNS: PresetPanel =
    PresetPanel { id: "patterns", label: "Patterns", prefix: "pattern.presets", new_params: || json!({}), rename_new: false };
pub(crate) const STYLES: PresetPanel =
    PresetPanel { id: "styles", label: "Styles", prefix: "style.presets", new_params: || json!({"name": "Style"}), rename_new: true };

/// A folder of a panel: its name and its presets as (key, name).
pub(crate) struct Folder {
    pub(crate) name: String,
    pub(crate) items: Vec<(String, String)>,
}

/// The browser's rows: the folders in `order` (library indices), each followed by its presets.
fn rows(folders: &[Folder], order: &[usize]) -> Vec<PresetRow<PresetKey>> {
    let mut rows = Vec::new();
    for folder in order.iter().filter_map(|&at| folders.get(at)) {
        rows.push(PresetRow::group(PresetKey::Group(folder.name.clone()), folder.name.clone(), 0));
        rows.extend(folder.items.iter().map(|(key, name)| PresetRow::item(PresetKey::Item(key.clone()), name.clone(), 1)));
    }
    rows
}

/// The thumbnail sizes of every preset panel, as factors of the grid's large thumbnail (48 pt): 20 to 96 pt in 6 pt
/// steps, 40 pt by default.
const THUMB_RANGE: TileZoom = TileZoom::bounded(20.0 / 48.0, 2.0, 6.0 / 48.0, 40.0 / 48.0);

/// Where preset drops land: the canvas proper, rulers excluded, on the canvas's layer.
fn canvas_zone(app: &PhotocraftApp) -> DropZone {
    DropZone::new(app.last_canvas_layer, canvas_drop_rect(app))
}

/// Shows `panel`'s browser over `folders` with `thumb` painting a preset's thumbnail, and returns its events with the
/// keys of the rows it showed, in order. `presets` is the panel state, `zone` the canvas; `room_height` sizes the
/// browser to its presets up to that height, else it takes the available height.
fn show(
    presets: &mut PresetUi,
    zone: DropZone,
    ui: &mut egui::Ui,
    panel: PresetPanel,
    folders: &[Folder],
    thumb: &dyn Fn(&ContentPainterCx, Rect, &str),
    room_height: Option<f32>,
) -> (Vec<PresetEvent<PresetKey>>, Vec<PresetKey>) {
    let order = presets.display_order(panel.id, &names_of(folders));
    let rows = rows(folders, &order);
    let selected = presets.selected.get(panel.id).cloned().map(PresetKey::Item);
    let mut state = presets.browsers.get(panel.id).cloned().unwrap_or_else(|| {
        let mut state = PresetState::new();
        state.zoom = THUMB_RANGE.default_value();
        state
    });
    let mut browser = PresetBrowser::new(tl!(panel.label), Bind::bound(&mut state), &rows);
    if let Some(key) = selected.clone() {
        browser = browser.selected(key);
    }
    if let Some(height) = room_height {
        browser = browser.room_height(height);
    }
    let out = browser
        .browser_id(browser_id(panel))
        .default_open(DefaultOpen::First)
        .thumb_range(THUMB_RANGE)
        .reorder(PresetReorder::items_and_groups())
        .drop_zone(zone)
        .footer(PresetFooterOptions::new().new_group(true).can_delete(selected.is_some()))
        .thumb(|cx, rect, info| {
            if let PresetKey::Item(key) = info.key {
                thumb(cx, rect, key);
            }
        })
        .show(ui);
    presets.browsers.insert(panel.id.to_string(), state);
    (out.events, rows.into_iter().map(|row| row.key).collect())
}

fn browser_id(panel: PresetPanel) -> egui::Id {
    egui::Id::unique(("preset-browser", panel.id))
}

/// Paints `image` (cached under `key` in the framework's texture store) into `rect`.
fn picture(cx: &ContentPainterCx, rect: Rect, key: impl std::hash::Hash + 'static, image: impl FnOnce() -> ColorImage) {
    let texture = TextureCache::texture(cx.painter.ctx(), key, image);
    cx.picture(texture, rect, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), 0.0);
}

fn c32(c: [f32; 4]) -> Color32 {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(q(c[0]), q(c[1]), q(c[2]), q(c[3]))
}

/// A 64×64 swatch of `pattern`. Only the pixels the swatch shows are read: a pattern file can declare a padded source
/// of any size, and reading it whole to pick 4096 of its pixels is gigabytes of floats. Tiles smaller than the swatch
/// repeat; larger ones are shown shrunk.
fn pattern_image(pattern: &photocraft_doc::Pattern) -> ColorImage {
    const S: u32 = 64;
    let (w, h) = (pattern.width.max(1), pattern.height.max(1));
    let format = pattern.surface.format();
    let step = (w.max(h) as f32 / S as f32).max(1.0);
    let mut image = ColorImage::new([S as usize, S as usize], vec![Color32::TRANSPARENT; (S * S) as usize]);
    for y in 0..S {
        for x in 0..S {
            let sx = ((x as f32 * step) as u32) % w;
            let sy = ((y as f32 * step) as u32) % h;
            let px = pattern.surface.read_region(photocraft_geom::Rect::from_xywh(sx as i32, sy as i32, 1, 1));
            image.pixels[(y * S + x) as usize] = c32(photocraft_raster::to_rgba(&format, &px));
        }
    }
    image
}

fn paint_pattern(cx: &ContentPainterCx, rect: Rect, pattern: &photocraft_doc::Pattern) {
    picture(cx, rect, ("pattern", pattern.id.clone()), || pattern_image(pattern));
}

// ------------------------------------------------------------------ Patterns

/// Window › Patterns: the library's patterns in the panel's folders. A click selects, a double click or a drop on
/// the canvas fills with the pattern.
pub fn patterns_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    // Until the preset store has loaded the saved patterns, their folder entries would look deleted: don't prune
    // them yet.
    if app.services.preset_store.is_none() {
        photocraft_engine::presets::patterns::sync(&mut app.session);
    }
    let library = app.session.patterns.items.clone();
    let folders: Vec<Folder> = app
        .session
        .presets
        .pattern_groups
        .iter()
        .map(|group| Folder {
            name: group.name.clone(),
            items: group
                .items
                .iter()
                .filter_map(|id| library.iter().find(|pattern| &pattern.id == id))
                .map(|pattern| (pattern.id.clone(), pattern.display_name().to_string()))
                .collect(),
        })
        .collect();
    let thumb = |cx: &ContentPainterCx, rect: Rect, id: &str| {
        if let Some(pattern) = library.iter().find(|pattern| pattern.id == id) {
            paint_pattern(cx, rect, pattern);
        }
    };
    let zone = canvas_zone(app);
    let (events, rows) = show(&mut app.ui.presets_ui, zone, ui, PATTERNS, &folders, &thumb, None);
    apply(app, ui.ctx(), PATTERNS, &names_of(&folders), &rows, events);
}

// ------------------------------------------------------------------ Styles

/// The most height the Styles presets take before they scroll.
const STYLES_HEIGHT: f32 = 300.0;

/// Window › Styles: a click applies the style to the selected layers (Shift adds its effects), a drop on the canvas
/// applies it to the active layer.
pub fn styles_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let groups = app.session.presets.styles.clone();
    let folders = style_folders(app);
    let zone = canvas_zone(app);
    let session = &app.session;
    let thumb = |cx: &ContentPainterCx, rect: Rect, name: &str| {
        let Some(style) = groups.iter().flat_map(|group| &group.items).find(|style| style.name == name) else { return };
        let key = ("style", style.name.clone(), style.effects.len(), format!("{:?}{:?}", style.blend, style.fill_opacity));
        picture(cx, rect, key, || {
            const S: u32 = 64;
            let px = photocraft_engine::presets::styles::thumbnail(session, style, S);
            ColorImage::from_rgba_unmultiplied([S as usize, S as usize], &px)
        });
    };
    // The floating window sizes to its content.
    let (events, rows) = show(&mut app.ui.presets_ui, zone, ui, STYLES, &folders, &thumb, Some(STYLES_HEIGHT));
    apply(app, ui.ctx(), STYLES, &names_of(&folders), &rows, events);
}

/// The Styles folders: each style group with its styles, keyed and named by the style's name.
fn style_folders(app: &PhotocraftApp) -> Vec<Folder> {
    let groups = &app.session.presets.styles;
    groups.iter().map(|group| Folder { name: group.name.clone(), items: group.items.iter().map(|style| (style.name.clone(), style.name.clone())).collect() }).collect()
}

/// The pick purpose of the style library files, which keeps their last folder.
const STYLE_FILES: &str = "styles";

/// The Styles files: Load… reads a style library (`.asl`); Save… writes `export`, offered while there are styles.
fn style_files(any: bool, export: Option<PresetExport>) -> PresetFiles<'static, PresetKey> {
    PresetFiles::new(STYLE_FILES, tl!("Style Library"), &["asl"]).export(move |_| export.clone()).can_export(move |_| any)
}

/// The Styles panel menu ("…"), drawn by PhotoCraft as the dock's menus still are: the rows, checks and actions of
/// `PresetPanelMenu`. Save… writes the selected style, else every style.
pub(crate) fn styles_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let any = app.session.presets.styles.iter().any(|group| !group.items.is_empty());
    let selected = app.ui.presets_ui.selected.get(STYLES.id).cloned().map(PresetKey::Item);
    let view = app.ui.presets_ui.browsers.get(STYLES.id).map_or(PresetView::Grid, |state| state.view);
    let files = style_files(any, None);
    let menu = PresetPanelMenu::new(view).new_group(true).files(&files).selected(selected.as_ref());
    let defs = PresetCommands::defs().unwrap_or_default();
    let mut picked = None;
    for row in menu.rows() {
        let PanelRow::Command(command) = row else {
            ui.separator();
            continue;
        };
        let Some(id) = PresetCommands::from_ref(&command) else { continue };
        let label = defs.iter().find(|def| def.command == command).map_or("", |def| def.label.as_str());
        let text = match menu.checked(id) {
            Some(on) => format!("{}{}", if on { "✔ " } else { "   " }, tl!(label)),
            None => tl!(label).to_string(),
        };
        if ui.add_enabled(menu.enabled(id), egui::Button::new(text)).clicked() {
            picked = Some(id);
            ui.close();
        }
    }
    let Some(command) = picked else { return };
    let action = match command {
        PresetCommands::Save => export_styles(app, selected.as_ref()).and_then(|(export, count)| {
            style_files(any, Some(export)).request(PresetCommands::Save, selected.as_ref()).map(|request| (PresetMenuAction::File(request), count))
        }),
        command => menu.run_command(command).map(|action| (action, 0)),
    };
    let Some((action, count)) = action else { return };
    let result = match action {
        PresetMenuAction::View(view) => {
            app.ui.presets_ui.browsers.entry(STYLES.id.to_string()).or_insert_with(PresetState::new).view = view;
            Ok(Value::Null)
        }
        PresetMenuAction::NewGroup => {
            let names = names_of(&style_folders(app));
            apply(app, ui.ctx(), STYLES, &names, &[], vec![PresetEvent::Footer(PresetFooterEvent::NewGroup)]);
            Ok(Value::Null)
        }
        PresetMenuAction::File(request) => style_file(app, request, count),
        _ => Ok(Value::Null),
    };
    if let Err(error) = result
        && error != "cancelled"
    {
        app.ui.status = error;
        app.ui.status_error = true;
    }
}

/// The `.asl` of the `selected` style, else of every style, through `style.presets.exportAsl`, suggested as the
/// style's name (or "Styles") without path-illegal characters; with the number of styles it holds.
fn export_styles(app: &mut PhotocraftApp, selected: Option<&PresetKey>) -> Option<(PresetExport, u64)> {
    let name = match selected {
        Some(PresetKey::Item(name)) => Some(name.as_str()),
        _ => None,
    };
    let reply = run(app, "style.presets.exportAsl", name.map_or_else(|| json!({}), |name| json!({"styles": [name]})))?;
    let bytes = reply["data"].as_str().and_then(photocraft_engine::paint::tile::b64_decode)?;
    let stem: String = name.unwrap_or("Styles").chars().filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')).collect();
    let stem = if stem.trim().is_empty() { "Styles" } else { stem.trim() };
    Some((PresetExport::new(format!("{stem}.asl"), bytes), reply["count"].as_u64().unwrap_or(0)))
}

/// Runs a Styles file request: Load… imports the picked style library, Save… writes the export (`count` styles) where
/// the user picks.
fn style_file(app: &mut PhotocraftApp, request: PresetFileRequest, count: u64) -> Result<Value, String> {
    match request {
        PresetFileRequest::Open { extensions, .. } => {
            let extensions: Vec<&str> = extensions.iter().map(String::as_str).collect();
            app.pick_file_bytes_filtered(&extensions, |app, name, bytes| match crate::preset_files_ui::open(app, &name, &bytes) {
                Some(result) => result.map(|()| Value::Null),
                None => Err(crate::i18n::fmt(tl!("{file} is not a style library (.asl)"), &[("file", &crate::file_open::display_name(&name))])),
            })
        }
        PresetFileRequest::Save { file_name, bytes, .. } => app.pick_save(&file_name, move |app, path| {
            let write = app.services.write.as_mut().ok_or("no writer configured")?;
            write(&path, &bytes)?;
            app.ui.status_error = false;
            app.ui.status = crate::i18n::fmt(
                &crate::i18n::trn(crate::i18n::current(), count, "Exported {n} style to {file}", "Exported {n} styles to {file}"),
                &[("file", &crate::file_open::display_name(&path))],
            );
            Ok(json!({"path": path, "count": count}))
        }),
        _ => Ok(Value::Null),
    }
}

fn names_of(folders: &[Folder]) -> Vec<String> {
    folders.iter().map(|folder| folder.name.clone()).collect()
}

// ------------------------------------------------------------------ events

/// What an event does: an engine command, or a change of the panel's folder order (app UI state, no command).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Act {
    Run(String, Value),
    MoveFolder { from: String, before: Option<String> },
}

/// The command `panel` runs for `event`, `None` for the events the browser keeps (open state) or that change nothing.
/// `rows` is the browser's rows in display order, `selected` the panel's selected preset. Shared by every preset panel;
/// a panel's own Select, Activate and DropAt come first in `panel_act`.
pub(crate) fn act(panel: PresetPanel, event: &PresetEvent<PresetKey>, rows: &[PresetKey], selected: Option<&str>) -> Option<Act> {
    let edit = format!("{}.edit", panel.prefix);
    let run = |params: Value| Some(Act::Run(edit.clone(), params));
    match event {
        PresetEvent::Rename { key: PresetKey::Item(key), text, .. } => run(json!({"action": "rename", "preset": key, "name": text})),
        PresetEvent::Rename { key: PresetKey::Group(group), text, .. } => run(json!({"action": "renameGroup", "group": group, "name": text})),
        PresetEvent::Delete(PresetKey::Item(key)) => run(json!({"action": "delete", "preset": key})),
        PresetEvent::Delete(PresetKey::Group(group)) => run(json!({"action": "deleteGroup", "group": group})),
        PresetEvent::MoveTo { key: PresetKey::Item(key), group: PresetKey::Group(group), .. } => run(json!({"action": "move", "preset": key, "to": group})),
        PresetEvent::Footer(PresetFooterEvent::NewGroup) => run(json!({"action": "newGroup", "name": "Group"})),
        PresetEvent::Footer(PresetFooterEvent::New) => Some(Act::Run(format!("{}.new", panel.prefix), (panel.new_params)())),
        PresetEvent::Footer(PresetFooterEvent::Delete) => selected.and_then(|key| run(json!({"action": "delete", "preset": key}))),
        PresetEvent::Reorder(edit) if matches!(edit.phase(), Some(craft::behavior::GesturePhase::Commit) | None) => {
            let mv = edit.value()?;
            // `to` counts the rows left once the moved ones, with a moved folder's presets, are taken out.
            let mut moving_folder = false;
            let remaining: Vec<&PresetKey> = rows
                .iter()
                .filter(|key| {
                    let moved = mv.keys.contains(key);
                    if matches!(key, PresetKey::Group(_)) {
                        moving_folder = moved;
                    }
                    !moved && !(moving_folder && matches!(key, PresetKey::Item(_)))
                })
                .collect();
            match &mv.key {
                PresetKey::Group(from) => {
                    // The next folder after the landing place, in the order without the moved one.
                    let before = remaining[mv.to.min(remaining.len())..].iter().find_map(|key| match key {
                        PresetKey::Group(name) => Some(name.clone()),
                        PresetKey::Item(_) => None,
                    });
                    Some(Act::MoveFolder { from: from.clone(), before })
                }
                PresetKey::Item(key) => {
                    let head = &remaining[..mv.to.min(remaining.len())];
                    let folder = head.iter().rposition(|key| matches!(key, PresetKey::Group(_)))?;
                    let PresetKey::Group(group) = head[folder] else { return None };
                    run(json!({"action": "move", "preset": key, "to": group, "index": head.len() - folder - 1}))
                }
            }
        }
        _ => None,
    }
}

/// `panel`'s own handling of the events that use a preset: Patterns select and fill, Styles apply.
fn panel_act(panel: PresetPanel, event: &PresetEvent<PresetKey>) -> Option<Act> {
    let (key, add) = match event {
        PresetEvent::Select { key: PresetKey::Item(key), mods, .. } | PresetEvent::Activate { key: PresetKey::Item(key), mods, .. } => (key, mods.shift),
        PresetEvent::DropAt { key: PresetKey::Item(key), .. } => (key, false),
        _ => return None,
    };
    let select = matches!(event, PresetEvent::Select { .. });
    Some(match panel.id {
        "patterns" if select => Act::Run("pattern.presets.select".into(), json!({"pattern": key})),
        "patterns" => Act::Run("pattern.presets.apply".into(), json!({"pattern": key})),
        // A drop styles the active layer; a click the selected layers, Shift adding the effects.
        _ if matches!(event, PresetEvent::DropAt { .. }) => Act::Run("style.presets.apply".into(), json!({"preset": key})),
        _ => Act::Run("style.presets.apply".into(), json!({"preset": key, "add": add})),
    })
}

/// Runs `events` for `panel`, in order, and keeps the panel's selection and folder order. `rows` are the keys of the
/// rows the browser showed, which the events' positions count in.
fn apply(app: &mut PhotocraftApp, ctx: &egui::Context, panel: PresetPanel, names: &[String], rows: &[PresetKey], events: Vec<PresetEvent<PresetKey>>) {
    for event in events {
        let selected = app.ui.presets_ui.selected.get(panel.id).cloned();
        let Some(act) = panel_act(panel, &event).or_else(|| act(panel, &event, rows, selected.as_deref())) else { continue };
        if let PresetEvent::Select { key: PresetKey::Item(key), .. }
        | PresetEvent::Activate { key: PresetKey::Item(key), .. }
        | PresetEvent::DropAt { key: PresetKey::Item(key), .. } = &event
        {
            app.ui.presets_ui.selected.insert(panel.id.into(), key.clone());
        }
        match act {
            Act::MoveFolder { from, before } => app.ui.presets_ui.move_group(panel.id, names, &from, before.as_deref()),
            Act::Run(command, params) => {
                let Some(reply) = run(app, &command, params) else { continue };
                after(app, ctx, panel, &event, &reply);
            }
        }
    }
}

/// What a command's reply changes in the panel: the selection follows a rename, a delete clears it, a new group and a
/// new preset open their rename.
fn after(app: &mut PhotocraftApp, ctx: &egui::Context, panel: PresetPanel, event: &PresetEvent<PresetKey>, reply: &Value) {
    let ui = &mut app.ui.presets_ui;
    match event {
        PresetEvent::Rename { key: PresetKey::Item(key), text, .. } if ui.selected.get(panel.id) == Some(key) => {
            ui.selected.insert(panel.id.into(), text.clone());
        }
        PresetEvent::Delete(PresetKey::Item(_)) | PresetEvent::Footer(PresetFooterEvent::Delete) => {
            ui.selected.remove(panel.id);
        }
        PresetEvent::Footer(PresetFooterEvent::NewGroup) => {
            let group = reply["group"].as_str().unwrap_or_default().to_string();
            PresetBrowser::request_rename(ctx, browser_id(panel), &PresetKey::Group(group));
        }
        PresetEvent::Footer(PresetFooterEvent::New) => {
            let name = reply.get("name").or(reply.get("pattern")).and_then(Value::as_str).unwrap_or_default().to_string();
            ui.selected.insert(panel.id.into(), name.clone());
            if panel.rename_new {
                PresetBrowser::request_rename(ctx, browser_id(panel), &PresetKey::Item(name));
            }
        }
        _ => {}
    }
}

// ------------------------------------------------------------------ pattern picker

/// Photoshop's pattern picker: a swatch of the library pattern `selected` (an id) that opens the library's swatches,
/// each named in its tooltip. Returns the id of a newly picked pattern.
pub(crate) fn pattern_picker(app: &PhotocraftApp, ui: &mut egui::Ui, selected: &str) -> Option<String> {
    let ctx = ui.ctx().clone();
    let rows: Vec<PresetRow<String>> = app
        .session
        .patterns
        .items
        .iter()
        .map(|pattern| {
            let texture = TextureCache::texture(&ctx, ("pattern", pattern.id.clone()), || pattern_image(pattern));
            let name = pattern.display_name().to_string();
            PresetRow::item(pattern.id.clone(), name.clone(), 0).tooltip(name).preview(ThumbAspect::SQUARE, move |cx, rect| {
                cx.picture(texture, rect, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), 0.0);
            })
        })
        .collect();
    let mut value = Some(selected.to_string()).filter(|id| app.session.patterns.items.iter().any(|pattern| &pattern.id == id));
    let before = value.clone();
    let _ = PresetPicker::new(tl!("Patterns"), Bind::bound(&mut value), &rows, Bind::given(PresetState::new())).show(ui);
    value.filter(|picked| before.as_ref() != Some(picked))
}

#[cfg(test)]
mod tests;
