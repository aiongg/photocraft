use super::{Color32, Rect, Response, Stroke, StrokeKind, Tokens, Ui, vec2};
use craft::ui::controls::{Button, ButtonVariant, Checkbox};
use craft::ui::dialog::{FooterButton, FooterButtons};
use craft::ui::theme::{Tokens as LibTokens, TokensOf};

/// A check box with a label. The library check box draws the same box in every theme; `toggle`
/// keeps its callers' name for the options that read as switches.
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    checkbox(ui, on, label)
}

/// Primary (filled accent) button, at least `min_width` wide.
pub fn primary_button(ui: &mut Ui, label: &str, min_width: f32) -> Response {
    ui.add(Button::new(tl!(label)).variant(ButtonVariant::Primary).min_width(min_width))
}

/// The width of the widest of `labels` as a button, for a column of equal-width buttons.
pub fn button_width(ui: &Ui, labels: &[&str]) -> f32 {
    labels.iter().map(|l| Button::new(tl!(l)).natural_width(ui)).fold(0.0, f32::max)
}

/// Secondary (outline) button, at least `min_width` wide.
pub fn secondary_button(ui: &mut Ui, label: &str, min_width: f32) -> Response {
    ui.add(Button::new(tl!(label)).min_width(min_width))
}

/// What a dialog button does, which decides where the platform's button order puts it: the
/// library's role and order rule.
pub use craft::ui::dialog::ButtonRole;

/// One button of a [`dialog_buttons`] row.
#[derive(Clone, Copy)]
pub struct DialogButton<'a> {
    pub role: ButtonRole,
    pub label: &'a str,
    pub min_width: f32,
    pub enabled: bool,
    /// Take keyboard focus this frame (see [`DialogButton::focus`]).
    pub focus: bool,
}

impl<'a> DialogButton<'a> {
    pub fn new(role: ButtonRole, label: &'a str, min_width: f32) -> Self {
        Self { role, label, min_width, enabled: true, focus: false }
    }

    pub fn enabled(self, enabled: bool) -> Self {
        Self { enabled, ..self }
    }

    /// Asks for keyboard focus for this button: from there the arrow keys move between the row's
    /// buttons and Enter or Space presses the focused one.
    pub fn focus(self, focus: bool) -> Self {
        Self { focus, ..self }
    }
}

/// A button row at the cursor, in the platform's order (see [`ButtonRole`]), for the windows that
/// are not hosted by `craft::ui::dialog::Dialog` (the workspaces, the modeless tool windows). Inside a right-to-left row it sits at the
/// right edge. The buttons are laid out left to right, so Tab walks them in reading order.
/// Returns the role of the button clicked this frame.
pub fn dialog_buttons(ui: &mut Ui, buttons: &[DialogButton]) -> Option<ButtonRole> {
    let tokens = LibTokens::of(ui);
    let dialog = &tokens.component.dialog;
    let labels: Vec<String> = buttons.iter().map(|b| tl!(b.label).to_owned()).collect();
    // The library row sits against the trailing edge of the room it has, so it gets a `Ui` as wide as the buttons.
    let width: f32 = buttons.iter().zip(&labels).map(|(b, l)| Button::new(l.as_str()).natural_width(ui).max(b.min_width).max(dialog.button_min)).sum::<f32>()
        + dialog.button_gap * buttons.len().saturating_sub(1) as f32
        + 1.0;
    let row: Vec<FooterButton> =
        buttons.iter().zip(&labels).map(|(b, l)| FooterButton::new(b.role, l.as_str()).min_width(b.min_width).enabled(b.enabled)).collect();
    let height = ui.spacing().interact_size.y.max(tokens.component.button.height.get(craft::ui::controls::ControlSize::Md));
    ui.allocate_ui_with_layout(vec2(width, height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        let shown = FooterButtons::new(("dialog-buttons", ui.scope_id()), row).show(ui);
        for (b, r) in buttons.iter().zip(&shown.responses) {
            if b.focus && b.enabled {
                r.request_focus();
            }
        }
        shown.clicked.map(|c| c.role)
    })
    .inner
}

/// Spectrum-style check box (blue when checked) with its label.
pub fn checkbox(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    Checkbox::bool(tl!(label), on).show(ui).response
}

/// The fill of a check box: the accent when `on`, else the field colour. While it's pressed
/// (the mouse is down on it or on its label) Photoshop shows that a release will act: an
/// unchecked box darkens a little and a checked one turns a little less opaque (#4476).
pub fn check_box_fill(on: bool, pressed: bool, t: &Tokens) -> Color32 {
    match (on, pressed) {
        (true, false) => t.accent,
        (true, true) => t.accent.gamma_multiply(0.75),
        (false, false) => t.field,
        (false, true) => t.field.lerp_to_gamma(Color32::BLACK, 0.18),
    }
}

/// Paint a check box in `rect`: [`check_box_fill`] with a white tick when `on`, else an
/// outline that brightens while `hovered` or `pressed`.
pub fn check_box(p: &egui::Painter, rect: Rect, on: bool, hovered: bool, pressed: bool, t: &Tokens) {
    p.rect_filled(rect, craft::ui::theme::scale::Radius::R2.px(), check_box_fill(on, pressed, t));
    if on {
        let a = rect.left_center() + vec2(3.0, 0.5);
        let b = rect.center_bottom() + vec2(-1.0, -3.5);
        let c = rect.right_top() + vec2(-3.0, 3.5);
        let tick = Stroke::new(rect.width() * 0.125, Color32::WHITE);
        p.line_segment([a, b], tick);
        p.line_segment([b, c], tick);
    } else {
        p.rect_stroke(
            rect,
            craft::ui::theme::scale::Radius::R2.px(),
            Stroke::new(1.5, if hovered || pressed { t.text_dim } else { t.text_faint }),
            StrokeKind::Inside,
        );
    }
}
