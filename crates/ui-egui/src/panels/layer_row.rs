use std::cell::{Cell, RefCell};

use craft_ui::edit::{EditEvent, GestureId};
use craft_ui::list::{OpenChange, ListRow, RowEvent, ThumbSize, ThumbTarget, ToggleColumn, ToggleEdit, TreeRow, TreeText};
use craft_ui::menu::{ContextMenu, OpenAt};
use craft_ui::theme::{StrokeExt, Tokens as LibTokens, ToEgui, TokensOf as _};

use super::layer_drag::{MaskDrag, fx_drop, layer_drag_and_drop, mask_drag_key, start_fx_drag, starts_mask_drag};
use super::layer_row_ui::{ICON_W, Indicator, TRIANGLE_W};
use super::layers::simple_lock_toggle;
use super::mask_thumbs_ui::MaskKind;
use super::{Color32, Layer, LayerContent, PhotocraftApp, Pos2, Rect, Sense, Tokens, Value, icons, json, pos2, theme, vec2, widgets};
use crate::state::LayerThumbnailSize;
use photocraft_color::BlendMode;

/// How a Layers panel row is selected: in the (multi-)selection, the primary/active layer, and
/// whether several layers are selected.
#[derive(Clone, Copy)]
pub(super) struct RowSel {
    pub(super) selected: bool,
    pub(super) primary: bool,
    pub(super) multi: bool,
}

/// The badge beside the pointer over a thumbnail ⌘-click loads with these modifiers: none for a
/// new selection, + for ⇧ (add), − for ⌥ (subtract), × for ⇧⌥ (intersect).
pub(crate) fn thumb_load_badge(m: egui::Modifiers) -> Option<crate::tool_feedback::Badge> {
    crate::tool_feedback::Badge::from_mode(crate::channels_panel::load_operation(m))
}

/// The cursor over a thumbnail ⌘-click loads as a selection: a pointing hand, with the selection
/// mode's badge (#7824).
fn thumb_load_cursor(ctx: &egui::Context, p: Pos2, m: egui::Modifiers) {
    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    if let Some(badge) = thumb_load_badge(m) {
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("thumb-load-badge")));
        crate::tool_feedback::draw_badge(&painter, p, badge, false);
    }
}

/// `layer.select` mode for a click with these modifiers (Photoshop: ⌘-click toggles, ⇧-click
/// selects a range, a plain click selects one layer).
fn select_mode(m: egui::Modifiers) -> &'static str {
    if m.command {
        "toggle"
    } else if m.shift {
        "range"
    } else {
        "replace"
    }
}

/// Marker in the clipped row's existing 12-point slot, beside its thumbnail.
pub(super) fn paint_clipping_indicator(painter: &egui::Painter, at: Pos2, color: Color32) {
    let tip = at + vec2(5.0, 3.0);
    let stroke = craft_ui::theme::scale::Stroke::W1.egui(painter.ctx(), color);
    painter.add(egui::Shape::line(vec![at + vec2(9.0, -4.0), at + vec2(5.0, -4.0), tip], stroke));
    painter.add(egui::Shape::line(vec![at + vec2(2.0, 0.0), tip, at + vec2(8.0, 0.0)], stroke));
}

/// Width of the clipping marker's slot beside the thumbnail.
const CLIP_W: f32 = 12.0;
/// Width of the link button before a mask thumbnail in the trailing slot.
const CHAIN_SLOT: f32 = 18.0;

/// What the row's closures hand back: where the library drew the parts the app paints into.
#[derive(Default)]
struct Cells {
    first: Cell<Option<Rect>>,
    second: Cell<Option<Rect>>,
    extra: Cell<Option<Rect>>,
    /// A trailing widget (effects triangle, lock, vector-mask chain) took a click this frame.
    clicked: Cell<bool>,
    indicators: RefCell<Vec<(Indicator, Rect)>>,
    queued: RefCell<Vec<(String, Value)>>,
}

/// The words of a group's disclosure triangle.
fn tree_text() -> TreeText {
    let alt = crate::shortcuts::pretty("Alt");
    let tip = |pattern: &str| crate::i18n::fmt(pattern, &[("key", &alt)]);
    TreeText {
        expand: tl!("Expand group {row}").to_owned(),
        collapse: tl!("Collapse group {row}").to_owned(),
        expand_tip: tip(tl!("Expand group  ({key}-click: all groups)")),
        collapse_tip: tip(tl!("Collapse group  ({key}-click: all groups)")),
    }
}

/// The thumbnail the row's editing is aimed at, and whether the library can bracket it.
#[derive(Clone, Copy, PartialEq)]
enum Target {
    Library(ThumbTarget),
    /// A vector mask in the trailing slot beside a layer mask: PhotoCraft brackets it.
    Extra,
}

fn target_of(app: &PhotocraftApp, l: &Layer, masks: &[(MaskKind, bool, bool)]) -> Option<Target> {
    if app.ui.filter_mask_target == Some(l.id.0) && app.filter_mask_targeted() {
        // The Smart Filters row's filter mask thumbnail carries the brackets.
        return None;
    }
    let slot = |kind: MaskKind| masks.iter().position(|m| m.0 == kind).map(|i| if i == 0 { Target::Library(ThumbTarget::Second) } else { Target::Extra });
    if app.ui.vector_mask_target
        && let Some(target) = slot(MaskKind::Vector)
    {
        return Some(target);
    }
    if app.ui.mask_target
        && let Some(target) = slot(MaskKind::Pixel)
    {
        return Some(target);
    }
    Some(Target::Library(ThumbTarget::First))
}

/// The drag and drop hooks that act on the row's response (layer, mask and effect drags).
fn drag_hooks(app: &mut PhotocraftApp, ctx: &egui::Context, ui: &egui::Ui, l: &Layer, rect: Rect, resp: &egui::Response, actions: &mut Vec<(String, Value)>) {
    // A drag from a mask's thumbnail drags that mask, not the layer (#3113, #3119).
    let mask_drag = starts_mask_drag(ctx, l, resp);
    if let Some(kind) = mask_drag {
        ctx.data_mut(|d| d.insert_temp::<MaskDrag>(mask_drag_key(), (l.id.0, kind)));
    }
    layer_drag_and_drop(app, ctx, ui, l, rect, resp, actions);
    fx_drop(ctx, ui, l, rect, actions);
    crate::smart_ui::drop_on_row(ctx, ui, l, rect, actions);
    if resp.drag_started() && mask_drag.is_none() {
        crate::layer_transfer::begin_from_panel(app, ctx, l.id);
    }
}

fn menu_open_key(layer: u64) -> egui::Id {
    egui::Id::unique(("layer-row-menu-open", layer))
}

/// The accessible name of a mask's link button.
fn link_name(kind: MaskKind, linked: bool, layer: &str) -> String {
    let (verb, what) = (if linked { "Unlink" } else { "Link" }, kind.label());
    let template = "{verb} {mask} {layer}";
    crate::i18n::fmt(tl!(template), &[("verb", tl!(verb)), ("mask", tl!(what)), ("layer", layer)])
}

/// The Layers panel's thumbnail setting as the library's.
fn thumb_size_of(size: LayerThumbnailSize) -> ThumbSize {
    match size {
        LayerThumbnailSize::None => ThumbSize::None,
        LayerThumbnailSize::Small => ThumbSize::Small,
        LayerThumbnailSize::Medium => ThumbSize::Medium,
        LayerThumbnailSize::Large => ThumbSize::Large,
    }
}

/// The trailing items of `l`'s row, outermost first: what is wrong, locks, effects and Smart
/// Filters, the blend mode. Each records its rect in `cells` and queues its commands there.
fn indicator_items<'a>(ui: &egui::Ui, ctx: &egui::Context, l: &'a Layer, doc: &'a photocraft_doc::Document, fx_open: bool, mut row: ListRow<'a, u64>, cells: &'a Cells) -> ListRow<'a, u64> {
    let t = craft_ui::theme::current(ctx);
    let (muted, faint, normal, warn) = (t.color.text.muted.to_egui(), t.color.text.faint.to_egui(), t.color.text.normal.to_egui(), t.color.status.warn.fg.to_egui());
    let has_fx = !l.effects.items.is_empty();
    let has_smart = crate::smart_ui::has_filters(l);
    let show_blend = l.blend != BlendMode::Normal && l.blend != BlendMode::PassThrough;
    let fx_galley = has_fx.then(|| ui.painter().layout_no_wrap("fx".into(), theme::semibold(11.0), muted));
    let blend_galley = show_blend.then(|| ui.painter().layout_no_wrap(tl!(l.blend.label()).into(), egui::FontId::proportional(10.5), faint));
    let icon_rect = |r: Rect| Rect::from_center_size(r.center(), vec2(ICON_W, ICON_W));
    let id = l.id.0;
    if let Some(key) = crate::layer_row_ui::unsupported_key(l) {
        row = row.trailing_item(ICON_W, move |ui| {
            let r = ui.max_rect();
            cells.indicators.borrow_mut().push((Indicator::Unsupported, r));
            icons::paint(ui, icon_rect(r), "triangle-alert", 12.0, warn);
            let tip = crate::i18n::fmt(tl!("Not rendered: this adjustment ({key}) is kept as imported."), &[("key", key)]);
            ui.interact(r, ui.scope_id().with(("row-unsupported", id)), Sense::hover()).on_hover_text(tip);
        });
    }
    if l.locks.any() {
        row = row.trailing_item(ICON_W, move |ui| {
            let r = ui.max_rect();
            cells.indicators.borrow_mut().push((Indicator::Lock, r));
            icons::paint(ui, icon_rect(r), "lock", 12.0, faint);
            // Clicking the lock icon unlocks the layer, and makes the Background a normal one
            // (Photoshop, #4723). It takes the click from the row, so the row isn't selected.
            if ui.interact(r, ui.scope_id().with(("row-lock", id)), Sense::click()).clicked() {
                cells.clicked.set(true);
                cells.queued.borrow_mut().push(simple_lock_toggle(doc.is_background(l.id), l, &json!({"layer": id})));
            }
        });
    }
    if has_fx || has_smart {
        row = row.trailing_item(TRIANGLE_W, move |ui| {
            let r = ui.max_rect();
            cells.indicators.borrow_mut().push((Indicator::FxTriangle, r));
            let resp = ui.interact(r, ui.scope_id().with(("fx-disclosure", id)), Sense::click());
            crate::layer_row_ui::paint_triangle(ui.painter(), r.center(), fx_open, normal);
            if resp.clicked() {
                cells.clicked.set(true);
                let all = ui.input(|i| i.modifiers.alt);
                cells.queued.borrow_mut().push(("layer.setEffectsExpanded".into(), json!({"layer": id, "expanded": !fx_open, "all": all})));
            }
            let (verb, name) = (if fx_open { tl!("Collapse") } else { tl!("Expand") }, l.name.clone());
            let tip = match (has_smart, fx_open) {
                (true, true) => tl!("Hide the layer's effects and Smart Filters  ({key}-click: all layers)"),
                (true, false) => tl!("Show the layer's effects and Smart Filters  ({key}-click: all layers)"),
                (false, true) => tl!("Hide the layer's effects  ({key}-click: all layers)"),
                (false, false) => tl!("Show the layer's effects  ({key}-click: all layers)"),
            };
            let resp = resp.on_hover_text(crate::i18n::fmt(tip, &[("key", &crate::shortcuts::pretty("Alt"))]));
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{verb} effects {name}")));
        });
    }
    if has_smart {
        row = row.trailing_item(ICON_W, move |ui| {
            let r = ui.max_rect();
            cells.indicators.borrow_mut().push((Indicator::SmartFilters, r));
            let enabled = matches!(&l.content, LayerContent::Smart(sm) if sm.filters_enabled);
            crate::smart_ui::paint_badge(ui.painter(), r.center(), if enabled { normal } else { faint });
            ui.interact(r, ui.scope_id().with(("smart-filters-badge", id)), Sense::hover()).on_hover_text(tl!("Smart Filters"));
        });
    }
    if let Some(g) = fx_galley {
        let ctx = ctx.clone();
        row = row.trailing_item(g.size().x, move |ui| {
            let r = ui.max_rect();
            cells.indicators.borrow_mut().push((Indicator::Fx, r));
            ui.painter().galley(pos2(r.left(), r.center().y - g.size().y / 2.0), g, muted);
            // Dragging the fx badge drags all the layer's effects, not the layer (Photoshop). Like
            // the eye, it takes the drag from the row; clicks still reach the row.
            if ui.interact(r, ui.scope_id().with(("fx-badge", id)), Sense::drag()).drag_started() {
                start_fx_drag(&ctx, l.id, None);
            }
        });
    }
    if l.link_group.is_some() {
        row = row.trailing_item(ICON_W, move |ui| {
            let r = ui.max_rect();
            cells.indicators.borrow_mut().push((Indicator::Link, r));
            icons::paint(ui, icon_rect(r), "link-2", 12.0, faint);
        });
    }
    if let Some(g) = blend_galley {
        row = row.optional_item(g.size().x, move |ui| {
            let r = ui.max_rect();
            cells.indicators.borrow_mut().push((Indicator::Blend, r));
            ui.painter().galley(pos2(r.left(), r.center().y - g.size().y / 2.0), g, faint);
        });
    }
    row
}

/// One row of the Layers panel for `l`, `depth` levels inside groups or artboards: it paints the
/// row, queues what a click or drag on it asks for in `actions`, and returns the row's rect.
#[allow(clippy::too_many_arguments)]
pub(super) fn layer_row(
    app: &mut PhotocraftApp,
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    doc: &photocraft_doc::Document,
    l: &Layer,
    depth: usize,
    row: RowSel,
    actions: &mut Vec<(String, Value)>,
) -> Rect {
    let selected = row.selected;
    let t = Tokens::get(ctx);
    let tokens = LibTokens::of(ui);
    let style = ListRow::<u64>::style(&tokens);
    // The Layers Panel Options thumbnail size controls rows and both layer/mask thumbnails.
    let thumb_size = thumb_size_of(app.ui.layers_panel_options.thumbnail_size);
    let has_thumbs = thumb_size != ThumbSize::None;
    let height = style.height(thumb_size, false);
    let place = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), height));
    // With thumbnails off no mask thumbnail is drawn, so none may keep catching drags from older frames.
    if !has_thumbs {
        crate::mask_thumbs_ui::forget(ctx, l.id.0);
    }
    let renaming = crate::layer_row_ui::renaming(ctx) == Some(l.id.0);
    // A row scrolled out of view only keeps its place (#125): a layout's hundreds of rows would
    // otherwise lay out names, icons and thumbnails every frame. Group rows stay whole: their
    // disclosure triangle is a widget (accessibility, scroll-to).
    let menu_open = ctx.data(|d| d.get_temp::<bool>(menu_open_key(l.id.0))).unwrap_or(false);
    if !ui.is_rect_visible(place) && !l.is_group() && !menu_open && !renaming {
        let (rect, resp) = ui.allocate_exact_size(place.size(), Sense::click_and_drag());
        drag_hooks(app, ctx, ui, l, rect, &resp, actions);
        return rect;
    }

    let cells = Cells::default();
    let masks = crate::mask_thumbs_ui::masks(l);
    let expanded = matches!(&l.content, LayerContent::Group(g) if g.expanded);
    let fx_open = app.session.active().is_none_or(|d| !d.fx_collapsed.contains(&l.id));
    let target = target_of(app, l, &masks).filter(|_| row.primary && has_thumbs);
    let mut lr = ListRow::new(l.id.0, l.name.as_str())
        .toggle(ToggleColumn::eye(tl!("Visibility {row}"), l.visible))
        .selected(selected)
        .dimmed(!l.visible)
        .highlight(app.canvas_hover_layer == Some(l.id))
        .layer(true)
        .renaming(renaming)
        .indent(if l.clipped { CLIP_W } else { 0.0 });
    if has_thumbs {
        lr = lr.thumb(thumb_size, |_, r| cells.first.set(Some(r)));
        if let Some(&(kind, _, linked)) = masks.first() {
            lr = lr.second_thumb(|_, r| cells.second.set(Some(r))).thumb_link(linked, link_name(kind, linked, &l.name));
        }
        if let Some(Target::Library(which)) = target {
            lr = lr.thumb_target(which);
        }
    }
    lr = indicator_items(ui, ctx, l, doc, fx_open, lr, &cells);
    if has_thumbs && let Some(&(kind, _, linked)) = masks.get(1) {
        // A second mask (a vector mask beside a layer mask): the row has room for one mask
        // thumbnail, so this one sits at the trailing end with its own link button.
        let side = style.thumb_side(thumb_size);
        let layer = l.id.0;
        let name = link_name(kind, linked, &l.name);
        let cells = &cells;
        lr = lr.optional_item(side + CHAIN_SLOT, move |ui| {
            let r = ui.max_rect();
            let chain = Rect::from_min_size(r.left_top(), vec2(CHAIN_SLOT, r.height()));
            let hit = ui.interact(chain, ui.scope_id().with(("mask-chain", layer, true)), Sense::click());
            icons::paint(ui, chain, if linked { "link" } else { "unlink" }, 12.0, if hit.hovered() { t.text } else { t.icon });
            hit.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
            if hit.on_hover_text(&name).clicked() {
                cells.clicked.set(true);
                cells.queued.borrow_mut().push((format!("{}.linked", kind.command()), json!({"layer": layer, "linked": !linked})));
            }
            cells.extra.set(Some(Rect::from_center_size(pos2(chain.right() + side / 2.0, r.center().y), vec2(side, side))));
        });
    }
    let out = TreeRow::new(lr, depth, matches!(&l.content, LayerContent::Group(_)), expanded).text(tree_text()).show(ui);
    let (rect, resp) = (out.response.rect, out.response.clone());
    drag_hooks(app, ctx, ui, l, rect, &resp, actions);
    if resp.hovered() {
        app.hover_layer = Some(l.id);
    }
    actions.append(&mut cells.queued.borrow_mut());

    // Events of the row's own parts: the eye column, the disclosure triangle, the mask link and
    // the rename field.
    let mut toggled = cells.clicked.get();
    let mut masks_clicked = false;
    for event in &out.events {
        match event {
            RowEvent::Toggle(edit) => eye_event(ctx, l, edit, actions),
            RowEvent::Open(OpenChange { open, all, .. }) => {
                toggled = true;
                actions.push(("layer.setExpanded".into(), json!({"layer": l.id.0, "expanded": open, "all": all})));
            }
            RowEvent::LinkToggled { linked, .. } => {
                if let Some(&(kind, ..)) = masks.first() {
                    masks_clicked = true;
                    actions.push((format!("{}.linked", kind.command()), json!({"layer": l.id.0, "linked": linked})));
                }
            }
            _ => {}
        }
    }
    if let Some(done) = crate::layer_row_ui::apply_rename_events(ctx, l.id.0, &out.events) {
        actions.push(done);
    }

    let thumb = cells.first.get().unwrap_or_else(|| Rect::from_center_size(rect.left_center(), vec2(0.0, 0.0)));
    let ts = if has_thumbs { thumb.width() } else { 0.0 };
    let mut mask_rects = crate::mask_thumbs_ui::MaskRects { clicked: cells.clicked.get() || masks_clicked, ..Default::default() };
    for (index, &(kind, ..)) in masks.iter().enumerate() {
        if let Some(r) = if index == 0 { cells.second.get() } else { cells.extra.get() } {
            mask_rects.thumbs.push((kind, r));
        }
    }
    if let (Some(a), Some(b), Some(&(kind, ..))) = (cells.first.get(), cells.second.get(), masks.first()) {
        mask_rects.chains.push((kind, Rect::from_min_max(a.right_top(), b.left_bottom())));
    }
    if has_thumbs {
        crate::mask_thumbs_ui::record(ctx, l.id.0, mask_rects.thumbs.clone(), mask_rects.chains.clone());
    }

    // What the library does not draw: the colour label behind the eye, the thumbnails' content
    // and the clipping marker.
    let painter = ui.painter_at(rect.expand(1.0));
    paint_label_color(ui, &t, l, eye_cell(rect, style.toggle_column, craft_ui::direction::is_rtl(ui)));
    if ts > 0.0 {
        draw_layer_thumb(app, ctx, ui, doc, l, thumb);
        for (index, &(kind, enabled, _)) in masks.iter().enumerate() {
            if let Some(r) = if index == 0 { cells.second.get() } else { cells.extra.get() } {
                paint_mask_thumb(app, ctx, ui, doc, l, kind, enabled, r);
            }
        }
        if target == Some(Target::Extra)
            && let Some(r) = cells.extra.get()
        {
            crate::mask_thumbs_ui::paint_brackets(&painter, r, t.text);
        }
    }
    if l.clipped {
        let group_column = if matches!(&l.content, LayerContent::Group(_)) { style.disclosure } else { 0.0 };
        let right = if ts > 0.0 { thumb.left() } else { out.label.left() } - group_column;
        paint_clipping_indicator(&painter, pos2(right - CLIP_W, rect.center().y), t.text_faint);
    }

    // The label as the library laid it out: the name is only as wide as its text.
    let galley = ui.painter().layout_no_wrap(l.name.clone(), egui::TextStyle::Body.resolve(ui.style()), Color32::WHITE);
    let name_size = vec2(galley.size().x.min(out.label.width()), galley.size().y.min(out.label.height()));
    let name_rect = (out.label.width() > 1.0).then(|| Rect::from_min_size(pos2(out.label.left(), out.label.center().y - name_size.y / 2.0), name_size));
    crate::layer_row_ui::record(
        ctx,
        crate::layer_row_ui::RowRects { layer: l.id.0, row: rect, thumbnail: (ts > 0.0).then_some(thumb), name: name_rect, indicators: cells.indicators.take() },
    );
    let rc = RowCtx { l, doc, row, rect, ts, thumb, resp: &resp, masks: &mask_rects };

    // ⌘-click a layer, mask or vector-mask thumbnail loads its transparency / mask / path as a
    // selection (⇧ add, ⌥ subtract, ⇧⌥ intersect) instead of changing the layer selection.
    let load_at = |m: egui::Modifiers, p: Pos2| {
        if !m.command {
            return None;
        }
        if let Some(kind) = mask_rects.hit(p) {
            return Some(crate::mask_thumbs_ui::load_params(l, kind, m));
        }
        // The Background has no transparency to load: Photoshop treats the click as a plain one.
        (ts > 0.0 && !doc.is_background(l.id) && thumb.expand(2.0).contains(p))
            .then(|| json!({"channel": "transparency", "layer": l.id.0, "operation": crate::channels_panel::load_operation(m)}))
    };
    let thumb_load = resp.clicked().then(|| (ui.input(|i| i.modifiers), resp.interact_pointer_pos())).and_then(|(m, pos)| load_at(m, pos?));
    // Over a thumbnail that ⌘ would load, the cursor says what the click does, as in Photoshop (#7824).
    if let Some(p) = resp.hover_pos() {
        let m = ui.input(|i| i.modifiers);
        if load_at(m, p).is_some() {
            thumb_load_cursor(ctx, p, m);
        }
    }
    // ⇧-click a mask thumbnail: disable / enable that mask; ⌥-click a layer mask: view it.
    let mask_toggle = resp.clicked().then(|| (ui.input(|i| i.modifiers), resp.interact_pointer_pos())).and_then(|(m, pos)| {
        let kind = mask_rects.hit(pos?)?;
        crate::mask_thumbs_ui::click_command(l, kind, m)
    });
    if let Some(p) = thumb_load {
        actions.push(("select.loadSelection".into(), p));
    } else if let Some(cmd) = mask_toggle {
        actions.push(cmd);
    } else if resp.clicked() && !toggled && !mask_rects.clicked {
        select_row_on_click(app, ui, &rc, actions);
    }
    // Double-click: the name renames in place (over the row's full height, not just the glyphs:
    // #651); the Background, which can't be renamed while it's locked, becomes a normal layer; an
    // adjustment, fill or shape thumbnail opens its settings (a solid colour: the Color Picker,
    // #2812) and a Smart Object thumbnail its contents, and a type thumbnail edits its text;
    // anywhere else on the row opens Layer Style (#350, #537).
    // The first click already made this the active layer.
    handle_row_double_click(app, ui.ctx(), &rc, name_rect, actions);
    row_context_menu(app, ui, &rc, eye_cell(rect, style.toggle_column, craft_ui::direction::is_rtl(ui)), actions);
    rect
}

/// What the row helpers share once the row's thumbnails are laid out.
struct RowCtx<'a> {
    l: &'a Layer,
    doc: &'a photocraft_doc::Document,
    row: RowSel,
    rect: Rect,
    ts: f32,
    thumb: Rect,
    resp: &'a egui::Response,
    masks: &'a crate::mask_thumbs_ui::MaskRects,
}

/// The eye column's cell at the row's reading start.
fn eye_cell(rect: Rect, column: f32, rtl: bool) -> Rect {
    let width = column.min(rect.width());
    if rtl { Rect::from_min_size(pos2(rect.right() - width, rect.top()), vec2(width, rect.height())) } else { Rect::from_min_size(rect.left_top(), vec2(width, rect.height())) }
}

/// The layer's colour label fills the eye column behind its icon.
fn paint_label_color(ui: &egui::Ui, t: &Tokens, l: &Layer, cell: Rect) {
    let Some((_, background)) = t.layer_label_colors(l.label) else { return };
    ui.painter().rect_filled(cell, craft_ui::theme::scale::Radius::R0.px(), background);
    // A hidden layer's eye box is left empty.
    if l.visible {
        icons::paint(ui, cell, "eye", 15.0, t.layer_label_icon(l.label));
    }
}

/// An edit of the eye column: a click, or a drag down the column (Photoshop), where the first eye
/// toggles and every row swept over gets the same visibility, all in one history step. ⌥ shows
/// only this layer (once per gesture).
fn eye_event(ctx: &egui::Context, l: &Layer, edit: &EditEvent<ToggleEdit<u64>>, actions: &mut Vec<(String, Value)>) {
    let alt_key = egui::Id::unique("layer-eye-alt");
    let (gesture, value, modifiers) = match edit {
        EditEvent::Update { id, value, modifiers, .. } => (Some(*id), value, *modifiers),
        EditEvent::Set { value, modifiers, .. } => (None, value, *modifiers),
        _ => return,
    };
    if modifiers.alt {
        let seen = gesture.is_some_and(|g| ctx.data(|d| d.get_temp::<GestureId>(alt_key)) == Some(g));
        if !seen {
            if let Some(g) = gesture {
                ctx.data_mut(|d| d.insert_temp(alt_key, g));
            }
            actions.push(("layer.showOnly".into(), json!({"layer": l.id.0})));
        }
        return;
    }
    let mut params = json!({"layer": l.id.0, "visible": value.on});
    if let Some(g) = gesture {
        params["coalesce"] = json!(format!("layer-eye-sweep:{g}"));
    }
    actions.push(("layer.setProps".into(), params));
}

/// A plain click on the row: select the layer and retarget pixels / mask / vector mask.
fn select_row_on_click(app: &mut PhotocraftApp, ui: &egui::Ui, rc: &RowCtx<'_>, actions: &mut Vec<(String, Value)>) {
    let (l, doc, resp, masks, row, ts, thumb) = (rc.l, rc.doc, rc.resp, rc.masks, rc.row, rc.ts, rc.thumb);
    let ctx = ui.ctx();
    let mask_rect = masks.thumb(crate::mask_thumbs_ui::MaskKind::Pixel);
    let m = ui.input(|i| i.modifiers);
    let pos = resp.interact_pointer_pos();
    let on_thumb = ts > 0.0 && pos.is_some_and(|p| thumb.expand(2.0).contains(p));
    // ⌘-click on the Background thumbnail stands in for the plain click (there is no
    // transparency to load): it selects the Background alone, not toggled into the set.
    let mode = if m.command && on_thumb && doc.is_background(l.id) { "replace" } else { select_mode(m) };
    actions.push(("layer.select".into(), json!({"layer": l.id.0, "mode": mode})));
    crate::smart_ui::deselect(ctx, doc.id);
    // A thumbnail explicitly selects content or a mask. Selecting an adjustment/fill row
    // defaults to its mask, but clicking its content thumbnail must leave the mask target.
    let on_mask = mask_rect.zip(pos).is_some_and(|(r, p)| r.expand(2.0).contains(p));
    let on_vector = pos.and_then(|p| masks.hit(p)) == Some(crate::mask_thumbs_ui::MaskKind::Vector);
    // Clicking the layer thumbnail leaves mask view (#196).
    let viewing = app.session.active().and_then(photocraft_engine::mask_view_cmds::current).filter(|v| v.layer == l.id);
    if let Some(v) = viewing.filter(|_| on_thumb) {
        let mask = if v.filter { "filter" } else { "layer" };
        actions.push((photocraft_engine::mask_view_cmds::ID.into(), json!({"layer": l.id.0, "mode": "off", "mask": mask})));
    }
    let content_less = matches!(l.content, LayerContent::Adjustment(_) | LayerContent::Fill(_));
    if on_vector {
        // The vector mask thumbnail targets the vector mask: the path tools edit it and the
        // Paths panel selects the layer's path.
        app.ui.selected_path = Some("layer".into());
        actions.push(("ui.vectorMaskTarget".into(), json!(true)));
    } else if on_mask || (content_less && l.mask.is_some() && !on_thumb) {
        if on_mask {
            // The mask thumbnail also switches Properties to the mask (#3369).
            app.ui.properties_layer = None;
        }
        actions.push(("ui.maskTarget".into(), json!(true)));
    } else if on_thumb || !row.primary {
        actions.push(("ui.maskTarget".into(), json!(false)));
    }
    // ⌥-click the layer thumbnail: fit the layer on screen, as Photoshop does.
    if on_thumb && m.alt && !m.shift && !m.command {
        actions.push(("view.fitLayersOnScreen".into(), Value::Null));
    }
}

/// Double-click: rename, un-background, open the thumbnail's settings or the row's Layer Style.
fn handle_row_double_click(app: &mut PhotocraftApp, ctx: &egui::Context, rc: &RowCtx<'_>, name_rect: Option<Rect>, actions: &mut Vec<(String, Value)>) {
    let (l, doc, resp, masks, ts, thumb, rect) = (rc.l, rc.doc, rc.resp, rc.masks, rc.ts, rc.thumb, rc.rect);
    if resp.double_clicked() {
        let pos = resp.interact_pointer_pos();
        let on = |r: Rect| pos.is_some_and(|p| r.expand(2.0).contains(p));
        if crate::doc_props_ui::is_background(doc, l) {
            actions.push(("layer.new.layerFromBackground".into(), json!({})));
        } else if name_rect.is_some_and(|n| on(Rect::from_x_y_ranges(n.x_range(), rect.y_range()))) {
            if let Some(done) = crate::layer_row_ui::start_rename(ctx, l.id.0, &l.name) {
                // One rename at a time: starting this one commits any other (#314).
                actions.push(done);
            }
        } else if let Some(kind) = pos.and_then(|p| masks.hit(p)) {
            match kind {
                crate::mask_thumbs_ui::MaskKind::Pixel => actions.push(("ui.maskTarget".into(), json!(true))),
                crate::mask_thumbs_ui::MaskKind::Vector => actions.push(("ui.vectorMaskTarget".into(), json!(true))),
            }
            if let Some((cmd, val)) = crate::mask_thumbs_ui::double_click_command(app, kind) {
                // The preference launches Select and Mask on a pixel mask.
                actions.push((cmd.into(), val));
            } else {
                // Otherwise double-click opens the mask's settings (Density/Feather).
                actions.push(("ui.properties".into(), json!(true)));
            }
        } else if pos.and_then(|p| masks.hit(p)).is_none() {
            let id = match &l.content {
                LayerContent::Adjustment(_) | LayerContent::Fill(_) | LayerContent::Shape(_) if ts > 0.0 && on(thumb) => "layer.layerContentOptions",
                LayerContent::Smart(_) if ts > 0.0 && on(thumb) => "layer.smartObjects.editContents",
                LayerContent::Text(_) if ts > 0.0 && on(thumb) => "type.editText",
                _ => "layer.layerStyle.blendingOptions",
            };
            if crate::menus::is_enabled(app, id) {
                // Null params: run like the menu item, dialog included.
                actions.push((id.into(), Value::Null));
            }
        }
    }
}

/// Where a right-click landed, which picks the menu: the eye column has its colour menu, the
/// layer-mask thumbnail its mask menu (#4575), the rest of the row the full layer menu.
#[derive(Clone, Copy, PartialEq)]
enum Zone {
    Row,
    Eye,
    Mask,
}

fn row_context_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui, rc: &RowCtx<'_>, eye: Rect, actions: &mut Vec<(String, Value)>) {
    let (l, resp, masks, row) = (rc.l, rc.resp, rc.masks, rc.row);
    let ctx = ui.ctx().clone();
    let zone_id = ui.scope_id().with(("row-menu-zone", l.id.0));
    let secondary = ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary));
    let pos = ui.input(|i| i.pointer.interact_pos());
    // The eye cell takes the pointer from the row, so its right-click is read from the pointer.
    let on_eye = secondary && pos.is_some_and(|p| eye.contains(p)) && ui.rect_contains_pointer(rc.rect);
    let open_at = if on_eye || resp.secondary_clicked() { pos } else { None };
    if let Some(p) = open_at {
        let zone = if on_eye {
            Zone::Eye
        } else if masks.hit(p) == Some(crate::mask_thumbs_ui::MaskKind::Pixel) {
            Zone::Mask
        } else {
            Zone::Row
        };
        ctx.data_mut(|d| d.insert_temp(zone_id, zone));
    }
    let open = ContextMenu::<()>::is_open(&ctx, resp);
    ctx.data_mut(|d| d.insert_temp(menu_open_key(l.id.0), open));
    if open_at.is_none() && !open {
        return;
    }
    // Right-clicking inside a multi-selection keeps it and acts on every selected layer.
    let on_set = row.multi && row.selected;
    let zone = ctx.data(|d| d.get_temp::<Zone>(zone_id)).unwrap_or(Zone::Row);
    let rows = match zone {
        Zone::Eye => crate::layer_menu_ui::color_rows(app, &ctx, l, on_set),
        Zone::Mask => crate::layer_menu_ui::mask_rows(app, l),
        Zone::Row => crate::layer_menu_ui::menu_rows(app, &ctx, l, on_set),
    };
    let menu = ContextMenu::new(&rows).own_gestures(false).open_at(open_at.map(OpenAt::Pointer)).show(ui, resp);
    if let Some(choice) = menu.chosen
        && crate::layer_menu_ui::run_choice(choice, l, on_set && zone != Zone::Mask, actions)
        && let Some(done) = crate::layer_row_ui::start_rename(&ctx, l.id.0, &l.name)
    {
        actions.push(done);
    }
}

/// Screen rect and UVs for a layer thumbnail: the document-shaped part of the square cell and of
/// the letterboxed square texture, so tall and wide documents don't show empty bars.
pub(super) fn layer_thumb_fit(cell: Rect, w: u32, h: u32) -> (Rect, Rect) {
    crate::channels_panel::fit_thumb(cell, w, h)
}

/// The content of a layer's thumbnail; the library frames it.
fn draw_layer_thumb(app: &mut PhotocraftApp, ctx: &egui::Context, ui: &egui::Ui, doc: &photocraft_doc::Document, l: &Layer, rect: Rect) {
    let t = Tokens::get(ctx);
    let p = ui.painter();
    match &l.content {
        LayerContent::Adjustment(_) | LayerContent::Group(_) => {
            p.rect_filled(rect, craft_ui::theme::scale::Radius::R6.px(), t.field);
            let icon = if l.is_group() { "folder" } else { "contrast" };
            icons::paint(ui, rect, icon, 18.0, t.icon);
        }
        LayerContent::Text(_) => {
            // Photoshop: type layers show a "T" tile rather than a pixel thumbnail.
            p.rect_filled(rect, 0.0, Color32::from_gray(222));
            icons::paint(ui, rect, "type", 20.0, Color32::from_gray(40));
        }
        // Gradient fills show the gradient itself (Photoshop); solid ones their colour.
        LayerContent::Fill(f) if crate::gradient_ui::paint_thumbnail(ui, l.id, f, rect) => {}
        LayerContent::Fill(photocraft_doc::Fill::Solid(color)) => {
            crate::solid_fill_ui::paint_thumbnail(ui, crate::solid_fill_ui::thumbnail_color(app, doc.id, l.id, *color), rect);
        }
        LayerContent::Fill(_) => {
            p.rect_filled(rect, craft_ui::theme::scale::Radius::R6.px(), Color32::from_rgba_unmultiplied(128, 128, 128, 255));
        }
        _ => {
            // Keep row layout and outside thumbnail decorations when the image is clipped.
            if ui.is_rect_visible(rect) {
                // The texture is a letterboxed square: draw only the part its frame (the document,
                // or the layer's bounds) fills.
                let (tex, frame) = app.layer_thumb(ctx, doc, l);
                let (fitted, uv) = layer_thumb_fit(rect, frame.width(), frame.height());
                widgets::checker(p, fitted, 5.0);
                p.image(tex, fitted, uv, Color32::WHITE);
            }
        }
    }
    crate::smart_ui::thumb_badge(ui, l, rect);
}

/// The content of a mask's thumbnail: its image, Photoshop's red X when it is disabled and the
/// kind badge; the library frames it.
#[allow(clippy::too_many_arguments)]
fn paint_mask_thumb(app: &mut PhotocraftApp, ctx: &egui::Context, ui: &egui::Ui, doc: &photocraft_doc::Document, l: &Layer, kind: MaskKind, enabled: bool, r: Rect) {
    let t = Tokens::get(ctx);
    let painter = ui.painter();
    if ui.is_rect_visible(r) {
        let tex = match kind {
            MaskKind::Pixel => l.mask.as_ref().map(|m| app.mask_thumb(ctx, doc, l.id, m)),
            MaskKind::Vector => l.vector_mask.as_ref().map(|m| app.vector_mask_thumb(ctx, doc, l.id, m)),
        };
        if let Some(tex) = tex {
            painter.image(tex, r, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
        }
        if !enabled {
            let st = craft_ui::theme::scale::Stroke::W1_5.egui(ui.ctx(), t.danger);
            let k = r.shrink(1.0);
            painter.line_segment([k.left_top(), k.right_bottom()], st);
            painter.line_segment([k.right_top(), k.left_bottom()], st);
        }
    }
    if app.ui.layers_panel_options.show_layer_mask_badges {
        // Distinguish pixel and vector masks without altering their thumbnails.
        let badge = Rect::from_min_size(pos2(r.right() - 9.0, r.bottom() - 9.0), vec2(9.0, 9.0));
        painter.rect_filled(badge, craft_ui::theme::scale::Radius::R2.px(), t.card);
        icons::paint(ui, badge, if kind == MaskKind::Vector { "pen-tool" } else { "circle-dot" }, 8.0, t.text);
    }
}
