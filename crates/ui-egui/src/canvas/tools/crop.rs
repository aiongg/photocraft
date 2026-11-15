//! The Crop family's adapter: the framework's `CropToolPass` is the host's tool, and `finish`
//! hands its output to the one `image.crop` mapping. The tool itself lives at `app.canvas_tools.crop.tool`
//! (`CropHost`); this module keeps no state.

use craft::draw::ui::crop::{CropOutput, CropTool, CropToolPass};

use crate::PhotocraftApp;
use crate::crop_ui::CropRun;

/// The tool for one pass of the host: `tool` over this pass's `run`.
pub(crate) fn pass<'a>(tool: &'a mut CropTool, run: &'a CropRun, ctx: &'a egui::Context) -> CropToolPass<'a> {
    tool.pass(run.input(ctx))
}

/// Shows the Crop tool's frame in the canvas body at `rect`, on a pass of its own over the same tool, and applies what
/// it reported. The host's pass of this canvas runs after the body, so the tool is not lent to both at once.
pub(crate) fn show_frame(app: &mut PhotocraftApp, ui: &mut egui::Ui, rect: egui::Rect) {
    let Some(run) = CropRun::of(app) else { return };
    let mut tool = std::mem::take(&mut app.canvas_tools.crop.tool);
    let ctx = ui.ctx().clone();
    let mut pass = pass(&mut tool, &run, &ctx);
    pass.show_frame(ui, rect);
    let output = CropToolPass::finish(pass);
    app.canvas_tools.crop.tool = tool;
    finish(app, output);
}

/// Applies what the pass reported to the document and the crop options.
pub(crate) fn finish(app: &mut PhotocraftApp, output: CropOutput) {
    crate::crop_ui::finish(app, output);
}
