//! Layer Style contours: the Contour row (thumbnail, preset picker, Anti-aliased) and the
//! Contour Editor. Everything edits the dialog's pending effect params, so the live preview
//! follows and OK applies it in the dialog's one `layer.layerStyle.replace`.
//!
//! Editor state lives in the dialog fields, so automation can drive it:
//! - `contourPicker`: `{effect, key}` while a preset picker is open.
//! - `contourEditor`: `{effect, key, original}` while the Contour Editor is open; `original` is
//!   the param Cancel restores.

use craft_ui::composites::{Curve, CurveEditor};
use craft_ui::edit::EditEvent;
use craft_ui::localized::LocaleNumbers;
use craft_ui::theme::StrokeExt;
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, pos2, vec2};
use photocraft_doc::Contour;
use photocraft_doc::adjust::CurvePoint;
use photocraft_engine::layer_style::{CONTOURS, contour_from_param, contour_param, contour_to_shc, contours_from_shc};
use serde_json::{Map, Value, json};

use super::{entry, set_param};
use crate::state::DialogKind;
use crate::theme::Tokens;
use crate::{PhotocraftApp, widgets};

pub(super) const PICKER: &str = "contourPicker";
pub(super) const EDITOR: &str = "contourEditor";

/// The contour a param names (Linear when it names none we know).
pub(super) fn contour_of(v: Option<&Value>) -> Contour {
    v.and_then(contour_from_param).unwrap_or_default()
}

/// A contour's points (`0..=255` levels, as the shared point editor works) and corner flags.
fn levels_of(c: &Contour) -> (Vec<[f32; 2]>, Vec<bool>) {
    match c {
        Contour::Linear => (vec![[0.0, 0.0], [255.0, 255.0]], vec![false, false]),
        Contour::Custom { points, .. } => {
            (points.iter().map(|p| [p.input * 255.0, p.output * 255.0]).collect(), (0..points.len()).map(|i| c.is_corner(i)).collect())
        }
    }
}

fn lut_of(c: &Contour) -> Vec<f32> {
    match c {
        Contour::Linear => (0..64).map(|i| i as f32 / 63.0).collect(),
        Contour::Custom { points, corners, .. } => photocraft_compose::adjust::contour_curve_lut(points, corners),
    }
}

fn sample(l: &[f32], x: f32) -> f32 {
    let i = (x.clamp(0.0, 1.0) * l.len().saturating_sub(1) as f32).round() as usize;
    l.get(i).copied().unwrap_or(0.0)
}

/// Draws `c` into `r` (field background, the curve, a border).
pub(super) fn draw(p: &egui::Painter, r: Rect, c: &Contour, line: Color32, t: &Tokens) {
    p.rect_filled(r, craft_ui::theme::scale::Radius::R0.px(), t.field);
    let inner = r.shrink(2.0);
    let l = lut_of(c);
    let n = (inner.width() as usize).clamp(8, 512);
    let pts: Vec<Pos2> = (0..=n)
        .map(|k| {
            let x = k as f32 / n as f32;
            pos2(inner.left() + x * inner.width(), inner.bottom() - sample(&l, x) * inner.height())
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.2, line)));
    p.rect_stroke(r, craft_ui::theme::scale::Radius::R0.px(), Stroke::new(1.0, t.field_border), StrokeKind::Inside);
}

fn picker_target(effect: &str, key: &str) -> Value {
    json!({"effect": effect, "key": key})
}

/// Picks a preset for `effect`'s `key` (the picker's click).
pub(super) fn pick(f: &mut Map<String, Value>, effect: &str, key: &str, name: &str) {
    pick_param(f, effect, key, json!(name));
}

/// Picks a contour param (a preset name, or a user contour's curve object).
pub(super) fn pick_param(f: &mut Map<String, Value>, effect: &str, key: &str, param: Value) {
    set_param(f, effect, key, param);
    f.remove(PICKER);
}

/// What the picker's menu asked for (Photoshop's Load, Save and Replace Contours).
#[derive(Clone, Copy, PartialEq)]
enum Act {
    Load,
    Replace,
    Save,
}

/// The Contour row of an effect page: label, thumbnail (opens the Contour Editor), the preset
/// picker arrow and, when the effect has one, Anti-aliased.
#[allow(clippy::too_many_arguments)]
pub(super) fn row(
    app: &mut PhotocraftApp,
    ui: &mut egui::Ui,
    f: &mut Map<String, Value>,
    effect: &str,
    key: &str,
    label: &str,
    aa_key: Option<&str>,
    disp: &mut Value,
) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        if label.is_empty() {
            ui.add_space(craft_ui::theme::scale::Space::S16.px());
        } else {
            ui.label(RichText::new(tl!(label)).color(t.text_dim));
        }
        let c = contour_of(disp.get(key));
        let (r, thumb) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
        draw(ui.painter(), r, &c, t.text, &t);
        if thumb.hovered() {
            ui.painter().rect_stroke(
                r,
                craft_ui::theme::scale::Radius::R0.px(),
                craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.accent),
                StrokeKind::Outside,
            );
        }
        if thumb.on_hover_text(tl!("Contour Editor")).clicked() {
            open_editor(f, effect, key);
        }
        let (ar, arrow) = ui.allocate_exact_size(vec2(14.0, 36.0), Sense::click());
        let (cx, s) = (ar.center(), 3.2);
        let col = if arrow.hovered() { t.text } else { t.text_dim };
        ui.painter().line_segment([cx + vec2(-s, -s * 0.5), cx + vec2(0.0, s * 0.5)], Stroke::new(1.3, col));
        ui.painter().line_segment([cx + vec2(0.0, s * 0.5), cx + vec2(s, -s * 0.5)], Stroke::new(1.3, col));
        let target = picker_target(effect, key);
        let was = f.get(PICKER) == Some(&target);
        let mut open = was;
        if arrow.clicked() {
            open = !open;
        }
        let mut chosen: Option<&str> = None;
        let mut chosen_user: Option<Value> = None;
        let mut delete: Option<String> = None;
        let mut act: Option<Act> = None;
        let user = &app.session.presets.contours;
        let parent_layer = arrow.layer_id;
        egui::Popup::from_response(&arrow).open_bool(&mut open).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            crate::press_menu::above_parent_ui(parent_layer, ui);
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            egui::Grid::new(("contour-presets", effect, key)).spacing(vec2(4.0, 4.0)).show(ui, |ui| {
                for (i, (name, _)) in CONTOURS.iter().enumerate() {
                    let preset = photocraft_engine::layer_style::builtin_contour(name).unwrap_or_default();
                    let (r, resp) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::click());
                    draw(ui.painter(), r, &preset, t.text, &t);
                    if preset == c || resp.hovered() {
                        ui.painter().rect_stroke(
                            r,
                            craft_ui::theme::scale::Radius::R0.px(),
                            craft_ui::theme::scale::Stroke::W1_5.egui(ui.ctx(), t.accent),
                            StrokeKind::Outside,
                        );
                    }
                    if resp.on_hover_text(tl!(name)).clicked() {
                        chosen = Some(name);
                    }
                    if i % 4 == 3 {
                        ui.end_row();
                    }
                }
            });
            // The user's own contours follow the standard ones (Contour Editor › New…).
            if !user.is_empty() {
                ui.add_space(craft_ui::theme::scale::Space::S4.px());
                egui::Grid::new(("contour-user", effect, key)).spacing(vec2(4.0, 4.0)).show(ui, |ui| {
                    for (i, u) in user.iter().enumerate() {
                        let (r, resp) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::click());
                        draw(ui.painter(), r, u, t.text, &t);
                        if *u == c || resp.hovered() {
                            ui.painter().rect_stroke(
                                r,
                                craft_ui::theme::scale::Radius::R0.px(),
                                craft_ui::theme::scale::Stroke::W1_5.egui(ui.ctx(), t.accent),
                                StrokeKind::Outside,
                            );
                        }
                        let name = name_of(u).to_string();
                        let resp = resp.on_hover_text(name.as_str());
                        if resp.clicked() {
                            chosen_user = Some(contour_param(u));
                        }
                        resp.context_menu(|ui| {
                            if ui.button(tl!("Delete Contour")).clicked() {
                                delete = Some(name.clone());
                                ui.close();
                            }
                        });
                        if i % 4 == 3 {
                            ui.end_row();
                        }
                    }
                });
            }
            ui.separator();
            if ui.button(tl!("Load Contours…")).clicked() {
                act = Some(Act::Load);
            }
            if ui.button(tl!("Replace Contours…")).clicked() {
                act = Some(Act::Replace);
            }
            if ui.add_enabled(!user.is_empty(), egui::Button::new(tl!("Save Contours…"))).clicked() {
                act = Some(Act::Save);
            }
        });
        if let Some(name) = delete {
            let r = app.run("layer.layerStyle.contours.delete", json!({"name": name}));
            report(app, r);
        }
        if let Some(act) = act {
            library_action(app, act);
        }
        if let Some(param) = chosen_user {
            disp[key] = param.clone();
            pick_param(f, effect, key, param);
        }
        if let Some(name) = chosen {
            disp[key] = json!(name);
            pick(f, effect, key, name);
        } else if open != was {
            if open {
                f.insert(PICKER.into(), target);
            } else {
                f.remove(PICKER);
            }
        }
        if let Some(aa_key) = aa_key {
            let mut on = disp.get(aa_key).and_then(Value::as_bool).unwrap_or(false);
            if widgets::checkbox(ui, &mut on, tl!("Anti-aliased")).changed() {
                disp[aa_key] = json!(on);
                set_param(f, effect, aa_key, json!(on));
            }
        }
    });
}

/// Opens the Contour Editor on `effect`'s `key`, remembering the param Cancel restores.
pub(super) fn open_editor(f: &mut Map<String, Value>, effect: &str, key: &str) {
    let original = current_param(f, effect, key);
    f.remove(PICKER);
    f.insert(EDITOR.into(), json!({"effect": effect, "key": key, "original": original}));
}

fn current_param(f: &Map<String, Value>, effect: &str, key: &str) -> Value {
    entry(f, effect)
        .and_then(|e| e.get("params"))
        .and_then(|p| p.get(key))
        .cloned()
        .or_else(|| entry(f, effect).and_then(|e| e.get("kind")).and_then(Value::as_str).and_then(|k| super::defaults(k).get(key).cloned()))
        .unwrap_or_else(|| json!("Linear"))
}

fn editor_target(f: &Map<String, Value>) -> Option<(String, String)> {
    let ed = f.get(EDITOR)?;
    let effect = ed.get("effect")?.as_str()?.to_string();
    let key = ed.get("key")?.as_str()?.to_string();
    entry(f, &effect)?;
    Some((effect, key))
}

/// The edited curve: its points (`0..=255`), corner flags and name.
fn editor_curve(f: &Map<String, Value>) -> Option<(String, String, Contour)> {
    let (effect, key) = editor_target(f)?;
    let c = contour_of(Some(&current_param(f, &effect, &key)));
    Some((effect, key, c))
}

/// Stores an edited curve on the effect. Editing a built-in preset makes a "Custom" contour, so a
/// saved file doesn't call the edited curve by the preset's name.
fn store(f: &mut Map<String, Value>, effect: &str, key: &str, name: &str, pts: &[[f32; 2]], corners: &[bool]) -> bool {
    let name = if name.is_empty() || photocraft_engine::layer_style::builtin_contour(name).is_some() { "Custom" } else { name };
    let v = json!({
        "name": name,
        "points": pts.iter().map(|q| json!([q[0] / 255.0, q[1] / 255.0])).collect::<Vec<_>>(),
        "corners": corners,
    });
    let Some(c) = contour_from_param(&v) else { return false };
    set_param(f, effect, key, contour_param(&c));
    true
}

fn name_of(c: &Contour) -> &str {
    match c {
        Contour::Linear => "Linear",
        Contour::Custom { name, .. } => name,
    }
}

/// The open Contour Editor's points (levels `0..=255`) and corner flags.
#[cfg(test)]
pub(super) fn editor_levels(f: &Map<String, Value>) -> Option<(Vec<[f32; 2]>, Vec<bool>)> {
    editor_curve(f).map(|(_, _, c)| levels_of(&c))
}

/// Writes the Contour Editor's curve (levels `0..=255`, with its corner flags) to the effect.
pub(super) fn set_curve(f: &mut Map<String, Value>, curve: &Curve) -> bool {
    let Some((effect, key, c)) = editor_curve(f) else { return false };
    let Curve::Points { points, .. } = curve else { return false };
    let corners: Vec<bool> = (0..points.len()).map(|i| curve.is_corner(i)).collect();
    store(f, &effect, &key, name_of(&c), points, &corners)
}

/// Closes the Contour Editor: OK keeps the edited curve, Cancel restores the original.
pub(super) fn close_editor(f: &mut Map<String, Value>, keep: bool) {
    if !keep
        && let Some((effect, key)) = editor_target(f)
        && let Some(original) = f.get(EDITOR).and_then(|e| e.get("original")).cloned()
    {
        set_param(f, &effect, &key, original);
    }
    f.remove(EDITOR);
}

/// The Contour Editor (a modal over the Layer Style dialog), when one is open.
pub(super) fn editor(app: &mut PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    if !f.contains_key(EDITOR) {
        return;
    }
    let Some((effect, key, c)) = editor_curve(f) else {
        f.remove(EDITOR);
        return;
    };
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    let mem = egui::Id::unique(("contour-editor", effect.as_str(), key.as_str()));
    let (mut pts, mut corners) = levels_of(&c);
    let mut close: Option<bool> = None;
    let mut load = false;
    let mut save = false;
    let naming_id = mem.with("naming");
    let mut naming: Option<String> = ctx.data(|d| d.get_temp(naming_id));
    let mut new_clicked = false;
    let mut commit_name = false;
    let mut preset: Option<String> = None;
    let modal = egui::Modal::new(mem.with("modal")).show(&ctx, |ui| {
        ui.label(RichText::new(tl!("Contour Editor")).font(crate::theme::semibold(14.0)).color(t.text));
        ui.add_space(craft_ui::theme::scale::Space::S6.px());
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tl!("Preset:")).color(t.text_dim));
                    let mut cur = name_of(&c).to_string();
                    let opts: Vec<(String, &str)> = CONTOURS.iter().map(|(n, _)| (n.to_string(), *n)).collect();
                    if widgets::dropdown(ui, "contour-editor-preset", tl!("Preset"), &mut cur, &opts, 190.0) {
                        preset = Some(cur);
                    }
                });
                ui.add_space(craft_ui::theme::scale::Space::S6.px());
                let curve = Curve::from_parts(pts.clone(), corners.clone());
                let numbers = LocaleNumbers::of(ui);
                let before = mem.with("before");
                let edit = CurveEditor::new(mem.with("curve"), tl!("Contour"), &curve, &numbers).percent().corners(true).side(250.0).show(ui);
                match edit.event {
                    Some(EditEvent::Begin { .. }) => {
                        ui.data_mut(|d| d.insert_temp(before, c.clone()));
                    }
                    Some(EditEvent::Update { value, .. } | EditEvent::Set { value, .. } | EditEvent::Commit { value, changed: true, .. }) => {
                        set_curve(f, &value);
                    }
                    Some(EditEvent::Cancel { .. }) => {
                        if let Some(original) = ui.data(|d| d.get_temp::<Contour>(before)) {
                            set_param(f, &effect, &key, contour_param(&original));
                        }
                    }
                    _ => {}
                }
                ui.add_space(craft_ui::theme::scale::Space::S6.px());
                if let Some(name) = naming.as_mut() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tl!("Name:")).color(t.text_dim));
                        let r = widgets::text_field(ui, mem.with("name-field"), name, "", 150.0, false).accessible_name(tl!("Name:"));
                        r.request_focus();
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            commit_name = true;
                        }
                        if widgets::primary_button(ui, tl!("OK"), 60.0).clicked() {
                            commit_name = true;
                        }
                    });
                    ui.add_space(craft_ui::theme::scale::Space::S6.px());
                }
            });
            ui.add_space(craft_ui::theme::scale::Space::S12.px());
            ui.vertical(|ui| {
                if widgets::primary_button(ui, tl!("OK"), 84.0).clicked() {
                    close = Some(true);
                }
                ui.add_space(craft_ui::theme::scale::Space::S4.px());
                if widgets::secondary_button(ui, tl!("Cancel"), 84.0).clicked() {
                    close = Some(false);
                }
                ui.add_space(craft_ui::theme::scale::Space::S12.px());
                load = widgets::secondary_button(ui, tl!("Load…"), 84.0).clicked();
                ui.add_space(craft_ui::theme::scale::Space::S4.px());
                save = widgets::secondary_button(ui, tl!("Save…"), 84.0).clicked();
                ui.add_space(craft_ui::theme::scale::Space::S4.px());
                new_clicked = widgets::secondary_button(ui, tl!("New…"), 84.0).clicked();
            });
        });
    });
    if new_clicked && naming.is_none() {
        // Photoshop asks for the new contour's name; "Contour N" is its suggestion.
        naming = Some(format!("Contour {}", app.session.presets.contours.len() + 1));
    }
    if commit_name && let Some(name) = naming.take() {
        let name = if name.trim().is_empty() { "Contour".to_string() } else { name };
        match app.run("layer.layerStyle.contours.new", json!({"contour": contour_param(&shown_contour(&pts, &corners, name_of(&c))), "name": name})) {
            Ok(v) => set_param(f, &effect, &key, v["contour"].clone()),
            Err(e) => report(app, Err(e)),
        }
    }
    ctx.data_mut(|d| match &naming {
        Some(n) => {
            d.insert_temp(naming_id, n.clone());
        }
        None => {
            d.remove_temp::<String>(naming_id);
        }
    });
    if let Some(name) = preset {
        set_param(f, &effect, &key, json!(name));
    }
    if close.is_none() && modal.should_close() {
        close = Some(false);
    }
    if close.is_none() && naming.is_none() && !commit_name && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
        close = Some(true);
    }
    if let Some(keep) = close {
        ctx.data_mut(|d| d.remove_temp::<String>(naming_id));
        close_editor(f, keep);
        return;
    }
    if load {
        let r = app.pick_file_bytes_filtered(&["shc"], |app, name, bytes| {
            let list = contours_from_shc(&bytes).map_err(|e| format!("{}: {e}", crate::file_open::display_name(&name)))?;
            let first = list.into_iter().next().ok_or_else(|| format!("{}: no contours", crate::file_open::display_name(&name)))?;
            let d = app.ui.dialogs.iter_mut().find(|d| d.kind == DialogKind::LayerStyle && d.fields.contains_key(EDITOR)).ok_or("the Contour Editor closed")?;
            let (effect, key) = editor_target(&d.fields).ok_or("the Contour Editor closed")?;
            set_param(&mut d.fields, &effect, &key, first);
            Ok(Value::Null)
        });
        report(app, r);
    }
    if save {
        let r = contour_to_shc(&c).map_err(|e| e.to_string()).and_then(|bytes| {
            let file = format!("{}.shc", name_of(&c));
            app.pick_save(&file, move |app, path| {
                let write = app.services.write.as_mut().ok_or("no writer configured")?;
                write(&path, &bytes)?;
                Ok(json!({"path": path}))
            })
        });
        report(app, r);
    }
}

/// The edited curve (levels `0..=255`) as a contour.
fn shown_contour(pts: &[[f32; 2]], corners: &[bool], name: &str) -> Contour {
    Contour::Custom {
        name: name.to_string(),
        points: pts.iter().map(|q| CurvePoint { input: q[0] / 255.0, output: q[1] / 255.0 }).collect(),
        corners: corners.to_vec(),
    }
}

/// Load, Replace or Save Contours: the library lives in the session, the file is an `.shc`.
fn library_action(app: &mut PhotocraftApp, act: Act) {
    let r = match act {
        Act::Load | Act::Replace => app.pick_file_bytes_filtered(&["shc"], move |app, name, bytes| {
            let mut p = crate::swatches_ui::source(&name, &bytes);
            p["mode"] = json!(if act == Act::Replace { "replace" } else { "append" });
            let r = app.run("layer.layerStyle.contours.load", p)?;
            let template = "Loaded contours from {file}";
            app.ui.status_error = false;
            app.ui.status = crate::i18n::fmt(tl!(template), &[("file", &crate::file_open::display_name(&name))]);
            Ok(r)
        }),
        Act::Save => app.run("layer.layerStyle.contours.save", json!({})).and_then(|r| {
            let bytes = r["data"].as_str().and_then(photocraft_engine::paint::tile::b64_decode).ok_or("the export returned no data")?;
            app.pick_save("Contours.shc", move |app, path| {
                let write = app.services.write.as_mut().ok_or("no writer configured")?;
                write(&path, &bytes)?;
                let template = "Saved contours to {file}";
                app.ui.status_error = false;
                app.ui.status = crate::i18n::fmt(tl!(template), &[("file", &crate::file_open::display_name(&path))]);
                Ok(json!({"path": path}))
            })
        }),
    };
    report(app, r);
}

fn report(app: &mut PhotocraftApp, r: Result<Value, String>) {
    if let Err(e) = r
        && e != crate::file_dialog::CANCELLED
    {
        app.ui.status_error = true;
        app.ui.status = e;
    }
}

#[cfg(test)]
mod library_tests {
    use super::*;

    #[test]
    fn cancelling_the_editor_discards_the_contour_name_draft() {
        let ctx = egui::Context::default();
        PhotocraftApp::setup_context(&ctx, Default::default());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": 32, "height": 32})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        app.run("layer.layerStyle.dropShadow", json!({})).unwrap();
        let st = app.session.active().unwrap();
        let mut fields = crate::layer_style::initial_fields(st.doc.layer(st.active_layer.unwrap()).unwrap(), None, 120.0);
        open_editor(&mut fields, "fx1", "contour");
        let naming = egui::Id::unique(("contour-editor", "fx1", "contour")).with("naming");
        ctx.data_mut(|d| d.insert_temp(naming, "Unfinished".to_string()));
        ctx.run_ui(Default::default(), |ui| editor(&mut app, ui, &mut fields)).textures_delta.clear();
        let input = egui::RawInput {
            events: vec![egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }],
            ..Default::default()
        };
        ctx.run_ui(input, |ui| editor(&mut app, ui, &mut fields)).textures_delta.clear();
        assert!(!fields.contains_key(EDITOR));
        assert!(ctx.data(|d| d.get_temp::<String>(naming)).is_none());
        assert!(app.session.presets.contours.is_empty());
    }
}
