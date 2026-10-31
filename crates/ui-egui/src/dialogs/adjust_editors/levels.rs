use super::curves::{ToneChannel, channel_color, channel_picker, tone_channels};
use super::{Edit, EditorCx, Value, json, round, space_of};
use craft::imaging::ui::{Levels, LevelsEditor, LevelsLabels};
use craft::ui::composites::HistogramData;
use craft::ui::edit::EditEvent;
use craft::ui::localized::LocaleNumbers;

// ---------------------------------------------------------------------------------------------
// Levels

type Lv = [f32; 5]; // in_black, gamma, in_white, out_black, out_white (0–255, gamma as is)

fn read_levels(v: &Value, key: &str) -> Lv {
    let o = if key.is_empty() { Some(v) } else { v.get(key) };
    let g = |k: &str, d: f32| o.and_then(|o| o.get(k)).and_then(Value::as_f64).map_or(d, |x| x as f32);
    [g("inBlack", 0.0), g("gamma", 1.0), g("inWhite", 255.0), g("outBlack", 0.0), g("outWhite", 255.0)]
}

fn write_levels(v: &mut Value, key: &str, l: &Lv) {
    let obj = json!({"inBlack": l[0].round(), "gamma": round(l[1], 0.01), "inWhite": l[2].round(), "outBlack": l[3].round(), "outWhite": l[4].round()});
    if key.is_empty() {
        if let (Some(dst), Value::Object(src)) = (v.as_object_mut(), obj) {
            dst.extend(src);
        }
    } else {
        v[key] = obj;
    }
}

/// Input level (0–255) where the gamma slider sits: the input that maps to 50% grey.
pub fn gamma_pos(black: f32, white: f32, gamma: f32) -> f32 {
    black + (white - black) * 0.5f32.powf(gamma)
}

pub fn gamma_from_pos(black: f32, white: f32, x: f32) -> f32 {
    let t = ((x - black) / (white - black).max(1.0)).clamp(0.01, 0.99);
    (t.ln() / 0.5f32.ln()).clamp(0.01, 9.99)
}

/// Black and white input levels clipping 0.1 % at each end of `h` (Photoshop's Auto).
pub fn auto_levels(h: &[u32; 256]) -> (f32, f32) {
    let total: u64 = h.iter().map(|x| u64::from(*x)).sum();
    let clip = total / 1000;
    let (mut acc, mut lo) = (0u64, 0usize);
    while lo < 253 && acc + u64::from(h[lo]) <= clip {
        acc += u64::from(h[lo]);
        lo += 1;
    }
    let (mut acc, mut hi) = (0u64, 255usize);
    while hi > lo + 2 && acc + u64::from(h[hi]) <= clip {
        acc += u64::from(h[hi]);
        hi -= 1;
    }
    (lo as f32, hi as f32)
}

pub(super) fn levels(ui: &mut egui::Ui, v: &mut Value, cx: &EditorCx) -> Edit {
    let chans = tone_channels(space_of(v), cx.gray);
    let mut ch = 0;
    let mut auto = false;
    ui.horizontal(|ui| {
        ch = channel_picker(ui, cx.mem.with("levels-ch"), chans, tl!("Channel:"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            auto = ui.add(craft::ui::controls::Button::new(tl!("Auto")).variant(craft::ui::controls::ButtonVariant::Secondary)).clicked();
        });
    });
    let chan = chans.get(ch).copied().unwrap_or(ToneChannel { key: "", label: "" });
    let mut l = read_levels(v, chan.key);
    let mut e = Edit::default();
    if auto && let Some(h) = &cx.hist {
        let (lo, hi) = auto_levels(&h[ch]);
        l = [lo, 1.0, hi, l[3], l[4]];
        write_levels(v, chan.key, &l);
        l = read_levels(v, chan.key);
        e = Edit { changed: true, commit: true, ..Default::default() };
    }
    let color = channel_color(chan.key);
    let levels =
        Levels { in_black: f64::from(l[0]), gamma: f64::from(l[1]), in_white: f64::from(l[2]), out_black: f64::from(l[3]), out_white: f64::from(l[4]) };
    let histogram = cx.hist.as_ref().and_then(|h| h.ch.get(ch)).map(|bins| HistogramData::tone(bins.as_slice(), color));
    let numbers = LocaleNumbers::of(ui);
    let labels = LevelsLabels {
        histogram: tl!("Histogram").into(),
        input: tl!("Input Levels").into(),
        output: tl!("Output Levels").into(),
        in_black: tl!("Black").into(),
        gamma: tl!("Gamma").into(),
        in_white: tl!("White").into(),
        out_black: tl!("Black").into(),
        out_white: tl!("White").into(),
    };
    let mut editor = LevelsEditor::new(cx.mem.with("levels-editor"), tl!("Levels"), &levels, &numbers).channel(color).value(tl!(chan.label)).labels(labels);
    if let Some(h) = &histogram {
        editor = editor.histogram(h);
    }
    let write =
        |v: &mut Value, n: &Levels| write_levels(v, chan.key, &[n.in_black as f32, n.gamma as f32, n.in_white as f32, n.out_black as f32, n.out_white as f32]);
    let before_id = cx.mem.with("levels-before");
    match editor.show(ui).event {
        Some(EditEvent::Begin { .. }) => {
            ui.data_mut(|d| d.insert_temp(before_id, l));
        }
        Some(EditEvent::Update { value, .. }) => {
            write(v, &value);
            e.changed = true;
        }
        Some(EditEvent::Commit { value, changed, .. }) => {
            if changed {
                write(v, &value);
                e.changed = true;
            }
            e.commit |= changed;
        }
        Some(EditEvent::Set { value, .. }) => {
            write(v, &value);
            e.changed = true;
            e.commit = true;
        }
        Some(EditEvent::Cancel { .. }) => {
            if let Some(before) = ui.data(|d| d.get_temp::<Lv>(before_id)) {
                write_levels(v, chan.key, &before);
                e.changed = true;
            }
        }
        _ => {}
    }
    e
}
