//! Layers-panel pieces for smart objects: the thumbnail badge and the Smart Filters sub-rows
//! (eye toggles; double-click a filter to re-open its dialog with the recorded parameters; click
//! to select one; drag onto another smart object to move it there, ⌥-drag to copy it, #4271;
//! drag it up or down its own stack to reorder it).
//! The Smart Filters row carries the filter mask thumbnail: click it to paint the filter mask,
//! ⌥-click to view it, ⇧-click to disable it (FILE-215-3).

use photocraft_doc::{Document, Layer, LayerContent, LayerId, SmartObject};
use serde_json::{Value, json};

use crate::PhotocraftApp;

/// The display name of a smart filter (its command's label without the ellipsis). A Photoshop
/// filter PhotoCraft doesn't implement shows its own name and is marked as kept as is.
pub(crate) fn filter_label(f: &photocraft_doc::SmartFilter) -> String {
    let command = f.command.as_str();
    if command == photocraft_engine::smart_cmds::UNSUPPORTED_FILTER {
        let name = f.params.get("name").and_then(Value::as_str).unwrap_or("Smart filter");
        return format!("{} (kept, not editable)", name.trim_end_matches("...").trim_end_matches('…'));
    }
    photocraft_engine::commands::find(command)
        .map_or_else(|| command.rsplit('.').next().unwrap_or(command).to_string(), |c| tl!(c.label).trim_end_matches('…').to_string())
}

/// Smart Filters header + one row per filter (top filter first, as in Photoshop) under a smart
/// layer. Edits are queued in `actions` as engine commands.
pub fn has_filters(l: &Layer) -> bool {
    matches!(&l.content, LayerContent::Smart(sm) if !sm.smart_filters.is_empty())
}

<<<<<<< HEAD
=======
/// The two rings in the layer row indicate an attached Smart Filters stack.
pub fn paint_badge(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    for dx in [-2.5, 2.5] {
        painter.circle_stroke(center + vec2(dx, 0.0), 3.5, Stroke::new(1.0, color));
    }
}

/// Select smart object `layer` now, ahead of its filter editor. The editor captures the
/// document's revision as its target, so the click's queued `layer.select` must not run after it
/// opens (OK would refuse the edit): select first, as the Layers panel runs it. False when the
/// selection failed and the editor must stay closed.
fn select_before_editor(app: &mut PhotocraftApp, layer: u64, actions: &mut Vec<(String, Value)>) -> bool {
    actions.retain(|(id, p)| !(id == "layer.select" && p.get("layer").and_then(Value::as_u64) == Some(layer)));
    if app.ui.text_edit.is_some() {
        crate::type_tool::commit(app);
    }
    app.run("layer.select", json!({"layer": layer})).is_ok()
}

/// Paint the Smart Filters header and one row per filter of Smart Object `l` (top filter first), with their drag, drop and click handling; actions go to `actions`.
pub fn filter_rows(app: &mut PhotocraftApp, ui: &mut egui::Ui, doc: &Document, l: &Layer, depth: usize, actions: &mut Vec<(String, Value)>) {
    let LayerContent::Smart(sm) = &l.content else { return };
    if sm.smart_filters.is_empty() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    let (header_height, mask_size) = smart_filters_header_geometry(app.ui.layers_panel_options.thumbnail_size, t.pro);
    let indent = 30.0 + crate::layer_row_ui::indent(depth, ui.available_width(), 130.0);
    let Some(st) = app.session.active() else { return };
    let active = st.active_layer == Some(l.id);
    let picked = ui.ctx().data(|d| d.get_temp::<FilterDrag>(selected_key(doc.id)));
    let rows = std::iter::once(None).chain((0..sm.smart_filters.len()).rev().map(Some));
    for index in rows {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), if index.is_none() { header_height } else { SUB_ROW_H }), Sense::click_and_drag());
        // Off screen: keep the row's place but skip its painting, widgets and drop (#13118), as
        // effect rows do. The drawn marker below still records `l`, so a selected filter that
        // scrolled away stays selected.
        if !ui.is_rect_visible(rect) {
            continue;
        }
        if let Some((to, action)) = reorder_drop(ui, l, sm.smart_filters.len(), index, rect) {
            // The moved filter ends up selected, with its smart object active, as after a click.
            ui.ctx().data_mut(|d| d.insert_temp::<FilterDrag>(selected_key(doc.id), (l.id.0, Some(to))));
            actions.push(action);
            if !active {
                actions.push(("layer.select".into(), json!({"layer": l.id.0})));
            }
        }
        // Selected as in Photoshop: the row stays highlighted while its smart object is active.
        let selected = active && picked == Some((l.id.0, index));
        if selected {
            ui.painter().rect_filled(rect, 0.0, if t.pro { t.row_selected } else { t.hover });
        } else if resp.contains_pointer() {
            ui.painter().rect_filled(rect, 0.0, t.hover.gamma_multiply(0.35));
        }
        if t.pro {
            ui.painter().line_segment([pos2(rect.left() + 30.0, rect.top()), pos2(rect.left() + 30.0, rect.bottom())], Stroke::new(1.0, t.separator));
        }
        let (name, on, shown) = filter_row_label(sm, index);
        let eye = Rect::from_center_size(pos2(rect.left() + indent + if index.is_none() { 9.0 } else { 41.0 }, rect.center().y), vec2(18.0, 18.0));
        let eye_resp = ui.interact(eye, ui.scope_id().with(("sf-eye", l.id.0, index)), Sense::click());
        if shown {
            icons::paint(ui, eye, "eye", 12.0, if on { t.icon } else { t.text_faint });
        }
        eye_resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, shown, format!("{} {name}", tl!("Visibility"))));
        if eye_resp.clicked() {
            actions.push(match index {
                None => ("layer.smartFilter.disableSmartFilters".into(), json!({"layer": l.id.0})),
                Some(i) => ("layer.smartFilter.setVisible".into(), json!({"layer": l.id.0, "index": i})),
            });
        }
        // The shared mask scales with the thumbnail setting (hidden at None); the header name
        // follows it, while filter names stay aligned past their eyes.
        let mask_left = rect.left() + indent + 28.0;
        let x = filter_label_x(rect.left(), indent, mask_size, index);
        if index.is_none() && mask_size > 0.0 {
            let sw = Rect::from_min_size(pos2(mask_left, rect.center().y - mask_size / 2.0), vec2(mask_size, mask_size));
            if let Some(mask) = &sm.filter_mask {
                let tex = app.mask_thumb_kind(ui.ctx(), doc, l.id, mask, crate::mask_thumbs_ui::THUMB_SMART_MASK);
                ui.painter().image(tex, sw, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                if !mask.enabled {
                    ui.painter().line_segment([sw.left_top(), sw.right_bottom()], Stroke::new(1.0, t.danger));
                    ui.painter().line_segment([sw.right_top(), sw.left_bottom()], Stroke::new(1.0, t.danger));
                }
            } else {
                ui.painter().rect_filled(sw, 0.0, Color32::WHITE);
            }
            ui.painter().rect_stroke(sw, 1.0, Stroke::new(1.0, t.field_border), egui::StrokeKind::Inside);
            if app.ui.filter_mask_target == Some(l.id.0) && app.filter_mask_targeted() {
                crate::mask_thumbs_ui::paint_brackets(ui.painter(), sw, t.text);
            }
            let sw_resp = ui.interact(sw.expand(2.0), ui.scope_id().with(("sf-mask", l.id.0)), Sense::click());
            if sw_resp.clicked() {
                actions.extend(filter_mask_click(l, sm, ui.input(|i| i.modifiers)));
            }
        }
        let blend = index.map(|_| Rect::from_center_size(pos2(rect.right() - crate::layer_row_ui::RIGHT_PAD - 9.0, rect.center().y), vec2(18.0, 18.0)));
        let right = blend.map_or(rect.right() - crate::layer_row_ui::RIGHT_PAD, |r| r.left() - 4.0);
        crate::layer_row_ui::label(
            ui.painter(),
            x,
            rect.center().y,
            right,
            &name,
            egui::FontId::proportional(11.5),
            if on { t.text_dim } else { t.text_faint },
        );
        // A filter's name is its own target, apart from the Blending Options icon (#3283): it
        // selects, drags (#4271) and opens the filter's editor on a double-click. The row around
        // it does the same, but only the name carries the row's label and selected state.
        let name_resp = index.map(|i| {
            let name_rect = Rect::from_min_max(pos2(x.min(right), rect.top()), pos2(right, rect.bottom()));
            ui.interact(name_rect, ui.scope_id().with(("sf-name", l.id.0, i)), Sense::click_and_drag())
        });
        match &name_resp {
            Some(r) => r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &name)),
            None => resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &name)),
        }
        let hit = |f: fn(&egui::Response) -> bool| f(&resp) || name_resp.as_ref().is_some_and(f);
        if hit(egui::Response::drag_started) {
            ui.ctx().data_mut(|d| d.insert_temp::<FilterDrag>(drag_key(), (l.id.0, index)));
        }
        if hit(egui::Response::clicked) {
            ui.ctx().data_mut(|d| d.insert_temp::<FilterDrag>(selected_key(doc.id), (l.id.0, index)));
            if !active {
                actions.push(("layer.select".into(), json!({"layer": l.id.0})));
            }
        }
        if let Some(i) = index
            && hit(egui::Response::double_clicked)
            && (active || select_before_editor(app, l.id.0, actions))
        {
            open_editor(app, ui.ctx(), l.id.0, sm, i);
        }
        if let (Some(i), Some(r)) = (index, blend) {
            let blend_resp = ui.interact(r, ui.scope_id().with(("sf-blend", l.id.0, i)), Sense::click());
            icons::paint(ui, r, "sliders-horizontal", 13.0, if blend_resp.hovered() { t.text } else { t.icon });
            let label = crate::i18n::fmt(tl!("Blending Options ({filter})"), &[("filter", &name)]);
            blend_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
            if blend_resp.double_clicked()
                && let Err(e) = crate::smart_filter_dialog::open(app, l.id, i)
            {
                crate::notices::error(app, e);
            }
            blend_resp.on_hover_text(label);
        }
    }
    // Every row of `l` is drawn, so a filter of it selected (now, after this pass's clicks) is on
    // screen: see [`forget_hidden`].
    if ui.ctx().data(|d| d.get_temp::<FilterDrag>(selected_key(doc.id))).is_some_and(|(layer, _)| layer == l.id.0) {
        let pass = ui.ctx().cumulative_pass_nr();
        ui.ctx().data_mut(|d| d.insert_temp(drawn_key(doc.id), pass));
    }
}

fn filter_row_label(sm: &SmartObject, index: Option<usize>) -> (String, bool, bool) {
    match index {
        None => (tl!("Smart Filters").to_string(), sm.filters_enabled, sm.filters_enabled),
        Some(i) => {
            let filter = &sm.smart_filters[i];
            (filter_label(filter), filter.visible && sm.filters_enabled, filter.visible)
        }
    }
}

fn filter_label_x(left: f32, indent: f32, mask_size: f32, index: Option<usize>) -> f32 {
    match index {
        None if mask_size > 0.0 => left + indent + 28.0 + mask_size + 8.0,
        None => left + indent + 28.0,
        Some(_) => left + indent + 60.0,
    }
}

>>>>>>> upstream/main
/// What a click on the filter mask thumbnail does: target it for painting (selecting the smart
/// object), ⌥ view it alone (⇧⌥ as an overlay), ⇧ disable or enable it.
pub(crate) fn filter_mask_click(l: &Layer, m: egui::Modifiers) -> Vec<(String, Value)> {
    let LayerContent::Smart(sm) = &l.content else { return Vec::new() };
    let layer = l.id.0;
    if m.alt {
        let mode = if m.shift { "toggleOverlay" } else { "toggleGray" };
        return sm
            .filter_mask
            .is_some()
            .then(|| (photocraft_engine::mask_view_cmds::ID.to_string(), json!({"layer": layer, "mode": mode, "mask": "filter"})))
            .into_iter()
            .collect();
    }
    if m.shift {
        return sm
            .filter_mask
            .as_ref()
            .map(|f| ("layer.smartFilter.disableFilterMask".to_string(), json!({"layer": layer, "enabled": !f.enabled})))
            .into_iter()
            .collect();
    }
    vec![("layer.select".into(), json!({"layer": layer})), ("ui.filterMaskTarget".into(), json!(layer))]
}

/// A Smart Filter row selected on its own: the smart object and the filter index (`None` = the
/// Smart Filters row, all of them).
pub(crate) type FilterDrag = (u64, Option<usize>);

/// The Smart Filter row each document has selected on its own.
pub(crate) type FilterPicks = std::collections::HashMap<photocraft_doc::DocId, FilterDrag>;

/// The selected smart filter row, while its smart object is active and the filter still exists.
pub(crate) fn selected(picks: &FilterPicks, doc: &Document, active: Option<LayerId>) -> Option<FilterDrag> {
    let (layer, filter) = *picks.get(&doc.id)?;
    if active != Some(LayerId(layer)) {
        return None;
    }
    let LayerContent::Smart(sm) = &doc.layer(LayerId(layer))?.content else { return None };
    let exists = match filter {
        Some(i) => i < sm.smart_filters.len(),
        None => !sm.smart_filters.is_empty(),
    };
    exists.then_some((layer, filter))
}

/// Selects a Smart Filter row of document `doc` on its own: `(smart object, filter index)`, `None`
/// for the Smart Filters row.
pub(crate) fn pick(picks: &mut FilterPicks, doc: photocraft_doc::DocId, row: FilterDrag) {
    picks.insert(doc, row);
}

/// Forgets the selected smart filter row: its smart object's own row was clicked, the filter was
/// deleted (its index would now name the next one) or the panel's filter hides its row.
pub(crate) fn deselect(picks: &mut FilterPicks, doc: photocraft_doc::DocId) {
    picks.remove(&doc);
}

/// The command for dropping the smart filters of `from` on the smart object `target`: they move
/// there, or with ⌥ held on release are copied (Photoshop).
pub(crate) fn drop_action(from: u64, filter: Option<usize>, target: LayerId, copy: bool) -> (String, Value) {
    let mut payload = json!({"from": from, "to": target.0, "copy": copy});
    if let (Some(i), Some(o)) = (filter, payload.as_object_mut()) {
        o.insert("filter".into(), json!(i));
    }
    ("layer.smartFilter.transfer".into(), payload)
}

<<<<<<< HEAD
=======
/// Another smart object's row accepts the smart filters being dragged: outlined while the pointer
/// is over it, dropped on release. Other layers don't take them, as in Photoshop.
pub(crate) fn drop_on_row(ctx: &egui::Context, ui: &egui::Ui, l: &Layer, rect: Rect, actions: &mut Vec<(String, Value)>) {
    let Some((from, filter)) = ctx.data(|d| d.get_temp::<FilterDrag>(drag_key())) else { return };
    let Some(p) = ctx.input(|i| i.pointer.interact_pos()) else { return };
    if from == l.id.0 || !matches!(l.content, LayerContent::Smart(_)) || !rect.contains(p) {
        return;
    }
    let t = Tokens::get(ctx);
    ui.painter().rect_stroke(rect.shrink(1.0), t.radius_sm, Stroke::new(2.0, t.accent), StrokeKind::Inside);
    if ctx.input(|i| i.pointer.any_released()) {
        actions.push(drop_action(from, filter, l.id, ctx.input(|i| i.modifiers.alt)));
    }
}

/// Where `layer.smartFilter.move` puts filter `src` (stack index, 0 = bottom) dropped on the row
/// of filter `row` (`None`: the Smart Filters row, i.e. on top of the stack), `above` or below
/// it, in a stack of `n`. `None` when the filter would stay where it is.
fn reorder_target(src: usize, row: Option<usize>, above: bool, n: usize) -> Option<usize> {
    let to = match row {
        None => n.checked_sub(1)?,
        Some(i) if i == src => return None,
        // Above a row is just over it in the stack, below a row just under it. `move` removes
        // the filter first, so the filters over `src` shift down by one.
        Some(i) if above => {
            if src < i {
                i
            } else {
                i + 1
            }
        }
        Some(i) => {
            if src < i {
                i - 1
            } else {
                i
            }
        }
    };
    (to < n && to != src).then_some(to)
}

/// A filter dragged up or down its own smart object's stack (Photoshop): over another row of the
/// stack a line marks where it lands, above or below that row (the Smart Filters row: on top),
/// and the release moves it there. The bottom filter is applied first, so the order changes the
/// result (Unsharp Mask over a blur sharpens the blurred image; under it, the sharpening is
/// blurred). Returns the filter's new index and the command.
fn reorder_drop(ui: &egui::Ui, l: &Layer, n: usize, row: Option<usize>, rect: Rect) -> Option<(usize, (String, Value))> {
    let ctx = ui.ctx();
    let (from, src) = ctx.data(|d| d.get_temp::<FilterDrag>(drag_key()))?;
    let (src, p) = (src?, ctx.input(|i| i.pointer.interact_pos())?);
    // Only the part of the row the list shows: one half scrolled under the panel's header takes
    // no drop there (#13118, as masks since #10485).
    if from != l.id.0 || !rect.intersect(ui.clip_rect()).contains(p) {
        return None;
    }
    let above = p.y < rect.center().y;
    let to = reorder_target(src, row, above, n)?;
    let t = Tokens::get(ctx);
    let y = if above && row.is_some() { rect.top() } else { rect.bottom() };
    ui.painter().line_segment([pos2(rect.left() + 30.0, y), pos2(rect.right(), y)], Stroke::new(2.0, t.accent));
    ctx.input(|i| i.pointer.any_released()).then(|| (to, ("layer.smartFilter.move".into(), json!({"layer": l.id.0, "index": src, "to": to}))))
}

>>>>>>> upstream/main
/// Smart filters dropped on the footer's Delete button: a filter's row deletes that filter, the
/// Smart Filters row clears them all.
pub(crate) fn trash_action((layer, filter): FilterDrag) -> (String, Value) {
    match filter {
        Some(i) => ("layer.smartFilter.delete".into(), json!({"layer": layer, "index": i})),
        None => ("layer.smartFilter.clearSmartFilters".into(), json!({"layer": layer})),
    }
}

/// Delete or Backspace with no modifiers while a filter's row is selected removes that filter (the
/// Smart Filters row: all of them), as the panel's Delete button does (#4751), instead of
/// deleting the smart object or reaching Edit › Clear.
pub(crate) fn keys(app: &mut PhotocraftApp, ctx: &egui::Context) -> bool {
    // A text edit keeps its keys. Text fields and widgets that delete with these keys themselves
    // take them before the key reaches here (#1534).
    if app.ui.text_edit.is_some() {
        return false;
    }
    let Some(doc) = app.session.active().map(|st| st.doc.id) else { return false };
    let Some(picked) = crate::panels::listed_filter(app) else { return false };
    // Only where the key would delete the smart object: Quick Mask, a single channel, a targeted
    // mask or a selection keep their own Delete.
    if crate::shortcut_dispatch::delete_key_command(app, "edit.clear".into()) != "layer.delete" {
        return false;
    }
    let mods = ctx.input(|i| i.modifiers);
    if !mods.is_none() || !ctx.input_mut(|i| i.consume_key(mods, egui::Key::Delete) || i.consume_key(mods, egui::Key::Backspace)) {
        return false;
    }
    let (command, params) = trash_action(picked);
    deselect(&mut app.smart_filter_picks, doc);
    if let Err(e) = app.run(&command, params) {
        app.ui.status = e;
        app.ui.status_error = true;
    }
    true
}

/// Re-opens a smart filter's dialog with its recorded parameters; OK runs
/// `layer.smartFilter.setParams` instead of adding another filter (see [`confirm_command`]).
pub fn open_editor(app: &mut PhotocraftApp, ctx: &egui::Context, layer: u64, sm: &SmartObject, index: usize) {
    let Some(f) = sm.smart_filters.get(index) else { return };
    // Camera Raw has its own full-window dialog rather than a generic parameter form.
    if f.command == photocraft_engine::lens_cmds::RAW {
        if let Err(e) = crate::camera_raw_ui::open_smart_filter(app, ctx, photocraft_doc::LayerId(layer), index) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
        return;
    }
    // So has the Filter Gallery.
    if f.command == "filter.filterGallery" {
        if let Err(e) = crate::gallery_ui::open_smart_filter(app, photocraft_doc::LayerId(layer), index) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
        return;
    }
    // And Liquify, which re-opens with the filter's recorded strokes.
    if f.command == photocraft_engine::distort_cmds::LIQUIFY {
        if let Err(e) = crate::liquify_ui::open_smart_filter(app, ctx, photocraft_doc::LayerId(layer), index) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
        return;
    }
    let target = match crate::smart_filter_dialog::target(app, photocraft_doc::LayerId(layer), index) {
        Ok(target) => target,
        Err(e) => {
            crate::notices::error(app, e);
            return;
        }
    };
    if crate::adjust_dialog::open_smart_filter(app, &f.command, &f.params, target.clone()).is_some() || !crate::filter_dialog::has_dialog(&f.command) {
        return;
    }
    // The hosted filter dialog carries the target as its captured value; the recorded parameters
    // overlay the schema defaults, not the user's last-used values.
    if let Err(e) = crate::filter_dialog::open_smart_edit(app, &f.command, &f.params.as_object().cloned().unwrap_or_default(), target) {
        crate::notices::error(app, e);
    }
}

/// The command a hosted filter dialog's live preview runs ([`confirm_command`]'s): the Smart Filter
/// blending command for the dialog `key` names (its Blending Options dialog),
/// `layer.smartFilter.setParams` while the dialog edits a smart filter (`captured` is a smart-filter
/// target, #6324), else `command`.
pub fn preview_command<'a>(key: &str, command: &'a str, captured: &Value) -> &'a str {
    if crate::smart_filter_dialog::owns(key) {
        crate::smart_filter_dialog::COMMAND
    } else if crate::smart_filter_dialog::is_target(captured) {
        "layer.smartFilter.setParams"
    } else {
        command
    }
}

/// The command a confirmed filter dialog runs: smart-filter edits update the recorded filter.
pub fn confirm_command(app: &PhotocraftApp, captured: &Value, command: String, params: Value) -> Result<(String, Value), String> {
    // One builder for the preview and for OK, so the two cannot differ.
    if crate::smart_filter_dialog::is_target(captured) {
        return crate::smart_filter_dialog::params_command(app, captured, &command, params);
    }
    Ok((command, params))
}

#[cfg(test)]
mod editor_tests;

#[cfg(test)]
mod tests {
    use photocraft_engine::Session;

    use super::*;
    use crate::Services;

    fn smart(app: &PhotocraftApp) -> (photocraft_doc::Layer, photocraft_doc::SmartObject) {
        let st = app.session.active().unwrap();
        let l = st.doc.layer(st.active_layer.unwrap()).unwrap().clone();
        let LayerContent::Smart(sm) = l.content.clone() else { panic!("not a smart object") };
        (l, sm)
    }

    /// FILE-215-3: clicking the filter mask thumbnail targets it, the Brush paints into it (not
    /// the layer mask, no rasterize prompt), and clicking the layer thumbnail goes back.
    #[test]
    fn the_filter_mask_thumbnail_targets_the_filter_mask() {
        let mut app = PhotocraftApp::new(Session::new(), Services::default());
        app.run("file.new", json!({"width": 32, "height": 32})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        app.run("edit.fill", json!({"contents": "color", "color": "#ff0000"})).unwrap();
        app.run("layer.smartObjects.convertToSmartObject", json!({})).unwrap();
        app.run("filter.blur.gaussianBlur", json!({"radius": 2})).unwrap();
        let (l, _) = smart(&app);
        let actions = super::filter_mask_click(&l, egui::Modifiers::NONE);
        assert_eq!(actions.last().unwrap().0, "ui.filterMaskTarget");
        assert!(super::filter_mask_click(&l, egui::Modifiers::ALT).is_empty(), "nothing to view yet");
        app.ui.filter_mask_target = Some(l.id.0);
        app.sync_views();
        assert!(app.filter_mask_targeted());
        assert_eq!(crate::canvas::paint_target(&app), json!("filterMask"));
        assert_eq!(app.with_mask_target("filter.blur.gaussianBlur", json!({}))["target"], "filterMask");
        assert!(crate::rasterize_prompt::needed(&app).is_none(), "painting the filter mask needs no rasterizing");
        app.run(
            "paint.stroke",
            json!({"points": [[4, 0], [4, 31]], "size": 4, "hardness": 1, "color": "#000000", "target": crate::canvas::paint_target(&app)}),
        )
        .unwrap();
        let (l, sm) = smart(&app);
        assert_eq!(sm.filter_mask.as_ref().unwrap().value(4, 16), 0.0);
        assert!(l.mask.is_none(), "no layer mask");
        assert_eq!(super::filter_mask_click(&l, egui::Modifiers::ALT)[0].1["mask"], "filter");
        // Targeting a layer mask (or the pixels, `ui.maskTarget`) ends it.
        app.ui.mask_target = true;
        app.sync_views();
        assert_eq!(app.ui.filter_mask_target, None, "a layer mask target replaces it");
    }

    #[test]
    fn selecting_the_filter_mask_replaces_a_layer_mask_view() {
        let mut app = PhotocraftApp::new(Session::new(), Services::default());
        app.run("file.new", json!({"width": 32, "height": 32})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        app.run("edit.fill", json!({"contents": "color", "color": "#ff0000"})).unwrap();
        app.run("layer.smartObjects.convertToSmartObject", json!({})).unwrap();
        app.run("filter.blur.gaussianBlur", json!({"radius": 2})).unwrap();
        app.run("layer.layerMask.revealAll", json!({})).unwrap();
        app.run("view.layerMask", json!({"mode": "gray"})).unwrap();
        app.sync_mask_targets();
        assert!(app.layer_mask_targeted());
        let id = app.session.active().unwrap().active_layer.unwrap();
        app.select_filter_mask_target(Some(id.0));
        app.sync_views();
        assert!(app.filter_mask_targeted());
        assert!(!app.layer_mask_targeted());
        assert!(photocraft_engine::mask_view_cmds::current(app.session.active().unwrap()).is_none());
        app.run("paint.stroke", json!({"points": [[8, 8]], "size": 6, "hardness": 1, "color": "#000000"})).unwrap();
        let st = app.session.active().unwrap();
        let layer = st.doc.layer(id).unwrap();
        assert_eq!(layer.mask.as_ref().unwrap().value(8, 8), 1.0);
        let LayerContent::Smart(sm) = &layer.content else { panic!("smart object") };
        assert_eq!(sm.filter_mask.as_ref().unwrap().value(8, 8), 0.0);
        app.run("view.layerMask", json!({"mode": "gray", "mask": "filter"})).unwrap();
        app.select_filter_mask_target(Some(id.0));
        assert!(photocraft_engine::mask_view_cmds::current(app.session.active().unwrap()).unwrap().filter, "a matching filter-mask view stays open");
    }

    /// #5689: an Image › Adjustments recorded on a smart object (Brightness/Contrast…) opens its
    /// own editor from the Smart Filters row, with the recorded settings, and OK updates that
    /// filter instead of stacking another.
    #[test]
    fn an_adjustment_smart_filter_opens_its_editor_and_updates_it() {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": 64, "height": 48})).unwrap();
        s.execute("layer.new.layer", json!({})).unwrap();
        s.execute("edit.fill", json!({"contents": "black"})).unwrap();
        let id = s.execute("layer.smartObjects.convertToSmartObject", json!({})).unwrap()["layer"].as_u64().unwrap();
        s.execute("image.adjustments.brightnessContrast", json!({"brightness": 40, "contrast": -10})).unwrap();
        let mut app = PhotocraftApp::new(s, crate::Services::default());
        let sm = match &app.session.active().unwrap().doc.layer(photocraft_doc::LayerId(id)).unwrap().content {
            LayerContent::Smart(sm) => sm.clone(),
            other => panic!("expected a smart object, got {other:?}"),
        };
        assert_eq!(sm.smart_filters.len(), 1);
        let app_ctx = app.ctx().clone();
        open_editor(&mut app, &app_ctx, id, &sm, 0);
        crate::dialogs::testing::show(&mut app);
        let d = app.framework.dialogs.open_dialogs().pop().expect("the adjustment editor opened");
        // An adjustment dialog shows its engine parameters, the form its command takes.
        let fields = crate::adjust_dialog::command_params(&app, d.id);
        assert_eq!(fields["brightness"].as_f64(), Some(40.0), "the recorded settings");
        assert_eq!(fields["contrast"].as_f64(), Some(-10.0));
        let dialog = d.id;
        // OK updates the recorded filter (one history step), it does not add a second filter.
        crate::dialogs::testing::confirm(&mut app, dialog);
        let LayerContent::Smart(sm) = &app.session.active().unwrap().doc.layer(photocraft_doc::LayerId(id)).unwrap().content else {
            panic!("the layer stays a smart object");
        };
        assert_eq!(sm.smart_filters.len(), 1, "no filter was stacked");
        assert_eq!(sm.smart_filters[0].params.get("brightness").and_then(Value::as_f64), Some(40.0));
        assert_eq!(sm.smart_filters[0].params.get("contrast").and_then(Value::as_f64), Some(-10.0));
    }

    /// #11463: a Selective Color smart filter recorded in the single-colour form, re-edited. Its
    /// old `cyan` and `relative` must not survive replacement and override the edit when the filter
    /// renders and reopens.
    #[test]
    fn a_selective_color_smart_filter_in_the_old_form_keeps_its_edit() {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": 64, "height": 48})).unwrap();
        s.execute("layer.new.layer", json!({})).unwrap();
        s.execute("edit.fill", json!({"contents": "color", "color": "#2030c0"})).unwrap();
        let id = s.execute("layer.smartObjects.convertToSmartObject", json!({})).unwrap()["layer"].as_u64().unwrap();
        s.execute("image.adjustments.selectiveColor", json!({"colors": "blues", "cyan": 30, "relative": false})).unwrap();
        let mut app = PhotocraftApp::new(s, crate::Services::default());
        app.background_jobs = false;
        let recorded = |app: &PhotocraftApp| match &app.session.active().unwrap().doc.layer(photocraft_doc::LayerId(id)).unwrap().content {
            LayerContent::Smart(sm) => sm.clone(),
            other => panic!("expected a smart object, got {other:?}"),
        };
        let sm = recorded(&app);
        assert_eq!(sm.smart_filters[0].params.get("cyan").and_then(Value::as_f64), Some(30.0), "recorded in the old form");
        let mut h = egui_kittest::Harness::builder()
            .with_context(app.ctx().clone())
            .with_size(egui::vec2(1200.0, 900.0))
            .build_ui_state(|ui, app| crate::dialogs::run(app, ui), app);
        PhotocraftApp::setup_context(&h.ctx, crate::theme::AppTheme::pro());
        let ctx = h.ctx.clone();
        // The open dialog's fields, whatever its type.
        let fields_of = |app: &PhotocraftApp, dialog| match crate::adjust_dialog::hosted(app, dialog) {
            Some(_) => crate::adjust_dialog::command_params(app, dialog),
            None => app.framework.dialogs.open_dialogs().into_iter().find(|d| d.id == dialog).and_then(|d| d.fields).unwrap(),
        };
        open_editor(h.state_mut(), &ctx, id, &sm, 0);
        h.run_steps(2);
        let dialog = h.state().framework.dialogs.open_dialogs().pop().unwrap().id;
        h.run_steps(3);
        let fields = fields_of(h.state(), dialog);
        assert!(fields.get("cyan").is_none() && fields.get("relative").is_none(), "folded: {fields:?}");
        assert_eq!(fields["colors"], json!("blues"), "normalization keeps the selected colour");
        assert_eq!(fields["blues"][0].as_f64(), Some(30.0));
        assert_eq!(fields["method"], json!("absolute"));
        // Edit the row and the Method, then OK.
        let mut edited = crate::adjust_dialog::command_params(h.state(), dialog);
        edited["blues"] = json!([10.0, 0.0, 0.0, 0.0]);
        edited["method"] = json!("relative");
        assert!(crate::adjust_dialog::apply_params(h.state_mut(), dialog, &edited), "the dialog takes the edit");
        h.run_steps(2);
        crate::dialogs::testing::confirm(h.state_mut(), dialog);
        let sm = recorded(h.state());
        assert_eq!(sm.smart_filters.len(), 1, "no filter was stacked");
        let photocraft_doc::Adjustment::SelectiveColor { relative, adjustments } =
            photocraft_engine::adjust_cmds::selective_from_params(&sm.smart_filters[0].params, None).unwrap()
        else {
            panic!("Selective Color params");
        };
        assert!(relative, "the edited Method renders");
        assert_eq!(adjustments[4], [10.0, 0.0, 0.0, 0.0], "the edited row renders");
        // And reopens as edited.
        open_editor(h.state_mut(), &ctx, id, &sm, 0);
        h.run_steps(2);
        let dialog = h.state().framework.dialogs.open_dialogs().pop().unwrap().id;
        h.run_steps(3);
        let fields = fields_of(h.state(), dialog);
        assert_eq!(fields["colors"], json!("blues"), "reopening keeps the selected colour");
        assert_eq!(fields["blues"][0].as_f64(), Some(10.0));
        assert_eq!(fields["method"], json!("relative"));
    }

    /// #6324: a smart filter opened for editing previews, and its preview replaces the filter in
    /// the stack (as OK does) instead of stacking a second one.
    #[test]
    fn an_edited_smart_filter_previews_what_ok_gives() {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": 64, "height": 48, "background": "#ffffff"})).unwrap();
        s.execute("layer.new.layer", json!({})).unwrap();
        s.execute("select.all", json!({})).unwrap();
        s.execute("edit.stroke", json!({"width": 4, "color": "#000000", "location": "inside"})).unwrap();
        s.execute("select.deselect", json!({})).unwrap();
        let id = s.execute("layer.smartObjects.convertToSmartObject", json!({})).unwrap()["layer"].as_u64().unwrap();
        s.execute("filter.blur.gaussianBlur", json!({"radius": 1.0})).unwrap();
        let mut app = PhotocraftApp::new(s, crate::Services::default());
        app.background_jobs = false;
        let smart = |app: &PhotocraftApp, doc: &Document| match &doc.layer(LayerId(id)).unwrap().content {
            LayerContent::Smart(sm) => sm.clone(),
            other => panic!("expected a smart object, got {other:?} ({})", app.ui.status),
        };
        let sm = smart(&app, &app.session.active().unwrap().doc);
        let ctx = app.ctx().clone();
        open_editor(&mut app, &ctx, id, &sm, 0);
        crate::dialogs::testing::show(&mut app);
        let open = app.framework.dialogs.open_dialogs().pop().expect("the filter's dialog opened");
        let (dialog, key) = (open.id, open.key.unwrap_or_default());
        app.framework.dialogs.set_json_field(&ctx, dialog, "radius", json!(6.0)).unwrap();

        crate::canvas::ensure_filter_preview(&mut app, 0, &ctx).expect("a preview");
        // The target builder takes the app mutably, so it runs before the preview is borrowed.
        let captured = crate::smart_filter_dialog::target(&mut app, LayerId(id), 0).unwrap();
        let preview = app.filter_preview.as_ref().unwrap();
        assert_eq!(preview.key.command, "layer.smartFilter.setParams");
        // The dialog's own preview window matches it to the dialog by that command.
        assert_eq!(preview_command(&key, "filter.blur.gaussianBlur", &captured), preview.key.command);
        assert_eq!(preview_command(&key, "filter.blur.gaussianBlur", &Value::Null), "filter.blur.gaussianBlur");
        let shown = preview.result.clone().expect("the preview's document");
        let filters = smart(&app, &shown).smart_filters;
        assert_eq!(filters.len(), 1, "the edited filter is replaced, not stacked");
        assert_eq!(filters[0].params.get("radius").and_then(Value::as_f64), Some(6.0));
        assert_eq!(smart(&app, &app.session.active().unwrap().doc).smart_filters[0].params["radius"], json!(1.0), "the document is untouched");

        crate::dialogs::testing::confirm(&mut app, dialog);
        let doc = &app.session.active().unwrap().doc;
        assert_eq!(smart(&app, doc).smart_filters.len(), 1);
        assert!(photocraft_compose::flatten(&shown).px == photocraft_compose::flatten(doc).px, "OK gives what the preview showed");
    }

    /// FILE-215-3: with the filter mask targeted, Move and Edit › Transform act on the smart object
    /// (no filter-mask target, so a move doesn't re-render the filters), and painting and filters
    /// still edit the filter mask.
    #[test]
    fn only_filter_mask_edits_carry_the_filter_mask_target() {
        let mut app = PhotocraftApp::new(Session::new(), Services::default());
        app.run("file.new", json!({"width": 32, "height": 32})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        app.run("edit.fill", json!({"contents": "color", "color": "#ff0000"})).unwrap();
        app.run("layer.smartObjects.convertToSmartObject", json!({})).unwrap();
        app.run("filter.blur.gaussianBlur", json!({"radius": 2})).unwrap();
        let id = app.session.active().unwrap().active_layer.unwrap();
        app.select_filter_mask_target(Some(id.0));
        assert!(app.filter_mask_targeted());
        for cmd in [
            "layer.translate",
            "select.float",
            "edit.transform.rotate180",
            "filter.convertForSmartFilters",
            "image.applyImage",
            "filter.liquify",
            "filter.lastFilter",
            "image.adjustments.desaturate",
            "filter.render.flame",
        ] {
            assert!(app.with_mask_target(cmd, json!({"dx": 1})).get("target").is_none(), "{cmd}");
        }
        for cmd in [
            "paint.stroke",
            "filter.blur.gaussianBlur",
            "edit.fill",
            "edit.paste",
            "image.adjustments.invert",
            "image.adjustments.equalize",
            "filter.blur.average",
        ] {
            assert_eq!(app.with_mask_target(cmd, json!({}))["target"], "filterMask", "{cmd}");
        }
    }
}
