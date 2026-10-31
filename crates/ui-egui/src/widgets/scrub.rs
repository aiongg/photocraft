use super::{Rect, Response, Ui};

/// A caption left for the control drawn next on its row (Photoshop's scrubby labels). Left by
/// [`scrub_label`], taken by the number field (which scrubs by it) or by a dropdown (which only
/// takes its name).
#[derive(Clone)]
struct Caption {
    pass: u64,
    label: Response,
    /// How far right of the label the control may start.
    max_gap: f32,
}

fn caption_id() -> egui::Id {
    egui::Id::unique("pc-label-scrub")
}

/// Makes `label` scrubby: dragging it moves the number field drawn right after it on the same row.
/// A label with no number field right after it stays a plain label, and names the control after it.
pub fn scrub_label(ui: &Ui, label: &Response) {
    let caption = Caption { pass: ui.ctx().cumulative_pass_nr(), label: label.clone(), max_gap: 24.0 };
    ui.data_mut(|d| d.insert_temp(caption_id(), caption));
}

/// [`scrub_label`] for a tool-options caption.
pub fn scrub_option_label(ui: &Ui, label: &Response) {
    scrub_label(ui, label);
}

/// The caption left for the control at `rect` this frame, if any, taken either way so it cannot
/// leak on to a later control. `adjacent` also requires the control to start within the caption's
/// gap, as a number field scrubbed by it must.
fn take(ui: &Ui, rect: Rect, adjacent: bool) -> Option<Caption> {
    let c = ui.data_mut(|d| {
        let c = d.get_temp::<Caption>(caption_id());
        d.remove::<Caption>(caption_id());
        c
    })?;
    let label = c.label.rect;
    // Only a caption on the control's own row names it; one left over from an earlier row does not.
    let same_row = rect.top() < label.bottom() && rect.bottom() > label.top();
    let gap = rect.left() - label.right();
    (c.pass == ui.ctx().cumulative_pass_nr() && same_row && (!adjacent || (gap >= -1.0 && gap <= c.max_gap))).then_some(c)
}

/// The caption that scrubs the number field at `rect`.
pub(super) fn take_scrub_label(ui: &Ui, rect: Rect) -> Option<Response> {
    take(ui, rect, true).map(|c| c.label)
}

/// The visible caption left for the control drawn next, for a control that does not scrub (a
/// dropdown) and takes only its name.
pub(super) fn take_label_scrub_name(ui: &Ui, rect: Rect) -> Option<egui::Id> {
    take(ui, rect, false).map(|c| c.label.id)
}
