//! PhotoCraft's tokens mapped onto the rt panel's: the panel keeps the look of the egui rows it replaces.

use craft_rt_layers::pro_dark_tokens;
use craft_ui_rt::Rgba;
use egui::Color32;

use crate::state::LayerThumbnailSize;
use crate::theme::Tokens;

fn rgba(c: Color32) -> Rgba {
    Rgba(c.to_srgba_unmultiplied())
}

/// The rt tokens for `t`: `craft.pro` dark compact with PhotoCraft's colours, thumbnail side, row height, eye size and
/// body size laid over it.
pub(super) fn rt_tokens(t: &Tokens, size: LayerThumbnailSize, body: f32) -> craft_ui_rt::Tokens {
    let mut r = pro_dark_tokens();
    let c = &mut r.color;
    c.surface.panel = rgba(t.card);
    c.surface.background = rgba(t.dock);
    c.text.normal = rgba(t.text);
    c.text.muted = rgba(t.text_dim);
    c.text.faint = rgba(t.text_faint);
    c.text.on_selected = rgba(t.text);
    c.state.hover = rgba(t.hover);
    c.state.pressed = rgba(t.pressed);
    c.state.selected = rgba(t.row_selected);
    c.line.divider = rgba(t.separator);
    c.line.border = rgba(t.card_border);
    c.line.control_edge = rgba(t.field_border);
    c.accent.solid = rgba(t.accent);
    c.accent.soft = rgba(t.accent_soft);
    // `widgets::checker`'s two greys.
    c.overlay.checker_light = rgba(Color32::from_gray(250));
    c.overlay.checker_dark = rgba(Color32::from_gray(214));

    let (thumb, row) = crate::layers_panel_ui::thumbnail_geometry(size, t.pro);
    // A hidden thumbnail still needs a drawable side; the panel's row is `thumb + gap.stack` or the control row, whichever is taller.
    let thumb = thumb.max(2.0);
    r.size.thumb.md = thumb;
    r.space.gap.stack = (row - thumb).max(0.0);
    r.space.pad.regular = r.space.pad.regular.min((row - r.size.control.md).max(0.0));
    r.size.icon.md = 15.0;
    let body_style = &mut r.typography.body;
    let ratio = if body_style.size > 0.0 { body / body_style.size } else { 1.0 };
    body_style.size = body;
    body_style.line_height *= ratio;
    r
}
