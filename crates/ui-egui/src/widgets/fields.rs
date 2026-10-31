use super::scrub::take_scrub_label;
use super::units::{FieldUnits, field_resolution_id, parse_num_in, unit_of_suffix};
use super::{Response, Sense, Tokens, Ui, Vec2, vec2};
use craft::ui::controls::{ControlSize, NumberField};
use craft::ui::edit::EditEvent;
use craft::ui::localized::{LocaleNumbers, Unit};
use craft::ui::menu::MenuEntry;
use craft::ui::theme::{Tokens as LibTokens, TokensOf as _};

/// Number field with a dimmed unit suffix, Photoshop style, `width` wide and as tall as the
/// library's small number field. Drag to scrub. Typed arithmetic (`1920/2`) applies on Enter, Tab
/// or click-away, and a unit after a number converts into the field's unit (`1in` in a px field,
/// at the resolution [`set_field_resolution`](super::set_field_resolution) gave).
pub fn value_field(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, suffix: &str, width: f32) -> Response {
    value_field_in(ui, value, range, suffix, vec2(width, field_height(ui)), FieldOpts::default(), None)
}

/// The height of a [`value_field`].
pub(super) fn field_height(ui: &Ui) -> f32 {
    NumberField::style(&LibTokens::of(ui), ControlSize::Sm).field.height
}

/// A single-line label for a row that also holds fixed-width controls: laid out no wider than
/// `max_w`, cut with '…' when the text (often a longer translation) does not fit, with the full
/// string on hover. Fixed chrome must never let a label overrun its row (#3580), so an *essential*
/// label beside a fixed control should use this rather than a show/hide fit check; a *redundant*
/// label (one whose control already carries the meaning, e.g. an icon row's caption) may be dropped.
pub fn elided_label_sensed(ui: &mut Ui, text: &str, max_w: f32, sense: Sense) -> Response {
    let t = Tokens::get(ui.ctx());
    let font = egui::TextStyle::Body.resolve(ui.style());
    let galley = crate::tab_strip::elided(ui, text, font, t.text_dim, max_w);
    let elided = galley.elided;
    let (rect, resp) = ui.allocate_exact_size(galley.size(), sense);
    ui.painter().galley(rect.min, galley, t.text_dim);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, ui.is_enabled(), text));
    if elided { resp.on_hover_text(text) } else { resp }
}

/// [`elided_label_sensed`] with hover only (a plain, non-interactive label).
pub fn elided_label(ui: &mut Ui, text: &str, max_w: f32) -> Response {
    elided_label_sensed(ui, text, max_w, Sense::hover())
}

/// A [`value_field`] in a box of `width` by `height`, for slim bars such as the status bar (#2823).
pub fn compact_value_field(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, suffix: &str, width: f32, height: f32) -> Response {
    value_field_in(ui, value, range, suffix, vec2(width, height), FieldOpts::default(), None)
}

/// A compact field whose mouse scrub wraps at the native fullscreen viewport's horizontal edges.
pub(crate) fn compact_wrapping_value_field(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, suffix: &str, size: Vec2) -> Response {
    value_field_in(ui, value, range, suffix, size, FieldOpts { wrap_drag: true, ..Default::default() }, None)
}

/// How a [`value_field_in`] shows and reads its number, beyond its suffix.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct FieldOpts {
    /// Decimal places shown (default: one, or two for a small range).
    pub(super) decimals: Option<u32>,
    /// The unit typed units convert into (default: the suffix's, at the document's resolution).
    pub(super) units: Option<FieldUnits>,
    pub(super) wrap_drag: bool,
}

/// A right-click menu of units: its rows and where a pick goes (the library's `unit_menu`).
pub(super) type UnitMenu<'a> = Option<(&'a [MenuEntry<usize>], &'a mut Option<usize>)>;

/// The unit template a suffix stands for: `{n}%`, `{n}°`, `{n} px`.
fn unit_of(suffix: &str) -> Unit {
    match suffix.trim() {
        "" => Unit::default(),
        s @ ("%" | "°") => Unit::new(&format!("{{n}}{s}")),
        s => Unit::new(&format!("{{n}} {s}")),
    }
}

/// A library [`NumberField`] in a box of `size` that reads and steps like PhotoCraft's value
/// fields: arrows and wheel step by 1 (0.01 over a range of 10 or less), a scrubby caption left
/// by [`scrub_label`](super::scrub_label) drags it, and a dialog's Tab reaches it.
pub(super) fn value_field_in(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    size: Vec2,
    opts: FieldOpts,
    menu: UnitMenu,
) -> Response {
    let FieldOpts { decimals, units, wrap_drag } = opts;
    let numbers = (*LocaleNumbers::of(ui)).clone().ungrouped();
    let (lo, hi) = (f64::from(*range.start()), f64::from(*range.end()));
    // Small ranges (gamma 0.01-9.99, 0-1 centres) need two decimals and a finer step.
    let fine = hi - lo <= 10.0;
    let places = decimals.map_or(if fine { 2 } else { 1 }, |d| d as usize);
    let (dpi, ppi) = ui.data(|d| d.get_temp::<(f64, f64)>(field_resolution_id())).unwrap_or((72.0, 72.0));
    let typed_suffix = suffix.trim().to_owned();
    let reader = move |text: &str| {
        // Short events can accumulate: reject before the parser normalizes the text.
        if text.chars().nth(NUMERIC_INPUT_LIMIT).is_some() {
            return None;
        }
        let units = match units {
            Some(u) => crate::numeric_expression::Units { unit: Some(u.unit), suffix: &typed_suffix, dpi: u.dpi, ppi: u.ppi, extent: None },
            None => crate::numeric_expression::Units { unit: unit_of_suffix(&typed_suffix), suffix: &typed_suffix, dpi, ppi, extent: None },
        };
        parse_num_in(text, units)
    };
    let salt = ui.next_auto_id();
    ui.skip_ahead_auto_ids(1);
    // The field is given the value as stored, rounded so that 53 does not come back as 53.000004.
    let shown = (f64::from(*value) * 1e4).round() / 1e4;
    ui.allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center).with_main_justify(true), |ui| {
        let caption = take_scrub_label(ui, ui.max_rect());
        let last = ui.data(|d| d.get_temp::<egui::Id>(salt));
        cap_numeric_events(ui, last);
        let mut field = NumberField::new(salt, "", shown, &numbers)
            .range(lo..=hi)
            .step(if fine { 0.01 } else { 1.0 })
            .decimals(0..=places)
            .commit_decimals(6)
            .unit(unit_of(suffix))
            .or_parse(reader)
            .scrub_wrap(wrap_drag)
            .size(ControlSize::Sm);
        if let Some(caption) = &caption {
            field = field.scrub_by(caption);
        }
        if let Some((entries, picked)) = menu {
            field = field.unit_menu(entries, picked);
        }
        let edit = field.show(ui);
        let mut response = edit.response.clone();
        // Typed text is a change of the field's text, not of the value: only `apply` reports one.
        response.flags.set(egui::response::Flags::CHANGED, false);
        ui.data_mut(|d| d.insert_temp(salt, response.id));
        if apply(ui, response.id.with("gesture-start"), edit.event.as_ref(), value) {
            response.mark_changed();
        }
        if let Some(caption) = caption {
            response = response.labelled_by(caption.id);
        }
        // Tab in a dialog goes from one of these to the next (field_tab.rs).
        if ui.is_enabled() {
            crate::field_tab::register(ui.ctx(), response.id);
        }
        response
    })
    .inner
}

/// The most characters one text, paste or input-method event may carry into a focused number
/// field; longer ones are dropped before the field lays out text (the library's field has no
/// such limit).
pub(super) const NUMERIC_INPUT_LIMIT: usize = 128;

fn cap_numeric_events(ui: &Ui, id: Option<egui::Id>) {
    if !id.is_some_and(|id| ui.is_enabled() && ui.memory(|m| m.has_focus(id))) {
        return;
    }
    ui.input_mut(|input| {
        input.events.retain(|event| {
            let text = match event {
                egui::Event::Text(text) | egui::Event::Paste(text) | egui::Event::Ime(egui::ImeEvent::Commit(text)) => text,
                egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => text,
                _ => return true,
            };
            text.chars().nth(NUMERIC_INPUT_LIMIT).is_none()
        });
    });
}

/// Applies a field's `event` to `value`: updates and one-shot sets apply, a commit applies when it
/// changed something, and a cancel puts back the value the gesture began with (kept in `start`).
/// True when `value` changed.
fn apply(ui: &Ui, start: egui::Id, event: Option<&EditEvent<f64>>, value: &mut f32) -> bool {
    let set = |value: &mut f32, to: f64| {
        let to = to as f32;
        (to.is_finite() && to != *value).then(|| *value = to).is_some()
    };
    match event {
        Some(EditEvent::Begin { value: first, .. }) => {
            ui.data_mut(|d| d.insert_temp(start, *value));
            first.is_some_and(|v| set(value, v))
        }
        Some(EditEvent::Update { value: v, .. } | EditEvent::Set { value: v, .. } | EditEvent::Commit { value: v, changed: true, .. }) => set(value, *v),
        Some(EditEvent::Cancel { .. }) => ui.data(|d| d.get_temp::<f32>(start)).is_some_and(|before| set(value, f64::from(before))),
        _ => false,
    }
}

/// Width of the ▾ that opens a pop-up slider inside a number field's box.
pub const POPUP_ARROW_W: f32 = 14.0;

/// What a number field with a pop-up slider did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PopupFieldResponse {
    /// The value changed (typed, scrubbed, stepped with the arrow keys, or set on the slider).
    pub changed: bool,
    /// While the value is dragged (on the slider, from the ▾, or scrubbing the number): a number
    /// for that drag, the same on every frame of it, for the caller to coalesce the drag's edits
    /// into one history step.
    pub drag: Option<u64>,
}
