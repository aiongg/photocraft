//! The Channels panel (tab of the Layers card): composite, colour, alpha / spot and Quick Mask
//! rows with thumbnails and eye toggles. Every action is an engine command (`channel.*`,
//! `select.loadSelection`, `select.saveSelection`, …); the targeted channel and the eyes live in
//! the engine's `DocState::channel_view`, so agents see the same state through `inspect`.
//! Rows can be dragged onto the footer's buttons, as in Photoshop (#4689): New duplicates the
//! channel, Delete deletes it, and Load Channel as Selection loads it.

use std::cell::Cell;
use std::ops::Range;

use craft_ui::behavior::{Axis, DropPosition, ListDrag, Reorder, RowInfo};
use craft_ui::composites::PanelFooter;
use craft_ui::edit::EditEvent;
use craft_ui::icons::Icon;
use craft_ui::list::{DropMark, ListRow, RowEvent, ThumbSize, ToggleColumn, paint_drop_mark};
use craft_ui::menu::{ContextMenu, MenuEntry, MenuItem};
use craft_ui::theme::{Tokens as LibTokens, TokensOf as _};
use egui::{Align2, Color32, Rect, pos2, vec2};
use photocraft_doc::ColorIndicates;
use photocraft_engine::channel_cmds::{self, ChannelTarget};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::theme::{self, Tokens};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Row {
    Composite,
    Color(usize),
    Alpha(usize),
    QuickMask,
    /// The selected layer's mask: a temporary channel, listed while the layer is selected.
    LayerMask,
}

impl Row {
    /// The channel reference the engine commands take.
    fn reference(self) -> Value {
        match self {
            Row::Composite => json!("composite"),
            Row::Color(k) => json!({ "color": k }),
            Row::Alpha(i) => json!(i),
            Row::QuickMask => json!("quickMask"),
            Row::LayerMask => json!("mask"),
        }
    }
}

/// The drag of a Channels row (egui's `DragAndDrop` payload), so the footer buttons take drops
/// from it.
type RowDrag = ListDrag<Row>;

/// ⌘ = new selection, ⌘⇧ add, ⌘⌥ subtract, ⌘⌥⇧ intersect (Photoshop's thumbnail ⌘-clicks).
pub fn load_operation(m: egui::Modifiers) -> &'static str {
    match (m.shift, m.alt) {
        (true, true) => "intersect",
        (true, false) => "add",
        (false, true) => "subtract",
        _ => "new",
    }
}

struct RowPresentationContext<'a> {
    doc: &'a photocraft_doc::Document,
    view: &'a photocraft_engine::channel_cmds::ChannelView,
    mode: photocraft_doc::ColorMode,
    colors: usize,
    gray_view: bool,
    quick: bool,
    mask_targeted: bool,
    masked: Option<&'a photocraft_doc::Layer>,
    shown_colors: usize,
}

struct RowPresentation {
    name: String,
    thumb_idx: usize,
    slot: Option<usize>,
    visible: bool,
    selected: bool,
}

/// What a row's context menu runs: an engine or `ui.*` command with its parameters.
type Cmd = (&'static str, Value);

/// The key of the open rename of an alpha channel's row.
fn rename_key(doc: u64, row: Row) -> egui::Id {
    egui::Id::unique(("chan-rename", doc, format!("{:?}", row.reference())))
}

fn list_id() -> egui::Id {
    egui::Id::unique("pc-channels-list")
}

/// Everything a row needs from the panel.
struct RowEnv<'a> {
    doc: &'a photocraft_doc::Document,
    masked: Option<&'a photocraft_doc::Layer>,
    thumbs: &'a [egui::TextureId],
    mask_tex: Option<egui::TextureId>,
    gray_view: bool,
    order: &'a [RowInfo<Row>],
}

/// What the rows report back to the panel.
#[derive(Default)]
struct RowsOut {
    actions: Vec<(String, Value)>,
    mask_click: Option<bool>,
    /// (name, thumbnail rect) of each row drawn.
    drawn: Vec<(String, Rect)>,
    /// The alpha channels' rows, for the reorder drop.
    alphas: Vec<(Row, Rect)>,
}

fn entry(label: &str, command: &'static str, params: Value) -> MenuEntry<Cmd> {
    MenuItem::new(tl!(&label), (command, params)).into()
}

/// Channel Options › Color Indicates.
fn indicates_items(channel: Value, current: ColorIndicates) -> Vec<MenuEntry<Cmd>> {
    let masked = current == ColorIndicates::MaskedAreas;
    let item = |label: &str, on: bool, indicates: &str| {
        MenuItem::new(tl!(&label), ("channel.options", json!({ "channel": channel.clone(), "indicates": indicates }))).check(on).into()
    };
    vec![item("Color Indicates Masked Areas", masked, "masked"), item("Color Indicates Selected Areas", !masked, "selected")]
}

/// Overlay colour presets (Channel Options › Color) and opacity.
fn overlay_colors(channel: &Value) -> Vec<MenuEntry<Cmd>> {
    let colors = [
        (tl!("Red"), "#ff0000"),
        (tl!("Green"), "#00c000"),
        (tl!("Blue"), "#0050ff"),
        (tl!("Cyan"), "#00d0e0"),
        (tl!("Magenta"), "#e000c0"),
        (tl!("Yellow"), "#f0d000"),
    ]
    .into_iter()
    .map(|(label, hex)| MenuItem::new(tl!(&label), ("channel.options", json!({ "channel": channel.clone(), "color": hex }))).into())
    .collect();
    let opacities = [25, 50, 75, 100]
        .into_iter()
        .map(|o| MenuItem::new(format!("{o}%"), ("channel.options", json!({ "channel": channel.clone(), "opacity": o }))).into())
        .collect();
    vec![MenuItem::submenu(tl!("Overlay Color"), colors).into(), MenuItem::submenu(tl!("Overlay Opacity"), opacities).into()]
}

fn menu_rows(row: Row, doc: &photocraft_doc::Document, masked: Option<&photocraft_doc::Layer>) -> Vec<MenuEntry<Cmd>> {
    let mut rows = match row {
        Row::Alpha(index) => alpha_rows(index, &doc.channels[index]),
        Row::QuickMask => {
            let mut rows = vec![
                entry("Exit Quick Mask", "select.editInQuickMaskMode", json!({ "on": false })),
                entry("Quick Mask Options…", "ui.channelOptions", json!("quickMask")),
            ];
            rows.extend(indicates_items(json!("quickMask"), doc.quick_mask.as_ref().map(|mask| mask.indicates).unwrap_or_default()));
            rows.extend(overlay_colors(&json!("quickMask")));
            rows
        }
        Row::Composite | Row::Color(_) => vec![entry("Duplicate Channel", "channel.duplicate", json!({ "channel": row.reference() }))],
        Row::LayerMask => {
            let enabled = masked.and_then(|layer| layer.mask.as_ref()).is_none_or(|mask| mask.enabled);
            vec![
                entry(crate::layer_menu_ui::mask_toggle_label(enabled), "layer.layerMask.enabled", json!({})),
                entry("Delete Layer Mask", "layer.layerMask.delete", json!({})),
                entry("Layer Mask Display Options…", "ui.channelOptions", json!("mask")),
            ]
        }
    };
    rows.extend([
        MenuEntry::Separator,
        entry("New Channel…", "channel.new", json!({})),
        entry("New Spot Channel…", "ui.newSpotChannel", json!({})),
        MenuEntry::Separator,
        entry("Split Channels", "channel.split", json!({})),
        entry("Merge Channels…", "channel.merge", json!({})),
    ]);
    rows
}

fn alpha_rows(index: usize, channel: &photocraft_doc::AlphaChannel) -> Vec<MenuEntry<Cmd>> {
    let mut rows = vec![
        entry("Duplicate Channel", "channel.duplicate", json!({ "channel": index })),
        entry("Delete Channel", "channel.delete", json!({ "channel": index })),
        entry("Rename Channel…", "ui.renameChannel", json!(index)),
        entry("Channel Options…", "ui.channelOptions", json!(index)),
        MenuEntry::Separator,
    ];
    if channel.spot.is_some() {
        rows.push(entry("Merge Spot Channel", "channel.mergeSpot", json!({ "channel": index })));
        rows.push(entry("Convert to Alpha Channel", "channel.options", json!({ "channel": index, "indicates": "masked" })));
    } else {
        rows.extend(indicates_items(json!(index), channel.indicates));
        rows.extend(overlay_colors(&json!(index)));
    }
    rows
}

/// What clicking `row`'s eye to the state `on` asks for.
fn eye_action(row: Row, on: bool, gray_view: bool, layer_id: Option<u64>) -> (String, Value) {
    let view_cmd = photocraft_engine::mask_view_cmds::ID.to_string();
    match row {
        // The mask's eye: the overlay on, or the mask view off.
        Row::LayerMask => (view_cmd, json!({ "layer": layer_id, "mode": if on { "overlay" } else { "off" } })),
        // Showing the composite again over a gray mask view keeps the mask as an overlay.
        Row::Composite if gray_view && on => (view_cmd, json!({ "layer": layer_id, "mode": "overlay" })),
        _ => ("channel.setVisible".into(), json!({ "channel": row.reference(), "visible": on })),
    }
}

/// Spot channels: a swatch of the ink on the thumbnail corner.
fn paint_spot_swatch(painter: &egui::Painter, thumb: Rect, spot: Option<(photocraft_doc::Color, f32)>) {
    let Some((ink, _)) = spot else { return };
    let c = ink.to_rgb().map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8);
    painter.rect_filled(
        Rect::from_min_size(thumb.right_bottom() - vec2(9.0, 9.0), vec2(9.0, 9.0)),
        craft_ui::theme::scale::Radius::R0.px(),
        Color32::from_rgb(c[0], c[1], c[2]),
    );
}

fn row_presentation(row: Row, context: &RowPresentationContext<'_>, mask_visible: bool) -> RowPresentation {
    let (name, thumb_idx, slot, visible, selected) = match row {
        Row::Composite => (
            channel_cmds::composite_name(context.mode).to_string(),
            0,
            Some(2),
            context.view.visible_colors(context.colors) == context.colors && !context.gray_view,
            context.view.target == ChannelTarget::Composite && !context.quick && !context.mask_targeted,
        ),
        Row::Color(k) => (
            channel_cmds::color_names(context.mode)[k].to_string(),
            1 + k,
            Some(3 + k),
            context.view.color_visible(k) && !context.gray_view,
            (context.view.target == ChannelTarget::Composite && !context.quick && !context.mask_targeted) || context.view.target == ChannelTarget::Color(k),
        ),
        Row::Alpha(i) => (
            context.doc.channels[i].name.clone(),
            1 + context.shown_colors + i,
            Some(3 + context.shown_colors + i).filter(|s| *s <= 9),
            context.view.alpha_shown(i),
            context.view.target == ChannelTarget::Alpha(i),
        ),
        Row::QuickMask => (
            "Quick Mask".to_string(),
            1 + context.shown_colors + context.doc.channels.len(),
            None,
            !context.view.quick_mask_hidden,
            context.view.target == ChannelTarget::Composite,
        ),
        Row::LayerMask => {
            (context.masked.map_or_else(String::new, |layer| format!("{} Mask", layer.name)), usize::MAX, None, mask_visible, context.mask_targeted)
        }
    };
    RowPresentation { name, thumb_idx, slot, visible, selected }
}

fn show_row(ui: &mut egui::Ui, env: &RowEnv<'_>, row: Row, presentation: RowPresentation, out: &mut RowsOut) {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    let RowPresentation { name, thumb_idx, slot, visible, selected } = presentation;
    let rename_id = rename_key(env.doc.id.0, row);
    let renaming = matches!(row, Row::Alpha(_)) && ui.data(|d| d.get_temp::<bool>(rename_id)).unwrap_or(false);
    // Built-in labels only: alpha/spot names and "{layer} Mask" stay verbatim.
    let label = match row {
        Row::Composite | Row::Color(_) | Row::QuickMask => tl!(&name).to_owned(),
        Row::Alpha(_) | Row::LayerMask => name.clone(),
    };
    let spot = match row {
        Row::Alpha(i) => env.doc.channels[i].spot,
        _ => None,
    };
    let italic = matches!(row, Row::QuickMask | Row::LayerMask) || spot.is_some();
    let tex = if row == Row::LayerMask { env.mask_tex } else { env.thumbs.get(thumb_idx).copied() };
    let (w, h) = (env.doc.size.width, env.doc.size.height);
    let thumb_rect = Cell::new(None::<Rect>);
    let mut list_row = ListRow::new(row, label.as_str())
        .toggle(ToggleColumn::eye(tl!("Visibility {row}"), visible))
        .selected(selected)
        .layer(true)
        .italic(italic)
        .renaming(renaming)
        .thumb(ThumbSize::Small, |painter, cell| {
            // Thumbnails keep the document's aspect ratio (the textures are letterboxed squares).
            let (thumb, uv) = fit_thumb(cell, w, h);
            if let Some(tex) = tex {
                painter.image(tex, thumb, uv, Color32::WHITE);
            }
            paint_spot_swatch(painter, thumb, spot);
            thumb_rect.set(Some(thumb));
        });
    if let Some(slot) = slot {
        let hint = crate::shortcuts::pretty(&format!("Cmd+{slot}"));
        let width = ui.painter().layout_no_wrap(hint.clone(), theme::mono(11.0), t.text_faint).size().x;
        list_row = list_row.optional_item(width, move |ui| {
            ui.painter().text(ui.max_rect().right_center(), Align2::RIGHT_CENTER, hint, theme::mono(11.0), t.text_faint);
        });
    }
    let shown = list_row.show(ui);
    let resp = shown.response.clone();
    if let Some(eye) = shown.toggles.first() {
        eye.clone().on_hover_text(if row == Row::LayerMask { tl!("Show the layer mask as an overlay") } else { tl!("Toggle visibility") });
    }
    let layer_id = env.masked.map(|l| l.id.0);
    for event in &shown.events {
        match event {
            RowEvent::Toggle(EditEvent::Update { value, .. } | EditEvent::Set { value, .. }) => {
                out.actions.push(eye_action(row, value.on, env.gray_view, layer_id));
            }
            RowEvent::Renamed { text, .. } => {
                if let Row::Alpha(index) = row
                    && !text.trim().is_empty()
                    && text.trim() != name
                {
                    out.actions.push(("channel.rename".into(), json!({ "channel": index, "name": text.trim() })));
                }
            }
            RowEvent::RenameEnded { .. } => ctx.data_mut(|d| d.remove::<bool>(rename_id)),
            _ => {}
        }
    }
    let thumb = thumb_rect.get().unwrap_or(Rect::from_min_size(shown.label.left_center(), vec2(0.0, 0.0)));
    out.drawn.push((name.clone(), thumb));
    if matches!(row, Row::Alpha(_)) {
        out.alphas.push((row, resp.rect));
    }
    if resp.drag_started() {
        egui::DragAndDrop::set_payload(&ctx, RowDrag::new(list_id(), env.order, &[], row, label.clone()));
    }
    click_row(ui, env, row, &resp, &mut *out);
    double_click_row(row, &resp, shown.label, rename_id, &ctx, &mut *out);
    let entries = menu_rows(row, env.doc, env.masked);
    if let Some((command, params)) = ContextMenu::new(&entries).show(ui, &resp).chosen {
        out.actions.push((command.into(), params));
    }
}

fn click_row(ui: &egui::Ui, env: &RowEnv<'_>, row: Row, resp: &egui::Response, out: &mut RowsOut) {
    if !resp.clicked() {
        return;
    }
    let modifiers = ui.input(|input| input.modifiers);
    if modifiers.command {
        let operation = load_operation(modifiers);
        let params = if row == Row::LayerMask {
            json!({ "channel": "mask", "layer": env.masked.map(|layer| layer.id.0), "operation": operation })
        } else {
            json!({ "channel": row.reference(), "operation": operation })
        };
        out.actions.push(("select.loadSelection".into(), params));
        return;
    }
    if row == Row::LayerMask {
        out.mask_click = Some(true);
        out.actions.push(("channel.target".into(), json!({ "channel": "mask" })));
        return;
    }
    if matches!(row, Row::Composite | Row::Color(_)) {
        out.mask_click = Some(false);
    }
    let target = if row == Row::QuickMask { json!("composite") } else { row.reference() };
    out.actions.push(("channel.target".into(), json!({ "channel": target })));
}

/// Like Photoshop: double-clicking the thumbnail opens Channel Options (where an alpha channel
/// becomes a spot channel, #3761), double-clicking the name renames it.
fn double_click_row(row: Row, resp: &egui::Response, label: Rect, rename_id: egui::Id, ctx: &egui::Context, out: &mut RowsOut) {
    if !resp.double_clicked() {
        return;
    }
    match row {
        Row::QuickMask => out.actions.push(("ui.channelOptions".into(), json!("quickMask"))),
        Row::LayerMask => out.actions.push(("ui.channelOptions".into(), json!("mask"))),
        Row::Alpha(index) => {
            if resp.interact_pointer_pos().is_some_and(|p| label.contains(p)) {
                ctx.data_mut(|d| d.insert_temp(rename_id, true));
            } else {
                out.actions.push(("ui.channelOptions".into(), json!(index)));
            }
        }
        Row::Composite | Row::Color(_) => {}
    }
}

/// The alpha or spot channel dragged over the rows reorders like Photoshop's (#9163): a mark
/// shows where it lands, above the colour channels it lands first, and colour channels don't move.
fn reorder_drop(ui: &egui::Ui, drag: &RowDrag, alphas: &[(Row, Rect)], actions: &mut Vec<(String, Value)>) {
    let Some(&Row::Alpha(from)) = drag.items.first() else { return };
    let ctx = ui.ctx();
    let Some(p) = ctx.input(|i| i.pointer.interact_pos()).filter(|p| ui.clip_rect().contains(*p)) else { return };
    let rows: Vec<RowInfo<Row>> = alphas.iter().map(|(r, _)| RowInfo { key: *r, depth: 0, has_children: false, open: false, section: false }).collect();
    let spans: Vec<Range<f32>> = alphas.iter().map(|(_, r)| r.top()..r.bottom()).collect();
    let Some(resolved) = Reorder::new(Axis::Vertical).resolve(&rows[..], &spans[..], p.y, &[Row::Alpha(from)], ctx.input(|i| i.modifiers)) else { return };
    let Row::Alpha(target) = resolved.mv.target else { return };
    let (slot, mark) = match resolved.mv.position {
        DropPosition::Before => (target, DropMark::Before),
        _ => (target + 1, DropMark::After),
    };
    paint_drop_mark(ui, alphas[resolved.at].1, mark);
    // `channel.move` takes the channel's final index: a slot below it is one less.
    let to = if slot > from { slot - 1 } else { slot };
    if ctx.input(|i| i.pointer.any_released()) && to != from {
        actions.push(("channel.move".into(), json!({ "channel": from, "to": to })));
    }
}


pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let Some(st) = app.session.active() else {
        ui.weak(tl!("No document"));
        return;
    };
    let doc = st.doc.clone();
    let view = st.channel_view.clone();
    // ⌥-click mask view (#196): gray shows the mask alone (colour eyes off), overlay over the composite.
    let mask_view = photocraft_engine::mask_view_cmds::current(st).map(|v| v.mode);
    let gray_view = mask_view == Some(photocraft_engine::mask_view_cmds::MaskViewMode::Gray);
    let active_layer = st.active_layer;
    let mode = doc.pixel_format().mode;
    let colors = mode.color_channels();
    let quick = doc.quick_mask.is_some();
    let ctx = ui.ctx().clone();
    let thumbs = app.channel_thumbs(&ctx);
    // The active layer's mask (Photoshop lists it, in italics, below the colour channels).
    let masked = active_layer.and_then(|id| doc.layer(id)).filter(|l| l.mask.is_some());
    let mask_tex = masked.as_ref().and_then(|l| Some(app.mask_thumb(&ctx, &doc, l.id, l.mask.as_ref()?)));
    let mask_targeted = masked.is_some()
        && (app.ui.mask_target || view.target == ChannelTarget::LayerMask)
        && (view.target == ChannelTarget::Composite || view.target == ChannelTarget::LayerMask)
        && !quick;
    // Multichannel images are their ink channels only (no composite or colour rows).
    let multichannel = doc.mode == photocraft_doc::ColorMode::Multichannel;
    let mut rows = if multichannel { Vec::new() } else { vec![Row::Composite] };
    if colors > 1 && !multichannel {
        rows.extend((0..colors).map(Row::Color));
    }
    if masked.is_some() {
        rows.push(Row::LayerMask);
    }
    rows.extend((0..doc.channels.len()).map(Row::Alpha));
    if quick {
        rows.push(Row::QuickMask);
    }
    let shown_colors = if colors > 1 { colors } else { 0 };
    let order: Vec<RowInfo<Row>> = rows.iter().map(|&key| RowInfo { key, depth: 0, has_children: false, open: false, section: false }).collect();
    // The buttons sit in a footer at the panel's bottom, like Photoshop's.
    let tokens = LibTokens::of(ui);
    let footer = PanelFooter::reserved_height(&PanelFooter::style(&tokens, ui.pixels_per_point())) + ui.spacing().item_spacing.y;
    let fill = ui.available_height() > footer + 60.0;
    let rows_h = if fill { ui.available_height() - footer } else { f32::INFINITY };
    let row_context = RowPresentationContext { doc: &doc, view: &view, mode, colors, gray_view, quick, mask_targeted, masked, shown_colors };
    let env = RowEnv { doc: &doc, masked, thumbs: &thumbs, mask_tex, gray_view, order: &order };
    let mut out = RowsOut::default();
    egui::ScrollArea::vertical().max_height(rows_h).min_scrolled_height(if fill { rows_h } else { 0.0 }).auto_shrink([false, !fill]).show(ui, |ui| {
        for row in rows {
            show_row(ui, &env, row, row_presentation(row, &row_context, mask_view.is_some()), &mut out);
        }
        if let Some(drag) = egui::DragAndDrop::payload::<RowDrag>(&ctx) {
            reorder_drop(ui, &drag, &out.alphas, &mut out.actions);
        }
    });
    let RowsOut { mut actions, mask_click, drawn, .. } = out;
    // A row being dragged: its name follows the pointer until it is dropped.
    let dragged = egui::DragAndDrop::payload::<RowDrag>(&ctx);
    if let Some(drag) = &dragged
        && let Some(p) = ctx.input(|i| i.pointer.interact_pos())
    {
        crate::layer_transfer::show_ghost(ui, p, &drag.label);
    }
    let mask_layer = masked.as_ref().map(|l| l.id.0);
    let drop = |ui: &egui::Ui, button: &egui::Response, on: Footer| {
        let drag = dragged.as_ref()?;
        let row = *drag.items.first()?;
        let action = drop_action(row, &drag.label, mask_layer, on)?;
        crate::panels::dropped_on(ui, button).then_some(action)
    };
    // Photoshop's order from the left: Load as selection, Save selection, New, Delete.
    PanelFooter::show(ui, |ui| {
        let target_ref = match view.target {
            ChannelTarget::Alpha(i) => json!(i),
            ChannelTarget::Color(k) => json!({ "color": k }),
            ChannelTarget::Composite => json!("composite"),
            ChannelTarget::LayerMask => json!("mask"),
            ChannelTarget::VectorMask => json!("vectorMask"),
        };
        let trash = PanelFooter::button(ui, Icon::Trash, tl!("Delete current channel"));
        if trash.clicked() {
            actions.push(("channel.delete".into(), json!({})));
        }
        actions.extend(drop(ui, &trash, Footer::Delete));
        let new = PanelFooter::button(ui, Icon::Plus, tl!("Create new channel"));
        if new.clicked() {
            actions.push(("channel.new".into(), json!({})));
        }
        actions.extend(drop(ui, &new, Footer::New));
        if PanelFooter::button(ui, Icon::MarqueeRect, tl!("Save selection as channel")).clicked() {
            actions.push(("select.saveSelection".into(), json!({})));
        }
        let load = PanelFooter::button(ui, Icon::MarqueeEllipse, tl!("Load channel as selection"));
        if load.clicked() {
            // The selected mask row can coexist with a Composite engine target while mask
            // painting is active. The footer follows the row the user sees as targeted.
            let params =
                if mask_targeted { json!({ "channel": "mask", "layer": masked.as_ref().map(|layer| layer.id.0) }) } else { json!({ "channel": target_ref }) };
            actions.push(("select.loadSelection".into(), params));
        }
        actions.extend(drop(ui, &load, Footer::Load));
    });
    ctx.data_mut(|d| d.insert_temp(thumbs_id(), drawn));
    for (id, p) in actions {
        if id == "ui.renameChannel" {
            if let Some(index) = p.as_u64().map(|i| i as usize).filter(|i| *i < doc.channels.len()) {
                ctx.data_mut(|d| d.insert_temp(rename_key(doc.id.0, Row::Alpha(index)), true));
            }
            continue;
        }
        if id == "ui.newSpotChannel" {
            crate::channel_options_ui::open_new_spot(app);
            continue;
        }
        if id == "ui.channelOptions" {
            crate::channel_options_ui::open(app, &p);
            continue;
        }
        let _ = app.run(&id, p);
    }
    if let Some(on) = mask_click {
        app.ui.mask_target = on;
        app.sync_mask_targets();
    }
}

/// The footer buttons a row can be dropped on.
#[derive(Clone, Copy)]
enum Footer {
    Delete,
    New,
    Load,
}

/// What dropping `row` (named `name`) on a footer button does, as in Photoshop; `None` where
/// Photoshop does nothing either (colour channels can't be deleted here). `mask_layer` is the
/// layer whose mask the temporary mask row shows.
fn drop_action(row: Row, name: &str, mask_layer: Option<u64>, on: Footer) -> Option<(String, Value)> {
    match (on, row) {
        (Footer::Delete, Row::Alpha(i)) => Some(("channel.delete".into(), json!({ "channel": i }))),
        // Asks "Apply mask to layer before removing?", like dropping the mask's Layers thumbnail.
        (Footer::Delete, Row::LayerMask) => Some(crate::mask_trash_prompt::drop_action(mask_layer?, crate::mask_thumbs_ui::MaskKind::Pixel)),
        (Footer::Delete, _) => None,
        // The engine would name a mask's copy after the composite; Photoshop says "Layer 1 Mask copy".
        (Footer::New, Row::LayerMask) => Some(("channel.duplicate".into(), json!({ "channel": "mask", "name": format!("{name} copy") }))),
        (Footer::New, _) => Some(("channel.duplicate".into(), json!({ "channel": row.reference() }))),
        (Footer::Load, Row::LayerMask) => Some(("select.loadSelection".into(), json!({ "channel": "mask", "layer": mask_layer? }))),
        (Footer::Load, _) => Some(("select.loadSelection".into(), json!({ "channel": row.reference() }))),
    }
}


fn thumbs_id() -> egui::Id {
    egui::Id::unique("channel-thumb-rects")
}

/// The Channels rows drawn last frame: (name, thumbnail rect).
pub fn recorded(ctx: &egui::Context) -> Vec<(String, Rect)> {
    ctx.data(|d| d.get_temp(thumbs_id())).unwrap_or_default()
}

/// The part of a square `cell` a `w`×`h` document fills, and the matching UVs of a square,
/// letterboxed thumbnail texture: thumbnails keep the document's aspect ratio.
pub fn fit_thumb(cell: Rect, w: u32, h: u32) -> (Rect, Rect) {
    let (w, h) = (w.max(1) as f32, h.max(1) as f32);
    let (fw, fh) = (w / w.max(h), h / w.max(h));
    let size = vec2(cell.width() * fw, cell.height() * fh).max(vec2(1.0, 1.0));
    let uv = Rect::from_center_size(pos2(0.5, 0.5), vec2(fw, fh));
    (Rect::from_center_size(cell.center(), size), uv)
}

#[cfg(test)]
mod tests;
