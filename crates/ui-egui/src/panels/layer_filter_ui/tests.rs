//! The Layers panel filter over PhotoCraft's layers: what each type lists comes from
//! `PhotoLayers` (its rows' info and the app data `listed` reads), the criteria from `UiState`.

use photocraft_color::BlendMode;
use photocraft_doc::{Document, Layer};
use photocraft_engine::Session;
use serde_json::json;

use super::{Filter, FilterBy};
use crate::PhotocraftApp;
use crate::state::UiState;

/// `doc`'s layers, top to bottom, groups before their contents.
fn top_down(doc: &Document) -> Vec<&Layer> {
    fn add<'a>(layers: &'a [Layer], out: &mut Vec<&'a Layer>) {
        for l in layers.iter().rev() {
            out.push(l);
            add(l.children().unwrap_or_default(), out);
        }
    }
    let mut out = Vec::new();
    add(&doc.layers, &mut out);
    out
}

/// The names the Layers panel lists under `app`'s filter, top to bottom: every layer while the
/// filter is off, else those `PhotoLayers::listed` keeps.
pub(super) fn listed_app(app: &PhotocraftApp) -> Vec<String> {
    use craft::layers::ui::tree::LayerTree;
    let filter = crate::panels::layers_tree::filter_of(&app.ui);
    let st = app.session.active().unwrap();
    let tree = crate::panels::layers_tree::PhotoLayers::new(app, st, &filter, false, None);
    top_down(&st.doc).into_iter().filter(|l| !filter.active() || tree.listed(&l.id, &filter)).map(|l| l.name.clone()).collect()
}

/// The names `f` lists in a session holding `doc`.
pub(super) fn listed(doc: &Document, f: &Filter) -> Vec<String> {
    let mut s = Session::new();
    s.add_document(doc.clone(), None);
    let mut app = PhotocraftApp::new(s, crate::Services::default());
    super::store(&mut app.ui, f.clone());
    listed_app(&app)
}

/// Background, "Ink" (Multiply) and "Paper" in a group "Sheets", and "ink notes" (type) on top.
fn session() -> photocraft_engine::Session {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 64, "height": 48})).unwrap();
    s.execute("layer.new.layer", json!({"name": "Paper"})).unwrap();
    s.execute("layer.groupLayers", json!({"name": "Sheets"})).unwrap();
    let paper = layer(&s.active().unwrap().doc, "Paper");
    s.execute("layer.select", json!({"layer": paper})).unwrap();
    let ink = s.execute("layer.new.layer", json!({"name": "Ink"})).unwrap()["layer"].as_u64().unwrap();
    s.execute("layer.setProps", json!({"layer": ink, "blend": "Multiply"})).unwrap();
    let sheets = layer(&s.active().unwrap().doc, "Sheets");
    s.execute("layer.select", json!({"layer": sheets})).unwrap();
    let text = s.execute("type.create", json!({"text": "ink notes", "x": 4, "y": 20, "size": 12})).unwrap()["layer"].as_u64().unwrap();
    s.execute("layer.setProps", json!({"layer": text, "name": "ink notes"})).unwrap();
    // Opened last: an open active group would take the type layer inside it, as in Photoshop.
    s.execute("layer.setExpanded", json!({"all": true, "expanded": true})).unwrap();
    s
}

fn layer(doc: &Document, name: &str) -> u64 {
    doc.walk().into_iter().find(|(_, _, l)| l.name == name).map(|(_, _, l)| l.id.0).unwrap_or_else(|| panic!("no layer {name}"))
}

fn filter(by: FilterBy, kinds: &[&str], name: &str, mode: BlendMode) -> Filter {
    Filter::of(&UiState {
        layer_filter_by: by,
        layer_filter: kinds.iter().map(|k| k.to_string()).collect(),
        layer_filter_name: name.into(),
        layer_filter_mode: mode,
        ..UiState::default()
    })
}

#[test]
fn each_filter_type_lists_its_matches() {
    let s = session();
    let doc = &s.active().unwrap().doc;
    let all = listed(doc, &Filter::of(&UiState::default()));
    assert_eq!(all, ["ink notes", "Sheets", "Ink", "Paper", "Background"]);
    for (f, want, case) in [
        (filter(FilterBy::Kind, &["pixel"], "", BlendMode::Normal), &["Ink", "Paper", "Background"][..], "Kind: groups match no kind"),
        (filter(FilterBy::Kind, &["type"], "", BlendMode::Normal), &["ink notes"], "Kind › Type"),
        (filter(FilterBy::Name, &[], " INK ", BlendMode::Normal), &["ink notes", "Ink"], "Name: part of it, any case, spaces trimmed"),
        (filter(FilterBy::Name, &[], "sheet", BlendMode::Normal), &["Sheets"], "Name: groups match too"),
        (filter(FilterBy::Name, &[], "nothing like it", BlendMode::Normal), &[], "Name: no match"),
        (filter(FilterBy::Mode, &[], "", BlendMode::Multiply), &["Ink"], "Mode"),
        (filter(FilterBy::Mode, &[], "", BlendMode::PassThrough), &["Sheets"], "Mode: a group's own Pass Through counts"),
        (filter(FilterBy::Mode, &[], "", BlendMode::Normal), &["ink notes", "Paper", "Background"], "Mode › Normal"),
    ] {
        assert_eq!(listed(doc, &f), want, "{case}");
    }
}

#[test]
fn saved_settings_without_the_new_fields_still_load() {
    let mut saved = serde_json::to_value(UiState::default()).unwrap();
    let fields = saved.as_object_mut().unwrap();
    for key in ["layer_filter_by", "layer_filter_name", "layer_filter_mode"] {
        assert!(fields.remove(key).is_some(), "{key} is saved");
    }
    let ui: UiState = serde_json::from_value(saved).unwrap();
    assert_eq!((ui.layer_filter_by, ui.layer_filter_name.as_str(), ui.layer_filter_mode), (FilterBy::Kind, "", BlendMode::Normal));
    let round = serde_json::to_value(UiState { layer_filter_by: FilterBy::Mode, ..UiState::default() }).unwrap();
    assert_eq!(round["layer_filter_by"], "mode");
}

/// #5898: Photoshop's Color filter lists the layers with one colour label; it filters at once,
/// by no colour (None) at first, like Mode. Agents set it with `layerFilter.color`.
#[test]
fn the_color_filter_lists_one_label() {
    let mut s = session();
    let ink = layer(&s.active().unwrap().doc, "Ink");
    let sheets = layer(&s.active().unwrap().doc, "Sheets");
    // A group's colour goes to its layers too (Photoshop), so the group first, then Ink inside it.
    s.execute("layer.setLabelColor", json!({"layer": sheets, "color": "seafoam"})).unwrap();
    s.execute("layer.setLabelColor", json!({"layer": ink, "color": "red"})).unwrap();
    let doc = &s.active().unwrap().doc;
    let by_color = |c: photocraft_doc::LabelColor| Filter::of(&UiState { layer_filter_by: FilterBy::Color, layer_filter_color: c, ..UiState::default() });
    assert_eq!(listed(doc, &by_color(photocraft_doc::LabelColor::Red)), ["Ink"]);
    assert_eq!(listed(doc, &by_color(photocraft_doc::LabelColor::Seafoam)), ["Sheets", "Paper"]);
    assert_eq!(listed(doc, &by_color(photocraft_doc::LabelColor::None)), ["ink notes", "Background"]);
    assert!(listed(doc, &by_color(photocraft_doc::LabelColor::Blue)).is_empty(), "a colour no layer has lists nothing");
    // Over the control channel.
    let f = super::patched(&UiState::default(), &json!({"by": "color", "color": "red"})).unwrap();
    assert_eq!((f.by, f.color), (FilterBy::Color, photocraft_doc::LabelColor::Red));
    assert!(super::patched(&UiState::default(), &json!({"color": "crimson"})).unwrap_err().contains("crimson"));
    assert!(super::patched(&UiState::default(), &json!({"color": 2})).is_err());
}

/// On a Background: the artboard "Back" holding "Grass", the artboard "Front" holding the group
/// "Trees" with "Oak", and "Loose" in no artboard, on top.
fn boards_session() -> photocraft_engine::Session {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 64, "height": 48})).unwrap();
    // Every layer is made before the artboards, since a new layer goes into the artboard it is on.
    for name in ["Grass", "Oak", "Loose"] {
        s.execute("layer.new.layer", json!({"name": name})).unwrap();
    }
    let grass = layer(&s.active().unwrap().doc, "Grass");
    s.execute("layer.select", json!({"layer": grass})).unwrap();
    s.execute("layer.new.artboardFromLayers", json!({"name": "Back"})).unwrap();
    let oak = layer(&s.active().unwrap().doc, "Oak");
    s.execute("layer.select", json!({"layer": oak})).unwrap();
    s.execute("layer.groupLayers", json!({"name": "Trees"})).unwrap();
    s.execute("layer.new.artboardFromLayers", json!({"name": "Front"})).unwrap();
    let loose = layer(&s.active().unwrap().doc, "Loose");
    s.execute("layer.select", json!({"layer": loose})).unwrap();
    s.execute("layer.arrange.bringToFront", json!({})).unwrap();
    s.execute("layer.setExpanded", json!({"all": true, "expanded": true})).unwrap();
    s
}

/// The names the Artboard filter lists with the layers `names` selected.
fn listed_by_artboard(app: &mut PhotocraftApp, names: &[&str]) -> Vec<String> {
    app.run("select.deselectLayers", json!({})).unwrap();
    for (i, name) in names.iter().enumerate() {
        let id = layer(&app.session.active().unwrap().doc, name);
        app.run("layer.select", json!({"layer": id, "mode": if i == 0 { "replace" } else { "add" }})).unwrap();
    }
    super::store(&mut app.ui, filter(FilterBy::Artboard, &[], "", BlendMode::Normal));
    listed_app(app)
}

#[test]
fn the_artboard_filter_lists_the_artboards_that_hold_the_selected_layers() {
    let mut app = PhotocraftApp::new(boards_session(), crate::Services::default());
    let all = ["Loose", "Front", "Trees", "Oak", "Back", "Grass", "Background"];
    for (selected, want, case) in [
        (&[][..], &all[..], "nothing selected: every layer"),
        (&["Oak"], &["Front", "Trees", "Oak"], "a layer at any depth in an artboard: that artboard"),
        (&["Front"], &["Front", "Trees", "Oak"], "the artboard itself"),
        (&["Grass"], &["Back", "Grass"], "the other artboard"),
        (&["Oak", "Grass"], &["Front", "Trees", "Oak", "Back", "Grass"], "layers in two artboards: both"),
        (&["Loose"], &all, "a layer in no artboard: every layer"),
        (&["Oak", "Loose"], &all, "one of them in no artboard: every layer"),
    ] {
        assert_eq!(listed_by_artboard(&mut app, selected), want, "{case}");
    }
}

/// A layer in an artboard nested in another lists the outer artboard.
#[test]
fn the_artboard_filter_lists_the_outer_artboard_for_a_layer_in_a_nested_artboard() {
    use photocraft_color::{ColorMode, SampleType};
    use photocraft_doc::{Artboard, Group, LayerContent, Size};
    use photocraft_geom::Rect;

    fn group(name: &str, children: Vec<Layer>, artboard: Option<Artboard>) -> Layer {
        Layer::new(name, LayerContent::Group(Group { children, expanded: true, artboard, frame_stroke: None }))
    }
    let inner = group("Inner", vec![group("Leaf", vec![], None)], Some(Artboard::new(Rect::new(0, 0, 4, 4))));
    let outer = group("Outer", vec![inner, group("Sibling", vec![], None)], Some(Artboard::new(Rect::new(0, 0, 8, 8))));
    let other = group("Other", vec![], Some(Artboard::new(Rect::new(8, 0, 8, 8))));
    let mut doc = Document::new("Nested artboards", Size::new(16, 8), ColorMode::Rgb, SampleType::U8);
    doc.layers.push(outer);
    doc.layers.push(other);
    let mut s = Session::new();
    s.add_document(doc, None);
    let mut app = PhotocraftApp::new(s, crate::Services::default());
    let outer_rows = ["Outer", "Sibling", "Inner", "Leaf"];
    assert_eq!(listed_by_artboard(&mut app, &["Leaf"]), outer_rows, "a layer in a nested artboard: the outer artboard's rows");
    assert_eq!(listed_by_artboard(&mut app, &["Inner"]), outer_rows, "the nested artboard itself: the outer artboard's rows");
    assert_eq!(listed_by_artboard(&mut app, &["Leaf", "Other"]), ["Other", "Outer", "Sibling", "Inner", "Leaf"]);
}

/// Creates a PNG fixture and returns its temporary directory with the file path.
fn art_file() -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("art.png");
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 8, "height": 8, "background": "#ff0000"})).unwrap();
    s.execute("file.saveACopy", json!({"path": file.to_string_lossy(), "layers": false})).unwrap();
    (directory, file)
}

/// "Linked art" (linked to `file`) above "Embedded art" (a copy of `file`), "Plain" and
/// "Background".
fn smart_session(file: &std::path::Path) -> photocraft_engine::Session {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 32, "height": 32})).unwrap();
    s.execute("layer.new.layer", json!({"name": "Plain"})).unwrap();
    for (command, name) in [("file.placeEmbedded", "Embedded art"), ("file.placeLinked", "Linked art")] {
        s.execute(command, json!({"path": file.to_string_lossy()})).unwrap();
        let id = s.active().unwrap().active_layer.unwrap().0;
        s.execute("layer.setProps", json!({"layer": id, "name": name})).unwrap();
    }
    s
}

#[test]
fn the_smart_object_filter_lists_linked_missing_and_embedded_smart_objects() {
    let (_directory, file) = art_file();
    let mut app = PhotocraftApp::new(smart_session(&file), crate::Services::default());
    let by_smart = |app: &mut PhotocraftApp, smart: &[&str]| {
        super::store(&mut app.ui, Filter { smart: smart.iter().map(|k| k.to_string()).collect(), ..filter(FilterBy::SmartObject, &[], "", BlendMode::Normal) });
        listed_app(app)
    };
    assert_eq!(by_smart(&mut app, &[]), ["Linked art", "Embedded art", "Plain", "Background"], "no toggle on: every layer");
    assert_eq!(by_smart(&mut app, &["linked"]), ["Linked art"]);
    assert_eq!(by_smart(&mut app, &["embedded"]), ["Embedded art"]);
    assert_eq!(by_smart(&mut app, &["linked", "embedded"]), ["Linked art", "Embedded art"]);
    assert_eq!(by_smart(&mut app, &["missing"]), Vec::<String>::new(), "the file is there");
    std::fs::remove_file(&file).unwrap();
    app.run("layer.smartObjects.updateChanged", json!({})).unwrap();
    assert_eq!(by_smart(&mut app, &["missing"]), ["Linked art"]);
    assert_eq!(by_smart(&mut app, &["missing", "embedded"]), ["Linked art", "Embedded art"]);
}

#[test]
fn saved_settings_without_the_smart_object_toggles_still_load() {
    let mut saved = serde_json::to_value(UiState::default()).unwrap();
    assert!(saved.as_object_mut().unwrap().remove("layer_filter_smart").is_some(), "layer_filter_smart is saved");
    let ui: UiState = serde_json::from_value(saved).unwrap();
    assert!(ui.layer_filter_smart.is_empty());
    let round =
        serde_json::to_value(UiState { layer_filter_by: FilterBy::SmartObject, layer_filter_smart: vec!["missing".into()], ..UiState::default() }).unwrap();
    let back: UiState = serde_json::from_value(round.clone()).unwrap();
    assert_eq!(
        (round["layer_filter_by"].as_str(), back.layer_filter_by, back.layer_filter_smart),
        (Some("smartObject"), FilterBy::SmartObject, vec!["missing".to_string()])
    );
}
