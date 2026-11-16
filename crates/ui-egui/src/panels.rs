//! Chrome around the canvas: title bar, options bar, toolbar, status bar, dock cards, Properties.

use craft::ui::patterns::blend_keys::{BlendKeyEntry, BlendShortcut};
use egui::{Align2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, pos2, vec2};
use photocraft_color::BlendMode;
use photocraft_doc::{Layer, LayerContent, LayerId};
use photocraft_engine::history_source_cmds::HistorySource;
use serde_json::{Value, json};

use crate::state::Tool;
use crate::theme::{self, Tokens};
use crate::{PhotocraftApp, icons};

pub mod actions;
pub mod adjust_ui;
pub mod apply_color;
pub(crate) mod blend_preview;
#[cfg(test)]
mod blend_rows_tests;
pub(super) mod brand;
pub mod brush_panel;
pub mod brush_preview;
pub mod brush_sections;
mod brush_settings;
mod brush_settings_events;
pub mod channels_panel;
pub mod chrome_ui;
pub mod clip_line_ui;
pub(crate) mod color;
pub mod color_hud;
pub mod color_panel_ui;
pub mod comps_ui;
pub mod doc_props_ui;
pub mod histogram_panel;
pub(crate) mod history;
pub mod history_menu_ui;
pub(crate) mod info;
pub mod layer_filter_ui;
pub mod layer_menu_ui;
pub mod layer_props_ui;
pub(super) mod layer_reveal;
pub mod layer_row_ui;
pub(super) mod layer_transfer;
mod layers;
pub(crate) mod layers_events;
pub mod layers_panel_ui;
pub(crate) mod layers_tree;
pub(super) mod mask_props_ui;
pub mod mask_thumbs_ui;
mod navigator;
pub(crate) mod options_bar;
pub(super) mod panel_docking;
pub(crate) mod preset_browser;
pub mod preset_panels;
mod properties;
pub(crate) mod rail_commands;
pub(crate) mod registry;
pub mod smart_ui;
mod status_bar;
pub mod swatches_ui;
pub(super) mod timeline_ui;
pub(crate) mod title_bar;
pub(crate) mod tool_commands;
pub(crate) mod toolbar;
pub mod type_panels_ui;
pub(super) mod variables_ui;

#[cfg(test)]
pub(crate) use history::select_snapshot;
pub(crate) use history::selected_snapshot;
pub(crate) use info::{SamplerReadouts, sampler_readouts};
pub(crate) use layers::{blend_options, fx_button};
pub(crate) use layers_tree::listed_filter;
pub use options_bar::options_bar;
pub use status_bar::status_bar;
pub use title_bar::{title_bar, window_title};
pub use toolbar::toolbar;
#[cfg(test)]
use toolbar::{TOOL_SECTIONS, color_chips};

/// Photoshop's Shift+Alt letter position of blend mode `m`; `None` for the modes without a letter (Darker Color,
/// Lighter Color, Subtract, Divide) and for PhotoCraft's own extra modes.
pub(crate) fn blend_shortcut(m: BlendMode) -> Option<BlendShortcut> {
    Some(match m {
        BlendMode::Normal => BlendShortcut::Normal,
        BlendMode::Dissolve => BlendShortcut::Dissolve,
        BlendMode::PassThrough => BlendShortcut::PassThrough,
        BlendMode::Darken => BlendShortcut::Darken,
        BlendMode::Multiply => BlendShortcut::Multiply,
        BlendMode::ColorBurn => BlendShortcut::ColorBurn,
        BlendMode::LinearBurn => BlendShortcut::LinearBurn,
        BlendMode::Lighten => BlendShortcut::Lighten,
        BlendMode::Screen => BlendShortcut::Screen,
        BlendMode::ColorDodge => BlendShortcut::ColorDodge,
        BlendMode::LinearDodge => BlendShortcut::LinearDodge,
        BlendMode::Overlay => BlendShortcut::Overlay,
        BlendMode::SoftLight => BlendShortcut::SoftLight,
        BlendMode::HardLight => BlendShortcut::HardLight,
        BlendMode::VividLight => BlendShortcut::VividLight,
        BlendMode::LinearLight => BlendShortcut::LinearLight,
        BlendMode::PinLight => BlendShortcut::PinLight,
        BlendMode::HardMix => BlendShortcut::HardMix,
        BlendMode::Difference => BlendShortcut::Difference,
        BlendMode::Exclusion => BlendShortcut::Exclusion,
        BlendMode::Hue => BlendShortcut::Hue,
        BlendMode::Saturation => BlendShortcut::Saturation,
        BlendMode::Color => BlendShortcut::Color,
        BlendMode::Luminosity => BlendShortcut::Luminosity,
        _ => return None,
    })
}

/// The entry of blend mode `m` under the app's `key`, for every blend list the framework builds.
pub(crate) fn blend_entry<K>(key: K, m: BlendMode) -> BlendKeyEntry<K> {
    match blend_shortcut(m) {
        Some(s) => BlendKeyEntry::new(key, s),
        None => BlendKeyEntry::unlettered(key),
    }
}

#[cfg(test)]
mod color_tests;

#[cfg(test)]
<<<<<<< HEAD
=======
mod info_panel_sampler_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    /// #1046: the Info panel lists each Color Sampler point under its number, with its position and
    /// the composite RGB/CMYK readout, one composite pixel each.
    #[test]
    fn info_panel_lists_the_sample_points() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 80, "height": 60, "background": "white"})).unwrap();
        app.run("view.colorSamplers.add", json!({"x": 5, "y": 6})).unwrap();
        app.run("view.colorSamplers.add", json!({"x": 40, "y": 30})).unwrap();
        let mut h = Harness::builder().with_size(vec2(340.0, 400.0)).build_ui_state(|ui, app: &mut PhotocraftApp| info_panel(app, ui), app);
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Pro);
        h.run_steps(3);
        // Both points are listed, numbered, and read as white (RGB 255 / CMYK 0%).
        assert!(h.query_by_label("#1").is_some(), "point 1 is listed");
        assert!(h.query_by_label("#2").is_some(), "point 2 is listed");
        assert_eq!(h.query_all_by_label_contains("R:255 G:255 B:255").count(), 2, "each point shows its RGB");
        assert_eq!(h.query_all_by_label_contains("C:0% M:0% Y:0% K:0%").count(), 2, "each point shows its CMYK");
        // Clearing them takes the list away.
        h.state_mut().run("view.colorSamplers.clear", json!({})).unwrap();
        h.run_steps(2);
        assert!(h.query_by_label("#1").is_none(), "no points listed once cleared");
    }
}

#[cfg(test)]
mod history_transform_tests {
    use super::*;
    use egui_kittest::Harness;

    fn click(h: &mut Harness<'static, PhotocraftApp>, p: egui::Pos2) {
        h.hover_at(p);
        h.run_steps(1);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            h.run_steps(1);
        }
    }

    /// While Free Transform is open, clicking a History state leaves the document alone: the
    /// transform owns Undo. Without a transform the same click steps back as usual.
    #[test]
    fn history_rows_wait_for_an_open_transform() {
        for transforming in [true, false] {
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
            app.run("file.new", json!({"width": 200, "height": 150})).unwrap();
            app.run("layer.new.layer", json!({})).unwrap();
            app.run("select.rect", json!({"x": 20, "y": 20, "width": 60, "height": 40})).unwrap();
            app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
            app.run("select.deselect", json!({})).unwrap();
            let mut h = Harness::builder().with_size(vec2(300.0, 400.0)).build_ui_state(|ui, app: &mut PhotocraftApp| history(app, ui), app);
            h.run_steps(2);
            if transforming {
                let ctx = h.ctx.clone();
                crate::transform_tool::begin(h.state_mut(), &ctx).unwrap();
            }
            let steps = h.state().session.active().unwrap().history.entries().len();
            // The first state's row, below the 38 pt snapshot row.
            let row = h.ctx.input(|i| i.viewport_rect()).min + vec2(60.0, 60.0);
            click(&mut h, row);
            let after = h.state().session.active().unwrap().history.entries().len();
            if transforming {
                assert_eq!(after, steps, "the document's history is untouched");
                assert!(h.state().ui.transform.is_some());
            } else {
                assert!(after < steps, "the click steps back: {steps} -> {after}");
            }
        }
    }

    /// The Photoshop-theme footer (#1117): trash deletes the current state, the camera takes a
    /// snapshot (#2966), and file-plus makes a new document from the current state.
    #[test]
    fn history_footer_buttons_run_their_commands() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 150})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
        let mut h = Harness::builder().with_size(vec2(300.0, 400.0)).build_ui_state(|ui, app: &mut PhotocraftApp| history(app, ui), app);
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ProMedium);
        h.state_mut().ui.theme = crate::theme::ThemeKind::ProMedium;
        h.run_steps(3);
        // Footer buttons are the only 26 pt squares; the bar lays them out right to left.
        let buttons = |h: &Harness<'static, PhotocraftApp>| {
            let mut b: Vec<(Rect, bool)> = h.ctx.viewport(|v| {
                v.prev_pass.widgets.layers().flat_map(|(_, w)| w.iter()).filter(|w| w.rect.size() == vec2(26.0, 26.0)).map(|w| (w.rect, w.enabled)).collect()
            });
            b.sort_by(|a, b| b.0.center().x.total_cmp(&a.0.center().x));
            b
        };
        let b = buttons(&h);
        assert_eq!(b.len(), 3, "trash, snapshot, new document: {b:?}");
        assert!(b.iter().all(|b| b.1), "all enabled: {b:?}");
        let steps = h.state().session.active().unwrap().history.entries().len();
        click(&mut h, b[1].0.center());
        let st = h.state().session.active().unwrap();
        assert_eq!(st.snapshots.len(), 2, "the camera takes a snapshot, after the first one");
        assert_eq!(st.history.entries().len(), steps, "which is not a history state");
        let docs = h.state().session.documents().len();
        click(&mut h, b[2].0.center());
        assert_eq!(h.state().session.documents().len(), docs + 1, "file-plus makes a new document");
        h.state_mut().session.set_active(0);
        h.run_steps(2);
        let steps = h.state().session.active().unwrap().history.entries().len();
        let b = buttons(&h);
        click(&mut h, b[0].0.center());
        let st = h.state().session.active().unwrap();
        assert_eq!(st.history.entries().len(), steps - 1, "trash deletes the current state");
        assert!(!st.history.can_redo(), "and it can't be redone");
    }

    /// The source column at the left of each state (#2966): a click makes that state the History
    /// Brush's source through `history.setSource`, without stepping to it.
    #[test]
    fn history_source_column_sets_the_history_brush_source() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 150})).unwrap();
        app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
        app.run("edit.fill", json!({"color": "#00ff00"})).unwrap();
        let mut h = Harness::builder().with_size(vec2(300.0, 400.0)).build_ui_state(|ui, app: &mut PhotocraftApp| history(app, ui), app);
        h.run_steps(2);
        // The column cells are the only 24 pt squares, one per state, top to bottom.
        let mut cells: Vec<Rect> = h
            .ctx
            .viewport(|v| v.prev_pass.widgets.layers().flat_map(|(_, w)| w.iter()).filter(|w| w.rect.size() == vec2(24.0, 24.0)).map(|w| w.rect).collect());
        cells.sort_by(|a, b| a.center().y.total_cmp(&b.center().y));
        cells.dedup();
        assert_eq!(cells.len(), 3, "New, Fill, Fill: {cells:?}");
        assert_eq!(h.state().session.active().unwrap().history.brush_source(), Some(0), "the default source");
        click(&mut h, cells[1].center());
        let st = h.state().session.active().unwrap();
        assert_eq!(st.history.brush_source(), Some(1), "the clicked state is the source");
        assert_eq!((st.history.entries().len(), st.history.can_redo()), (3, false), "and the document did not step back");
    }

    /// Snapshot rows have the source column too: a click makes the snapshot the source without
    /// returning to it.
    #[test]
    fn history_source_column_on_a_snapshot_row() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 150})).unwrap();
        app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
        app.run("history.newSnapshot", json!({})).unwrap();
        let mut h = Harness::builder().with_size(vec2(300.0, 400.0)).build_ui_state(|ui, app: &mut PhotocraftApp| history(app, ui), app);
        h.run_steps(2);
        // Snapshot rows' cells are the 24×38 ones: the opening snapshot (0) first, then the new one.
        let mut cells: Vec<Rect> = h
            .ctx
            .viewport(|v| v.prev_pass.widgets.layers().flat_map(|(_, w)| w.iter()).filter(|w| w.rect.size() == vec2(24.0, 38.0)).map(|w| w.rect).collect());
        cells.sort_by(|a, b| a.top().total_cmp(&b.top()));
        assert_eq!(cells.len(), 2, "a source cell on each snapshot row");
        let cell = cells[1];
        click(&mut h, cell.center());
        let st = h.state().session.active().unwrap();
        assert_eq!(photocraft_engine::history_source_cmds::source(st), Some(HistorySource::Snapshot(1)));
        assert_eq!(st.history.entries().len(), 2, "the document did not return to the snapshot");
    }
}

#[cfg(test)]
mod snapshot_row_tests {
    use egui::vec2;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};
    use serde_json::json;

    use super::*;

    fn top_pixel(h: &Harness<'static, PhotocraftApp>) -> [f32; 4] {
        h.state().session.active().unwrap().doc.layers.last().unwrap().surface().unwrap().rgba(1, 1)
    }

    /// The names of the snapshots taken after the first one (the document as opened).
    fn snapshot_names(h: &Harness<'static, PhotocraftApp>) -> Vec<String> {
        h.state().session.active().unwrap().snapshots.iter().skip(1).map(|s| s.name.clone()).collect()
    }

    /// Snapshot rows (#2966) sit under the opening row: clicking one returns to it as a new
    /// state, and its right-click menu renames or deletes it.
    #[test]
    fn snapshot_rows_select_rename_and_delete() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 150})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
        app.run("history.newSnapshot", json!({})).unwrap();
        app.run("edit.fill", json!({"color": "#0000ff"})).unwrap();
        // 60 fps steps, so two clicks a frame apart count as a double-click.
        let mut h =
            Harness::builder().with_size(vec2(300.0, 400.0)).with_step_dt(1.0 / 60.0).build_ui_state(|ui, app: &mut PhotocraftApp| history(app, ui), app);
        h.run_steps(3);
        let steps = h.state().session.active().unwrap().history.entries().len();
        h.get_by_label("Snapshot 1").click();
        h.run_steps(2);
        let st = h.state().session.active().unwrap();
        assert_eq!(st.history.entries().len(), steps + 1);
        assert_eq!(st.history.entries().last().map(String::as_str), Some("Snapshot 1"));
        assert_eq!(top_pixel(&h), [1.0, 0.0, 0.0, 1.0], "back to red");
        // Right-click › Rename…: type a name, Enter. The new "Snapshot 1" state is listed under the
        // snapshot rows, so the first match is the snapshot's row.
        h.get_all_by_label("Snapshot 1").next().unwrap().click_secondary();
        h.run_steps(2);
        h.get_by_label("Rename…").click();
        h.run_steps(2);
        h.get_by_role(egui::accesskit::Role::TextInput).type_text("Lesson start");
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        assert_eq!(snapshot_names(&h), ["Lesson start"]);
        // A double-click renames too; Escape keeps the old name.
        let steps_now = h.state().session.active().unwrap().history.entries().len();
        // A real double-click (accessibility clicks are single clicks).
        let p = h.get_by_label("Lesson start").rect().center();
        let p = egui::pos2(p.x as f32, p.y as f32);
        h.event(egui::Event::PointerMoved(p));
        // Wait out the double-click window of the earlier clicks.
        h.run_steps(30);
        for pressed in [true, false, true, false] {
            h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            h.step();
        }
        h.run_steps(3);
        h.get_by_role(egui::accesskit::Role::TextInput).type_text("Oops");
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert_eq!(snapshot_names(&h), ["Lesson start"], "Escape cancels");
        assert_eq!(h.state().session.active().unwrap().history.entries().len(), steps_now + 1, "the click selected it once");
        // Right-click › Delete (the snapshot's row, above the state the click made).
        h.get_all_by_label("Lesson start").next().unwrap().click_secondary();
        h.run_steps(2);
        h.get_by_label("Delete").click();
        h.run_steps(2);
        assert!(snapshot_names(&h).is_empty());
        assert_eq!(h.state().session.active().unwrap().history.entries().len(), steps + 2, "deleting leaves the history alone");
    }

    /// A History panel in the Photoshop theme (the footer with the trash) on red, Snapshot 1, blue.
    fn pro_history() -> Harness<'static, PhotocraftApp> {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 150})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
        app.run("history.newSnapshot", json!({})).unwrap();
        app.run("edit.fill", json!({"color": "#0000ff"})).unwrap();
        let mut h = Harness::builder().with_size(vec2(300.0, 400.0)).build_ui_state(|ui, app: &mut PhotocraftApp| history(app, ui), app);
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ProMedium);
        h.state_mut().ui.theme = crate::theme::ThemeKind::ProMedium;
        h.run_steps(3);
        h
    }

    /// The footer's trash: the rightmost of its 26 pt buttons (the bar lays them out right to left).
    fn trash(h: &Harness<'static, PhotocraftApp>) -> egui::Pos2 {
        let buttons: Vec<Rect> = h
            .ctx
            .viewport(|v| v.prev_pass.widgets.layers().flat_map(|(_, w)| w.iter()).filter(|w| w.rect.size() == vec2(26.0, 26.0)).map(|w| w.rect).collect());
        buttons.into_iter().max_by(|a, b| a.center().x.total_cmp(&b.center().x)).unwrap().center()
    }

    fn click(h: &mut Harness<'static, PhotocraftApp>, p: egui::Pos2) {
        h.hover_at(p);
        h.run_steps(1);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            h.run_steps(1);
        }
    }

    fn selected(h: &Harness<'static, PhotocraftApp>, label: &str) -> bool {
        h.get_all_by_label(label).next().unwrap().accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
    }

    /// Press on `from`, drag in small steps to `to` and release there.
    fn drag(h: &mut Harness<'static, PhotocraftApp>, from: egui::Pos2, to: egui::Pos2) {
        h.event(egui::Event::PointerMoved(from));
        h.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.step();
        for k in 1..=10 {
            h.event(egui::Event::PointerMoved(from + (to - from) * (k as f32 / 10.0)));
            h.step();
        }
        h.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(3);
    }

    /// Clicking a snapshot selects it, and the trash then deletes the snapshot instead of a state
    /// (#5324). Any other step deselects it, and the trash deletes the current state again.
    #[test]
    fn the_trash_deletes_the_selected_snapshot() {
        let mut h = pro_history();
        h.get_by_label("Snapshot 1").click();
        h.run_steps(2);
        let steps = h.state().session.active().unwrap().history.entries().len();
        assert!(selected(&h, "Snapshot 1"), "the clicked snapshot shows selected");
        let p = trash(&h);
        click(&mut h, p);
        let st = h.state().session.active().unwrap();
        assert_eq!(st.snapshots.len(), 1, "the trash deleted the snapshot, leaving the first one");
        assert_eq!(st.history.entries().len(), steps, "and left the states alone");

        // An edit after the click deselects the snapshot: the trash deletes the state.
        let mut h = pro_history();
        h.get_by_label("Snapshot 1").click();
        h.run_steps(2);
        h.state_mut().run("edit.fill", json!({"color": "#00ff00"})).unwrap();
        h.run_steps(2);
        assert!(!selected(&h, "Snapshot 1"), "an edit deselects the snapshot");
        let steps = h.state().session.active().unwrap().history.entries().len();
        let p = trash(&h);
        click(&mut h, p);
        let st = h.state().session.active().unwrap();
        assert_eq!(st.snapshots.len(), 2, "the snapshot is kept, after the first one");
        assert_eq!(st.history.entries().len(), steps - 1, "the current state is deleted");
    }

    /// A snapshot or a past state dragged onto the trash is deleted (#5324).
    #[test]
    fn rows_dragged_onto_the_trash_are_deleted() {
        let mut h = pro_history();
        let steps = h.state().session.active().unwrap().history.entries().len();
        let row = h.get_by_label("Snapshot 1").rect().center();
        let to = trash(&h);
        drag(&mut h, egui::pos2(row.x, row.y), to);
        let st = h.state().session.active().unwrap();
        assert_eq!(st.snapshots.len(), 1, "the dragged snapshot is deleted, leaving the first one");
        assert_eq!(st.history.entries().len(), steps, "no state is");

        // The second-to-last state: it and the state after it go, as with Delete.
        let mut h = pro_history();
        // The states are Open (or New), New Layer, Fill, Fill: drag New Layer.
        let entries = h.state().session.active().unwrap().history.entries();
        assert_eq!(entries.len(), 4, "{entries:?}");
        let row = h.get_by_label(&entries[1]).rect().center();
        let to = trash(&h);
        drag(&mut h, egui::pos2(row.x, row.y), to);
        let st = h.state().session.active().unwrap();
        assert_eq!(st.history.entries().len(), 1, "the dragged state and the ones after it are deleted");
        assert_eq!(st.snapshots.len(), 2, "the snapshot is kept, after the first one");
    }

    /// With Allow Non-Linear History and only the oldest state current, a click on the trash has
    /// nothing to delete (the oldest state stays, as in Photoshop), so the trash is disabled; a
    /// later (dimmed) state dragged onto it is still deleted, and once a past state exists again
    /// the click works.
    #[test]
    fn non_linear_trash_click_never_targets_the_oldest_state() {
        let mut h = pro_history();
        h.state_mut().run("history.options", json!({"allowNonLinearHistory": true})).unwrap();
        for _ in 0..3 {
            h.state_mut().run("edit.undo", json!({})).unwrap();
        }
        h.run_steps(3);
        let history = |h: &Harness<'static, PhotocraftApp>| {
            let st = h.state().session.active().unwrap();
            (st.history.past_len(), st.history.redo_labels().count())
        };
        assert_eq!(history(&h), (0, 3), "New current, New Layer, Fill, Fill dimmed");
        assert!(h.state().session.is_enabled("history.deleteState"), "the engine still offers a later state to delete");
        let enabled = |h: &Harness<'static, PhotocraftApp>| {
            let p = trash(h);
            h.ctx.viewport(|v| {
                v.prev_pass.widgets.layers().flat_map(|(_, w)| w.iter()).find(|w| w.rect.size() == vec2(26.0, 26.0) && w.rect.center() == p).map(|w| w.enabled)
            })
        };
        assert_eq!(enabled(&h), Some(false), "a click would only target the oldest state");
        let p = trash(&h);
        click(&mut h, p);
        assert_eq!(history(&h), (0, 3), "the click deletes nothing");
        assert!(!h.state().ui.status_error, "and reports no error: {}", h.state().ui.status);

        // The last dimmed Fill dragged onto the trash: only it goes.
        let row = h.get_all_by_label("Fill").last().expect("the dimmed Fill row").rect().center();
        drag(&mut h, row, p);
        assert_eq!(history(&h), (0, 2), "the dragged later state is deleted");

        // A past state again: the click deletes the current one.
        h.state_mut().run("edit.redo", json!({})).unwrap();
        h.run_steps(3);
        assert_eq!(enabled(&h), Some(true));
        click(&mut h, p);
        assert_eq!(history(&h).0, 0, "the click deleted the current state");
    }

    /// Snapshot rows scroll with the states: many snapshots in a short dock leave the footer and
    /// its camera button in view.
    #[test]
    fn many_snapshots_keep_the_footer_in_a_short_panel() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 150})).unwrap();
        for _ in 0..6 {
            app.run("history.newSnapshot", json!({})).unwrap();
        }
        let mut h = Harness::builder().with_size(vec2(300.0, 250.0)).build_ui_state(|ui, app: &mut PhotocraftApp| history(app, ui), app);
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ProMedium);
        h.state_mut().ui.theme = crate::theme::ThemeKind::ProMedium;
        h.run_steps(3);
        let view = h.ctx.input(|i| i.viewport_rect());
        let buttons: Vec<Rect> = h
            .ctx
            .viewport(|v| v.prev_pass.widgets.layers().flat_map(|(_, w)| w.iter()).filter(|w| w.rect.size() == vec2(26.0, 26.0)).map(|w| w.rect).collect());
        assert_eq!(buttons.len(), 3, "{buttons:?}");
        assert!(buttons.iter().all(|b| view.contains_rect(*b)), "footer in view: {buttons:?} in {view:?}");
    }

    /// The top row is the first snapshot, the document as opened: clicking it brings that back
    /// (the Discord report on #2966: "History original file click does not work").
    #[test]
    fn the_top_row_returns_to_the_document_as_opened() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 150})).unwrap();
        let opened = app.session.active().unwrap().doc.layers[0].surface().unwrap().rgba(1, 1);
        app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
        let name = app.session.active().unwrap().doc.name.clone();
        let mut h = Harness::builder().with_size(vec2(300.0, 400.0)).build_ui_state(|ui, app: &mut PhotocraftApp| history(app, ui), app);
        h.run_steps(3);
        h.get_by_label(&name).click();
        h.run_steps(2);
        let st = h.state().session.active().unwrap();
        assert_eq!(st.doc.layers[0].surface().unwrap().rgba(1, 1), opened);
        assert_eq!(st.history.entries().last(), Some(&name));
    }

    /// ⌥-click on the camera opens New Snapshot (#2966): the name, selected so typing replaces
    /// it, and From (Full Document, Merged Layers, Current Layer); OK takes the snapshot.
    #[test]
    fn alt_click_on_the_camera_opens_new_snapshot() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 150})).unwrap();
        app.run("layer.new.layer", json!({})).unwrap();
        app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
        let mut h = Harness::builder().with_size(vec2(900.0, 700.0)).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                let ctx = ui.ctx().clone();
                if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                    return;
                }
                history(app, ui);
                crate::dialogs::show(app, &ctx);
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ProMedium);
        h.state_mut().ui.theme = crate::theme::ThemeKind::ProMedium;
        h.run_steps(4);
        let mut buttons: Vec<Rect> = h
            .ctx
            .viewport(|v| v.prev_pass.widgets.layers().flat_map(|(_, w)| w.iter()).filter(|w| w.rect.size() == vec2(26.0, 26.0)).map(|w| w.rect).collect());
        buttons.sort_by(|a, b| b.center().x.total_cmp(&a.center().x));
        let camera = buttons[1].center();
        let alt = egui::Modifiers::ALT;
        h.event(egui::Event::ModifiersChanged(alt));
        h.event(egui::Event::PointerMoved(camera));
        h.run_steps(1);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos: camera, button: egui::PointerButton::Primary, pressed, modifiers: alt });
            h.run_steps(1);
        }
        h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        h.run_steps(3);
        assert_eq!(h.state().session.active().unwrap().snapshots.len(), 1, "⌥-click opens the dialog instead");
        assert!(h.state().ui.dialogs.last().is_some_and(|d| crate::snapshot_dialog::owns(&d.fields)));
        h.get_by_role(egui::accesskit::Role::TextInput).type_text("Red layer only");
        h.run_steps(1);
        h.get_all_by_role(egui::accesskit::Role::ComboBox).find(|n| n.accesskit_node().value().as_deref() == Some("Full Document")).unwrap().click();
        h.run_steps(3);
        h.get_by_label("Current Layer").click();
        h.run_steps(3);
        h.get_by_label("OK").click();
        h.run_steps(3);
        let st = h.state().session.active().unwrap();
        let snap = st.snapshots.last().unwrap();
        assert_eq!(snap.name, "Red layer only");
        assert_eq!(snap.doc.layers.len(), 1, "only the current layer");
        assert!(h.state().ui.dialogs.is_empty());
    }

    /// History Options › Show New Snapshot Dialog by Default (#2966): a plain click on the camera
    /// opens New Snapshot as ⌥-click does, and ⌥-click still opens it (Photoshop's button always
    /// prompts with the option on). Off, a plain click takes the snapshot at once.
    #[test]
    fn show_new_snapshot_dialog_by_default_makes_a_click_open_the_dialog() {
        for (dialog_by_default, modifiers) in [(true, egui::Modifiers::NONE), (true, egui::Modifiers::ALT), (false, egui::Modifiers::NONE)] {
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
            app.run("file.new", json!({"width": 200, "height": 150})).unwrap();
            app.run("history.options", json!({"showNewSnapshotDialogByDefault": dialog_by_default})).unwrap();
            let mut h = Harness::builder().with_size(vec2(900.0, 700.0)).build_ui_state(
                |ui, app: &mut PhotocraftApp| {
                    let ctx = ui.ctx().clone();
                    if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                        return;
                    }
                    history(app, ui);
                    crate::dialogs::show(app, &ctx);
                },
                app,
            );
            PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ProMedium);
            h.state_mut().ui.theme = crate::theme::ThemeKind::ProMedium;
            h.run_steps(4);
            let mut buttons: Vec<Rect> = h
                .ctx
                .viewport(|v| v.prev_pass.widgets.layers().flat_map(|(_, w)| w.iter()).filter(|w| w.rect.size() == vec2(26.0, 26.0)).map(|w| w.rect).collect());
            buttons.sort_by(|a, b| b.center().x.total_cmp(&a.center().x));
            let camera = buttons[1].center();
            h.event(egui::Event::ModifiersChanged(modifiers));
            h.event(egui::Event::PointerMoved(camera));
            h.run_steps(1);
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton { pos: camera, button: egui::PointerButton::Primary, pressed, modifiers });
                h.run_steps(1);
            }
            h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
            h.run_steps(3);
            let snaps = h.state().session.active().unwrap().snapshots.len();
            let dialog = h.state().ui.dialogs.last().is_some_and(|d| crate::snapshot_dialog::owns(&d.fields));
            if dialog_by_default {
                assert!(dialog, "{modifiers:?}: the dialog opens");
                assert_eq!(snaps, 1, "{modifiers:?}: no snapshot until OK");
                h.get_by_label("OK").click();
                h.run_steps(3);
                assert_eq!(h.state().session.active().unwrap().snapshots.last().unwrap().name, "Snapshot 1");
            } else {
                assert!(!dialog, "off: no dialog");
                assert_eq!(snaps, 2, "off: the click takes the snapshot");
            }
        }
    }

    /// History Options › Automatically Create First Snapshot: on, the panel's top row is a
    /// snapshot named after the document; off, there is none above the opening state.
    #[test]
    fn the_first_snapshot_row_follows_the_option() {
        for first in [true, false] {
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
            app.run("history.options", json!({"automaticallyCreateFirstSnapshot": first})).unwrap();
            app.run("file.new", json!({"width": 200, "height": 150})).unwrap();
            let name = app.session.active().unwrap().doc.name.clone();
            let mut h = Harness::builder().with_size(vec2(300.0, 400.0)).build_ui_state(|ui, app: &mut PhotocraftApp| history(app, ui), app);
            h.run_steps(3);
            assert_eq!(h.query_by_label(&name).is_some(), first, "a snapshot row named after the document: {first}");
        }
    }

    /// Snapshot thumbnails belong to the snapshot's document (#6087): closing a document drops
    /// them, and a document reopened with the same id (a `.pcraft` keeps it) gets fresh ones
    /// rather than the closed document's, even before the views are synced.
    #[test]
    fn snapshot_thumbnails_follow_their_document() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 150})).unwrap();
        let doc = (*app.session.active().unwrap().doc).clone();
        let doc_id = doc.id;
        let mut h = Harness::builder().with_size(vec2(300.0, 400.0)).build_ui_state(|ui, app: &mut PhotocraftApp| history(app, ui), app);
        h.run_steps(2);
        let thumb = |h: &Harness<'static, PhotocraftApp>| h.state().snapshot_thumbs.get(&(doc_id, 0)).map(|(_, t)| t.id());
        let first = thumb(&h).expect("the first snapshot's thumbnail");
        // Closed: its thumbnails go with it.
        h.state_mut().session.close(0);
        h.state_mut().sync_views();
        assert!(h.state().snapshot_thumbs.keys().all(|(d, _)| *d != doc_id), "closing drops them");
        h.state_mut().session.add_document(doc.clone(), None);
        h.run_steps(2);
        let reopened = thumb(&h).expect("a thumbnail for the reopened document");
        assert_ne!(reopened, first);
        // Closed and reopened between two frames, with no sync in between: still a fresh one.
        h.state_mut().session.close(0);
        h.state_mut().session.add_document(doc, None);
        assert_eq!(h.state().session.active().unwrap().doc.id, doc_id, "same id");
        h.run_steps(2);
        assert_ne!(thumb(&h).unwrap(), reopened);
    }

    /// Renaming belongs to one document: snapshot ids restart in each, so switching tabs must not
    /// carry the edit over to the other document's snapshot with the same id.
    #[test]
    fn a_rename_in_progress_stays_with_its_document() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        for _ in 0..2 {
            app.run("file.new", json!({"width": 200, "height": 150})).unwrap();
            app.run("history.newSnapshot", json!({})).unwrap();
        }
        let mut h = Harness::builder().with_size(vec2(300.0, 400.0)).build_ui_state(|ui, app: &mut PhotocraftApp| history(app, ui), app);
        h.run_steps(3);
        h.get_by_label("Snapshot 1").click_secondary();
        h.run_steps(2);
        h.get_by_label("Rename…").click();
        h.run_steps(2);
        assert!(h.query_by_role(egui::accesskit::Role::TextInput).is_some(), "renaming in document 2");
        h.state_mut().session.set_active(0);
        h.run_steps(2);
        assert!(h.query_by_role(egui::accesskit::Role::TextInput).is_none(), "not in document 1");
    }
}

#[cfg(test)]
mod layer_pct_slider_tests;

#[cfg(test)]
>>>>>>> upstream/main
mod fx_drag_tests;

#[cfg(test)]
mod unsupported_marker_tests;

#[cfg(test)]
mod swatch_type_tests {
    use super::*;
    use egui_kittest::Harness;

    /// The Color panel only acts on an edit. A colour's round trip through the panel's models isn't
    /// exact for every colour (#D8452E isn't), and writing it back every frame would recolour
    /// selected type the moment it was selected.
    #[test]
    fn idle_color_panel_leaves_the_foreground_and_selected_type_alone() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 400, "height": 200})).unwrap();
        let id = app.run("type.create", json!({"text": "Hello world", "size": 40, "x": 20, "y": 100, "color": "#ffffff"})).unwrap()["layer"].as_u64().unwrap();
        app.run("tools.setColors", json!({"foreground": "#d8452e"})).unwrap();
        let fg = app.session.tools.foreground;
        app.ui.tool = Tool::Type;
        app.ui.text_edit = Some(crate::state::TextEdit { layer: id, document: None, caret: 0, anchor: 5, session: "s".into(), created: false, mask: false });
        let mut h = Harness::builder()
            .with_context(app.ctx().clone())
            .with_size(vec2(300.0, 300.0))
            .build_ui_state(|ui, app: &mut PhotocraftApp| crate::panels::color::panel(app, ui), app);
        h.run_steps(4);
        assert_eq!(h.state().session.tools.foreground, fg);
        let st = h.state().session.active().unwrap();
        let Some(photocraft_doc::LayerContent::Text(t)) = st.doc.layer(photocraft_doc::LayerId(id)).map(|l| &l.content) else { panic!("type layer") };
        assert_eq!(t.char_runs().len(), 1, "still one white run");
    }
}

#[cfg(test)]
mod type_flyout_tests;

#[cfg(test)]
mod clipping_indicator_tests;

#[cfg(test)]
mod mask_properties_tests;

#[cfg(test)]
mod history_panel_select_tests;
<<<<<<< HEAD
=======
#[cfg(test)]
mod layer_click_type_commit_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    fn harness(kind: theme::ThemeKind) -> (Harness<'static, PhotocraftApp>, LayerId, LayerId) {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": 200, "height": 200})).unwrap();
        let text = LayerId(app.run("type.create", json!({"text": "Before", "size": 24, "x": 20, "y": 80})).unwrap()["layer"].as_u64().unwrap());
        app.run("layer.setProps", json!({"layer": text.0, "name": "Edited type"})).unwrap();
        let other = LayerId(app.run("layer.new.layer", json!({"name": "Other pixels"})).unwrap()["layer"].as_u64().unwrap());
        app.run("layer.select", json!({"layer": text.0})).unwrap();
        crate::type_tool::edit_active(&mut app).unwrap();
        let session = app.ui.text_edit.as_ref().unwrap().session.clone();
        app.run("type.edit", json!({"layer": text.0, "text": "Preserved edit", "coalesce": session})).unwrap();
        let mut ready = false;
        let mut h = Harness::builder().with_size(vec2(360.0, 650.0)).with_step_dt(1.0 / 60.0).build_ui_state(
            move |ui, app: &mut PhotocraftApp| {
                if !ready {
                    PhotocraftApp::setup_context(ui.ctx(), kind);
                    ready = true;
                    return;
                }
                layers(app, ui);
            },
            app,
        );
        h.run_steps(4);
        (h, text, other)
    }

    fn click(h: &mut Harness<'_, PhotocraftApp>, at: Pos2, modifiers: egui::Modifiers) {
        h.event(egui::Event::ModifiersChanged(modifiers));
        h.hover_at(at);
        h.run_steps(1);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers });
            h.run_steps(1);
        }
    }

    fn preserved(app: &PhotocraftApp, id: LayerId) {
        let layer = app.session.active().unwrap().doc.layer(id).unwrap();
        let LayerContent::Text(text) = &layer.content else { panic!("type must remain editable") };
        assert_eq!(text.text, "Preserved edit");
    }

    #[test]
    fn layer_name_and_thumbnail_clicks_commit_same_or_other_type_selection() {
        for kind in theme::ThemeKind::ALL {
            for same in [true, false] {
                for thumbnail in [true, false] {
                    let (mut h, text, other) = harness(kind);
                    let label = if same { "Edited type" } else { "Other pixels" };
                    let row = h.get_by_label(label).rect();
                    let at = pos2(row.left() + if thumbnail { 43.0 } else { 120.0 }, row.center().y);
                    click(&mut h, at, egui::Modifiers::NONE);
                    assert!(h.state().ui.text_edit.is_none(), "{kind:?}, same={same}, thumbnail={thumbnail}");
                    assert_eq!(h.state().session.active().unwrap().active_layer, Some(if same { text } else { other }));
                    preserved(h.state(), text);
                }
            }
        }
    }

    #[test]
    fn layer_click_commits_without_breaking_modifier_selection_or_double_click_editing() {
        for kind in theme::ThemeKind::ALL {
            let (mut h, text, other) = harness(kind);
            let row = h.get_by_label("Other pixels").rect();
            click(&mut h, pos2(row.left() + 120.0, row.center().y), egui::Modifiers { command: true, ..Default::default() });
            assert!(h.state().ui.text_edit.is_none());
            let selected = h.state().session.active().unwrap().selected_layers();
            assert!(selected.contains(&text) && selected.contains(&other));
            preserved(h.state(), text);

            let (mut h, text, _) = harness(kind);
            let row = h.get_by_label("Edited type").rect();
            let thumb = pos2(row.left() + 43.0, row.center().y);
            click(&mut h, thumb, egui::Modifiers::NONE);
            assert!(h.state().ui.text_edit.is_none());
            click(&mut h, thumb, egui::Modifiers::NONE);
            assert_eq!(h.state().ui.text_edit.as_ref().map(|ed| ed.layer), Some(text.0));
            preserved(h.state(), text);
        }
    }

    #[test]
    #[ignore = "offscreen visual evidence for #5584"]
    fn layer_click_synthetic_visual_evidence() {
        let _gpu = photocraft_testkit::gpu_lock();
        let (h, text, _) = harness(theme::ThemeKind::ProMedium);
        let mut app = h.into_state();
        app.ui.tool = Tool::Type;
        let mut h = Harness::builder().with_size(vec2(1280.0, 900.0)).with_step_dt(1.0 / 60.0).with_max_steps(64).wgpu().build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, theme::ThemeKind::ProMedium);
            app
        });
        h.run_steps(4);
        assert!(h.state().ui.text_edit.is_some());
        h.render().unwrap().save("5584-before.png").unwrap();
        let row = h.get_by_label("Other pixels").rect();
        click(&mut h, pos2(row.left() + 120.0, row.center().y), egui::Modifiers::NONE);
        assert!(h.state().ui.text_edit.is_none());
        preserved(h.state(), text);
        h.run_steps(4);
        h.render().unwrap().save("5584-after.png").unwrap();
    }
}

#[cfg(test)]
mod sampler_readout_tests {
    use super::*;

    /// Composited once per revision: repeated frames share the readouts; adding a sampler or an
    /// edit recomputes them (#4528).
    #[test]
    fn sampler_readouts_are_reused_until_the_document_changes() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 32, "height": 32, "background": "white"})).unwrap();
        app.run("view.colorSamplers.add", json!({"x": 4, "y": 4})).unwrap();
        let a = sampler_readouts(&mut app);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0]["rgb"], json!([255, 255, 255]));
        assert!(std::sync::Arc::ptr_eq(&a, &sampler_readouts(&mut app)), "the next frame reuses them");
        app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
        let b = sampler_readouts(&mut app);
        assert!(!std::sync::Arc::ptr_eq(&a, &b));
        assert_eq!(b[0]["rgb"], json!([255, 0, 0]), "an edit shows");
        app.run("view.colorSamplers.add", json!({"x": 8, "y": 8})).unwrap();
        assert_eq!(sampler_readouts(&mut app).len(), 2, "so does a new sampler");
    }
}

#[cfg(test)]
mod brush_chip_tests {
    use super::brush_controls::brush_preset_chip;

    /// The options-bar chip's tip texture for `brush`, row-major alpha (32 × 32).
    fn chip_tip(brush: &photocraft_engine::BrushSettings) -> Vec<u8> {
        let ctx = egui::Context::default();
        let mut state = crate::state::UiState::default();
        let mut out = ctx.run_ui(Default::default(), |ui| brush_preset_chip(ui, brush, &mut state));
        // `Painter::image` is a textured mesh.
        let id = out.shapes.iter().find_map(|c| match &c.shape {
            egui::Shape::Mesh(m) if (m.calc_bounds().width() - 14.0).abs() < 0.01 => Some(m.texture_id),
            _ => None,
        });
        let image = id.and_then(|id| out.textures_delta.set.get(&id)).and_then(|d| d.last()).map(|d| match &d.image {
            egui::ImageData::Color(c) => c.clone(),
        });
        out.textures_delta.clear();
        let image = image.expect("the chip draws its tip as an uploaded texture");
        assert_eq!(image.size, [32, 32]);
        image.pixels.iter().map(|p| p.a()).collect()
    }

    /// The chip shows the brush's own tip: a soft brush fades from its centre as its dabs do, a
    /// hard one is solid to its rim, and a flattened tip is flat, not a round blob.
    #[test]
    fn the_brush_chip_draws_the_brushs_own_tip() {
        let at = |a: &[u8], x: usize, y: usize| a[y * 32 + x];
        let hard = chip_tip(&photocraft_engine::BrushSettings { hardness: 1.0, ..Default::default() });
        let soft = chip_tip(&photocraft_engine::BrushSettings { hardness: 0.0, ..Default::default() });
        assert!(at(&hard, 16, 16) == 255 && at(&hard, 16 + 11, 16) == 255, "hard: solid to near the rim");
        assert!(at(&soft, 16, 16) > 230, "soft: full at the centre");
        assert!(at(&soft, 16 + 11, 16) < 80, "soft: faint near the rim: {}", at(&soft, 16 + 11, 16));
        assert!(at(&soft, 16 + 6, 16) > at(&soft, 16 + 11, 16), "and fading outwards");
        let flat = chip_tip(&photocraft_engine::BrushSettings { hardness: 1.0, roundness: 0.3, ..Default::default() });
        assert!(at(&flat, 16 + 11, 16) > 200 && at(&flat, 16, 16 + 11) == 0, "a flattened tip is flat");
    }
}

#[cfg(test)]
mod panel_state_migration_tests {
    use super::{migrate_panel_state, panel_state_id};
    use egui::containers::panel::PanelState;
    use egui::{Context, Id, Rect, pos2, vec2};

    fn state(width: f32) -> PanelState {
        PanelState { outer_rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 600.0)) }
    }

    /// 0.36 saved `dock` under `Id::new("dock")`; 0.37 reads a parent-scoped key.
    #[test]
    fn the_parent_scoped_key_matches_the_panel_egui_shows() {
        let ctx = Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            egui::Panel::right("dock").exact_size(410.0).show(ui, |ui| ui.set_min_width(ui.available_width()));
        });
        output.textures_delta.clear();
        let key = panel_state_id(&ctx, "dock");
        assert_eq!(key, ctx.viewport_id().root_ui_id().with("dock"));
        let panel = PanelState::load(&ctx, key).expect("the panel persisted under its parent-scoped id");
        assert_eq!(panel.size().x, 410.0);
        assert_ne!(key, Id::unique("dock"), "the old persisted key is a different id");
    }

    #[test]
    fn old_panel_widths_move_to_the_new_key_once() {
        let ctx = Context::default();
        let old = Id::unique("dock");
        ctx.data_mut(|d| d.insert_persisted(old, state(410.0)));
        migrate_panel_state(&ctx);
        let new = panel_state_id(&ctx, "dock");
        assert_eq!(PanelState::load(&ctx, new).map(|p| p.size().x), Some(410.0));
        assert_eq!(PanelState::load(&ctx, old).map(|p| p.size().x), Some(410.0), "0.36 rollback still reads it");

        // An existing 0.37 value wins, in a partial migration or on a later run.
        ctx.data_mut(|d| d.insert_persisted(new, state(290.0)));
        ctx.data_mut(|d| d.insert_temp(Id::unique("photocraft-panel-state-migrated"), false));
        migrate_panel_state(&ctx);
        assert_eq!(PanelState::load(&ctx, new).map(|p| p.size().x), Some(290.0));
        assert_eq!(PanelState::load(&ctx, old).map(|p| p.size().x), Some(410.0));
    }

    /// 0.37 resize → 0.36 rollback resize → upgrade must keep the newer 0.36 width, not the
    /// stale 0.37 one (the old key stays the only one a 0.36 build writes).
    #[test]
    fn a_0_36_rollback_resize_after_0_37_is_picked_up() {
        use super::remember_panel_state;
        let ctx = Context::default();
        let old = Id::unique("dock");
        let new = panel_state_id(&ctx, "dock");
        ctx.data_mut(|d| d.insert_persisted(old, state(410.0)));
        migrate_panel_state(&ctx);
        // This 0.37 session resized the dock; the pass-end stamp records it.
        ctx.data_mut(|d| d.insert_persisted(new, state(290.0)));
        remember_panel_state(&ctx);
        // A 0.36 build then resized the same dock (it writes only the old key).
        ctx.data_mut(|d| d.insert_persisted(old, state(330.0)));
        ctx.data_mut(|d| d.insert_temp(Id::unique("photocraft-panel-state-migrated"), false));
        migrate_panel_state(&ctx);
        assert_eq!(PanelState::load(&ctx, new).map(|p| p.size().x), Some(330.0), "the newer 0.36 width wins");
        assert_eq!(PanelState::load(&ctx, old).map(|p| p.size().x), Some(330.0), "and stays for a rollback");
    }

    /// A 0.37 resize after the migration is not overwritten by the unchanged old key.
    #[test]
    fn a_0_37_resize_after_the_migration_is_kept() {
        use super::remember_panel_state;
        let ctx = Context::default();
        let old = Id::unique("dock");
        let new = panel_state_id(&ctx, "dock");
        ctx.data_mut(|d| d.insert_persisted(old, state(410.0)));
        migrate_panel_state(&ctx);
        ctx.data_mut(|d| d.insert_persisted(new, state(290.0)));
        remember_panel_state(&ctx);
        ctx.data_mut(|d| d.insert_temp(Id::unique("photocraft-panel-state-migrated"), false));
        migrate_panel_state(&ctx);
        assert_eq!(PanelState::load(&ctx, new).map(|p| p.size().x), Some(290.0));
    }
}
>>>>>>> upstream/main
