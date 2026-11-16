//! The state of the preset panels (Gradients, Patterns, Styles, Shapes, Tool Presets, Clone Source) and the paths their
//! callers use.
//!
//! The panels share one browser, the framework's `PresetBrowser`, in `preset_browser`. Every action is an engine command
//! (`gradient.presets.*`, `pattern.presets.*`, `style.presets.*`, `shape.presets.*`, `tool.presets.*`, `cloneSource.*`);
//! the panel modules only draw and dispatch. The dock draws the bodies from the panel registry.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::PhotocraftApp;

<<<<<<< HEAD
pub(crate) mod clone_source;
pub(crate) mod tool_presets;

pub(crate) use super::preset_browser::pattern_picker;
pub use super::preset_browser::{finish_custom_shape, gradients_panel, patterns_panel, shape_picker, shapes_panel, styles_panel, tool_presets_panel};
pub use clone_source::{clone_sample_point, panel as clone_source_panel};
pub use tool_presets::select as select_tool_preset;
=======
mod clone_offsets;
mod panels;

pub use clone_offsets::{clone_mapping_at, clone_sample_point};
pub(crate) use clone_offsets::{clone_offset_edit, clone_offset_readout, remember_clone_cursor};

pub use panels::{
    clone_source_panel, finish_custom_shape, gradients_panel, patterns_panel, select_tool_preset, shape_picker, shapes_panel, styles_panel, tool_presets_panel,
    windows,
};
#[cfg(test)]
use panels::{new_tool_preset_params, tool_id};
>>>>>>> upstream/main

/// Panel state (serde, so the control channel can read and drive it).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PresetUi {
<<<<<<< HEAD
=======
    /// Floating panels.
    pub styles: bool,
    pub shapes: bool,
    pub tool_presets: bool,
    pub clone_source: bool,
    /// Last clone preview position in its destination document; never restored into another session.
    #[serde(skip)]
    pub clone_cursor: Option<(photocraft_doc::DocId, [f64; 2])>,
    /// Thumbnail size per panel (px; the minimum shows a list).
    pub sizes: BTreeMap<String, f32>,
    /// Folders whose open state differs from the default (only the first folder starts open).
    pub toggled: BTreeSet<String>,
>>>>>>> upstream/main
    /// Selected preset per panel (by name / pattern id).
    pub selected: BTreeMap<String, String>,
    /// Tool Presets: Current Tool Only.
    pub current_tool_only: bool,
    /// Custom Shape tool: the shape it draws.
    pub custom_shape: String,
    /// Folder/group order per panel, by name (#3658), like the dock's `tabOrder` (#2272). A panel
    /// with no entry keeps the library's order; unknown names are ignored and groups missing from
    /// a list follow it in library order. Saved with the UI state.
    #[serde(rename = "groupOrder", alias = "group_order", deserialize_with = "lenient_group_order", skip_serializing_if = "BTreeMap::is_empty")]
    pub group_order: BTreeMap<String, Vec<String>>,
    /// The view state of the panels on the framework's preset browser, by panel. Read leniently
    /// like `groupOrder`: a panel's entry that no longer parses is dropped, not the whole saved UI state.
    #[serde(deserialize_with = "lenient_browsers", skip_serializing_if = "BTreeMap::is_empty")]
    pub browsers: BTreeMap<String, craft::ui::patterns::presets::PresetState<super::preset_browser::PresetKey>>,
}

/// Read `groupOrder` keeping what makes sense: non-object values, non-list entries and non-string
/// names are dropped instead of failing the whole saved UI state.
const MAX_GROUP_ORDER: usize = 10_000;
fn lenient_group_order<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BTreeMap<String, Vec<String>>, D::Error> {
    let v = Value::deserialize(d)?;
    let mut out = BTreeMap::new();
    for (panel, names) in v.as_object().into_iter().flatten() {
        let Some(names) = names.as_array() else { continue };
        out.insert(panel.clone(), names.iter().filter_map(Value::as_str).take(MAX_GROUP_ORDER).map(str::to_owned).collect());
    }
    Ok(out)
}

fn lenient_browsers<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, craft::ui::patterns::presets::PresetState<super::preset_browser::PresetKey>>, D::Error> {
    let v = Value::deserialize(d)?;
    Ok(v.as_object().into_iter().flatten().filter_map(|(panel, state)| Some((panel.clone(), serde_json::from_value(state.clone()).ok()?))).collect())
}

impl PresetUi {
    /// The Custom Shape tool's shape name.
    pub fn shape(&self) -> &str {
        if self.custom_shape.is_empty() { tl!("Heart") } else { &self.custom_shape }
    }

    /// Display order of the folders for `panel`, as library indices: the stored order first
    /// (names that no longer exist are dropped), then groups missing from it in library order.
    /// Mirrors the dock's per-group tab order (#2272, #3658).
    pub(crate) fn display_order(&self, panel: &str, names: &[String]) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::with_capacity(names.len());
        for name in self.group_order.get(panel).into_iter().flatten() {
            if let Some(i) = names.iter().position(|n| n == name)
                && !out.contains(&i)
            {
                out.push(i);
            }
        }
        for i in 0..names.len() {
            if !out.contains(&i) {
                out.push(i);
            }
        }
        out
    }

    /// Move folder `from` so it is drawn before `before` (`None` = last) for `panel`. The
    /// library's own order is stored as no entry, as the dock does for its tabs.
    pub(crate) fn move_group(&mut self, panel: &str, names: &[String], from: &str, before: Option<&str>) {
        if before == Some(from) {
            return;
        }
        let mut order: Vec<String> = self.display_order(panel, names).into_iter().filter_map(|i| names.get(i).cloned()).collect();
        let Some(at) = order.iter().position(|n| n == from) else { return };
        let moved = order.remove(at);
        let to = before.and_then(|b| order.iter().position(|n| n == b)).unwrap_or(order.len());
        order.insert(to, moved);
        if order.as_slice() == names {
            self.group_order.remove(panel);
        } else {
            self.group_order.insert(panel.to_string(), order);
        }
    }
}

/// Runs the engine command `id`; a failure goes to the status line.
pub(crate) fn run(app: &mut PhotocraftApp, id: &str, p: Value) -> Option<Value> {
    match app.run(id, p) {
        Ok(v) => Some(v),
        Err(e) => {
            app.ui.status = e;
            app.ui.status_error = true;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
<<<<<<< HEAD
=======
    use crate::menus::invoke;
    use crate::state::ToolOptions;

    fn app() -> (PhotocraftApp, egui::Context) {
        let ctx = egui::Context::default();
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": 64, "height": 48})).unwrap();
        app.sync_views();
        (app, ctx)
    }

    #[test]
    fn window_menu_toggles_panels_and_color_tabs() {
        let (mut app, ctx) = app();
        for id in PANELS {
            assert!(crate::menus::is_live(id), "{id}");
        }
        invoke(&mut app, &ctx, "window.panel.gradients", Value::Null).unwrap();
        assert!(app.ui.panels.color && app.ui.dock_tabs.color == 2);
        assert_eq!(checked(&app, "window.panel.gradients"), Some(true));
        assert_eq!(checked(&app, "window.panel.patterns"), Some(false));
        invoke(&mut app, &ctx, "window.panel.patterns", Value::Null).unwrap();
        assert_eq!(app.ui.dock_tabs.color, 3);
        // Choosing it again hides the card.
        invoke(&mut app, &ctx, "window.panel.patterns", Value::Null).unwrap();
        assert!(!app.ui.panels.color);
        for (id, f) in [("window.panel.styles", 0), ("window.panel.shapes", 1), ("window.panel.toolPresets", 2), ("window.panel.cloneSource", 3)] {
            invoke(&mut app, &ctx, id, Value::Null).unwrap();
            let p = &app.ui.presets_ui;
            assert!([p.styles, p.shapes, p.tool_presets, p.clone_source][f], "{id}");
            assert_eq!(checked(&app, id), Some(true));
            invoke(&mut app, &ctx, id, json!({"show": false})).unwrap();
            assert_eq!(checked(&app, id), Some(false));
        }
    }

    #[test]
    fn tool_preset_switches_tool_and_options() {
        let (mut app, _) = app();
        app.ui.tool = Tool::Brush;
        select_tool_preset(&mut app, "Radial Gradient");
        assert_eq!(app.ui.tool, Tool::Gradient);
        assert_eq!(app.ui.tool_options.gradient_style, "radial");
        select_tool_preset(&mut app, "Soft Eraser 60 px");
        assert_eq!(app.ui.tool, Tool::Eraser);
        assert_eq!(app.session.tools.brush.size, 60.0);
        assert_eq!(tool_id(Tool::CloneStamp), "cloneStamp");
        assert_eq!(tool_id(Tool::PatternStamp), "patternStamp");
        assert_eq!(Tool::from_name(&tool_id(Tool::CustomShape)), Some(Tool::CustomShape));
    }

    /// A crop preset picked in Tool Presets fits the frame to its ratio at once, as the options
    /// bar's dropdown does.
    #[test]
    fn a_crop_tool_preset_fits_the_frame_to_its_ratio() {
        let (mut app, _) = app();
        app.ui.tool = Tool::Crop;
        crate::crop_ui::ensure_frame(&mut app);
        assert_eq!(app.ui.crop_rect, Some([0.0, 0.0, 64.0, 48.0]));
        app.run("tool.presets.new", json!({"name": "Square", "tool": "crop", "options": {"toolOptions": {"crop_ratio": "1:1"}}})).unwrap();
        select_tool_preset(&mut app, "Square");
        assert_eq!(app.ui.tool_options.crop_ratio, "1:1");
        assert_eq!(app.ui.crop_rect, Some([8.0, 0.0, 56.0, 48.0]));
    }

    /// #7356: a preset made with + keeps only its own tool's options, so choosing it doesn't
    /// reset the Crop tool's Delete Cropped Pixels or the Move tool's Auto-Select.
    #[test]
    fn new_tool_preset_stores_only_its_tools_options() {
        let (mut app, _) = app();
        app.ui.tool = Tool::RectMarquee;
        app.ui.tool_options.feather = 5.0;
        let p = new_tool_preset_params(&app, Tool::RectMarquee);
        let r = app.run("tool.presets.new", p).unwrap();
        let name = r["name"].as_str().unwrap().to_string();
        let stored = &app.session.presets.tool_presets.iter().find(|t| t.name == name).unwrap().options["toolOptions"];
        assert_eq!(stored["feather"], json!(5.0));
        assert!(stored.as_object().unwrap().keys().all(|k| !k.starts_with("crop_") && !k.starts_with("move_")), "{stored}");
        app.ui.tool_options.feather = 0.0;
        app.ui.tool_options.crop_delete = !ToolOptions::default().crop_delete;
        app.ui.tool_options.move_auto_select = !ToolOptions::default().move_auto_select;
        select_tool_preset(&mut app, &name);
        assert_eq!(app.ui.tool_options.feather, 5.0);
        assert_eq!(app.ui.tool_options.crop_delete, !ToolOptions::default().crop_delete);
        assert_eq!(app.ui.tool_options.move_auto_select, !ToolOptions::default().move_auto_select);
    }

    /// #5524 with #7356: a Blur preset keeps Blur's Sample All Layers and leaves the Magic Wand's
    /// alone, both when it is made and when it is chosen.
    #[test]
    fn new_tool_preset_stores_only_its_tools_sample_all_layers() {
        let (mut app, _) = app();
        app.ui.tool = Tool::Blur;
        app.ui.tool_options.sample_all.blur = true;
        let p = new_tool_preset_params(&app, Tool::Blur);
        assert_eq!(p["options"]["toolOptions"]["sample_all"], json!({"blur": true}));
        let name = app.run("tool.presets.new", p).unwrap()["name"].as_str().unwrap().to_string();
        app.ui.tool_options.sample_all.blur = false;
        app.ui.tool_options.sample_all.magic_wand = true;
        select_tool_preset(&mut app, &name);
        assert!(app.ui.tool_options.sample_all.blur);
        assert!(app.ui.tool_options.sample_all.magic_wand, "the Magic Wand's Sample All Layers is not Blur's");
    }

    #[test]
    fn custom_shape_tool_places_the_picked_shape() {
        let (mut app, _) = app();
        app.ui.presets_ui.custom_shape = "Star".into();
        finish_custom_shape(&mut app, [4.0, 4.0, 20.0, 20.0], true, json!("#ff0000"), Value::Null);
        let d = app.session.active().unwrap();
        let l = d.doc.layer(d.active_layer.unwrap()).unwrap();
        assert_eq!(l.name, "Star 1");
        assert!(matches!(l.content, photocraft_doc::LayerContent::Shape(_)));
    }
>>>>>>> upstream/main

    /// #3658: a group/folder drag reorders by name, stores only a real reorder, and falls back to
    /// the library's order when the saved order matches it or names a group that's gone.
    #[test]
    fn moving_a_folder_reorders_like_the_dock_tabs() {
        let names: Vec<String> = ["Grays", "Pastel", "Pure", "Dark"].iter().map(|s| s.to_string()).collect();
        let mut st = PresetUi::default();
        assert_eq!(st.display_order("swatches", &names), vec![0, 1, 2, 3]);
        st.move_group("swatches", &names, "Pastel", None);
        assert_eq!(st.group_order.get("swatches").unwrap(), &["Grays", "Pure", "Dark", "Pastel"]);
        st.move_group("swatches", &names, "Pastel", Some("Grays"));
        assert_eq!(st.group_order.get("swatches").unwrap(), &["Pastel", "Grays", "Pure", "Dark"]);
        assert_eq!(st.display_order("swatches", &names), vec![1, 0, 2, 3]);
        // Back to the library's own order: no entry, as the dock stores its default tab order.
        st.move_group("swatches", &names, "Pastel", Some("Pure"));
        assert!(!st.group_order.contains_key("swatches"), "a no-op order is not stored");
        // Out-of-range names, moves onto themselves and unknown `before` names change nothing bad.
        for (from, before) in [("Nope", None), ("Grays", Some("Grays"))] {
            let mut m = PresetUi::default();
            m.move_group("swatches", &names, from, before);
            assert!(m.group_order.is_empty(), "{from} {before:?}");
            assert_eq!(m.display_order("swatches", &names), vec![0, 1, 2, 3]);
        }
        // A missing group in a saved order follows the rest in library order.
        let mut m = PresetUi::default();
        m.group_order.insert("swatches".into(), vec!["Dark".into(), "Grays".into()]);
        assert_eq!(m.display_order("swatches", &names), vec![3, 0, 1, 2]);
    }

    /// #3658: the folder order persists with the UI state and reads leniently (an old state has no
    /// field, and garbage in one never fails the whole state).
    #[test]
    fn folder_order_persists_and_reads_leniently() {
        let old: PresetUi = serde_json::from_value(json!({"sizes": {"swatches": 22.0}})).unwrap();
        assert!(old.group_order.is_empty(), "an older state without the field still loads");
        let mut st = PresetUi::default();
        st.group_order.insert("swatches".into(), vec!["Dark".into(), "Grays".into()]);
        let back: PresetUi = serde_json::from_value(serde_json::to_value(&st).unwrap()).unwrap();
        assert_eq!(back.group_order.get("swatches").unwrap(), &["Dark", "Grays"]);
        for bad in [json!("x"), json!(7), json!({"swatches": "paths"}), json!({"swatches": [1, null, {"a": 1}]})] {
            let back: PresetUi = serde_json::from_value(json!({"groupOrder": bad})).unwrap();
            assert!(back.group_order.get("swatches").is_none_or(Vec::is_empty), "{bad}");
        }
        let mut st = PresetUi::default();
        st.browsers.insert("patterns".into(), Default::default());
        st.group_order.insert("patterns".into(), vec!["Rocks".into()]);
        st.selected.insert("patterns".into(), "p1".into());
        let mut saved = serde_json::to_value(&st).unwrap();
        saved["browsers"]["styles"] = json!("a state of an older format");
        let back: PresetUi = serde_json::from_value(saved).unwrap();
        assert!(back.browsers.contains_key("patterns") && !back.browsers.contains_key("styles"), "only the entry that fails is dropped");
        assert_eq!((back.group_order, back.selected), (st.group_order, st.selected), "the rest of the state loads");
    }
}
