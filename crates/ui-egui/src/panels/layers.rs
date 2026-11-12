use super::{BlendMode, Layer, LayerId, PhotocraftApp, Rect, Sense, Tokens, Value, empty, icons, json, vec2};

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

pub(super) fn layers(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    use craft::layers::ui::panel::{LayersDrag, LayersPanel};
    use craft::layers::ui::thumbs::LayerThumb;
    use craft::ui::contract::Bind;
    use craft::ui::primitives::textures::{ThumbnailOptions, Thumbnails};

    use super::layers_tree::{self, PhotoLayers, PhotoThumbs};

    // A Move-way pick of a layer opens its groups only when the Move option says so (#11381).
    let reveal = crate::layer_reveal::track(app, ui.ctx());
    let ctx = ui.ctx().clone();
    // A Layers panel row drag can be dropped on another document's tab (#589).
    if let Some(drag) = craft::ui::runtime::drag::active::<LayersDrag<LayerId>>(&ctx) {
        let layers: Vec<LayerId> = drag.layers().into_iter().copied().collect();
        crate::layer_transfer::begin_from_panel(app, &ctx, &layers);
    }
    let Some(st) = app.session.active() else {
        empty(ui, tl!("No document"));
        return;
    };
    // While the drag shows another document, its rows take no drop.
    let foreign = crate::layer_transfer::source(&ctx).is_some_and(|source| source != st.doc.id);
    let settings = layers_tree::settings(app);
    let mut filter = layers_tree::filter_of(&app.ui);
    let before = filter.clone();
    let picked = crate::smart_ui::selected(&ctx, &st.doc, st.active_layer);
    let tree = PhotoLayers::new(app, st, &filter, foreign, picked);
    let thumbs = Thumbnails::<LayerThumb<LayerId>>::in_context(&ctx, "photocraft-layer-thumbs", ThumbnailOptions::new());
    let source = PhotoThumbs::new(app, &st.doc);
    let mut panel = LayersPanel::new(&tree)
        .settings(&settings)
        .thumbnails(&thumbs, &source)
        .filter(Bind::bound(&mut filter))
        .row_menu(|target| layers_tree::row_menu(&tree, target))
        .document(st.doc.id);
    if let Some((id, groups)) = reveal {
        panel = panel.reveal(id, groups);
    }
    if let Some(key) = crate::shortcuts::chord(app, "layer.hideLayers") {
        panel = panel.visibility_key(key);
    }
    if app.find_layers_focus {
        panel = panel.focus_filter_name();
    }
    if let Some(id) = app.layers_rename {
        panel = panel.start_rename(id);
    }
    let out = panel.show(ui);
    drop(tree);
    app.find_layers_focus = false;
    app.layers_rename = None;
    if filter != before {
        layers_tree::store_filter(&mut app.ui, &filter);
    }
    if out.hovered.is_some() {
        app.hover_layer = out.hovered;
    }
    // Preview on Canvas When Reordering Layers: the move the release would make.
    match out.drop_preview.as_ref().filter(|_| app.ui.layers_panel_options.preview_on_canvas_when_reordering_layers) {
        Some(mv) => crate::layers_panel_ui::preview_reorder(app, &super::layers_events::move_params(mv)),
        None => app.reorder_preview = None,
    }
    super::layers_events::apply_layers_events(app, &ctx, out.events);
}

/// The small down-arrow makes it clear that the footer icon expands into a menu.
fn footer_menu_arrow(ui: &egui::Ui, rect: Rect) {
    let t = Tokens::get(ui.ctx());
    let arrow = Rect::from_min_size(rect.right_bottom() - vec2(11.0, 10.0), vec2(9.0, 9.0));
    icons::paint(ui, arrow, "chevron-down", 8.0, t.text_dim);
}

/// Photoshop's italic "fx" footer button (no icon-font equivalent).
pub(crate) fn fx_button(ui: &mut egui::Ui, size: f32, tip: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, t.radius_sm, t.hover);
    }
    let mut job = egui::text::LayoutJob::default();
    job.append("fx", 0.0, egui::TextFormat { font_id: egui::FontId::proportional(15.0), color: t.icon, italics: true, ..Default::default() });
    let g = ui.painter().layout_job(job);
    ui.painter().galley(r.center() - g.size() / 2.0, g, t.icon);
    footer_menu_arrow(ui, r);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
    resp.on_hover_text(tip)
}

pub(super) fn channels(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    crate::channels_panel::show(app, ui);
}
