//! Layers panel flyout and display preferences, observed from Photoshop's panel menu.
//! UI options do not edit document pixels or add history steps.
use craft_ui::composites::GroupBox;
use craft_ui::controls::{Checkbox, RadioGroup, RadioSet};
use craft_ui::theme::StrokeExt;
use egui::{self, Ui};
use photocraft_doc::Document;
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;
use crate::state::{DialogKind, LayerThumbnailContents, LayerThumbnailSize, LayersPanelOptions};

const OPTIONS_PREF: &str = "ui.layersPanelOptions";

/// Save independently of Workspace > Remember Workspace Changes, as Photoshop does.
pub fn persist_options(app: &mut PhotocraftApp) {
    if let Ok(value) = serde_json::to_value(&app.ui.layers_panel_options) {
        app.session.prefs.edit(|p| {
            p.dialogs.insert(OPTIONS_PREF.into(), value);
        });
    }
}

/// Restore independently from a saved workspace. Older preferences retain the defaults.
pub fn restore_options(app: &mut PhotocraftApp) {
    if let Some(opts) = app.session.prefs().dialogs.get(OPTIONS_PREF)
        && let Ok(value) = serde_json::from_value(opts.clone())
    {
        app.ui.layers_panel_options = value;
    }
}

/// The real menu command dispatcher handles rename's in-place editor and other shell routing.
pub fn run_menu_command(app: &mut PhotocraftApp, ui: &mut Ui, label: &str, id: &str) {
    if ui.add_enabled(app.session.is_enabled(id), egui::Button::new(label)).clicked() {
        let ctx = ui.ctx().clone();
        if let Err(e) = crate::menus::invoke(app, &ctx, id, json!({})) {
            app.ui.status = e;
            app.ui.status_error = true;
        }
        ui.close();
    }
}

/// Thumbnail Contents as the shared preference has it: Layer Bounds is Clip Thumbnails to Layer
/// Bounds (#4281), which frames each layer's pixels instead of the canvas.
pub fn thumbnail_contents(app: &PhotocraftApp) -> LayerThumbnailContents {
    if app.session.prefs().interface.clip_thumbnails_to_layer_bounds { LayerThumbnailContents::LayerBounds } else { LayerThumbnailContents::EntireDocument }
}

/// Panel Options › OK: the panel's own options, and Thumbnail Contents into the shared preference.
fn apply_options(app: &mut PhotocraftApp, options: LayersPanelOptions) -> Result<(), String> {
    let contents = options.thumbnail_contents;
    app.ui.layers_panel_options = options;
    persist_options(app);
    if contents == thumbnail_contents(app) {
        return Ok(());
    }
    let clip = contents == LayerThumbnailContents::LayerBounds;
    app.run("prefs.set", json!({"path": "interface.clipThumbnailsToLayerBounds", "value": clip})).map(|_| ())
}

/// Preview cache recomputes only when hovering a new target, not each pointer frame.
/// Compositor sees it as a temporary document and never commits it to history.
pub(crate) struct ReorderPreview {
    pub source: photocraft_doc::DocId,
    pub revision: u64,
    pub signature: u64,
    pub key: u64,
    pub document: std::sync::Arc<Document>,
}

/// Cache the on-canvas preview of the layer drop in `payload` (a `layer.moveTo` request), unless Preview on Canvas is off.
/// Marks the destination as hovered every frame; the preview itself is rebuilt only when the target changes.
pub(crate) fn preview_reorder(app: &mut PhotocraftApp, payload: &serde_json::Value) {
    if !app.ui.layers_panel_options.preview_on_canvas_when_reordering_layers {
        app.reorder_preview = None;
        return;
    }
    // Mark the destination as hovered even if the cached preview already matches.
    app.reorder_preview_hit = true;
    let Some(st) = app.session.active() else { return };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    payload.to_string().hash(&mut hasher);
    st.doc.id.0.hash(&mut hasher);
    st.revision.hash(&mut hasher);
    let signature = hasher.finish();
    if app.reorder_preview.as_ref().is_some_and(|cache| cache.signature == signature) {
        return;
    }
    let (source, revision) = (st.doc.id, st.revision);
    let add_copy = app.ui.layers_panel_options.add_copy_to_copied_layers_and_groups;
    let preview = photocraft_engine::layer_multi_cmds::preview_move(&st.doc, payload, add_copy);
    app.reorder_preview = preview.map(|document| ReorderPreview {
        source,
        revision,
        signature,
        key: (1u64 << 60) | (signature & ((1u64 << 60) - 1)),
        document: std::sync::Arc::new(document),
    });
}

/// The cached reorder preview and its compositor cache key for document `index`, if it was built for that document's current revision.
pub(crate) fn display_reorder(app: &PhotocraftApp, index: usize) -> Option<(std::sync::Arc<Document>, u64)> {
    if !app.ui.layers_panel_options.preview_on_canvas_when_reordering_layers {
        return None;
    }
    let st = app.session.documents().get(index)?;
    let cached = app.reorder_preview.as_ref()?;
    (cached.source == st.doc.id && cached.revision == st.revision).then(|| (cached.document.clone(), cached.key))
}

/// Photoshop shows the same flower at three resolutions. This example is original,
/// vector-painted artwork, not an extracted Adobe asset, and it is independent of
/// whichever layer happens to be selected (including adjustment layers).
fn paint_flower_example(ui: &Ui, rect: egui::Rect) {
    use egui::{Color32, Rect as UiRect, Shape, Stroke, pos2, vec2};
    let painter = ui.painter().with_clip_rect(rect);
    let side = rect.width();
    let point = |x: f32, y: f32| pos2(rect.min.x + x * side, rect.min.y + y * side);
    // A monochrome miniature landscape: light sky, shaded hills and a dark meadow.
    for n in 0..22 {
        let y0 = n as f32 / 22.0;
        let y1 = (n + 1) as f32 / 22.0;
        let f = n as f32 / 22.0;
        painter.rect_filled(
            UiRect::from_min_max(point(0.0, y0), point(1.0, y1)),
            craft_ui::theme::scale::Radius::R0.px(),
            Color32::from_gray((119.0 + f * 30.0) as u8),
        );
    }
    painter.add(Shape::convex_polygon(
        vec![point(0.0, 0.60), point(0.22, 0.36), point(0.46, 0.56), point(0.72, 0.33), point(1.0, 0.62)],
        Color32::from_gray(80),
        Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        vec![point(0.0, 0.76), point(0.32, 0.60), point(0.57, 0.69), point(1.0, 0.51), point(1.0, 1.0), point(0.0, 1.0)],
        Color32::from_gray(64),
        Stroke::NONE,
    ));
    for n in 0..16 {
        let x = ((n * 43 + 13) % 97) as f32 / 96.0;
        let y = 0.78 + ((n * 11 % 19) as f32) / 100.0;
        painter.line_segment([point(x, 1.0), point((x + 0.02).min(1.0), y)], Stroke::new(side * 0.01, Color32::from_gray(104)));
    }
    // A white daisy with layered petals and a dark central disc, in the same
    // composition at all three sizes, so the controls genuinely preview scale.
    let center = point(0.49, 0.57);
    let unit = side * 0.34;
    for n in 0..15 {
        let theta = (n as f32 + 0.5) * std::f32::consts::TAU / 15.0;
        let direction = vec2(theta.cos(), theta.sin());
        let normal = vec2(-theta.sin(), theta.cos());
        let mut outline = Vec::with_capacity(20);
        for i in 0..=9 {
            let t = i as f32 / 9.0;
            let base = center + direction * unit * (0.22 + 0.83 * t);
            outline.push(base + normal * (unit * 0.22 * (std::f32::consts::PI * t).sin()));
        }
        for i in (0..=9).rev() {
            let t = i as f32 / 9.0;
            let base = center + direction * unit * (0.22 + 0.83 * t);
            outline.push(base - normal * (unit * 0.22 * (std::f32::consts::PI * t).sin()));
        }
        painter.add(Shape::convex_polygon(
            outline,
            if n % 3 == 0 { Color32::from_gray(213) } else { Color32::from_gray(238) },
            Stroke::new(0.3, Color32::from_gray(155)),
        ));
    }
    painter.circle_filled(center, unit * 0.27, Color32::from_gray(33));
    painter.circle_filled(center - vec2(unit * 0.065, unit * 0.055), unit * 0.16, Color32::from_gray(55));
    for n in 0..16 {
        let theta = (n as f32) * std::f32::consts::TAU / 16.0;
        let r = unit * if n % 2 == 0 { 0.16 } else { 0.21 };
        painter.circle_filled(center + vec2(theta.cos(), theta.sin()) * r, (side * 0.008).max(0.3), Color32::from_gray(119));
    }
    painter.rect_stroke(
        rect,
        craft_ui::theme::scale::Radius::R0.px(),
        craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), Color32::from_gray(92)),
        egui::StrokeKind::Inside,
    );
}

/// Returns both the actual thumbnail width and the height allocated by the Layers
/// row. Used by painting and by tests to keep the preference functional.
pub(crate) fn thumbnail_geometry(size: LayerThumbnailSize, pro: bool) -> (f32, f32) {
    let thumb: f32 = match size {
        LayerThumbnailSize::None => 0.0,
        LayerThumbnailSize::Small => 18.0,
        LayerThumbnailSize::Medium => {
            if pro {
                24.0
            } else {
                34.0
            }
        }
        LayerThumbnailSize::Large => 48.0,
    };
    let row = if thumb == 0.0 { 23.0 } else { (thumb + 8.0).max(if pro { 32.0 } else { 40.0 }) };
    (thumb, row)
}

/// Marks the Layers panel's own dialogs (Panel Options, Lock Layers); its value names which.
const MARK: &str = "__layersPanel";

pub fn owns(fields: &Map<String, Value>) -> bool {
    fields.contains_key(MARK)
}

fn is_lock(fields: &Map<String, Value>) -> bool {
    fields.get(MARK).and_then(Value::as_str) == Some("lock")
}

/// The content width of the dialog with `fields`.
pub fn width(fields: &Map<String, Value>) -> craft_ui::dialog::Width {
    if is_lock(fields) { craft_ui::dialog::Width::Compact } else { craft_ui::dialog::Width::Fixed(360.0) }
}

/// The Panel Options dialog's `fields`: the options under their saved names, and Thumbnail Contents
/// (`thumbnailContents`), which is saved in the shared preference instead.
fn fields_of(options: &LayersPanelOptions) -> Map<String, Value> {
    let mut fields = match serde_json::to_value(options) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    };
    fields.insert("thumbnailContents".into(), json!(options.thumbnail_contents));
    fields
}

fn options_of(fields: &Map<String, Value>) -> LayersPanelOptions {
    let mut options: LayersPanelOptions = serde_json::from_value(Value::Object(fields.clone())).unwrap_or_default();
    options.thumbnail_contents = fields.get("thumbnailContents").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    options
}

/// Opens Panel Options. Cancel never changes the persisted options.
pub fn open_options(app: &mut PhotocraftApp) -> u64 {
    let mut options = app.ui.layers_panel_options.clone();
    options.thumbnail_contents = thumbnail_contents(app);
    let mut fields = fields_of(&options);
    fields.insert(MARK.into(), json!("options"));
    fields.insert("__label".into(), json!("Layers Panel Options"));
    app.ui.open_dialog(DialogKind::Command, fields)
}

/// Opens Lock Layers (the panel menu's), with Lock All set as the dialog has always started.
pub fn open_lock(app: &mut PhotocraftApp) -> u64 {
    let mut fields = Map::new();
    fields.insert(MARK.into(), json!("lock"));
    fields.insert("__label".into(), json!("Lock Layers"));
    for key in LOCKS.map(|(key, _)| key) {
        fields.insert(key.into(), json!(key == "all"));
    }
    app.ui.open_dialog(DialogKind::Command, fields)
}

/// The locks of Lock Layers, in the order the dialog lists them.
const LOCKS: [(&str, &str); 5] =
    [("transparency", "Transparency"), ("pixels", "Image"), ("position", "Position"), ("artboard", "Prevent Auto-Nesting"), ("all", "All")];

pub fn body(ui: &mut Ui, fields: &mut Map<String, Value>) {
    if is_lock(fields) {
        lock_body(ui, fields);
    } else {
        options_body(ui, fields);
    }
}

fn lock_body(ui: &mut Ui, fields: &mut Map<String, Value>) {
    for (key, label) in LOCKS {
        let mut on = fields.get(key).and_then(Value::as_bool).unwrap_or(false);
        if Checkbox::bool(tl!(label), &mut on).show(ui).response.changed() {
            fields.insert(key.into(), json!(on));
        }
    }
}

/// Content column of the Photoshop-style dialog, with fixed-size radio/preview
/// slots rather than text labels that change the position of each thumbnail.
fn options_body(ui: &mut Ui, fields: &mut Map<String, Value>) {
    let mut draft = options_of(fields);
    let before = draft.clone();
    GroupBox::new(tl!("Thumbnail Size")).show(ui, |ui| {
        // The original Photoshop dialog has None with a label, then progressively
        // larger previews, with the radio controls aligned vertically.
        let sizes = [LayerThumbnailSize::None, LayerThumbnailSize::Small, LayerThumbnailSize::Medium, LayerThumbnailSize::Large];
        let mut picked = None;
        let mut set = RadioSet::new("layers-thumbnail-size", tl!("Thumbnail Size"), &mut draft.thumbnail_size);
        for size in sizes {
            set = set.item(size);
        }
        let mut set = set.begin(ui);
        set.radio(ui, 0, tl!("None"));
        for (index, side, name) in [(1, 38.0, tl!("Small")), (2, 68.0, tl!("Medium")), (3, 96.0, tl!("Large"))] {
            ui.horizontal(|ui| {
                // Empty radio captions are intentional: the sample itself conveys
                // size. The accessible/hover label still names the setting.
                set.radio_named(ui, index, "", name).on_hover_text(name);
                let (rect, response) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click());
                paint_flower_example(ui, rect);
                if response.clicked() {
                    picked = Some(sizes[index]);
                }
                response.on_hover_text(name);
            });
        }
        let _ = set.end(ui);
        if let Some(size) = picked {
            draft.thumbnail_size = size;
        }
    });
    GroupBox::new(tl!("Thumbnail Contents")).show(ui, |ui| {
        let _ = RadioGroup::new("layers-thumbnail-contents", tl!("Thumbnail Contents"), &mut draft.thumbnail_contents)
            .item(LayerThumbnailContents::LayerBounds, tl!("Layer Bounds"))
            .item(LayerThumbnailContents::EntireDocument, tl!("Entire Document"))
            .show(ui);
    });
    let _ = Checkbox::bool(tl!("Use Default Masks on Fill Layers"), &mut draft.use_default_masks_on_fill_layers).show(ui);
    let _ = Checkbox::bool(tl!("Expand New Effects"), &mut draft.expand_new_effects).show(ui);
    let _ = Checkbox::bool(tl!("Add “copy” to Copied Layers and Groups"), &mut draft.add_copy_to_copied_layers_and_groups).show(ui);
    let _ = Checkbox::bool(tl!("Preview on canvas when reordering layers"), &mut draft.preview_on_canvas_when_reordering_layers).show(ui);
    let _ = Checkbox::bool(tl!("Show layer mask badges"), &mut draft.show_layer_mask_badges).show(ui);
    if draft != before {
        *fields = fields_of(&draft).into_iter().chain(fields.iter().filter(|(k, _)| k.starts_with("__")).map(|(k, v)| (k.clone(), v.clone()))).collect();
    }
}

/// OK in either dialog: Panel Options sets the options, Lock Layers runs `layer.lockLayers`.
pub fn confirm(app: &mut PhotocraftApp, fields: &Map<String, Value>) -> Result<Value, String> {
    if is_lock(fields) {
        let params: Map<String, Value> =
            LOCKS.iter().map(|(key, _)| (key.to_string(), json!(fields.get(*key).and_then(Value::as_bool).unwrap_or(false)))).collect();
        app.run("layer.lockLayers", Value::Object(params))
    } else {
        apply_options(app, options_of(fields)).map(|()| Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh install shows Medium thumbnails of the whole document, like Photoshop's Panel Options defaults.
    #[test]
    fn default_panel_options_match_medium_document_previews() {
        let options = LayersPanelOptions::default();
        assert_eq!(options.thumbnail_size, LayerThumbnailSize::Medium);
        assert_eq!(options.thumbnail_contents, LayerThumbnailContents::EntireDocument);
        assert!(options.show_filters);
        assert!(options.use_default_masks_on_fill_layers);
        assert!(options.expand_new_effects);
        assert!(options.add_copy_to_copied_layers_and_groups);
        assert!(options.preview_on_canvas_when_reordering_layers);
        assert!(!options.show_layer_mask_badges);
    }

    /// Thumbnail Contents is the same setting as the panel menu's Clip Thumbnails to Layer Bounds
    /// (#4281): the dialog shows it and OK sets it, so the two never disagree.
    #[test]
    fn thumbnail_contents_is_clip_thumbnails_to_layer_bounds() {
        let mut session = photocraft_engine::Session::new();
        session.execute("file.new", json!({"width": 8, "height": 8})).unwrap();
        let mut app = PhotocraftApp::new(session, crate::Services::default());
        assert_eq!(thumbnail_contents(&app), LayerThumbnailContents::EntireDocument);
        let mut options = app.ui.layers_panel_options.clone();
        options.thumbnail_contents = LayerThumbnailContents::LayerBounds;
        apply_options(&mut app, options.clone()).unwrap();
        assert!(app.session.prefs().interface.clip_thumbnails_to_layer_bounds);
        assert_eq!(thumbnail_contents(&app), LayerThumbnailContents::LayerBounds);
        assert!(app.session.prefs().dialogs[OPTIONS_PREF].get("thumbnailContents").is_none(), "saved once, in the shared preference");
        options.thumbnail_contents = LayerThumbnailContents::EntireDocument;
        apply_options(&mut app, options).unwrap();
        assert!(!app.session.prefs().interface.clip_thumbnails_to_layer_bounds);
    }

    /// The options survive a UiState round trip.
    #[test]
    fn options_round_trip() {
        let mut ui = crate::state::UiState::default();
        ui.layers_panel_options.thumbnail_size = LayerThumbnailSize::Large;
        ui.layers_panel_options.show_filters = false;
        let json = serde_json::to_value(&ui).unwrap();
        let restored: crate::state::UiState = serde_json::from_value(json).unwrap();
        assert_eq!(restored.layers_panel_options, ui.layers_panel_options);
    }

    /// Panel Options is a dialog of JSON fields: setting them and confirming applies the options,
    /// Thumbnail Contents included, and cancelling changes nothing.
    #[test]
    fn panel_options_dialog_applies_its_fields_on_confirm() {
        let mut session = photocraft_engine::Session::new();
        session.execute("file.new", json!({"width": 8, "height": 8})).unwrap();
        let mut app = PhotocraftApp::new(session, crate::Services::default());
        let id = open_options(&mut app);
        let fields = &mut app.ui.dialog_mut(id).unwrap().fields;
        assert_eq!(fields["thumbnailSize"], "medium");
        fields.insert("thumbnailSize".into(), json!("large"));
        fields.insert("thumbnailContents".into(), json!("layerBounds"));
        crate::dialogs::cancel(&mut app, id).unwrap();
        assert_eq!(app.ui.layers_panel_options.thumbnail_size, LayerThumbnailSize::Medium);
        let id = open_options(&mut app);
        let fields = &mut app.ui.dialog_mut(id).unwrap().fields;
        fields.insert("thumbnailSize".into(), json!("large"));
        fields.insert("thumbnailContents".into(), json!("layerBounds"));
        crate::dialogs::confirm(&mut app, id).unwrap();
        assert_eq!(app.ui.layers_panel_options.thumbnail_size, LayerThumbnailSize::Large);
        assert_eq!(thumbnail_contents(&app), LayerThumbnailContents::LayerBounds);
    }

    /// With the option off a new fill layer has no mask and the mask is not targeted; with it on the new layer's mask is targeted.
    #[test]
    fn disabling_default_fill_masks_never_targets_an_absent_mask() {
        let mut session = photocraft_engine::Session::new();
        session.execute("file.new", json!({"width": 32, "height": 24})).unwrap();
        let mut app = PhotocraftApp::new(session, crate::Services::default());
        app.ui.layers_panel_options.use_default_masks_on_fill_layers = false;
        persist_options(&mut app);
        app.run("layer.newFillLayer.solidColor", json!({"color": "#778899"})).unwrap();
        let layer = app.session.active().and_then(|st| st.active_layer.and_then(|id| st.doc.layer(id))).unwrap();
        assert!(layer.mask.is_none());
        assert!(!app.ui.mask_target && !app.ui.vector_mask_target);
        app.ui.layers_panel_options.use_default_masks_on_fill_layers = true;
        persist_options(&mut app);
        app.run("layer.newFillLayer.gradient", json!({})).unwrap();
        let layer = app.session.active().and_then(|st| st.active_layer.and_then(|id| st.doc.layer(id))).unwrap();
        assert!(layer.mask.is_some());
        assert!(app.ui.mask_target);
    }

    /// Every option, including the mask badges, goes to the preferences and comes back from them.
    #[test]
    fn prefs_options_save_and_restore_all_switches() {
        let mut session = photocraft_engine::Session::new();
        session.execute("file.new", json!({"width": 8, "height": 8})).unwrap();
        let mut app = PhotocraftApp::new(session, crate::Services::default());
        let saved = LayersPanelOptions {
            thumbnail_size: LayerThumbnailSize::Large,
            thumbnail_contents: LayerThumbnailContents::EntireDocument,
            show_filters: false,
            use_default_masks_on_fill_layers: false,
            expand_new_effects: false,
            add_copy_to_copied_layers_and_groups: false,
            preview_on_canvas_when_reordering_layers: false,
            show_layer_mask_badges: true,
        };
        app.ui.layers_panel_options = saved.clone();
        persist_options(&mut app);
        assert_eq!(app.session.prefs().dialogs[OPTIONS_PREF]["showLayerMaskBadges"], true);
        app.ui.layers_panel_options = LayersPanelOptions::default();
        restore_options(&mut app);
        assert_eq!(app.ui.layers_panel_options, saved);
    }

    /// The flyout's Flatten Image runs the real `layer.flattenImage` command, and Undo brings the layers back.
    #[test]
    fn flyout_flatten_routes_to_real_engine_and_undo_restores_layers() {
        let mut session = photocraft_engine::Session::new();
        session.execute("file.new", json!({"width": 18, "height": 12})).unwrap();
        session.execute("layer.new.layer", json!({"name": "Red pixels"})).unwrap();
        session.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
        let mut app = PhotocraftApp::new(session, crate::Services::default());
        let count = app.session.active().unwrap().doc.layers.len();
        assert!(count >= 2);
        // The flyout uses menus::invoke with this exact command id.
        crate::menus::invoke(&mut app, &egui::Context::default(), "layer.flattenImage", json!({})).unwrap();
        let flattened = app.session.active().unwrap();
        assert_eq!(flattened.doc.layers.len(), 1);
        assert_eq!(flattened.doc.layers[0].name, "Background");
        assert!(flattened.doc.layers[0].surface().is_some());
        app.run("edit.undo", json!({})).unwrap();
        assert_eq!(app.session.active().unwrap().doc.layers.len(), count);
    }

    /// Thumbnail and row sizes grow from None to Large in both themes, with the rows at least as tall as the thumbnails.
    #[test]
    fn geometry_matches_four_real_panel_sizes() {
        for pro in [false, true] {
            let mut previous = -1.0;
            for size in [LayerThumbnailSize::None, LayerThumbnailSize::Small, LayerThumbnailSize::Medium, LayerThumbnailSize::Large] {
                let (thumb, row) = thumbnail_geometry(size, pro);
                assert!(thumb > previous && row >= thumb, "{size:?}: {thumb}/{row}");
                previous = thumb;
            }
        }
        assert_eq!(thumbnail_geometry(LayerThumbnailSize::None, true), (0.0, 23.0));
        assert_eq!(thumbnail_geometry(LayerThumbnailSize::Large, true), (48.0, 56.0));
    }

    /// Preferences saved before an option existed still load, with the default for the missing ones.
    #[test]
    fn panel_options_missing_fields_deserialize_with_defaults() {
        let options: LayersPanelOptions = serde_json::from_value(json!({})).unwrap();
        assert_eq!(options, LayersPanelOptions::default());
        let saved = serde_json::to_value(&options).unwrap();
        assert_eq!(saved["thumbnailSize"], "medium");
    }
}
