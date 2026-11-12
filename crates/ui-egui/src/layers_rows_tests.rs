//! The Layers panel's rows through the real panel: what a click, a drag or a key on a row runs in
//! PhotoCraft. How the rows look and hit-test is craft-layers'.

use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use photocraft_doc::{Effect, Layer, LayerContent};
use photocraft_engine::Session;
use serde_json::json;

use crate::PhotocraftApp;
use crate::layers_panel_nodes::{click, double_click, drag, layer_id, part, row, row_drawn};

type H = Harness<'static, PhotocraftApp>;

fn harness(s: Session) -> H {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_step_dt(1.0 / 60.0).with_max_steps(64).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(s, crate::Services::default())
    });
    let ctx = h.ctx.clone();
    let (r, _) = crate::control::ControlRequest::new("ui.set", json!({"dock": {"collapsed": ["color", "properties", "history", "navigator"]}}));
    crate::control::handle(h.state_mut(), &ctx, &r);
    h.run_steps(8);
    h
}

fn st(h: &H) -> &photocraft_engine::DocState {
    h.state().session.active().unwrap()
}

fn layer(h: &H, name: &str) -> Layer {
    st(h).doc.layer(layer_id(h.state(), name)).unwrap().clone()
}

fn active(h: &H) -> Option<String> {
    st(h).active_layer.map(|id| st(h).doc.layer(id).unwrap().name.clone())
}

fn names(h: &H) -> Vec<String> {
    st(h).doc.layers.iter().map(|l| l.name.clone()).collect()
}

fn steps(h: &H) -> usize {
    st(h).history.past_len()
}

fn undo_label(h: &H) -> Option<&str> {
    st(h).history.undo_label()
}

/// "L0" … "L2" over the Background, "L2" active.
fn three() -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 64, "height": 48})).unwrap();
    for i in 0..3 {
        s.execute("layer.new.layer", json!({"name": format!("L{i}")})).unwrap();
    }
    s
}

/// "Styled" (a Drop Shadow and a Color Overlay) under "Top", active.
fn styled() -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 64, "height": 48})).unwrap();
    let id = s.execute("layer.new.layer", json!({"name": "Styled"})).unwrap()["layer"].as_u64().unwrap();
    s.execute("layer.layerStyle.dropShadow", json!({"layer": id})).unwrap();
    s.execute("layer.layerStyle.colorOverlay", json!({"layer": id})).unwrap();
    s.execute("layer.new.layer", json!({"name": "Top"})).unwrap();
    s
}

/// A layer with each lock ("transparency", "pixels", "position", "artboard", "all"), "all" active.
fn locked() -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 64, "height": 48})).unwrap();
    for key in ["transparency", "pixels", "position", "artboard", "all"] {
        let id = s.execute("layer.new.layer", json!({"name": key})).unwrap()["layer"].as_u64().unwrap();
        s.execute("layer.setProps", json!({"layer": id, "locks": {key: true}})).unwrap();
    }
    s
}

fn steps_of(s: Session) -> usize {
    s.active().unwrap().history.past_len()
}

fn fx(h: &H) -> (bool, Vec<bool>) {
    let fx = &layer(h, "Styled").effects;
    (fx.enabled, fx.items.iter().map(Effect::enabled).collect())
}

/// A click on a named part of a row runs PhotoCraft's command for it: a row selects its layer;
/// an eye hides or shows its layer (or effect, or the whole effects list), one step each, without
/// selecting; the fx triangle folds the effects rows away; a lock indicator clears the locks, and
/// on the Background makes it a normal layer.
/// What a click lands on: a row (by its layer's name) or a named part of one.
#[derive(Clone, Copy)]
enum On {
    Row(&'static str),
    Part(&'static str),
}

#[test]
fn clicks_on_row_parts() {
    use On::*;
    type Case = (&'static str, fn() -> Session, On, fn(&mut H));
    let cases: [Case; 9] = [
        ("a row selects its layer", three, Row("L0"), |h| {
            assert_eq!(active(h).as_deref(), Some("L0"));
            assert_eq!(st(h).selected_layers().len(), 1);
        }),
        ("the eye hides its layer without selecting it", three, Part("Visibility L0"), |h| {
            assert!(!layer(h, "L0").visible);
            assert_eq!(active(h).as_deref(), Some("L2"));
            let at = part(h, "Visibility L0").center();
            click(h, at, Modifiers::NONE);
            assert!(layer(h, "L0").visible, "a second click shows it");
        }),
        ("an effect's eye hides only that effect, one step", styled, Part("Visibility Color Overlay"), |h| {
            assert_eq!(fx(h), (true, vec![true, false]));
            assert_eq!(steps(h), 1 + steps_of(styled()), "one history step");
            assert_eq!(undo_label(h), Some("Hide Color Overlay"));
            assert!(h.state().ui.dialogs.is_empty(), "no Layer Style dialog");
        }),
        ("the Effects eye hides the list; each effect keeps its own eye", styled, Part("Visibility Effects"), |h| {
            assert_eq!(fx(h), (false, vec![true, true]));
            assert_eq!(undo_label(h), Some("Hide Layer Effects"));
        }),
        ("the fx triangle folds the effects rows away without selecting", styled, Part("Collapse effects Styled"), |h| {
            assert!(st(h).fx_collapsed.contains(&layer_id(h.state(), "Styled")));
            assert!(!row_drawn(h, "Drop Shadow"));
            assert_eq!(active(h).as_deref(), Some("Top"));
            let at = part(h, "Expand effects Styled").center();
            click(h, at, Modifiers::NONE);
            assert!(st(h).fx_collapsed.is_empty() && row_drawn(h, "Drop Shadow"));
        }),
        ("a lock indicator clears the locks without selecting", locked, Part("Unlock pixels"), |h| {
            assert!(!layer(h, "pixels").locks.any());
            assert_eq!(active(h).as_deref(), Some("all"));
        }),
        ("the lock indicator of a full lock", locked, Part("Unlock all"), |h| assert!(!layer(h, "all").locks.any())),
        ("the Background's lock indicator makes it a normal layer", locked, Part("Unlock Background"), |h| {
            let doc = &st(h).doc;
            assert!(doc.background().is_none());
            assert!(!doc.layers[0].locks.any(), "the bottom layer, no longer the Background, is unlocked");
        }),
        (
            "a disabled effect keeps its row",
            || {
                let mut s = styled();
                let st = s.active_mut().unwrap();
                let doc = std::sync::Arc::make_mut(&mut st.doc);
                let l = doc.layers.iter_mut().find(|l| l.name == "Styled").unwrap();
                let Some(Effect::DropShadow(shadow)) = l.effects.items.first_mut() else { panic!("a drop shadow") };
                shadow.common.enabled = false;
                s
            },
            Row("Drop Shadow"),
            |h| assert!(h.state().ui.dialogs.is_empty()),
        ),
    ];
    for (case, doc, on, after) in cases {
        let mut h = harness(doc());
        let at = match on {
            Row(name) => row(&h, name).center(),
            Part(label) => part(&h, label).center(),
        };
        click(&mut h, at, Modifiers::NONE);
        eprintln!("{case}");
        after(&mut h);
    }
}

/// Dragging down the eye column hides every layer swept over in one history step and never
/// reorders the layers, however far one pointer move jumps.
#[test]
fn sweeping_the_eyes_is_one_step() {
    for moves in [10, 1] {
        let mut h = harness(three());
        let (before, n) = (names(&h), steps(&h));
        let first = part(&h, "Visibility L2").center();
        let last = part(&h, "Visibility L0").center();
        h.event(Event::PointerMoved(first));
        h.step();
        h.event(Event::PointerButton { pos: first, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        h.step();
        for k in 1..=moves {
            h.event(Event::PointerMoved(first + (last - first) * (k as f32 / moves as f32)));
            h.step();
        }
        h.event(Event::PointerButton { pos: last, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        h.run_steps(3);
        for name in ["L0", "L1", "L2"] {
            assert!(!layer(&h, name).visible, "{moves} moves: {name} swept hidden");
        }
        assert!(layer(&h, "Background").visible, "{moves} moves: rows past the sweep are untouched");
        assert_eq!(names(&h), before, "{moves} moves: an eye drag never reorders");
        assert_eq!(steps(&h), n + 1, "{moves} moves: one history step");
        assert_eq!(undo_label(&h), Some("Layer Visibility"));
    }
}

/// The upper quarter of the row named `name`: a drop there lands above it.
fn above(h: &H, name: &str) -> Pos2 {
    let r = row(h, name);
    egui::pos2(r.center().x, r.top() + r.height() * 0.25)
}

/// A dragged row lands where it is dropped (Photoshop: Alt copies it there, one "Duplicate Layer"
/// step, the copy active); on the footer's New Layer it is duplicated, on New Group grouped, on
/// Delete deleted, a row outside the selection alone (#736).
#[test]
fn dragging_rows() {
    type Case = (&'static str, &'static str, fn(&H) -> Pos2, Modifiers, fn(&H));
    let cases: [Case; 5] = [
        (
            "a plain drag moves",
            "L0",
            |h| above(h, "L2"),
            Modifiers::NONE,
            |h| {
                assert_eq!(names(h), ["Background", "L1", "L2", "L0"]);
                assert_eq!(undo_label(h), Some("Reorder Layers"));
            },
        ),
        (
            "Alt copies",
            "L0",
            |h| above(h, "L2"),
            Modifiers::ALT,
            |h| {
                assert_eq!(names(h), ["Background", "L0", "L1", "L2", "L0 copy"]);
                assert_eq!(undo_label(h), Some("Duplicate Layer"));
                assert_eq!(active(h).as_deref(), Some("L0 copy"), "the copy is active");
            },
        ),
        (
            "onto New Layer: a copy of the dragged row alone",
            "L1",
            |h| part(h, "Create a new layer").center(),
            Modifiers::NONE,
            |h| {
                assert_eq!(names(h), ["Background", "L0", "L1", "L1 copy", "L2"]);
            },
        ),
        (
            "onto New Group: grouped, one step",
            "L0",
            |h| part(h, "Create a new group").center(),
            Modifiers::NONE,
            |h| {
                let group = st(h).doc.layers.iter().find(|l| l.is_group()).expect("a group");
                let LayerContent::Group(g) = &group.content else { panic!("a group") };
                assert_eq!(g.children.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["L0"]);
            },
        ),
        (
            "onto Delete",
            "L2",
            |h| part(h, "Delete layer").center(),
            Modifiers::NONE,
            |h| {
                assert_eq!(names(h), ["Background", "L0", "L1"]);
            },
        ),
    ];
    for (case, from, to, mods, after) in cases {
        let mut h = harness(three());
        let n = steps(&h);
        let (a, b) = (row(&h, from).center(), to(&h));
        drag(&mut h, a, b, mods);
        eprintln!("{case}");
        assert_eq!(steps(&h), n + 1, "{case}: one history step");
        after(&h);
    }
}

/// A double-click on a row's name, typed text and Enter rename the layer in one step.
#[test]
fn renaming_a_row_commits_on_enter() {
    let mut h = harness(three());
    let n = steps(&h);
    let at = row(&h, "L1").center();
    double_click(&mut h, at);
    h.event(Event::Text("Sky".into()));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(names(&h), ["Background", "L0", "Sky", "L2"]);
    assert_eq!(steps(&h), n + 1);
}

/// Cmd+] with the rows focused moves the active layer up once per press: the list's stacking key
/// and PhotoCraft's Layer › Arrange › Bring Forward share the chord, and one press runs one of them.
#[test]
fn command_bracket_moves_the_layer_once() {
    let mut h = harness(three());
    let at = row(&h, "L0").center();
    click(&mut h, at, Modifiers::NONE);
    let n = steps(&h);
    h.key_press_modifiers(Modifiers::COMMAND, Key::CloseBracket);
    h.run_steps(3);
    assert_eq!(names(&h), ["Background", "L1", "L0", "L2"], "one place up");
    assert_eq!(steps(&h), n + 1, "one history step");
}

fn groups_open(s: &Session) -> Vec<bool> {
    s.active().unwrap().doc.walk().into_iter().filter_map(|(_, _, l)| if let LayerContent::Group(g) = &l.content { Some(g.expanded) } else { None }).collect()
}

/// Two open groups, one inside the other, over "Loose".
fn nested() -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 64, "height": 48})).unwrap();
    s.execute("layer.new.layer", json!({"name": "Inner"})).unwrap();
    s.execute("layer.groupLayers", json!({"name": "In"})).unwrap();
    s.execute("layer.groupLayers", json!({"name": "Out"})).unwrap();
    s.execute("layer.new.layer", json!({"name": "Loose"})).unwrap();
    s.execute("layer.setExpanded", json!({"all": true, "expanded": true})).unwrap();
    s
}

fn panel_menu(h: &mut H) {
    let group = crate::panel_docking::probe::group_rect(h.state(), &h.ctx, "layers").expect("layers group");
    let menu = h.query_all_by_label("Panel menu").map(|n| n.rect()).find(|r| group.contains(r.center())).expect("Layers panel menu");
    click(h, menu.center(), Modifiers::NONE);
}

/// #143: the panel menu's Collapse All Groups closes every group, and the closed state survives a
/// PSD and a .pcraft round trip.
#[test]
fn collapse_all_groups_from_the_panel_menu_and_it_is_saved() {
    let mut h = harness(nested());
    assert!(groups_open(&h.state().session).iter().all(|o| *o));
    panel_menu(&mut h);
    let item = part(&h, "Collapse All Groups").center();
    click(&mut h, item, Modifiers::NONE);
    let open = groups_open(&h.state().session);
    assert_eq!(open.len(), 2);
    assert!(open.iter().all(|o| !o), "every group closed: {open:?}");
    let doc = (*st(&h).doc).clone();
    for name in ["c.psd", "c.pcraft"] {
        let bytes = photocraft_io::export(&doc, name, &Default::default()).unwrap().bytes;
        let back = photocraft_io::import(name, &bytes).unwrap().document;
        let mut s = Session::new();
        s.add_document(back, None);
        assert!(groups_open(&s).iter().all(|o| !o), "{name}: groups stay closed");
    }
}

/// Photoshop's panel menu has New Group… and New Group from Layers… just above Collapse All
/// Groups; both open the New Group dialog.
#[test]
fn new_group_items_in_the_panel_menu_open_the_dialog() {
    for (label, command) in [("New Group…", "layer.new.group"), ("New Group from Layers…", "layer.new.groupFromLayers")] {
        let mut h = harness(nested());
        panel_menu(&mut h);
        let item = part(&h, label);
        assert!(item.top() < part(&h, "Collapse All Groups").top(), "{label} comes before Collapse All Groups");
        click(&mut h, item.center(), Modifiers::NONE);
        let fields = h.state().ui.dialogs.last().map(|d| d.fields.clone()).expect("the New Group dialog opened");
        assert!(crate::new_group_ui::owns(&fields));
        assert_eq!(fields["__command"], command);
    }
}

/// The fx badge drags a layer's effects (Photoshop): dropped on another layer's row it moves them
/// there, with Alt it copies them.
#[test]
fn dragging_the_fx_badge() {
    for (mods, kept) in [(Modifiers::NONE, false), (Modifiers::ALT, true)] {
        let mut h = harness(styled());
        let from = crate::layers_panel_nodes::fx_badge(&h, "Styled").center();
        let to = row(&h, "Top").center();
        drag(&mut h, from, to, mods);
        assert_eq!(layer(&h, "Top").effects.items.len(), 2, "{mods:?}: Top has the effects");
        assert_eq!(!layer(&h, "Styled").effects.items.is_empty(), kept, "{mods:?}: Styled keeps them only with Alt");
    }
}
