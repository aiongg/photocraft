use super::layer_drag::{DrawnRow, drag_feedback, dropped_on, edge_scroll, effect_rows, footer_drop, fx_trash_drop, held, layer_drop};
use super::layer_row::{RowSel, layer_row};
use craft_ui::composites::PanelFooter;
use craft_ui::controls::{ControlSize, IconButton, NumberField, Select};
use craft_ui::edit::EditEvent;
use craft_ui::icons::Icon;
use craft_ui::localized::{LocaleNumbers, Unit};
use craft_ui::theme::{Tokens as LibTokens, TokensOf as _};
use std::hash::{Hash, Hasher};

use super::options_bar::{LAYER_PCT_W, body_text_width, pct_action};
use super::{BlendMode, Layer, LayerId, PhotocraftApp, Rect, Sense, Tokens, Value, empty, json, label, pos2, theme, vec2, widgets};

// ----------------------------------------------------------------------------- layers

/// The simple themes' single lock button. On the Background it converts the layer to a normal
/// one, as clicking the Background's lock does in Photoshop. Elsewhere it clears every lock when
/// any is set, and locks all otherwise. Toggling only "lock all" would leave the Background's
/// transparency lock (still showing as locked), and a second click would lock its pixels,
/// shutting out the Eraser and every other paint tool (#76).
/// The Basic layout's lock toggle: the active layer's state decides, and every target follows it
/// (`targets`: `{"layer": id}` or `{"layers": [ids]}`).
pub(super) fn simple_lock_toggle(background: bool, l: &Layer, targets: &Value) -> (String, Value) {
    if background {
        return ("layer.new.layerFromBackground".into(), json!({}));
    }
    let locks =
        if l.locks.any() { json!({"transparency": false, "pixels": false, "position": false, "artboard": false, "all": false}) } else { json!({"all": true}) };
    let mut p = targets.clone();
    p["locks"] = locks;
    ("layer.setProps".into(), p)
}

/// The Layers panel's blend modes: Photoshop's, plus the layer's own Paint.NET mode when it has one
/// (an imported `.pdn`), so the menu matches Photoshop's for every other layer.
pub(crate) fn blend_options(groups: bool, current: BlendMode) -> Vec<(BlendMode, &'static str)> {
    let own = Some(current).filter(|m| !m.has_psd_equivalent());
    std::iter::once(BlendMode::PassThrough).filter(|_| groups).chain(BlendMode::LAYER_MODES).chain(own).map(|m| (m, m.label())).collect()
}

/// Fit blend mode + Opacity into the available Layers-panel width (#2309).
/// Preserve the normal translated label when there is room; on a narrow dock,
/// keep the field interactive and give the dropdown the remaining width.
pub(super) fn opacity_row_layout(available: f32, label_width: f32, gap: f32) -> (f32, bool) {
    let full_right = label_width + LAYER_PCT_W + 2.0 * gap + 16.0;
    // The label shows while the blend dropdown keeps a usable width (at the default dock width
    // with an English label it does; a long translated label on a narrow dock gives way).
    let show_label = available >= full_right + 80.0;
    let reserved = LAYER_PCT_W + gap + if show_label { label_width + gap + 16.0 } else { 0.0 };
    ((available - reserved).max(40.0), show_label)
}

/// A percentage field's edit this frame: the new percentage and, inside a gesture, a number that
/// is the same for every frame of it (the history coalesces on it).
type PctEdit = Option<(f32, Option<u64>)>;

/// The gesture number of an edit event, for coalescing its frames into one history step.
fn gesture_of<T>(event: &EditEvent<T>) -> Option<u64> {
    event.id().map(|g| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        g.hash(&mut hasher);
        hasher.finish()
    })
}

/// Turns a field's or select's `event` into the value to apply: updates and one-shot sets apply, a
/// commit applies when it changed something, and a cancel puts back the value the gesture began
/// with (kept in `start`).
fn edit_value<T: Clone + Send + Sync + 'static>(ui: &egui::Ui, start: egui::Id, event: Option<&EditEvent<T>>, current: &T) -> Option<(T, Option<u64>)> {
    match event? {
        EditEvent::Begin { .. } => {
            ui.data_mut(|d| d.insert_temp(start, current.clone()));
            None
        }
        e @ (EditEvent::Update { value, .. } | EditEvent::Set { value, .. }) => Some((value.clone(), gesture_of(e))),
        e @ EditEvent::Commit { value, changed: true, .. } => Some((value.clone(), gesture_of(e))),
        e @ EditEvent::Cancel { .. } => ui.data(|d| d.get_temp::<T>(start)).map(|before| (before, gesture_of(e))),
        _ => None,
    }
}

/// An Opacity or Fill field: the scrubby `label` (when `show_label`), then the number with its
/// pop-up slider, [`LAYER_PCT_W`] wide, right-aligned in the row.
fn pct_field(ui: &mut egui::Ui, salt: &str, label: &str, show_label: bool, pct: f32) -> PctEdit {
    let numbers = LocaleNumbers::of(ui);
    let name = label.trim_end_matches([':', '：']);
    let height = NumberField::style(&LibTokens::of(ui), ControlSize::Sm).field.height;
    let gap = ui.spacing().item_spacing.x;
    let label_w = if show_label { body_text_width(ui, label).min((ui.available_width() - LAYER_PCT_W - gap).max(0.0)) } else { 0.0 };
    let total = LAYER_PCT_W + if show_label { label_w + gap } else { 0.0 };
    let mut out = None;
    ui.allocate_ui_with_layout(vec2(total, height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        // Elided to the room left in the row (full text on hover): a longer translation shrinks
        // instead of running over the fixed controls beside it (#3580).
        let scrub = show_label.then(|| widgets::elided_label_sensed(ui, label, label_w, Sense::hover()));
        ui.allocate_ui_with_layout(vec2(LAYER_PCT_W, height), egui::Layout::left_to_right(egui::Align::Center).with_main_justify(true), |ui| {
            // The field is given the value as stored, rounded so that 53 does not come back as 53.000004.
            let shown = (f64::from(pct) * 1e4).round() / 1e4;
            let mut field = NumberField::new(salt, name, shown, &numbers)
                .range(0.0..=100.0)
                .decimals(0..=0)
                .unit(Unit::new("{n}%"))
                .or_parse(super::options_bar::reader("%", None))
                .popup_slider(name)
                .size(ControlSize::Sm);
            if let Some(scrub) = &scrub {
                field = field.scrub_by(scrub);
            }
            let edit = field.show(ui);
            out = edit_value(ui, edit.response.id.with("gesture-start"), edit.event.as_ref(), &f64::from(pct)).map(|(v, g)| (v as f32, g));
        });
    });
    out
}

/// What the blend-mode select did this frame.
#[derive(Default)]
struct BlendEdit {
    /// The mode chosen (a click, an arrow key or a wheel notch), with its gesture number.
    chosen: Option<(BlendMode, Option<u64>)>,
    /// The mode under the pointer in the open list.
    hovered: Option<BlendMode>,
}

/// The blend-mode select, `width` wide. Hovering a mode in the open list previews it on the
/// canvas (#970); a wheel notch over the closed box steps the mode (#1747).
fn blend_select(ui: &mut egui::Ui, current: BlendMode, options: &[(BlendMode, &str)], width: f32) -> BlendEdit {
    let height = NumberField::style(&LibTokens::of(ui), ControlSize::Sm).field.height;
    let mut out = BlendEdit::default();
    ui.allocate_ui_with_layout(vec2(width, height), egui::Layout::left_to_right(egui::Align::Center).with_main_justify(true), |ui| {
        let (mut value, mut hovered) = (current, None);
        let mut select = Select::new("blend", tl!("Blend Mode"), &mut value).size(ControlSize::Sm).wheel_steps(true).highlighted(&mut hovered);
        for (mode, label) in options {
            select = select.option(*mode, tl!(label));
        }
        let edit = select.show(ui);
        out.chosen = edit_value(ui, edit.response.id.with("gesture-start"), edit.event.as_ref(), &current);
        out.hovered = hovered;
    });
    out
}

/// What the Opacity row did this frame.
#[derive(Default)]
struct OpacityEdit {
    blend: BlendEdit,
    opacity: PctEdit,
}

/// Blend mode and Opacity. Greyed when the ui is disabled.
fn opacity_row(ui: &mut egui::Ui, pro: bool, blend: BlendMode, groups: bool, opacity: f32) -> OpacityEdit {
    let mut out = OpacityEdit::default();
    ui.horizontal(|ui| {
        // Reserve space for the numeric field first. On a narrow dock, hide the redundant visible
        // Opacity label rather than force the dropdown beyond the panel's right edge (#2309). The
        // numeric field keeps its name.
        let label = if pro { tl!("Opacity:") } else { tl!("Opacity") };
        let (blend_width, show_label) = opacity_row_layout(ui.available_width(), body_text_width(ui, label), ui.spacing().item_spacing.x);
        out.blend = blend_select(ui, blend, &blend_options(groups, blend), blend_width);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            out.opacity = pct_field(ui, "layer-opacity", label, show_label, opacity);
        });
    });
    out
}

/// What the Lock and Fill row did this frame.
#[derive(Default)]
struct LockEdit {
    /// The lock toggled (`transparency`, `pixels`, `position`, `artboard`, `all`) and its new state.
    lock: Option<(&'static str, bool)>,
    /// The simple themes' single lock button was clicked.
    simple: bool,
    fill: PctEdit,
}

/// The Lock row: the locks, in the order of `locks` (transparency, pixels, position, artboard,
/// all), and Fill. `any` is whether any lock is set. Greyed when the ui is disabled; `fill_enabled`
/// greys Fill alone.
fn lock_row(ui: &mut egui::Ui, pro: bool, locks: [bool; 5], any: bool, fill: f32, fill_enabled: bool) -> LockEdit {
    let mut out = LockEdit::default();
    ui.horizontal(|ui| {
        let lock_label = if pro { tl!("Lock:") } else { tl!("Lock") };
        // Layer fill opacity (≠ Edit › Fill / path fill → Tô).
        let lang = crate::i18n::current();
        let fill_label = if pro { crate::i18n::tr_ctx(lang, "layers", "Fill:") } else { crate::i18n::tr_ctx(lang, "layers", "Fill") };
        // In a narrow panel with long translated labels, drop the "Lock:" text (the icons keep
        // their tooltips) rather than let the Fill label run over the lock icons.
        let gap = ui.spacing().item_spacing.x;
        let lock_button = craft_ui::controls::IconButton::style(&LibTokens::of(ui), ControlSize::Sm).side;
        let icons_w = if pro { 5.0 * lock_button + gap } else { lock_button + gap };
        let fits = body_text_width(ui, lock_label) + icons_w + body_text_width(ui, fill_label) + LAYER_PCT_W + 2.0 * gap <= ui.available_width();
        if fits {
            label(ui, lock_label);
        }
        if pro {
            // The lock icons touch, but only they: the Fill label keeps the same gap before its
            // field as Opacity's, so both stack in one right-aligned column (#2085).
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (n, (key, icon, tip)) in [
                    ("transparency", Icon::Grid, tl!("Lock transparent pixels")),
                    ("pixels", Icon::Brush, tl!("Lock image pixels")),
                    ("position", Icon::Move, tl!("Lock position")),
                    ("artboard", Icon::Scan, tl!("Prevent auto-nesting in and out of Artboards and Frames")),
                    ("all", Icon::Lock, tl!("Lock all")),
                ]
                .into_iter()
                .enumerate()
                {
                    let on = locks[n];
                    // Lock Image Pixels includes Lock Transparent Pixels, which waits for it (#4892).
                    let enabled = key != "transparency" || !locks[1];
                    let button = IconButton::new(icon, tip).size(ControlSize::Sm).selected(on);
                    if ui.add_enabled_ui(enabled, |ui| ui.add(button)).inner.clicked() {
                        out.lock = Some((key, !on));
                    }
                }
            });
        } else {
            let icon = if any { Icon::Lock } else { Icon::LockOpen };
            out.simple = ui.add(IconButton::new(icon, tl!("Lock layer")).size(ControlSize::Sm).selected(any)).clicked();
        }
        ui.add_enabled_ui(fill_enabled, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                out.fill = pct_field(ui, "layer-fill", fill_label, true, fill);
            })
        });
    });
    out
}

/// Blend mode, Opacity, the locks and Fill with no layer selected: Photoshop keeps them in place,
/// greyed out at Normal, 100% and unlocked, instead of removing them (#8880). Same layout as the
/// active layer's rows in `layers`.
fn inactive_blend_rows(ui: &mut egui::Ui, pro: bool) {
    ui.add_enabled_ui(false, |ui| {
        opacity_row(ui, pro, BlendMode::Normal, false, 100.0);
        // Docks zero the item spacing: keep the Opacity and Fill fields apart (#155).
        ui.add_space((theme::ROW_GAP - ui.spacing().item_spacing.y).max(0.0));
        lock_row(ui, pro, [false; 5], false, 100.0, true);
        ui.add_space(craft_ui::theme::scale::Space::S2.px());
    });
}

pub(super) fn layers(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    // A new active layer opens its parent groups and is scrolled into view (#152).
    let reveal = crate::layer_reveal::track(app, ui.ctx());
    let Some(st) = app.session.active() else {
        empty(ui, tl!("No document"));
        return;
    };
    let doc = st.doc.clone();
    let active = st.active_layer;
    let active_layer = active.and_then(|id| doc.layer(id));
    let selection = st.selected_layers();
    let isolated = st.isolated_layers.clone();
    let mut actions: Vec<(String, Value)> = Vec::new();

    let t = Tokens::get(ui.ctx());
    app.reorder_preview_hit = false;
    if !ui.input(|i| i.pointer.primary_down()) || !app.ui.layers_panel_options.preview_on_canvas_when_reordering_layers {
        app.reorder_preview = None;
    }
    if t.pro && app.ui.layers_panel_options.show_filters {
        crate::layer_filter_ui::bar(app, ui);
        ui.add_space(craft_ui::theme::scale::Space::S4.px());
    }
    if let Some(l) = active_layer {
        // With several layers selected, blend mode, Opacity, Fill and the locks change every one
        // of them, as in Photoshop; the controls show the active layer's values.
        // The Background stays out of a several-layer change until Photoshop's behaviour is
        // confirmed (#5856): Opacity, Fill or a blend mode would turn it into Layer 0.
        let ids: Vec<LayerId> = if selection.len() > 1 && selection.contains(&l.id) {
            selection.iter().copied().filter(|id| *id == l.id || !doc.layer(*id).is_some_and(|x| crate::doc_props_ui::is_background(&doc, x))).collect()
        } else {
            vec![l.id]
        };
        let targets = if ids.len() > 1 { json!({"layers": ids.iter().map(|id| id.0).collect::<Vec<_>>()}) } else { json!({"layer": l.id.0}) };
        // Photoshop greys blend mode, Opacity and Fill for the Background layer. (With the
        // Background among several selected, what Photoshop does awaits a check: #5856.)
        let bg = crate::doc_props_ui::is_background(&doc, l);
        // Pass Through is a group's mode: offered only when every target is a group.
        let all_groups = ids.iter().all(|id| doc.layer(*id).is_some_and(|x| x.is_group()));
        let edit = ui.add_enabled_ui(!bg, |ui| opacity_row(ui, t.pro, l.blend, all_groups, l.opacity * 100.0)).inner;
        if !bg {
            // One step per choice: a click, an arrow key or each wheel notch (#1747).
            if let Some((mode, gesture)) = edit.blend.chosen.filter(|(mode, _)| *mode != l.blend) {
                let mut p = targets.clone();
                p["blend"] = json!(mode.label());
                if let Some(n) = gesture {
                    p["coalesce"] = json!(format!("layer-blend-select:{n}"));
                }
                actions.push(("layer.setProps".into(), p));
            }
            // Hovering a mode previews it on the canvas (#970).
            crate::blend_preview::hover(app, &ids, edit.blend.hovered.filter(|_| edit.blend.chosen.is_none()));
        }
        if let Some((pct, gesture)) = edit.opacity {
            actions.push(pct_action(&targets, "opacity", pct, gesture));
        }
        // Docks zero the item spacing: keep the Opacity and Fill fields apart (#155).
        ui.add_space((theme::ROW_GAP - ui.spacing().item_spacing.y).max(0.0));
        let lk = l.locks;
        let row = lock_row(ui, t.pro, [lk.transparency, lk.pixels, lk.position, lk.artboard, lk.all], lk.any(), l.fill_opacity * 100.0, !bg);
        if let Some((key, on)) = row.lock {
            let mut p = targets.clone();
            p["locks"] = json!({ key: on });
            actions.push(("layer.setProps".into(), p));
        }
        if row.simple {
            actions.push(simple_lock_toggle(crate::doc_props_ui::is_background(&doc, l), l, &targets));
        }
        if let Some((pct, gesture)) = row.fill {
            actions.push(pct_action(&targets, "fill", pct, gesture));
        }
        ui.add_space(craft_ui::theme::scale::Space::S2.px());
    } else {
        inactive_blend_rows(ui, t.pro);
    }

    // Top of the stack first, groups above their contents, closed groups' contents hidden (#126).
    let rows = crate::layer_tree_ui::panel_rows(app, &doc);
    let ctx = ui.ctx().clone();
    let footer = PanelFooter::reserved_height(&PanelFooter::style(&LibTokens::of(ui), ui.pixels_per_point())) + ui.spacing().item_spacing.y;
    let fill = ui.available_height() > footer + 60.0;
    let rows_h = if fill { ui.available_height() - footer } else { f32::INFINITY };
    egui::ScrollArea::vertical()
        // Keep each document's position independent, even when tab indices change (#5522).
        .id_salt(("layer-rows", doc.id.0))
        .max_height(rows_h)
        .min_scrolled_height(if fill { rows_h } else { 0.0 })
        .auto_shrink([false, !fill])
        .show(ui, |ui| {
            let delta = edge_scroll(&ctx, ui.clip_rect());
            if delta != 0.0 {
                ui.scroll_with_delta(vec2(0.0, delta));
                ctx.request_repaint();
            }
            let filter = crate::layer_filter_ui::Filter::of(&app.ui);
            let fx_collapsed = app.session.active().map(|d| d.fx_collapsed.clone()).unwrap_or_default();
            crate::layer_row_ui::begin(ui.ctx());
            // The drawn rows, top to bottom (#1721): the space between them and below the last
            // one is a drop zone too.
            let mut drawn: Vec<DrawnRow> = Vec::new();
            for &(depth, l) in &rows {
                // Select › Isolate Layers.
                if !photocraft_engine::select_extra_cmds::isolation_shows(&doc, &isolated, l.id) {
                    continue;
                }
                if !filter.shows(l) {
                    continue;
                }
                let row = RowSel { selected: selection.contains(&l.id), primary: active == Some(l.id), multi: selection.len() > 1 };
                let top = ui.cursor().top();
                drawn.push(DrawnRow { layer: l, depth, rect: layer_row(app, &ctx, ui, &doc, l, depth, row, &mut actions) });
                if reveal == Some(l.id) {
                    crate::layer_reveal::scroll_to_row(ui, top);
                }
                if !l.effects.items.is_empty() && fx_collapsed.iter().all(|id| *id != l.id) {
                    effect_rows(app, ui, l, depth, &mut actions);
                }
                if !fx_collapsed.contains(&l.id) {
                    crate::smart_ui::filter_rows(app, ui, &doc, l, depth, &mut actions);
                }
            }
            // The bottom of everything drawn: the last layer's effect and Smart Filter rows sit
            // under its own row (#8764).
            let rows_bottom = ui.min_rect().bottom();
            // #1721: the list takes drops everywhere, not only on a row (Photoshop): the space
            // between two rows lands above the lower one, the empty space below the bottom-most
            // row below it. Both copy with ⌥ held, move without.
            layer_drop(app, ui, &drawn, &mut actions);
            if let Some(last_rect) = drawn.last().map(|d| d.rect) {
                let below = Rect::from_min_max(pos2(last_rect.left(), last_rect.bottom()), pos2(last_rect.right(), ui.clip_rect().bottom()));
                // A click on that empty space deselects every layer, as in Photoshop (#8100). It
                // starts under the last layer's sub-rows, which take their own clicks.
                let empty = Rect::from_min_max(pos2(below.left(), rows_bottom), below.max);
                if empty.height() > 0.0 && ui.interact(empty, ui.scope_id().with("layers-empty-space"), Sense::click()).clicked() && !selection.is_empty() {
                    actions.push(("select.deselectLayers".into(), json!({})));
                }
            }
            // Tab / ⇧Tab in a rename: rename the layer below / above next, as in Photoshop. The
            // Background isn't renamed in place (double-clicking it converts it), so it ends there.
            if let Some((from, up)) = crate::layer_row_ui::take_next_rename(ui.ctx())
                && let Some(at) = drawn.iter().position(|d| d.layer.id.0 == from)
                && let Some(next) = if up { at.checked_sub(1).and_then(|i| drawn.get(i)) } else { drawn.get(at + 1) }.map(|d| d.layer.id)
                && let Some(l) = doc.layer(next).filter(|l| !crate::doc_props_ui::is_background(&doc, l))
                && let Some(done) = crate::layer_row_ui::start_rename_in(ui, next.0, &l.name)
            {
                actions.push(done);
            }
            // ⌥-click the line between two layers: clip / release the upper one (#967).
            crate::clip_line_ui::show(ui, &doc, &mut actions);
            // A rename whose row is gone (deleted, filtered out, inside a closed group) ends,
            // committed: nothing else could commit or cancel it.
            if let Some(layer) = crate::layer_row_ui::renaming(ui.ctx())
                && !crate::layer_row_ui::recorded(ui.ctx()).iter().any(|r| r.layer == layer)
                && let Some(done) = crate::layer_row_ui::end_rename(ui.ctx(), true)
            {
                actions.push(done);
            }
            // Likewise a selected smart filter whose row is gone.
            crate::smart_ui::forget_hidden(ui.ctx(), doc.id);
        });
    // Leaving the list or hovering an invalid target immediately cancels the temporary
    // image. The release still uses the actual command, not the preview snapshot.
    if !app.reorder_preview_hit {
        app.reorder_preview = None;
    }
    // A layer being dragged can also be dropped on the footer's Delete, New Layer and Group
    // buttons (#736), an effect row on Delete (#3102), a mask's thumbnail (#3113, #3119) and a
    // smart filter's row (#4271); read the drag before it ends.
    let held = held(&ctx, &selection);
    drag_feedback(ui);
    PanelFooter::show(ui, |ui| {
        let active_layer = active.and_then(|id| doc.layer(id));
        let (delete, label) = if app.ui.vector_mask_target && active_layer.is_some_and(|l| l.vector_mask.is_some()) {
            ("layer.vectorMask.delete", "Delete Vector Mask")
        } else if app.ui.mask_target && active_layer.is_some_and(|l| l.mask.is_some()) {
            ("layer.layerMask.delete", "Delete Layer Mask")
        } else {
            ("layer.delete", "Delete layer")
        };
        let trash = PanelFooter::button(ui, Icon::Trash, tl!(label));
        // A selected smart filter row is what Delete removes, as in Photoshop (#4751).
        let picked_filter = crate::smart_ui::selected(&ctx, &doc, active);
        if trash.clicked()
            && let Some(picked) = picked_filter
        {
            actions.push(crate::smart_ui::trash_action(picked));
            crate::smart_ui::deselect(&ctx, doc.id);
        } else if trash.clicked() {
            // Like Photoshop, ⌥/Alt-click deletes without asking.
            let confirm = delete == "layer.delete" && !ui.input(|i| i.modifiers.alt);
            let params = if confirm { json!({"__trash": true}) } else { json!({}) };
            actions.push((delete.into(), params));
        }
        actions.extend(footer_drop(ui, &trash, held.layer, "layer.delete"));
        actions.extend(fx_trash_drop(ui, &trash, held.fx));
        if let Some(drag) = held.filter
            && dropped_on(ui, &trash)
        {
            actions.push(crate::smart_ui::trash_action(drag));
        }
        // Photoshop asks "Apply mask to layer before removing?" or "Delete the vector mask?"
        // (`mask_trash_prompt`).
        if let Some((layer, kind)) = held.mask
            && dropped_on(ui, &trash)
        {
            actions.push(crate::mask_trash_prompt::drop_action(layer, kind));
        }
        // A click follows the selected thumbnail; name that action for screen readers even
        // though dropping a whole layer row onto the same button always deletes the layer.
        trash.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!(label)));
        let new_layer = PanelFooter::button(ui, Icon::NewLayer, &crate::shortcuts::tip_label(app, "Create a new layer", "layer.new.layer"));
        if new_layer.clicked() {
            // A click makes the layer above the selection; ⌥-click asks first (the New Layer
            // dialog) and ⌘/Ctrl-click adds it below, as in Photoshop.
            let (alt, below) = ui.input(|i| (i.modifiers.alt, i.modifiers.command));
            let params = if alt {
                Value::Null
            } else if below {
                json!({"position": "below"})
            } else {
                json!({})
            };
            actions.push(("layer.new.layer".into(), params));
        }
        actions.extend(footer_drop(ui, &new_layer, held.layer, "layer.duplicate"));
        let group = PanelFooter::button(ui, Icon::Folder, tl!("Create a new group"));
        if group.clicked() {
            // Like Photoshop CC 2014 and later, a click with two or more layers selected puts them
            // in the new group (Layer › Group Layers); with one layer or none it adds an empty
            // group above the active layer. Shift-click, the older way, groups the selection too
            // (#2561). ⌥-click asks first in the New Group dialog, ⌘-click adds an empty group
            // below the active layer.
            let m = ui.input(|i| i.modifiers);
            let groups = (m.shift || selection.len() > 1) && active_layer.is_some();
            if m.alt {
                // Null params go through the menu path, which opens the dialog.
                let cmd = if groups { crate::new_group_ui::FROM_LAYERS } else { crate::new_group_ui::NEW_GROUP };
                actions.push((cmd.into(), Value::Null));
            } else if m.command {
                actions.push(("layer.new.group".into(), json!({"position": "below"})));
            } else {
                let cmd = if groups { "layer.groupLayers" } else { "layer.new.group" };
                actions.push((cmd.into(), json!({})));
            }
        }
        actions.extend(footer_drop(ui, &group, held.layer, "layer.groupLayers"));
        let adj_open = ui.scope_id().with("adjustment_menu_open");
        let adj = PanelFooter::menu_button(ui, Icon::AdjustmentLayer, tl!("Create new fill or adjustment layer"), was_open(ui, adj_open));
        // Opens on the press like Photoshop's: drag onto an item and release to choose it (#2071).
        crate::press_menu::show(&adj, |ui, menu| {
            ui.set_min_width(190.0);
            for c in photocraft_engine::command_specs().iter().filter(|c| c.id.starts_with("layer.newAdjustmentLayer.")) {
                if menu.item(ui, tl!(c.label).trim_end_matches('…')) {
                    actions.push((c.id.into(), json!({})));
                }
            }
            ui.separator();
            if menu.item(ui, tl!("Solid Color…")) {
                actions.push(("layer.newFillLayer.solidColor".into(), json!({})));
            }
            if menu.item(ui, tl!("Gradient…")) {
                actions.push(("layer.newFillLayer.gradient".into(), json!({})));
            }
        });
        remember_open(ui, adj_open, &adj);
        // Like Photoshop the button never replaces a mask (#2075): with a layer mask it adds a
        // vector mask, and with both it is greyed.
        let alt = ui.input(|i| i.modifiers.alt);
        let mask_cmd = crate::layer_menu_ui::mask_button_command(active_layer, doc.selection.is_some(), alt);
        let mask_tip = if active_layer.is_some_and(|l| l.mask.is_some()) {
            tl!("Add vector mask").to_string()
        } else {
            crate::i18n::fmt(tl!("Add a mask  (from the selection; {key} inverts)"), &[("key", &crate::shortcuts::pretty("Alt"))])
        };
        let mask_btn = ui.add_enabled_ui(mask_cmd.is_some(), |ui| PanelFooter::button(ui, Icon::LayerMask, &mask_tip)).inner;
        mask_btn.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &mask_tip));
        if mask_btn.clicked()
            && let Some(cmd) = mask_cmd
        {
            // A path selected in the Paths panel becomes the vector mask, as in Photoshop (#4728);
            // Alt still inverts it.
            match crate::vector_ui::selected_document_path(app).filter(|_| cmd.starts_with("layer.vectorMask.")) {
                Some(name) => actions.push(("layer.vectorMask.currentPath".into(), json!({"name": name, "hide": alt}))),
                None => actions.push((cmd.into(), json!({}))),
            }
        }
        let fx_open = ui.scope_id().with("style_menu_open");
        let fx = PanelFooter::menu_button(ui, Icon::Fx, tl!("Add a layer style"), was_open(ui, fx_open));
        crate::press_menu::show(&fx, |ui, menu| {
            ui.set_min_width(180.0);
            if menu.item(ui, tl!("Blending Options…")) {
                crate::layer_style::open(app, None);
            }
            ui.separator();
            for &(kind, label) in crate::layer_style::KINDS {
                if menu.item(ui, format!("{}…", tl!(label))) {
                    crate::layer_style::open(app, Some(kind));
                }
            }
        });
        remember_open(ui, fx_open, &fx);
        // Photoshop's footer starts with Link Layers (enabled with two or more layers selected).
        let can_link = app.session.is_enabled("layer.linkLayers");
        if ui.add_enabled_ui(can_link, |ui| PanelFooter::button(ui, Icon::Link, tl!("Link layers"))).inner.clicked() {
            actions.push(("layer.linkLayers".into(), json!({})));
        }
    });
    for (id, p) in actions {
        // A row click commits even when it selects the layer already being edited (#5584).
        // Finish before selection changes so the existing type session keeps its own target.
        if id == "layer.select" && app.ui.text_edit.is_some() {
            crate::type_tool::commit(app);
        }
        if id == "ui.maskTarget" {
            app.end_filter_mask_view();
            app.ui.mask_target = p.as_bool().unwrap_or(false);
            app.ui.vector_mask_target = false;
            app.ui.filter_mask_target = None;
            app.sync_mask_targets();
            continue;
        }
        if id == "ui.vectorMaskTarget" {
            app.end_filter_mask_view();
            app.ui.vector_mask_target = p.as_bool().unwrap_or(false);
            app.ui.mask_target = false;
            app.ui.filter_mask_target = None;
            app.sync_mask_targets();
            continue;
        }
        if id == "ui.filterMaskTarget" {
            app.select_filter_mask_target(p.as_u64());
            continue;
        }
        // Double-clicking a mask thumbnail opens its settings: show Properties.
        if id == "ui.properties" {
            app.ui.panels.properties = true;
            app.ui.dock_tabs.properties = 0;
            app.ui.dock.collapsed.retain(|g| *g != crate::dock::Group::Properties);
            continue;
        }
        if p.is_null() || id == "layer.newFillLayer.solidColor" {
            // Context-menu items and the Layers footer's Solid Color… action behave like
            // their menu-bar twins. In particular, a new Solid Color fill opens its picker.
            let ctx = ui.ctx().clone();
            let params = if p.is_null() { json!({}) } else { p };
            if let Err(e) = crate::menus::invoke(app, &ctx, &id, params) {
                app.ui.status = e;
                app.ui.status_error = true;
            }
            continue;
        }
        let _ = app.run(&id, p);
    }
}

/// Whether a footer button's press menu was open after the last pass: `PanelFooter::menu_button`
/// reports it as the button's expanded state, and the menu's id is known only from the button.
fn was_open(ui: &egui::Ui, key: egui::Id) -> bool {
    ui.data(|d| d.get_temp(key)).unwrap_or(false)
}

fn remember_open(ui: &egui::Ui, key: egui::Id, button: &egui::Response) {
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(button));
    ui.data_mut(|d| d.insert_temp(key, open));
}

pub(super) fn footer_label(command: &str) -> &'static str {
    match command {
        "layer.delete" => tl!("Delete layer"),
        "layer.duplicate" => tl!("Create a new layer"),
        _ => tl!("Create a new group"),
    }
}

pub(super) fn channels(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    crate::channels_panel::show(app, ui);
}
