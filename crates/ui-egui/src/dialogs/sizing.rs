//! Image Size and Canvas Size on the library's [`ImageSizeDialog`] and [`CanvasSizeDialog`], shown by
//! the `DialogHost`.
//!
//! The app opens the dialog with the document's size, resolution and the preferences, keeps a
//! [`Hosted`] record for automation, and runs `image.imageSize` or `image.canvasSize` with the
//! answer. The library dialogs hold their own state: a script's `ui.dialog.set` values ride along
//! in the record's `extra` and win over the dialog's own at OK (gap Z2).

use craft::ui::dialog::{Anchor, CanvasExtension, CanvasSizeDialog, CanvasSizeResult, ImageSizeDialog, ImageSizeResult, PixelSize, Resample, StackDialog};
use serde_json::{Map, Value, json};

use super::hosted::{Hosted, HostedKind, Pending};
use crate::PhotocraftApp;
use crate::state::UiState;

type Fields = Map<String, Value>;

/// Image Size resampling for Preferences › General › Image Interpolation.
pub(crate) fn pref_resample(i: photocraft_engine::prefs::Interpolation) -> &'static str {
    use photocraft_engine::prefs::Interpolation;
    match i {
        Interpolation::BicubicAutomatic => "automatic",
        Interpolation::Nearest => "nearest",
        Interpolation::Bilinear => "bilinear",
        Interpolation::Bicubic => "bicubic",
        Interpolation::BicubicSmoother => "bicubicSmoother",
        Interpolation::BicubicSharper => "bicubicSharper",
        Interpolation::PreserveDetails => "preserveDetails",
    }
}
pub fn is_sizing(command: &str) -> bool {
    matches!(command, "image.imageSize" | "image.canvasSize")
}

fn resample_of(name: &str) -> Resample {
    match name {
        "none" => Resample::None,
        "preserveDetails" => Resample::PreserveDetails,
        "bicubic" => Resample::Bicubic,
        "bicubicSmoother" => Resample::BicubicSmoother,
        "bicubicSharper" => Resample::BicubicSharper,
        "bilinear" => Resample::Bilinear,
        "nearest" => Resample::NearestNeighbor,
        "lanczos" => Resample::Lanczos,
        _ => Resample::Automatic,
    }
}

fn resample_name(method: Resample) -> &'static str {
    match method {
        Resample::None => "none",
        Resample::PreserveDetails => "preserveDetails",
        Resample::Bicubic => "bicubic",
        Resample::BicubicSmoother => "bicubicSmoother",
        Resample::BicubicSharper => "bicubicSharper",
        Resample::Bilinear => "bilinear",
        Resample::NearestNeighbor => "nearest",
        Resample::Lanczos => "lanczos",
        _ => "automatic",
    }
}

fn anchor_name(anchor: Anchor) -> &'static str {
    match anchor {
        Anchor::TopLeft => "topLeft",
        Anchor::Top => "top",
        Anchor::TopRight => "topRight",
        Anchor::Left => "left",
        Anchor::Center => "center",
        Anchor::Right => "right",
        Anchor::BottomLeft => "bottomLeft",
        Anchor::Bottom => "bottom",
        Anchor::BottomRight => "bottomRight",
    }
}

/// The `extensionColor` parameter of Canvas Size. The command has no Gray: it is a colour.
fn extension_name(extension: &CanvasExtension) -> String {
    match extension {
        CanvasExtension::Foreground => "foreground".into(),
        CanvasExtension::Background => "background".into(),
        CanvasExtension::White => "white".into(),
        CanvasExtension::Black => "black".into(),
        CanvasExtension::Gray => "#808080".into(),
        CanvasExtension::Other(rgb) => crate::color_picker_ui::hex(*rgb),
        CanvasExtension::Transparent => "transparent".into(),
    }
}

fn size_params(size: PixelSize) -> Fields {
    let mut f = Fields::new();
    f.insert("width".into(), json!(size.width));
    f.insert("height".into(), json!(size.height));
    f
}

fn image_params(result: &ImageSizeResult) -> Fields {
    let mut f = size_params(result.size);
    f.insert("resolution".into(), json!(result.size.pixels_per_inch));
    f.insert("resample".into(), json!(resample_name(result.resample)));
    f
}

fn canvas_params(result: &CanvasSizeResult) -> Fields {
    let mut f = size_params(result.size);
    f.insert("relative".into(), json!(false));
    f.insert("anchor".into(), json!(anchor_name(result.anchor)));
    f.insert("extensionColor".into(), json!(extension_name(&result.extension)));
    f
}

/// The values of an open size dialog as `ui.dialog.*` and the command see them: what the dialog's
/// size fields hold now, the options it opened with, and the values a script set.
pub fn fields_of(ui: &UiState, record: &Hosted) -> Option<Fields> {
    let mut f = match record.kind {
        HostedKind::ImageSize => {
            let fields = ui.host.get::<ImageSizeDialog>()?.fields();
            let mut f = size_params(fields.current);
            f.insert("resolution".into(), json!(fields.current.pixels_per_inch));
            let method = if fields.resampling { record.private.get("__resample").cloned().unwrap_or(json!("automatic")) } else { json!("none") };
            f.insert("resample".into(), method);
            f
        }
        _ => {
            let mut f = size_params(ui.host.get::<CanvasSizeDialog>()?.fields().current);
            f.insert("relative".into(), json!(false));
            f.insert("anchor".into(), record.private.get("__anchor").cloned().unwrap_or(json!("center")));
            f.insert("extensionColor".into(), record.private.get("__extension").cloned().unwrap_or(json!("background")));
            f
        }
    };
    f.extend(record.extra.clone());
    Some(f)
}

/// Opens Image Size or Canvas Size for the active document.
pub fn open(app: &mut PhotocraftApp, command: &str) -> Option<u64> {
    let prefs = app.session.prefs();
    let per_inch = prefs.units_and_rulers.point_size.per_inch();
    let mut resample = pref_resample(prefs.general.image_interpolation);
    let d = &app.session.active()?.doc;
    let original = PixelSize { width: d.size.width, height: d.size.height, pixels_per_inch: f64::from(d.resolution_dpi) };
    let bytes = d.layers.first().and_then(|l| l.surface()).map_or(4, |s| s.format().bytes_per_pixel()) as f64;
    let label = photocraft_engine::commands::find(command).map(|c| c.label).unwrap_or("Image Size…");
    let mut private = Fields::new();
    private.insert("__command".into(), json!(command));
    private.insert("__label".into(), json!(label));
    if command == "image.imageSize" {
        // Indexed Color resamples with Nearest Neighbor only, as the command does (#6019).
        if d.mode == photocraft_color::ColorMode::Indexed && resample != "automatic" {
            resample = "nearest";
        }
        private.insert("__resample".into(), json!(resample));
        let dialog = ImageSizeDialog::new(original).bytes_per_pixel(bytes).resample(resample_of(resample)).points_per_inch(per_inch);
        Some(host(app, HostedKind::ImageSize, dialog, command, label, private))
    } else {
        private.insert("__anchor".into(), json!("center"));
        private.insert("__extension".into(), json!("background"));
        let has_background = d.background().is_some();
        let dialog = CanvasSizeDialog::new(original).bytes_per_pixel(bytes).has_background(has_background).points_per_inch(per_inch);
        Some(host(app, HostedKind::CanvasSize, dialog, command, label, private))
    }
}

/// Opens `dialog` and records it. One of each type is open: opening over it replaces it, with its
/// cancel cleanup.
fn host<D: StackDialog>(app: &mut PhotocraftApp, kind: HostedKind, dialog: D, command: &str, label: &str, private: Fields) -> u64 {
    let id = app.ui.alloc_id();
    let replaced = app.ui.hosted.iter().find(|h| h.kind == kind).map(|h| h.id);
    app.ui.host.open(dialog);
    if app.ui.host.take::<D>().is_some()
        && let Some(old) = replaced
    {
        super::hosted::cancelled(app, old);
    }
    app.ui.hosted.push(Hosted {
        kind,
        id,
        key: command.to_owned(),
        params: Vec::new(),
        title: label.trim_end_matches('…').to_owned(),
        extra: Fields::new(),
        private,
        documents: Vec::new(),
        whole: Vec::new(),
        table: Pending::default(),
    });
    id
}

/// Closes the open dialog of `kind` without an answer.
pub(super) fn close(app: &mut PhotocraftApp, ctx: &egui::Context, kind: HostedKind) {
    if kind == HostedKind::ImageSize {
        app.ui.host.cancel::<ImageSizeDialog>(ctx);
        app.ui.host.take::<ImageSizeDialog>();
    } else {
        app.ui.host.cancel::<CanvasSizeDialog>(ctx);
        app.ui.host.take::<CanvasSizeDialog>();
    }
}

/// `ui.dialog.confirm`: the library dialogs have no programmatic OK (gap Z3), so the dialog's
/// values are read, the dialog is closed and the command runs with them.
pub fn confirm(app: &mut PhotocraftApp, ctx: &egui::Context, id: u64) -> Result<Value, String> {
    let record = app.ui.hosted(id).cloned().ok_or_else(|| format!("no dialog {id}"))?;
    let params = fields_of(&app.ui, &record).ok_or_else(|| format!("no dialog {id}"))?;
    close(app, ctx, record.kind);
    app.ui.hosted.retain(|h| h.id != id);
    super::hosted::run_form(app, record, params)
}

/// The answer of the dialog of `kind`, after `DialogHost::show`: OK runs the command, Cancel runs the
/// cleanup.
pub fn settle(app: &mut PhotocraftApp, kind: HostedKind) -> Option<Result<Value, String>> {
    let answer = if kind == HostedKind::ImageSize {
        app.ui.host.take::<ImageSizeDialog>()?.ok().map(|result| image_params(&result))
    } else {
        app.ui.host.take::<CanvasSizeDialog>()?.ok().map(|result| canvas_params(&result))
    };
    let record = app.ui.hosted.iter().find(|h| h.kind == kind).cloned()?;
    match answer {
        Some(params) => {
            app.ui.hosted.retain(|h| h.id != record.id);
            Some(super::hosted::run_form(app, record, params))
        }
        None => {
            super::hosted::cancelled(app, record.id);
            None
        }
    }
}

#[cfg(test)]
mod tests;
