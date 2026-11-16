//! Front ends of the File-menu commands added with slices: the Save for Web (Legacy) dialog
//! (the framework's `HostedSaveForWeb`: Original / Optimized / 2-Up / 4-Up previews, per-format
//! settings, image size, per-slice output), the Print dialog (`HostedPrint`), and the form
//! dialogs for Contact Sheet II, Create Droplet, Statistics, Script Events Manager, Package and
//! Paths to Illustrator. Every OK runs the engine command with the dialog's values, so the
//! control channel can drive them with `ui.dialog.set` / `confirm`.

use craft::ui::dialog::DialogId;
use serde_json::{Value, json};

use crate::PhotocraftApp;

pub(crate) mod print;
mod web;
#[path = "file_ui/web_preview.rs"]
mod web_preview;

pub use print::open_print;
pub(crate) use print::print_status;
#[cfg(test)]
use print::{drag_print_position, print_params, printer_list};
pub use web::open_web;
#[cfg(test)]
use web::{WEB_COMMAND, download_for_web, options_of, pane_key, web_frame_document, web_params};

fn no_params(p: &Value) -> bool {
    p.as_object().is_none_or(|o| o.is_empty())
}

/// The reply of a menu item that opened the dialog `id`.
fn opened(id: DialogId) -> Value {
    json!({"dialog": id.value()})
}

/// File › Scripts › Browse… lists Photoshop's script types (matching its filter, #2452) plus the
/// JSON/text/droplet files the engine parses; selecting a Photoshop script gets the engine's
/// "not Photoshop ExtendScript" error instead of hiding those files from the picker.
const SCRIPT_EXTENSIONS: &[&str] = &["psjs", "ccjs", "jsx", "js", "jsxbin", "json", "txt", "pcdroplet"];

/// Menu items fronted here (dialogs before the engine command). `None` when not ours.
pub fn invoke(app: &mut PhotocraftApp, ctx: &egui::Context, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if !no_params(params) {
        return None;
    }
    Some(match id {
        "file.export.saveForWebLegacy" => {
            app.session.active()?;
            Ok(opened(open_web(app)))
        }
        "file.print" => {
            app.session.active()?;
            Ok(opened(open_print(app)))
        }
        "file.printOneCopy" if photocraft_engine::print_cmds::last_print(&app.session).is_none() => {
            app.session.active()?;
            Ok(opened(open_print(app)))
        }
        "file.export.exportPreferences" => return crate::prefs_ui::invoke(app, ctx, "edit.preferences.export", &json!({})),
<<<<<<< HEAD
        "file.automate.contactSheetII" => Ok(opened(crate::dialogs::file_forms::open_contact_sheet(app))),
        "file.automate.createDroplet" => crate::dialogs::file_forms::open_create_droplet(app).map(opened),
        "file.scripts.statistics" => Ok(opened(crate::stack_dialog::open_statistics(app))),
=======
        "file.automate.contactSheetII" => form(
            app,
            id,
            tl!("Contact Sheet II"),
            json!({"input": dir, "units": "inches", "width": 8.0, "height": 10.0, "resolution": 300.0, "mode": "rgb", "depth": 8, "columns": 5, "rows": 6, "placeAcrossFirst": true, "autoSpacing": true, "rotateForBestFit": false, "caption": true, "font": photocraft_text::fonts::DEFAULT_FAMILY, "fontSize": 12.0, "flatten": false,
                // Source Images: the folder, then Include Subfolders (on) and, right below it,
                // Group Images by Folder (off, greyed while Include Subfolders is off).
                "includeSubfolders": true, "groupByFolder": false, "__order": ["input", "includeSubfolders", "groupByFolder"],
                "__labels": {"includeSubfolders": tl!("Include Subfolders"), "groupByFolder": tl!("Group Images by Folder")},
                "__enabledBy": {"groupByFolder": "includeSubfolders"},
                "__tooltips": {"groupByFolder": tl!("Images in subfolders will start on a new Contact Sheet")}}),
            json!({"units": ["inches", "cm", "pixels"], "mode": ["rgb", "gray", "cmyk", "lab"]}),
        ),
        "file.automate.createDroplet" => {
            let dir = droplet_default_dir(app);
            let Some(action) = crate::actions::selected_action(app) else {
                return Some(Err("record an action in the Actions panel first".into()));
            };
            let (name, set) = (action.name.clone(), action.set.clone());
            let (names, sets) = crate::actions::action_form_choices(app);
            let mut fields =
                json!({"path": format!("{dir}/{name}.pcdroplet"), "name": name, "action": name, "output": format!("{dir}/droplet-output"), "format": "same"});
            // The Set dropdown picks between same-named actions; an action outside any set has none.
            if let Some(set) = set {
                fields["set"] = json!(set);
            }
            form(app, id, tl!("Create Droplet"), fields, json!({"set": sets, "action": names, "format": ["same", "png", "jpg", "psd", "tiff"]}))
        }
        "file.scripts.statistics" => json!({"dialog": crate::stack_dialog::open_statistics(app)}),
>>>>>>> upstream/main
        "file.scripts.browse" => {
            return Some(app.pick_file_bytes_filtered(SCRIPT_EXTENSIONS, |app, name, bytes| {
                let r = app.run("file.scripts.browse", json!({"name": name, "script": String::from_utf8_lossy(&bytes)}));
                if r.is_ok() {
                    let template = "Ran script {name}";
                    app.ui.status = crate::i18n::fmt(tl!(template), &[("name", &name)]);
                }
                r
            }));
        }
<<<<<<< HEAD
        "file.scripts.scriptEventsManager" => Ok(opened(crate::dialogs::file_forms::open_script_events(app))),
        "file.package" => Ok(opened(crate::dialogs::file_forms::open_package(app))),
        "file.export.pathsToIllustrator" => Ok(opened(crate::dialogs::file_forms::open_paths_to_file(app))),
=======
        "file.scripts.scriptEventsManager" => {
            let enabled = app.session.prefs().script_events.enabled;
            let events: Vec<&str> = photocraft_engine::automate_cmds::EVENTS.iter().map(|e| e.0).collect();
            form(app, id, "Script Events Manager", json!({"enabled": enabled, "event": "openDocument", "script": "", "name": ""}), json!({"event": events}))
        }
        "file.package" => form(app, id, "Package", json!({"dir": dir}), json!({})),
        "file.export.pathsToIllustrator" => {
            // Document Bounds comes first; a document without paths offers only it, selected (#13771).
            let mut names = vec!["documentBounds".to_string()];
            if let Some(d) = app.session.active().filter(|d| d.doc.work_path.is_some() || !d.doc.paths.is_empty()) {
                names.push("all".into());
                if d.doc.work_path.is_some() {
                    names.push("work".into());
                }
                names.extend(d.doc.paths.iter().map(|p| p.name.clone()));
            }
            let fallback = if names.len() > 1 { "all" } else { "documentBounds" };
            // Opens on the Paths panel's selected row, as the reference application does (#13777).
            // A shape layer's path ("layer") is not an entry yet, and a row the document no longer
            // has falls back to All Paths (Document Bounds without paths).
            let paths = app.ui.selected_path.as_deref().filter(|s| *s != "layer" && names.iter().any(|n| n == s)).unwrap_or(fallback).to_string();
            form(app, id, "Export Paths to File", json!({"path": format!("{dir}/{stem}.ai"), "paths": paths}), json!({"paths": names}))
        }
>>>>>>> upstream/main
        _ => return None,
    })
}

/// Checked state of toggle items owned here.
pub fn checked(app: &PhotocraftApp, id: &str) -> Option<bool> {
    match id {
        "view.lockSlices" => Some(app.session.file_menu.slices_locked),
        "file.generate.imageAssets" => Some(app.session.active().is_some_and(|d| app.session.file_menu.image_assets.contains(&d.doc.id))),
        _ => None,
    }
}

/// Answers the requests and results of the Save for Web and Print dialogs; one row of the app's takers.
pub(crate) fn take(app: &mut PhotocraftApp, ctx: &egui::Context) {
    web::take(app, ctx);
    print::take(app, ctx);
}

#[cfg(test)]
mod tests;

#[cfg(test)]
<<<<<<< HEAD
mod custom_paper_tests;
=======
mod contact_sheet_subfolders_tests;
>>>>>>> upstream/main

#[cfg(test)]
mod web_playback_tests {
    use std::sync::Arc;

    use craft::ui::patterns::form_dialogs::{WebFormat, WebView};
    use photocraft_engine::web_cmds::{self, Optimized, WebSettings};

    use super::tests::{app, pass, set_option};
    use super::*;

    fn animated_app(depth: u32) -> PhotocraftApp {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 32, "height": 24, "depth": depth})).unwrap();
        app.run("timeline.create", json!({"duration": 3, "fps": 2})).unwrap();
        app.run("layer.videoLayers.newBlankVideoLayer", json!({})).unwrap();
        for (frame, color) in ["#ff0000", "#00ff00", "#0000ff"].iter().enumerate() {
            app.run("timeline.setFrame", json!({"frame":frame})).unwrap();
            app.run("edit.fill", json!({"color":color})).unwrap();
        }
        app.run("timeline.setFrame", json!({"frame":0})).unwrap();
        app
    }

    /// The indexed encode the preview of pane `index` keeps, for the pictures it was asked for.
    fn encoded(app: &PhotocraftApp, index: usize) -> Arc<Optimized> {
        let doc_id = app.session.active().unwrap().doc.id.0;
        app.ctx().data(|d| d.get_temp::<(String, Arc<Optimized>)>(pane_key(doc_id, index).with("indexed-source"))).unwrap().1
    }

    /// Sets the frame the dialog shows, as its play row does.
    // PLAN: assumed the hosted dialog takes the frame as the JSON field `frame` (`HostedSaveForWeb::frame` is its getter).
    fn set_frame(app: &mut PhotocraftApp, id: DialogId, frame: usize) {
        set_option(app, id, "frame", json!(frame));
        pass(app);
        let ctx = app.ctx().clone();
        take(app, &ctx);
    }

    /// WebP (#6391): 4-Up halves a lossy quality like JPEG's and drops quality when lossless. The dialog's own
    /// variants are read from what it asks the app to encode.
    #[test]
    fn webp_settings_show_quality_until_lossless() {
        let mut app = app();
        let id = open_web(&mut app);
        set_option(&mut app, id, "format", json!(WebFormat::WebP));
        set_option(&mut app, id, "quality", json!(80));
        set_option(&mut app, id, "view", json!(WebView::FourUp));
        pass(&mut app);
        let four_up = |app: &mut PhotocraftApp| {
            let dialog = app.framework.dialogs.get_mut_by_id::<craft::ui::patterns::form_dialogs::HostedSaveForWeb>(id).unwrap();
            let wanted = dialog.wanted_variants();
            let options = |index: usize| wanted.iter().find(|v| v.index == index).expect("the pane is asked for").request.options.clone();
            [options(1), options(2)]
        };
        let [half, quarter] = four_up(&mut app);
        assert_eq!((half.format, half.quality, quarter.quality), (WebFormat::WebP, 40, 20));
        set_option(&mut app, id, "lossless", json!(true));
        pass(&mut app);
        let [half, quarter] = four_up(&mut app);
        assert_eq!((half.format, quarter.format), (WebFormat::Png8, WebFormat::Gif));
    }

    #[test]
    fn frame_previews_preserve_source_and_match_colors_across_depths() {
        for depth in [8, 16, 32] {
            let app = animated_app(depth);
            let source = app.session.active().unwrap().doc.clone();
            for (frame, expected) in [[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]].iter().enumerate() {
                let preview = web_frame_document(&source, Some(frame)).unwrap();
                let (proxy, _, _) = web_cmds::web_document(&preview, &json!({"width":16,"height":12}), &WebSettings::default()).unwrap();
                let pixels = photocraft_compose::flatten(&proxy);
                for (actual, expected) in pixels.px[0].iter().zip(expected) {
                    assert!((actual - expected).abs() < 0.002, "depth={depth} frame={frame}");
                }
            }
            assert_eq!(source.timeline.as_ref().unwrap().current, 0);
            assert_eq!(photocraft_compose::flatten(&source).px[0], [1.0, 0.0, 0.0, 1.0]);
        }
    }

    /// Every pane of 4-Up is encoded from the frame the dialog reports.
    #[test]
    fn four_up_caches_follow_the_local_frame() {
        let mut app = animated_app(8);
        let id = open_web(&mut app);
        set_option(&mut app, id, "format", json!(WebFormat::Gif));
        set_option(&mut app, id, "view", json!(WebView::FourUp));
        let colors = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]];
        for frame in [1, 2, 0] {
            set_frame(&mut app, id, frame);
            for index in 0..3 {
                assert_eq!(&encoded(&app, index).preview[..4], &colors[frame], "pane {index} must show frame {frame}");
            }
        }
    }

    /// A frame change re-encodes from fresh indices and keeps the palette edits; a palette edit alone reuses the indices.
    #[test]
    fn playback_refreshes_indexed_frames_without_resetting_palette_edits() {
        let mut app = animated_app(8);
        let id = open_web(&mut app);
        set_option(&mut app, id, "format", json!(WebFormat::Gif));
        set_option(&mut app, id, "view", json!(WebView::Optimized));
        pass(&mut app);
        let ctx = app.ctx().clone();
        take(&mut app, &ctx);
        let first = encoded(&app, 0);
        set_option(&mut app, id, "palette_edits", json!({"0": [255, 255, 0]}));
        pass(&mut app);
        take(&mut app, &ctx);
        assert!(Arc::ptr_eq(&first, &encoded(&app, 0)), "palette edits reuse the current frame's indices");
        let edits = app.framework.dialogs.get_by_id::<craft::ui::patterns::form_dialogs::HostedSaveForWeb>(id).unwrap().current().palette_edits.clone();
        assert_eq!(edits.len(), 1);
        let mut previous = first;
        for (frame, rgba) in [(1, [0, 255, 0, 255]), (2, [0, 0, 255, 255]), (0, [255, 0, 0, 255])] {
            set_frame(&mut app, id, frame);
            let next = encoded(&app, 0);
            assert!(!Arc::ptr_eq(&previous, &next), "a changed frame needs fresh indexed pixels");
            assert_eq!(&next.preview[..4], &rgba);
            let kept = app.framework.dialogs.get_by_id::<craft::ui::patterns::form_dialogs::HostedSaveForWeb>(id).unwrap().current().palette_edits.clone();
            assert_eq!(kept, edits);
            assert_eq!(app.session.active().unwrap().doc.timeline.as_ref().unwrap().current, 0);
            previous = next;
        }
    }

    #[test]
    fn preview_keeps_current_frame_edits_without_storing_into_source() {
        let mut app = animated_app(8);
        app.run("edit.fill", json!({"color":"#ffff00"})).unwrap();
        let source = app.session.active().unwrap().doc.clone();
        let edited = web_frame_document(&source, Some(0)).unwrap();
        let other = web_frame_document(&source, Some(1)).unwrap();
        assert_eq!(photocraft_compose::flatten(&edited).px[0], [1.0, 1.0, 0.0, 1.0]);
        assert_eq!(photocraft_compose::flatten(&other).px[0], [0.0, 1.0, 0.0, 1.0]);
        assert!(Arc::ptr_eq(&source, &app.session.active().unwrap().doc));
        assert_eq!(source.timeline.as_ref().unwrap().current, 0);
    }

    #[test]
    fn optimized_gif_preview_matches_each_frame_at_every_depth() {
        let settings = WebSettings::from_params(&json!({"format":"gif","colors":16}), "preview").unwrap();
        for depth in [8, 16, 32] {
            let app = animated_app(depth);
            for (frame, expected) in [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]].iter().enumerate() {
                let doc = web_frame_document(&app.session.active().unwrap().doc, Some(frame)).unwrap();
                let (proxy, _, _) = web_cmds::web_document(&doc, &json!({"width":16,"height":12}), &settings).unwrap();
                let pixels = photocraft_compose::flatten(&proxy);
                let optimized = web_cmds::optimize(&pixels.px, 16, photocraft_geom::Rect::new(0, 0, 16, 12), &settings, None, None, None, 72.0, true).unwrap();
                assert_eq!(&optimized.preview[..4], expected, "depth={depth} frame={frame}");
            }
        }
    }

    #[test]
    fn a_still_document_opens_without_an_animation() {
        let mut app = app();
        let id = open_web(&mut app);
        assert!(app.framework.dialogs.get_by_id::<craft::ui::patterns::form_dialogs::HostedSaveForWeb>(id).is_some());
    }
}
