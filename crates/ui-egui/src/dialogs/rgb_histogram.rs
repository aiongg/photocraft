//! Camera Raw-style continuous RGB ribbons on craft-ui's [`Histogram`]. Raw bins remain exact;
//! the library filters them for display.
use craft_ui::composites::{Histogram, HistogramScale};
use photocraft_algo::histogram::RgbHistogram;

/// `h` as RGB ribbons. `typical` scales by the typical columns as Camera Raw does, so a shadow or
/// highlight spike runs off the top; otherwise the tallest interior column fits.
pub(crate) fn histogram(id: &str, label: &str, h: &RgbHistogram, typical: bool) -> Histogram {
    let [red, green, blue] = &h.channels;
    let histogram = Histogram::new(id, label).rgb([&red[..], &green[..], &blue[..]]);
    if typical { histogram.scale(HistogramScale::Typical) } else { histogram }
}
