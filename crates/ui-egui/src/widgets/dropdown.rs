use super::scrub::take_label_scrub_name;
use super::{Rect, Response, Ui};
use craft::ui::controls::{ControlSize, Select};
use craft::ui::theme::{Tokens as LibTokens, TokensOf as _};

/// Whether a dropdown list or any other popup is open: it then owns the keyboard, as an open menu
/// does (a letter picks an option instead of a tool, Enter closes the list).
pub fn dropdown_list_open(ctx: &egui::Context) -> bool {
    craft::ui::popup_list::any_open(ctx)
}

/// A compact labelled dropdown in the studio style.
pub fn dropdown<T: PartialEq + Clone>(ui: &mut Ui, id: &str, name: &str, current: &mut T, options: &[(T, &str)], width: f32) -> bool {
    dropdown_hovered(ui, id, name, current, options, width).0
}

/// [`dropdown`] whose option labels are shown exactly as given: names the user typed (a channel
/// or layer name) must not be translated, unlike the built-in labels [`dropdown`] translates (#5335).
pub fn dropdown_literal<T: PartialEq + Clone>(ui: &mut Ui, id: &str, name: &str, current: &mut T, options: &[(T, &str)], width: f32) -> bool {
    let entries = options.iter().map(|(v, l)| (v.clone(), l.to_string(), None)).collect();
    select_box(ui, id, name, current, entries, width).0
}

/// [`dropdown`], also returning the option under the pointer in its open list (live previews).
pub fn dropdown_hovered<T: PartialEq + Clone>(ui: &mut Ui, id: &str, name: &str, current: &mut T, options: &[(T, &str)], width: f32) -> (bool, Option<T>) {
    select_box(ui, id, name, current, translated(options), width)
}

/// [`dropdown`] whose options and the closed box carry a tooltip.
pub fn dropdown_with_tooltips<T: PartialEq + Clone>(ui: &mut Ui, id: &str, name: &str, current: &mut T, options: &[(T, &str, &str)], width: f32) -> bool {
    let entries = options.iter().map(|(v, l, tip)| (v.clone(), tl!(l).to_string(), Some(tl!(tip).to_string()))).collect();
    let (changed, _) = select_box(ui, id, name, current, entries, width);
    changed
}

fn translated<T: Clone>(options: &[(T, &str)]) -> Vec<(T, String, Option<String>)> {
    options.iter().map(|(v, l)| (v.clone(), tl!(l).to_string(), None)).collect()
}

/// A [`Select`] `width` wide over `entries` (value, text, tooltip). Returns whether `current`
/// changed this frame (live as the keys step the open list) and the option under the pointer.
/// The select chooses an entry's index, so values need no `'static` bound.
fn select_box<T: PartialEq + Clone>(
    ui: &mut Ui,
    id: &str,
    name: &str,
    current: &mut T,
    entries: Vec<(T, String, Option<String>)>,
    width: f32,
) -> (bool, Option<T>) {
    let size = ControlSize::Sm;
    let height = Select::style(&LibTokens::of(ui), size).field.height;
    let at = entries.iter().position(|(v, _, _)| v == current);
    let tip = at.and_then(|i| entries[i].2.clone());
    // `usize::MAX` is none of the options: the box shows the placeholder.
    let (mut index, mut hovered) = (at.unwrap_or(usize::MAX), None);
    let mut response = None;
    ui.allocate_ui_with_layout(egui::vec2(width, height), egui::Layout::left_to_right(egui::Align::Center).with_main_justify(true), |ui| {
        let mut select = Select::new(id, name, &mut index).size(size).placeholder("—").highlighted(&mut hovered);
        for (i, (_, text, tip)) in entries.iter().enumerate() {
            select = select.option(i, text.as_str());
            if let Some(tip) = tip {
                select = select.option_tooltip(tip.as_str());
            }
        }
        response = Some(select.show(ui).response);
    });
    let hovered = hovered.and_then(|i| entries.get(i)).map(|(v, _, _)| v.clone());
    let Some(response) = response else { return (false, hovered) };
    // A scrubby caption drawn just before the dropdown names it; otherwise its own name does.
    let response = match take_label_scrub_name(ui, response.rect) {
        Some(label) => response.labelled_by(label),
        None => response,
    };
    if let Some(tip) = tip {
        response.on_hover_text(tip);
    }
    let changed = Some(index) != at;
    if let Some((v, _, _)) = entries.get(index) {
        *current = v.clone();
    }
    (changed, hovered)
}

/// The body of a right-click menu: as tall as its items up to the part of the window that can be
/// seen (clear of a taskbar a too-tall window runs under, #315), scrolling past that, so long
/// context menus (a layer's, the canvas tools') stay reachable on small windows instead of running
/// off the bottom. egui moves a popup up to keep it in the window, so a menu opened low on the
/// screen first shifts up and only scrolls when it is taller than the window.
pub fn menu_scroll<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    // The popup frame's margin and stroke, and a small gap to the window's edges.
    let frame = ui.spacing().menu_margin.sum().y + 2.0 + 2.0 * MENU_EDGE;
    let room = (crate::work_area::visible_rect(ui.ctx()).height() - frame).max(MENU_MIN_HEIGHT);
    // A popup's Ui is only as tall as the popup was last frame (400 pt on the first), so ask for
    // the whole room: the area still shrinks to its rows when they need less.
    egui::ScrollArea::vertical().max_height(room).min_scrolled_height(room).show(ui, add_contents).inner
}

/// Gap kept between a context menu and the window's edges, and the shortest it gets.
const MENU_EDGE: f32 = 4.0;

const MENU_MIN_HEIGHT: f32 = 120.0;

/// The colour picker popup of a colour swatch: a click on `swatch` toggles it, a click outside
/// closes it. While open, its left edge stays where it first showed (at the swatch, or further
/// left when the window edge needs it), below the swatch or above it as room allows. The picker's
/// width follows its value readouts, and placing it anew every frame moved it under the pointer,
/// flipping it from side to side near the right edge of the window (#534).
pub fn swatch_popup(swatch: &Response) -> egui::Popup<'static> {
    // Room for the readouts to widen after the picker opened.
    const SLACK: f32 = 32.0;
    let popup = egui::Popup::from_toggle_button_response(swatch).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    let ctx = &swatch.ctx;
    let key = popup.get_id().with("left");
    if !popup.is_open() {
        ctx.data_mut(|d| d.remove::<f32>(key));
        return popup;
    }
    crate::press_menu::above_parent(ctx, swatch.layer_id, egui::LayerId::new(egui::Order::Foreground, popup.get_id()));
    // Its width is known from the frame after it opened (egui sizes it unseen first).
    let left = ctx.data(|d| d.get_temp::<f32>(key)).or_else(|| {
        let width = popup.get_expected_size()?.x;
        let screen = ctx.content_rect();
        let left = swatch.rect.left().min(screen.right() - width - SLACK).max(screen.left());
        ctx.data_mut(|d| d.insert_temp(key, left));
        Some(left)
    });
    let Some(left) = left else { return popup };
    popup
        .anchor(Rect::from_x_y_ranges(left..=left, swatch.rect.y_range()))
        .align(egui::RectAlign::BOTTOM_START)
        .align_alternatives(&[egui::RectAlign::TOP_START])
}
