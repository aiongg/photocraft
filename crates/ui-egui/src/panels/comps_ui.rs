//! Window › Layer Comps panel (a tab of the History card).
//!
//! Rows: the Last Document State, then each comp with its apply marker, name, warning triangle
//! (layers deleted since capture) and the three apply toggles (visibility, position,
//! appearance). Footer: previous / next comp, update, new, delete. Every action is a
//! `layerComp.*` engine command; the selection is `UiState::layer_comp_selected`.

use std::cell::RefCell;

use craft_ui::behavior::{Axis, DropPosition, ListDrag, Reorder, RowInfo};
use craft_ui::composites::{PanelFooter, Tone};
use craft_ui::controls::{ControlSize, IconButton};
use craft_ui::edit::EditEvent;
use craft_ui::icons::Icon;
use craft_ui::list::{DropMark, ListRow, RowEvent, ToggleColumn, paint_drop_mark};
use craft_ui::menu::{ContextMenu, MenuItem};
use craft_ui::theme::{Tokens as LibTokens, TokensOf as _};
use egui::{Rect, vec2};
use serde_json::{Value, json};

use crate::PhotocraftApp;

/// The comp footer and row actions act on: the panel selection if still valid, else the last
/// applied comp.
pub fn target(app: &PhotocraftApp) -> Option<u32> {
    let doc = &app.session.active()?.doc;
    app.ui.layer_comp_selected.filter(|id| doc.comp(*id).is_some()).or(doc.last_applied_comp.filter(|id| doc.comp(*id).is_some()))
}

/// The row's warning triangle: Clear Layer Comp Warning for that comp only (Photoshop keeps
/// Clear All Layer Comp Warnings as a separate command).
fn clear_warning_action(comp: u32) -> (&'static str, Value) {
    ("layerComp.updateWarnings", json!({"clear": true, "comp": comp}))
}

fn run(app: &mut PhotocraftApp, id: &str, p: Value) {
    if let Err(e) = app.run(id, p) {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

fn drop_index(from: usize, target: usize, after: bool) -> usize {
    let slot = target + usize::from(after);
    if from < slot { slot - 1 } else { slot }
}

/// Whether the comp row for `rename` (keyed by document and comp) is open for a new name.
fn renaming(ui: &egui::Ui, rename: egui::Id) -> bool {
    ui.data(|d| d.get_temp::<bool>(rename)).unwrap_or(false)
}

pub fn panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let Some(st) = app.session.active() else {
        ui.weak(tl!("No document"));
        return;
    };
    let doc = st.doc.clone();
    let selected = target(app);
    let applied = doc.last_applied_comp;
    let tokens = LibTokens::of(ui);
    let footer_h = PanelFooter::reserved_height(&PanelFooter::style(&tokens, ui.pixels_per_point())) + ui.spacing().item_spacing.y;
    let max_h = (ui.available_height() - footer_h - 40.0).clamp(80.0, 320.0);
    let toggle_side = IconButton::style(&tokens, ControlSize::Sm).side;
    let mut action: Option<(&'static str, Value)> = None;
    let mut select: Option<u32> = None;
    // Comp ids restart in every document, so the list is keyed by the document too.
    let list = egui::Id::unique(("comp-list", doc.id.0));
    let order: Vec<RowInfo<u32>> = doc.layer_comps.iter().map(|c| RowInfo { key: c.id, depth: 0, has_children: false, open: false, section: false }).collect();
    egui::ScrollArea::vertical().id_salt("comps-rows").max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
        // Last Document State.
        let out = ListRow::new("last-document-state", tl!("Last Document State"))
            .toggle(ToggleColumn::new(tl!("Apply {row}"), Icon::Check, Icon::Check, applied.is_none()))
            .selected(applied.is_none())
            .show(ui);
        let marked = out.events.iter().any(|e| matches!(e, RowEvent::Toggle(EditEvent::Update { .. } | EditEvent::Set { .. })));
        if (out.response.clicked() || marked) && doc.last_document_state.is_some() && applied.is_some() {
            action = Some(("layerComp.restoreLastDocumentState", json!({})));
        }

        if doc.layer_comps.is_empty() {
            ui.add_space(craft_ui::theme::scale::Space::S6.px());
            ui.weak(tl!("Capture the visibility, position and style of every layer with + below, then switch between versions."));
        }
        let mut rects: Vec<Rect> = Vec::new();
        for (index, c) in doc.layer_comps.iter().enumerate() {
            // Comp ids restart in every document, so the pending name is keyed by the document too.
            let rename_id = egui::Id::unique(("comp-rename", doc.id.0, c.id));
            let missing = c.missing_layers(&doc).len();
            let row_hits = RefCell::new(None::<(&'static str, Value)>);
            let mut row = ListRow::new(c.id, c.name.as_str())
                .toggle(ToggleColumn::new(tl!("Apply {row}"), Icon::Check, Icon::Check, applied == Some(c.id)))
                .selected(selected == Some(c.id))
                .renaming(renaming(ui, rename_id));
            if missing > 0 {
                row = row.trailing_item(18.0, |ui| {
                    let r = Rect::from_center_size(ui.max_rect().center(), vec2(14.0, 14.0));
                    let tip = crate::i18n::trn(
                        crate::i18n::current(),
                        missing as u64,
                        "{n} layer recorded by this comp was deleted. Update the comp, or clear the warning.",
                        "{n} layers recorded by this comp were deleted. Update the comp, or clear the warning.",
                    );
                    if IconButton::new(Icon::Warning, &tip).tone(Tone::Warn).show_at(ui, r).clicked() {
                        *row_hits.borrow_mut() = Some(clear_warning_action(c.id));
                    }
                });
            }
            // The three apply toggles, right-aligned.
            let toggles = [
                (Icon::Eye, tl!("Visibility"), c.apply_visibility, "visibility"),
                (Icon::Move, tl!("Position"), c.apply_position, "position"),
                (Icon::Sparkles, tl!("Appearance (Layer Style)"), c.apply_appearance, "appearance"),
            ];
            row = row.trailing_item(3.0 * toggle_side, |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (icon, tip, on, key) in toggles {
                    let template = "Apply {option}: {state}";
                    let state = if on { "on" } else { "off" };
                    let label = crate::i18n::fmt(tl!(template), &[("option", tip), ("state", tl!(state))]);
                    if ui.add(IconButton::new(icon, label).size(ControlSize::Sm).selected(on)).clicked() {
                        *row_hits.borrow_mut() = Some(("layerComp.setOptions", json!({"comp": c.id, key: !on})));
                    }
                }
            });
            let out = row.show(ui);
            let resp = out.response.clone();
            if resp.drag_started() {
                egui::DragAndDrop::set_payload(ui.ctx(), ListDrag::new(list, &order[..], &[], c.id, c.name.clone()));
            }
            rects.push(resp.rect);
            let entries = [
                MenuItem::new(tl!("Move Up"), -1).enabled(index > 0).into(),
                MenuItem::new(tl!("Move Down"), 1).enabled(index + 1 < doc.layer_comps.len()).into(),
            ];
            if let Some(step) = ContextMenu::new(&entries).show(ui, &resp).chosen {
                let to = if step < 0 { index.saturating_sub(1) } else { index + 1 };
                action = Some(("layerComp.reorder", json!({"comp":c.id, "index":to})));
                select = Some(c.id);
            }
            let mut toggle_hit = false;
            if let Some(hit) = row_hits.take() {
                toggle_hit = true;
                action = Some(hit);
            }
            for event in out.events {
                match event {
                    // The marker applies the comp, and selects it.
                    RowEvent::Toggle(EditEvent::Update { .. } | EditEvent::Set { .. }) => {
                        toggle_hit = true;
                        select = Some(c.id);
                        action = Some(("layerComp.apply", json!({"comp": c.id})));
                    }
                    RowEvent::Renamed { text, .. } => {
                        let text = text.trim();
                        if !text.is_empty() && text != c.name {
                            action = Some(("layerComp.rename", json!({"comp": c.id, "name": text})));
                        }
                    }
                    RowEvent::RenameEnded { .. } => ui.data_mut(|d| d.remove::<bool>(rename_id)),
                    _ => {}
                }
            }
            if !c.comment.is_empty() && resp.hovered() {
                resp.clone().on_hover_text(&c.comment);
            }
            if resp.clicked() && !toggle_hit {
                select = Some(c.id);
            }
            if resp.double_clicked() && !toggle_hit {
                // Photoshop renames in place when the name itself is double-clicked.
                // The label rect runs to the row's trailing items; the name is only as wide as its text.
                let name_end = out.label.left() + super::options_bar::body_text_width(ui, &c.name) + 8.0;
                if resp.interact_pointer_pos().is_some_and(|p| out.label.contains(p) && p.x <= name_end) {
                    ui.data_mut(|d| d.insert_temp(rename_id, true));
                } else {
                    action = Some(("layerComp.apply", json!({"comp": c.id})));
                }
            }
        }
        if let Some(drag) = egui::DragAndDrop::payload::<ListDrag<u32>>(ui.ctx())
            && drag.list == list
            && let Some(&source) = drag.items.first()
            && let Some(p) = ui.input(|i| i.pointer.interact_pos()).filter(|p| ui.clip_rect().contains(*p))
            && let Some(from) = doc.layer_comps.iter().position(|c| c.id == source)
        {
            let spans: Vec<std::ops::Range<f32>> = rects.iter().map(|r| r.top()..r.bottom()).collect();
            if let Some(resolved) = Reorder::new(Axis::Vertical).resolve(&order[..], &spans[..], p.y, &[source], ui.input(|i| i.modifiers))
                && let Some(target) = doc.layer_comps.iter().position(|c| c.id == resolved.mv.target)
            {
                let after = resolved.mv.position != DropPosition::Before;
                paint_drop_mark(ui, rects[resolved.at], if after { DropMark::After } else { DropMark::Before });
                let to = drop_index(from, target, after);
                if ui.input(|i| i.pointer.any_released()) && from != to {
                    select = Some(source);
                    action = Some(("layerComp.reorder", json!({"comp":source, "index":to})));
                }
            }
        }
    });
    // Footer: apply previous, apply next | update, new, delete (Photoshop's order). The bar fills
    // from the trailing end, so the previous/next pair is added last, after a spacer.
    let has = !doc.layer_comps.is_empty();
    PanelFooter::show(ui, |ui| {
        if PanelFooter::button(ui, Icon::Trash, tl!("Delete layer comp")).clicked()
            && let Some(id) = selected
        {
            action = Some(("layerComp.delete", json!({"comp": id})));
        }
        if PanelFooter::button(ui, Icon::NewFile, tl!("Create new layer comp")).clicked() {
            action = Some(("layerComp.new", json!({})));
        }
        if PanelFooter::button(ui, Icon::RotateCw, tl!("Update layer comp (visibility, position and appearance)")).clicked()
            && let Some(id) = selected
        {
            action = Some(("layerComp.update", json!({"comp": id})));
        }
        let (side, gap) = (tokens.component.panel_footer.button, tokens.component.panel_footer.gap);
        ui.add_space((ui.available_width() - 2.0 * side - gap).max(0.0));
        if PanelFooter::button(ui, Icon::ChevronsRight, tl!("Apply next selected layer comp")).clicked() && has {
            action = Some(("layerComp.next", json!({})));
        }
        if PanelFooter::button(ui, Icon::ChevronsLeft, tl!("Apply previous selected layer comp")).clicked() && has {
            action = Some(("layerComp.previous", json!({})));
        }
    });
    if let Some(id) = select {
        app.ui.layer_comp_selected = Some(id);
    }
    if let Some((cmd, p)) = action {
        let created = cmd == "layerComp.new";
        run(app, cmd, p);
        if created {
            app.ui.layer_comp_selected = app.session.active().and_then(|d| d.doc.last_applied_comp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;
    use photocraft_engine::Session;

    /// The row of the comp named `name`: the list box option, not its label.
    fn row_rect(h: &egui_kittest::Harness<'_, PhotocraftApp>, name: &str) -> egui::Rect {
        use egui_kittest::kittest::{NodeT, Queryable};
        h.query_all_by_label(name).find(|n| n.accesskit_node().role() == egui::accesskit::Role::ListBoxOption).expect("the comp's row").rect()
    }

    #[test]
    fn dragging_a_comp_row_reorders_without_applying_it() {
        use egui_kittest::Harness;
        let mut app = PhotocraftApp::new(Session::new(), crate::Services::default());
        app.run("file.new", json!({"width":16,"height":16})).unwrap();
        for name in ["First", "Second", "Third"] {
            app.run("layerComp.new", json!({"name":name})).unwrap();
        }
        let applied = app.session.active().unwrap().doc.last_applied_comp;
        let first = app.session.active().unwrap().doc.layer_comps[0].id;
        let mut h = Harness::builder().with_size(vec2(360.0, 300.0)).build_ui_state(|ui, app| panel(app, ui), app);
        h.run_steps(3);
        let from = row_rect(&h, "First").center();
        let row = row_rect(&h, "Third");
        let to = pos2(row.center().x, row.bottom() - 2.0);
        h.event(egui::Event::PointerMoved(from));
        h.run_steps(1);
        h.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
        h.event(egui::Event::PointerMoved(from + vec2(10.0, 0.0)));
        h.run_steps(2);
        h.event(egui::Event::PointerMoved(to));
        h.run_steps(2);
        h.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(3);
        let app = h.state();
        let doc = &app.session.active().unwrap().doc;
        assert_eq!(doc.layer_comps.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Second", "Third", "First"]);
        assert_eq!(doc.last_applied_comp, applied);
        assert_eq!(app.ui.layer_comp_selected, Some(first));
    }

    #[test]
    fn comp_drop_slots_account_for_the_removed_source_row() {
        assert_eq!(drop_index(0, 2, false), 1);
        assert_eq!(drop_index(0, 2, true), 2);
        assert_eq!(drop_index(2, 0, false), 0);
        assert_eq!(drop_index(2, 0, true), 1);
        assert_eq!(drop_index(1, 1, false), 1);
        assert_eq!(drop_index(1, 1, true), 1);
    }

    #[test]
    fn warning_icon_clears_only_its_own_comp() {
        // Regression for #4197.
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 16, "height": 16})).unwrap();
        let layer = s.execute("layer.new.layer", json!({})).unwrap()["layer"].as_u64().unwrap();
        let c1 = s.execute("layerComp.new", json!({"name": "One"})).unwrap()["comp"].as_u64().unwrap() as u32;
        let c2 = s.execute("layerComp.new", json!({"name": "Two"})).unwrap()["comp"].as_u64().unwrap() as u32;
        s.execute("layer.delete", json!({"layer": layer})).unwrap();
        let (id, p) = clear_warning_action(c1);
        assert_eq!(p["comp"], c1);
        s.execute(id, p).unwrap();
        let doc = &s.active().unwrap().doc;
        assert!(doc.comp(c1).unwrap().missing_layers(doc).is_empty());
        assert_eq!(doc.comp(c2).unwrap().missing_layers(doc).iter().map(|l| l.0).collect::<Vec<_>>(), vec![layer]);
    }

    #[test]
    fn double_clicking_a_comp_name_renames_it_in_place() {
        // #6496.
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        let mut app = PhotocraftApp::new(Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 16, "height": 16})).unwrap();
        let first = app.run("layerComp.new", json!({"name": "Comp number one"})).unwrap()["comp"].as_u64().unwrap() as u32;
        let second = app.run("layerComp.new", json!({"name": "Two"})).unwrap()["comp"].as_u64().unwrap() as u32;
        // 60 fps steps, so two clicks a frame apart count as a double-click.
        let mut h = Harness::builder().with_size(vec2(320.0, 260.0)).with_step_dt(1.0 / 60.0).build_ui_state(|ui, app| panel(app, ui), app);
        h.run_steps(3);
        // The name starts after the marker column and is as wide as its text, so the click stays
        // near its start to hit the short "Hero" too; the rest of the row is past the name and
        // before the three apply toggles.
        let row = row_rect(&h, "Comp number one");
        let (name, rest) = (pos2(row.left() + 45.0, row.center().y), pos2(row.right() - 100.0, row.center().y));
        let double_click = |h: &mut Harness<'_, PhotocraftApp>, pos: egui::Pos2| {
            h.event(egui::Event::PointerMoved(pos));
            // Wait out the double-click window of the previous attempt.
            h.run_steps(30);
            for pressed in [true, false, true, false] {
                h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
                h.step();
            }
            h.run_steps(3);
        };
        let type_name = |h: &mut Harness<'_, PhotocraftApp>, text: &str, key: egui::Key| {
            double_click(h, name);
            h.event(egui::Event::Text(text.into()));
            h.run_steps(1);
            let fields: Vec<_> = h.query_all_by_role(egui::accesskit::Role::TextInput).map(|n| n.accesskit_node().value()).collect();
            assert_eq!(fields, vec![Some(text.to_string())], "the name field opened and holds {text:?}");
            h.key_press(key);
            h.run_steps(3);
        };
        let comp_name = |h: &Harness<'_, PhotocraftApp>| h.state().session.active().unwrap().doc.comp(first).unwrap().name.clone();
        let applied = |h: &Harness<'_, PhotocraftApp>| h.state().session.active().unwrap().doc.last_applied_comp;
        // Whitespace keeps the old name.
        type_name(&mut h, "  ", egui::Key::Enter);
        assert_eq!(comp_name(&h), "Comp number one");
        type_name(&mut h, "Hero", egui::Key::Enter);
        assert_eq!(comp_name(&h), "Hero");
        assert_eq!(applied(&h), Some(second), "renaming must not apply the comp");
        type_name(&mut h, "Lost", egui::Key::Escape);
        assert_eq!(comp_name(&h), "Hero", "Escape cancels");
        assert_eq!(applied(&h), Some(second), "Escape applies nothing");
        // Double-clicking the rest of the row still applies the comp.
        double_click(&mut h, rest);
        assert_eq!(applied(&h), Some(first));
        assert_eq!(comp_name(&h), "Hero");
    }

    #[test]
    fn a_rename_left_open_does_not_carry_over_to_another_document() {
        // Comp ids restart in every document, so the pending name must stay with its own document.
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        let mut app = PhotocraftApp::new(Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 16, "height": 16})).unwrap();
        let a = app.run("layerComp.new", json!({"name": "Alpha comp"})).unwrap()["comp"].as_u64().unwrap() as u32;
        app.run("file.new", json!({"width": 16, "height": 16})).unwrap();
        let b = app.run("layerComp.new", json!({"name": "Beta comp"})).unwrap()["comp"].as_u64().unwrap() as u32;
        assert_eq!(a, b, "the two documents' comps share an id");
        app.session.set_active(0);
        let mut h = Harness::builder().with_size(vec2(320.0, 260.0)).with_step_dt(1.0 / 60.0).build_ui_state(|ui, app| panel(app, ui), app);
        h.run_steps(3);
        let row = row_rect(&h, "Alpha comp");
        let name = pos2(row.left() + 45.0, row.center().y);
        h.event(egui::Event::PointerMoved(name));
        h.run_steps(3);
        for pressed in [true, false, true, false] {
            h.event(egui::Event::PointerButton { pos: name, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            h.step();
        }
        h.run_steps(3);
        h.event(egui::Event::Text("Leak".into()));
        h.run_steps(1);
        let fields: Vec<_> = h.query_all_by_role(egui::accesskit::Role::TextInput).map(|n| n.accesskit_node().value()).collect();
        assert_eq!(fields, vec![Some("Leak".to_string())], "the name field opened in the first document");
        h.state_mut().session.set_active(1);
        h.run_steps(3);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert_eq!(h.state().session.active().unwrap().doc.comp(b).unwrap().name, "Beta comp");
    }
}
