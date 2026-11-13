//! The Patterns and Styles panels and the pattern picker on the framework's `PresetBrowser` and `PresetPicker`. Rows
//! come from a panel's groups in the panel's folder order; the browser's events run as the panel's engine commands,
//! through one mapping for every preset panel (`events`).

use craft::ui::contract::{Bind, DropZone, Show as _};
use craft::ui::list::ThumbAspect;
use craft::ui::patterns::presets::{
    DefaultOpen, PresetBrowser, PresetEvent, PresetFooterEvent, PresetFooterOptions, PresetPicker, PresetReorder, PresetRow, PresetState,
};
use craft::ui::primitives::ContentPainterCx;
use craft::ui::primitives::textures::TextureCache;
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

pub(crate) const PATTERNS: PresetPanel = PresetPanel { id: "patterns", label: "Patterns", prefix: "pattern.presets", new_params: || json!({}), rename_new: false };
pub(crate) const STYLES: PresetPanel = PresetPanel { id: "styles", label: "Styles", prefix: "style.presets", new_params: || json!({"name": "Style"}), rename_new: true };

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

/// Where preset drops land: the canvas proper, rulers excluded, on the canvas's layer.
fn canvas_zone(app: &PhotocraftApp) -> DropZone {
    DropZone::new(app.last_canvas_layer, canvas_drop_rect(app))
}

/// Shows `panel`'s browser over `folders` with `thumb` painting a preset's thumbnail, and returns its events with the
/// keys of the rows it showed, in order. `presets` is the panel state, `zone` the canvas.
fn show(
    presets: &mut PresetUi,
    zone: DropZone,
    ui: &mut egui::Ui,
    panel: PresetPanel,
    folders: &[Folder],
    thumb: &dyn Fn(&ContentPainterCx, Rect, &str),
) -> (Vec<PresetEvent<PresetKey>>, Vec<PresetKey>) {
    let order = presets.display_order(panel.id, &names_of(folders));
    let rows = rows(folders, &order);
    let selected = presets.selected.get(panel.id).cloned().map(PresetKey::Item);
    let mut state = presets.browsers.get(panel.id).cloned().unwrap_or_default();
    let mut browser = PresetBrowser::new(tl!(panel.label), Bind::bound(&mut state), &rows);
    if let Some(key) = selected.clone() {
        browser = browser.selected(key);
    }
    let out = browser
        .browser_id(browser_id(panel))
        .default_open(DefaultOpen::First)
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
    let (events, rows) = show(&mut app.ui.presets_ui, zone, ui, PATTERNS, &folders, &thumb);
    apply(app, ui.ctx(), PATTERNS, &names_of(&folders), &rows, events);
}

// ------------------------------------------------------------------ Styles

/// The height of the Styles browser, footer included.
const STYLES_HEIGHT: f32 = 300.0;

/// Window › Styles: a click applies the style to the selected layers (Shift adds its effects), a drop on the canvas
/// applies it to the active layer.
pub fn styles_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let groups = app.session.presets.styles.clone();
    let folders: Vec<Folder> = groups
        .iter()
        .map(|group| Folder { name: group.name.clone(), items: group.items.iter().map(|style| (style.name.clone(), style.name.clone())).collect() })
        .collect();
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
    // The floating window sizes to its content: the browser fills a room of the height Styles' list had.
    let room = egui::vec2(ui.available_width(), STYLES_HEIGHT);
    let (events, rows) = ui.allocate_ui(room, |ui| {
        ui.set_min_height(STYLES_HEIGHT);
        show(&mut app.ui.presets_ui, zone, ui, STYLES, &folders, &thumb)
    }).inner;
    apply(app, ui.ctx(), STYLES, &names_of(&folders), &rows, events);
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
        if let PresetEvent::Select { key: PresetKey::Item(key), .. } | PresetEvent::Activate { key: PresetKey::Item(key), .. } | PresetEvent::DropAt { key: PresetKey::Item(key), .. } =
            &event
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
