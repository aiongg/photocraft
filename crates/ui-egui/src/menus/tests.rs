use super::invoke::data_sets_text;
use super::*;

#[test]
fn data_set_files_must_be_valid_utf8() {
    assert_eq!(data_sets_text(b"DataSet,headline\nS,Jos\xc3\xa9\n".to_vec()).unwrap(), "DataSet,headline\nS,José\n");
    assert!(data_sets_text(b"DataSet,headline\nS\xff,Jos\xe9\n".to_vec()).is_err());
}

#[test]
fn showing_grid_restores_hidden_extras() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    let ctx = egui::Context::default();
    app.ui.extras.grid = false;
    app.ui.view.extras = false;

    assert_eq!(invoke(&mut app, &ctx, "view.show.grid", json!({})).unwrap(), json!(true));
    assert_eq!(checked(&app, "view.show.grid"), Some(true));
    assert!(app.ui.view.extras, "showing Grid must make the overlay visible when Extras was hidden");

    assert_eq!(invoke(&mut app, &ctx, "view.show.grid", json!({})).unwrap(), json!(false));
    assert!(!app.ui.extras.grid);
    assert!(app.ui.view.extras);

    app.ui.extras.grid = true;
    app.ui.view.extras = false;
    assert_eq!(invoke(&mut app, &ctx, "view.show.grid", json!({})).unwrap(), json!(false));
    assert!(!app.ui.view.extras, "hiding Grid must leave hidden Extras alone");
}

#[test]
fn sync_with_system_menu_keeps_saved_slots_and_its_checkmark_across_palettes() {
    use photocraft_engine::prefs::{AppearanceMode, DarkTheme, LightTheme};
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    let ctx = egui::Context::default();
    app.services.system_theme = Some(Box::new(|_| Some(egui::Theme::Light)));
    app.run("prefs.set", json!({"values": {"interface.darkTheme": "studio", "interface.lightTheme": "classic"}})).unwrap();
    invoke(&mut app, &ctx, "window.theme.system", json!({})).unwrap();
    assert_eq!(app.session.prefs().interface.appearance_mode, AppearanceMode::Auto);
    assert_eq!(app.session.prefs().interface.dark_theme, DarkTheme::Studio);
    assert_eq!(app.session.prefs().interface.light_theme, LightTheme::Classic);
    assert_eq!(app.ui.theme, crate::theme::ThemeKind::Classic);
    for appearance in [egui::Theme::Dark, egui::Theme::Light] {
        app.services.system_theme = Some(Box::new(move |_| Some(appearance)));
        crate::prefs_ui::tick(&mut app, &ctx);
        let items = menu_items(&app);
        let item = items.iter().find(|i| i.id == "window.theme.system").unwrap();
        assert_eq!(item.label, "Sync with system");
        assert_eq!(item.path, ["Window", "Theme"]);
        assert!(item.enabled);
        assert_eq!(item.checked, Some(true));
        for kind in crate::theme::ThemeKind::ALL {
            assert_eq!(checked(&app, &format!("window.theme.{}", kind.id())), Some(false));
        }
    }
    invoke(&mut app, &ctx, "window.theme.studio", json!({})).unwrap();
    assert_eq!(app.session.prefs().interface.appearance_mode, AppearanceMode::Dark);
    assert_eq!(checked(&app, "window.theme.system"), Some(false));
    assert_eq!(checked(&app, "window.theme.studio"), Some(true));
}

#[test]
fn live_lookup_finds_engine_and_shell_commands() {
    for command in photocraft_engine::commands::command_specs() {
        assert!(is_live(command.id), "{}", command.id);
    }
    for &(id, ..) in UI_COMMANDS {
        assert!(is_live(id), "{id}");
    }
    for id in ["", "missing.command", "💾"] {
        assert!(!is_live(id));
    }
}

#[test]
fn select_menu_contains_every_selection_context_action_and_more() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 32, "height": 32})).unwrap();
    app.run("select.rect", json!({"x": 2, "y": 2, "width": 8, "height": 8})).unwrap();
    let items = menu_items(&app);
    let top: Vec<_> = items.iter().filter(|i| i.path.len() == 1 && i.path.first().is_some_and(|p| p == "Select")).collect();
    // The context menus' Select actions live somewhere under Select (Feather stays in Select >
    // Modify, as in the reference menus). Make Work Path is a Paths panel action.
    let select: Vec<_> = items.iter().filter(|i| i.path.first().is_some_and(|p| p == "Select")).collect();
    let rows = crate::canvas_tool_menu::SELECTION_MENU.iter().chain(crate::canvas_tool_menu::NO_SELECTION_MENU).flatten();
    for &(label, id) in rows.filter(|(_, id)| id.starts_with("select.") && *id != "select.toWorkPath") {
        assert!(select.iter().any(|i| i.id == id), "Select menu missing {label} ({id})");
    }
    assert!(top.iter().any(|i| i.id == "select.all"));
    assert!(top.iter().any(|i| i.id == "select.colorRange"));
    assert!(
        items.iter().any(|i| i.id == "select.modify.feather" && i.path.iter().map(String::as_str).eq(["Select", "Modify"])),
        "keep the Photoshop Modify route"
    );
    assert!(!top.iter().any(|i| i.id == "select.modify.feather"), "no extra top-level Feather");
}

/// Color to Alpha (#1576) has no Photoshop counterpart: it joins Photoshop's own Filter ›
/// Other submenu (not a second "Other") right after Offset, enabled on a pixel layer.
#[test]
fn color_to_alpha_sits_in_filter_other_after_offset() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 32, "height": 32})).unwrap();
    let items = menu_items(&app);
    let at = |id: &str| items.iter().position(|i| i.id == id).unwrap_or_else(|| panic!("{id} missing"));
    let (offset, c2a) = (at("filter.other.offset"), at("filter.other.colorToAlpha"));
    assert_eq!(c2a, offset + 1);
    assert_eq!(items[c2a].path, items[offset].path);
    assert_eq!(items[c2a].label, "Color to Alpha…");
    assert!(items[c2a].enabled, "the Background becomes a layer when the filter runs");
    let others: Vec<_> = items.iter().filter(|i| i.path.first().is_some_and(|p| p == "Filter") && i.path.len() == 2).map(|i| &i.path[1]).collect();
    assert!(!others.iter().any(|p| p.as_str() == "Other"), "no duplicate Other submenu: {others:?}");
}

/// #1447, #1438: a click on Help leaves its search field focused, so typing searches at once,
/// and clicking into the field and typing keeps the menu open. Whole app, real clicks (press
/// and release in separate frames), with the custom title bar (Windows, Linux) and without.
#[test]
fn help_search_takes_typing_and_clicks_without_closing_the_menu() {
    use egui::{Event, Modifiers, PointerButton};
    use egui_kittest::{Harness, kittest::Queryable};
    for (custom, doc, scale) in [(true, false, 1.0), (true, true, 1.25), (false, true, 1.5)] {
        let what = format!("custom title bar {custom}, document {doc}, scale {scale}");
        let mut h =
            Harness::builder().with_size(egui::vec2(1200.0 / scale, 800.0 / scale)).with_pixels_per_point(scale).with_max_steps(64).build_eframe(move |cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, crate::theme::ThemeKind::ALL[0]);
                let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
                app.custom_titlebar = custom;
                if doc {
                    app.run("file.new", json!({"width": 64, "height": 48})).unwrap();
                }
                app
            });
        h.run_steps(4);
        let click = |h: &mut Harness<'_, PhotocraftApp>, at: egui::Pos2| {
            h.event(Event::PointerMoved(at));
            h.run_steps(1);
            h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
            h.run_steps(1);
            h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
            h.run_steps(3);
        };
        let typing = |h: &mut Harness<'_, PhotocraftApp>, text: &str| {
            for c in text.chars() {
                h.event(Event::Text(c.to_string()));
                h.run_steps(1);
            }
            h.run_steps(3);
        };
        let open = |h: &Harness<'_, PhotocraftApp>| h.query_by_label_contains("About PhotoCraft").is_some();
        let field_focused = |h: &Harness<'_, PhotocraftApp>| h.ctx.text_edit_focused();
        let help = h.get_by_label("Help").rect().center();
        click(&mut h, help);
        assert!(open(&h), "Help opened ({what})");
        assert!(field_focused(&h), "the click on Help leaves the search field focused ({what})");
        typing(&mut h, "lev");
        assert!(open(&h), "typing keeps Help open ({what})");
        assert!(h.query_all_by_label_contains("› Levels").next().is_some(), "typing right away searches ({what})");
        let field = h.query_all_by_role(egui::accesskit::Role::SearchInput).find(|n| n.is_focused()).map(|n| n.rect().center());
        let field = field.unwrap_or_else(|| panic!("the search field is in the accessibility tree ({what})"));
        click(&mut h, field);
        assert!(open(&h), "clicking the search field keeps Help open ({what})");
        assert!(field_focused(&h), "and focused ({what})");
        typing(&mut h, "els");
        assert!(open(&h), "typing after the click keeps Help open ({what})");
        assert!(h.query_all_by_label_contains("› Levels").next().is_some(), "and lists the matches ({what})");
    }
}

#[test]
fn window_panel_and_workspace_ids_drive_the_shell() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    let ctx = egui::Context::default();
    assert!(!app.ui.panels.history);
    invoke(&mut app, &ctx, "window.panel.history", Value::Null).unwrap();
    assert!(app.ui.panels.history);
    assert_eq!(checked(&app, "window.panel.history"), Some(true));
    invoke(&mut app, &ctx, "window.workspace.painting", Value::Null).unwrap();
    assert_eq!(app.ui.workspace, "Painting");
    assert!(!app.ui.panels.properties);
    assert_eq!(checked(&app, "window.workspace.painting"), Some(true));
    invoke(&mut app, &ctx, "window.workspace.essentials", Value::Null).unwrap();
    assert!(app.ui.panels.properties);
    // Live in the menu, and no duplicate "Layers" entry from the window.toggle.* commands.
    let items = menu_items(&app);
    assert!(items.iter().any(|i| i.id == "window.panel.layers" && i.enabled));
    assert_eq!(items.iter().filter(|i| i.path == ["Window"] && i.label == "Layers").count(), 1);
    assert!(items.iter().any(|i| i.id == "edit.stroke"));
}

#[test]
fn proof_setup_presets_and_checkmarks() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    let ctx = egui::Context::default();
    app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
    app.sync_views();
    assert_eq!(checked(&app, "view.proofColors"), Some(false));
    invoke(&mut app, &ctx, "view.proofSetup.workingCmyk", Value::Null).unwrap();
    assert_eq!(checked(&app, "view.proofColors"), Some(true));
    invoke(&mut app, &ctx, "view.gamutWarning", json!({"on": true})).unwrap();
    assert_eq!(checked(&app, "view.gamutWarning"), Some(true));
    let doc = app.session.active().unwrap().doc.clone();
    let lut = app.session.color.canvas_lut(&doc, 9).unwrap().unwrap();
    assert_eq!(lut.len(), 9 * 9 * 9 * 4);
    // Pure sRGB green is outside coated CMYK; mid grey is inside.
    let at = |r: usize, g: usize, b: usize| lut[(r + g * 9 + b * 81) * 4 + 3];
    assert_eq!((at(0, 8, 0), at(4, 4, 4)), (255, 0));
    // Custom… opens the generated Proof Setup dialog with a profile choice.
    assert!(invoke(&mut app, &ctx, "view.proofSetup.custom", Value::Null).unwrap()["dialog"].is_u64());
    let spec = photocraft_engine::commands::find("edit.convertToProfile").unwrap();
    let params = crate::filter_dialog::parse_spec(spec.params);
    assert!(matches!(&params[0].kind, crate::filter_dialog::Kind::Choice { options, .. } if options[0] == "srgb" && !options.iter().any(|v| v.contains('/'))));
}

/// The menu model keeps every item, shortcut and state of [`menu_items`]; its bar rows add the
/// tints and tooltips, and match the English names in another UI language.
#[test]
fn menu_model_and_bar_rows_keep_the_items() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
    app.session.execute("prefs.set", json!({"values": {"menus.colors.file.close": "red"}})).unwrap();
    let items = menu_items(&app);
    let model = menu_model(&items);
    let in_model: Vec<&craft_menu::Item> = model.bar.items();
    for it in items.iter().filter(|i| i.label != "---" && i.path.first().is_some_and(|top| TOP_MENUS.contains(&top.as_str()))) {
        let found = in_model.iter().find(|m| m.command.id == it.id).unwrap_or_else(|| panic!("{} is in the model", it.id));
        assert_eq!((found.label.as_str(), found.enabled, found.kind.checked()), (it.label.as_str(), it.enabled, it.checked), "{}", it.id);
        if let Some(sc) = &it.shortcut {
            let chord = found.shortcut.unwrap_or_else(|| panic!("{}: `{sc}` is not a craft-keymap chord", it.id));
            assert_eq!(chord.portable(), sc.parse::<craft_keymap::Chord>().unwrap().portable());
        }
    }
    assert_eq!(model.bar.menus.iter().map(|m| m.title.as_str()).collect::<Vec<_>>(), TOP_MENUS);
    assert!(model.tints.contains_key("file.close"));

    let platform = craft_keymap::Platform::Linux;
    let fr = crate::i18n::Lang::from_code("fr").unwrap();
    let rows = model::ui_menus(&model, fr, platform);
    assert_eq!(rows.len(), TOP_MENUS.len());
    let entries = craft_ui::behavior::PaletteEntry::from_menu(&rows);
    let found = craft_ui::behavior::CommandSearch::new(&entries).query("distort").fuzzy(false).results(15);
    assert!(found.iter().any(|m| m.entry.command().id == "edit.transform.distort"), "the English name stays searchable");
}

/// An idle bar keeps its rows (building them walks every command and layer); an open menu
/// follows the state, and one with input rebuilds.
#[test]
fn an_idle_bar_keeps_its_rows_and_an_open_menu_follows_the_state() {
    use egui_kittest::{Harness, kittest::Queryable};

    let app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    let mut harness = Harness::builder().with_size(egui::vec2(1200.0, 700.0)).build_ui_state(
        |ui, app| {
            menu_bar(app, ui);
        },
        app,
    );
    PhotocraftApp::setup_context(&harness.ctx, crate::theme::ThemeKind::ALL[0]);
    harness.run_steps(3);
    let rows = |app: &PhotocraftApp| app.menu_cache.as_ref().map(|c| c.menus.as_ptr());
    let close = |app: &PhotocraftApp| {
        let file = app.menu_cache.as_ref().and_then(|c| c.menus[0].children())?;
        file.iter().filter_map(|e| e.item()).find(|i| i.command().is_some_and(|c| c.id == "file.close")).map(|i| i.is_enabled())
    };
    let idle = rows(harness.state());
    assert!(idle.is_some() && close(harness.state()) == Some(false));
    // A document opened while nobody looks at the bar: nothing is rebuilt.
    harness.state_mut().run("file.new", json!({"width": 8, "height": 8})).unwrap();
    harness.run_steps(3);
    assert_eq!(rows(harness.state()), idle, "an idle bar is not rebuilt");
    // Opening the menu shows the current state, and the rows then stay put frame after frame.
    harness.get_by_label("File").click();
    harness.run_steps(3);
    assert_eq!(close(harness.state()), Some(true));
    let open = rows(harness.state());
    harness.run_steps(3);
    assert_eq!(rows(harness.state()), open);
    // State that changes under the open menu (a job ending, an agent): the rows follow.
    harness.state_mut().run("file.close", json!({})).unwrap();
    harness.run_steps(2);
    assert_eq!(close(harness.state()), Some(false));
}
