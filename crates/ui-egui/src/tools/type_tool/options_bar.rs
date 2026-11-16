use super::defaults::{target, target_doc};
use super::{LayerId, PhotocraftApp, byte_of, fonts, insertion_style, json, selected_styles, text_layer, values};
use craft::text::ui::type_bar::TypeBar;

/// Photoshop's Type options bar: the members of the type tool, shown inside the host's options `Bar`, which adds
/// the commit and cancel buttons while a type session is open.
pub fn options_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let character = values::character_values(app);
    let paragraph = values::paragraph_values(app);
    let families = fonts::families();
    let styles = values::style_names(app, &character);
    let methods = values::anti_alias_methods();
    let mut font_state = fonts::font_state(app);
    let mut bar = TypeBar::new(&character, &paragraph, &families, &styles)
        .unit(values::doc_unit(app))
        .orientation(values::orientation(app))
        .anti_alias_methods(&methods)
        .can_warp(crate::menus::is_enabled(app, "type.warpText"));
    bar = bar.font_picker(font_state.source());
    // A mixed selection leaves the anti-aliasing menu without a shown method.
    if let Some(method) = values::anti_alias(app) {
        bar = bar.anti_alias(method);
    }
    let mut out = bar.show(ui);
    fonts::finish_font_state(app, font_state);
    values::apply_bar_events(app, &out.edits);
    values::follow_family(app, out.preview_family.as_deref(), out.rested_family.as_deref(), values::bar_family_gesture(&out.edits));
    if let Some(request) = out.requests.take_color() {
        values::pick_color(app, request.color.map(|c| [c.r, c.g, c.b]));
    }
    let ctx = ui.ctx().clone();
    for (pressed, command) in [(out.warp, "type.warpText"), (out.panels, "window.panel.character")] {
        if pressed && let Err(e) = crate::menus::invoke(app, &ctx, command, json!({})) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
    }
<<<<<<< HEAD
=======
    // Font family and style menus (one placeholder in their place while the fonts load), in one
    // scope so the controls after them keep their ids when the fonts finish loading: a click
    // pressed before and released after still lands. The salt keeps the menus' ids apart from the
    // Character panel's font menus.
    ui.push_id("type-options-fonts", |ui| {
        if fonts_ready(ui, 170.0 + ui.spacing().item_spacing.x + 110.0) {
            if let Some(picked) = font_picker(app, ui, &mut fam, 170.0) {
                app.ui.tool_options.type_font = fam.clone();
                let st = styles(&fam);
                let picked = picked.filter(|s| st.contains(s));
                style = match &picked {
                    Some(s) => s.clone(),
                    None if st.contains(&style) => style.clone(),
                    None => st.first().cloned().unwrap_or_else(|| "Regular".into()),
                };
                app.ui.tool_options.type_style = style.clone();
                // Only a style picked under the family's ">" is set: otherwise each selected layer
                // keeps its own face in the new family (#3556).
                match picked {
                    Some(s) => apply(app, ui.ctx(), json!({"font": fam, "fontStyle": s})),
                    None => apply(app, ui.ctx(), json!({"font": fam})),
                }
            }
            font_picker_tip(ui, tl!("Search and select fonts"));
            let style_family = if fam.is_empty() { styles_at(app).map(|(c, _)| c.font_family).unwrap_or_default() } else { fam.clone() };
            let opts: Vec<(String, String)> = styles(&style_family).into_iter().map(|s| (s.clone(), style_label(&s))).collect();
            let opts_ref: Vec<(String, &str)> = opts.iter().map(|(a, b)| (a.clone(), b.as_str())).collect();
            if crate::widgets::dropdown(ui, "type-style", tl!("Style"), &mut style, &opts_ref, 110.0) {
                app.ui.tool_options.type_style = style.clone();
                apply(app, ui.ctx(), json!({"fontStyle": style}));
            }
            // The menu's button, by the id its combo box takes from "type-style"; "Style" stays its name.
            if let Some(menu) = ui.ctx().read_response(ui.make_persistent_id(egui::IdSalt::new("type-style"))) {
                menu.on_hover_text(tl!("Set font style"));
            }
        }
    });
    let (r, icon) = ui.allocate_exact_size(egui::vec2(18.0, 22.0), egui::Sense::hover());
    crate::icons::paint(ui, r, "type", 13.0, t.icon);
    icon.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, tl!("Font size")));
    // Photoshop's "T" scrubs the size field beside it, like the Character panel's icons (#6437).
    crate::widgets::scrub_option_icon(ui, &icon, tl!("Set font size"));
    let (unit, label) = type_unit(app);
    let mut display = size * unit;
    let edited = size_field(ui, &mut display, unit, label);
    // A preset is already in points: converting it through the display unit would not round-trip.
    if let Some(size) = match edited {
        Some(MetricChoice::Typed(v)) => Some(v / unit),
        Some(MetricChoice::Preset(pt)) => Some(pt),
        _ => None,
    } {
        app.ui.tool_options.type_size = size;
        apply(app, ui.ctx(), json!({"size": size, "metricsAsShown": true}));
    }
    let mut aa = target(app)
        .and_then(|(id, _)| app.session.active().and_then(|st| text_layer(&st.doc, LayerId(id))))
        .map(|t| photocraft_engine::type_extra_cmds::aa_name(t.antialias).to_string())
        .unwrap_or_else(|| o.type_aa.clone());
    let aa_opts = [
        ("none".to_string(), tl!("None")),
        ("sharp".to_string(), tl!("Sharp")),
        ("crisp".to_string(), tl!("Crisp")),
        ("strong".to_string(), tl!("Strong")),
        ("smooth".to_string(), tl!("Smooth")),
    ];
    if crate::widgets::dropdown(ui, "type-aa", tl!("Anti-aliasing method"), &mut aa, &aa_opts, 80.0) {
        app.ui.tool_options.type_aa = aa.clone();
        if let Some(params) = formatting_params(app) {
            let _ = app.run(&format!("type.antiAlias.{aa}"), params);
        }
    }
    crate::widgets::vline(ui, 22.0);
    ui.spacing_mut().item_spacing.x = 2.0;
    // Alignment and direction show the target paragraph's, as Photoshop's bar follows the text
    // being edited or the selected type layer; the tool defaults otherwise. A justified paragraph
    // lights none of the three.
    let para = styles_at(app).map(|(_, p)| p);
    let shown_align = match &para {
        Some(p) => match p.align {
            photocraft_doc::text::TextAlign::Left => "left",
            photocraft_doc::text::TextAlign::Center => "center",
            photocraft_doc::text::TextAlign::Right => "right",
            _ => "",
        },
        None => o.type_align.as_str(),
    };
    // Vertical text shows Top / Center / Bottom for the same left / center / right values, after the
    // targeted layer's orientation, or with no text targeted the vertical tools' (#12059, #12088).
    // The Horizontal Type Mask tool keeps the horizontal buttons (not measured).
    let vertical = app.ui.tool != crate::state::Tool::HorizontalTypeMask
        && match super::defaults::target_text(app) {
            Some(t) => t.orientation == photocraft_doc::text::Orientation::Vertical,
            None => app.ui.tool.is_vertical_type(),
        };
    let buttons = if vertical {
        [
            ("left", "vertical-align-top", tl!("Top align text")),
            ("center", "vertical-align-middle", tl!("Center text")),
            ("right", "vertical-align-bottom", tl!("Bottom align text")),
        ]
    } else {
        [("left", "align-left", tl!("Left align text")), ("center", "align-center", tl!("Center text")), ("right", "align-right", tl!("Right align text"))]
    };
    for (align, icon, tip) in buttons {
        if crate::icons::button(ui, icon, 24.0, shown_align == align, tip).clicked() {
            app.ui.tool_options.type_align = align.into();
            apply(app, ui.ctx(), json!({"align": align}));
        }
    }
    ui.spacing_mut().item_spacing.x = 8.0;
    crate::widgets::vline(ui, 22.0);
    ui.spacing_mut().item_spacing.x = 2.0;
    for (key, tip, d) in direction_items() {
        let on = para.as_ref().map_or(o.type_direction == key, |p| p.direction == d);
        if direction_button(ui, d, on, tip).clicked() {
            app.ui.tool_options.type_direction = key.into();
            apply(app, ui.ctx(), json!({"direction": key}));
        }
    }
    ui.spacing_mut().item_spacing.x = 8.0;
    crate::widgets::vline(ui, 22.0);
    // Text colour swatch with a picker popup.
    let c = shown
        .as_ref()
        .map(|s| s.3)
        .map(|c| {
            let v = c.to_rgb();
            [v[0], v[1], v[2], 1.0]
        })
        .unwrap_or(app.session.tools.foreground);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(28.0, 18.0), egui::Sense::click());
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(q(c[0]), q(c[1]), q(c[2])));
    ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, t.field_border), egui::StrokeKind::Outside);
    let resp = resp.on_hover_text(tl!("Set text color"));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("Set the text color")));
    if resp.clicked() {
        open_color_picker(app, [c[0], c[1], c[2]]);
    }
    // Create warped text, after the swatch as in Photoshop (#5911): the Type › Warp Text… dialog.
    let can_warp = crate::menus::is_enabled(app, "type.warpText");
    if ui.add_enabled_ui(can_warp, |ui| crate::icons::button(ui, "text-warp", 24.0, false, tl!("Create warped text"))).inner.clicked()
        && let Err(e) = crate::menus::invoke(app, &ui.ctx().clone(), "type.warpText", json!({}))
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
    // The bar's last button, as in Photoshop (#12054): Window › Character, which shows the
    // Character | Paragraph group with Character in front, and hides it when clicked again.
    if crate::icons::button(ui, "panels-top-left", 24.0, false, tl!("Toggle Character and Paragraph panels")).clicked()
        && let Err(e) = crate::menus::invoke(app, &ui.ctx().clone(), "window.panel.character", json!({}))
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
    if app.ui.text_edit.is_some() {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            if crate::icons::button(
                ui,
                "check",
                24.0,
                false,
                &crate::i18n::fmt(tl!("Commit any current edits ({key})"), &[("key", &crate::shortcuts::pretty("Cmd+Enter"))]),
            )
            .clicked()
            {
                commit_with(app, ui.input(|i| i.modifiers));
            }
            if crate::icons::button(ui, "ban", 24.0, false, tl!("Cancel any current edits (Esc)")).clicked() {
                cancel(app);
            }
        });
    }
>>>>>>> upstream/main
}

/// A blank family represents mixed fonts, rather than falsely reporting the first one after
/// commit. At a caret, the current/pending insertion font is the relevant single style.
pub(super) fn shown_family(app: &PhotocraftApp, c: &photocraft_doc::text::CharStyle) -> String {
    let same = |s: &photocraft_doc::text::CharStyle| s.font_family == c.font_family;
    let Some(st) = target_doc(app) else { return c.font_family.clone() };
    let uniform = if let Some(ed) = &app.ui.text_edit {
        if ed.caret == ed.anchor {
            true
        } else {
            text_layer(&st.doc, LayerId(ed.layer)).is_none_or(|t| selected_styles(t, ed.anchor, ed.caret).iter().all(same))
        }
    } else {
        st.selected_layers().into_iter().filter_map(|id| text_layer(&st.doc, id)).all(|t| t.char_runs().iter().all(|r| same(&r.style)))
    };
    if uniform { c.font_family.clone() } else { String::new() }
}

/// Character and paragraph style at the selection, caret, or first run of selected layers.
/// The paragraph at a collapsed caret is the caret's, as the options bar and Paragraph panel show it.
pub(crate) fn styles_at(app: &PhotocraftApp) -> Option<(photocraft_doc::text::CharStyle, photocraft_doc::text::ParagraphStyle)> {
    let (layer, range) = target(app)?;
    let t = text_layer(&target_doc(app)?.doc, LayerId(layer))?;
    let at = range.map_or_else(|| app.ui.text_edit.as_ref().map_or(0, |ed| byte_of(&t.text, ed.caret.saturating_sub(1))), |r| byte_of(&t.text, r[0]));
    let para_at = match (range, &app.ui.text_edit) {
        (None, Some(ed)) => byte_of(&t.text, ed.caret),
        _ => at,
    };
    let pick_at = |at: usize, lens: Vec<usize>| -> usize {
        let mut acc = 0;
        for (i, len) in lens.iter().enumerate() {
            acc += len;
            if at < acc {
                return i;
            }
        }
        lens.len().saturating_sub(1)
    };
    let runs = t.char_runs();
    let paras = t.paragraph_runs();
    let mut c = runs.get(pick_at(at, runs.iter().map(|r| r.len).collect())).map(|r| r.style.clone())?;
    insertion_style::shown(app, &mut c);
    let p = paras.get(pick_at(para_at, paras.iter().map(|r| r.len).collect())).map(|r| r.style.clone()).unwrap_or_default();
    Some((c, p))
}

/// Kerning shown for the target: at a collapsed caret, the pair before it (Photoshop's
/// Character panel); else the style at the selection.
pub(super) fn kerning_at(app: &PhotocraftApp) -> Option<(photocraft_doc::text::Kerning, f32)> {
    let ed = app.ui.text_edit.as_ref().filter(|e| e.caret == e.anchor && e.caret > 0)?;
    let st = app.session.active()?;
    let t = text_layer(&st.doc, LayerId(ed.layer))?;
    let at = byte_of(&t.text, ed.caret - 1);
    let mut end = 0;
    t.char_runs()
        .into_iter()
        .find(|r| {
            end += r.len;
            at < end
        })
        .map(|r| (r.style.kerning, r.style.kern))
}

#[cfg(test)]
#[path = "fonts_loading_tests.rs"]
mod fonts_loading_tests;

#[cfg(test)]
#[path = "size_presets_tests.rs"]
mod size_presets_tests;

#[cfg(test)]
#[path = "vertical_mask_align_buttons_tests.rs"]
mod vertical_mask_align_buttons_tests;
