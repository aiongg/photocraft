//! The window's own title bar. The app's top bar (`panels::title_bar`) is a
//! [`craft_ui::title_bar::TitleBar`] for the frame craft-window's policy chose: on Windows and
//! Linux ([`PhotocraftApp::custom_titlebar`]) it is the title bar, with caption buttons, a drag
//! area and resize zones, as in Photoshop on Windows; on macOS
//! ([`PhotocraftApp::integrated_titlebar`]) it leaves room for the traffic lights.
//!
//! What stays PhotoCraft's: the caption buttons' family and layout as the desktop reports them
//! ([`CaptionStyle`], from `Services::caption_style`), their colours from the theme, the bar's
//! height, and the drag area over the bar's gap while a modal dialog blocks the bar.

use craft_ui::title_bar::{CaptionColors, CaptionFamily, CaptionIcons, CaptionLabels, Captions, Frame};
use egui::{Id, Order, PointerButton, Rect, Sense, ViewportCommand};

use crate::PhotocraftApp;
use crate::theme::Tokens;

const MODAL_DRAG_AREA: &str = "titlebar-modal-drag-area";

/// The caption buttons' family and placement, and the icon theme's glyphs for the round
/// families (`None`, or an icon the theme lacks, draws the family's own).
#[derive(Clone, PartialEq, Debug)]
pub struct CaptionStyle {
    pub captions: Captions,
    pub icons: Option<CaptionIcons>,
}

impl Default for CaptionStyle {
    /// Windows' buttons: minimize, maximize and close at the right end.
    fn default() -> Self {
        Self { captions: Captions { family: CaptionFamily::Windows, layout: craft_window::Layout::default() }, icons: None }
    }
}

impl CaptionStyle {
    /// `family` with the buttons a GTK decoration layout names (`gtk-decoration-layout`, GNOME's
    /// `button-layout`, e.g. `appmenu:minimize,maximize,close`), as [`craft_window::Layout::parse`]
    /// reads it.
    #[cfg(test)]
    pub fn from_gtk_layout(family: CaptionFamily, layout: &str) -> Self {
        Self { captions: Captions { family, layout: craft_window::Layout::parse(layout) }, icons: None }
    }
}

/// The caption style the platform reports, or Windows' when it reports none.
pub(crate) fn style(app: &PhotocraftApp, ctx: &egui::Context) -> CaptionStyle {
    app.services.caption_style.as_ref().map(|read| read(ctx)).unwrap_or_default()
}

/// The title bar's height: `base`, or with GNOME's buttons at least libadwaita's header bar
/// (46 pt, plus the bar's 1 pt separator), so the bar is as tall as its neighbours' title bars.
pub fn bar_height(app: &PhotocraftApp, ctx: &egui::Context, base: f32) -> f32 {
    if app.custom_titlebar && style(app, ctx).captions.family == CaptionFamily::Adwaita { base.max(47.0) } else { base }
}

/// The frame the bar is drawn for this frame. macOS measures the traffic lights; in full screen
/// there are none, and the bar is a plain strip.
pub(crate) fn frame(app: &mut PhotocraftApp, ctx: &egui::Context, style: &CaptionStyle) -> Frame {
    if app.custom_titlebar {
        Frame::Integrated(style.captions.clone())
    } else if app.integrated_titlebar {
        app.traffic_lights.update(ctx).map_or(Frame::Native, Frame::Inset)
    } else {
        Frame::Native
    }
}

/// The edge resize zones of the undecorated window when the menu bar, and with it the title bar,
/// is hidden.
pub(crate) fn edge_zones_without_bar(app: &PhotocraftApp, ui: &mut egui::Ui) {
    let style = style(app, ui.ctx());
    craft_ui::title_bar::resize_zones_only(ui, &Frame::Integrated(style.captions), false);
}

/// The caption buttons' colours: a suite theme's from its roles; PhotoCraft's own palettes give
/// Windows' buttons the hover and pressed fills, and the round ones libadwaita's disc, the glyph
/// colour at 15 % under the pointer and 30 % held down.
pub(crate) fn caption_colors(t: &Tokens, family: CaptionFamily) -> CaptionColors {
    if let Some(suite) = t.suite_caption {
        return suite;
    }
    let (hover, pressed) = match family {
        CaptionFamily::Windows => (t.hover, t.pressed),
        CaptionFamily::Adwaita | CaptionFamily::Breeze | CaptionFamily::Generic => (t.icon.gamma_multiply(0.15), t.icon.gamma_multiply(0.30)),
    };
    CaptionColors { glyph: t.icon, glyph_inactive: t.text_dim, hover, pressed, bar: t.chrome }
}

/// The caption buttons' tooltips and accessibility labels.
pub(crate) fn caption_labels() -> CaptionLabels {
    CaptionLabels { minimize: tl!("Minimize").into(), maximize: tl!("Maximize").into(), restore: tl!("Restore").into(), close: tl!("Close").into() }
}

/// The title bar's unused gap, saved by `panels::title_bar` for drags that begin while a modal
/// backdrop is on top of the normal title-bar widgets.
pub(crate) fn modal_drag_area_id() -> Id {
    Id::unique(MODAL_DRAG_AREA)
}

/// Restore window dragging over the title bar's empty gap after a modal has blocked the normal
/// title-bar layer. The hit target stays limited to the same gap, so menus and caption controls
/// remain owned by their widgets.
pub(crate) fn modal_drag_overlay(ctx: &egui::Context) {
    let Some(area) = ctx.data(|d| d.get_temp::<Rect>(modal_drag_area_id())) else { return };
    if !area.is_positive() || egui::Popup::is_any_open(ctx) || !crate::dialogs::titlebar_drag_area_is_clear(ctx, area) {
        return;
    }
    egui::Area::new(Id::unique("titlebar-modal-drag-overlay")).order(Order::Tooltip).fixed_pos(area.min).movable(false).sense(Sense::hover()).show(ctx, |ui| {
        let response = ui.allocate_rect(area, Sense::click_and_drag());
        if response.drag_started_by(PointerButton::Primary) {
            start_os_move(ctx);
        }
        if response.double_clicked() {
            let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
            ctx.send_viewport_cmd(ViewportCommand::Maximized(!max));
        }
    });
}

fn move_handed_off_id() -> Id {
    Id::unique("titlebar-move-handed-off")
}

/// An OS move started at `pass`; `released` is the pass whose input ended the press
/// ([`release_after_os_resize`]).
#[derive(Clone, Copy, Debug)]
struct HandOff {
    pass: u64,
    released: Option<u64>,
}

/// Move the window from this press of the modal drag area. The window manager takes the press of
/// an OS move and keeps its release, so egui kept the drag: every other move failed, and a resize
/// right after one (#5725). [`release_after_os_resize`] ends it.
fn start_os_move(ctx: &egui::Context) {
    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(move_handed_off_id(), HandOff { pass, released: None }));
}

/// The compositor takes the button release of an OS resize or move (KDE Wayland, KWin on X11), so
/// egui kept the press (#1581, #5725). End it on the input after the hand-off, for the bar's own
/// drags and resizes and for the modal drag area's, unless this input brings the release itself.
/// Call from `raw_input_hook`.
pub fn release_after_os_resize(ctx: &egui::Context, raw: &mut egui::RawInput) {
    if let Some(h) = ctx.data(|d| d.get_temp::<HandOff>(move_handed_off_id()))
        && h.released.is_none()
        && h.pass < ctx.cumulative_pass_nr()
    {
        let os_released = raw.events.iter().any(|e| matches!(e, egui::Event::PointerButton { button: PointerButton::Primary, pressed: false, .. }));
        if let Some(pos) = ctx.input(|i| i.pointer.latest_pos()).filter(|_| !os_released && ctx.input(|i| i.pointer.primary_down())) {
            raw.events.push(egui::Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers: ctx.input(|i| i.modifiers) });
        }
        let pass = ctx.cumulative_pass_nr();
        ctx.data_mut(|d| d.insert_temp(move_handed_off_id(), HandOff { released: Some(pass), ..h }));
    }
    // After the modal area's release, which this one then finds and doesn't double.
    craft_ui::title_bar::release_after_os_resize(ctx, raw);
}

/// Windows with the system title bar: the native caption in the variant the app shows. Sent when
/// that changes, and again when the OS appearance changes, since Windows then repaints the
/// caption in the OS's.
pub(crate) fn sync_native_caption_theme(ctx: &egui::Context) {
    let want = if Tokens::get(ctx).dark() { egui::SystemTheme::Dark } else { egui::SystemTheme::Light };
    let key = (want, ctx.input(|i| i.raw.system_theme));
    let id = Id::unique("titlebar-native-caption-theme");
    if ctx.data(|d| d.get_temp::<(egui::SystemTheme, Option<egui::Theme>)>(id)) != Some(key) {
        ctx.data_mut(|d| d.insert_temp(id, key));
        // egui's own sync would send `SystemDefault` at the end of the pass, after this.
        ctx.options_mut(|o| o.sync_window_theme = false);
        ctx.send_viewport_cmd(ViewportCommand::SetTheme(want));
    }
}

#[cfg(test)]
mod golden_tests;
#[cfg(test)]
mod tests;
