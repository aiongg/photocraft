use super::{PhotocraftApp, Sense, Tokens, icons, theme, vec2, widgets};

// ----------------------------------------------------------------------------- title bar

/// The app's top bar: brand mark, menus, the document title and the workspace controls, drawn as
/// a [`craft_ui::title_bar::TitleBar`] for the frame craft-window chose. With
/// [`PhotocraftApp::custom_titlebar`] (Windows and most Linux desktops) it is also the window's
/// title bar, as in Photoshop on Windows: the caption buttons take its ends (`titlebar`). On
/// macOS it leaves room for the traffic lights. With the system title bar (`system_title_bar`)
/// the OS draws its own icon and title, so the in-app bar shows menus and workspace controls
/// only: no brand mark, no centred title.
pub fn title_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    sync_library_direction(ui.ctx());
    let system = system_title_bar(app);
    let custom = app.custom_titlebar;
    let style = crate::titlebar::style(app, ui.ctx());
    let frame = crate::titlebar::frame(app, ui.ctx(), &style);
    let labels = crate::titlebar::caption_labels();
    // Preferences › Interface › Show File Name In Title: with it off the bar keeps the app name
    // and the unsaved dot, since the tabs already name the file (#5037).
    let show_name = app.session.prefs().interface.show_file_name_in_title;
    let active = app.session.active();
    let name = active.filter(|_| show_name).map_or_else(|| "PhotoCraft".to_owned(), |d| d.doc.name.clone());
    let dirty = active.is_some_and(|d| d.is_dirty());
    // Under the native frame the library draws no title: the OS does. The web build and
    // embedders have none, so the bar draws it there itself (`paint_title`).
    let own_title = !system && matches!(frame, craft_ui::title_bar::Frame::Native);
    let mut bar = craft_ui::title_bar::TitleBar::new(&frame).labels(&labels).icons(style.icons.as_ref()).separator(true);
    if !system {
        bar = bar.title(craft_ui::title_bar::Title::new(name.clone()).unsaved(dirty).unsaved_label(tl!("Unsaved changes")));
    }
    let mut response = None;
    // The leading and trailing groups run one after the other, each needing the app.
    let shared = std::cell::RefCell::new(&mut *app);
    egui::Panel::top("title_bar").frame(egui::Frame::NONE).show_separator_line(false).show(ui, |ui| {
        response = Some(bar.show_titled(
            ui,
            |ui, slots| {
                let mut end = 0.0;
                centred_on(ui, &slots, crate::direction::row(), |ui| {
                    // Beside caption buttons the contents keep 4 pt clear of them.
                    if custom && slots.leading_caption > 0.0 {
                        ui.add_space(CAPTION_CLEARANCE);
                    }
                    if !system {
                        let t = Tokens::get(ui.ctx());
                        let side = if t.pro { 18.0 } else { 20.0 };
                        let (mark, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                        crate::brand::paint_mark(ui, mark);
                        ui.add_space(craft_ui::theme::scale::Space::S6.px());
                    }
                    // With the macOS menu bar the menus are at the top of the screen instead.
                    // Either way this is the menus' inner edge, facing the title.
                    let app = &mut **shared.borrow_mut();
                    end = if app.services.native_menu.is_some() { craft_ui::title_bar::LeadingEnd::from_ui(ui) } else { crate::menus::menu_bar(app, ui) };
                });
                if own_title {
                    paint_title(ui, &slots, &name, dirty, end);
                }
                end
            },
            |ui, slots| {
                centred_on(ui, &slots, crate::direction::trailing(), |ui| {
                    if custom && slots.trailing_caption > 0.0 {
                        ui.add_space(CAPTION_CLEARANCE);
                    }
                    bar_controls(&mut **shared.borrow_mut(), ui);
                });
            },
        ));
    });
    if response.is_some_and(|r| r.close_requested) {
        let ctx = ui.ctx().clone();
        let _ = crate::menus::invoke(app, &ctx, "file.exit", serde_json::json!({}));
    }
}

/// The library bar mirrors itself from `craft_ui::direction`; PhotoCraft's own direction
/// (`crate::direction`) picks sides and rows itself and keeps egui's style left to right, so only
/// the library's record of the direction is set here, not the style flag `apply` also sets.
// TODO(library): `craft_ui::direction` needs a way to set the direction without the style flag
// (or PhotoCraft adopts it), so this bridge can go.
fn sync_library_direction(ctx: &egui::Context) {
    use craft_ui::direction::{Direction, HasDirection as _};
    let want = if crate::direction::is_rtl() { Direction::Rtl } else { Direction::Ltr };
    if ctx.direction() != want {
        craft_ui::direction::apply(ctx, want);
        ctx.all_styles_mut(|s| s.right_to_left = false);
    }
}

/// `add` in the bar's slot, centred on the height the traffic lights are on (the bar's middle
/// without them), in `layout`.
fn centred_on(ui: &mut egui::Ui, slots: &craft_ui::title_bar::BarSlots, layout: egui::Layout, add: impl FnOnce(&mut egui::Ui)) {
    let mut rect = ui.max_rect();
    rect = rect.translate(vec2(0.0, slots.centre_line - rect.center().y));
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect).layout(layout), add);
}

/// The far-end controls: workspace switcher, search and the appearance toggle.
fn bar_controls(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing.x = 6.0;
    let mut ws = app.ui.workspace.clone();
    let options = crate::workspace_ui::switcher_options(app);
    let opts: Vec<(String, &str)> = options.iter().map(|(k, l)| (k.clone(), l.as_str())).collect();
    // A narrow bar drops what is also in a menu, the theme toggle first (Preferences), then
    // search (Edit › Search), and narrows the switcher (Window › Workspace) before anything runs
    // over.
    let room = ui.available_width();
    let icons_shown: u8 = if room >= WORKSPACE_MIN + 2.0 * ICON_SLOT {
        2
    } else if room >= WORKSPACE_MIN + ICON_SLOT {
        1
    } else {
        0
    };
    let ws_width = (room - f32::from(icons_shown) * ICON_SLOT - 6.0).clamp(WORKSPACE_MIN, 130.0);
    if room >= WORKSPACE_MIN && widgets::dropdown(ui, "workspace", tl!("Workspace"), &mut ws, &opts, ws_width) {
        app.ui.workspace = ws;
        crate::menus::apply_workspace(app);
    }
    if icons_shown >= 1 && icons::button(ui, "search", 28.0, app.ui.palette_open, &crate::shortcuts::tip_label(app, "Search commands", "edit.search")).clicked()
    {
        app.ui.palette_open = !app.ui.palette_open;
    }
    let (theme_icon, appearance_label) = match app.session.prefs().interface.appearance_mode {
        photocraft_engine::prefs::AppearanceMode::Auto => ("monitor", tl!("Auto")),
        photocraft_engine::prefs::AppearanceMode::Dark => ("moon", tl!("Dark")),
        photocraft_engine::prefs::AppearanceMode::Light => ("sun", tl!("Light")),
    };
    let appearance_tip = format!("{}: {}", tl!("Appearance Mode"), appearance_label);
    if icons_shown >= 2 {
        let button = icons::button(ui, theme_icon, 28.0, false, &appearance_tip);
        if button.clicked() {
            crate::prefs_ui::cycle_appearance(app, ui.ctx());
        }
        button.context_menu(|ui| crate::prefs_ui::appearance_context_menu(app, ui));
    }
}

/// The centred title where nothing else draws one (the web build, embedders): in the free gap
/// the library measured between the menus (ending at `menus_end`) and the controls.
fn paint_title(ui: &egui::Ui, slots: &craft_ui::title_bar::BarSlots, name: &str, dirty: bool, menus_end: f32) {
    let t = Tokens::get(ui.ctx());
    let title = format!("{name}{}", if dirty { "  \u{2022}" } else { "" });
    let rtl = crate::direction::is_rtl();
    let gap = slots.gap;
    let (free_l, free_r) = if gap.is_positive() {
        (gap.left(), gap.right())
    } else if rtl {
        (slots.content.left(), menus_end)
    } else {
        (menus_end, slots.content.right())
    };
    let font = theme::medium(13.0);
    let galley = ui.painter().layout_job(title_job(&title, font.clone(), t.text_dim));
    let centre_y = slots.centre_line;
    if let Some(x) = title_x(slots.content.center().x, free_l, free_r, galley.size().x) {
        ui.painter().galley(egui::pos2(x, centre_y - galley.size().y / 2.0), galley, t.text_dim);
    } else if free_r - free_l > 80.0 {
        // Too narrow for the whole name: the start of it, elided, in the free gap next to the menus.
        let avail = free_r - free_l - 2.0 * TITLE_GAP;
        let mut job = title_job(&title, font, t.text_dim);
        job.wrap = egui::text::TextWrapping::truncate_at_width(avail);
        let g = ui.painter().layout_job(job);
        let x = if rtl { free_r - TITLE_GAP - g.size().x } else { free_l + TITLE_GAP };
        ui.painter().galley(egui::pos2(x, centre_y - g.size().y / 2.0), g, t.text_dim);
    }
}

/// Whether the OS draws the window's title bar and title, so the app draws neither: the shell
/// decided on the native frame ([`PhotocraftApp::native_frame`]). Never on the web build, whose
/// browser tab has no document title.
pub fn system_title_bar(app: &PhotocraftApp) -> bool {
    app.native_frame && !cfg!(target_arch = "wasm32")
}

/// The OS window title: the active document's name suffixed with the app name (a `*` prefix
/// marks unsaved changes, like the `•` in the app-drawn title), or just the app name with no
/// document open. With Preferences › Interface › Show File Name In Title off, the file name
/// drops out and only the app name (and the `*`) remains (#5037).
pub fn window_title(app: &PhotocraftApp) -> String {
    if !app.session.prefs().interface.show_file_name_in_title {
        return format!("{}PhotoCraft", if app.session.active().is_some_and(|d| d.is_dirty()) { "*" } else { "" });
    }
    app.session.active().map(|d| format!("{}{} \u{2014} PhotoCraft", if d.is_dirty() { "*" } else { "" }, d.doc.name)).unwrap_or_else(|| "PhotoCraft".into())
}

/// Keep the OS window title (and the taskbar / Alt-Tab entry) on the active file. Sends
/// `ViewportCommand::Title` only when the title changed since the last frame.
pub fn sync_window_title(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let want = window_title(app);
    if app.last_window_title != want {
        app.last_window_title = want.clone();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(want));
    }
}

/// The in-app title line. It reads left to right as a whole, so a right-to-left document name keeps
/// "— PhotoCraft" after it, at the right; egui orders the name's own words.
pub(crate) fn title_job(title: &str, font: egui::FontId, color: egui::Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::simple_singleline(title.to_owned(), font, color);
    job.direction = egui::epaint::text::TextDirection::LeftToRight;
    job
}

/// Room kept between the caption buttons and the bar's contents.
const CAPTION_CLEARANCE: f32 = 4.0;

/// Minimum space kept between the window title and the menus or controls beside it.
const TITLE_GAP: f32 = 16.0;

/// The workspace switcher's narrowest width, and one 28 pt title-bar icon with its spacing.
const WORKSPACE_MIN: f32 = 90.0;

const ICON_SLOT: f32 = 34.0;

/// Left edge for a title `width` wide: centred on `center` when it fits in the free gap from
/// `left` to `right` (the menus' and the controls' facing edges, whichever side each is on),
/// else slid into that gap; None when the gap is too small for the whole title.
pub(super) fn title_x(center: f32, left: f32, right: f32, width: f32) -> Option<f32> {
    let (lo, hi) = (left + TITLE_GAP, right - TITLE_GAP - width);
    if !(lo.is_finite() && hi.is_finite() && width.is_finite()) || hi < lo {
        return None;
    }
    if !center.is_finite() {
        return Some(lo);
    }
    Some((center - width / 2.0).clamp(lo, hi))
}
