//! The Type tool's family field: the options bar's `FontPicker`, the font state it edits, and the preview text.

use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::{PhotocraftApp, Services};

/// Frames a harness may take to settle: opening the list loads the rows' samples over several passes.
const SETTLE_STEPS: u64 = 64;

fn app_with_layer() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
    app.run("file.new", json!({"width": 200, "height": 100})).unwrap();
    app.run("type.create", json!({"text": "Sample", "font": "Inter", "size": 18})).unwrap();
    app.ui.tool = crate::state::Tool::Type;
    app.sync_views();
    app
}

/// The options bar over `app`, on the app's own context.
fn bar_harness(app: PhotocraftApp) -> Harness<'static, PhotocraftApp> {
    // The full app frame: the app takes the window's context on its first pass.
    Harness::builder().with_size(vec2(1600.0, 1000.0)).with_max_steps(SETTLE_STEPS).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, crate::theme::AppTheme::pro());
        app
    })
}

fn field<'a>(h: &'a Harness<'static, PhotocraftApp>) -> egui_kittest::Node<'a> {
    h.query_all_by_label("Font family").next().expect("the family field")
}

<<<<<<< HEAD
/// Opens the family field and types `name` into it.
fn type_family(h: &mut Harness<'static, PhotocraftApp>, name: &str) {
    field(h).click();
=======
fn harness_with_recents(preview: FontPreview, recents: Vec<String>) -> Harness<'static, State> {
    harness_sized(preview, recents, vec2(600.0, 900.0))
}

fn harness_sized(preview: FontPreview, recents: Vec<String>, size: egui::Vec2) -> Harness<'static, State> {
    // Enough families that the unfiltered menu hits its maximum height and scrolls.
    let mut families: Vec<String> = (0..60).map(|i| format!("Family {i:02}")).collect();
    families.push("Zeta Sans".into());
    let state = State { current: "Family 00".into(), families, button: egui::Id::NULL, popup: egui::Id::NULL, preview, recents };
    // As the app does: the wheel is a gesture over the field or past it (#11353).
    let mut h = Harness::builder()
        .with_size(size)
        .with_max_steps(SETTLE_STEPS)
        .with_context_setup(|ctx| ctx.add_plugin(crate::field_wheel::FieldWheel))
        .build_ui_state(
            |ui, s: &mut State| {
                s.button = ui.make_persistent_id(egui::IdSalt::new("type-font"));
                s.popup = s.button.with("popup");
                let fams = s.families.clone();
                let recents = s.recents.clone();
                font_picker_in(ui, &mut s.current, 170.0, &fams, s.preview, &recents);
            },
            state,
        );
>>>>>>> upstream/main
    h.run();
    field(h).type_text(name);
    h.run();
}

fn layer_font(app: &PhotocraftApp) -> String {
    let st = app.session.active().unwrap();
    match &st.doc.layer(st.active_layer.unwrap()).unwrap().content {
        photocraft_doc::LayerContent::Text(t) => t.font_family.clone(),
        _ => panic!("type layer"),
    }
}

#[test]
fn font_cycle_in_options_bar_edits_the_layer_and_can_be_undone() {
    let mut h = bar_harness(app_with_layer());
    h.run_steps(4);
    type_family(&mut h, "JetBrains");
    assert_eq!(layer_font(h.state()), "Inter", "typing creates no document edit");
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(layer_font(h.state()), "JetBrains Mono");
    let steps = h.state().session.active().unwrap().history.entries().len();
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(layer_font(h.state()), "Inter");
    assert_eq!(h.state().session.active().unwrap().history.entries().len(), steps - 1, "one pick is one undoable step");
    h.state_mut().run("edit.redo", json!({})).unwrap();
    assert_eq!(layer_font(h.state()), "JetBrains Mono");
}

#[test]
fn options_bar_hover_previews_on_canvas_without_committing() {
    use photocraft_doc::LayerContent;

    let app = app_with_layer();
    let layer = app.session.active().unwrap().active_layer.unwrap();
    let mut h = bar_harness(app);
    h.run_steps(3);
    let font_of = |app: &mut PhotocraftApp| {
        let (shown, key) = crate::canvas::display_doc(app, 0);
        let LayerContent::Text(t) = &shown.layer(layer).unwrap().content else { panic!("type layer") };
        (t.char_runs()[0].style.font_family.clone(), key)
    };
    let steps = h.state().session.active().unwrap().history.entries().len();
    type_family(&mut h, "JetBrains Mono");
    h.get_by_label("JetBrains Mono").hover();
    h.run();
    assert_eq!(font_of(h.state_mut()).0, "JetBrains Mono", "the hovered row previews on the canvas");
    assert_eq!(layer_font(h.state()), "Inter", "the document keeps its font");
    assert_eq!(h.state().session.active().unwrap().history.entries().len(), steps, "hover adds no history entry");

    field(&h).hover();
    h.run();
    let (restored, key) = font_of(h.state_mut());
    assert_eq!(restored, "Inter");
    assert_eq!(key, 0, "leaving the row restores the committed canvas content");

    h.get_by_label("JetBrains Mono").hover();
    h.run();
    h.get_by_label("JetBrains Mono").click();
    h.run();
    assert_eq!(font_of(h.state_mut()), ("JetBrains Mono".to_string(), 0), "a pick replaces the preview with committed content");
    assert_eq!(h.state().session.active().unwrap().history.entries().len(), steps + 1, "a pick adds one history step");
}

#[test]
fn hovering_a_font_previews_only_the_selected_characters() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
    app.run("file.new", json!({"width": 200, "height": 100})).unwrap();
    app.run("type.create", json!({"text": "abcdef", "font": "Inter", "size": 18})).unwrap();
    let layer = app.session.active().unwrap().active_layer.unwrap();
    app.run("type.setStyle", json!({"layer": layer.0, "range": [0, 2], "font": "JetBrains Mono"})).unwrap();
    app.ui.text_edit =
        Some(crate::state::TextEdit { layer: layer.0, document: None, caret: 5, anchor: 3, session: "font-preview-test".into(), created: false, mask: false });

    crate::type_font_preview::set(&mut app, "Inter".into());
    let (preview, _) = crate::type_font_preview::display_doc(&mut app, 0).expect("preview document");
    let fonts = |doc: &photocraft_doc::Document| {
        let photocraft_doc::LayerContent::Text(text) = &doc.layer(layer).unwrap().content else {
            panic!("type layer");
        };
        text.char_runs().into_iter().flat_map(|run| std::iter::repeat_n(run.style.font_family, run.len)).collect::<Vec<_>>()
    };

    assert_eq!(fonts(&preview), ["JetBrains Mono", "JetBrains Mono", "Inter", "Inter", "Inter", "Inter"]);
    assert_eq!(fonts(&app.session.active().unwrap().doc), ["JetBrains Mono", "JetBrains Mono", "Inter", "Inter", "Inter", "Inter"]);
}

<<<<<<< HEAD
/// A star goes to the engine's preferences library; a changed recent list lands in `UiState.recent_fonts`, capped
/// by the preference; the next pass's state starts from what was stored.
=======
/// Preferences ▸ Type ▸ Font Preview Size: Extra Large grows the sample rows, Off drops the
/// samples (rows shrink to the plain height).
#[test]
fn the_preview_size_preference_resizes_or_removes_the_samples() {
    let mut h = harness_with(FontPreview::Medium);
    open(&mut h);
    let medium = h.get_by_label("Family 00").rect().height();
    assert!((medium - 28.0).abs() < 0.5, "medium rows hold the 24 px sample: {medium}");
    let mut h = harness_with(FontPreview::ExtraLarge);
    open(&mut h);
    let extra = h.get_by_label("Family 00").rect().height();
    assert!(extra > medium, "extra large rows are taller: {extra} vs {medium}");
    let mut h = harness_with(FontPreview::Off);
    open(&mut h);
    let off = h.get_by_label("Family 00").rect().height();
    assert!((off - 28.0).abs() < 0.5, "off keeps the plain row height: {off}");
}

/// Preferences ▸ Type ▸ Show Font Names in English off: the menu lists a family by its localized
/// name, finds it by either name, and picking it still applies the English family.
#[test]
fn localized_font_names_label_the_menu_but_pick_the_family() {
    use std::collections::HashMap;
    use std::sync::Arc;
    let state = State {
        current: "Arial".into(),
        families: vec!["Arial".into(), "MS Gothic".into(), "Zeta Sans".into()],
        button: egui::Id::NULL,
        popup: egui::Id::NULL,
        preview: FontPreview::Medium,
        recents: Vec::new(),
    };
    let mut h = Harness::builder().with_size(vec2(600.0, 900.0)).with_max_steps(SETTLE_STEPS).build_ui_state(
        |ui, s: &mut State| {
            s.popup = ui.make_persistent_id(egui::IdSalt::new("type-font")).with("popup");
            let fams = s.families.clone();
            let recents = s.recents.clone();
            font_picker_in(ui, &mut s.current, 170.0, &fams, s.preview, &recents);
        },
        state,
    );
    // The localized menu lists Japanese names, so install the app's bundled theme fonts
    // (including the craft-fonts Japanese faces) before the first pass shapes text.
    crate::theme::install_fonts(&h.ctx);
    // Show Font Names in English on: no localized labels.
    crate::type_tool::sync_font_names(&h.ctx, true);
    let labels: HashMap<String, String> = [("MS Gothic".to_string(), "ＭＳ ゴシック".to_string())].into();
    h.ctx.data_mut(|d| d.insert_temp(super::font_labels_id(), Arc::new(labels.clone())));
    h.run();
    open(&mut h);
    assert!(h.query_by_label("ＭＳ ゴシック").is_some(), "the localized name labels the family");
    assert!(h.query_by_label("MS Gothic").is_none());
    assert!(h.query_by_label("Arial").is_some(), "fonts without a localized name keep theirs");
    h.get_by_role(Role::TextInput).type_text("gothic");
    h.run();
    assert!(h.query_by_label("ＭＳ ゴシック").is_some(), "the English name still finds it");
    assert!(h.query_by_label("Arial").is_none());
    h.get_by_label("ＭＳ ゴシック").click();
    h.run();
    assert_eq!(h.state().current, "MS Gothic", "the document keeps the English family");
    // Turning the preference back on drops the labels.
    crate::type_tool::sync_font_names(&h.ctx, true);
    h.run();
    open(&mut h);
    assert!(h.query_by_label("MS Gothic").is_some());
    assert!(h.query_by_label("ＭＳ ゴシック").is_none());
}

#[test]
fn font_menu_rows_search_both_names_and_sort_by_label() {
    use std::collections::HashMap;
    let families: Vec<String> = ["Arial", "Microsoft YaHei", "MS Gothic"].iter().map(|s| s.to_string()).collect();
    let labels: HashMap<String, String> =
        [("MS Gothic", "ＭＳ ゴシック"), ("Microsoft YaHei", "微软雅黑")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    let rows = |labels: Option<&HashMap<String, String>>, q: &str| -> Vec<(String, String)> {
        super::font_menu_rows(&families, labels, q, &[]).0.into_iter().map(|(f, l)| (f.clone(), l.to_string())).collect()
    };
    let english = rows(None, "");
    assert!(english.iter().all(|(f, l)| f == l));
    let local = rows(Some(&labels), "");
    assert_eq!(local.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(), ["Arial", "微软雅黑", "ＭＳ ゴシック"]);
    assert_eq!(rows(Some(&labels), "yahei"), [("Microsoft YaHei".to_string(), "微软雅黑".to_string())]);
    assert_eq!(rows(Some(&labels), "雅黑"), [("Microsoft YaHei".to_string(), "微软雅黑".to_string())]);
    assert!(rows(Some(&labels), "nothing").is_empty());
}

/// Preferences ▸ Type ▸ Recent Fonts: as in Photoshop, recently used families head the menu in
/// their own group (most recent first, once each) and keep their alphabetical place below it; a
/// query lists each match once, alphabetically.
#[test]
fn recent_fonts_head_the_menu_and_keep_their_place() {
    let families: Vec<String> = ["Arial", "Baskerville", "Courier", "Zeta Sans"].iter().map(|s| s.to_string()).collect();
    let recents: Vec<String> = vec!["Zeta Sans".into(), "Courier".into(), "Gone".into()];
    let rows = |q: &str| -> (Vec<String>, usize) {
        let (rows, n) = super::font_menu_rows(&families, None, q, &recents);
        (rows.into_iter().map(|(f, _)| f.clone()).collect(), n)
    };
    assert_eq!(
        rows(""),
        (vec!["Zeta Sans".into(), "Courier".into(), "Arial".into(), "Baskerville".into(), "Courier".into(), "Zeta Sans".into()], 2),
        "a recent group (no uninstalled family), then the whole alphabetical list"
    );
    assert_eq!(rows("a"), (vec!["Arial".into(), "Baskerville".into(), "Zeta Sans".into()], 0), "a query lists each match once");
}

/// The menu itself draws the recent group first, and the recent families again in their place.
#[test]
fn recent_fonts_appear_first_in_the_menu() {
    let mut h = harness_with_recents(FontPreview::Medium, vec!["Zeta Sans".into(), "Family 05".into()]);
    open(&mut h);
    let tops = |h: &Harness<'static, State>, label: &str| -> Vec<f32> {
        let mut t: Vec<f32> = h.query_all_by_label(label).map(|n| n.rect().top()).collect();
        t.sort_by(f32::total_cmp);
        t
    };
    let (zeta, five, first, six) = (tops(&h, "Zeta Sans"), tops(&h, "Family 05"), tops(&h, "Family 00"), tops(&h, "Family 06"));
    assert_eq!(five.len(), 2, "a recent family is listed twice: {five:?}");
    assert!(zeta[0] < five[0] && five[0] < first[0], "the recent group comes first, in order: {zeta:?} {five:?} {first:?}");
    assert!(first[0] < five[1] && five[1] < six[0], "and keeps its alphabetical place: {five:?} {six:?}");
    assert!(first[0] - five[0] > five[0] - zeta[0], "a divider ends the recent group: {zeta:?} {five:?} {first:?}");
}

/// The open menu's arrows walk on through the list although each step records its family as
/// the most recent one (CodeRabbit on #3559): the recent group keeps its order while the menu
/// is open, and a step from a recent family's lower copy goes on from there.
#[test]
fn open_menu_arrows_walk_on_through_recent_fonts() {
    let mut h = harness_with_recents(FontPreview::Off, vec!["Family 03".into()]);
    open(&mut h);
    let mut seen = vec![h.state().current.clone()];
    for _ in 0..5 {
        h.key_press(Key::ArrowDown);
        h.run();
        let now = h.state().current.clone();
        // The options bar records each applied family as the most recent one.
        h.state_mut().recents.retain(|f| *f != now);
        h.state_mut().recents.insert(0, now.clone());
        h.run();
        seen.push(now);
    }
    assert_eq!(seen, ["Family 00", "Family 01", "Family 02", "Family 03", "Family 04", "Family 05"], "{seen:?}");
    h.key_press(Key::ArrowUp);
    h.run();
    assert_eq!(h.state().current, "Family 04", "↑ steps back");
}

/// Opened on a family that is also a recent one (the usual case: every pick records it), the
/// menu marks its copy in the recent group, as Photoshop's does (#9160), and only that row; the
/// arrows walk on from there through the list as shown (Photoshop: ↓ goes to the next recent
/// family).
#[test]
fn open_menu_on_a_recent_font_marks_its_recent_row() {
    use egui_kittest::kittest::NodeT;
    let mut h = harness_with_recents(FontPreview::Off, vec!["Family 05".into(), "Family 09".into()]);
    h.state_mut().current = "Family 05".into();
    h.run();
    open(&mut h);
    let marked = |h: &Harness<'static, State>, label: &str| -> Vec<f32> {
        h.query_all_by_label(label).filter(|n| n.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)).map(|n| n.rect().top()).collect()
    };
    let tops = |h: &Harness<'static, State>, label: &str| -> Vec<f32> {
        let mut t: Vec<f32> = h.query_all_by_label(label).map(|n| n.rect().top()).collect();
        t.sort_by(f32::total_cmp);
        t
    };
    assert_eq!(marked(&h, "Family 05"), [tops(&h, "Family 05")[0]], "the upper (recent) copy alone is marked");
    h.key_press(Key::ArrowDown);
    h.run();
    assert_eq!(h.state().current, "Family 09", "↓ walks on to the next recent family");
    h.key_press(Key::ArrowUp);
    h.run();
    assert_eq!(h.state().current, "Family 05", "and ↑ back");
}

/// #9160: the menu opens showing the current family, as Photoshop's does: a family deep in the
/// list is scrolled to the top of the view, not left wherever the list happens to start.
#[test]
fn open_menu_shows_the_current_family() {
    let mut h = harness_with_recents(FontPreview::Off, vec!["Zeta Sans".into()]);
    h.state_mut().current = "Family 40".into();
    h.run();
    open(&mut h);
    h.run();
    let popup = popup_rect(&h).expect("the menu is open");
    let row = h.query_all_by_label("Family 40").next().expect("its row").rect();
    assert!(popup.contains_rect(row), "Family 40 is in view: {row:?} in {popup:?}");
}

/// The current family is in view the frame the menu opens, with the app's scroll animation on:
/// the reveal jumps, where an animated scroll left the row outside the popup for a while.
/// The harness turns animations off, so this turns a slow one back on.
#[test]
fn open_menu_shows_the_current_family_without_animating() {
    let mut h = harness_with_recents(FontPreview::Off, Vec::new());
    h.ctx.all_styles_mut(|s| s.scroll_animation = egui::style::ScrollAnimation::duration(10.0));
    h.state_mut().current = "Family 40".into();
    h.run();
    h.query_all_by_role(Role::TextInput).next().unwrap().click();
    h.run_steps(4);
    let popup = popup_rect(&h).expect("the menu is open");
    let row = h.query_all_by_label("Family 40").next().expect("its row").rect();
    assert!(popup.contains_rect(row), "Family 40 is in view: {row:?} in {popup:?}");
}

/// Photoshop: ↑ on the top recent row stays there, without wrapping or changing the font.
#[test]
fn open_menu_arrow_up_on_the_top_recent_row_stays_there() {
    use egui_kittest::kittest::NodeT;
    let mut h = harness_with_recents(FontPreview::Off, vec!["Family 05".into(), "Family 09".into()]);
    h.state_mut().current = "Family 05".into();
    h.run();
    open(&mut h);
    let marked_top = |h: &Harness<'static, State>| -> Vec<f32> {
        h.query_all_by_label("Family 05").filter(|n| n.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)).map(|n| n.rect().top()).collect()
    };
    let before = marked_top(&h);
    assert_eq!(before.len(), 1, "one row marked");
    for _ in 0..2 {
        h.key_press(Key::ArrowUp);
        h.run();
        assert_eq!(h.state().current, "Family 05", "↑ on the top row keeps the font");
        assert_eq!(marked_top(&h), before, "and the mark stays on the top row");
    }
}

/// #9160: when the marked recent row fits below the top of the recent group, the menu opens
/// with the whole group in view, its first row at the top of the list.
#[test]
fn open_menu_shows_the_recent_group_when_the_marked_row_fits_below_it() {
    use egui_kittest::kittest::NodeT;
    let recents: Vec<String> = (10..15).map(|i| format!("Family {i}")).collect();
    let mut h = harness_with_recents(FontPreview::Off, recents);
    h.state_mut().current = "Family 12".into();
    h.run();
    open(&mut h);
    h.run();
    let popup = popup_rect(&h).expect("the menu is open");
    let search = h.get_by_role(Role::TextInput).rect();
    let first = h.query_all_by_label("Family 10").map(|n| n.rect()).min_by(|a, b| a.top().total_cmp(&b.top())).expect("the recent group's first row");
    let row = h.query_all_by_label("Family 12").find(|n| n.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)).expect("its marked row").rect();
    assert!(popup.contains_rect(row) && row.top() >= search.bottom(), "recent #3 is in view: {row:?} in {popup:?}");
    // The class list starts level with the font list, under the preview and filters.
    let top = h.get_by_label("All Classes").rect().top();
    assert!(
        popup.contains_rect(first) && (-1.0..=8.0).contains(&(first.top() - top)),
        "the group's first row sits at the top of the list: {first:?}, list top {top}"
    );
}

/// #9160 with large previews: a recent family further down the default ten recents opens in view,
/// not below the fold because the menu scrolled to the top of the recent group.
#[test]
fn open_menu_shows_a_lower_recent_family_with_large_previews() {
    use egui_kittest::kittest::NodeT;
    for preview in [FontPreview::Huge, FontPreview::ExtraLarge] {
        let recents: Vec<String> = (10..20).map(|i| format!("Family {i}")).collect();
        let mut h = harness_with_recents(preview, recents);
        h.state_mut().current = "Family 17".into();
        h.run();
        open(&mut h);
        h.run();
        let popup = popup_rect(&h).expect("the menu is open");
        let search = h.get_by_role(Role::TextInput).rect();
        let row =
            h.query_all_by_label("Family 17").find(|n| n.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)).expect("its marked row").rect();
        assert!(popup.contains_rect(row) && row.top() >= search.bottom(), "{preview:?}: recent #8 is in view: {row:?} in {popup:?} below {search:?}");
    }
}

/// The open-scroll happens once, when the menu opens. With no row for the current family then
/// (a missing font, a mixed selection), the list the user scrolled does not jump later when one
/// turns up: the font finishes loading, or the selection settles on one family.
#[test]
fn open_menu_without_the_current_family_never_jumps_later() {
    for missing in [true, false] {
        let mut h = harness_with_recents(FontPreview::Off, Vec::new());
        if missing {
            h.state_mut().families.retain(|f| f != "Family 40");
            h.state_mut().current = "Family 40".into();
        } else {
            h.state_mut().current = String::new();
        }
        h.run();
        open(&mut h);
        h.run();
        h.hover_at(h.get_by_label("Family 05").rect().center());
        for _ in 0..3 {
            h.event(egui::Event::MouseWheel {
                source: egui::MouseWheelSource::Unknown,
                unit: egui::MouseWheelUnit::Line,
                delta: vec2(0.0, -3.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            });
            h.run();
        }
        let scrolled = h.get_by_label("Family 00").rect();
        if missing {
            h.state_mut().families.push("Family 40".into());
        } else {
            h.state_mut().current = "Family 40".into();
        }
        h.run_steps(3);
        assert!(popup_rect(&h).is_some(), "the menu stays open");
        assert_eq!(h.get_by_label("Family 00").rect(), scrolled, "missing = {missing}: the list jumped");
    }
}

/// The same through the real options bar, where the pick itself records the recent family.
#[test]
fn options_bar_menu_arrows_walk_on_while_recording_recent_fonts() {
    use crate::{PhotocraftApp, Services};
    use serde_json::json;
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
    app.run("file.new", json!({"width": 200, "height": 100})).unwrap();
    app.run("type.create", json!({"text": "Sample", "font": "Inter", "size": 18})).unwrap();
    let mut h = Harness::builder().with_size(vec2(900.0, 700.0)).with_max_steps(SETTLE_STEPS).build_ui_state(
        |ui, app| {
            if ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                ui.horizontal(|ui| crate::type_tool::options_bar(app, ui));
            }
        },
        app,
    );
    crate::PhotocraftApp::setup_context(&h.ctx, Default::default());
    h.run_steps(4);
    h.query_all_by_role(Role::TextInput).next().unwrap().click();
    h.run();
    let mut seen = vec![h.state().ui.tool_options.type_font.clone()];
    for _ in 0..4 {
        h.key_press(Key::ArrowDown);
        // The open font menu repaints for its caret, so step a bounded number of times.
        h.run_steps(2);
        seen.push(h.state().ui.tool_options.type_font.clone());
    }
    let mut once = seen.clone();
    once.sort();
    once.dedup();
    assert_eq!(once.len(), seen.len(), "each ↓ reaches a new family: {seen:?}");
    assert_eq!(h.state().ui.recent_fonts.first(), seen.last(), "the picks are recorded");
}

#[test]
fn wheel_over_the_closed_font_button_steps_through_the_families() {
    let mut h = harness();
    let button = h.query_all_by_role(Role::TextInput).next().unwrap().rect().center();
    h.hover_at(button);
    h.run();
    let wheel = |h: &mut Harness<'static, State>, y: f32| {
        h.event(egui::Event::MouseWheel {
            source: egui::MouseWheelSource::Unknown,
            unit: egui::MouseWheelUnit::Line,
            delta: vec2(0.0, y),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        h.run();
    };
    wheel(&mut h, -2.0);
    assert_eq!(h.state().current, "Family 02", "two notches down pick the second next family");
    assert!(popup_rect(&h).is_none(), "the list stays closed");
    wheel(&mut h, 1.0);
    assert_eq!(h.state().current, "Family 01");
    wheel(&mut h, 5.0);
    assert_eq!(h.state().current, "Family 00", "stops at the first family");
}

/// The closed font field (on "Family 03" of ten) 100 pt down a scroll area 200 pt tall over a large
/// filler, as in a panel: (the family, the panel's vertical offset).
fn font_field_in_a_panel() -> Harness<'static, (String, f32)> {
    let families: Vec<String> = (0..10).map(|i| format!("Family {i:02}")).collect();
    let mut h = Harness::builder().with_size(vec2(400.0, 300.0)).with_step_dt(1.0 / 30.0).build_ui_state(
        move |ui, s: &mut (String, f32)| {
            crate::field_wheel::begin_frame(ui.ctx());
            let out = egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                ui.add_space(100.0);
                font_picker_in(ui, &mut s.0, 170.0, &families, FontPreview::Medium, &[]);
                ui.allocate_space(vec2(300.0, 1000.0));
            });
            s.1 = out.state.clamped_offset().y;
        },
        ("Family 03".to_string(), 0.0),
    );
    h.run();
    h
}

/// #11353: a trackpad scrolling a panel past the closed font field keeps scrolling the panel and
/// leaves the family alone.
#[test]
fn a_gesture_that_starts_over_the_panel_keeps_scrolling_it_over_the_font_field() {
    let mut h = font_field_in_a_panel();
    let field = |h: &Harness<'static, (String, f32)>| h.query_all_by_role(Role::TextInput).next().unwrap().rect();
    h.hover_at(field(&h).center() + vec2(0.0, 50.0));
    h.run_steps(1);
    let down = || egui::Event::MouseWheel {
        source: egui::MouseWheelSource::Unknown,
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, -12.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    };
    h.event(down());
    h.run_steps(1);
    // Still the same gesture as the pointer reaches the field, a few notches' worth.
    for _ in 0..6 {
        let over = field(&h);
        assert!(over.top() > 0.0, "the field is still in view");
        h.hover_at(over.center());
        h.event(down());
        h.run_steps(1);
    }
    assert_eq!(h.state().0, "Family 03", "the field passing under the pointer is not stepped");
    assert!(h.state().1 > 12.0, "the panel kept scrolling");
}

/// #11353: a trackpad gesture that starts over the font field scrolls the panel and leaves the
/// family alone.
#[test]
fn a_trackpad_gesture_that_starts_over_the_font_field_scrolls_the_panel() {
    let mut h = font_field_in_a_panel();
    let over = h.query_all_by_role(Role::TextInput).next().unwrap().rect().center();
    h.hover_at(over);
    h.run_steps(1);
    for _ in 0..4 {
        h.event(egui::Event::MouseWheel {
            source: egui::MouseWheelSource::Trackpad,
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -12.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
    }
    assert_eq!(h.state().0, "Family 03", "the family is not stepped");
    assert!(h.state().1 > 0.0, "the panel scrolled");
}

/// #11353: a gesture that steps the font field, then wheels on with the pointer off it, is the
/// panel's from then on: coming back over the field in the same gesture doesn't step it again.
#[test]
fn a_gesture_that_wheels_off_the_font_field_does_not_step_it_on_coming_back() {
    let mut h = font_field_in_a_panel();
    let over = h.query_all_by_role(Role::TextInput).next().unwrap().rect().center();
    h.hover_at(over);
    h.run_steps(1);
    // Up, so the panel (already at the top) doesn't move the field.
    let up = || egui::Event::MouseWheel {
        source: egui::MouseWheelSource::Unknown,
        unit: egui::MouseWheelUnit::Line,
        delta: vec2(0.0, 1.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    };
    h.event(up());
    h.run_steps(1);
    assert_eq!(h.state().0, "Family 02", "the start of the gesture steps it");
    h.hover_at(over + vec2(0.0, 50.0));
    h.event(up());
    h.run_steps(1);
    h.hover_at(over);
    h.event(up());
    h.run_steps(4);
    assert_eq!(h.state().0, "Family 02", "the field was stepped once, by the start of the gesture");
}

#[test]
fn wheel_over_the_closed_button_keeps_a_stable_order_with_recents() {
    let mut h = harness_with_recents(FontPreview::Medium, vec![]);
    let button = h.query_all_by_role(Role::TextInput).next().unwrap().rect().center();
    h.hover_at(button);
    h.run();
    let wheel = |h: &mut Harness<'static, State>, y: f32| {
        h.event(egui::Event::MouseWheel {
            source: egui::MouseWheelSource::Unknown,
            unit: egui::MouseWheelUnit::Line,
            delta: vec2(0.0, y),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        h.run();
    };
    wheel(&mut h, -1.0);
    assert_eq!(h.state().current, "Family 01");
    // The caller (the options bar) records the picked family as the most recent one.
    h.state_mut().recents.insert(0, "Family 01".into());
    h.run();
    wheel(&mut h, -1.0);
    assert_eq!(h.state().current, "Family 02", "the next notch keeps going, not back to the recent family");
    h.state_mut().recents.insert(0, "Family 02".into());
    h.run();
    wheel(&mut h, -1.0);
    assert_eq!(h.state().current, "Family 03");
    assert!(popup_rect(&h).is_none(), "the list stays closed");
}

#[test]
fn wheel_family_index_clamps_to_the_ends() {
    assert_eq!(super::wheel_family_index(0, 3, -5.0), Some(2));
    assert_eq!(super::wheel_family_index(1, 3, 1.0), Some(0));
    assert_eq!(super::wheel_family_index(1, 3, 0.0), None);
    assert_eq!(super::wheel_family_index(0, 0, -1.0), None);
}

/// A picker over the test families with a recent/favourite library; the library is kept in the
/// state so a test can read the star that was clicked.
struct LibState {
    current: String,
    families: Vec<String>,
    library: super::FontLibrary,
    popup: egui::Id,
}

fn library_harness(recent: &[&str], favorites: &[&str]) -> Harness<'static, LibState> {
    library_harness_with(recent, favorites, None, None)
}

fn library_harness_with(
    recent: &[&str],
    favorites: &[&str],
    traits: Option<crate::font_filters::TraitsMap>,
    styles: Option<HashMap<String, Vec<String>>>,
) -> Harness<'static, LibState> {
    let mut families: Vec<String> = (0..20).map(|i| format!("Family {i:02}")).collect();
    families.push("Zeta Sans".into());
    let library = super::FontLibrary {
        recent: recent.iter().map(|s| s.to_string()).collect(),
        favorites: favorites.iter().map(|s| s.to_string()).collect(),
        traits: traits.map(std::sync::Arc::new),
        styles: styles.map(std::sync::Arc::new),
        ..Default::default()
    };
    let state = LibState { current: "Family 00".into(), families, library, popup: egui::Id::NULL };
    let mut h = Harness::builder().with_size(vec2(600.0, 900.0)).build_ui_state(
        |ui, s: &mut LibState| {
            let fams = s.families.clone();
            s.popup = ui.make_persistent_id(egui::IdSalt::new("type-font")).with("popup");
            super::family_picker_in(ui, "type-font", &mut s.current, 170.0, &fams, FontPreview::Medium, &mut s.library);
        },
        state,
    );
    h.run();
    h.query_all_by_role(Role::TextInput).next().unwrap().click();
    h.run();
    h
}

/// The tops of every row labelled `label`, top to bottom.
fn row_tops(h: &Harness<'static, LibState>, label: &str) -> Vec<f32> {
    let mut tops: Vec<f32> = h.query_all_by_label(label).map(|n| n.rect().top()).collect();
    tops.sort_by(f32::total_cmp);
    tops
}

#[test]
fn recent_fonts_come_first_and_stay_in_the_alphabetical_list() {
    let h = library_harness(&["Zeta Sans", "Family 05", "Not Installed"], &[]);
    let zeta = row_tops(&h, "Zeta Sans");
    let five = row_tops(&h, "Family 05");
    let first = row_tops(&h, "Family 00");
    assert_eq!(zeta.len(), 2, "Zeta Sans is listed as recent and in its alphabetical place");
    assert_eq!(five.len(), 2);
    assert!(zeta[0] < five[0] && five[0] < first[0], "recent first, most recent on top: {zeta:?} {five:?} {first:?}");
    assert!(h.query_by_label("Not Installed").is_none(), "a recent font that is no longer installed isn't listed");
}

#[test]
fn a_search_hides_the_recent_section() {
    let mut h = library_harness(&["Zeta Sans"], &[]);
    h.get_by_role(Role::TextInput).type_text("zeta");
    h.run();
    assert_eq!(row_tops(&h, "Zeta Sans").len(), 1);
}

#[test]
fn clicking_a_star_marks_the_font_without_picking_it_or_closing_the_menu() {
    let mut h = library_harness(&[], &[]);
    let row = h.get_by_label("Family 03").rect();
    // The star sits left of the style expander's slot, which is left of the row's button.
    let star = egui::pos2(row.left() - 2.0 - 16.0 - 2.0 - 11.0, row.center().y);
    h.hover_at(star);
    h.run();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: star, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.run();
    }
    assert_eq!(h.state().library.toggled.as_deref(), Some("Family 03"));
    assert_eq!(h.state().current, "Family 00", "the star doesn't pick the font");
    assert!(h.query_by_label("Family 10").is_some(), "the menu stays open");
}

#[test]
fn the_favorites_filter_lists_only_starred_fonts() {
    let mut h = library_harness(&["Family 01"], &["Zeta Sans", "Family 07"]);
    assert_eq!(h.query_all_by_label("Remove from Favorites").count(), 2, "starred rows show a filled star");
    h.get_by_label("Show Favorite Fonts").click();
    h.run();
    assert!(h.query_by_label("Zeta Sans").is_some() && h.query_by_label("Family 07").is_some());
    assert!(h.query_by_label("Family 01").is_none(), "neither other fonts nor the recent section show");
    assert!(h.query_by_label("Family 02").is_none());
}

#[test]
fn the_favorites_filter_explains_an_empty_library() {
    let mut h = library_harness(&[], &[]);
    h.get_by_label("Show Favorite Fonts").click();
    h.run();
    assert!(h.query_by_label_contains("No favorite fonts yet").is_some());
}

>>>>>>> upstream/main
#[test]
fn a_clicked_star_is_saved_to_the_preferences_library() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services::default());
    let mut state = crate::type_tool::font_state(&app);
    state.settings.set_favorite("Bebas Neue", true);
    state.settings.note_recent("Arial");
    crate::type_tool::finish_font_state(&mut app, state);
    assert_eq!(app.session.prefs().type_.favorite_fonts, ["Bebas Neue"]);
    assert_eq!(app.ui.recent_fonts, ["Arial"]);
    let next = crate::type_tool::font_state(&app);
    assert_eq!(next.settings.favorites, ["Bebas Neue"]);
    assert_eq!(next.settings.recent, ["Arial"]);

    let mut state = crate::type_tool::font_state(&app);
    state.settings.set_favorite("Bebas Neue", false);
    crate::type_tool::finish_font_state(&mut app, state);
    assert!(app.session.prefs().type_.favorite_fonts.is_empty(), "un-starring removes the favourite");

    app.run("prefs.set", json!({"path": "type.recentFonts", "value": 1})).unwrap();
    let mut state = crate::type_tool::font_state(&app);
    state.settings.note_recent("Zeta");
    crate::type_tool::finish_font_state(&mut app, state);
    assert_eq!(app.ui.recent_fonts.len(), 1, "the recent list is capped at once");
}

#[test]
fn the_preview_sets_the_type_layers_text_or_the_selection_being_typed() {
    let mut app = crate::PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    assert_eq!(super::preview_text(&app), None, "no document");
    app.run("file.new", serde_json::json!({"width": 64, "height": 64})).unwrap();
    let id = app.run("type.create", serde_json::json!({"text": "Hello\rworld  ", "size": 24})).unwrap()["layer"].as_u64().unwrap();
    app.run("layer.select", serde_json::json!({"id": id})).ok();
    app.ui.text_edit = Some(crate::state::TextEdit { layer: id, document: None, caret: 0, anchor: 0, session: "s".into(), created: false, mask: false });
    assert_eq!(super::preview_text(&app).as_deref(), Some("Hello world"), "the whole text, on one line");
    if let Some(ed) = app.ui.text_edit.as_mut() {
        ed.anchor = 6;
        ed.caret = 11;
    }
    assert_eq!(super::preview_text(&app).as_deref(), Some("world"), "only the selection while typing");
    if let Some(ed) = app.ui.text_edit.as_mut() {
        ed.anchor = 50;
        ed.caret = 400;
    }
    assert_eq!(super::preview_text(&app), None, "an out-of-range selection is blank, not a panic");
}

/// #5152: with the Character panel's picker also shown, the open picker previews and the closed one never clears it.
#[test]
fn options_bar_hover_previews_with_the_character_picker_also_shown() {
    use photocraft_doc::LayerContent;

    let theme = crate::theme::AppTheme::pro();
    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).with_max_steps(SETTLE_STEPS).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, theme.clone());
        let mut app = PhotocraftApp::with_context(cc.egui_ctx.clone(), photocraft_engine::Session::new(), Services::default());
        app.ui.theme = theme;
        app.run("file.new", json!({"width": 200, "height": 100})).unwrap();
        app.run("type.create", json!({"text": "Sample", "font": "Inter", "size": 18})).unwrap();
        app.ui.tool = crate::state::Tool::Type;
        app.sync_views();
        app
    });
    h.run_steps(8);
    let layer = h.state().session.active().unwrap().active_layer.unwrap();
    let font = |h: &mut Harness<'static, PhotocraftApp>| {
        let (shown, _) = crate::canvas::display_doc(h.state_mut(), 0);
        let LayerContent::Text(t) = &shown.layer(layer).unwrap().content else { panic!("type layer") };
        t.char_runs()[0].style.font_family.clone()
    };
    assert_eq!(h.query_all_by_label("Font family").count(), 2, "options bar and Character section both show the font picker");
    h.query_all_by_label("Font family").next().unwrap().click();
    h.run();
    h.query_all_by_label("Font family").next().unwrap().type_text("JetBrains Mono");
    h.run();
    h.get_by_label("JetBrains Mono").hover();
    h.run();
    assert_eq!(font(&mut h), "JetBrains Mono");
    h.query_all_by_label("Font family").next().unwrap().hover();
    h.run();
    assert_eq!(font(&mut h), "Inter", "leaving the row restores the committed font");
    let _ = Role::ComboBox;
}
