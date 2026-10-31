//! A label laid out on one row, cut with '…'. The tab strips themselves are the library's
//! `craft::ui::tab_strip::TabStrip`.

use egui::Ui;

/// A label laid out on one row no wider than `max_w`, cut with '…' when it doesn't fit.
pub fn elided(ui: &Ui, text: &str, font: egui::FontId, color: egui::Color32, max_w: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping { max_width: max_w.max(1.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    ui.painter().layout_job(job)
}
