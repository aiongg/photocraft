use super::{Edit, EditorCx, ToneSpace, Value, adjust_params, json, label, space_of};
use craft::ui::composites::{ChannelColor, Curve, CurveChannel, CurveEditor, HistogramData};
use craft::ui::edit::EditEvent;
use craft::ui::localized::LocaleNumbers;

// ---------------------------------------------------------------------------------------------
// Channels of Levels / Curves

/// A channel of a tone editor: the params key (None = the top-level / composite keys for Levels;
/// Curves uses "points"), its label, and its colour name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneChannel {
    pub key: &'static str,
    pub label: &'static str,
}

/// The channels a tone editor lists for `space` (histogram index = position).
pub fn tone_channels(space: ToneSpace, gray: bool) -> &'static [ToneChannel] {
    const RGB: [ToneChannel; 4] = [
        ToneChannel { key: "", label: "RGB" },
        ToneChannel { key: "red", label: "Red" },
        ToneChannel { key: "green", label: "Green" },
        ToneChannel { key: "blue", label: "Blue" },
    ];
    const GRAY: [ToneChannel; 1] = [ToneChannel { key: "", label: "Gray" }];
    const CMYK: [ToneChannel; 5] = [
        ToneChannel { key: "", label: "CMYK" },
        ToneChannel { key: "cyan", label: "Cyan" },
        ToneChannel { key: "magenta", label: "Magenta" },
        ToneChannel { key: "yellow", label: "Yellow" },
        ToneChannel { key: "black", label: "Black" },
    ];
    const LAB: [ToneChannel; 3] =
        [ToneChannel { key: "lightness", label: "Lightness" }, ToneChannel { key: "a", label: "a" }, ToneChannel { key: "b", label: "b" }];
    match space {
        ToneSpace::Rgb if gray => &GRAY,
        ToneSpace::Rgb => &RGB,
        ToneSpace::Cmyk => &CMYK,
        ToneSpace::Lab => &LAB,
    }
}

pub(super) fn channel_picker(ui: &mut egui::Ui, id: egui::Id, chans: &[ToneChannel], text: &str) -> usize {
    let opts: Vec<(usize, &str)> = chans.iter().enumerate().map(|(i, c)| (i, c.label)).collect();
    channel_picker_opts(ui, id, &opts, text, true)
}

/// The popup, with `translate` for whether the built-in labels follow the interface language;
/// a user-typed channel name is shown exactly as entered (#5335).
fn channel_picker_opts(ui: &mut egui::Ui, id: egui::Id, opts: &[(usize, &str)], text: &str, translate: bool) -> usize {
    let mut ch: usize = ui.data(|d| d.get_temp(id)).unwrap_or(0);
    if ch >= opts.len() {
        ch = 0;
    }
    ui.horizontal(|ui| {
        label(ui, text);
        let key = format!("{id:?}-ch");
        if translate {
            craft::ui::controls::Select::new(&key, text, &mut ch).rows(opts.len(), |i| opts[i].0.clone(), |i| tl!(opts[i].1)).show(ui).response.changed();
        } else {
            craft::ui::controls::Select::new(&key, text, &mut ch).rows(opts.len(), |i| opts[i].0.clone(), |i| opts[i].1).show(ui).response.changed();
        }
    });
    ui.data_mut(|d| d.insert_temp(id, ch));
    ch
}

/// The library colour of a channel, by its params key: the composite, Lightness and CMYK Black are
/// neutral, Lab a and b take magenta and yellow.
pub(super) fn channel_color(key: &str) -> ChannelColor {
    match key {
        "red" => ChannelColor::Red,
        "green" => ChannelColor::Green,
        "blue" => ChannelColor::Blue,
        "cyan" => ChannelColor::Cyan,
        "magenta" | "a" => ChannelColor::Magenta,
        "yellow" | "b" => ChannelColor::Yellow,
        _ => ChannelColor::Neutral,
    }
}

// ---------------------------------------------------------------------------------------------
// Curves

const CURVE_SAMPLES: usize = 256;

/// The room left on screen for the Curves dialog's other rows (title, channel and mode, Input and
/// Output, eyedroppers, buttons) when its graph is as tall as it can be.
const CURVES_CHROME: f32 = 360.0;

/// The smallest and the default largest side of the Curves graph: 300 points was the fixed
/// size until #8984.
const CURVE_SIDE: (f32, f32) = (120.0, 300.0);

/// The Curves graph's side (#8984): the width it is given, in the Properties panel and in the
/// resizable dialog, and no taller than the window leaves room for beside the other rows.
fn curve_side(ui: &egui::Ui) -> f32 {
    let tallest = curve_side_max(ui.ctx());
    let width = ui.available_width();
    if width.is_finite() { width.clamp(CURVE_SIDE.0, tallest) } else { CURVE_SIDE.1 }
}

/// The largest the Curves graph gets in this window: as tall as the window leaves room for beside
/// the dialog's other rows, and never under the old 300 points. The Curves dialog is no wider.
pub(crate) fn curve_side_max(ctx: &egui::Context) -> f32 {
    (ctx.content_rect().height() - CURVES_CHROME).max(CURVE_SIDE.1)
}

pub(crate) fn read_curve(v: &Value, key: &str) -> Vec<[f32; 2]> {
    let pts: Vec<[f32; 2]> = v
        .get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|q| {
                    let q = q.as_array()?;
                    Some([q.first()?.as_f64()? as f32, q.get(1)?.as_f64()? as f32])
                })
                .collect()
        })
        .unwrap_or_default();
    if pts.len() < 2 { vec![[0.0, 0.0], [255.0, 255.0]] } else { pts }
}

fn curve_key(ch: &ToneChannel) -> &'static str {
    if ch.key.is_empty() { "points" } else { ch.key }
}

/// A pencil-drawn channel: its 256 output levels (identity when absent or not a map).
fn read_map(v: &Value, key: &str) -> Vec<f32> {
    let map: Option<Vec<f32>> = v
        .get(key)
        .and_then(Value::as_array)
        .filter(|a| a.len() == photocraft_doc::adjust::CURVE_MAP_LEN)
        .and_then(|a| a.iter().map(|o| o.as_f64().map(|o| o as f32)).collect());
    map.unwrap_or_else(|| (0..photocraft_doc::adjust::CURVE_MAP_LEN).map(|k| k as f32).collect())
}

fn write_map(v: &mut Value, key: &str, map: &[f32]) {
    v[key] = json!(map.iter().map(|o| (o * 100.0).round() / 100.0).collect::<Vec<_>>());
}

/// Switches the Curves values between point editing and the pencil: the engine draws the point
/// curves into maps, or fits points to the drawn maps (Photoshop's two curve buttons).
fn set_pencil(v: &mut Value, pencil: bool, gray: bool) -> bool {
    let mut p = v.clone();
    p["pencil"] = json!(pencil);
    let mode = if gray { photocraft_doc::ColorMode::Grayscale } else { photocraft_doc::ColorMode::Rgb };
    let Ok(a) = adjust_params::from_params("curves", &p, None, mode) else { return false };
    *v = adjust_params::to_params(&a);
    // Explicit, so an update over a pencil-drawn layer leaves the pencil.
    v["pencil"] = json!(pencil);
    true
}

/// Mode buttons above the graph: points, pencil, and Smooth for the shown channel's drawing.
fn curves_mode_row(ui: &mut egui::Ui, v: &mut Value, key: &str, gray: bool) -> Edit {
    let pencil = v.get("pencil").and_then(Value::as_bool).unwrap_or(false);
    let mut e = Edit::default();
    ui.horizontal(|ui| {
        if ui
            .add(
                craft::ui::controls::IconButton::new(craft::ui::icons::Icon::Spline, tl!("Edit points to modify the curve"))
                    .size(craft::ui::controls::ControlSize::Md)
                    .selected(!pencil),
            )
            .clicked()
            && pencil
        {
            e.add(Edit::discrete(set_pencil(v, false, gray)));
        }
        if ui
            .add(
                craft::ui::controls::IconButton::new(craft::ui::icons::Icon::Pencil, tl!("Draw to modify the curve"))
                    .size(craft::ui::controls::ControlSize::Md)
                    .selected(pencil),
            )
            .clicked()
            && !pencil
        {
            e.add(Edit::discrete(set_pencil(v, true, gray)));
        }
        let label = crate::i18n::tr_ctx(crate::i18n::current(), "curves", "Smooth");
        let smooth = ui
            .add_enabled_ui(pencil, |ui| ui.add(craft::ui::controls::Button::new(tl!(label)).variant(craft::ui::controls::ButtonVariant::Secondary)))
            .inner
            .on_hover_text(tl!("Smooth the curve values"))
            .on_disabled_hover_text(tl!("Smooth the curve values"));
        if smooth.clicked() {
            let mut map = read_map(v, key);
            adjust_params::smooth_curve_map(&mut map);
            write_map(v, key, &map);
            e.add(Edit::discrete(true));
        }
    });
    e
}

/// The curve the params hold for `key`: the pencil's drawing or the points.
fn read_shape(v: &Value, key: &str, pencil: bool) -> Curve {
    if pencil { Curve::Map(read_map(v, key)) } else { Curve::smooth(read_curve(v, key)) }
}

/// Writes `curve` into the params of `key`.
fn write_shape(v: &mut Value, key: &str, space: ToneSpace, curve: &Curve) {
    match curve {
        Curve::Points { points, .. } => v[key] = json!(points.iter().map(|q| [q[0].round(), q[1].round()]).collect::<Vec<_>>()),
        Curve::Map(map) => write_map(v, key, map),
    }
    if space == ToneSpace::Lab
        && let Some(o) = v.as_object_mut()
    {
        // Lab has no composite: "points" would alias lightness.
        o.remove("points");
    }
}

/// The photocraft curve through `points`: one output level per input level.
fn sample_levels(points: &[[f32; 2]]) -> Vec<f32> {
    let cp: Vec<photocraft_doc::adjust::CurvePoint> =
        points.iter().map(|q| photocraft_doc::adjust::CurvePoint { input: q[0] / 255.0, output: q[1] / 255.0 }).collect();
    photocraft_compose::adjust::curve_samples::<CURVE_SAMPLES>(&cp).into_iter().map(|y| y * 255.0).collect()
}

pub(super) fn curves(ui: &mut egui::Ui, v: &mut Value, cx: &EditorCx) -> Edit {
    let space = space_of(v);
    let chans = tone_channels(space, cx.gray);
    let isolated = cx.isolated_curve.as_ref();
    let ch = if let Some(iso) = isolated {
        channel_picker_opts(ui, cx.mem.with("curves-ch"), &[(0, iso.name.as_str())], tl!("Channel:"), iso.translate)
    } else {
        channel_picker(ui, cx.mem.with("curves-ch"), chans, tl!("Channel:"))
    };
    let chan =
        if let Some(iso) = isolated { ToneChannel { key: iso.key, label: "" } } else { chans.get(ch).copied().unwrap_or(ToneChannel { key: "", label: "" }) };
    let overlay_others = isolated.is_none() && ch == 0;
    let tinted = isolated.map_or(ch != 0, |c| c.tint);
    let hist_i = isolated.map_or(ch, |c| c.hist);
    let key = curve_key(&chan);
    let mut e = curves_mode_row(ui, v, key, cx.gray);
    let pencil = v.get("pencil").and_then(Value::as_bool).unwrap_or(false);
    ui.add_space(craft::ui::theme::scale::Space::S4.px());

    let neutral = |c: ChannelColor| if tinted { c } else { ChannelColor::Neutral };
    let channels: Vec<CurveChannel> = match isolated {
        Some(iso) => vec![CurveChannel::new(if iso.translate { tl!(iso.name.as_str()).to_owned() } else { iso.name.clone() }, neutral(channel_color(iso.key)))],
        None => chans.iter().map(|c| CurveChannel::new(tl!(c.label), channel_color(c.key))).collect(),
    };
    let overlays: Vec<Curve> = if overlay_others {
        // Photoshop overlays the edited channel curves on the composite.
        chans.iter().enumerate().map(|(i, c)| if i == 0 { Curve::identity(255.0) } else { read_shape(v, c.key, pencil) }).collect()
    } else {
        Vec::new()
    };
    let curve = read_shape(v, key, pencil);
    let histogram = cx.hist.as_ref().and_then(|h| h.ch.get(hist_i)).map(|bins| HistogramData::tone(bins.as_slice(), ChannelColor::Neutral));
    let hint = if pencil {
        crate::i18n::fmt(tl!("Drag to draw the curve · {key}-click draws a straight line"), &[("key", &crate::shortcuts::pretty("Shift"))])
    } else {
        crate::i18n::fmt(tl!("Click to add a point · drag off or {key}-click or Delete to remove"), &[("key", &crate::shortcuts::pretty("Cmd"))])
    };
    let numbers = LocaleNumbers::of(ui);
    let sampler: &craft::ui::composites::Sampler = &sample_levels;
    let salt = cx.mem.with("curves-editor-salt");
    // A point the host added on the image (⌘/Ctrl-click) is the selected one.
    if let Some(i) = ui.data_mut(|d| d.remove_temp::<usize>(cx.mem.with("curves-select"))) {
        CurveEditor::select(ui.ctx(), CurveEditor::id_in(ui, salt), Some(i));
    }
    let mut editor = CurveEditor::new(salt, tl!("Curve"), &curve, &numbers)
        .channels(&channels)
        .channel(if isolated.is_some() { 0 } else { ch })
        .sampler(sampler)
        .side(curve_side(ui))
        .pencil(pencil)
        .overlays(&overlays)
        .hint(hint)
        .labels(craft::ui::composites::CurveLabels {
            input: tl!("Input:").into(),
            output: tl!("Output:").into(),
            point: tl!("Point {i} of {n}").into(),
            point_value: tl!("Input {x}, Output {y}").into(),
            corner: tl!("Corner").into(),
            corner_value: tl!("Input {x}, Output {y}, corner").into(),
            graph_value: tl!("{channel}, {n} points").into(),
            graph_points: tl!("{n} points").into(),
            graph_drawn: tl!("{channel}, drawn").into(),
            drawn: tl!("Drawn").into(),
            percent: tl!("{n}%").into(),
        });
    if let Some(h) = &histogram {
        editor = editor.histogram(h);
    }
    let shown = editor.show(ui);
    // The graph's value area, for the tests' pointer input and the canvas pick.
    let style = CurveEditor::style(&craft::ui::theme::current(ui.ctx()));
    ui.data_mut(|d| {
        d.insert_temp(cx.mem.with("curves-graph"), shown.response.rect.shrink(style.pad));
    });

    let before_id = cx.mem.with("curves-before");
    match shown.event {
        Some(EditEvent::Begin { .. }) => {
            ui.data_mut(|d| d.insert_temp(before_id, curve.clone()));
        }
        Some(EditEvent::Update { value, .. }) => {
            write_shape(v, key, space, &value);
            e.changed = true;
        }
        Some(EditEvent::Commit { value, changed, .. }) => {
            if changed {
                write_shape(v, key, space, &value);
                e.changed = true;
            }
            e.commit |= changed;
        }
        Some(EditEvent::Set { value, .. }) => {
            write_shape(v, key, space, &value);
            e.changed = true;
            e.commit = true;
        }
        Some(EditEvent::Cancel { .. }) => {
            if let Some(before) = ui.data(|d| d.get_temp::<Curve>(before_id)) {
                write_shape(v, key, space, &before);
                e.changed = true;
            }
        }
        _ => {}
    }
    e
}
