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
#[path = "size_presets_tests.rs"]
mod size_presets_tests;

#[cfg(test)]
#[path = "vertical_mask_align_buttons_tests.rs"]
mod vertical_mask_align_buttons_tests;
