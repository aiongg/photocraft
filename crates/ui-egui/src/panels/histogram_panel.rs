//! Histogram panel (Window › Histogram): Compact, Expanded and All Channels
//! views, a channel and a source selector, statistics for the level under the pointer or a
//! dragged tonal range, the cached-data warning and Uncached Refresh, and the adjusted histogram
//! over the original while an adjustment previews.
//!
//! Histograms are counted off the UI thread (`tone::compute_histograms_at`) for the document
//! revision, source and preview they describe, and kept until one of those changes; the panel
//! shows the last one with the cached-data warning until the new one lands.

use std::sync::Arc;

use craft_ui::composites::{ChannelColor, Histogram};
use egui::vec2;
use photocraft_color::ColorMode;
use photocraft_doc::adjust::ToneSpace;
use photocraft_doc::{DocId, Document, LayerContent};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::theme::Tokens;
use crate::tone::{self, HistSource, Histograms};

/// Where the panel options are kept between launches (`prefs.dialogs`).
const OPTIONS_PREF: &str = "histogramPanel";

/// The panel's view (its menu's Compact / Expanded / All Channels View).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistView {
    /// The histogram alone, of the entire image (Photoshop's default view).
    #[default]
    Compact,
    /// The histogram with the channel and source selectors, refresh and statistics.
    Expanded,
    /// Expanded, plus one histogram per colour channel (no alpha, spot or mask channels).
    AllChannels,
}

/// The Channel menu. Which entries a document offers depends on its colour mode
/// ([`channel_options`]); one it lacks shows as its composite.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistChannel {
    /// RGB, CMYK, Lab's Lightness or Gray.
    #[default]
    Composite,
    Red,
    Green,
    Blue,
    Cyan,
    Magenta,
    Yellow,
    Black,
    A,
    B,
    Luminosity,
    /// The colour channels overlaid in their colours.
    Colors,
}

/// What the histogram counts (the Source menu, shown for documents with several layers).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistSourceChoice {
    /// The composite of every layer.
    #[default]
    EntireImage,
    /// The selected layer on its own.
    SelectedLayer,
    /// The selected adjustment layer and every layer beneath it.
    AdjustmentComposite,
}

/// The panel options (its menu and selectors), kept between launches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HistogramPanelState {
    pub view: HistView,
    /// Kept when the view changes (Expanded ↔ All Channels); in All Channels View it picks only
    /// the top histogram.
    pub channel: HistChannel,
    /// Show Channels in Color: single channels in Expanded and All Channels View in their colour
    /// (Compact View always colours them; Colors is always in colour).
    pub show_channels_in_color: bool,
    pub show_statistics: bool,
    pub source: HistSourceChoice,
}

impl Default for HistogramPanelState {
    fn default() -> Self {
        Self {
            view: HistView::Compact,
            channel: HistChannel::Composite,
            show_channels_in_color: false,
            show_statistics: true,
            source: HistSourceChoice::EntireImage,
        }
    }
}

/// Saves the panel options with the preferences.
pub fn persist_options(app: &mut PhotocraftApp) {
    if let Ok(value) = serde_json::to_value(app.ui.histogram_panel) {
        app.session.prefs.edit(|p| {
            p.dialogs.insert(OPTIONS_PREF.into(), value);
        });
    }
}

/// Restores the saved panel options; unreadable ones keep the defaults.
pub fn restore_options(app: &mut PhotocraftApp) {
    if let Some(v) = app.session.prefs().dialogs.get(OPTIONS_PREF)
        && let Ok(state) = serde_json::from_value(v.clone())
    {
        app.ui.histogram_panel = state;
    }
}

// ---------------------------------------------------------------------------------------------
// Channels

/// The channels a document in `mode` offers, with their labels, in menu order.
pub fn channel_options(mode: ColorMode) -> Vec<(HistChannel, &'static str)> {
    use HistChannel::*;
    match tone_space(mode) {
        _ if gray(mode) => vec![(Composite, "Gray")],
        ToneSpace::Cmyk => vec![
            (Composite, "CMYK"),
            (Cyan, "Cyan"),
            (Magenta, "Magenta"),
            (Yellow, "Yellow"),
            (Black, "Black"),
            (Luminosity, "Luminosity"),
            (Colors, "Colors"),
        ],
        ToneSpace::Lab => vec![(Composite, "Lightness"), (A, "a"), (B, "b"), (Luminosity, "Luminosity")],
        ToneSpace::Rgb => vec![(Composite, "RGB"), (Red, "Red"), (Green, "Green"), (Blue, "Blue"), (Luminosity, "Luminosity"), (Colors, "Colors")],
    }
}

fn gray(mode: ColorMode) -> bool {
    matches!(mode, ColorMode::Grayscale | ColorMode::Bitmap | ColorMode::Duotone)
}

/// The space the panel counts a document of `mode` in.
pub fn tone_space(mode: ColorMode) -> ToneSpace {
    photocraft_engine::adjust_params::default_space(mode)
}

/// `want`, or the composite when a document of `mode` doesn't have it.
pub fn resolve_channel(mode: ColorMode, want: HistChannel) -> HistChannel {
    if channel_options(mode).iter().any(|(c, _)| *c == want) { want } else { HistChannel::Composite }
}

/// The single colour channels of a document of `mode` (All Channels View; Colors overlays them).
pub fn color_channels(mode: ColorMode) -> &'static [HistChannel] {
    use HistChannel::*;
    match tone_space(mode) {
        _ if gray(mode) => &[],
        ToneSpace::Cmyk => &[Cyan, Magenta, Yellow, Black],
        ToneSpace::Lab => &[A, B],
        ToneSpace::Rgb => &[Red, Green, Blue],
    }
}

/// The 256 bins of `channel` in `h` (counted in the space of a document of `mode`).
pub fn bins(h: &Histograms, mode: ColorMode, channel: HistChannel) -> &[u32; 256] {
    use HistChannel::*;
    let i = match (tone_space(mode), channel) {
        (_, Luminosity) => return &h.lum,
        (ToneSpace::Rgb, Red) | (ToneSpace::Cmyk, Cyan) | (ToneSpace::Lab, A) => 1,
        (ToneSpace::Rgb, Green) | (ToneSpace::Cmyk, Magenta) | (ToneSpace::Lab, B) => 2,
        (ToneSpace::Rgb, Blue) | (ToneSpace::Cmyk, Yellow) => 3,
        (ToneSpace::Cmyk, Black) => 4,
        _ => 0,
    };
    &h[i]
}

/// The label a channel is listed under in a document of `mode`.
pub fn channel_label(mode: ColorMode, channel: HistChannel) -> &'static str {
    channel_options(mode).into_iter().find(|(c, _)| *c == channel).map_or("", |(_, l)| l)
}

/// The id of a channel (`ui.inspect`, the serde name).
fn channel_id(channel: HistChannel) -> String {
    serde_json::to_value(channel).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

/// The library colour a single channel draws in: its own when colourised, else the neutral grey.
/// Lab a and b take magenta and yellow, the composite, Luminosity and CMYK Black the neutral.
pub fn channel_color(channel: HistChannel, colored: bool) -> ChannelColor {
    use HistChannel::*;
    if !colored {
        return ChannelColor::Neutral;
    }
    match channel {
        Red => ChannelColor::Red,
        Green => ChannelColor::Green,
        Blue => ChannelColor::Blue,
        Cyan => ChannelColor::Cyan,
        Magenta | A => ChannelColor::Magenta,
        Yellow | B => ChannelColor::Yellow,
        _ => ChannelColor::Neutral,
    }
}

/// The level of `channel` at a straight RGBA 0..1 pixel, as the histogram bins it (the Color
/// Sampler marks).
pub fn channel_level(mode: ColorMode, channel: HistChannel, rgba: [f32; 4]) -> u8 {
    use HistChannel::*;
    let rgb = [rgba[0], rgba[1], rgba[2]].map(|v| if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) });
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let lum = || q(rgb.iter().zip(tone::LUMINOSITY).map(|(v, w)| v * w).sum());
    match (tone_space(mode), channel) {
        (_, Luminosity) => lum(),
        (ToneSpace::Rgb, Red) => q(rgb[0]),
        (ToneSpace::Rgb, Green) => q(rgb[1]),
        (ToneSpace::Rgb, Blue) => q(rgb[2]),
        (ToneSpace::Rgb, _) => ((u16::from(q(rgb[0])) + u16::from(q(rgb[1])) + u16::from(q(rgb[2]))) / 3) as u8,
        (ToneSpace::Cmyk, c) => {
            let ink = photocraft_color::convert::rgb_to_cmyk(rgb);
            let v = match c {
                Cyan => ink[0],
                Magenta => ink[1],
                Yellow => ink[2],
                Black => ink[3],
                _ => ink.iter().sum::<f32>() / 4.0,
            };
            q(1.0 - v)
        }
        (ToneSpace::Lab, c) => {
            let l = photocraft_color::convert::srgb_to_lab(rgb);
            match c {
                A => q((l[1] + 128.0) / 255.0),
                B => q((l[2] + 128.0) / 255.0),
                _ => q(l[0] / 100.0),
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Statistics

/// The Histogram statistics. Mean, Std Dev, Median and Pixels describe the whole
/// histogram; Level, Count and Percentile the level under the pointer or the dragged range.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub mean: f64,
    pub std_dev: f64,
    pub median: u8,
    pub pixels: u64,
    /// The inspected levels, `lo..=hi` (one level when they are equal), if any.
    pub level: Option<[u8; 2]>,
    /// Pixels at the inspected levels.
    pub count: Option<u64>,
    /// Percent of the pixels at or below the inspected level; for a range, in it.
    pub percentile: Option<f64>,
    /// Percent of the pixels at 0 and at 255: shadow and highlight clipping.
    pub clipped: [f64; 2],
}

/// The statistics of `h`, inspecting `levels` (a level or a range, in either order). None when
/// nothing was counted.
pub fn stats(h: &[u32; 256], levels: Option<[u8; 2]>) -> Option<Stats> {
    let n: u64 = h.iter().map(|v| u64::from(*v)).sum();
    if n == 0 {
        return None;
    }
    let nf = n as f64;
    let mean = h.iter().enumerate().map(|(i, v)| i as f64 * f64::from(*v)).sum::<f64>() / nf;
    let var = h.iter().enumerate().map(|(i, v)| (i as f64 - mean).powi(2) * f64::from(*v)).sum::<f64>() / nf;
    let mut acc = 0u64;
    let median = h
        .iter()
        .position(|v| {
            acc += u64::from(*v);
            acc * 2 >= n
        })
        .unwrap_or(0) as u8;
    let level = levels.map(|[a, b]| [a.min(b), a.max(b)]);
    let sum = |lo: u8, hi: u8| h.get(usize::from(lo)..=usize::from(hi)).map_or(0, |s| s.iter().map(|v| u64::from(*v)).sum::<u64>());
    let count = level.map(|[lo, hi]| sum(lo, hi));
    let percentile = level.map(|[lo, hi]| 100.0 * if lo == hi { sum(0, hi) } else { sum(lo, hi) } as f64 / nf);
    let clipped = [100.0 * f64::from(h[0]) / nf, 100.0 * f64::from(h[255]) / nf];
    Some(Stats { mean, std_dev: var.sqrt(), median, pixels: n, level, count, percentile, clipped })
}

// ---------------------------------------------------------------------------------------------
// Computing and caching

/// What one panel histogram was counted from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistKey {
    pub doc: DocId,
    pub revision: u64,
    /// `tone::tag` of the source and space.
    pub source: u64,
    /// The adjustment preview's display key; 0 for the document itself.
    pub preview: u64,
    /// The highest cache level allowed (1: an Uncached Refresh).
    pub max_level: u32,
}

/// A counted panel histogram.
#[derive(Clone, Debug)]
pub struct PanelHist {
    pub key: HistKey,
    /// When it landed (ms), for throttling recounts while the document keeps changing.
    pub at: f64,
    pub hist: Arc<Histograms>,
}

/// A histogram being counted off the UI thread: its key, and the receiver of (key, ms, result).
pub(crate) type HistJob = (HistKey, std::sync::mpsc::Receiver<(HistKey, f64, Arc<Histograms>)>);

/// The source the panel counts for `choice` in `st`: Entire Image whenever the choice doesn't
/// apply (a single layer, no selected layer, or Adjustment Composite without an adjustment layer
/// selected).
pub fn resolve_source(st: &photocraft_engine::DocState, choice: HistSourceChoice) -> (HistSourceChoice, HistSource) {
    if st.doc.layer_count() < 2 {
        return (HistSourceChoice::EntireImage, tone::IMAGE);
    }
    let selected = st.active_layer.and_then(|id| st.doc.layer(id));
    match (choice, selected) {
        (HistSourceChoice::SelectedLayer, Some(l)) => (choice, HistSource::Layer(l.id)),
        (HistSourceChoice::AdjustmentComposite, Some(l)) if matches!(l.content, LayerContent::Adjustment(_)) => (choice, HistSource::UpTo(l.id)),
        _ => (HistSourceChoice::EntireImage, tone::IMAGE),
    }
}

/// Whether Adjustment Composite applies to `st` (an adjustment layer is selected).
fn adjustment_selected(st: &photocraft_engine::DocState) -> bool {
    st.active_layer.and_then(|id| st.doc.layer(id)).is_some_and(|l| matches!(l.content, LayerContent::Adjustment(_)))
}

/// Preferences › Performance › Cache Levels: the highest level the panel counts at by default.
fn max_cache_level(app: &PhotocraftApp) -> u32 {
    app.session.prefs().performance.cache_levels.clamp(1, tone::MAX_CACHE_LEVEL)
}

/// The source the panel counts: Compact View shows no Source menu and represents the entire image.
fn source_choice(opts: &HistogramPanelState) -> HistSourceChoice {
    if opts.view == HistView::Compact { HistSourceChoice::EntireImage } else { opts.source }
}

/// The key of the document's histogram the panel wants now, with the space and source it counts.
fn committed_key(app: &PhotocraftApp) -> Option<(HistKey, ToneSpace, HistSource)> {
    let st = app.session.active()?;
    let (doc, revision) = (st.doc.id, st.revision);
    let space = tone_space(st.doc.mode);
    let (_, source) = resolve_source(st, source_choice(&app.ui.histogram_panel));
    let max_level = if app.hist_uncached == Some((doc, revision)) { 1 } else { max_cache_level(app) };
    Some((HistKey { doc, revision, source: tone::tag(source, space), preview: 0, max_level }, space, source))
}

/// The document an open adjustment previews on the canvas (an Image › Adjustments dialog through
/// its preview layer, or an adjustment layer's Properties drag) and its display key.
fn adjustment_preview(app: &mut PhotocraftApp) -> Option<(Arc<Document>, u64)> {
    let idx = app.session.active_index()?;
    if let Some(shown) = crate::adjust_preview::display_doc(app, idx) {
        return Some(shown);
    }
    app.live_adjust.as_ref()?;
    let (doc, key) = crate::canvas::display_doc(app, idx);
    (key != 0).then_some((doc, key))
}

/// What the panel wants shown now: the document's histogram key, and the preview's document and
/// key while an adjustment previews.
struct Want {
    committed: HistKey,
    preview: Option<(Arc<Document>, HistKey)>,
    space: ToneSpace,
    source: HistSource,
}

fn want(app: &mut PhotocraftApp) -> Option<Want> {
    let preview = adjustment_preview(app);
    let (committed, space, source) = committed_key(app)?;
    let preview = preview.map(|(d, k)| (d, HistKey { preview: k | 1, ..committed }));
    Some(Want { committed, preview, space, source })
}

/// Brings the panel's histograms up to date as far as one frame should: lands a finished count,
/// keeps entries a view-only revision didn't invalidate, and starts the next count (at most one at
/// a time, at most every 250 ms per entry). `ctx` repaints when a worker delivers.
pub fn update(app: &mut PhotocraftApp, ctx: &egui::Context) {
    // A finished worker lands its histogram in the slot its key names. A worker that died
    // without sending clears the job so the next frame schedules a new one.
    if let Some((_, rx)) = &app.hist_job {
        match rx.try_recv() {
            Ok((key, ms, hist)) => {
                app.perf.span("histogram", ms);
                // egui's clock works on every target; `gpu_canvas::now_ms` is always 0 on wasm32.
                let entry = PanelHist { key, at: ctx.input(|i| i.time) * 1000.0, hist };
                if key.preview == 0 {
                    app.doc_hist = Some(entry);
                } else {
                    app.doc_hist_preview = Some(entry);
                }
                app.hist_job = None;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => app.hist_job = None,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }
    // A revision that changed no pixels (selecting a layer, say) keeps the histogram of the one
    // before (#125), and an Uncached Refresh with it.
    if let Some(st) = app.session.active()
        && tone::view_only(st)
    {
        let (doc, rev) = (st.doc.id, st.revision);
        if app.hist_uncached == Some((doc, rev.wrapping_sub(1))) {
            app.hist_uncached = Some((doc, rev));
        }
        if let Some(e) = app.doc_hist.as_mut()
            && e.key.doc == doc
            && e.key.revision.wrapping_add(1) == rev
        {
            e.key.revision = rev;
        }
    }
    let Some(want) = want(app) else { return };
    if want.preview.is_none() {
        app.doc_hist_preview = None;
    }
    if app.hist_job.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
        return;
    }
    let now = ctx.input(|i| i.time) * 1000.0;
    let due = |e: &Option<PanelHist>| e.as_ref().is_none_or(|e| e.key.doc != want.committed.doc || now - e.at > 250.0);
    let stale = |e: &Option<PanelHist>, k: &HistKey| e.as_ref().is_none_or(|e| e.key != *k);
    // An Uncached Refresh is asked for, so it doesn't wait out the throttle (Cache Levels 1 alone
    // counts at level 1 too, and still waits).
    let forced = app.hist_uncached == Some((want.committed.doc, want.committed.revision));
    let job = if stale(&app.doc_hist, &want.committed) {
        (forced || due(&app.doc_hist)).then(|| (app.session.active().map(|st| st.doc.clone()), want.committed))
    } else if let Some((doc, key)) = &want.preview
        && stale(&app.doc_hist_preview, key)
    {
        due(&app.doc_hist_preview).then(|| (Some(doc.clone()), *key))
    } else {
        None
    };
    match job {
        Some((Some(doc), key)) => spawn(app, ctx, doc, key, want.source, want.space),
        Some((None, _)) => {}
        None if stale(&app.doc_hist, &want.committed) || want.preview.as_ref().is_some_and(|(_, k)| stale(&app.doc_hist_preview, k)) => {
            ctx.request_repaint_after(std::time::Duration::from_millis(260));
        }
        None => {}
    }
}

/// Counts `doc` for `key` on a worker thread (inline on the web, where there are none, or when a
/// spawn fails), delivered through `app.hist_job`.
fn spawn(app: &mut PhotocraftApp, ctx: &egui::Context, doc: Arc<Document>, key: HistKey, source: HistSource, space: ToneSpace) {
    let (tx, rx) = std::sync::mpsc::channel();
    #[cfg(not(target_arch = "wasm32"))]
    let spawned = {
        let doc = doc.clone();
        let tx = tx.clone();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("histogram".into())
            .spawn(move || {
                let t0 = crate::gpu_canvas::now_ms();
                let h = Arc::new(tone::compute_histograms_at(&doc, source, space, key.max_level));
                let _ = tx.send((key, crate::gpu_canvas::now_ms() - t0, h));
                ctx.request_repaint();
            })
            .is_ok()
    };
    #[cfg(target_arch = "wasm32")]
    let spawned = {
        let _ = ctx;
        false
    };
    if !spawned {
        let t0 = crate::gpu_canvas::now_ms();
        let h = Arc::new(tone::compute_histograms_at(&doc, source, space, key.max_level));
        let _ = tx.send((key, crate::gpu_canvas::now_ms() - t0, h));
    }
    app.hist_job = Some((key, rx));
}

/// Uncached Refresh: recount the active document's current state at cache level 1 (every pixel).
pub fn uncached_refresh(app: &mut PhotocraftApp) {
    if let Some(st) = app.session.active() {
        app.hist_uncached = Some((st.doc.id, st.revision));
    }
}

/// The histogram the panel shows now: the preview's while an adjustment previews (with the
/// document's as the original), else the document's. `stale` when it isn't the one wanted yet.
struct Shown {
    hist: Arc<Histograms>,
    original: Option<Arc<Histograms>>,
    stale: bool,
    mode: ColorMode,
    layers: usize,
    adjustment: bool,
}

fn shown(app: &mut PhotocraftApp) -> Option<Shown> {
    let want = want(app)?;
    let st = app.session.active()?;
    let (mode, layers, adjustment) = (st.doc.mode, st.doc.layer_count(), adjustment_selected(st));
    let same_doc = |e: &PanelHist| e.key.doc == want.committed.doc;
    let committed = app.doc_hist.as_ref().filter(|e| same_doc(e));
    let preview = app.doc_hist_preview.as_ref().filter(|e| same_doc(e) && want.preview.is_some());
    Some(match (preview, committed) {
        (Some(p), c) => Shown {
            hist: p.hist.clone(),
            original: c.map(|c| c.hist.clone()),
            stale: want.preview.as_ref().is_none_or(|(_, k)| *k != p.key),
            mode,
            layers,
            adjustment,
        },
        (None, Some(c)) => Shown { hist: c.hist.clone(), original: None, stale: c.key != want.committed || want.preview.is_some(), mode, layers, adjustment },
        (None, None) => return None,
    })
}

// ---------------------------------------------------------------------------------------------
// Drawing

/// `channel` of `h` as a library [`Histogram`] (Colors overlays the colour channels: added light
/// in RGB, multiplied inks in CMYK).
fn plot(id: &str, h: &Histograms, mode: ColorMode, channel: HistChannel, colored: bool) -> Histogram {
    let histogram = Histogram::new(id, tl!("Histogram")).value(tl!(channel_label(mode, channel)));
    let of = |c| &bins(h, mode, c)[..];
    match (channel, tone_space(mode)) {
        (HistChannel::Colors, ToneSpace::Rgb) => histogram.rgb([of(HistChannel::Red), of(HistChannel::Green), of(HistChannel::Blue)]),
        (HistChannel::Colors, ToneSpace::Cmyk) => histogram.channels(&[
            (of(HistChannel::Cyan), ChannelColor::Cyan),
            (of(HistChannel::Magenta), ChannelColor::Magenta),
            (of(HistChannel::Yellow), ChannelColor::Yellow),
            (of(HistChannel::Black), ChannelColor::Neutral),
        ]),
        _ => histogram.tone(of(channel), channel_color(channel, colored)),
    }
}

/// The histogram graph with its marks and pointer interaction. Returns the levels under the
/// pointer (hover) for the statistics.
fn graph(app: &mut PhotocraftApp, ui: &mut egui::Ui, s: &Shown, channel: HistChannel, colored: bool, height: f32) -> Option<u8> {
    let mark_channel = if channel == HistChannel::Colors { HistChannel::Composite } else { channel };
    // The Color Samplers' levels.
    let marks: Vec<(f32, String)> = crate::panels::sampler_readouts(app)
        .iter()
        .filter_map(|v| {
            let rgb: Vec<f32> = v["rgb"].as_array()?.iter().filter_map(Value::as_f64).map(|c| (c / 255.0) as f32).collect();
            let level = channel_level(s.mode, mark_channel, [*rgb.first()?, *rgb.get(1)?, *rgb.get(2)?, 1.0]);
            Some((f32::from(level), v["number"].to_string()))
        })
        .collect();
    let mut histogram = plot("hist-graph", &s.hist, s.mode, channel, colored)
        .height(height)
        .selection(app.ui.histogram_range.map(|[a, b]| f32::from(a.min(b))..=f32::from(a.max(b))))
        .markers(&marks)
        .selection_labels(tl!("Selection start"), tl!("Selection end"))
        .stale(s.stale || s.hist.level > 1)
        .on_stale_click(tl!("Click to refresh histogram with uncached data"));
    // The original histogram outlined over the adjusted one while an adjustment previews.
    if let Some(o) = &s.original {
        histogram = histogram.outline(&bins(o, s.mode, mark_channel)[..], ChannelColor::Neutral);
    }
    let shown = histogram.show(ui);
    if shown.clicked || shown.double_clicked() {
        uncached_refresh(app);
    }
    if let Some(range) = shown.selection {
        app.ui.histogram_range = range.map(|r| [r.start().round().clamp(0.0, 255.0) as u8, r.end().round().clamp(0.0, 255.0) as u8]);
    }
    shown.hovered_level.map(|l| l.round().clamp(0.0, 255.0) as u8)
}

/// The statistics grid: Mean, Std Dev, Median, Pixels | Level, Count, Percentile, Cache Level.
fn statistics(ui: &mut egui::Ui, h: &Histograms, mode: ColorMode, channel: HistChannel, levels: Option<[u8; 2]>, t: &Tokens) {
    let ch = if channel == HistChannel::Colors { HistChannel::Composite } else { channel };
    let Some(s) = stats(bins(h, mode, ch), levels) else { return };
    let level = match s.level {
        Some([a, b]) if a == b => a.to_string(),
        Some([a, b]) => format!("{a}..{b}"),
        None => String::new(),
    };
    let rows = [
        (tl!("Mean:"), format!("{:.2}", s.mean), tl!("Level:"), level),
        (tl!("Std Dev:"), format!("{:.2}", s.std_dev), tl!("Count:"), s.count.map(|c| c.to_string()).unwrap_or_default()),
        (tl!("Median:"), s.median.to_string(), tl!("Percentile:"), s.percentile.map(|p| format!("{p:.2}")).unwrap_or_default()),
        (tl!("Pixels:"), s.pixels.to_string(), tl!("Cache Level:"), h.level.max(1).to_string()),
    ];
    let mono = crate::theme::mono(11.5);
    egui::Grid::new("hist-stats").num_columns(4).spacing(vec2(6.0, 2.0)).show(ui, |ui| {
        for (k1, v1, k2, v2) in rows {
            for (k, v) in [(k1, v1), (k2, v2)] {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(k).color(t.text_dim).size(11.5));
                });
                ui.label(egui::RichText::new(v).font(mono.clone()).color(t.text));
            }
            ui.end_row();
        }
    });
}

/// Histogram panel (Window › Histogram).
pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    if app.session.active().is_none() {
        ui.label(egui::RichText::new(tl!("No document")).color(t.text_faint));
        return;
    }
    update(app, ui.ctx());
    let Some(s) = shown(app) else {
        ui.label(egui::RichText::new(tl!("Calculating…")).color(t.text_faint));
        return;
    };
    let opts = app.ui.histogram_panel;
    let channel = resolve_channel(s.mode, opts.channel);
    if opts.view == HistView::Compact {
        // Compact View: the graph alone; a single channel is always in its colour.
        graph(app, ui, &s, channel, true, 100.0);
        return;
    }
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!("Channel:")).color(t.text_dim).size(12.0));
        let mut ch = channel;
        let options = channel_options(s.mode);
        if crate::widgets::dropdown(ui, "histogram-panel-ch", tl!("Channel"), &mut ch, &options, 110.0) {
            app.ui.histogram_panel.channel = ch;
            changed = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::icons::button(ui, "rotate-cw", 20.0, false, tl!("Uncached Refresh")).clicked() {
                uncached_refresh(app);
            }
        });
    });
    let colored = opts.show_channels_in_color;
    let hover = graph(app, ui, &s, channel, colored, 100.0);
    // Source: only for documents with several layers.
    if s.layers > 1 {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tl!("Source:")).color(t.text_dim).size(12.0));
            let mut src = opts.source;
            let mut options = vec![(HistSourceChoice::EntireImage, "Entire Image"), (HistSourceChoice::SelectedLayer, "Selected Layer")];
            if s.adjustment {
                options.push((HistSourceChoice::AdjustmentComposite, "Adjustment Composite"));
            } else if src == HistSourceChoice::AdjustmentComposite {
                src = HistSourceChoice::EntireImage;
            }
            if crate::widgets::dropdown(ui, "histogram-panel-src", tl!("Source"), &mut src, &options, 150.0) {
                app.ui.histogram_panel.source = src;
                changed = true;
            }
        });
    }
    if opts.show_statistics {
        ui.add_space(craft_ui::theme::scale::Space::S4.px());
        let levels = app.ui.histogram_range.or_else(|| hover.map(|l| [l, l]));
        statistics(ui, &s.hist, s.mode, channel, levels, &t);
    }
    if opts.view == HistView::AllChannels {
        for c in color_channels(s.mode) {
            ui.add_space(craft_ui::theme::scale::Space::S4.px());
            ui.label(egui::RichText::new(tl!(channel_label(s.mode, *c))).color(t.text).size(11.5));
            plot(&format!("hist-all-{}", channel_id(*c)), &s.hist, s.mode, *c, colored).height(90.0).show(ui);
        }
    }
    if changed {
        persist_options(app);
    }
}

/// The Histogram items of the panel menu.
pub fn panel_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if ui.add_enabled(app.session.active().is_some(), egui::Button::new(tl!("Uncached Refresh"))).clicked() {
        uncached_refresh(app);
        ui.close();
    }
    ui.separator();
    let mut changed = false;
    for (view, label) in [(HistView::Compact, "Compact View"), (HistView::Expanded, "Expanded View"), (HistView::AllChannels, "All Channels View")] {
        if ui.radio(app.ui.histogram_panel.view == view, tl!(label)).clicked() {
            // The channel is one setting for every view, so it is kept.
            app.ui.histogram_panel.view = view;
            changed = true;
        }
    }
    ui.separator();
    let mut stats_on = app.ui.histogram_panel.show_statistics;
    if crate::widgets::checkbox(ui, &mut stats_on, tl!("Show Statistics")).changed() {
        app.ui.histogram_panel.show_statistics = stats_on;
        changed = true;
    }
    let mut color = app.ui.histogram_panel.show_channels_in_color;
    if crate::widgets::checkbox(ui, &mut color, tl!("Show Channels in Color")).changed() {
        app.ui.histogram_panel.show_channels_in_color = color;
        changed = true;
    }
    if changed {
        persist_options(app);
        ui.close();
    }
}

/// The panel's state for `ui.inspect` (`histogram`): its options as they apply to the active
/// document, and the shown histogram's statistics for the selected range.
pub fn inspect(app: &PhotocraftApp) -> Value {
    let opts = app.ui.histogram_panel;
    let Some(st) = app.session.active() else { return json!({"options": opts}) };
    let mode = st.doc.mode;
    let channel = resolve_channel(mode, opts.channel);
    let (source, _) = resolve_source(st, source_choice(&opts));
    let preview = app.doc_hist_preview.as_ref().filter(|e| e.key.doc == st.doc.id);
    let entry = preview.or(app.doc_hist.as_ref()).filter(|e| e.key.doc == st.doc.id);
    let committed = committed_key(app).map(|(k, _, _)| k);
    let ch = if channel == HistChannel::Colors { HistChannel::Composite } else { channel };
    json!({
        "options": opts,
        "channel": channel_id(channel),
        "channels": channel_options(mode).iter().map(|(c, _)| channel_id(*c)).collect::<Vec<_>>(),
        "source": source,
        "sourceShown": st.doc.layer_count() > 1 && opts.view != HistView::Compact,
        "range": app.ui.histogram_range,
        "preview": preview.is_some(),
        "revision": entry.map(|e| e.key.revision),
        "current": entry.zip(committed).is_some_and(|(e, k)| e.key == HistKey { preview: e.key.preview, ..k }),
        "cacheLevel": entry.map(|e| e.hist.level.max(1)),
        "maxCacheLevel": max_cache_level(app),
        "stats": entry.and_then(|e| stats(bins(&e.hist, mode, ch), app.ui.histogram_range)),
    })
}

#[cfg(test)]
mod tests;
