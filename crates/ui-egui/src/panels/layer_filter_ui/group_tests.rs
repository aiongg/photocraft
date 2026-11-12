//! #10883: the Layers panel's Group filter.

use photocraft_engine::Session;
use serde_json::json;

use super::FilterBy;
use crate::PhotocraftApp;
use crate::state::UiState;

fn new_session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 64, "height": 48})).unwrap();
    s
}

/// The app with `s` open and the Layers panel filtered by Group.
fn app(s: Session) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(s, crate::Services::default());
    app.ui.layer_filter_by = FilterBy::Group;
    app
}

fn id(s: &Session, name: &str) -> u64 {
    s.active().unwrap().doc.walk().into_iter().find(|(_, _, l)| l.name == name).map(|(_, _, l)| l.id.0).unwrap_or_else(|| panic!("no layer {name}"))
}

fn select(s: &mut Session, name: &str) {
    let layer = id(s, name);
    s.execute("layer.select", json!({"layer": layer})).unwrap();
}

/// A new layer above the one selected (inside its group, if it is in one).
fn add(s: &mut Session, name: &str) {
    s.execute("layer.new.layer", json!({"name": name})).unwrap();
}

/// Groups the layer `name`.
fn group(s: &mut Session, name: &str, group_name: &str) {
    select(s, name);
    s.execute("layer.groupLayers", json!({"name": group_name})).unwrap();
}

use super::tests::listed_app as listed;

/// Top Pixel · Outer { Inner { Deep } } · Background.
fn nested() -> Session {
    let mut s = new_session();
    add(&mut s, "Top Pixel");
    add(&mut s, "Deep");
    group(&mut s, "Deep", "Inner");
    group(&mut s, "Inner", "Outer");
    s.execute("layer.setExpanded", json!({"all": true, "expanded": true})).unwrap();
    s
}

#[test]
fn lists_nested_groups_with_their_contents_and_hides_loose_layers() {
    let app = app(nested());
    assert_eq!(listed(&app), ["Outer", "Inner", "Deep"]);
}

#[test]
fn an_artboard_is_not_listed_but_its_groups_are() {
    let mut s = new_session();
    add(&mut s, "G child");
    group(&mut s, "G child", "Plain G");
    select(&mut s, "Background");
    add(&mut s, "A child");
    s.execute("layer.new.artboardFromLayers", json!({"name": "Artboard 1"})).unwrap();
    select(&mut s, "A child");
    add(&mut s, "GA child");
    group(&mut s, "GA child", "G in AB");
    s.execute("layer.setExpanded", json!({"all": true, "expanded": true})).unwrap();
    let app = app(s);
    assert_eq!(listed(&app), ["G in AB", "GA child", "Plain G", "G child"]);
}

#[test]
fn the_list_does_not_depend_on_the_selection() {
    let mut app = app(nested());
    select(&mut app.session, "Deep");
    let with_deep = listed(&app);
    app.session.execute("select.deselectLayers", json!({})).unwrap();
    assert_eq!(listed(&app), with_deep, "same list with nothing selected");
}

#[test]
fn group_is_set_by_name_through_the_control_channel() {
    let ui = UiState::default();
    let f = super::patched(&ui, &json!({"by": "group"})).unwrap();
    assert_eq!(f.by, FilterBy::Group);
    let err = super::patched(&ui, &json!({"by": "nope"})).unwrap_err();
    assert!(err.contains("group"), "the error lists group: {err}");
}

#[test]
fn lists_empty_groups_and_nothing_without_groups() {
    let mut app = app(new_session());
    assert!(listed(&app).is_empty(), "no group: nothing is listed");
    app.session.execute("layer.new.group", json!({"name": "Empty Group"})).unwrap();
    assert_eq!(listed(&app), ["Empty Group"], "an empty group is listed as a group row");
}
