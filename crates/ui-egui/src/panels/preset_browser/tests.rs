use super::*;
use craft::behavior::{EditEvent, Mixed};
use craft::keymap::Mods;
use craft::ui::contract::ReorderMove;
use std::sync::Arc;

fn group(name: &str) -> PresetKey {
    PresetKey::Group(name.into())
}

fn item(key: &str) -> PresetKey {
    PresetKey::Item(key.into())
}

fn reorder(key: PresetKey, to: usize) -> PresetEvent<PresetKey> {
    let mv = ReorderMove::new(key.clone(), vec![key], to, false);
    PresetEvent::Reorder(EditEvent::set(Mixed::Same(mv.clone()), mv, Arc::from("Reorder"), None))
}

fn run(command: &str, params: Value) -> Option<Act> {
    Some(Act::Run(command.into(), params))
}

/// Every browser event of the shared mapping, for each engine prefix that takes `{prefix}.edit` and `{prefix}.new`:
/// the command and params it runs, or the folder move it makes.
#[test]
fn each_preset_event_runs_its_edit_command_for_every_prefix() {
    // Folder A holds a1 and a2, folder B holds b1.
    let rows = [group("A"), item("a1"), item("a2"), group("B"), item("b1")];
    for prefix in ["gradient.presets", "pattern.presets", "style.presets", "shape.presets", "tool.presets"] {
        let panel = PresetPanel { id: "panel", label: "Panel", prefix, new_params: || json!({"name": "New"}), rename_new: true };
        let edit = format!("{prefix}.edit");
        let cases: Vec<(PresetEvent<PresetKey>, Option<Act>)> = vec![
            (PresetEvent::rename(item("a1"), "b"), run(&edit, json!({"action": "rename", "preset": "a1", "name": "b"}))),
            (PresetEvent::rename(group("A"), "C"), run(&edit, json!({"action": "renameGroup", "group": "A", "name": "C"}))),
            (PresetEvent::Delete(item("a1")), run(&edit, json!({"action": "delete", "preset": "a1"}))),
            (PresetEvent::Delete(group("A")), run(&edit, json!({"action": "deleteGroup", "group": "A"}))),
            (PresetEvent::move_to(item("a1"), group("B")), run(&edit, json!({"action": "move", "preset": "a1", "to": "B"}))),
            (PresetEvent::Footer(PresetFooterEvent::NewGroup), run(&edit, json!({"action": "newGroup", "name": "Group"}))),
            (PresetEvent::Footer(PresetFooterEvent::New), run(&format!("{prefix}.new"), json!({"name": "New"}))),
            (PresetEvent::Footer(PresetFooterEvent::Delete), run(&edit, json!({"action": "delete", "preset": "a2"}))),
            // a1 lands after b1: the end of B.
            (reorder(item("a1"), 4), run(&edit, json!({"action": "move", "preset": "a1", "to": "B", "index": 1}))),
            // a2 lands first in A.
            (reorder(item("a2"), 1), run(&edit, json!({"action": "move", "preset": "a2", "to": "A", "index": 0}))),
            // B goes before A; A after B is the last place.
            (reorder(group("B"), 0), Some(Act::MoveFolder { from: "B".into(), before: Some("A".into()) })),
            (reorder(group("A"), 2), Some(Act::MoveFolder { from: "A".into(), before: None })),
            (PresetEvent::Action(0), None),
        ];
        for (event, expected) in cases {
            assert_eq!(act(panel, &event, &rows, Some("a2")), expected, "{prefix}: {event:?}");
        }
    }
}

/// The events that use a preset: Patterns select on a click and fill on a double click or a drop; Styles apply,
/// Shift adding the effects, and a drop styles the active layer.
#[test]
fn patterns_and_styles_use_a_preset_their_own_way() {
    let shift = Mods { shift: true, ..Mods::default() };
    let at = pos2(10.0, 20.0);
    let cases: Vec<(PresetPanel, PresetEvent<PresetKey>, Option<Act>)> = vec![
        (PATTERNS, PresetEvent::select(item("p"), Mods::default()), run("pattern.presets.select", json!({"pattern": "p"}))),
        (PATTERNS, PresetEvent::activate(item("p"), Mods::default()), run("pattern.presets.apply", json!({"pattern": "p"}))),
        (PATTERNS, PresetEvent::drop_at(item("p"), at, Mods::default()), run("pattern.presets.apply", json!({"pattern": "p"}))),
        (STYLES, PresetEvent::select(item("s"), Mods::default()), run("style.presets.apply", json!({"preset": "s", "add": false}))),
        (STYLES, PresetEvent::select(item("s"), shift), run("style.presets.apply", json!({"preset": "s", "add": true}))),
        (STYLES, PresetEvent::drop_at(item("s"), at, shift), run("style.presets.apply", json!({"preset": "s"}))),
        (STYLES, PresetEvent::select(group("G"), Mods::default()), None),
    ];
    for (panel, event, expected) in cases {
        assert_eq!(panel_act(panel, &event), expected, "{}: {event:?}", panel.id);
    }
}
