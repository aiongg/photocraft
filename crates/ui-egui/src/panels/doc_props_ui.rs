//! Properties panel › Document: what Photoshop shows when nothing (or the Background layer) is
//! selected. Canvas size and orientation, resolution, colour mode and bit depth, and the
//! Rulers & Grids toggles. Every edit dispatches an engine or view command, so the panel stays thin.

use craft_ui::composites::FieldControl as _;
use craft_ui::composites::{PropertyGroup, PropertyLabel, PropertyRow};
use craft_ui::controls::{ControlSize, IconButton, NumberField};
use craft_ui::icons::Icon;
use craft_ui::localized::{LocaleNumbers, Unit};
use craft_ui::theme::{StrokeExt, ToEgui, Tokens as LibTokens, TokensOf as _};
use egui::{RichText, Sense, StrokeKind, vec2};
use photocraft_doc::{ColorMode, Document, Layer, LayerId, SampleType};
use serde_json::{Value, json};

use crate::layer_props_ui::{committed, number};
use crate::props_layout::section;
use crate::{PhotocraftApp, icons, widgets};

/// Photoshop's Background layer: the locked, opaque raster layer at the bottom of the stack,
/// whatever its (localized) name ([`Document::background`]).
pub fn is_background(doc: &Document, l: &Layer) -> bool {
    doc.is_background(l.id)
}

/// Does the Properties panel show the Document view? (No layer selected, or the Background layer.)
pub fn shows_document(doc: &Document, active: Option<LayerId>) -> bool {
    match active.and_then(|id| doc.layer(id)) {
        None => true,
        Some(l) => is_background(doc, l),
    }
}

/// Image › Mode command id and label per colour mode, in Photoshop's dropdown order.
pub const MODES: &[(ColorMode, &str, &str)] = &[
    (ColorMode::Bitmap, "Bitmap", "image.mode.bitmap"),
    (ColorMode::Grayscale, "Grayscale", "image.mode.grayscale"),
    (ColorMode::Duotone, "Duotone", "image.mode.duotone"),
    (ColorMode::Indexed, "Indexed Color", "image.mode.indexedColor"),
    (ColorMode::Rgb, "RGB Color", "image.mode.rgb"),
    (ColorMode::Cmyk, "CMYK Color", "image.mode.cmyk"),
    (ColorMode::Lab, "Lab Color", "image.mode.lab"),
    (ColorMode::Multichannel, "Multichannel", "image.mode.multichannel"),
];

pub const DEPTHS: &[(SampleType, &str, &str)] = &[
    (SampleType::U8, "8 Bits/Channel", "image.mode.bits8"),
    (SampleType::U16, "16 Bits/Channel", "image.mode.bits16"),
    (SampleType::F32, "32 Bits/Channel", "image.mode.bits32"),
];

/// Ruler units offered in the Rulers & Grids dropdown (`unitsAndRulers.rulers` values).
pub const UNITS: &[(&str, &str)] = &[
    ("pixels", "Pixels"),
    ("inches", "Inches"),
    ("cm", "Centimeters"),
    ("mm", "Millimeters"),
    ("points", "Points"),
    ("picas", "Picas"),
    ("percent", "Percent"),
];

/// `image.canvasSize` params for a new width/height, keeping the aspect ratio when linked.
pub fn canvas_resize_params(old: (u32, u32), new_w: Option<f32>, new_h: Option<f32>, linked: bool) -> Option<Value> {
    let (ow, oh) = (old.0 as f32, old.1 as f32);
    let (mut w, mut h) = (new_w.unwrap_or(ow).round().max(1.0), new_h.unwrap_or(oh).round().max(1.0));
    if linked {
        if new_w.is_some() {
            h = (w * oh / ow).round().max(1.0);
        } else if new_h.is_some() {
            w = (h * ow / oh).round().max(1.0);
        }
    }
    ((w, h) != (ow, oh)).then(|| json!({"width": w, "height": h, "anchor": "center"}))
}

pub fn properties(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = craft_ui::theme::current(ui.ctx());
    let Some(st) = app.session.active() else { return };
    let (w, h, mode, depth, dpi) = (st.doc.size.width, st.doc.size.height, st.doc.mode, st.doc.depth, st.doc.resolution_dpi);
    let has_bg = st.doc.layers.first().is_some_and(|l| is_background(&st.doc, l));
    let lu = crate::layer_props_ui::LenUnit::of(app);
    let mut run: Vec<(String, Value)> = Vec::new();
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
        ui.painter().rect_stroke(
            r,
            t.shape.radius.control,
            craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.color.line.control_edge.to_egui()),
            StrokeKind::Inside,
        );
        icons::paint(ui, r, "file", 15.0, t.color.text.normal.to_egui());
        ui.label(RichText::new(tl!("Document")).color(t.color.text.normal.to_egui()));
    });
    ui.add_space(t.space.gap.stack);
    widgets::hairline(ui);
    ui.add_space(craft_ui::theme::scale::Space::S2.px());
    let tokens = LibTokens::of(ui);
    let row_style = PropertyRow::style(&tokens, ControlSize::Md);
    // The word column plus its gap: where the rows without a label start their control.
    let indent = row_style.word_width + row_style.label_gap;
    if section(ui, "canvas", tl!("Canvas")) {
        let link_key = egui::Id::unique("doc-props-link");
        let linked = ui.data(|d| d.get_temp::<bool>(link_key)).unwrap_or(false);
        let keys = ["w", "h"].map(|id| ui.make_persistent_id(("doc-props-field", id)));
        let numbers = LocaleNumbers::of(ui);
        // W and H with the link toggle between them; X and Y under them are greyed out
        // (Photoshop greys them for documents without artboards).
        let size = PropertyGroup::new("doc-props-canvas")
            .show(ui, |ui| {
                let size = PropertyRow::pair(PropertyLabel::glyph(tl!("W")), PropertyLabel::glyph(tl!("H")))
                    .show_linked(
                        ui,
                        |ui, _| number(ui, "doc-w", keys[0], "Width", w as f32, (lu, f64::from(w))),
                        |ui| {
                            let icon = if linked { Icon::Link } else { Icon::Unlink };
                            if ui.add(IconButton::new(icon, tl!("Link width and height")).size(ControlSize::Sm).selected(linked)).clicked() {
                                ui.data_mut(|d| d.insert_temp(link_key, !linked));
                            }
                        },
                        |ui, _| number(ui, "doc-h", keys[1], "Height", h as f32, (lu, f64::from(h))),
                    )
                    .inner;
                let dim =
                    |ui: &mut egui::Ui, id: &str, name: &str| NumberField::new(id, tl!(name), 0.0, &numbers).unit(Unit::new("{n} px")).enabled(false).show(ui);
                PropertyRow::pair(PropertyLabel::glyph(tl!("X")), PropertyLabel::glyph(tl!("Y"))).reserve_link_slot().show(
                    ui,
                    |ui, _| dim(ui, "doc-props-x", "X"),
                    |ui, _| dim(ui, "doc-props-y", "Y"),
                );
                size
            })
            .inner;
        if let Some(v) = committed(ui, keys[0], &size.0, w as f32, (lu, f64::from(w)), 0.5) {
            run.extend(canvas_resize_params((w, h), Some(v.clamp(1.0, 300_000.0)), None, linked).map(|p| ("image.canvasSize".to_string(), p)));
        }
        if let Some(v) = committed(ui, keys[1], &size.2, h as f32, (lu, f64::from(h)), 0.5) {
            run.extend(canvas_resize_params((w, h), None, Some(v.clamp(1.0, 300_000.0)), linked).map(|p| ("image.canvasSize".to_string(), p)));
        }
        ui.horizontal(|ui| {
            ui.add_space(indent);
            ui.spacing_mut().item_spacing.x = 2.0;
            let portrait = h > w;
            if icons::button(ui, "rectangle-vertical", 26.0, portrait, "Portrait").clicked() && !portrait && w != h {
                run.push(("image.canvasSize".into(), json!({"width": h, "height": w, "anchor": "center"})));
            }
            if icons::button(ui, "rectangle-horizontal", 26.0, !portrait, tl!("Landscape")).clicked() && portrait {
                run.push(("image.canvasSize".into(), json!({"width": h, "height": w, "anchor": "center"})));
            }
        });
        ui.add_space(craft_ui::theme::scale::Space::S2.px());
        ui.horizontal(|ui| {
            ui.add_space(indent);
            ui.label(
                RichText::new(crate::i18n::fmt(tl!("Resolution: {dpi} pixels/inch"), &[("dpi", &widgets::fmt_num(dpi as f64))]))
                    .color(t.color.text.normal.to_egui()),
            );
        });
        ui.add_space(craft_ui::theme::scale::Space::S2.px());
        // Mode, with the bit depth under it, then the canvas fill.
        PropertyRow::word(tl!("Mode")).show(ui, |ui, _| {
            ui.vertical(|ui| {
                let mut m = mode;
                let opts: Vec<(ColorMode, &str)> = MODES.iter().map(|(m, l, _)| (*m, *l)).collect();
                if widgets::dropdown(ui, "doc-props-mode", tl!("Color Mode"), &mut m, &opts, 150.0)
                    && let Some((_, _, id)) = MODES.iter().find(|(x, _, _)| *x == m)
                {
                    run.push(((*id).to_string(), Value::Null));
                }
                let mut d = depth;
                let opts: Vec<(SampleType, &str, &str)> =
                    DEPTHS.iter().map(|(d, l, _)| (*d, *l, if *d == SampleType::F32 { "Floating point" } else { "Integer" })).collect();
                if widgets::dropdown_with_tooltips(ui, "doc-props-depth", tl!("Depth"), &mut d, &opts, 150.0)
                    && let Some((_, _, id)) = DEPTHS.iter().find(|(x, _, _)| *x == d)
                {
                    run.push(((*id).to_string(), Value::Null));
                }
            })
            .response
            .own_name()
        });
        PropertyRow::word(tl!("Fill")).show(ui, |ui, _| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                ui.painter().rect_stroke(
                    r,
                    craft_ui::theme::scale::Radius::R0.px(),
                    craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.color.line.control_edge.to_egui()),
                    StrokeKind::Inside,
                );
                // The canvas fill is only editable for documents without a Background layer.
                ui.add_enabled_ui(!has_bg, |ui| {
                    let mut f = 0u8;
                    widgets::dropdown(ui, "doc-props-fill", tl!("Background"), &mut f, &[(0u8, tl!("Background Color"))], 124.0);
                });
            })
            .response
            .own_name()
        });
        ui.add_space(craft_ui::theme::scale::Space::S4.px());
    }
    widgets::hairline(ui);
    ui.add_space(craft_ui::theme::scale::Space::S2.px());
    if section(ui, "rulers", tl!("Rulers & Grids")) {
        ui.horizontal(|ui| {
            ui.add_space(craft_ui::theme::scale::Space::S8.px());
            ui.spacing_mut().item_spacing.x = t.space.gap.inline;
            let pixel_grid = crate::view_cmds::checked(app, "view.show.pixelGrid").unwrap_or(false);
            for (icon, tip, id, on) in [
                ("ruler", crate::shortcuts::tip_label(app, "Rulers", "view.rulers"), "view.rulers", app.ui.extras.rulers),
                ("grid-3x3", crate::shortcuts::tip_label(app, "Grid", "view.show.grid"), "view.show.grid", app.ui.extras.grid),
                ("grid-2x2", crate::shortcuts::tip_label(app, "Pixel Grid", "view.show.pixelGrid"), "view.show.pixelGrid", pixel_grid),
            ] {
                if icons::button(ui, icon, 26.0, on, &tip).clicked() {
                    run.push((id.to_string(), Value::Null));
                }
            }
            ui.add_space(craft_ui::theme::scale::Space::S8.px());
            let mut unit = app.session.prefs().units_and_rulers.rulers.name().to_string();
            let opts: Vec<(String, &str)> = UNITS.iter().map(|(k, l)| (k.to_string(), *l)).collect();
            if widgets::dropdown(ui, "doc-props-units", tl!("Units"), &mut unit, &opts, 96.0) {
                run.push(("prefs.set".into(), json!({"path": "unitsAndRulers.rulers", "value": unit})));
            }
        });
        ui.add_space(craft_ui::theme::scale::Space::S4.px());
    }
    widgets::hairline(ui);
    ui.add_space(craft_ui::theme::scale::Space::S2.px());
    if section(ui, "guides", tl!("Guides")) {
        ui.horizontal(|ui| {
            ui.add_space(craft_ui::theme::scale::Space::S8.px());
            ui.spacing_mut().item_spacing.x = t.space.gap.inline;
            for (icon, tip, id) in [
                ("eye", crate::shortcuts::tip_label(app, "Show Guides", "view.show.guides"), "view.show.guides"),
                ("lock", crate::shortcuts::tip_label(app, "Lock Guides", "view.lockGuides"), "view.lockGuides"),
            ] {
                let on = if id == "view.lockGuides" { app.ui.extras.lock_guides } else { app.ui.extras.guides };
                if icons::button(ui, icon, 26.0, on, &tip).clicked() {
                    run.push((id.to_string(), Value::Null));
                }
            }
            for (label, id) in [(tl!("New Guide…"), "view.newGuide"), (tl!("Clear"), "view.clearGuides")] {
                if crate::menus::is_enabled(app, id) && widgets::secondary_button(ui, label, 0.0).clicked() {
                    run.push((id.to_string(), Value::Null));
                }
            }
        });
    }
    let ctx = ui.ctx().clone();
    for (id, params) in run {
        let params = if params.is_null() { json!({}) } else { params };
        if let Err(e) = crate::menus::invoke(app, &ctx, &id, params) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_doc::Document;

    fn doc_with_background() -> photocraft_engine::Session {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": 40, "height": 20, "background": "white"})).unwrap();
        s
    }

    #[test]
    fn background_selection_shows_document() {
        let mut s = doc_with_background();
        let st = s.active().unwrap();
        let bg = st.doc.layers[0].id;
        assert!(is_background(&st.doc, &st.doc.layers[0]));
        assert!(shows_document(&st.doc, Some(bg)));
        assert!(shows_document(&st.doc, None));
        s.execute("layer.new.layer", json!({"name": "Ink"})).unwrap();
        let st = s.active().unwrap();
        let ink = st.active_layer.unwrap();
        assert!(!shows_document(&st.doc, Some(ink)));
        assert!(!is_background(&st.doc, st.doc.layer(ink).unwrap()));
    }

    #[test]
    fn transparent_documents_have_no_background() {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": 40, "height": 20, "background": "transparent"})).unwrap();
        let st = s.active().unwrap();
        let d: &Document = &st.doc;
        assert!(d.layers.iter().all(|l| !is_background(d, l)));
    }

    #[test]
    fn canvas_resize_keeps_ratio_when_linked() {
        assert_eq!(canvas_resize_params((2400, 1500), Some(1200.0), None, true), Some(json!({"width": 1200.0, "height": 750.0, "anchor": "center"})));
        assert_eq!(canvas_resize_params((2400, 1500), None, Some(3000.0), true), Some(json!({"width": 4800.0, "height": 3000.0, "anchor": "center"})));
        assert_eq!(canvas_resize_params((2400, 1500), Some(1000.0), None, false), Some(json!({"width": 1000.0, "height": 1500.0, "anchor": "center"})));
        assert_eq!(canvas_resize_params((2400, 1500), Some(2400.2), None, false), None);
    }

    /// #5661: a 1 x 1 px document at 300 ppi is 0.0033 in wide. With the rulers in inches the
    /// Width/Height fields clamped that to 0.01 in (3 px) and resized the document unasked.
    #[test]
    fn tiny_document_in_inches_is_not_resized_by_showing_it() {
        use egui_kittest::Harness;
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 1, "height": 1, "resolution": 300, "background": "white"})).unwrap();
        app.session.prefs.edit(|p| p.units_and_rulers.rulers = photocraft_engine::prefs::Unit::Inches);
        let undo = app.session.active().unwrap().history.entries().len();
        let mut h = Harness::builder().with_size(vec2(320.0, 600.0)).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                    properties(app, ui);
                }
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
        h.run_steps(3);
        let st = h.state().session.active().unwrap();
        assert_eq!((st.doc.size.width, st.doc.size.height), (1, 1));
        assert_eq!(st.history.entries().len(), undo, "no history step");
    }

    /// Horizontal centre of every single-letter label the panel drew, by letter.
    fn letter_centres(h: &egui_kittest::Harness<'_, PhotocraftApp>) -> Vec<(String, f32)> {
        h.output()
            .shapes
            .iter()
            .filter_map(|s| if let egui::Shape::Text(t) = &s.shape { Some(t) } else { None })
            .filter(|t| t.galley.text().chars().count() == 1)
            .map(|t| (t.galley.text().to_string(), t.pos.x + t.galley.size().x * 0.5))
            .collect()
    }

    /// #7314: the Canvas section's W over H (and X over Y) share a centre; a narrow "H" used to sit
    /// flush right under the wider "W".
    #[test]
    fn canvas_size_labels_are_centred_on_each_other() {
        use egui_kittest::Harness;
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 40, "height": 20, "background": "white"})).unwrap();
        let mut h = Harness::builder().with_size(vec2(320.0, 600.0)).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                    properties(app, ui);
                }
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
        h.run_steps(3);
        let centres = letter_centres(&h);
        let at = |l: &str| centres.iter().find(|(s, _)| s == l).map(|(_, x)| *x).unwrap_or_else(|| panic!("no {l} label in {centres:?}"));
        assert!((at("W") - at("H")).abs() < 0.01, "W and H: {centres:?}");
        assert!((at("X") - at("Y")).abs() < 0.01, "X and Y: {centres:?}");
    }

    #[test]
    fn document_dropdowns_map_to_live_mode_commands() {
        for id in MODES.iter().map(|m| m.2).chain(DEPTHS.iter().map(|d| d.2)) {
            assert!(photocraft_engine::commands::find(id).is_some(), "{id} is not an engine command");
        }
    }

    #[test]
    fn image_mode_depth_labels_match_sample_types() {
        assert_eq!(
            DEPTHS.iter().map(|(sample, label, _)| (*sample, *label)).collect::<Vec<_>>(),
            [(SampleType::U8, "8 Bits/Channel"), (SampleType::U16, "16 Bits/Channel"), (SampleType::F32, "32 Bits/Channel"),]
        );
    }
}
