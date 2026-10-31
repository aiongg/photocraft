use super::basic::{sliders, track_stops};
use super::{Color32, Edit, EditorCx, HUE_RANGES, HueRange, Rect, Row, Value, apply_event, color32, flag, hue_color, json, num, nums, pos2, vec2, widgets};
use craft::theme::{Srgba, TypeRole};
use craft::draw::ui::gradient::GradientRamp;
use craft::ui::controls::{Thumb, ThumbRow};
use craft::ui::primitives::{Content, Text};
use craft::ui::theme::{ToEgui, Tokens as LibTokens, TokensOf as _};

// ---------------------------------------------------------------------------------------------
// Hue/Saturation

pub(super) const RANGE_LABELS: [&str; 6] = ["Reds", "Yellows", "Greens", "Cyans", "Blues", "Magentas"];

/// The Saturation slider's track: grey to the hue at full saturation (UI-217-16).
pub(super) fn saturation_track(hue_deg: f32) -> [Color32; 2] {
    [Color32::from_gray(128), hue_color(hue_deg)]
}

/// The Lightness slider's track: black, mid grey, white, whatever the hue, as in Photoshop
/// (UI-217-16, #6539).
pub(super) fn lightness_track() -> [Color32; 3] {
    [Color32::BLACK, Color32::from_gray(128), Color32::WHITE]
}

pub(super) fn move_hue_range_handle(bounds: [f32; 4], k: usize, deg: f32) -> [f32; 4] {
    let mut b = HueRange::canonical_bounds(bounds);
    if k >= b.len() {
        return b;
    }
    if k == 0 {
        let delta = (deg - b[0] + 180.0).rem_euclid(360.0) - 180.0;
        b[0] = (b[0] + delta).min(b[1]).max(b[1] - 180.0);
    } else {
        let rel = (deg - b[0]).rem_euclid(360.0);
        let hi = if k == 3 { b[0] + 359.0 } else { b[k + 1] };
        b[k] = (b[0] + rel).clamp(b[k - 1], hi);
    }
    b
}

/// The spectrum bar after the adjustment (what each hue becomes), sampled at `n` hues.
fn adjusted_spectrum(v: &Value, n: usize) -> Vec<Color32> {
    let adj = photocraft_engine::commands::adjustment_from_params("hueSaturation", v);
    let rect = photocraft_geom::Rect::new(0, 0, n as i32, 1);
    let px = (0..n)
        .map(|i| {
            let c = photocraft_color::convert::hsb_to_rgb([i as f32 / (n - 1).max(1) as f32 * 360.0, 1.0, 1.0]);
            [c[0], c[1], c[2], 1.0]
        })
        .collect();
    let mut buf = photocraft_compose::Buffer { rect, px };
    photocraft_compose::adjust::apply(&adj, &mut buf);
    buf.px.iter().map(|p| color32([p[0], p[1], p[2]])).collect()
}

/// A spectrum bar of `colors`, spread evenly across `r`.
fn spectrum_bar(ui: &egui::Ui, r: Rect, colors: &[Color32]) {
    let last = colors.len().saturating_sub(1).max(1) as f32;
    let stops: Vec<(f32, Srgba)> = colors.iter().enumerate().map(|(i, c)| (i as f32 / last, Srgba::from(*c))).collect();
    GradientRamp::paint_colors(ui.painter(), r, &stops, &LibTokens::of(ui));
}

pub(super) fn hue_saturation(ui: &mut egui::Ui, v: &mut Value, cx: &EditorCx) -> Edit {
    let t = craft::ui::theme::current(ui.ctx());
    let range_id = cx.mem.with("hs-range");
    let mut range: usize = ui.data(|d| d.get_temp(range_id)).unwrap_or(0);
    let colorize = flag(v, "colorize", false);
    if colorize {
        range = 0;
    }
    ui.horizontal(|ui| {
        let opts: Vec<(usize, &str)> = std::iter::once((0, tl!("Master"))).chain(RANGE_LABELS.iter().enumerate().map(|(i, l)| (i + 1, *l))).collect();
        ui.add_enabled_ui(!colorize, |ui| {
            craft::ui::controls::Select::new(&format!("{range_id:?}"), tl!("Range"), &mut range)
                .rows(opts.len(), |i| opts[i].0.clone(), |i| tl!(opts[i].1))
                .show(ui)
                .response
                .changed()
        });
    });
    ui.data_mut(|d| d.insert_temp(range_id, range));
    let mut e = Edit::default();
    let hue_grad = widgets::hue_stops();
    if range == 0 {
        let (hlo, hhi, slo) = if colorize { (0.0, 360.0, 0.0) } else { (-180.0, 180.0, -100.0) };
        let mut h = num(v, "hue", 0.0).clamp(hlo, hhi);
        let r = Row::new(tl!("Hue"), hlo..=hhi).unit("°").track(track_stops(cx, &hue_grad)).show(ui, &mut h);
        if r.changed {
            v["hue"] = json!(h.round());
        }
        e.add(r);
        // The Saturation track follows the current hue; the Lightness track is a fixed
        // black-grey-white ramp, as in Photoshop (UI-217-16).
        let sat_track = saturation_track(h);
        let mut s = num(v, "saturation", 0.0).clamp(slo, 100.0);
        let r = Row::new(tl!("Saturation"), slo..=100.0).unit("%").track(track_stops(cx, &sat_track)).show(ui, &mut s);
        if r.changed {
            v["saturation"] = json!(s.round());
        }
        e.add(r);
        let light_track = lightness_track();
        let mut l = num(v, "lightness", 0.0).clamp(-100.0, 100.0);
        let r = Row::new(tl!("Lightness"), -100.0..=100.0).unit("%").track(track_stops(cx, &light_track)).show(ui, &mut l);
        if r.changed {
            v["lightness"] = json!(l.round());
        }
        e.add(r);
    } else {
        let key = HUE_RANGES[range - 1];
        if !v.get(key).is_some_and(Value::is_object) {
            v[key] = json!({});
        }
        let mut o = v[key].clone();
        let mut sub = Edit::default();
        let mut h = num(&o, "hue", 0.0);
        let r = Row::new(tl!("Hue"), -180.0..=180.0).unit("°").track(track_stops(cx, &hue_grad)).show(ui, &mut h);
        if r.changed {
            o["hue"] = json!(h.round());
        }
        sub.add(r);
        sub.add(sliders(
            ui,
            &mut o,
            &[("saturation", tl!("Saturation"), -100.0, 100.0, 0.0, "%", None), ("lightness", tl!("Lightness"), -100.0, 100.0, 0.0, "%", None)],
        ));
        if sub.changed {
            v[key] = o;
        }
        e.add(sub);
    }
    let mut c = colorize;
    if craft::ui::controls::Checkbox::bool(tl!("Colorize"), &mut c).show(ui).response.changed() {
        v["colorize"] = json!(c);
        let h = num(v, "hue", 0.0);
        v["hue"] = json!(if c {
            h.rem_euclid(360.0)
        } else if h > 180.0 {
            h - 360.0
        } else {
            h
        });
        if c {
            v["saturation"] = json!(num(v, "saturation", 25.0).max(0.0).max(25.0));
        }
        e.add(Edit::discrete(true));
    }
    // Spectrum bars: input hues above, adjusted below; the selected range's thumbs between.
    ui.add_space(craft::ui::theme::scale::Space::S6.px());
    // As wide as the sliders above, whatever the panel's width (#9242).
    let w = ui.available_width();
    let tokens = LibTokens::of(ui);
    let style = ThumbRow::<4>::style(&tokens);
    let bar = 10.0;
    let height = if range > 0 { bar * 2.0 + style.height } else { bar * 2.0 + 20.0 };
    let (_, bars) = ui.allocate_space(vec2(w, height));
    ui.data_mut(|d| d.insert_temp(cx.mem.with("hs-bars"), bars));
    let input: Vec<Color32> = (0..=36).map(|i| hue_color(i as f32 * 10.0)).collect();
    spectrum_bar(ui, Rect::from_min_size(bars.min, vec2(w, bar)), &input);
    spectrum_bar(ui, Rect::from_min_size(pos2(bars.left(), bars.bottom() - bar), vec2(w, bar)), &adjusted_spectrum(v, 37));
    let middle = Rect::from_min_max(pos2(bars.left(), bars.top() + bar), pos2(bars.right(), bars.bottom() - bar));
    if range > 0 {
        let key = HUE_RANGES[range - 1];
        let neutral = photocraft_doc::adjust::HueRange::neutral(range - 1).bounds;
        let b = HueRange::canonical_bounds(nums(&v[key], "range", neutral));
        // Range band (fully affected) and fall-off, drawn along the top of the thumbs.
        let band_rect = Rect::from_min_size(middle.min, vec2(w, 6.0));
        let xof = |deg: f32| band_rect.left() + deg.rem_euclid(360.0) / 360.0 * w;
        let band = |a: f32, z: f32, color: Color32| {
            let (xa, xz) = (xof(a), xof(z));
            let strip = |x0: f32, x1: f32| Rect::from_min_max(pos2(x0, band_rect.top()), pos2(x1, band_rect.bottom()));
            let p = ui.painter();
            if xa <= xz {
                p.rect_filled(strip(xa, xz), craft::ui::theme::scale::Radius::R0.px(), color);
            } else {
                p.rect_filled(strip(xa, band_rect.right()), craft::ui::theme::scale::Radius::R0.px(), color);
                p.rect_filled(strip(band_rect.left(), xz), craft::ui::theme::scale::Radius::R0.px(), color);
            }
        };
        band(b[0], b[3], t.color.text.faint.to_egui());
        band(b[1], b[2], t.color.text.muted.to_egui());
        let faces = [t.color.surface.sunken, t.color.text.normal, t.color.text.normal, t.color.surface.sunken].map(|c| c.to_egui());
        let names = [tl!("Range start"), tl!("Fully affected start"), tl!("Fully affected end"), tl!("Range end")];
        let thumbs = std::array::from_fn(|k| Thumb::new(names[k], faces[k]));
        let shown = b.map(|d| f64::from(d.rem_euclid(360.0)));
        let row = ThumbRow::new(cx.mem.with("hs-range-row"), shown, &(0.0..=360.0), Some(1.0), thumbs).wrap_from(360.0, 0, 359.0).width(w);
        let out = row.show(&mut ui.new_child(egui::UiBuilder::new().max_rect(middle)), &style);
        let mut moved = b;
        let r = apply_event(ui, cx.mem.with("hs-range-before"), out.edit.event, &mut moved, |value| {
            let k = (0..4).find(|&k| (value[k] - shown[k]).abs() > 1e-6);
            k.map_or(b, |k| move_hue_range_handle(b, k, value[k] as f32))
        });
        if moved != b {
            v[key]["range"] = json!(moved.map(f32::round));
        }
        e.add(r);
        let b = moved;
        let lab =
            format!("{:.0}° / {:.0}°  ·  {:.0}° \\ {:.0}°", b[0].rem_euclid(360.0), b[1].rem_euclid(360.0), b[2].rem_euclid(360.0), b[3].rem_euclid(360.0));
        ui.add(Text::new(lab).role(TypeRole::Mono).content(Content::Faint));
    }
    e
}
