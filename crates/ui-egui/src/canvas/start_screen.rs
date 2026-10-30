use super::PhotocraftApp;
use super::tabs::{open_on_double_click, paint_dots};
use craft_ui::theme::ToEgui;

pub(super) fn start_screen(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = craft_ui::theme::current(ui.ctx());
    let area = ui.available_rect_before_wrap();
    paint_dots(ui, area, app.session.prefs().interface.show_canvas_dots);
    open_on_double_click(app, ui, area);
    // File › Open Recent, newest first (on the web there are no paths to reopen).
    let show_recent = app.session.prefs().interface.show_home_recent_files;
    let recent: Vec<String> =
        if cfg!(target_arch = "wasm32") || !show_recent { Vec::new() } else { app.ui.recent_files.iter().take(HOME_RECENT).cloned().collect() };
    let new_label = crate::shortcuts::command_label(app, "New document…", "file.new");
    let open_label = crate::shortcuts::command_label(app, "Open…", "file.open");
    let hint = start_screen_drop_hint(app).into_owned();
    let mut recent_action = None;
    let out = craft_ui::composites::EmptyState::centred(tl!("PhotoCraft"))
        .brand(tl!("open source"))
        .description(tl!("Create a new document or open an existing file."))
        .action(craft_ui::controls::Button::new(tl!(&new_label)))
        .action(craft_ui::controls::Button::new(tl!(&open_label)))
        .below_actions(|ui| {
            ui.add_space(craft_ui::theme::scale::Space::S24.px());
            ui.horizontal(|ui| {
                ui.add(
                    craft_ui::primitives::Icon::new(craft_ui::icons::Icon::Image)
                        .size(craft_ui::icons::IconSize::Sm)
                        .content(craft_ui::primitives::Content::Faint),
                );
                ui.label(egui::RichText::new(hint).color(t.color.text.faint.to_egui()));
            });
        })
        .footer(|ui| {
            if !recent.is_empty() {
                recent_action = home_recent(ui, &recent);
                ui.add_space(craft_ui::theme::scale::Space::S24.px());
            }
            crate::links::discord_button(app, ui, 190.0);
            ui.add_space(craft_ui::theme::scale::Space::S12.px());
            crate::links::link_row(app, ui);
        })
        .show(ui);
    let new_clicked = out.actions.first().is_some_and(|r| r.clicked());
    let open_clicked = out.actions.get(1).is_some_and(|r| r.clicked());
    if new_clicked {
        let fields = app.new_document_fields();
        app.ui.open_dialog(crate::state::DialogKind::NewDocument, fields);
    }
    if open_clicked {
        let _ = app.open_dialog_file();
    }
    match recent_action {
        Some(RecentAction::Open(path)) => {
            if let Err(e) = app.open_path(&path) {
                app.open_failed(&crate::file_open::display_name(&path), &e);
            }
        }
        Some(RecentAction::Remove(path)) => app.remove_recent(&path),
        Some(RecentAction::Clear) => app.clear_recent(),
        None => {}
    }
}

/// Wayland has no native file drops (winit 0.30, #386), so the hint offers File › Open and paste.
pub(super) fn start_screen_drop_hint(app: &PhotocraftApp) -> std::borrow::Cow<'static, str> {
    if app.services.is_wayland {
        crate::i18n::fmt(tl!("Use File › Open, or paste a copied image with {paste}."), &[("paste", &crate::notices::paste_hint(app))]).into()
    } else {
        tl!("Drop an image or PSD anywhere to open it.").into()
    }
}

/// What the Home screen's recent list asks for.
enum RecentAction {
    Open(String),
    Remove(String),
    Clear,
}

/// Recent files listed on the Home screen.
const HOME_RECENT: usize = 6;

/// Width of the Home screen's recent list.
const HOME_RECENT_WIDTH: f32 = 380.0;

/// The Home screen's "Recent" list: file name, its folder after it, click to open. The hovered
/// row shows a × that takes it off the list, and right-click offers Remove from Recent and Clear
/// Recent Files (#2691).
fn home_recent(ui: &mut egui::Ui, recent: &[String]) -> Option<RecentAction> {
    use craft_ui::controls::{ControlSize, IconButton};
    use craft_ui::icons::Icon;
    use craft_ui::list::ListRow;
    use craft_ui::menu::{ContextMenu, MenuEntry, MenuItem};
    #[derive(Clone, Copy)]
    enum Command {
        Remove,
        Clear,
    }
    let t = craft_ui::theme::current(ui.ctx());
    ui.add_space(craft_ui::theme::scale::Space::S24.px());
    ui.label(egui::RichText::new(tl!("Recent")).color(t.color.text.muted.to_egui()));
    ui.add_space(craft_ui::theme::scale::Space::S4.px());
    let mut action = None;
    ui.allocate_ui(egui::vec2(HOME_RECENT_WIDTH.min(ui.available_width()), 0.0), |ui| {
        for path in recent {
            let name = crate::file_open::display_name(path);
            let folder = std::path::Path::new(path).parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
            let remove_label = crate::i18n::fmt(tl!("Remove {name} from Recent"), &[("name", &name)]);
            let close = ControlSize::Sm;
            let mut removed = false;
            let mut row = ListRow::new(path.as_str(), name.clone()).icon(Icon::File);
            if !folder.is_empty() {
                row = row.detail(folder);
            }
            let out = row
                .trailing_item(22.0, |ui| {
                    // The item is as tall as the row; the × shows while the pointer is on the row.
                    let row = egui::Rect::from_x_y_ranges(ui.clip_rect().x_range(), ui.max_rect().y_range());
                    if ui.rect_contains_pointer(row) {
                        removed = ui.add(IconButton::new(Icon::Close, remove_label).size(close)).on_hover_text(tl!("Remove from Recent")).clicked();
                    }
                })
                .show(ui);
            let resp = out.response.on_hover_text(path).on_hover_cursor(egui::CursorIcon::PointingHand);
            if removed {
                action = Some(RecentAction::Remove(path.clone()));
            } else if resp.clicked() {
                action = Some(RecentAction::Open(path.clone()));
            }
            let entries: Vec<MenuEntry<Command>> =
                vec![MenuItem::new(tl!("Remove from Recent"), Command::Remove).into(), MenuItem::new(tl!("Clear Recent Files"), Command::Clear).into()];
            match ContextMenu::new(&entries).show(ui, &resp).chosen {
                Some(Command::Remove) => action = Some(RecentAction::Remove(path.clone())),
                Some(Command::Clear) => action = Some(RecentAction::Clear),
                None => {}
            }
        }
    });
    action
}
