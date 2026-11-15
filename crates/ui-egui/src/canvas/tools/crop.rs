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

/// Applies what the pass reported to the document and the crop options.
pub(crate) fn finish(app: &mut PhotocraftApp, output: CropOutput) {
    crate::crop_ui::finish(app, output);
}
