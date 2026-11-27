//! The Layers panel's rows on the rt runtime inside egui (spike g1b, stage a).
//!
//! The rows block of `layers()` becomes one embedded rt window over the same rect. The model is rebuilt from the
//! document only when its version key moves; the panel's edits come back as PhotoCraft's own commands in `layers()`'s
//! `actions` vec. `PHOTOCRAFT_LAYERS_RT=0` (read once) keeps the egui rows, as the measurement control.

mod actions;
mod model;
mod theme;

use craft_rt_layers::{Doc, DocCell, List};
use craft_ui_embed_egui::{EmbedStats, EmbeddedWindow};
use craft_ui_rt::Os;
use photocraft_doc::{Document, LayerId};
use serde_json::{Value, json};

use super::{PhotocraftApp, Tokens, vec2};
use crate::state::LayerThumbnailSize;
use crate::theme::ThemeKind;

/// Whether the rt rows are on: everything but `PHOTOCRAFT_LAYERS_RT=0`.
pub(super) fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("PHOTOCRAFT_LAYERS_RT").map_or(true, |v| v != "0"))
}

/// The binding's state on `PhotocraftApp`: the embedded window, the cell the panel reads and what was last given to it.
pub(crate) struct LayersRt {
    window: EmbeddedWindow,
    cell: DocCell,
    key: Option<model::ModelKey>,
    theme: Option<(ThemeKind, LayerThumbnailSize, u32)>,
    thumbs: model::Thumbs,
    eye_sweep: Option<u64>,
    stats: Option<EmbedStats>,
}

impl LayersRt {
    fn new(ctx: &egui::Context, tokens: craft_ui_rt::Tokens) -> LayersRt {
        let mut window = EmbeddedWindow::new(ctx, egui::Id::new("photocraft-layers-rt"), tokens);
        let cell = DocCell::create(window.runtime_mut().store_mut(), Doc::from_rows(Vec::new(), [], None));
        let os = Os::current();
        window.set_root(move |cx| {
            cx.child(List { doc: cell, os });
        });
        LayersRt { window, cell, key: None, theme: None, thumbs: model::Thumbs::default(), eye_sweep: None, stats: None }
    }
}

/// `perf.rtPanel` of `ui.inspect`: the last frame of the embedded panel, or null when it is off or not yet shown.
pub(crate) fn inspect(app: &PhotocraftApp) -> Value {
    let Some(s) = app.layers_rt.as_ref().and_then(|rt| rt.stats) else { return Value::Null };
    json!({
        "step_ms": s.step_ms, "emit_ms": s.emit_ms, "ran": s.ran, "frames": s.frames, "events": s.events,
        "built": s.built, "laid_out": s.laid_out, "painted": s.painted, "prepared": s.prepared, "reused": s.reused,
        "meshes": s.meshes, "vertices": s.vertices,
    })
}

/// What `layers()` knows of the active document.
pub(super) struct Rows<'a> {
    pub doc: &'a Document,
    pub revision: u64,
    pub selection: &'a [LayerId],
    pub active: Option<LayerId>,
    pub isolated: &'a [LayerId],
    /// The height of the rows' rect (the panel's room above the footer).
    pub height: f32,
}

/// Shows the rows over `rows.height` of `ui`, and pushes the edits the panel made into `actions`.
pub(super) fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui, rows: &Rows<'_>, actions: &mut Vec<(String, Value)>) {
    let ctx = ui.ctx().clone();
    let t = Tokens::get(&ctx);
    let size = app.ui.layers_panel_options.thumbnail_size;
    let body = ui.style().text_styles.get(&egui::TextStyle::Body).map_or(13.0, |f| f.size);
    let mut rt = match app.layers_rt.take() {
        Some(rt) => rt,
        None => LayersRt::new(&ctx, theme::rt_tokens(&t, size, body)),
    };

    let theme_key = (t.kind, size, body.to_bits());
    if rt.theme != Some(theme_key) {
        rt.window.runtime_mut().set_tokens(theme::rt_tokens(&t, size, body));
        rt.theme = Some(theme_key);
    }

    let key = model::ModelKey {
        doc: rows.doc.id.0,
        revision: rows.revision,
        theme: t.kind,
        thumbnail: size,
        filter: crate::layer_filter_ui::Filter::of(&app.ui),
        isolation: rows.isolated.to_vec(),
        type_mask: app.ui.text_edit.as_ref().filter(|ed| ed.mask).map(|ed| ed.layer),
        clip: app.session.prefs().interface.clip_thumbnails_to_layer_bounds,
    };
    if rt.key.as_ref() != Some(&key) {
        let with_thumbs = size != LayerThumbnailSize::None;
        let runtime = rt.window.runtime_mut();
        let new = model::build(app, rows.doc, rows.selection, rows.active, rows.isolated, runtime.images_mut(), &mut rt.thumbs, with_thumbs);
        rt.cell.set_doc(rt.window.runtime_mut().store_mut(), new);
        rt.key = Some(key);
    }

    let (_, rect) = ui.allocate_space(vec2(ui.available_width(), rows.height));
    rt.stats = Some(rt.window.show(ui, rect).stats);

    let ops = rt.cell.take_edits(rt.window.runtime_mut().store_mut());
    if !ops.is_empty() {
        // The commands run at the end of this pass; the next pass shows their result.
        ctx.request_repaint();
    }
    let mut frame =
        actions::Frame { doc: rows.doc, eye_sweep: &mut rt.eye_sweep, pass: ctx.cumulative_pass_nr(), primary_down: ui.input(|i| i.pointer.primary_down()) };
    actions::to_actions(ops, &mut frame, actions);
    app.layers_rt = Some(rt);
}
