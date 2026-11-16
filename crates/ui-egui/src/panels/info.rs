use craft::color::ui::{InfoColor, InfoPanel, InfoReadings, SamplerReadout};
use craft::ui::runtime::env::Label;

use super::{PhotocraftApp, Value, json};
use crate::canvas::document_display::CanvasHost;

/// The active document's pixel under the pointer, if the pointer is over the image.
pub(crate) fn hover_pixel(app: &PhotocraftApp) -> Option<(i32, i32)> {
    let size = app.session.active()?.doc.size;
    app.hover_doc.map(|p| (p[0].floor() as i32, p[1].floor() as i32)).filter(|(x, y)| *x >= 0 && *y >= 0 && *x < size.width as i32 && *y < size.height as i32)
}

/// The colour the Info panel reads at document pixel `(x, y)`, straight RGBA 0..1: averaged over
/// the Eyedropper's Sample Size over the box it reads at the view zoom, as in Photoshop (#1649,
/// #11751), and cached per pixel, revision, size and zoom.
pub(crate) fn pointer_sample(app: &mut PhotocraftApp, (x, y): (i32, i32)) -> Option<[f32; 4]> {
    let rev = app.session.active()?.revision;
    let sample_size = app.ui.tool_options.eyedropper.size.pixels();
    let zoom = app.current_zoom();
    let key = (x, y, rev, sample_size, zoom.to_bits());
    if let Some((k, v)) = app.info_sample
        && k == key
    {
        return Some(v);
    }
    let params = json!({"x": f64::from(x) + 0.5, "y": f64::from(y) + 0.5, "size": sample_size, "zoom": f64::from(zoom)});
    let v: Vec<f32> = serde_json::from_value(app.session.execute("document.sampleColor", params).ok()?).ok()?;
    let v = [*v.first()?, *v.get(1)?, *v.get(2)?, *v.get(3)?];
    app.info_sample = Some((key, v));
    Some(v)
}

/// The Info panel's W and H for a size of `wh` px (the selection's or the pending crop frame's):
/// both go through the ruler unit's formatter at `dpi`, percent of the document's `extent`.
pub(crate) fn wh_readout(ur: &photocraft_engine::prefs::UnitsAndRulers, dpi: f64, extent: [f64; 2], wh: [f64; 2]) -> [String; 2] {
    [ur.format(wh[0], dpi, extent[0]), ur.format(wh[1], dpi, extent[1])]
}

<<<<<<< HEAD
/// The 8-bit value of a channel read 0..1.
fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// What the Info panel shows now: the colour under the pointer (RGB and CMYK), its position, the selection's
/// or pending crop frame's size, the document size and the Color Sampler `samplers`. A guide being dragged
/// (#5728) shows its position on its own axis only, even when the pointer is outside the canvas. Without a
/// document the readings are empty, which the panel shows as its no-document notice.
pub(crate) fn readings<'a>(app: &mut PhotocraftApp, samplers: &'a [(SamplerReadout, [Label; 2])]) -> InfoReadings<'a> {
    let Some(st) = app.session.active() else { return InfoReadings::new() };
    let (size, origin, dpi, idx) = (st.doc.size, st.doc.ruler_origin, f64::from(st.doc.resolution_dpi), app.session.active_index());
=======
/// The Info panel's ruler cells (#11991), shown while the Ruler tool is active and a ruler is
/// drawn: the angle and length(s), and the point and size the X/Y and W/H cells show instead of
/// the pointer's and the selection's.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RulerRows {
    /// "A:" and then "L:" (a plain ruler) or "L1:" and "L2:" (a protractor), with their values.
    pub(crate) measure: Vec<(&'static str, String)>,
    /// The start point (a plain ruler) or the second arm's end (a protractor), in document pixels.
    pub(crate) point: [f64; 2],
    /// End minus start, signed and y down, for a plain ruler. A protractor's is not measured, so
    /// W/H keep showing what they show today.
    pub(crate) size: Option<[f64; 2]>,
}

/// [`RulerRows`] for the active document, or `None` with another tool or no ruler. Lengths are in
/// the ruler unit with 2 decimals and the angle has 1 decimal and "°"; the options bar's Use
/// Measurement Scale never changes them, as measured.
pub(crate) fn ruler_rows(app: &PhotocraftApp) -> Option<RulerRows> {
    if app.ui.tool != crate::state::Tool::Ruler {
        return None;
    }
    let doc = &app.session.active()?.doc;
    let r = doc.measurement.ruler?;
    // A unit scale: the Info panel reads pixels converted to the ruler unit, never the scale.
    let info = photocraft_engine::analysis_cmds::ruler_info(&r, &Default::default());
    let g = |k: &str| info[k].as_f64();
    let units = &app.session.prefs().units_and_rulers;
    let dpi = f64::from(doc.resolution_dpi);
    let length = |px: f64| {
        let v = units.rulers.from_px(px, dpi, f64::from(doc.size.width), units.point_size.per_inch());
        format!("{v:.2}")
    };
    let angle = format!("{:.1}°", g("angle")?);
    match r.protractor {
        None => Some(RulerRows { measure: vec![("A:", angle), ("L:", length(g("l1")?))], point: r.start, size: Some([g("w")?, g("h")?]) }),
        Some(q) => Some(RulerRows { measure: vec![("A:", angle), ("L1:", length(g("l1")?)), ("L2:", length(g("l2")?))], point: q, size: None }),
    }
}

/// Photoshop's Info panel: colour under the pointer (RGB and CMYK), position, selection size.
pub(super) fn info_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(st) = app.session.active() else {
        empty(ui, tl!("No document"));
        return;
    };
    let (size, origin) = (st.doc.size, st.doc.ruler_origin);
>>>>>>> upstream/main
    let cmyk_space = photocraft_compose::cmyk_space(&st.doc);
    let selection = st.doc.selection.as_ref().map(photocraft_compose::bounds::content_bounds).map(|r| [r.width() as f64, r.height() as f64]);
    // Positions and sizes in the ruler unit (Preferences › Units & Rulers).
    let units = app.session.prefs().units_and_rulers.clone();
    let extent = [f64::from(size.width), f64::from(size.height)];
    // With the Crop tool, W and H are the pending frame's, as in Photoshop (#3145).
<<<<<<< HEAD
    let wh = crate::crop_ui::info_size(app).or(selection);
    let guide = idx.and_then(|idx| craft::ui::canvas_controls::reading::running(&app.ctx(), CanvasHost::main(idx).canvas_id()));
    let pointer = hover_pixel(app);
    let rgba = pointer.and_then(|p| pointer_sample(app, p));
    let color = rgba.map(|c| {
        // Colour-managed CMYK in percent like Photoshop's readout, through the same conversion as the Color
        // Picker (#8101) and the Color Sampler readout (`sampler_cmds::device_cmyk`).
        let cmyk = photocraft_color::convert::with_cmyk_space(cmyk_space.as_ref(), || photocraft_engine::sampler_cmds::device_cmyk(c[0], c[1], c[2]));
        InfoColor { rgb: [byte(c[0]), byte(c[1]), byte(c[2])], cmyk: cmyk.map(|v| v.clamp(0, 100) as u8) }
    });
    // Measured from the rulers' zero point (#2955).
    let at = |axis: usize, px: f64| Label::from(units.format(px - origin[axis], dpi, extent[axis]));
    let position = match guide {
        Some(g) if g.along == craft::behavior::Axis::Y => Some([at(0, g.position), Label::Empty]),
        Some(g) => Some([Label::Empty, at(1, g.position)]),
        None => pointer.map(|p| [at(0, f64::from(p.0)), at(1, f64::from(p.1))]),
    };
    let extent_labels = wh.map(|s| wh_readout(&units, dpi, extent, s).map(Label::from));
    InfoReadings::new().color(color).position(position).extent(extent_labels).document(Some([size.width, size.height])).samplers(samplers)
}

/// The first `N` entries of the JSON array `v` as bytes; `None` when one is missing or not a byte.
fn channels<const N: usize>(v: &Value) -> Option<[u8; N]> {
    let list = v.as_array()?;
    let mut out = [0u8; N];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = u8::try_from(list.get(i)?.as_i64()?).ok()?;
    }
    Some(out)
}

/// The Color Sampler points (#1046) as the panel shows them: each with its position in the current ruler
/// unit and the same RGB/CMYK readout as the pointer (one composite pixel each). A point with no colour
/// to read is left out.
pub(crate) fn sampler_rows(app: &mut PhotocraftApp) -> Vec<(SamplerReadout, [Label; 2])> {
    let readouts = sampler_readouts(app);
    let Some(st) = app.session.active() else { return Vec::new() };
    let (size, dpi) = (st.doc.size, f64::from(st.doc.resolution_dpi));
    let units = &app.session.prefs().units_and_rulers;
    readouts
        .iter()
        .filter_map(|s| {
=======
    let crop = crate::crop_ui::info_size(app);
    let ruler = ruler_rows(app);
    let wh = ruler.as_ref().and_then(|r| r.size).or(crop).or(sel);
    let pos = hover_pixel(app);
    // While a guide is dragged (#5728), the Info panel shows its position in one axis only, even
    // when the pointer is outside the canvas (ruler drags do not update `hover_doc` there).
    let (guide_x, guide_y) = match app.guide_drag {
        Some(d) if d.vertical => (Some(d.pos), None),
        Some(d) => (None, Some(d.pos)),
        None => (None, None),
    };
    let rgba = pos.and_then(|p| pointer_sample(app, p));
    let mono = theme::mono(11.5);
    let row = |ui: &mut egui::Ui, k: &str, v: String| {
        ui.horizontal(|ui| {
            ui.add_sized([18.0, 16.0], egui::Label::new(RichText::new(k).color(t.text_dim).size(11.5)));
            ui.label(RichText::new(v).font(mono.clone()).color(t.text));
        });
    };
    ui.columns(2, |cols| {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as i32;
        let dash = || "—".to_string();
        row(&mut cols[0], "R:", rgba.map_or_else(dash, |c| q(c[0]).to_string()));
        row(&mut cols[0], "G:", rgba.map_or_else(dash, |c| q(c[1]).to_string()));
        row(&mut cols[0], "B:", rgba.map_or_else(dash, |c| q(c[2]).to_string()));
        // Colour-managed CMYK in percent like Photoshop's readout, through the same conversion as
        // the Color Picker (#8101) and the Color Sampler readout (`sampler_cmds::device_cmyk`).
        let cmyk =
            rgba.map(|c| photocraft_color::convert::with_cmyk_space(cmyk_space.as_ref(), || photocraft_engine::sampler_cmds::device_cmyk(c[0], c[1], c[2])));
        for (i, k) in ["C:", "M:", "Y:", "K:"].iter().enumerate() {
            row(&mut cols[1], k, cmyk.map_or_else(dash, |c| format!("{}%", c[i])));
        }
    });
    widgets::hairline(ui);
    // The Ruler tool's angle and length(s) (#11991).
    if let Some(r) = &ruler {
        ui.columns(2, |cols| {
            for (i, (k, v)) in r.measure.iter().enumerate() {
                row(&mut cols[usize::from(i > 0)], k, v.clone());
            }
        });
        widgets::hairline(ui);
    }
    // With a ruler, X/Y are its point instead of the pointer's (#11991).
    let (guide_x, guide_y) = match &ruler {
        Some(r) => (Some(r.point[0]), Some(r.point[1])),
        None => (guide_x, guide_y),
    };
    ui.columns(2, |cols| {
        let dash = || "—".to_string();
        // Measured from the rulers' zero point (#2955).
        row(&mut cols[0], "X:", guide_x.map(|p| fx(p - origin[0])).or_else(|| pos.map(|p| fx(p.0 as f64 - origin[0]))).unwrap_or_else(dash));
        row(&mut cols[0], "Y:", guide_y.map(|p| fy(p - origin[1])).or_else(|| pos.map(|p| fy(p.1 as f64 - origin[1]))).unwrap_or_else(dash));
        let [w, h] = wh.map_or_else(|| [dash(), dash()], |s| wh_readout(&units_wh, dpi, [size.width as f64, size.height as f64], s));
        row(&mut cols[1], "W:", w);
        row(&mut cols[1], "H:", h);
    });
    widgets::hairline(ui);
    ui.label(
        RichText::new(crate::i18n::fmt(tl!("Doc: {w} × {h} px"), &[("w", &size.width.to_string()), ("h", &size.height.to_string())]))
            .color(t.text_dim)
            .size(11.5),
    );
    // Color Sampler points (#1046): each under its number, with its position in the current ruler
    // unit and the same RGB/CMYK readout as the pointer (one composite pixel each).
    let samplers = sampler_readouts(app);
    if !samplers.is_empty() {
        widgets::hairline(ui);
        let cell = |ui: &mut egui::Ui, s: String| {
            ui.label(RichText::new(s).font(mono.clone()).color(t.text));
        };
        for s in samplers.iter() {
            let n = s["number"].as_i64().unwrap_or(0);
>>>>>>> upstream/main
            let at = s["position"]
                .as_array()
                .map_or((0.0, 0.0), |a| (a.first().and_then(Value::as_f64).unwrap_or(0.0), a.get(1).and_then(Value::as_f64).unwrap_or(0.0)));
            let (rgb, cmyk) = (channels::<3>(&s["rgb"])?, channels::<4>(&s["cmyk"])?);
            let number = usize::try_from(s["number"].as_i64().unwrap_or(0)).unwrap_or(0);
            // The point sits on its pixel's centre (#11725); the row names the pixel, as the pointer row does.
            let (x, y) = (at.0.floor(), at.1.floor());
            let labels = [Label::from(units.format(x, dpi, f64::from(size.width))), Label::from(units.format(y, dpi, f64::from(size.height)))];
            Some((SamplerReadout::from_values(number, [x as i64, y as i64], rgb, cmyk), labels))
        })
        .collect()
}

/// Photoshop's Info panel: colour under the pointer (RGB and CMYK), position, selection size.
pub(super) fn info_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let samplers = sampler_rows(app);
    let readings = readings(app, &samplers);
    InfoPanel::new(tl!("Info"), &readings).show(ui);
}

/// The Info panel's Color Sampler readouts and the (document, revision, Sample Size) they were
/// composited for.
pub(crate) type SamplerReadouts = ((photocraft_doc::DocId, u64, u32), std::sync::Arc<Vec<Value>>);

/// The Color Sampler readouts of the active document, composited once per revision (#4528):
/// adding, moving or deleting a sampler and every pixel edit bump the revision, so between them
/// the Info panel reuses them instead of compositing each point through the stack every frame.
/// They average over the Sample Size shared with the Eyedropper (#11732), which is part of the
/// key, so changing it re-reads every sampler at once.
pub(crate) fn sampler_readouts(app: &mut PhotocraftApp) -> std::sync::Arc<Vec<Value>> {
    let size = app.ui.tool_options.eyedropper.size.pixels();
    let Some(key) = app.session.active().map(|s| (s.doc.id, s.revision, size)) else { return std::sync::Arc::default() };
    if let Some((k, v)) = &app.info_samplers
        && *k == key
    {
        return v.clone();
    }
    let list = app.session.execute("view.colorSamplers.list", json!({"size": size})).ok();
    let v = std::sync::Arc::new(list.as_ref().and_then(|v| v["samplers"].as_array()).cloned().unwrap_or_default());
    app.info_samplers = Some((key, v.clone()));
    v
}

#[cfg(test)]
<<<<<<< HEAD
mod tests {
    use super::*;

    /// Composited once per revision: repeated frames share the readouts; adding a sampler or an
    /// edit recomputes them (#4528).
    #[test]
    fn sampler_readouts_are_reused_until_the_document_changes() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 32, "height": 32, "background": "white"})).unwrap();
        app.run("view.colorSamplers.add", json!({"x": 4, "y": 4})).unwrap();
        let a = sampler_readouts(&mut app);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0]["rgb"], json!([255, 255, 255]));
        assert!(std::sync::Arc::ptr_eq(&a, &sampler_readouts(&mut app)), "the next frame reuses them");
        app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
        let b = sampler_readouts(&mut app);
        assert!(!std::sync::Arc::ptr_eq(&a, &b));
        assert_eq!(b[0]["rgb"], json!([255, 0, 0]), "an edit shows");
        app.run("view.colorSamplers.add", json!({"x": 8, "y": 8})).unwrap();
        assert_eq!(sampler_readouts(&mut app).len(), 2, "so does a new sampler");
    }
}
=======
#[path = "info_ruler_tests.rs"]
mod info_ruler_tests;
>>>>>>> upstream/main
