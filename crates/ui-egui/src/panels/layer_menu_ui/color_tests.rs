use super::*;
use egui::{Event, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::kittest::NodeT;
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::LayerId;

fn harness(kind: crate::theme::ThemeKind, ppp: f32) -> (Harness<'static, crate::PhotocraftApp>, LayerId, LayerId) {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 8, "height": 8})).unwrap();
    let red = LayerId(s.execute("layer.new.layer", json!({"name": "Red layer"})).unwrap()["layer"].as_u64().unwrap());
    s.execute("layer.setLabelColor", json!({"color": "red"})).unwrap();
    let other = LayerId(s.execute("layer.new.layer", json!({"name": "Other layer"})).unwrap()["layer"].as_u64().unwrap());
    let mut h = Harness::builder().with_size(vec2(1440.0, 900.0)).with_pixels_per_point(ppp).with_max_steps(64).build_eframe(move |cc| {
        crate::PhotocraftApp::setup_context(&cc.egui_ctx, kind);
        let mut app = crate::PhotocraftApp::new(s, crate::Services::default());
        app.ui.theme = kind;
        app
    });
    h.run_steps(8);
    (h, red, other)
}

fn click(h: &mut Harness<'_, crate::PhotocraftApp>, at: Pos2, button: PointerButton, modifiers: Modifiers) {
    h.event(Event::ModifiersChanged(modifiers));
    h.hover_at(at);
    h.step();
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos: at, button, pressed, modifiers });
        h.step();
    }
    h.run_steps(3);
}

fn row(h: &Harness<'_, crate::PhotocraftApp>, id: LayerId) -> egui::Rect {
    crate::layer_row_ui::recorded(&h.ctx).into_iter().find(|r| r.layer == id.0).unwrap().row
}

#[test]
fn layer_color_context_menu_edits_the_clicked_layer_and_preserves_multi_selection() {
    for ppp in [1.0, 2.0] {
        let (mut h, red, other) = harness(crate::theme::ThemeKind::Pro, ppp);
        let at = row(&h, red).center();
        click(&mut h, at, PointerButton::Secondary, Modifiers::NONE);
        let at = h.query_all_by_label("Color").last().expect("the Color submenu").rect().center();
        click(&mut h, at, PointerButton::Primary, Modifiers::NONE);
        let at = h.get_by_label("Seafoam").rect().center();
        click(&mut h, at, PointerButton::Primary, Modifiers::NONE);
        let st = h.state().session.active().unwrap();
        assert_eq!(st.active_layer, Some(red));
        assert_eq!(st.doc.layer(red).unwrap().label, LabelColor::Seafoam);
        assert_eq!(st.doc.layer(other).unwrap().label, LabelColor::None);
        h.state_mut().run("layer.select", json!({"layer": other.0, "mode": "add"})).unwrap();
        h.run_steps(4);
        let clicked = h.state().session.active().unwrap().doc.layer(red).unwrap();
        assert_eq!(common_color(h.state(), clicked, true), None);
        let at = row(&h, red).center();
        click(&mut h, at, PointerButton::Secondary, Modifiers::NONE);
        let at = h.query_all_by_label("Color").last().expect("the Color submenu").rect().center();
        click(&mut h, at, PointerButton::Primary, Modifiers::NONE);
        let at = h.get_by_label("Indigo").rect().center();
        click(&mut h, at, PointerButton::Primary, Modifiers::NONE);
        let st = h.state().session.active().unwrap();
        assert_eq!(st.selected_layers().len(), 2);
        assert_eq!(st.doc.layer(red).unwrap().label, LabelColor::Indigo);
        assert_eq!(st.doc.layer(other).unwrap().label, LabelColor::Indigo);
        assert_eq!(common_color(h.state(), st.doc.layer(red).unwrap(), true), Some(LabelColor::Indigo));
    }
}

#[test]
fn clicking_the_eye_of_a_coloured_row_toggles_visibility_and_keeps_the_label_in_every_theme() {
    for kind in crate::theme::ThemeKind::ALL {
        let (mut h, red, _) = harness(kind, 1.0);
        h.state_mut().run("layer.setProps", json!({"layer": red.0, "visible": false})).unwrap();
        h.run_steps(4);
        let eye = pos2(row(&h, red).left() + 17.0, row(&h, red).center().y);
        click(&mut h, eye, PointerButton::Primary, Modifiers::NONE);
        assert!(h.state().session.active().unwrap().doc.layer(red).unwrap().visible, "{kind:?}");
        assert_eq!(h.state().session.active().unwrap().doc.layer(red).unwrap().label, LabelColor::Red);
        click(&mut h, eye, PointerButton::Primary, Modifiers::ALT);
        assert!(h.state().session.active().unwrap().doc.layer(red).unwrap().visible);
    }
}

#[test]
fn right_clicking_eye_opens_color_choices_without_toggling_visibility() {
    let (mut h, red, other) = harness(crate::theme::ThemeKind::Pro, 1.0);
    let r = row(&h, red);
    let eye = pos2(r.left() + 17.0, r.center().y);
    click(&mut h, eye, PointerButton::Secondary, Modifiers::NONE);
    let blue = h.query_all_by_label("Blue").last().expect("the Blue choice").rect().center();
    click(&mut h, blue, PointerButton::Primary, Modifiers::NONE);

    let st = h.state().session.active().unwrap();
    assert!(st.doc.layer(red).unwrap().visible);
    assert_eq!(st.doc.layer(red).unwrap().label, LabelColor::Blue);
    assert_eq!(st.doc.layer(other).unwrap().label, LabelColor::None);
}

#[test]
fn layer_color_menu_marks_only_the_common_color() {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 8, "height": 8})).unwrap();
    s.execute("layer.new.layer", json!({"name": "Blue layer"})).unwrap();
    s.execute("layer.setLabelColor", json!({"color": "blue"})).unwrap();
    let app = crate::PhotocraftApp::new(s, crate::Services::default());
    let l = app.session.active().unwrap().doc.layer(app.session.active().unwrap().active_layer.unwrap()).unwrap().clone();
    let rows = color_rows(&app, &egui::Context::default(), &l, false);
    let checked = |name: &str| rows.iter().filter_map(|r| r.item()).find(|i| i.label() == name).and_then(|i| i.item_kind().checked());
    assert_eq!(checked("Blue"), Some(true));
    assert_eq!(checked("Red"), Some(false));
}
