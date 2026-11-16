use super::dispatch::no_params;
use super::{PhotocraftApp, Value, json};
use craft::ui::contract::Bind;
use craft::ui::dialog::DialogId;
use craft::ui::patterns::form_dialogs::{DialogFields, Param, ParamDialog, ParamKind};
use craft::ui::runtime::env::Label;
use craft::ui::runtime::requests::PathKind;

// ---------- dialogs and pickers in front of engine commands ----------

/// The command ids whose dialog is a hosted parameter form built by [`params_for`] (Open EXR Parts included).
/// The dialogs area's `PARAM_FAMILIES` row owns exactly these.
const FORM_COMMANDS: [&str; 21] = [
    "file.openExrParts",
    "layer.videoLayers.interpretFootage",
    "file.fileInfo",
    "file.automate.fitImage",
    "file.automate.conditionalModeChange",
    "file.export.layersToFiles",
    "file.export.layerCompsToFiles",
    "file.export.artboardsToFiles",
    "file.export.artboardsToPdf",
    "file.export.dataSetsAsFiles",
    "layer.new.artboard",
    "file.export.colorLookupTables",
    "file.automate.photomerge",
    "file.automate.mergeToHdrPro",
    "file.automate.lensCorrection",
    "file.scripts.imageProcessor",
    "file.automate.batch",
    "select.saveSelection",
    "select.loadSelection",
    "image.applyImage",
    "image.calculations",
];

/// Whether `key` is the command id of a form this file opens.
pub(crate) fn is_form(key: &str) -> bool {
    FORM_COMMANDS.contains(&key)
}

/// The reply of a command that opened a dialog.
fn opened(id: DialogId) -> Value {
    json!({"dialog": id.value().to_string()})
}

/// Opens the hosted parameter form of command `id`. `fixed` holds values the command needs that no row edits.
fn open_form(app: &mut PhotocraftApp, id: &str, title: &str, params: Vec<Param>, fields: DialogFields, fixed: Value) -> DialogId {
    let title = Label::from(crate::i18n::t(title).to_owned());
    let key = id.to_owned();
    crate::dialogs::params::open_param(app, id, fields, fixed, move |fields: &mut DialogFields| {
        ParamDialog::command(key.clone(), title.clone(), params.clone()).values(Bind::bound(fields))
    })
}

/// Choice options from `(id, label)` pairs whose labels are `tl!` literals at the call site.
fn opts(list: &[(&'static str, &'static str)]) -> Vec<(String, Label)> {
    list.iter().map(|(id, label)| ((*id).to_owned(), Label::from(*label))).collect()
}

/// Options named by the user's own text (action and set names): shown as they are.
fn named(list: &[String]) -> Vec<(String, Label)> {
    list.iter().map(|n| (n.clone(), Label::from(n.clone()))).collect()
}

/// Channel and layer options: the engine's fixed ids read as English words, every other name is data.
fn channels(list: &[String]) -> Vec<(String, Label)> {
    let special = |id: &str| -> Option<Label> {
        Some(Label::from(match id {
            "new" => tl!("New"),
            "none" => tl!("None"),
            "merged" => tl!("Merged"),
            "transparency" => tl!("Transparency"),
            "mask" => tl!("Mask"),
            "selection" => tl!("Selection"),
            _ => return None,
        }))
    };
    list.iter().map(|n| (n.clone(), special(n).unwrap_or_else(|| Label::from(n.clone())))).collect()
}

/// A choice row with `default` selected.
fn choice(key: &str, label: &'static str, options: Vec<(String, Label)>, default: &str) -> Param {
    Param::new(key, label, ParamKind::choice(options, Some(default.to_owned())))
}

/// The file formats the exports to files write.
fn export_formats() -> Vec<(String, Label)> {
    opts(&[("png", tl!("PNG")), ("jpg", tl!("JPEG")), ("psd", tl!("PSD")), ("tiff", tl!("TIFF")), ("webp", tl!("WebP")), ("bmp", tl!("BMP"))])
}

/// The blending modes of the channel dialogs.
fn blend_modes() -> Vec<(String, Label)> {
    opts(&[
        ("multiply", tl!("Multiply")),
        ("screen", tl!("Screen")),
        ("normal", tl!("Normal")),
        ("overlay", tl!("Overlay")),
        ("softLight", tl!("Soft Light")),
        ("hardLight", tl!("Hard Light")),
        ("darken", tl!("Darken")),
        ("lighten", tl!("Lighten")),
        ("colorDodge", tl!("Color Dodge")),
        ("colorBurn", tl!("Color Burn")),
        ("linearBurn", tl!("Linear Burn")),
        ("linearDodge", tl!("Linear Dodge (Add)")),
        ("difference", tl!("Difference")),
        ("exclusion", tl!("Exclusion")),
        ("add", tl!("Add")),
        ("subtract", tl!("Subtract")),
    ])
}

/// A choice id read as words ("colorDodge" is "Color dodge"), translated when the catalog has the sentence.
pub(crate) fn label_of(key: &str) -> String {
    // A name that already reads as text ("iPhone 14 Pro Max", "Web 1920") is not camelCase.
    if key.contains(' ') {
        return tl!(key).to_owned();
    }
    let mut out = String::new();
    for (i, c) in key.chars().enumerate() {
        if i == 0 {
            out.extend(c.to_uppercase());
        } else if c.is_uppercase() {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    tl!(&out).to_owned()
}

/// A folder field with its Browse button.
fn folder(key: &str, label: &'static str, default: impl Into<String>) -> Param {
    Param::new(key, label, ParamKind::text(default)).browse(PathKind::Folder)
}

/// The JPEG quality of the exports to files: sent only for a JPEG with the Custom check box on, so an untouched
/// export writes what the command writes with no quality (a quality would also make a WebP lossy).
fn export_quality() -> Param {
    Param::new("quality", tl!("Quality"), ParamKind::optional_number(0.0, 12.0, None, 10.0, 0)).enabled_when("format", "jpg")
}

/// A document's name without its extension.
fn stem(name: &str) -> String {
    name.rsplit_once('.').map_or(name, |(a, _)| a).to_owned()
}

/// File › Open EXR Parts…: with `{path}` in the parameters the picker is skipped; without, an EXR is picked first. Then
/// the parts and channel groups that become the layers of one document (a Maya/Arnold render writes one part per AOV).
fn open_exr_parts_dialog(app: &mut PhotocraftApp, params: &Value) -> Option<Result<Value, String>> {
    if let Some(path) = params.get("path").and_then(Value::as_str) {
        return Some(open_exr_parts(app, path));
    }
    Some(app.pick_file_bytes_filtered(&["exr"], |app, name, _bytes| open_exr_parts(app, &name)))
}

/// Reads the parts of the EXR at `path` and opens their form.
fn open_exr_parts(app: &mut PhotocraftApp, path: &str) -> Result<Value, String> {
    let info = app.run("file.exrParts", json!({"path": path})).map_err(|e| e.to_string())?;
    let entries = info.as_array().cloned().unwrap_or_default();
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned();
    let mut rows = vec![
        Param::read_only_text("path", tl!("File"), file),
        choice("view", tl!("View"), opts(&[("both", tl!("Both")), ("left", tl!("Left")), ("right", tl!("Right"))]), "both"),
        Param::new("precomp", tl!("Add beauty precomp from the AOVs"), ParamKind::bool(false)),
    ];
    let mut any = false;
    for e in &entries {
        let kind = e["kind"].as_str().unwrap_or_default();
        let name = e["name"].as_str().unwrap_or_default();
        let channel_count = e["channels"].as_u64().unwrap_or(0);
        let key = format!("{kind}/{name}");
        match kind {
            "part" => {
                let on = !any && channel_count >= 3;
                any |= on;
                let label = format!("{name} ({}×{}, {channel_count} ch)", e["width"].as_u64().unwrap_or(0), e["height"].as_u64().unwrap_or(0));
                rows.push(Param::new(key, label, ParamKind::bool(on)));
            }
            "group" => rows.push(Param::new(key, format!("{name} ({channel_count} ch)"), ParamKind::bool(false))),
            _ => {}
        }
    }
    // Cryptomatte layers are information here (the picker comes separately); they ride along for the control channel.
    let crypto: Vec<Value> = entries.iter().filter(|e| e["kind"] == "cryptomatte").cloned().collect();
    let mut fixed = json!({"path": path});
    if !crypto.is_empty() {
        fixed["cryptomatte"] = Value::Array(crypto);
    }
    let id = open_form(app, "file.openExrParts", "Open EXR Parts…", rows, DialogFields::new(), fixed);
    Ok(opened(id))
}

/// The rows, and the opening values that are not row defaults, of command `id`'s form. `None` when `id` has no form or
/// its precondition (a document, a layer) is missing; the engine command then reports it.
pub(super) fn params_for(app: &mut PhotocraftApp, id: &str) -> Option<(Vec<Param>, DialogFields)> {
    let doc = app.session.active().map(|d| (d.doc.size.width, d.doc.size.height, d.doc.name.clone()));
    let dir = crate::dialogs::file_forms::default_dir(app);
    let plain = |rows: Vec<Param>| Some((rows, DialogFields::new()));
    match id {
        "layer.videoLayers.interpretFootage" => {
            let st = app.session.active()?;
            let layer = st.active_layer?;
            let fps = st.doc.layer(layer).and_then(|l| l.video.as_ref()).map_or(30.0, |v| f64::from(v.fps));
            plain(vec![Param::new("fps", tl!("Frame Rate"), ParamKind::number(Some(0.01), Some(1000.0), fps, 1.0, 2))])
        }
        "file.fileInfo" => file_info_form(app),
        "file.automate.fitImage" => {
            let (w, h, _) = doc?;
            plain(vec![
                Param::new("width", tl!("Width"), ParamKind::int(Some(1), None, i64::from(w))),
                Param::new("height", tl!("Height"), ParamKind::int(Some(1), None, i64::from(h))),
                Param::new("dontEnlarge", tl!("Don't Enlarge"), ParamKind::bool(false)),
            ])
        }
        "file.automate.conditionalModeChange" => {
            let modes = opts(&[
                ("any", tl!("Any")),
                ("rgb", tl!("RGB Color")),
                ("grayscale", tl!("Grayscale")),
                ("cmyk", tl!("CMYK Color")),
                ("lab", tl!("Lab Color")),
                ("indexed", tl!("Indexed Color")),
                ("bitmap", tl!("Bitmap")),
                ("duotone", tl!("Duotone")),
                ("multichannel", tl!("Multichannel")),
            ]);
            let targets = opts(&[("rgb", tl!("RGB Color")), ("grayscale", tl!("Grayscale")), ("cmyk", tl!("CMYK Color")), ("lab", tl!("Lab Color"))]);
            plain(vec![choice("from", tl!("From"), modes, "any"), choice("to", tl!("To"), targets, "rgb")])
        }
        "file.export.layersToFiles" => {
            let (_, _, name) = doc?;
            plain(vec![
                folder("dir", tl!("Destination"), dir),
                Param::new("prefix", tl!("File Name Prefix"), ParamKind::text(stem(&name))),
                Param::new("visibleOnly", tl!("Visible Layers Only"), ParamKind::bool(true)),
                choice("format", tl!("File Type"), export_formats(), "png"),
                export_quality(),
                Param::new("trim", tl!("Trim Layers"), ParamKind::bool(false)),
            ])
        }
        "file.export.layerCompsToFiles" => {
            let (_, _, name) = doc?;
            plain(vec![
                folder("dir", tl!("Destination"), dir),
                Param::new("prefix", tl!("File Name Prefix"), ParamKind::text(stem(&name))),
                Param::new("selectedOnly", tl!("Selected Layer Comp Only"), ParamKind::bool(false)),
                choice("format", tl!("File Type"), export_formats(), "png"),
                export_quality(),
            ])
        }
        "file.export.artboardsToFiles" => {
            let (_, _, name) = doc?;
            plain(vec![
                folder("dir", tl!("Destination"), dir),
                Param::new("prefix", tl!("File Name Prefix"), ParamKind::text(stem(&name))),
                choice("format", tl!("File Type"), export_formats(), "png"),
                export_quality(),
            ])
        }
        "file.export.artboardsToPdf" => {
            let (_, _, name) = doc?;
            plain(vec![
                Param::new("path", tl!("File"), ParamKind::text(format!("{dir}/{}.pdf", stem(&name)))),
                Param::new("quality", tl!("Quality"), ParamKind::int(Some(0), Some(12), 10)),
            ])
        }
        "file.export.dataSetsAsFiles" => {
            doc?;
            plain(vec![
                folder("dir", tl!("Destination"), dir),
                Param::new("naming", tl!("File Naming"), ParamKind::text("{name}")),
                choice("format", tl!("File Type"), export_formats(), "png"),
            ])
        }
        "layer.new.artboard" => {
            let (w, h, _) = doc?;
            let mut presets = vec![(String::new(), Label::from(tl!("Custom")))];
            presets.extend(photocraft_engine::artboard_cmds::PRESETS.iter().map(|p| (p.0.to_owned(), Label::from(crate::i18n::t(p.0).to_owned()))));
            plain(vec![
                Param::new("name", tl!("Name"), ParamKind::text("")),
                choice("preset", tl!("Preset"), presets, ""),
                Param::new("width", tl!("Width"), ParamKind::int(Some(1), None, i64::from(w))),
                Param::new("height", tl!("Height"), ParamKind::int(Some(1), None, i64::from(h))),
                choice(
                    "background",
                    tl!("Background Contents"),
                    opts(&[("white", tl!("White")), ("black", tl!("Black")), ("transparent", tl!("Transparent"))]),
                    "white",
                ),
            ])
        }
        "file.export.colorLookupTables" => {
            let (_, _, name) = doc?;
            let base = stem(&name);
            // A selected adjustment is a common use case, but existing no-selection exports
            // continue to bake the entire visible stack. The scope remains explicit in the UI.
            let selected_adjustments = app.session.active().is_some_and(|st| {
                let chosen = st.selected_layers();
                !chosen.is_empty()
                    && chosen.iter().all(|id| {
                        st.doc.layers.iter().any(|root| root.id == *id)
                            && st.doc.layer(*id).is_some_and(|layer| layer.visible && matches!(layer.content, photocraft_doc::LayerContent::Adjustment(_)))
                    })
            });
            let scope = if selected_adjustments { "selected" } else { "all" };
            plain(vec![
                Param::new("path", tl!("File"), ParamKind::text(format!("{dir}/{base}.cube"))),
                Param::new("size", tl!("Grid Size"), ParamKind::int(Some(2), Some(256), 33)),
                Param::new("title", tl!("Title"), ParamKind::text(base)),
                choice("scope", tl!("Scope"), opts(&[("all", tl!("All Visible Layers")), ("selected", tl!("Selected Adjustments"))]), scope),
            ])
        }
        // Photography automation (photo_cmds / lens_cmds): a folder (or the open documents).
        "file.automate.photomerge" => plain(vec![
            folder("paths", tl!("Source Folder"), dir),
            Param::new("useOpenDocuments", tl!("Use Open Documents"), ParamKind::bool(false)),
            choice(
                "layout",
                tl!("Layout"),
                opts(&[
                    ("auto", tl!("Auto")),
                    ("perspective", tl!("Perspective")),
                    ("cylindrical", tl!("Cylindrical")),
                    ("spherical", tl!("Spherical")),
                    ("collage", tl!("Collage")),
                    ("reposition", tl!("Reposition")),
                ]),
                "auto",
            ),
            Param::new("blend", tl!("Blend Images Together"), ParamKind::bool(true)),
            Param::new("vignetteRemoval", tl!("Vignette Removal"), ParamKind::bool(false)),
            Param::new("geometricCorrection", tl!("Geometric Distortion Correction"), ParamKind::bool(false)),
            Param::new("contentAwareFill", tl!("Content-Aware Fill Transparent Areas"), ParamKind::bool(false)),
        ]),
        "file.automate.mergeToHdrPro" => plain(vec![
            folder("paths", tl!("Source Folder"), dir),
            Param::new("useOpenDocuments", tl!("Use Open Documents"), ParamKind::bool(false)),
            Param::new("align", tl!("Attempt to Automatically Align Source Images"), ParamKind::bool(true)),
            Param::new("removeGhosts", tl!("Remove Ghosts"), ParamKind::bool(false)),
            choice("mode", tl!("Mode"), opts(&[("32", tl!("32 Bit")), ("16", tl!("16 Bit")), ("8", tl!("8 Bit"))]), "32"),
            choice(
                "method",
                tl!("Method"),
                opts(&[
                    ("localAdaptation", tl!("Local Adaptation")),
                    ("exposureGamma", tl!("Exposure and Gamma")),
                    ("highlightCompression", tl!("Highlight Compression")),
                    ("equalizeHistogram", tl!("Equalize Histogram")),
                ]),
                "localAdaptation",
            ),
            Param::new("radius", tl!("Radius"), ParamKind::number(Some(0.0), None, 7.0, 0.5, 1)),
            Param::new("strength", tl!("Strength"), ParamKind::number(Some(0.0), None, 0.52, 0.01, 2)),
            Param::new("gamma", tl!("Gamma"), ParamKind::number(Some(0.0), None, 1.0, 0.01, 2)),
            Param::new("exposure", tl!("Exposure"), ParamKind::number(None, None, 0.0, 0.1, 2)),
            Param::new("detail", tl!("Detail"), ParamKind::number(None, None, 30.0, 1.0, 0)),
            Param::new("saturation", tl!("Saturation"), ParamKind::number(None, None, 20.0, 1.0, 0)),
        ]),
        "file.automate.lensCorrection" => plain(vec![
            folder("input", tl!("Source Folder"), dir.clone()),
            folder("output", tl!("Destination"), format!("{dir}/corrected")),
            choice(
                "format",
                tl!("File Type"),
                opts(&[("same", tl!("Same as Source")), ("png", tl!("PNG")), ("jpg", tl!("JPEG")), ("psd", tl!("PSD")), ("tiff", tl!("TIFF"))]),
                "same",
            ),
            choice("profile", tl!("Lens Profile"), opts(&[("auto", tl!("Auto")), ("generic", tl!("Generic")), ("none", tl!("None"))]), "auto"),
            Param::new("correctDistortion", tl!("Geometric Distortion"), ParamKind::bool(true)),
            Param::new("correctVignette", tl!("Vignette"), ParamKind::bool(true)),
            Param::new("correctCA", tl!("Chromatic Aberration"), ParamKind::bool(true)),
            Param::new("autoScale", tl!("Auto Scale Image"), ParamKind::bool(true)),
            choice(
                "edge",
                tl!("Edge"),
                opts(&[
                    ("transparency", tl!("Transparency")),
                    ("edgeExtension", tl!("Edge Extension")),
                    ("black", tl!("Black Color")),
                    ("white", tl!("White Color")),
                ]),
                "transparency",
            ),
        ]),
        "file.scripts.imageProcessor" => plain(vec![
            choice(
                "source",
                tl!("Source"),
                opts(&[("current", tl!("Current document")), ("open", tl!("All open documents")), ("folder", tl!("Folder"))]),
                if doc.is_some() { "current" } else { "folder" },
            ),
            folder("input", tl!("Input Folder"), dir.clone()).visible_when("source", "folder"),
            Param::new("includeSubfolders", tl!("Include All Subfolders"), ParamKind::bool(false)).visible_when("source", "folder"),
            folder("output", tl!("Output Folder"), format!("{dir}/processed")),
            choice(
                "format",
                tl!("File Type"),
                opts(&[("jpg", tl!("JPEG")), ("png", tl!("PNG")), ("psd", tl!("PSD")), ("tiff", tl!("TIFF")), ("webp", tl!("WebP"))]),
                "jpg",
            ),
            Param::new("quality", tl!("Quality"), ParamKind::int(Some(0), Some(12), 8)),
            Param::new("width", tl!("Width"), ParamKind::int(Some(0), None, 0)),
            Param::new("height", tl!("Height"), ParamKind::int(Some(0), None, 0)),
            Param::new("convertToSrgb", tl!("Convert Profile to sRGB"), ParamKind::bool(true)),
        ]),
        "file.automate.batch" => batch_form(app, &dir),
        "select.saveSelection" | "select.loadSelection" | "image.applyImage" | "image.calculations" => channel_form(app, id),
        _ => None,
    }
}

/// File › File Info…: the document's metadata in Photoshop's order and labels, then its read-only Camera Data.
fn file_info_form(app: &mut PhotocraftApp) -> Option<(Vec<Param>, DialogFields)> {
    let info = app.session.execute("file.fileInfo", json!({})).ok()?;
    let text = |key: &str| info[key].as_str().unwrap_or_default().to_owned();
    let keywords = info["keywords"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; ")).unwrap_or_default();
    let mut rows = vec![
        Param::new("title", tl!("Document Title"), ParamKind::text("")),
        Param::new("author", tl!("Author"), ParamKind::text("")),
        Param::new("authorTitle", tl!("Author Title"), ParamKind::text("")),
        Param::new("description", tl!("Description"), ParamKind::text("")).multiline(3),
        Param::new("keywords", tl!("Keywords"), ParamKind::text("")).multiline(3),
        choice(
            "copyrightStatus",
            tl!("Copyright Status"),
            opts(&[("unknown", tl!("Unknown")), ("copyrighted", tl!("Copyrighted")), ("publicDomain", tl!("Public Domain"))]),
            "unknown",
        ),
        Param::new("copyright", tl!("Copyright Notice"), ParamKind::text("")).multiline(3),
        Param::new("copyrightUrl", tl!("Copyright Info URL"), ParamKind::text("")),
    ];
    let camera: Vec<(&'static str, String)> = photocraft_engine::file_cmds::CAMERA_FIELDS
        .iter()
        .filter_map(|(key, label)| info["camera"].get(*key).and_then(Value::as_str).map(|v| (*label, v.to_owned())))
        .collect();
<<<<<<< HEAD
    if !camera.is_empty() {
        rows.push(Param::section(tl!("Camera Data")));
        let lang = crate::i18n::current();
        for (label, value) in camera {
            rows.push(Param::read_only_text(format!("camera/{label}"), crate::i18n::tr_ctx(lang, "cameraData", label), value));
        }
=======
    let rest: Vec<String> = f.keys().filter(|k| !k.starts_with("__") && !keys.contains(k)).cloned().collect();
    keys.extend(rest);
    keys
}

/// Batch keeps its normal caption fields. Only same-name actions in the chosen set need another selector.
fn batch_occurrence_field(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    use crate::actions::{BATCH_OCCURRENCE, BATCH_OCCURRENCES};

    let Some(choices) = f.get(BATCH_OCCURRENCES).and_then(Value::as_array) else { return };
    let action = f.get("action");
    let set = f.get("set").filter(|set| !set.is_null());
    let mut choices: Vec<_> = choices
        .iter()
        .filter(|choice| choice.get("action") == action && choice.get("set").filter(|set| !set.is_null()) == set)
        .filter_map(|choice| Some((Some(choice["row"].as_u64()?), choice["label"].as_str()?)))
        .collect();
    if choices.len() < 2 {
        return;
    }
    let name = tl!("Occurrence");
    let mut current = f.get(BATCH_OCCURRENCE).and_then(Value::as_u64).filter(|row| choices.iter().any(|(choice, _)| *choice == Some(*row)));
    choices.insert(0, (None, tl!("By name")));
    ui.label(name);
    if crate::widgets::dropdown_literal(ui, "form-batch-occurrence", name, &mut current, &choices, 200.0) {
        f.insert(BATCH_OCCURRENCE.into(), json!(current));
    }
    ui.end_row();
}

/// Body of a `__form` dialog: text fields, number fields, checkboxes and `__choices` dropdowns.
pub fn form_body(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let choices = f.get("__choices").cloned().unwrap_or(Value::Null);
    // Data-driven labels (a per-part checkbox shows the part's name, not a catalogue entry)
    // come from `__labels` verbatim; everything else goes through the catalogue.
    let labels = f.get("__labels").cloned().unwrap_or(Value::Null);
    let command = f.get("__command").and_then(Value::as_str);
    let file_info = command == Some("file.fileInfo");
    let image_processor = command == Some("file.scripts.imageProcessor");
    let label = |k: &str| {
        if file_info {
            match k {
                "title" => return tl!("Document Title").to_owned(),
                "copyright" => return tl!("Copyright Notice").to_owned(),
                "copyrightUrl" => return tl!("Copyright Info URL").to_owned(),
                _ => {}
            }
        }
        labels.get(k).and_then(Value::as_str).map_or_else(|| label_of(k), str::to_string)
    };
    let keys = form_keys(f);
    let mut browse: Option<String> = None;
    egui::Grid::new("form-dialog").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        for k in keys {
            if image_processor
                && f.get("source").and_then(Value::as_str).is_some_and(|source| source != "folder")
                && matches!(k.as_str(), "input" | "includeSubfolders")
            {
                continue;
            }
            let v = f.get(&k).cloned().unwrap_or(Value::Null);
            if k == "naming" && f.get("__command").and_then(Value::as_str) == Some("file.automate.batch") {
                let label = ui.label(tl!("File Naming"));
                let mut naming = v.as_str().unwrap_or_default().to_owned();
                ui.add(egui::TextEdit::singleline(&mut naming).desired_width(260.0).hint_text("{name}"))
                    .labelled_by(label.id)
                    .on_hover_text("{name} · {serial} · {date} (UTC)");
                f.insert(k, json!(naming));
                ui.end_row();
                continue;
            }
            if k == "quality" && f.get("__command").and_then(Value::as_str) == Some("file.automate.batch") {
                ui.label(label(&k));
                let mut custom = v.as_f64().is_some();
                let mut quality = v.as_f64().or_else(|| f.get(BATCH_QUALITY).and_then(Value::as_f64)).unwrap_or(8.0).clamp(0.0, 12.0) as f32;
                custom_quality_row(ui, &mut custom, &mut quality, true);
                // None preserves the command's existing JPEG and lossless WebP defaults.
                // The inactive custom value is shell state, never a command parameter.
                f.insert(BATCH_QUALITY.into(), json!(quality));
                f.insert(k, if custom { json!(quality) } else { Value::Null });
                ui.end_row();
                continue;
            }
            if k == "quality" && f.contains_key(JPEG_QUALITY) {
                ui.label(label(&k));
                jpeg_quality_field(ui, f);
                ui.end_row();
                continue;
            }
            let new = match &v {
                Value::Array(_) | Value::Object(_) | Value::Null => continue,
                Value::Bool(b) => {
                    ui.label("");
                    let mut b = *b;
                    // A checkbox named in `__enabledBy` is greyed while the checkbox it names is off.
                    let on =
                        f.get("__enabledBy").and_then(|m| m.get(&k)).and_then(Value::as_str).is_none_or(|by| f.get(by).and_then(Value::as_bool) != Some(false));
                    let response = ui.add_enabled(on, egui::Checkbox::new(&mut b, label(&k)));
                    if let Some(tip) = f.get("__tooltips").and_then(|m| m.get(&k)).and_then(Value::as_str) {
                        response.on_hover_text(tip).on_disabled_hover_text(tip);
                    }
                    json!(b)
                }
                Value::String(s) => {
                    ui.label(label(&k));
                    let mut s = s.clone();
                    match choices.get(&k).and_then(Value::as_array) {
                        Some(opts) => {
                            // Action and set names are the user's own text, never catalogue entries.
                            let action_names = matches!(k.as_str(), "action" | "set")
                                && matches!(f.get("__command").and_then(Value::as_str), Some("file.automate.batch" | "file.automate.createDroplet"));
                            let opts: Vec<(String, String)> =
                                opts.iter().filter_map(Value::as_str).map(|o| (o.to_string(), if action_names { o.to_string() } else { label(o) })).collect();
                            let refs: Vec<(String, &str)> = opts.iter().map(|(a, b)| (a.clone(), b.as_str())).collect();
                            if action_names {
                                crate::widgets::dropdown_literal(ui, &format!("form-{k}"), &label(&k), &mut s, &refs, 200.0);
                            } else {
                                crate::widgets::dropdown(ui, &format!("form-{k}"), &label(&k), &mut s, &refs, 200.0);
                            }
                        }
                        None => {
                            if file_info && matches!(k.as_str(), "description" | "keywords" | "copyright") {
                                egui::ScrollArea::vertical().id_salt(("form-text", &k)).max_height(80.0).show(ui, |ui| {
                                    let response = ui.add(egui::TextEdit::multiline(&mut s).desired_rows(3).desired_width(260.0)).accessible_name(label(&k));
                                    if response.has_focus() {
                                        ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                                    }
                                });
                            } else {
                                ui.horizontal(|ui| {
                                    let field = ui.add(egui::TextEdit::singleline(&mut s).desired_width(260.0)).accessible_name(label(&k));
                                    if image_processor && crate::dialogs::is_folder_field(&k) {
                                        crate::field_tab::register(ui.ctx(), field.id);
                                        if crate::field_tab::reached(ui.ctx(), field.id) {
                                            crate::field_tab::select_all(ui.ctx(), field.id, &s);
                                        }
                                    }
                                    // A folder field gets the system folder picker (#7942, #9357).
                                    if crate::dialogs::is_folder_field(&k) && crate::widgets::secondary_button(ui, tl!("Browse…"), 0.0).clicked() {
                                        browse = Some(k.clone());
                                    }
                                });
                            }
                        }
                    }
                    json!(s)
                }
                Value::Number(n) => {
                    ui.label(label(&k));
                    if let Some(mut i) = n.as_i64() {
                        ui.add(egui::DragValue::new(&mut i).custom_parser(crate::widgets::parse_num)).accessible_name(label(&k));
                        json!(i)
                    } else {
                        let mut x = n.as_f64().unwrap_or(0.0);
                        ui.add(egui::DragValue::new(&mut x).speed(0.5).custom_parser(crate::widgets::parse_num)).accessible_name(label(&k));
                        json!(x)
                    }
                }
            };
            let batch_action = k == "action" && f.get("__command").and_then(Value::as_str) == Some("file.automate.batch");
            f.insert(k, new);
            ui.end_row();
            if batch_action {
                batch_occurrence_field(ui, f);
            }
        }
    });
    if let Some(key) = browse {
        f.insert(crate::dialogs::BROWSE_FIELD.into(), json!(key));
>>>>>>> upstream/main
    }
    let status = info["copyrightStatus"].as_str().unwrap_or("unknown").to_owned();
    let fields = DialogFields::from_pairs([
        ("title", text("title")),
        ("author", text("author")),
        ("authorTitle", text("authorTitle")),
        ("description", text("description")),
        ("keywords", keywords),
        ("copyrightStatus", status),
        ("copyright", text("copyright")),
        ("copyrightUrl", text("copyrightUrl")),
    ]);
    Some((rows, fields))
}

/// File › Automate › Batch…: the selected action and its set, and where the files come from and go.
fn batch_form(app: &PhotocraftApp, dir: &str) -> Option<(Vec<Param>, DialogFields)> {
    let action = crate::actions::selected_action(app)?;
    let (name, set) = (action.name.clone(), action.set.clone());
    let (names, sets) = crate::actions::action_form_choices(app);
    let mut rows = Vec::new();
    // The Set dropdown picks between same-named actions; an action outside any set has none.
    if !sets.is_empty() {
        rows.push(Param::new("set", tl!("Set"), ParamKind::choice(named(&sets), set)));
    }
    rows.extend([
        choice("action", tl!("Action"), named(&names), &name),
        choice("source", tl!("Source"), opts(&[("folder", tl!("Folder")), ("open", tl!("Opened Files"))]), "folder"),
        folder("input", tl!("Input Folder"), dir),
        Param::new("includeSubfolders", tl!("Include All Subfolders"), ParamKind::bool(false)),
        folder("output", tl!("Output Folder"), format!("{dir}/batch")),
        choice(
            "format",
            tl!("File Type"),
            opts(&[
                ("same", tl!("Same as Source")),
                ("png", tl!("PNG")),
                ("jpg", tl!("JPEG")),
                ("psd", tl!("PSD")),
                ("tiff", tl!("TIFF")),
                ("webp", tl!("WebP")),
            ]),
            "same",
        ),
        Param::new("naming", tl!("File Naming"), ParamKind::text("")).hint("{name}").tooltip("{name} · {serial} · {date} (UTC)"),
        Param::new("startingSerial", tl!("Starting Serial Number"), ParamKind::int(Some(1), None, 1)),
        // Off sends no quality, which keeps the command's JPEG and lossless WebP defaults.
        Param::new("quality", tl!("Quality"), ParamKind::optional_number(0.0, 12.0, None, 8.0, 0)),
        Param::new("stopOnError", tl!("Stop For Errors"), ParamKind::bool(false)),
        Param::new("errorLog", tl!("Error Log"), ParamKind::text("")),
    ]);
    Some((rows, DialogFields::new()))
}

/// The channel workflow forms (see channels_panel.rs for the channel references): Save and Load Selection, Apply
/// Image and Calculations.
fn channel_form(app: &PhotocraftApp, id: &str) -> Option<(Vec<Param>, DialogFields)> {
    let st = app.session.active()?;
    let d = &st.doc;
    let mode = d.pixel_format().mode;
    let mut chans: Vec<String> = vec![photocraft_engine::channel_cmds::composite_name(mode).to_string()];
    if mode.color_channels() > 1 {
        chans.extend(photocraft_engine::channel_cmds::color_names(mode).iter().map(|n| n.to_string()));
    }
    chans.extend(d.channels.iter().map(|c| c.name.clone()));
    let alphas: Vec<String> = std::iter::once("new".to_string()).chain(d.channels.iter().map(|c| c.name.clone())).collect();
    // The active layer's transparency, and its mask only when it has one.
    let layer = st.active_layer.and_then(|l| d.layer(l));
    let mut load = chans.clone();
    if layer.is_some() {
        load.push("transparency".into());
    }
    if layer.is_some_and(|l| l.mask.is_some()) {
        load.push("mask".into());
    }
    load.push("selection".into());
    let operation = |with_replace: bool| {
        let mut list = vec![("new", tl!("New"))];
        if with_replace {
            list.push(("replace", tl!("Replace")));
        }
        list.extend([("add", tl!("Add")), ("subtract", tl!("Subtract")), ("intersect", tl!("Intersect"))]);
        opts(&list)
    };
    let rows = match id {
        "select.saveSelection" => vec![
            choice("channel", tl!("Channel"), channels(&alphas), "new"),
            Param::new("name", tl!("Name"), ParamKind::text("")),
            choice("operation", tl!("Operation"), operation(true), "new"),
        ],
        "select.loadSelection" => {
            let first = d.channels.first().map_or_else(|| chans[0].clone(), |c| c.name.clone());
            vec![
                choice("channel", tl!("Channel"), channels(&load), &first),
                Param::new("invert", tl!("Invert"), ParamKind::bool(false)),
                choice("operation", tl!("Operation"), operation(false), "new"),
            ]
        }
        "image.applyImage" => {
            let mut masks = vec!["none".to_string()];
            masks.extend(load.iter().cloned());
            // Source layer: Merged, then the pixel layers top to bottom (the engine takes
            // the topmost layer of a name), shown by their own names.
            let mut layers = vec!["merged".to_string()];
            for (_, _, l) in d.walk().into_iter().rev() {
                if l.surface().is_some() && !layers.contains(&l.name) {
                    layers.push(l.name.clone());
                }
            }
            // Scale and Offset only affect Add and Subtract.
            vec![
                choice("sourceLayer", tl!("Layer"), channels(&layers), "merged"),
                choice("sourceChannel", tl!("Source Channel"), channels(&load), &chans[0]),
                Param::new("sourceInvert", tl!("Invert"), ParamKind::bool(false)),
                choice("blending", tl!("Blending"), blend_modes(), "multiply"),
                Param::new("opacity", tl!("Opacity"), ParamKind::number(Some(0.0), Some(100.0), 100.0, 1.0, 0)),
                Param::new("scale", tl!("Scale"), ParamKind::number(Some(0.001), Some(2.0), 1.0, 0.1, 3)),
                Param::new("offset", tl!("Offset"), ParamKind::int(Some(-255), Some(255), 0)),
                Param::new("preserveTransparency", tl!("Preserve Transparency"), ParamKind::bool(false)),
                choice("maskChannel", tl!("Mask Channel"), channels(&masks), "none"),
            ]
        }
        _ => vec![
            choice("source1Channel", tl!("Source 1 Channel"), channels(&load), &chans[0]),
            Param::new("source1Invert", tl!("Invert Source 1"), ParamKind::bool(false)),
            choice("source2Channel", tl!("Source 2 Channel"), channels(&load), &chans[0]),
            Param::new("source2Invert", tl!("Invert Source 2"), ParamKind::bool(false)),
            choice("blending", tl!("Blending"), blend_modes(), "multiply"),
            Param::new("opacity", tl!("Opacity"), ParamKind::number(Some(0.0), Some(100.0), 100.0, 1.0, 0)),
            choice(
                "result",
                tl!("Result"),
                opts(&[("newChannel", tl!("New Channel")), ("newDocument", tl!("New Document")), ("selection", tl!("Selection"))]),
                "newChannel",
            ),
        ],
    };
    Some((rows, DialogFields::new()))
}

/// Handle menu commands that need a shell action or a parameter dialog before execution.
pub(super) fn front(app: &mut PhotocraftApp, ctx: &egui::Context, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if id == "file.closeAll" || id == "file.closeOthers" {
        // Keep the remaining document's view (views are index-aligned with documents).
        let keep = app.session.active_index().and_then(|i| app.ui.views.get(i).cloned());
        let r = app.run(id, params.clone());
        if r.is_ok() {
            app.ui.views = if id == "file.closeAll" { Vec::new() } else { keep.into_iter().collect() };
            app.ui.windows.clear();
            app.sync_views();
        }
        return Some(r);
    }
    if id == "type.pasteLoremIpsum" && no_params(params) {
        // While typing, paste at the caret.
        if let Some(te) = app.ui.text_edit.clone() {
            let r = app.run(id, json!({"layer": te.layer, "at": te.caret, "coalesce": te.session}));
            if let (Ok(v), Some(te)) = (&r, app.ui.text_edit.as_mut())
                && let Some(c) = v.get("caret").and_then(Value::as_u64)
            {
                te.caret = c as usize;
                te.anchor = c as usize;
            }
            return Some(r);
        }
        return None;
    }
    // The form's own OK runs the command with every part chosen; only a lone `path` asks for the parts of that file.
    let path_only = params.as_object().is_some_and(|o| o.len() == 1 && o.contains_key("path"));
    if id == "file.openExrParts" && (no_params(params) || path_only) {
        return open_exr_parts_dialog(app, params);
    }
    if !no_params(params) {
        return None;
    }
    let label = photocraft_engine::commands::find(id).map_or(id, |c| c.label);
    match id {
        "file.openAs" => Some(app.open_dialog_file()),
        "file.openAsSmartObject" => Some(app.open_dialog_as_smart_object()),
        "file.saveACopy" => Some(save_a_copy(app)),
        "file.placeEmbedded" | "file.placeLinked" => {
            if app.ui.transform.is_some() {
                return Some(Err("Commit or cancel the transform first".into()));
            }
            let doc = match app.active_doc_id() {
                Ok(doc) => doc,
                Err(e) => return Some(Err(e)),
            };
            let linked = id == "file.placeLinked";
            let ctx = ctx.clone();
            Some(app.pick_file_bytes(move |app, name, bytes| {
                if app.ui.transform.is_some() {
                    return Err("Commit or cancel the transform first".into());
                }
                app.refocus(doc)?;
                let linked = linked.then(|| name.clone());
                let result = app.place_bytes(&name, bytes, linked)?;
                // In the background, Free Transform starts when the layer has landed.
                if result.get("frame").and_then(Value::as_u64).is_none() && !crate::jobs_ui::track_place(app, &name, &result) {
                    crate::transform_tool::begin_placed(app, &ctx)?;
                }
                Ok(result)
            }))
        }
        "layer.smartObjects.replaceContents" => {
            let st = app.session.active()?;
            let (doc, layer) = (st.doc.id, st.active_layer?);
            Some(app.pick_file_bytes(move |app, name, bytes| {
                app.with_document(doc, |app| {
                    let r = photocraft_engine::smart_cmds::replace_contents_bytes(&mut app.session, layer, &name, bytes).map_err(|e| e.to_string());
                    app.sync_views();
                    r
                })
            }))
        }
        "layer.smartObjects.exportContents" => Some(export_contents(app)),
        "layer.smartObjects.relinkToFile" => {
            let st = app.session.active()?;
            let (doc, layer) = (st.doc.id, st.active_layer?);
            // A link needs a file on disk: the desktop picker answers its full path, the web
            // picker only a name and the bytes, which can't be linked to.
            Some(app.pick_file_bytes(move |app, path, _| {
                if !std::path::Path::new(&path).is_absolute() {
                    return Err(format!("{path}: Relink to File needs a file on disk"));
                }
                app.with_document(doc, |app| app.run("layer.smartObjects.relinkToFile", json!({"layer": layer.0, "path": path})))
            }))
        }
        "layer.smartObjects.convertToLinked" => {
            let st = app.session.active()?;
            let (doc, layer) = (st.doc.id, st.active_layer?);
            let (name, _) = match photocraft_engine::smart_cmds::contents_of(&st.doc, layer) {
                Ok(c) => c,
                Err(e) => return Some(Err(e.to_string())),
            };
            Some(app.pick_save(&name, move |app, path| {
                app.with_document(doc, |app| app.run("layer.smartObjects.convertToLinked", json!({"layer": layer.0, "path": path})))
            }))
        }
        "layer.videoLayers.newVideoLayerFromFile" | "layer.videoLayers.replaceFootage" => {
            let doc = match app.active_doc_id() {
                Ok(doc) => doc,
                Err(e) => return Some(Err(e)),
            };
            let cmd = id.to_string();
            Some(app.pick_file_bytes(move |app, path, _bytes| {
                app.with_document(doc, |app| {
                    let r = app.run(&cmd, json!({"path": path}));
                    if let Ok(v) = &r {
                        // #11113: footage salvaged from a truncated GIF or WebP must say so.
                        let what = crate::i18n::fmt(crate::i18n::t("Imported {name}"), &[("name", &crate::file_open::display_name(&path))]);
                        crate::notices::footage_warnings(app, v, &what);
                    }
                    if r.is_ok() && app.session.active().is_some_and(|d| d.doc.timeline.is_some()) {
                        let _ = crate::panel_docking::show_panel(app, "timeline", Some(true));
                    }
                    app.sync_views();
                    r
                })
            }))
        }
<<<<<<< HEAD
        "layer.lockLayers" => Some(Ok(opened(crate::layers_panel_ui::open_lock_layers(app)))),
        "view.newGuideLayout" => Some(crate::guide_layout_ui::open(app).map(opened).ok_or_else(|| "no document".into())),
        "type.warpText" => Some(crate::dialogs::open_command_dialog(app, id, label).map(opened)),
        "file.scripts.loadFilesIntoStack" => Some(Ok(json!({"dialog": crate::stack_dialog::open(app).value().to_string()}))),
        "file.export.dataSetsAsFiles" if cfg!(target_arch = "wasm32") => Some(Err(tl!("Data Sets as Files is unavailable on the web").to_string())),
        "file.automate.batch" if crate::actions::selected_action(app).is_none() => Some(Err("record an action in the Actions panel first".into())),
        _ if is_form(id) => {
            let (rows, fields) = params_for(app, id)?;
            Some(Ok(opened(open_form(app, id, label, rows, fields, Value::Null))))
=======
        "layer.videoLayers.interpretFootage" => {
            let st = app.session.active()?;
            let id_layer = st.active_layer?;
            let fps = st.doc.layer(id_layer).and_then(|l| l.video.as_ref()).map_or(30.0, |v| v.fps);
            dialog(app, json!({"fps": fps}), json!({}))
        }
        "file.fileInfo" => {
            let info = app.session.execute("file.fileInfo", json!({})).ok()?;
            let kw = info["keywords"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; ")).unwrap_or_default();
            dialog(
                app,
                json!({
                    "title": info["title"],
                    "author": info["author"],
                    "authorTitle": info["authorTitle"],
                    "description": info["description"],
                    "keywords": kw,
                    "copyrightStatus": info["copyrightStatus"],
                    "copyright": info["copyright"],
                    "copyrightUrl": info["copyrightUrl"],
                    "__order": ["title", "author", "authorTitle", "description", "keywords", "copyrightStatus", "copyright", "copyrightUrl"],
                    "__labels": {
                        "title": tl!("Document Title"),
                        "copyright": tl!("Copyright Notice"),
                        "copyrightUrl": tl!("Copyright Info URL"),
                    },
                    "__camera": info["camera"]
                }),
                json!({"copyrightStatus": ["unknown", "copyrighted", "publicDomain"]}),
            )
        }
        "file.automate.fitImage" => {
            let (w, h, _) = doc?;
            // The values Fit Image last ran with, else the document's size (#13853). A stored
            // value that isn't a usable size is ignored.
            let kept = app.session.prefs().dialogs.get(photocraft_engine::file_cmds::FIT_IMAGE).cloned().unwrap_or_default();
            let size = |key: &str| kept.get(key).and_then(Value::as_u64).filter(|v| (1..=300_000).contains(v));
            let enlarge = kept.get("dontEnlarge").and_then(Value::as_bool).unwrap_or(false);
            dialog(
                app,
                json!({"width": size("width").unwrap_or(w.into()), "height": size("height").unwrap_or(h.into()), "dontEnlarge": enlarge, "__compact": true}),
                json!({}),
            )
        }
        "file.automate.conditionalModeChange" => dialog(
            app,
            json!({"from": "any", "to": "rgb"}),
            json!({"from": ["any", "rgb", "grayscale", "cmyk", "lab", "indexed", "bitmap", "duotone", "multichannel"], "to": ["rgb", "grayscale", "cmyk", "lab"]}),
        ),
        "view.newGuideLayout" => Some(Ok(json!({"dialog": crate::guide_layout_ui::open(app)}))),
        "type.warpText" => Some(Ok(json!({"dialog": crate::dialogs::open_command_dialog(app, id, label)}))),
        "file.export.layersToFiles" => {
            let (_, _, name) = doc?;
            dialog(
                app,
                json!({"dir": dir, "prefix": name.rsplit_once('.').map_or(name.as_str(), |(a, _)| a), "format": "png", "quality": null, JPEG_CUSTOM: false,
                    JPEG_QUALITY: 10, "visibleOnly": true, "trim": false, "__order": ["dir", "prefix", "visibleOnly", "format", "quality", "trim"]}),
                json!({"format": ["png", "jpg", "psd", "tiff", "webp", "bmp"]}),
            )
        }
        "file.export.layerCompsToFiles" | "file.export.artboardsToFiles" => {
            let (_, _, name) = doc?;
            let prefix = name.rsplit_once('.').map_or(name.as_str(), |(a, _)| a).to_string();
            let fields = if id == "file.export.layerCompsToFiles" {
                json!({"dir": dir, "prefix": prefix, "format": "png", "quality": null, JPEG_CUSTOM: false, JPEG_QUALITY: 10, "selectedOnly": false,
                    "__order": ["dir", "prefix", "selectedOnly", "format", "quality"]})
            } else {
                json!({"dir": dir, "prefix": prefix, "format": "png", "quality": null, JPEG_CUSTOM: false, JPEG_QUALITY: 10, "__order": ["dir", "prefix", "format", "quality"]})
            };
            dialog(app, fields, json!({"format": ["png", "jpg", "psd", "tiff", "webp", "bmp"]}))
        }
        "file.export.artboardsToPdf" => {
            let (_, _, name) = doc?;
            let stem = name.rsplit_once('.').map_or(name.as_str(), |(a, _)| a).to_string();
            dialog(app, json!({"path": format!("{dir}/{stem}.pdf"), "quality": 10}), json!({}))
        }
        "file.export.dataSetsAsFiles" => {
            #[cfg(target_arch = "wasm32")]
            {
                let _ = (app, dir, doc);
                Some(Err(tl!("Data Sets as Files is unavailable on the web").to_string()))
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                let (_, _, _) = doc?;
                dialog(app, json!({"dir": dir, "naming": "{name}", "format": "png"}), json!({"format": ["png", "jpg", "psd", "tiff", "webp", "bmp"]}))
            }
        }
        "layer.new.artboard" => {
            let (w, h, _) = doc?;
            let presets: Vec<&str> = std::iter::once("").chain(photocraft_engine::artboard_cmds::PRESETS.iter().map(|p| p.0)).collect();
            dialog(
                app,
                json!({"name": "", "preset": "", "width": w, "height": h, "background": "white"}),
                json!({"preset": presets, "background": ["white", "black", "transparent"]}),
            )
        }
        "file.export.colorLookupTables" => {
            let (_, _, name) = doc?;
            let stem = name.rsplit_once('.').map_or(name.as_str(), |(a, _)| a).to_string();
            // A selected adjustment is a common use case, but existing no-selection exports
            // continue to bake the entire visible stack. The scope remains explicit in the UI.
            let selected_adjustments = app.session.active().is_some_and(|st| {
                let chosen = st.selected_layers();
                !chosen.is_empty()
                    && chosen.iter().all(|id| {
                        st.doc.layers.iter().any(|root| root.id == *id)
                            && st.doc.layer(*id).is_some_and(|layer| layer.visible && matches!(layer.content, photocraft_doc::LayerContent::Adjustment(_)))
                    })
            });
            let scope = if selected_adjustments { "selected" } else { "all" };
            dialog(app, json!({"path": format!("{dir}/{stem}.cube"), "size": 33, "title": stem, "scope": scope}), json!({"scope": ["all", "selected"]}))
        }
        "file.scripts.loadFilesIntoStack" => Some(Ok(json!({"dialog": crate::stack_dialog::open(app)}))),
        // Photography automation (photo_cmds / lens_cmds): a folder (or the open documents).
        "file.automate.photomerge" => dialog(
            app,
            json!({"paths": dir, "useOpenDocuments": false, "layout": "auto", "blend": true, "vignetteRemoval": false, "geometricCorrection": false, "contentAwareFill": false}),
            json!({"layout": ["auto", "perspective", "cylindrical", "spherical", "collage", "reposition"]}),
        ),
        "file.automate.mergeToHdrPro" => dialog(
            app,
            json!({"paths": dir, "useOpenDocuments": false, "align": true, "removeGhosts": false, "mode": "32", "method": "localAdaptation", "radius": 7.0, "strength": 0.52, "gamma": 1.0, "exposure": 0.0, "detail": 30.0, "saturation": 20.0}),
            json!({"mode": ["32", "16", "8"], "method": ["localAdaptation", "exposureGamma", "highlightCompression", "equalizeHistogram"]}),
        ),
        "file.automate.lensCorrection" => dialog(
            app,
            json!({"input": dir, "output": format!("{dir}/corrected"), "format": "same", "profile": "auto", "correctDistortion": true, "correctVignette": true, "correctCA": true, "autoScale": true, "edge": "transparency"}),
            json!({"format": ["same", "png", "jpg", "psd", "tiff"], "profile": ["auto", "generic", "none"], "edge": ["transparency", "edgeExtension", "black", "white"]}),
        ),
        "file.scripts.imageProcessor" => dialog(
            app,
            json!({
                "source": if doc.is_some() { "current" } else { "folder" },
                "input": dir, "includeSubfolders": false, "output": format!("{dir}/processed"),
                "jpeg": {"enabled": true, "quality": 5, "convertToSrgb": false, "resize": false, "width": 1, "height": 1},
                "psd": {"enabled": false, "maximizeCompatibility": true, "resize": false, "width": 1, "height": 1},
                "tiff": {"enabled": false, "lzw": false, "resize": false, "width": 1, "height": 1},
                "__labels": {"current": tl!("Current document"), "open": tl!("All open documents"), "folder": tl!("Folder"), "includeSubfolders": tl!("Include All Subfolders")},
                "__order": ["source", "input", "includeSubfolders", "output"]
            }),
            json!({"source": ["current", "open", "folder"]}),
        ),
        "file.automate.batch" => {
            let Some(row) = crate::actions::selected_action_row(app) else {
                return Some(Err("record an action in the Actions panel first".into()));
            };
            let action = &app.session.actions.list[row];
            let (name, set) = (action.name.clone(), action.set.clone());
            let (names, sets) = crate::actions::action_form_choices(app);
            let mut fields = json!({
                "action": name, "source": "folder", "input": dir,
                "includeSubfolders": false, "__labels": subfolders_label(),
                "output": format!("{dir}/batch"), "format": "same", "quality": null,
                "__batchQuality": 8, "naming": "", "startingSerial": 1,
                "stopOnError": false, "errorLog": "",
                "__order": ["set", "action", "source", "input", "includeSubfolders", "output", "format", "naming", "startingSerial", "quality", "stopOnError", "errorLog"]
            });
            // The Set dropdown picks between same-named actions; an action outside any set has none.
            if let Some(set) = set {
                fields["set"] = json!(set);
            }
            let choices = json!({"set": sets, "action": names, "format": ["same", "png", "jpg", "psd", "tiff", "webp"], "source": ["folder", "open"]});
            let dialog = form(app, id, label, fields, choices);
            crate::actions::bind_batch_dialog(app, dialog, row);
            Some(Ok(json!({"dialog": dialog})))
        }
        // Channel workflow dialogs (see channels_panel.rs for the channel references).
        "select.saveSelection" | "select.loadSelection" | "image.applyImage" | "image.calculations" => {
            let st = app.session.active()?;
            let d = &st.doc;
            let mode = d.pixel_format().mode;
            let mut chans: Vec<String> = vec![photocraft_engine::channel_cmds::composite_name(mode).to_string()];
            if mode.color_channels() > 1 {
                chans.extend(photocraft_engine::channel_cmds::color_names(mode).iter().map(|n| n.to_string()));
            }
            chans.extend(d.channels.iter().map(|c| c.name.clone()));
            let alphas: Vec<String> = std::iter::once("new".to_string()).chain(d.channels.iter().map(|c| c.name.clone())).collect();
            let blends = [
                "multiply",
                "screen",
                "normal",
                "overlay",
                "softLight",
                "hardLight",
                "darken",
                "lighten",
                "colorDodge",
                "colorBurn",
                "linearBurn",
                "linearDodge",
                "difference",
                "exclusion",
                "add",
                "subtract",
            ];
            // The active layer's transparency, and its mask only when it has one.
            let layer = st.active_layer.and_then(|l| d.layer(l));
            let mut load = chans.clone();
            if layer.is_some() {
                load.push("transparency".into());
            }
            if layer.is_some_and(|l| l.mask.is_some()) {
                load.push("mask".into());
            }
            load.push("selection".into());
            match id {
                "select.saveSelection" => dialog(
                    app,
                    json!({"channel": "new", "name": "", "operation": "new"}),
                    json!({"channel": alphas, "operation": ["new", "replace", "add", "subtract", "intersect"]}),
                ),
                "select.loadSelection" => {
                    let first = d.channels.first().map_or_else(|| chans[0].clone(), |c| c.name.clone());
                    dialog(
                        app,
                        json!({"channel": first, "invert": false, "operation": "new"}),
                        json!({"channel": load, "operation": ["new", "add", "subtract", "intersect"]}),
                    )
                }
                "image.applyImage" => {
                    let mut masks = vec!["none".to_string()];
                    masks.extend(load.iter().cloned());
                    // Source layer: Merged, then the pixel layers top to bottom (the engine takes
                    // the topmost layer of a name), shown by their own names.
                    let mut layers = vec!["merged".to_string()];
                    for (_, _, l) in d.walk().into_iter().rev() {
                        if l.surface().is_some() && !layers.contains(&l.name) {
                            layers.push(l.name.clone());
                        }
                    }
                    let names: Map<String, Value> = layers.iter().skip(1).map(|n| (n.clone(), json!(n))).collect();
                    // Scale and Offset only affect Add and Subtract.
                    dialog(
                        app,
                        json!({"sourceLayer": "merged", "sourceChannel": chans[0], "sourceInvert": false, "blending": "multiply", "opacity": 100.0, "scale": 1.0, "offset": 0, "preserveTransparency": false, "maskChannel": "none", "__labels": names}),
                        json!({"sourceLayer": layers, "sourceChannel": load, "blending": blends, "maskChannel": masks}),
                    )
                }
                _ => dialog(
                    app,
                    json!({"source1Channel": chans[0], "source1Invert": false, "source2Channel": chans[0], "source2Invert": false, "blending": "multiply", "opacity": 100.0, "result": "newChannel"}),
                    json!({"source1Channel": load.clone(), "source2Channel": load, "blending": blends, "result": ["newChannel", "newDocument", "selection"]}),
                ),
            }
>>>>>>> upstream/main
        }
        _ => None,
    }
}

/// The name Save a Copy suggests: "<name> copy.psd" for a document with a file, or with
/// Preferences › File Handling › Do not append "copy" to filename (`copy_suffix` empty) the
/// document's own name (only the extension may change), as in Photoshop (#8080). A document that
/// was never saved suggests its own name without " copy" either way (#14169).
pub(super) fn suggested_copy_name(doc_name: &str, has_file: bool, copy_suffix: &str) -> String {
    let stem = doc_name.rsplit_once('.').map_or(doc_name, |(a, _)| a);
    let suffix = if has_file { copy_suffix } else { "" };
    format!("{stem}{suffix}.psd")
}

/// File › Save a Copy: pick a name, encode with the export service, write; the document's path
/// and saved state are untouched. An action being recorded keeps it as a `file.saveACopy` step
/// with its path (#4373).
fn save_a_copy(app: &mut PhotocraftApp) -> Result<Value, String> {
    let doc = app.session.active().ok_or("no document")?.doc.clone();
    let has_file = app.session.active().is_some_and(|d| d.path.is_some());
    let suggested = suggested_copy_name(&doc.name, has_file, app.session.prefs().file_handling.copy_suffix());
    // The copy is of the document as it was when asked.
    app.pick_save(&suggested, move |app, path| {
        // An RGB Targa asks for its bits per pixel first, a DDS texture for its compression, a
        // JPEG for its JPEG Options and a TIFF for its TIFF Options; the copy is written from the
        // prompt.
        let parked = app.with_document(doc.id, |app| {
            if crate::targa_options_ui::wants_prompt(app, &path) {
                crate::targa_options_ui::park(app, path.clone(), true, "file.saveACopy")?;
                return Ok(true);
            }
            if crate::dds_options_ui::wants_prompt(&path) {
                crate::dds_options_ui::park(app, path.clone(), true, "file.saveACopy")?;
                return Ok(true);
            }
            if crate::jpeg_options_ui::wants_prompt(app, &path) {
                crate::jpeg_options_ui::park(app, path.clone(), true, "file.saveACopy")?;
                return Ok(true);
            }
            if crate::tiff_options_ui::wants_prompt(app, &path) {
                crate::tiff_options_ui::park(app, path.clone(), true, "file.saveACopy")?;
                return Ok(true);
            }
            Ok(false)
        })?;
        if parked {
            return Ok(json!({"path": path, "warnings": []}));
        }
        let r = write_copy(app, &doc, &path, crate::FormatChoices::default())?;
        let step = json!({"path": path});
        photocraft_engine::actions_cmds::note_automation_step(&mut app.session, "file.saveACopy", &step);
        app.session.journal.push(("file.saveACopy".into(), step));
        Ok(r)
    })
}

/// Writes `doc` to `path` as a copy (Save a Copy, and its replayed action step): the document's
/// path and saved state are untouched. `choices` are the Targa, DDS and JPEG answers, when there
/// are any.
pub(crate) fn write_copy(app: &mut PhotocraftApp, doc: &photocraft_doc::Document, path: &str, choices: crate::FormatChoices) -> Result<Value, String> {
    let settings = choices.apply(app.save_settings());
    crate::app::ensure_memory_to_save(doc, path)?;
    let export = app.services.export.as_ref().ok_or("no exporter configured")?;
    let (bytes, warnings) = export(doc, path, &settings)?;
    let write = app.services.write.as_mut().ok_or("no writer configured")?;
    write(path, &bytes)?;
    app.detach_others_from_file(path, doc.id);
    let template = "Saved a copy as {path}";
    app.ui.status = crate::i18n::fmt(tl!(template), &[("path", path)]);
    app.ui.status_error = false;
    let what = crate::i18n::fmt(tl!(template), &[("path", &crate::file_open::display_name(path))]);
    crate::notices::io_warnings(app, &what, &warnings);
    Ok(json!({"path": path, "warnings": warnings}))
}

/// Layer › Smart Objects › Export Contents: saves the contents file under its own name by default.
fn export_contents(app: &mut PhotocraftApp) -> Result<Value, String> {
    let st = app.session.active().ok_or("no document")?;
    let layer = st.active_layer.ok_or("no layer selected")?;
    // The contents as they were when asked, like Save a Copy.
    let (name, bytes) = photocraft_engine::smart_cmds::contents_of(&st.doc, layer).map_err(|e| e.to_string())?;
    app.pick_save(&name.clone(), move |app, path| {
        let write = app.services.write.as_mut().ok_or("no writer configured")?;
        write(&path, &bytes)?;
        let template = "Exported contents to {path}";
        app.ui.status = crate::i18n::fmt(tl!(template), &[("path", &path)]);
        app.ui.status_error = false;
        Ok(json!({"path": path, "fileName": name, "bytes": bytes.len()}))
    })
}
