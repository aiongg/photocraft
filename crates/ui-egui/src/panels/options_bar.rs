use super::brush_controls::{brush_preset_chip, mixer_load_swatch, paint_mode_dropdown, smoothing_options, tool_mode_dropdown};
use super::{BlendMode, Color32, PhotocraftApp, RichText, Sense, Tokens, Tool, Value, icons, json, widgets};
use craft_ui::controls::{Button, ButtonVariant, Checkbox, ControlSize, IconButton, NumberField, Select};
use craft_ui::edit::EditEvent;
use craft_ui::icons::Icon;
use craft_ui::localized::{LocaleNumbers, Unit};
use craft_ui::menu::{ContextMenu, MenuEntry, MenuItem};
use craft_ui::patterns::{CellWidth, OptionsBar};
use egui::Response;
use photocraft_paint::BrushSettings;
use std::hash::{Hash, Hasher};
use std::ops::RangeInclusive;

// Custom controls also need to bring keyboard focus into a scroll viewport. The toolbar rail
// still draws its own scroll body.
pub(super) fn reveal_focused_control(ui: &egui::Ui) {
    // read_response reprocesses pointer events. Reading a popup's newly focused field
    // during its opening click can undo the focus that the popup just requested.
    if ui.input(|input| input.pointer.any_pressed() || input.pointer.any_released()) {
        return;
    }
    let focused = ui.memory(|memory| memory.focused());
    let key = ui.scope_id().with("scroll-focused-control");
    let changed = ui.data_mut(|data| {
        let previous = data.get_temp::<Option<egui::Id>>(key).flatten();
        data.insert_temp(key, focused);
        previous != focused
    });
    if changed
        && let Some(response) = focused.and_then(|id| ui.ctx().read_response(id))
        && response.layer_id == ui.layer_id()
        && ui.min_rect().contains_rect(response.rect)
    {
        ui.scroll_to_rect(response.rect, Some(egui::Align::Center));
    }
}

// ----------------------------------------------------------------------------- cells

/// What a number cell reported in a frame.
pub(crate) struct CellEdit {
    /// The value was set by a typed, scrubbed, stepped or cancelled edit.
    pub changed: bool,
    /// The edit gesture's number while one runs: edits of one gesture share it, so a host can
    /// coalesce them into one undo step.
    pub gesture: Option<u64>,
}

/// A number cell: the caption, its width class and the number's limits.
pub(crate) struct Field<'a> {
    /// The caption's English text without its colon; it is also the field's accessible name.
    pub label: &'a str,
    pub range: RangeInclusive<f64>,
    /// "", "%", "px" or "°".
    pub suffix: &'a str,
    pub width: CellWidth,
    pub decimals: RangeInclusive<usize>,
    pub tooltip: Option<&'a str>,
    /// How many field units one stored unit is: 100 for a fraction shown as a percentage.
    pub scale: f64,
}

impl<'a> Field<'a> {
    pub(crate) fn new(label: &'a str, range: RangeInclusive<f32>, suffix: &'a str, width: CellWidth) -> Self {
        Self { label, range: f64::from(*range.start())..=f64::from(*range.end()), suffix, width, decimals: 0..=1, tooltip: None, scale: 1.0 }
    }

    pub(crate) fn decimals(mut self, decimals: RangeInclusive<usize>) -> Self {
        self.decimals = decimals;
        self
    }

    /// The stored value is a fraction shown as a percentage.
    pub(crate) fn percent(mut self) -> Self {
        self.scale = 100.0;
        self
    }

    pub(crate) fn tooltip(mut self, tooltip: &'a str) -> Self {
        self.tooltip = Some(tooltip);
        self
    }
}

/// The caption of a cell: the translated label with Photoshop's colon.
fn caption(label: &str) -> String {
    let text = if label.ends_with(':') { label.to_string() } else { format!("{label}:") };
    tl!(&text).to_string()
}

/// The unit template a suffix stands for: "{n}%", "{n}°", "{n} px".
fn unit_of(suffix: &str) -> Option<Unit> {
    match suffix {
        "" => None,
        "%" | "°" => Some(Unit::new(&format!("{{n}}{suffix}"))),
        _ => Some(Unit::new(&format!("{{n}} {suffix}"))),
    }
}

/// Reads typed arithmetic (`1920/2`) and, in a length or percentage field, a number with a unit
/// (`1in`), as PhotoCraft's fields do.
pub(super) fn reader(suffix: &str, units: Option<widgets::FieldUnits>) -> impl Fn(&str) -> Option<f64> + 'static {
    let unit = units.map(|u| u.unit).or(match suffix {
        "%" => Some(photocraft_engine::prefs::Unit::Percent),
        "px" => Some(photocraft_engine::prefs::Unit::Pixels),
        _ => None,
    });
    let (dpi, ppi) = units.map_or((72.0, 72.0), |u| (u.dpi, u.ppi));
    let suffix = suffix.to_string();
    move |text| {
        crate::numeric_expression::parse_with_units(
            &text.split_whitespace().collect::<String>(),
            Some(crate::numeric_expression::Units { unit, suffix: &suffix, dpi, ppi, extent: None }),
        )
    }
}

/// The host's value as the field is given it. The field compares it with what it sent, so a stored
/// f32 that left the field as 53 must come back as 53, not 53.000004.
fn shown(value: f32, scale: f64) -> f64 {
    (f64::from(value) * scale * 1e4).round() / 1e4
}

/// Applies a field's edit to `value`. A cancelled gesture puts back the value it began with.
fn apply(ui: &egui::Ui, id: egui::Id, scale: f64, event: Option<&EditEvent<f64>>, value: &mut f32) -> (bool, Option<u64>) {
    let start = id.with("gesture-start");
    let gesture = event.and_then(EditEvent::id).map(|g| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        g.hash(&mut hasher);
        hasher.finish()
    });
    let changed = match event {
        Some(EditEvent::Begin { .. }) => {
            ui.data_mut(|d| d.insert_temp(start, *value));
            false
        }
        Some(EditEvent::Update { value: v, .. } | EditEvent::Set { value: v, .. }) => {
            *value = (*v / scale) as f32;
            true
        }
        Some(EditEvent::Commit { value: v, changed, .. }) => {
            if *changed {
                *value = (*v / scale) as f32;
            }
            *changed
        }
        Some(EditEvent::Cancel { .. }) => {
            if let Some(before) = ui.data(|d| d.get_temp::<f32>(start)) {
                *value = before;
            }
            true
        }
        _ => false,
    };
    (changed, gesture)
}

/// A caption and the number field it scrubs, in a cell of the field's width class.
pub(crate) fn number_cell(ui: &mut egui::Ui, field: &Field<'_>, value: &mut f32) -> CellEdit {
    number_cell_with(ui, field, value, None, None, None)
}

/// [`number_cell`] whose typed units convert at `units` and whose secondary click lists `menu`.
fn number_cell_with(
    ui: &mut egui::Ui,
    field: &Field<'_>,
    value: &mut f32,
    units: Option<widgets::FieldUnits>,
    template: Option<Unit>,
    menu: Option<(&[MenuEntry<usize>], &mut Option<usize>)>,
) -> CellEdit {
    let numbers = LocaleNumbers::of(ui);
    let salt = format!("options-{}", field.label);
    let name = tl!(field.label);
    let mut out = None;
    OptionsBar::cell(ui, &caption(field.label), field.width, |ui, caption| {
        let mut number = NumberField::new(salt.as_str(), name, shown(*value, field.scale), &numbers)
            .range(field.range.clone())
            .decimals(field.decimals.clone())
            .or_parse(reader(field.suffix, units))
            .scrub_by(caption)
            .size(ControlSize::Sm);
        if let Some(unit) = template.or_else(|| unit_of(field.suffix)) {
            number = number.unit(unit);
        }
        if let Some((entries, picked)) = menu {
            number = number.unit_menu(entries, picked);
        }
        let edit = number.show(ui);
        let (changed, gesture) = apply(ui, edit.response.id, field.scale, edit.event.as_ref(), value);
        if let Some(tip) = field.tooltip {
            edit.response.on_hover_text(tip);
        }
        out = Some(CellEdit { changed, gesture });
    });
    out.expect("the cell draws its content")
}

/// A percent cell for a stored fraction (0..=1) shown as a percentage.
fn percent_cell(ui: &mut egui::Ui, label: &str, fraction: &mut f32, range: RangeInclusive<f32>) -> CellEdit {
    number_cell(ui, &Field::new(label, range, "%", CellWidth::Md).percent(), fraction)
}

/// The Smoothing percentage of a brush (the live stroke and the commit use it).
pub(super) fn smoothing_cell(ui: &mut egui::Ui, b: &mut BrushSettings) {
    let mut smoothing = (b.smoothing.amount * 100.0).round();
    if number_cell(ui, &Field::new("Smoothing", 0.0..=100.0, "%", CellWidth::Md), &mut smoothing).changed {
        b.smoothing.enabled = true;
        b.smoothing.amount = (smoothing / 100.0).clamp(0.0, 1.0);
    }
}

/// A caption and a select over `options`, whose labels are translated; the wheel over it steps the
/// value when `wheel` is set. Returns whether this frame picked an option.
pub(crate) fn select_cell<T: PartialEq + Clone + Send + Sync + 'static>(
    ui: &mut egui::Ui,
    caption_label: Option<&str>,
    id: &str,
    name: &str,
    value: &mut T,
    options: &[(T, &str)],
    wheel: bool,
) -> bool {
    let mut show = |ui: &mut egui::Ui| {
        let mut select = Select::new(id, tl!(name), value).size(ControlSize::Sm).fit(true).wheel_steps(wheel);
        for (v, label) in options {
            select = select.option(v.clone(), tl!(label));
        }
        select.show(ui).response.changed()
    };
    match caption_label {
        Some(label) => OptionsBar::cell(ui, &caption(label), CellWidth::Content, |ui, _| show(ui)),
        None => show(ui),
    }
}

/// A checkbox in the bar.
fn check(ui: &mut egui::Ui, label: &str, value: &mut bool) -> Response {
    Checkbox::bool(tl!(label), value).size(ControlSize::Sm).show(ui).response
}

/// A secondary button in the bar.
pub(super) fn bar_button(ui: &mut egui::Ui, label: &str) -> Response {
    ui.add(Button::new(tl!(label)).variant(ButtonVariant::Secondary).size(ControlSize::Sm))
}

/// An icon toggle in the bar: selected while `on`.
fn icon_toggle(ui: &mut egui::Ui, icon: Icon, tip: &str, on: bool) -> Response {
    ui.add(IconButton::new(icon, tip).size(ControlSize::Md).selected(on))
}

// ----------------------------------------------------------------------------- options bar

/// Marks, in the frame's data, that the Crop strip drew: only then does the trailing slot show
/// the crop's commit and cancel.
fn crop_flag() -> egui::Id {
    egui::Id::NULL.with("options-bar-crop-strip")
}

pub fn options_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let narrow = app.session.prefs().workspace.narrow_options_bar();
    let crop = app.ui.tool == Tool::Crop;
    ui.data_mut(|d| d.insert_temp(crop_flag(), false));
    let app = std::cell::RefCell::new(app);
    OptionsBar::new("tool-options").narrow(narrow).label(tl!("Tool options")).show(ui, |bar| {
        bar.leading(|ui| leading(&mut app.borrow_mut(), ui));
        bar.body(|ui| body(&mut app.borrow_mut(), ui));
        if crop {
            bar.trailing(|ui| {
                if ui.data(|d| d.get_temp::<bool>(crop_flag())).unwrap_or(false) {
                    let done = OptionsBar::commit_cancel(ui, tl!("Commit current crop operation"), tl!("Cancel current crop operation"));
                    let mut app = app.borrow_mut();
                    if done.commit {
                        crate::canvas::commit_crop(&mut app);
                    }
                    if done.cancel {
                        crate::crop_ui::cancel(&mut app);
                    }
                }
            });
        }
    });
}

/// Home and the tool icon (right-click: Reset Tool, Reset All Tools).
fn leading(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    crate::chrome_ui::home_button(app, ui);
    let tool = app.ui.tool;
    let icon = OptionsBar::tool_icon(ui, icons::tool_library_icon(tool), tl!(tool.label()));
    // Photoshop: right-click the tool icon for Reset Tool / Reset All Tools (#2825).
    let entries: Vec<MenuEntry<&'static str>> =
        vec![MenuItem::new(tl!("Reset Tool"), "tool.reset").into(), MenuItem::new(tl!("Reset All Tools"), "tool.resetAll").into()];
    if let Some(id) = ContextMenu::new(&entries).show(ui, &icon).chosen
        && let Err(e) = crate::menus::invoke(app, ui.ctx(), id, json!({}))
    {
        app.ui.status = e;
    }
}

fn body(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if app.ui.transform.is_some() {
        crate::transform_tool::options_bar(app, ui);
        return;
    }
    if crate::symmetry_ui::options_bar(app, ui) || crate::distort_ui::options_bar(app, ui) {
        return;
    }
    let tool = app.ui.tool;
    // The options bar shows the active tool's brush: switch it in before drawing the
    // chip, so a tool change swaps the brush with it (Photoshop keeps one per tool).
    crate::paint_mouse::sync_tool_brush(app);
    // Brush edits here go through `tools.setBrush`, one journal entry per gesture (Rule 1).
    if (tool.is_brushlike() && !matches!(tool, Tool::Brush | Tool::Pencil | Tool::MixerBrush | Tool::Eraser)) || tool == Tool::QuickSelection {
        brush_preset_chip(ui, &app.session.tools.brush, &mut app.ui);
        crate::brush_picker::settings_toggle(app, ui);
        OptionsBar::divider(ui);
    }
    if matches!(tool, Tool::CloneStamp | Tool::PatternStamp | Tool::HistoryBrush | Tool::Healing) {
        let before = app.session.tools.brush.clone();
        let mut brush = before.clone();
        // Mode is the tool's own brush's, kept per tool like its size.
        if let Some(modes) = crate::blend_keys::tool_modes(tool) {
            if let Some(Some(m)) = tool_mode_dropdown(ui, "retouch-mode", &modes, Some(crate::blend_keys::brush_tool_mode(tool, brush.mode))) {
                brush.mode = m;
            }
        }
        if tool == Tool::CloneStamp {
            percent_cell(ui, "Opacity", &mut brush.opacity, 1.0..=100.0);
            percent_cell(ui, "Flow", &mut brush.flow, 1.0..=100.0);
        }
        crate::brush_panel::commit_gesture(app, ui.ctx(), &before, &brush);
        OptionsBar::divider(ui);
    }
    if crate::eraser_ui::options_bar(app, ui, tool)
        || crate::retouch_ui::options_bar(app, ui, tool)
        || crate::vector_ui::options_bar(app, ui, tool)
        || crate::analysis_ui::options_bar(app, ui, tool)
        || crate::slice_ui::options_bar(app, ui, tool)
        || crate::frame_tool::options_bar(app, ui, tool)
        || crate::artboard_tool::options_bar(app, ui, tool)
    {
        return;
    }
    let brush_before = app.session.tools.brush.clone();
    let mut brush = brush_before.clone();
    tool_specific_options(app, ui, tool, &mut brush);
    // Painting symmetry is independent of visual theme. Keep the options-bar
    // action available for Brush, Pencil and Eraser in every palette (#2659).
    if matches!(tool, Tool::Brush | Tool::Pencil | Tool::Eraser) {
        crate::symmetry_ui::menu(app, ui);
    }
    crate::brush_panel::commit_gesture(app, ui.ctx(), &brush_before, &brush);
}

/// The per-tool options of the bar: the brush, mixer, marquee, pencil and view controls.
/// Brush family options share the same per-tool settings model.
fn brush_specific_options(app: &mut PhotocraftApp, ui: &mut egui::Ui, b: &mut BrushSettings) {
    match app.ui.tool {
        Tool::Brush | Tool::Eraser => {
            // A Block-mode Eraser has a fixed size, opacity and flow (Photoshop, #2770).
            let block = crate::eraser_ui::block_mode(app, app.ui.tool);
            OptionsBar::group(ui, !block, |ui| {
                brush_preset_chip(ui, b, &mut app.ui);
                crate::brush_picker::settings_toggle(app, ui);
            });
            OptionsBar::divider(ui);
            // The Eraser has no blend mode: its Mode is Brush, Pencil or Block (#2662).
            if app.ui.tool == Tool::Eraser {
                crate::eraser_ui::mode_dropdown(ui, &mut app.ui.tool_options.eraser_mode);
            } else {
                let held = app.held_erase;
                paint_mode_dropdown(ui, "brush-mode", b, &mut app.ui.tool_options.brush_clear, held);
            }
            OptionsBar::group(ui, !block, |ui| {
                percent_cell(ui, "Opacity", &mut b.opacity, 0.0..=100.0);
                ui.add(IconButton::new(Icon::Target, tl!("Always use pressure for opacity")).size(ControlSize::Md).toggle(&mut b.pressure_opacity));
            });
            // A Pencil- or Block-mode Eraser is always full flow, without build-up (Photoshop).
            OptionsBar::group(ui, !block && !crate::eraser_ui::pencil_mode(app, app.ui.tool), |ui| {
                percent_cell(ui, "Flow", &mut b.flow, 1.0..=100.0);
                ui.add(IconButton::new(Icon::Sparkles, tl!("Enable airbrush-style build-up effects")).size(ControlSize::Md).toggle(&mut b.build_up));
            });
            smoothing_cell(ui, b);
            smoothing_options(ui, b);
            OptionsBar::divider(ui);
            ui.add(IconButton::new(Icon::Target, tl!("Always use pressure for size")).size(ControlSize::Md).toggle(&mut b.pressure_size));
        }
        // Pencil: Photoshop's options (no hardness or flow: the pencil is always hard).
        Tool::Pencil => {
            brush_preset_chip(ui, b, &mut app.ui);
            crate::brush_picker::settings_toggle(app, ui);
            OptionsBar::divider(ui);
            let held = app.held_erase;
            paint_mode_dropdown(ui, "pencil-mode", b, &mut app.ui.tool_options.pencil_clear, held);
            percent_cell(ui, "Opacity", &mut b.opacity, 0.0..=100.0);
            smoothing_cell(ui, b);
            OptionsBar::divider(ui);
            check(ui, "Auto Erase", &mut app.ui.tool_options.pencil_auto_erase);
        }
        Tool::MixerBrush => {
            brush_preset_chip(ui, b, &mut app.ui);
            crate::brush_picker::settings_toggle(app, ui);
            OptionsBar::divider(ui);
            mixer_load_swatch(app, ui);
            let m = &mut b.mixer;
            ui.add(IconButton::new(Icon::Brush, tl!("Load the brush after each stroke")).size(ControlSize::Md).toggle(&mut m.load_after_stroke));
            ui.add(IconButton::new(Icon::BrushCleaning, tl!("Clean the brush after each stroke")).size(ControlSize::Md).toggle(&mut m.clean_after_stroke));
            OptionsBar::divider(ui);
            for (label, value) in [("Wet", &mut b.mixer.wet), ("Load", &mut b.mixer.load), ("Mix", &mut b.mixer.mix), ("Flow", &mut b.mixer.flow)] {
                percent_cell(ui, label, value, 0.0..=100.0);
            }
            OptionsBar::divider(ui);
            check(ui, "Sample All Layers", &mut b.mixer.sample_all_layers);
        }
        _ => {}
    }
}

/// New / add / subtract / intersect selection buttons (shared by selection tools): separate
/// buttons, 2 pt apart.
fn selection_mode_buttons(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let modes = [
        (Icon::Square, tl!("New selection").to_string()),
        (Icon::Plus, crate::i18n::fmt(tl!("Add to selection  ({key})"), &[("key", &crate::shortcuts::pretty("Shift"))])),
        (Icon::Minus, crate::i18n::fmt(tl!("Subtract from selection  ({key})"), &[("key", &crate::shortcuts::pretty("Alt"))])),
        (Icon::SquaresSubtract, crate::i18n::fmt(tl!("Intersect with selection  ({key})"), &[("key", &crate::shortcuts::pretty("Shift+Alt"))])),
    ];
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (i, (icon, tip)) in modes.iter().enumerate() {
            if icon_toggle(ui, *icon, tip, app.ui.selection_mode == i as u8).clicked() {
                app.ui.selection_mode = i as u8;
            }
        }
    });
}

/// Select and Mask… (enabled with a selection).
fn select_and_mask_button(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let enabled = crate::menus::is_enabled(app, "select.selectAndMask");
    let clicked = ui.add_enabled_ui(enabled, |ui| bar_button(ui, "Select and Mask…").clicked()).inner;
    if clicked {
        let _ = crate::menus::invoke(app, ui.ctx(), "select.selectAndMask", json!({}));
    }
}

/// Marquee tools: selection mode, feather, anti-alias, style and fixed size.
fn marquee_options(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    selection_mode_buttons(app, ui);
    OptionsBar::divider(ui);
    let (units, size) = fixed_size_units(app);
    let o = &mut app.ui.tool_options;
    number_cell(ui, &Field::new("Feather", 0.0..=1000.0, "px", CellWidth::Md), &mut o.feather);
    // Photoshop greys Anti-alias for the Rectangular Marquee (its edges are always hard).
    OptionsBar::group(ui, app.ui.tool == Tool::EllipseMarquee, |ui| check(ui, "Anti-alias", &mut o.anti_alias));
    OptionsBar::divider(ui);
    // A single row or column has no style or size: Photoshop greys them.
    let sized = matches!(app.ui.tool, Tool::RectMarquee | Tool::EllipseMarquee);
    let styles = [("normal".to_string(), "Normal"), ("fixedRatio".to_string(), "Fixed Ratio"), ("fixedSize".to_string(), "Fixed Size")];
    let picked = OptionsBar::group(ui, sized, |ui| select_cell(ui, Some("Style"), "marquee-style", "Style", &mut o.marquee_style, &styles, false));
    if picked {
        (o.marquee_width, o.marquee_height) = if o.marquee_style == "fixedSize" { (64.0, 64.0) } else { (1.0, 1.0) };
    }
    let fixed = sized && o.marquee_style != "normal";
    let fixed_size = o.marquee_style == "fixedSize";
    // Fixed Size shows its pixels in the unit picked by right-clicking Width or Height (#7910).
    let units = widgets::FieldUnits { unit: photocraft_engine::prefs::Unit::parse(&o.marquee_unit).unwrap_or_default(), ..units };
    OptionsBar::group(ui, fixed, |ui| {
        let mut picked = None;
        if fixed_size {
            picked = fixed_size_field(ui, "Width", &mut o.marquee_width, units, size[0]);
        } else {
            let range = crate::tool_memory::style_range(&o.marquee_style);
            number_cell(ui, &Field::new("Width", range, "", CellWidth::Md).decimals(0..=3), &mut o.marquee_width);
        }
        if ui.add(IconButton::new(Icon::SwapHorizontal, tl!("Swaps height and width")).size(ControlSize::Md)).clicked() {
            std::mem::swap(&mut o.marquee_width, &mut o.marquee_height);
        }
        if fixed_size {
            picked = fixed_size_field(ui, "Height", &mut o.marquee_height, units, size[1]).or(picked);
        } else {
            let range = crate::tool_memory::style_range(&o.marquee_style);
            number_cell(ui, &Field::new("Height", range, "", CellWidth::Md).decimals(0..=3), &mut o.marquee_height);
        }
        if let Some(name) = picked {
            o.marquee_unit = name.into();
        }
    });
    OptionsBar::divider(ui);
    select_and_mask_button(app, ui);
}

/// Lasso family: selection mode, feather, anti-alias, magnetic settings and the drawing hints.
fn lasso_options(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    selection_mode_buttons(app, ui);
    OptionsBar::divider(ui);
    number_cell(ui, &Field::new("Feather", 0.0..=1000.0, "px", CellWidth::Md), &mut app.ui.tool_options.feather);
    check(ui, "Anti-alias", &mut app.ui.tool_options.anti_alias);
    OptionsBar::divider(ui);
    if app.ui.tool == Tool::MagneticLasso {
        let o = &mut app.ui.tool_options;
        let tip = tl!("Follows only edges this close to the pointer ([ and ] change it)");
        number_cell(ui, &Field::new("Width", 1.0..=256.0, "px", CellWidth::Md).tooltip(tip), &mut o.magnetic_width);
        let tip = tl!("Higher values follow only edges that contrast sharply with their surroundings");
        number_cell(ui, &Field::new("Contrast", 1.0..=100.0, "%", CellWidth::Md).tooltip(tip), &mut o.magnetic_contrast);
        let tip = tl!("How often fastening points are placed by themselves");
        number_cell(ui, &Field::new("Frequency", 0.0..=100.0, "", CellWidth::Sm).tooltip(tip), &mut o.magnetic_frequency);
        ui.add(IconButton::new(Icon::Target, tl!("Use tablet pressure to change pen width")).size(ControlSize::Md).toggle(&mut o.magnetic_pressure));
        OptionsBar::divider(ui);
    }
    select_and_mask_button(app, ui);
    if crate::lasso_ui::active(app) || (app.ui.tool == Tool::PolygonLasso && !app.ui.polygon.is_empty()) {
        OptionsBar::hint(
            ui,
            &crate::i18n::fmt(tl!("Click the first point or press {key} to close · Esc cancels"), &[("key", &crate::shortcuts::pretty("Enter"))]),
        );
    } else if app.ui.tool == Tool::Lasso {
        OptionsBar::hint(ui, tl!("Hold Alt while drawing for straight segments"));
    }
    if app.ui.tool == Tool::MagneticLasso && app.ui.magnetic.active() {
        OptionsBar::hint(
            ui,
            &crate::i18n::fmt(
                tl!("Click the first point or press {key} to close · {del} removes a point · Esc cancels"),
                &[("key", &crate::shortcuts::pretty("Enter")), ("del", &crate::shortcuts::pretty("Backspace"))],
            ),
        );
    }
}

/// Selection and paint tools expose their controls as a family of options rows.
fn selection_specific_options(app: &mut PhotocraftApp, ui: &mut egui::Ui, tool: Tool) {
    match app.ui.tool {
        Tool::RectMarquee | Tool::EllipseMarquee | Tool::SingleRowMarquee | Tool::SingleColumnMarquee => {
            marquee_options(app, ui);
        }
        Tool::Lasso | Tool::PolygonLasso | Tool::MagneticLasso => {
            lasso_options(app, ui);
        }
        Tool::MagicWand => {
            selection_mode_buttons(app, ui);
            OptionsBar::divider(ui);
            number_cell(ui, &Field::new("Tolerance", 0.0..=255.0, "", CellWidth::Sm), &mut app.ui.tool_options.tolerance);
            check(ui, "Anti-alias", &mut app.ui.tool_options.anti_alias);
            check(ui, "Contiguous", &mut app.ui.tool_options.contiguous);
            check(ui, "Sample All Layers", &mut app.ui.tool_options.sample_all.magic_wand);
            OptionsBar::divider(ui);
            if bar_button(ui, "Select Subject").clicked() {
                let r = app.run("select.subject", json!({}));
                crate::retouch_ui::report_smart_result(app, r, tl!("No subject found — try Object Selection or the Lasso"));
            }
            select_and_mask_button(app, ui);
        }
        Tool::PaintBucket => {
            let mut src = u8::from(app.ui.tool_options.bucket_fill_pattern);
            if select_cell(ui, Some("Fill"), "bucket-src", "Source", &mut src, &[(0u8, "Foreground"), (1, "Pattern")], false) {
                app.ui.tool_options.bucket_fill_pattern = src == 1;
            }
            if let Some(modes) = crate::blend_keys::tool_modes(tool) {
                let o = &mut app.ui.tool_options;
                let current = if o.bucket_clear { None } else { Some(o.bucket_blend_mode) };
                if let Some(m) = tool_mode_dropdown(ui, "bucket-mode", &modes, current) {
                    o.bucket_clear = m.is_none();
                    if let Some(m) = m {
                        o.bucket_blend_mode = m;
                    }
                }
            }
            number_cell(ui, &Field::new("Opacity", 1.0..=100.0, "%", CellWidth::Md), &mut app.ui.tool_options.fill_opacity);
            number_cell(ui, &Field::new("Tolerance", 0.0..=255.0, "", CellWidth::Sm), &mut app.ui.tool_options.tolerance);
            check(ui, "Anti-alias", &mut app.ui.tool_options.anti_alias);
            check(ui, "Contiguous", &mut app.ui.tool_options.contiguous);
            check(ui, "All Layers", &mut app.ui.tool_options.sample_all.paint_bucket);
        }
        Tool::Gradient => {
            // Live "Gradient" (a Gradient Fill layer) or "Classic gradient" (pixels).
            let mut classic = app.ui.tool_options.gradient_classic;
            if select_cell(ui, None, "gradient-mode", "Mode", &mut classic, &[(false, "Gradient"), (true, "Classic gradient")], false) {
                app.ui.tool_options.gradient_classic = classic;
            }
            // Live "Gradient" (a Gradient Fill layer) or "Classic gradient" (pixels):
            // both paint with the same engine-owned preset (#1651), and either swatch
            // click opens the Gradient Editor window, as in Photoshop.
            if crate::gradient_ui::preset_swatch(app, ui) {
                app.ui.panels.gradient_editor = true;
            }
            OptionsBar::divider(ui);
            let before = app.ui.tool_options.clone();
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (style, icon, tip) in [
                    ("linear", Icon::Gradient, tl!("Linear Gradient")),
                    ("radial", Icon::Circle, tl!("Radial Gradient")),
                    ("angle", Icon::RotateCw, tl!("Angle Gradient")),
                    ("reflected", Icon::SwapHorizontal, tl!("Reflected Gradient")),
                    ("diamond", Icon::Diamond, tl!("Diamond Gradient")),
                ] {
                    if icon_toggle(ui, icon, tip, app.ui.tool_options.gradient_style == style).clicked() {
                        app.ui.tool_options.gradient_style = style.into();
                    }
                }
            });
            OptionsBar::divider(ui);
            let opts: Vec<(BlendMode, &str)> = BlendMode::LAYER_MODES.iter().map(|m| (*m, m.label())).collect();
            // The wheel over it steps the mode, as over the Layers panel's (#2061).
            select_cell(ui, Some("Mode"), "gradient-blend-mode", "Blend Mode", &mut app.ui.tool_options.gradient_blend_mode, &opts, true);
            // Live while scrubbed, one undo step per scrub (#5012).
            let opacity = number_cell(ui, &Field::new("Opacity", 1.0..=100.0, "%", CellWidth::Md), &mut app.ui.tool_options.fill_opacity);
            if opacity.changed {
                crate::gradient_ui::opacity_changed(app, opacity.gesture);
            }
            check(ui, "Reverse", &mut app.ui.tool_options.gradient_reverse);
            check(ui, "Dither", &mut app.ui.tool_options.gradient_dither);
            // Live mode: the options also change a selected gradient fill layer.
            crate::gradient_ui::options_changed(app, &before);
        }
        Tool::PerspectiveCrop => crate::perspective_crop_ui::options_bar(app, ui),
        Tool::Crop => {
            ui.data_mut(|d| d.insert_temp(crop_flag(), true));
            let before_ratio = crate::crop_ui::preset_ratio(app);
            let doc_size = app.session.active().map_or((0.0, 0.0), |s| (f64::from(s.doc.size.width), f64::from(s.doc.size.height)));
            // Ratios, sizes, saved presets, New / Delete Crop Preset… (#2443, #1919).
            crate::crop_presets::dropdown(app, ui);
            let o = &mut app.ui.tool_options;
            let mut ratio_base = None;
            if o.crop_ratio == crate::crop_size::WHR {
                crate::crop_size::fields(o, ui);
            } else {
                // Keep partial edits in ToolOptions so the first side survives a frame;
                // Original Ratio shows the document's numbers, as Photoshop does.
                let (mut rw, mut rh) = crate::crop_size::custom_ratio_fields(o, doc_size);
                let decimals = crate::crop_size::RATIO_DECIMALS;
                let fw = widgets::value_field_prec(ui, &mut rw, 0.0..=99_999.0, "", 54.0, decimals);
                let swap = ui
                    .add(
                        craft_ui::controls::IconButton::new(craft_ui::icons::Icon::SwapHorizontal, tl!("Swaps height and width"))
                            .size(craft_ui::controls::ControlSize::Sm),
                    )
                    .clicked();
                let fh = widgets::value_field_prec(ui, &mut rh, 0.0..=99_999.0, "", 54.0, decimals);
                let (cw, ch) = (fw.changed(), fh.changed());
                if swap {
                    std::mem::swap(&mut rw, &mut rh);
                }
                if cw || ch || swap {
                    crate::crop_size::set_custom_ratio_fields(o, rw, rh);
                }
                // Every ratio typed or scrubbed in one edit fits from the frame the edit
                // started with: 1:3 on the way to 16:3 mustn't shrink it first (#7752).
                let base_id = egui::Id::unique("crop-ratio-edit-base");
                if cw || ch {
                    ratio_base = ui.data(|d| d.get_temp::<[f64; 4]>(base_id)).or(app.ui.crop_rect);
                }
                if ![&fw, &fh].iter().any(|f| f.has_focus() || f.dragged()) {
                    ui.data_mut(|d| d.remove::<[f64; 4]>(base_id));
                } else if let Some(base) = ratio_base {
                    ui.data_mut(|d| d.insert_temp(base_id, base));
                }
            }
            widgets::vline(ui, 22.0);
            if ui.add(craft_ui::controls::Button::new(tl!("Clear")).variant(craft_ui::controls::ButtonVariant::Secondary)).clicked() {
                crate::crop_size::clear_ratio(o);
            }
            crate::crop_straighten::options_button(&mut app.crop.straighten, ui);
            crate::crop_overlay::options_button(o, ui);
            let pick_shield_color = crate::crop_shield::options_button(&mut o.crop_shield, ui);
            craft_ui::controls::Checkbox::bool(tl!("Delete Cropped Pixels"), &mut o.crop_delete).show(ui).response;
            // Fill: what a turned or extending crop puts where the image doesn't reach
            // (#3823). Generative Expand is not offered.
            ui.label(tl!("Fill:"));
            craft_ui::controls::Select::new("crop-fill", tl!("Fill:"), &mut o.crop_content_aware)
                .fit(true)
                .option(false, tl!("Background (Default)"))
                .option(true, tl!("Content-Aware Fill"))
                .show(ui)
                .response
                .changed();
            if crate::crop_ui::preset_ratio(app) != before_ratio {
                match ratio_base {
                    Some(base) => crate::crop_ui::apply_ratio_from(app, base),
                    None => crate::crop_ui::apply_ratio(app),
                }
            }
            if pick_shield_color {
                crate::crop_shield::pick_custom_color(app);
            }
        }
        // Every theme gets the font, style and size controls (#1387), as with Auto-Select (#1275).
        Tool::Type | Tool::VerticalType | Tool::HorizontalTypeMask | Tool::VerticalTypeMask => crate::type_tool::options_bar(app, ui),
        _ => {}
    }
}

fn is_selection_options_tool(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::RectMarquee
            | Tool::EllipseMarquee
            | Tool::SingleRowMarquee
            | Tool::SingleColumnMarquee
            | Tool::Lasso
            | Tool::PolygonLasso
            | Tool::MagneticLasso
            | Tool::MagicWand
            | Tool::PaintBucket
            | Tool::Gradient
            | Tool::PerspectiveCrop
            | Tool::Crop
            | Tool::Type
            | Tool::VerticalType
            | Tool::HorizontalTypeMask
            | Tool::VerticalTypeMask
    )
}

/// Resets a view to 100 % (Zoom's and Hand's 100% button).
fn view_to_actual_size(app: &mut PhotocraftApp) {
    if let Some(i) = app.session.active_index() {
        app.ui.views[i].zoom = 1.0;
        app.ui.views[i].request_zoom_animation(app.session.prefs().general.animated_zoom);
        app.ui.views[i].zoom_anchor = None;
        app.ui.views[i].fit_pending = false;
        app.ui.views[i].fill_pending = false;
    }
}

/// Fits the view to the screen (Zoom's and Hand's Fit Screen button).
fn view_to_fit(app: &mut PhotocraftApp) {
    if let Some(i) = app.session.active_index() {
        app.ui.views[i].fit_pending = true;
        app.ui.views[i].request_zoom_animation(app.session.prefs().general.animated_zoom);
        app.ui.views[i].zoom_anchor = None;
        app.ui.views[i].fill_pending = false;
    }
}

fn tool_specific_options(app: &mut PhotocraftApp, ui: &mut egui::Ui, tool: Tool, b: &mut BrushSettings) {
    if matches!(app.ui.tool, Tool::Brush | Tool::Eraser | Tool::Pencil | Tool::MixerBrush) {
        brush_specific_options(app, ui, b);
        return;
    }
    if is_selection_options_tool(app.ui.tool) {
        selection_specific_options(app, ui, tool);
        return;
    }
    match app.ui.tool {
        Tool::Move => {
            let o = &mut app.ui.tool_options;
            check(ui, "Auto-Select:", &mut o.move_auto_select);
            let targets = [("layer".to_string(), "Layer"), ("group".to_string(), "Group")];
            select_cell(ui, None, "move-target", "Auto-Select:", &mut o.move_target, &targets, false);
            OptionsBar::divider(ui);
            check(ui, "Show Transform Controls", &mut o.move_show_transform);
            OptionsBar::divider(ui);
            crate::align_bar::options(app, ui);
        }
        Tool::Eyedropper => crate::eyedropper_ui::options(app, ui),
        Tool::Zoom => {
            // Resize Windows to Fit is Preferences › Zoom Resizes Windows itself, as in
            // Photoshop: one setting, shown in both places (zoom_window_fit.rs).
            let mut fit = app.session.prefs().general.zoom_resizes_windows;
            if check(ui, "Resize Windows to Fit", &mut fit).changed() {
                app.session.prefs.edit(|p| p.general.zoom_resizes_windows = fit);
            }
            check(ui, "Scrubby Zoom", &mut app.ui.tool_options.zoom_scrubby);
            OptionsBar::hint(
                ui,
                &crate::i18n::fmt(
                    tl!("Click to zoom in  ·  {key}-click to zoom out  ·  drag right/left to zoom in/out"),
                    &[("key", &crate::shortcuts::pretty("Alt"))],
                ),
            );
            if bar_button(ui, "Fit Screen").clicked() {
                view_to_fit(app);
            }
            if bar_button(ui, "100%").clicked() {
                view_to_actual_size(app);
            }
        }
        Tool::RotateView => {
            OptionsBar::hint(ui, tl!("Drag around the centre to rotate the view  ·  Shift constrains to 15°"));
            if let Some(i) = app.session.active_index() {
                let mut angle = app.ui.views.get(i).map(|v| v.rotation).unwrap_or(0.0);
                if number_cell(ui, &Field::new("Angle", -180.0..=180.0, "°", CellWidth::Md), &mut angle).changed
                    && let Some(v) = app.ui.views.get_mut(i)
                {
                    v.rotation = crate::rotate_view::wrap_deg(angle);
                }
                if bar_button(ui, "Reset View").clicked()
                    && let Some(v) = app.ui.views.get_mut(i)
                {
                    v.rotation = 0.0;
                }
            }
        }
        Tool::Hand => {
            OptionsBar::hint(ui, tl!("Drag to pan  ·  hold Space with any tool"));
            if bar_button(ui, "100%").clicked() {
                view_to_actual_size(app);
            }
            if bar_button(ui, "Fit Screen").clicked() {
                view_to_fit(app);
            }
            if bar_button(ui, "Fill Screen").clicked()
                && let Some(i) = app.session.active_index()
            {
                app.ui.views[i].fill_pending = true;
                app.ui.views[i].request_zoom_animation(app.session.prefs().general.animated_zoom);
                app.ui.views[i].zoom_anchor = None;
                app.ui.views[i].fit_pending = false;
            }
        }
        // Retouching and smart-selection tools draw their bar in `retouch_ui::options_bar`.
        _ => {}
    }
}

/// Width of the Layers panel's Opacity and Fill fields, with room for their pop-up slider's ▾.
pub(super) const LAYER_PCT_W: f32 = 66.0 + widgets::POPUP_ARROW_W;

/// The `layer.setProps` edit of a Layers panel Opacity or Fill (`key`) field at `pct` percent. A
/// drag of the pop-up slider (`drag`) coalesces into one history step, as in Photoshop.
pub(super) fn pct_action(target: &Value, key: &str, pct: f32, drag: Option<u64>) -> (String, Value) {
    let mut p = target.clone();
    p[key] = json!(pct / 100.0);
    if let Some(n) = drag {
        p["coalesce"] = json!(format!("layer-{key}-slider:{n}"));
    }
    ("layer.setProps".into(), p)
}

/// Photoshop's scrubby Opacity/Fill labels: drag horizontally to change the percentage without
/// hitting the small number field. The unique drag ID coalesces every frame into one undo step.
pub(super) fn scrub_pct_label(ui: &mut egui::Ui, text: &str, percent: &mut f32, max_w: f32) -> widgets::PopupFieldResponse {
    // Elide to the room left in the row (full text on hover): a longer translation shrinks here
    // instead of running over the fixed controls beside it (#3580).
    let resp = widgets::elided_label_sensed(ui, text, max_w, Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    let key = resp.id.with("percent-label-drag");
    if resp.drag_started() {
        let stamp = ui.ctx().cumulative_pass_nr();
        ui.data_mut(|d| d.insert_temp(key, (*percent, stamp)));
    }
    let mut out = widgets::PopupFieldResponse::default();
    if resp.dragged()
        && let Some((start, stamp)) = ui.data(|d| d.get_temp::<(f32, u64)>(key))
        && let (Some(origin), Some(pos)) = (ui.input(|i| i.pointer.press_origin()), resp.interact_pointer_pos())
    {
        // Match DragValue's 0.5 percentage-point per screen point; do not jump on press.
        let next = (start + (pos.x - origin.x) * 0.5).clamp(0.0, 100.0);
        if next.is_finite() && next != *percent {
            *percent = next;
            out.changed = true;
            out.drag = Some(stamp);
        }
    }
    if resp.drag_stopped() {
        ui.data_mut(|d| d.remove::<(f32, u64)>(key));
    }
    out
}

/// Width of `s` as [`label`] draws it (body text).
pub(super) fn body_text_width(ui: &egui::Ui, s: &str) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    ui.painter().layout_no_wrap(s.to_string(), font, Color32::WHITE).size().x
}

/// An options-bar label; one right before a value field scrubs it when dragged (Photoshop). The
/// strips of the other tools still draw their cells with it.
pub(crate) fn opt_label(ui: &mut egui::Ui, s: &str) {
    let t = Tokens::get(ui.ctx());
    let label = ui.add(egui::Label::new(RichText::new(caption(s)).color(t.text_dim)).selectable(false));
    widgets::scrub_option_label(ui, &label);
}

/// The largest Fixed Size side, in pixels.
pub(crate) const FIXED_SIZE_MAX_PX: f64 = 300_000.0;

/// The resolution Fixed Size's units convert at (the active document's, 72 ppi without one) and
/// the document's size in pixels, which a percentage is of. The unit is the caller's.
fn fixed_size_units(app: &PhotocraftApp) -> (widgets::FieldUnits, [f64; 2]) {
    let ppi = app.session.prefs().units_and_rulers.point_size.per_inch();
    let (dpi, size) = app
        .session
        .active()
        .map_or((72.0, [1.0, 1.0]), |d| (f64::from(d.doc.resolution_dpi.max(1.0)), [f64::from(d.doc.size.width), f64::from(d.doc.size.height)]));
    (widgets::FieldUnits { unit: photocraft_engine::prefs::Unit::Pixels, dpi, ppi }, size)
}

/// One of Fixed Size's Width and Height (#7910): `px` shown in `units.unit` (a typed unit converts),
/// still stored in pixels. A right-click lists the units, as on the rulers; returns the one picked.
fn fixed_size_field(ui: &mut egui::Ui, label: &str, px: &mut f32, units: widgets::FieldUnits, extent: f64) -> Option<&'static str> {
    let widgets::FieldUnits { unit, dpi, ppi } = units;
    let to_unit = |px: f64| unit.from_px(px, dpi, extent, ppi) as f32;
    let before = to_unit(f64::from(*px));
    let mut v = before;
    let places = unit.decimals() as usize;
    let entries = crate::rulers::unit_entries(unit);
    let mut picked = None;
    let field = Field::new(label, to_unit(1.0)..=to_unit(FIXED_SIZE_MAX_PX), unit.suffix().trim(), CellWidth::Lg).decimals(0..=places);
    let edit = number_cell_with(ui, &field, &mut v, Some(units), None, Some((&entries, &mut picked)));
    // Leaving the field re-reads the rounded text it showed (100 px at 72 ppi reads back as
    // 1.389 in); that isn't an edit, so the pixels stay exact.
    let shown = |x: f32| (f64::from(x) * 10f64.powi(places as i32)).round();
    if edit.changed && shown(v) != shown(before) {
        *px = unit.to_px(f64::from(v), dpi, extent, ppi).clamp(1.0, FIXED_SIZE_MAX_PX) as f32;
    }
    picked.map(crate::rulers::unit_name_at)
}
