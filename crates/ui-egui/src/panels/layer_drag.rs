use super::layers::footer_label;
use super::{Layer, LayerId, MaskKind, PhotocraftApp, Rect, StrokeKind, Tokens, Value, json};
use craft::ui::behavior::{Axis, DropPosition, ListDrag, Reorder, RowInfo};
use craft::ui::edit::EditEvent;
use craft::ui::icons::Icon;
use craft::ui::list::{DropMark, ListRow, RowEvent, ToggleColumn, TreeRow, paint_drop_mark};
use craft::ui::theme::StrokeExt;
use photocraft_doc::LayerContent;
use std::ops::Range;
use std::sync::Arc;

/// What a Layers panel row drag carries. The drag is egui's `DragAndDrop` payload [`ListDrag`], so
/// the footer buttons, the document tabs and the other rows take drops from one source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RowKey {
    /// A layer row.
    Layer(LayerId),
    /// A layer's effects: one (`Some`) or the "Effects" row (all of them).
    Effects(LayerId, Option<usize>),
    /// A smart object's filters: one (`Some`) or the Smart Filters row (all of them).
    Filters(LayerId, Option<usize>),
    /// A mask thumbnail (#3113, #3119).
    Mask(LayerId, MaskKind),
}

/// The drag of a Layers panel row.
pub(super) type PanelDrag = ListDrag<RowKey>;

/// An fx row being dragged: the layer it belongs to and the effect index (`None` = the "Effects"
/// row, all of them).
pub(super) type FxDrag = (u64, Option<usize>);

/// A mask thumbnail being dragged: the layer and which of its masks (#3113, #3119).
pub(super) type MaskDrag = (u64, MaskKind);

/// The Layers panel's list id in its drags.
fn list_id() -> egui::Id {
    egui::Id::unique("pc-layers-list")
}

/// The Layers panel row drag under way, if any.
pub(super) fn current(ctx: &egui::Context) -> Option<Arc<PanelDrag>> {
    egui::DragAndDrop::payload::<PanelDrag>(ctx)
}

fn start(ctx: &egui::Context, items: Vec<RowKey>, label: impl Into<String>) {
    egui::DragAndDrop::set_payload(ctx, PanelDrag { list: list_id(), items, label: label.into() });
}

/// Ends the Layers panel row drag (the pointer went to another document).
pub(crate) fn cancel(ctx: &egui::Context) {
    if current(ctx).is_some() {
        egui::DragAndDrop::clear_payload(ctx);
    }
}

/// What the Layers panel's drag holds this frame, by kind, for the footer buttons.
#[derive(Default)]
pub(super) struct Held {
    /// The grabbed layer and whether it belongs to the selection (the drop then acts on the selection).
    pub(super) layer: Option<(u64, bool)>,
    pub(super) fx: Option<FxDrag>,
    pub(super) filter: Option<crate::smart_ui::FilterDrag>,
    pub(super) mask: Option<MaskDrag>,
}

pub(super) fn held(ctx: &egui::Context, selection: &[LayerId]) -> Held {
    let mut held = Held::default();
    let Some(drag) = current(ctx) else { return held };
    match drag.items.first() {
        Some(RowKey::Layer(id)) => held.layer = Some((id.0, selection.contains(id))),
        Some(RowKey::Effects(l, i)) => held.fx = Some((l.0, *i)),
        Some(RowKey::Filters(l, i)) => held.filter = Some((l.0, *i)),
        Some(RowKey::Mask(l, k)) => held.mask = Some((l.0, *k)),
        None => {}
    }
    held
}

/// The content displacement in points for this frame while a row drag is held at the Layers
/// panel's top or bottom edge or past it: positive moves the list downward (reveals rows above).
/// The speed is the library's: it ramps with the nearness to the edge and uses the frame time.
pub(super) fn edge_scroll(ctx: &egui::Context, viewport: Rect) -> f32 {
    if current(ctx).is_none() || !ctx.input(|i| i.pointer.primary_down()) {
        return 0.0;
    }
    let Some(pointer) = ctx.input(|i| i.pointer.interact_pos()) else { return 0.0 };
    -Reorder::<LayerId>::new(Axis::Vertical).auto_scroll(pointer, viewport, ctx.input(|i| i.stable_dt))
}

/// While a row is dragged: its name by the pointer, and the copy cursor while ⌥ is held for the
/// rows that copy (layers, effects, filters; not masks).
pub(super) fn drag_feedback(ui: &egui::Ui) {
    let ctx = ui.ctx();
    let Some(drag) = current(ctx) else { return };
    let Some(p) = ctx.input(|i| i.pointer.interact_pos()) else { return };
    crate::layer_transfer::show_ghost(ui, p, &drag.label);
    if !matches!(drag.items.first(), Some(RowKey::Mask(..))) && ctx.input(|i| i.modifiers.alt) {
        ctx.set_cursor_icon(egui::CursorIcon::Copy);
    }
}

fn mark_of_position(position: DropPosition) -> DropMark {
    match position {
        DropPosition::Before => DropMark::Before,
        DropPosition::After => DropMark::After,
        DropPosition::Into => DropMark::Into,
    }
}

fn position_name(position: DropPosition) -> &'static str {
    match position {
        DropPosition::Before => "above",
        DropPosition::After => "below",
        DropPosition::Into => "into",
    }
}

/// The pointer, while it is over the visible part of the panel's rows.
fn pointer_in(ui: &egui::Ui) -> Option<egui::Pos2> {
    ui.ctx().input(|i| i.pointer.interact_pos()).filter(|p| ui.clip_rect().contains(*p))
}

/// Marks layer `target`'s row as this pass's drop target.
fn mark_row(ui: &egui::Ui, drawn: &[DrawnRow], target: LayerId, mark: DropMark) {
    if let Some(d) = drawn.iter().find(|d| d.layer.id == target) {
        paint_drop_mark(ui, d.rect, mark);
    }
}

/// The marker of filter row `rect` of a smart object's stack for the drop `position`.
pub(crate) fn mark_filter_row(ui: &egui::Ui, rect: Rect, position: DropPosition) {
    paint_drop_mark(ui, rect, mark_of_position(position));
}

/// A layer row dragged onto a Layers panel footer button, as in Photoshop: onto Delete deletes it,
/// onto New Layer duplicates it, onto New Group groups it. A row that is part of the selection
/// carries the whole selection; any other row goes alone. Highlights the button while over it and
/// returns the command on release.
pub(super) fn footer_drop(ui: &egui::Ui, button: &egui::Response, drag: Option<(u64, bool)>, command: &str) -> Option<(String, Value)> {
    // Named for screen readers (and tests) after the command a drop runs.
    button.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, footer_label(command)));
    let (layer, in_selection) = drag?;
    dropped_on(ui, button).then(|| (command.into(), if in_selection { json!({}) } else { json!({"layer": layer}) }))
}

/// While something is dragged over a footer `button`: highlights it, and is true on the release.
pub(crate) fn dropped_on(ui: &egui::Ui, button: &egui::Response) -> bool {
    let Some(p) = ui.ctx().input(|i| i.pointer.interact_pos()) else { return false };
    if !button.rect.contains(p) {
        return false;
    }
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_stroke(button.rect, t.radius_sm, craft::ui::theme::scale::Stroke::W2.egui(ui.ctx(), t.accent), StrokeKind::Inside);
    ui.ctx().input(|i| i.pointer.any_released())
}

/// Effects dragged onto the footer's Delete button, as in Photoshop (#3102): the "Effects" row
/// clears the layer style, an effect's row deletes just that effect.
pub(super) fn fx_trash_drop(ui: &egui::Ui, button: &egui::Response, drag: Option<FxDrag>) -> Option<(String, Value)> {
    let (layer, effect) = drag?;
    if !dropped_on(ui, button) {
        return None;
    }
    let params = match effect {
        Some(i) => json!({"layer": layer, "effect": i}),
        None => json!({"layer": layer}),
    };
    Some(("layer.layerStyle.clear".into(), params))
}

/// A drag from within a multi-selection moves the whole selection, not only the grabbed
/// row. Dragging an unselected row keeps the existing single-layer behavior.
pub(super) fn layer_drop_payload(dragged: u64, target: LayerId, position: &str, selected: &[LayerId]) -> Value {
    if selected.len() > 1 && selected.contains(&LayerId(dragged)) {
        json!({"layers": selected.iter().map(|id| id.0).collect::<Vec<_>>(), "target": target.0, "position": position})
    } else {
        json!({"layer": dragged, "target": target.0, "position": position})
    }
}

/// Would this drop move the Background off the bottom, or put a layer below it? Photoshop
/// refuses both (no drop indicator); ⌥-dragging the Background still copies it above.
pub(super) fn background_drop_refused(doc: &photocraft_doc::Document, payload: &Value, target: LayerId, position: &str, copy: bool) -> bool {
    let Some(bg) = doc.layers.first().filter(|b| crate::doc_props_ui::is_background(doc, b)).map(|b| b.id) else { return false };
    let moves_bg =
        !copy && (payload["layer"].as_u64() == Some(bg.0) || payload["layers"].as_array().is_some_and(|a| a.iter().any(|v| v.as_u64() == Some(bg.0))));
    moves_bg || (target == bg && position == "below")
}

/// A Layers panel row as drawn this pass.
pub(super) struct DrawnRow<'a> {
    pub(super) layer: &'a Layer,
    pub(super) depth: usize,
    pub(super) rect: Rect,
}

/// A drag begins on layer `l`'s row: the selection when `l` is in it, else `l` alone. The layers
/// can be dropped on another document too (`layer_transfer`).
pub(super) fn start_layer_drag(app: &PhotocraftApp, ctx: &egui::Context, l: &Layer) {
    let Some(st) = app.session.active() else { return };
    let layers = if st.is_layer_selected(l.id) { st.selected_layers() } else { vec![l.id] };
    start(ctx, layers.iter().map(|id| RowKey::Layer(*id)).collect(), l.name.as_str());
    crate::layer_transfer::begin_from_panel(app, ctx, &layers);
}

/// Drag layer rows to reorder: drop above, below, or inside an existing group, on a row, between
/// two rows or below the last one (#1721). With ⌥ held on release the layers stay put and copies
/// land there instead (Photoshop). Multi-layer moves are atomic (one undo step), using the
/// engine's stable document order. The library's `Reorder` rules find the target; the Background
/// neither moves nor takes a layer below it, and a multi-selection is not copied onto its own
/// row (the engine copies it beside each of its own layers).
///
/// Effects and smart filters dragged over a layer row mark it too; they are dropped on release
/// (smart filters only on other smart objects).
pub(super) fn layer_drop(app: &mut PhotocraftApp, ui: &egui::Ui, drawn: &[DrawnRow], actions: &mut Vec<(String, Value)>) {
    let ctx = ui.ctx();
    let Some(drag) = current(ctx) else { return };
    let Some(p) = pointer_in(ui) else { return };
    let released = ctx.input(|i| i.pointer.any_released());
    let alt = ctx.input(|i| i.modifiers.alt);
    match drag.items.first() {
        Some(RowKey::Layer(_)) => {
            let items: Vec<LayerId> = drag.items.iter().filter_map(|k| if let RowKey::Layer(id) = k { Some(*id) } else { None }).collect();
            let rows: Vec<RowInfo<LayerId>> =
                drawn.iter().map(|d| RowInfo { key: d.layer.id, depth: d.depth, has_children: d.layer.is_group(), open: false, section: false }).collect();
            let spans: Vec<Range<f32>> = drawn.iter().map(|d| d.rect.top()..d.rect.bottom()).collect();
            let Some(st) = app.session.active() else { return };
            let resolved = {
                let doc = &st.doc;
                let rules = Reorder::new(Axis::Vertical).copy_with_alt(true).accepts(|mv, _| {
                    let position = position_name(mv.position);
                    !background_drop_refused(doc, &layer_drop_payload(items[0].0, mv.target, position, &items), mv.target, position, mv.copy)
                        && !(mv.copy && items.len() > 1 && items.contains(&mv.target))
                });
                rules.resolve(&rows[..], &spans[..], p.y, &items, ctx.input(|i| i.modifiers))
            };
            let Some(r) = resolved else { return };
            mark_row(ui, drawn, r.mv.target, mark_of_position(r.mv.position));
            let mut payload = layer_drop_payload(items[0].0, r.mv.target, position_name(r.mv.position), &items);
            if r.mv.copy
                && let Some(o) = payload.as_object_mut()
            {
                o.insert("copy".into(), json!(true));
            }
            if released {
                app.reorder_preview = None;
                actions.push(("layer.moveTo".into(), payload));
            } else if ctx.input(|i| i.pointer.primary_down()) {
                crate::layers_panel_ui::preview_reorder(app, &payload);
            }
        }
        Some(&(RowKey::Effects(from, _) | RowKey::Filters(from, _))) => {
            // Not a reorder: the whole row takes the drop.
            let over = drawn
                .iter()
                .find(|d| d.rect.contains(p))
                .filter(|d| d.layer.id != from && (matches!(drag.items[0], RowKey::Effects(..)) || matches!(d.layer.content, LayerContent::Smart(_))));
            if let Some(d) = over {
                paint_drop_mark(ui, d.rect, DropMark::Into);
            }
            if let Some(d) = over.filter(|_| released) {
                actions.push(match drag.items[0] {
                    RowKey::Effects(_, effect) => fx_drop_action(from.0, effect, d.layer.id, alt),
                    RowKey::Filters(_, filter) => crate::smart_ui::drop_action(from.0, filter, d.layer.id, alt),
                    _ => return,
                });
            }
        }
        _ => {}
    }
}

/// Which mask's thumbnail (where it was drawn last frame) the row's drag starts on, if any.
pub(super) fn starts_mask_drag(ctx: &egui::Context, l: &Layer, resp: &egui::Response) -> Option<MaskKind> {
    if !resp.drag_started() {
        return None;
    }
    let p = ctx.input(|i| i.pointer.press_origin())?;
    let (thumbs, _) = crate::mask_thumbs_ui::recorded(ctx, l.id.0)?;
    let kind = thumbs.iter().find(|(_, r)| r.contains(p))?.0;
    let present = match kind {
        MaskKind::Pixel => l.mask.is_some(),
        MaskKind::Vector => l.vector_mask.is_some(),
    };
    present.then_some(kind)
}

/// A drag begins on `layer`'s `kind` mask thumbnail.
pub(super) fn start_mask_drag(ctx: &egui::Context, layer: LayerId, kind: MaskKind) {
    let label = if kind == MaskKind::Vector { tl!("Vector Mask") } else { tl!("Layer Mask") };
    start(ctx, vec![RowKey::Mask(layer, kind)], label);
}

/// Start dragging the effects of `layer`: one (`effect`) or all of them, named `label`.
pub(super) fn start_fx_drag(ctx: &egui::Context, layer: LayerId, effect: Option<usize>, label: &str) {
    start(ctx, vec![RowKey::Effects(layer, effect)], label);
}

/// Start dragging the smart filters of `layer`: one (`filter`) or all of them, named `label`.
pub(crate) fn start_filter_drag(ctx: &egui::Context, layer: LayerId, filter: Option<usize>, label: &str) {
    start(ctx, vec![RowKey::Filters(layer, filter)], label);
}

/// The command for dropping effects of layer `from` on `target`: they move there, or with ⌥ held
/// on release are copied (Photoshop). `effect` is one effect row, or all effects when `None`.
fn fx_drop_action(from: u64, effect: Option<usize>, target: LayerId, copy: bool) -> (String, Value) {
    let mut payload = json!({"from": from, "to": target.0, "copy": copy});
    if let Some(o) = payload.as_object_mut()
        && let Some(i) = effect
    {
        o.insert("effect".into(), json!(i));
    }
    ("layer.layerStyle.transferEffects".into(), payload)
}

/// Photoshop shows a layer's effects as indented sub-rows ("Effects", then each effect). The eye
/// on "Effects" shows or hides them all, the eye on an effect's row just that one (#1622). Library
/// rows nested under the layer's, with the eye in the layers' eye column like the Smart Filters
/// rows.
pub(super) fn effect_rows(app: &mut PhotocraftApp, ui: &mut egui::Ui, l: &Layer, depth: usize, actions: &mut Vec<(String, Value)>) {
    // English `Effect::label` keys stay for kind matching; display goes through `tl!` (catalogs
    // already have Drop Shadow → 投影, Effects → 效果, …).
    let mut rows: Vec<(String, bool, Option<&'static str>)> = vec![(tl!("Effects").into(), l.effects.enabled, None)];
    for e in &l.effects.items {
        let kind = crate::layer_style::KINDS.iter().find(|k| k.1 == e.label()).map(|k| k.0);
        rows.push((tl!(e.label()).into(), e.enabled(), kind));
    }
    for (i, (name, on, kind)) in rows.into_iter().enumerate() {
        let effect = i.checked_sub(1);
        let mut row = ListRow::new(("fx", l.id.0, effect), name.as_str())
            .toggle(ToggleColumn::eye(tl!("Visibility {row}"), on))
            .dimmed(!on)
            .layer(true)
            .indented_toggles(true);
        if effect.is_none() {
            row = row.icon(Icon::Sparkles);
        }
        let out = TreeRow::new(row, depth + 1 + usize::from(effect.is_some()), false, false).text(super::layer_row::tree_text()).show(ui);
        let resp = &out.response;
        let mut handled = false;
        for event in &out.events {
            // A click or a sweep over the eye never opens the Layer Style dialog.
            if let RowEvent::Toggle(EditEvent::Update { value, .. } | EditEvent::Set { value, .. }) = event {
                handled = true;
                if value.on != on {
                    let mut params = json!({"layer": l.id.0, "visible": value.on});
                    if let (Some(index), Some(o)) = (effect, params.as_object_mut()) {
                        o.insert("index".into(), json!(index));
                    }
                    actions.push(("layer.setEffectsVisible".into(), params));
                }
            }
        }
        // Drag the row onto another layer to move its effects there, ⌥-drag to copy them.
        if resp.drag_started() {
            start_fx_drag(ui.ctx(), l.id, effect, &name);
        }
        if resp.double_clicked() && !handled {
            crate::layer_style::open_for_layer(app, l.id, kind);
        }
        // #2846: right-click opens the layer style menu (Create Layer, Copy Layer Style…), as in Photoshop.
        crate::layer_menu_ui::show_effects(app, ui, resp, l, actions);
    }
}
