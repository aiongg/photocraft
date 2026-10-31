use super::{Color32, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, Tokens, Ui, pos2, theme, vec2};
use craft::ui::theme::StrokeExt;

/// Match the Spectrum application-menu padding and blue/white hover state in
/// secondary menus (status bar, dock hamburger, layer context menus; #2187).
/// The setting is local to this popup's Ui and leaves other themes unchanged.
pub fn style_spectrum_popup_menu(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    // The 6 pt gap between rows was outside both buttons, so a drag across a menu
    // dropped the highlight (#4602). Padding stays inside the row, which is the hit target.
    ui.spacing_mut().item_spacing.y = 0.0;
    if !t.pro {
        return;
    }
    ui.spacing_mut().button_padding = vec2(10.0, 4.0);
    let v = &mut ui.style_mut().visuals;
    v.widgets.hovered.weak_bg_fill = t.accent;
    v.widgets.hovered.bg_fill = t.accent;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.inactive.corner_radius = CornerRadius::ZERO;
    v.widgets.hovered.corner_radius = CornerRadius::ZERO;
    v.widgets.active.corner_radius = CornerRadius::ZERO;
}

/// An accent insertion line on one edge of `r` while a drag hovers it (vertical: on its left or,
/// `after`, right edge; else on its top or bottom).
pub fn drop_line(ui: &Ui, r: Rect, after: bool, vertical: bool, t: &Tokens) {
    let s = craft::ui::theme::scale::Stroke::W2.egui(ui.ctx(), t.accent);
    if vertical {
        let x = if after { r.right() } else { r.left() };
        ui.painter().line_segment([pos2(x, r.top() + 2.0), pos2(x, r.bottom() - 2.0)], s);
    } else {
        let y = if after { r.bottom() } else { r.top() };
        ui.painter().line_segment([pos2(r.left() + 4.0, y), pos2(r.right() - 4.0, y)], s);
    }
}

/// Draw a bevelled box (Classic theme) or a flat rounded box.
pub fn surface(ui: &Ui, rect: Rect, fill: Color32, raised: bool) {
    let t = Tokens::get(ui.ctx());
    let p = ui.painter();
    p.rect_filled(rect, t.radius_sm, fill);
    if t.bevel {
        let (hi, lo) = if raised { (Color32::WHITE, Color32::from_gray(64)) } else { (Color32::from_gray(64), Color32::WHITE) };
        p.line_segment([rect.left_bottom(), rect.left_top()], craft::ui::theme::scale::Stroke::W1.egui(ui.ctx(), hi));
        p.line_segment([rect.left_top(), rect.right_top()], craft::ui::theme::scale::Stroke::W1.egui(ui.ctx(), hi));
        p.line_segment([rect.right_top(), rect.right_bottom()], craft::ui::theme::scale::Stroke::W1.egui(ui.ctx(), lo));
        p.line_segment([rect.right_bottom(), rect.left_bottom()], craft::ui::theme::scale::Stroke::W1.egui(ui.ctx(), lo));
    }
}

/// Inner margin of a dock panel's body: a Pro panel's, or a Studio card's below its tab strip.
pub(crate) fn body_margin(pro: bool) -> egui::Margin {
    if pro { egui::Margin::same(8) } else { egui::Margin { left: 8, right: 8, top: 6, bottom: 10 } }
}

/// The panel menu button: the hamburger of the docks' tab strips, on a card's header.
pub fn panel_menu_button(ui: &mut Ui) -> Response {
    ui.add(craft::ui::controls::IconButton::new(craft::ui::icons::Icon::Menu, tl!("Panel menu")).size(craft::ui::controls::ControlSize::Sm))
}

pub fn pill_tab(ui: &mut Ui, label: &str, selected: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let translated = tl!(label);
    let font = theme::medium(12.5);
    let galley = ui.painter().layout_no_wrap(translated.to_string(), font, t.text);
    let size = vec2(galley.size().x + 20.0, 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if selected {
        surface(ui, rect, t.hover, true);
        if !t.bevel {
            ui.painter().rect_stroke(rect, t.radius_sm, craft::ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.field_border), StrokeKind::Inside);
        }
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, t.radius_sm, t.hover.gamma_multiply(0.6));
    }
    let color = if selected { t.text } else { t.text_dim };
    ui.painter().galley_with_override_text_color(rect.center() - galley.size() / 2.0, galley, color);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, translated));
    resp
}

/// Hairline separator.
pub fn hairline(ui: &mut Ui) {
    craft::ui::primitives::Divider::horizontal().show(ui);
}

/// Vertical hairline for horizontal layouts.
pub fn vline(ui: &mut Ui, height: f32) {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(9.0, height), Sense::hover());
    ui.painter().line_segment([r.center_top(), r.center_bottom()], craft::ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.separator));
}

/// Small chevron for dropdowns (replaces egui's filled triangle).
pub fn chevron_icon(ui: &Ui, rect: Rect, visuals: &egui::style::WidgetVisuals, _open: bool) {
    let c = rect.center();
    let s = 3.2;
    let stroke = Stroke::new(1.3, visuals.fg_stroke.color.gamma_multiply(0.8));
    ui.painter().line_segment([c + vec2(-s, -s * 0.5), c + vec2(0.0, s * 0.5)], stroke);
    ui.painter().line_segment([c + vec2(0.0, s * 0.5), c + vec2(s, -s * 0.5)], stroke);
}
