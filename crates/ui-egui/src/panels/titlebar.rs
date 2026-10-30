//! The window's own title bar. The app's top bar (`panels::title_bar`) is a
//! [`craft_ui::title_bar::TitleBar`] for the frame craft-window's policy chose: on Windows and
//! Linux ([`PhotocraftApp::custom_titlebar`]) it is the title bar, with caption buttons, a drag
//! area and resize zones, as in Photoshop on Windows; on macOS
//! ([`PhotocraftApp::integrated_titlebar`]) it leaves room for the traffic lights.
//!
//! What stays PhotoCraft's: the caption buttons' family and layout as the desktop reports them
//! ([`CaptionStyle`], from `Services::caption_style`), and the tooltips' translations.

use craft_ui::title_bar::{CaptionFamily, CaptionIcons, CaptionLabels, Captions, Frame};

use crate::PhotocraftApp;

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
    craft_ui::title_bar::resize_zones_only(ui, &Frame::Integrated(style.captions), false, &[]);
}

/// The caption buttons' tooltips and accessibility labels.
pub(crate) fn caption_labels() -> CaptionLabels {
    CaptionLabels { minimize: tl!("Minimize").into(), maximize: tl!("Maximize").into(), restore: tl!("Restore").into(), close: tl!("Close").into() }
}

/// The compositor takes the button release of an OS resize or move (KDE Wayland, KWin on X11), so
/// egui kept the press (#1581, #5725). End it on the input after the hand-off. Call from
/// `raw_input_hook`.
pub fn release_after_os_resize(ctx: &egui::Context, raw: &mut egui::RawInput) {
    craft_ui::title_bar::release_after_os_resize(ctx, raw);
}

#[cfg(test)]
mod tests;
