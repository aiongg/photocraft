//! Image Size and Canvas Size on the library's [`ImageSizeDialog`] and [`CanvasSizeDialog`], shown by
//! the `DialogHost`.
//!
//! The app opens the dialog with the document's size, resolution and the preferences, keeps a
//! [`Hosted`] record for automation, and runs `image.imageSize` or `image.canvasSize` with the
//! answer. Automation sets the dialogs' values through their setters and reads their choices through the getters.

use craft::ui::dialog::{
    Anchor, CanvasExtension, CanvasSizeDialog, CanvasSizeResult, ImageSizeDialog, ImageSizeResult, PixelSize, Resample, ResolutionUnit, SizeUnit, SizingLabels,
    StackDialog,
};
use craft::ui::localized::Labels;
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
    f.insert("scaleStyles".into(), json!(result.scale_styles));
    f
}

fn canvas_params(result: &CanvasSizeResult) -> Fields {
    let mut f = size_params(result.size);
    f.insert("relative".into(), json!(false));
    f.insert("anchor".into(), json!(anchor_name(result.anchor)));
    f.insert("extensionColor".into(), json!(extension_name(&result.extension)));
    f
}

/// Length unit names of the `unit` field, as the legacy dialog and the rulers' units name them.
const UNIT_NAMES: [(&str, SizeUnit); 7] = [
    ("px", SizeUnit::Pixels),
    ("percent", SizeUnit::Percent),
    ("in", SizeUnit::Inches),
    ("cm", SizeUnit::Centimetres),
    ("mm", SizeUnit::Millimetres),
    ("pt", SizeUnit::Points),
    ("pica", SizeUnit::Picas),
];

fn unit_of(unit: photocraft_engine::prefs::Unit) -> SizeUnit {
    use photocraft_engine::prefs::Unit;
    match unit {
        Unit::Pixels => SizeUnit::Pixels,
        Unit::Percent => SizeUnit::Percent,
        Unit::Inches => SizeUnit::Inches,
        Unit::Centimeters => SizeUnit::Centimetres,
        Unit::Millimeters => SizeUnit::Millimetres,
        Unit::Points => SizeUnit::Points,
        Unit::Picas => SizeUnit::Picas,
    }
}

fn unit_name(unit: SizeUnit) -> &'static str {
    UNIT_NAMES.iter().find(|(_, u)| *u == unit).map_or("px", |(name, _)| name)
}

const RESOLUTION_UNITS: [(&str, ResolutionUnit); 2] =
    [("pixelsPerInch", ResolutionUnit::PixelsPerInch), ("pixelsPerCentimeter", ResolutionUnit::PixelsPerCentimetre)];

const ANCHORS: [Anchor; 9] =
    [Anchor::TopLeft, Anchor::Top, Anchor::TopRight, Anchor::Left, Anchor::Center, Anchor::Right, Anchor::BottomLeft, Anchor::Bottom, Anchor::BottomRight];

/// PhotoCraft's words for the library's size labels, by the library's keys. A key not listed keeps
/// the library's English.
const LABELS: &[(&str, &str)] = &[
    ("sizing.imageSizeTitle", "Image Size"),
    ("sizing.canvasSizeTitle", "Canvas Size"),
    ("sizing.imageSize", "Image Size:"),
    ("sizing.sizeWas", "{size} (was {old})"),
    ("sizing.dimensions", "Dimensions:"),
    ("sizing.currentSize", "Current Size"),
    ("sizing.newSize", "New Size"),
    ("sizing.width", "Width:"),
    ("sizing.height", "Height:"),
    ("sizing.constrain", "Constrain proportions"),
    ("sizing.resolution", "Resolution:"),
    ("sizing.pixelsPerInch", "Pixels/Inch"),
    ("sizing.resample", "Resample"),
    ("sizing.scaleStyles", "Scale Styles"),
    ("sizing.relative", "Relative"),
    ("sizing.anchor", "Anchor:"),
    ("sizing.extension", "Canvas extension color:"),
    ("sizing.unitPixels", "Pixels"),
    ("sizing.unitPercent", "Percent"),
    ("sizing.unitInches", "Inches"),
    ("sizing.unitCentimetres", "Centimeters"),
    ("sizing.unitMillimetres", "Millimeters"),
    ("sizing.unitPoints", "Points"),
    ("sizing.unitPicas", "Picas"),
    ("sizing.automatic", "Automatic"),
    ("sizing.preserveDetails", "Preserve Details"),
    ("sizing.bicubicSmoother", "Bicubic Smoother (enlargement)"),
    ("sizing.bicubicSharper", "Bicubic Sharper (reduction)"),
    ("sizing.bicubic", "Bicubic (smooth gradients)"),
    ("sizing.nearestNeighbor", "Nearest Neighbor (hard edges)"),
    ("sizing.bilinear", "Bilinear"),
    ("sizing.lanczos", "Lanczos (sharp)"),
    ("sizing.foreground", "Foreground"),
    ("sizing.background", "Background"),
    ("sizing.white", "White"),
    ("sizing.black", "Black"),
    ("sizing.transparent", "Transparent"),
];

fn labels() -> SizingLabels {
    <SizingLabels as Labels>::english().localized(|key| LABELS.iter().find(|(k, _)| *k == key).map(|(_, english)| tl!(english).to_string()))
}

/// The values of an open size dialog as `ui.dialog.*` and the command see them. `width` and
/// `height` are pixels; `unit` names the unit the dialog's fields show.
pub fn fields_of(ui: &UiState, record: &Hosted) -> Option<Fields> {
    let did = record.did;
    Some(match record.kind {
        HostedKind::ImageSize => {
            let dialog = ui.host.get_by_id::<ImageSizeDialog>(did)?;
            let fields = dialog.fields();
            let mut f = size_params(fields.current);
            f.insert("resolution".into(), json!(fields.current.pixels_per_inch));
            let method = json!(resample_name(dialog.chosen_resample()));
            f.insert("__resample".into(), method.clone());
            f.insert("resample".into(), method);
            f.insert("scaleStyles".into(), json!(dialog.chosen_scale_styles()));
            f.insert("unit".into(), json!(unit_name(fields.unit)));
            let resolution_unit = RESOLUTION_UNITS.iter().find(|(_, u)| *u == fields.resolution_unit).map_or("pixelsPerInch", |(name, _)| name);
            f.insert("resolutionUnit".into(), json!(resolution_unit));
            f
        }
        _ => {
            let dialog = ui.host.get_by_id::<CanvasSizeDialog>(did)?;
            let mut f = size_params(dialog.fields().current);
            f.insert("relative".into(), json!(dialog.chosen_relative()));
            f.insert("anchor".into(), json!(anchor_name(dialog.chosen_anchor())));
            f.insert("extensionColor".into(), json!(extension_name(&dialog.chosen_extension())));
            f.insert("unit".into(), json!(unit_name(dialog.fields().unit)));
            f
        }
    })
}

fn number(field: &str, value: &Value) -> Result<f64, String> {
    value.as_f64().filter(|n| n.is_finite()).ok_or_else(|| format!("`{field}`: {value} is not a number"))
}

fn choice<T: Copy>(field: &str, value: &Value, table: &[(&str, T)]) -> Result<T, String> {
    let name = value.as_str().ok_or_else(|| format!("`{field}`: {value} is not a name"))?;
    table.iter().find(|(n, _)| *n == name).map(|(_, v)| *v).ok_or_else(|| format!("`{field}`: `{name}` is not one of this dialog's values"))
}

fn flag(field: &str, value: &Value) -> Result<bool, String> {
    value.as_bool().ok_or_else(|| format!("`{field}`: {value} is not true or false"))
}

/// `ui.dialog.set` for a size dialog: the dialog's setters change what it shows, as typing or
/// picking would. `width` and `height` are pixels, whatever unit the dialog shows.
pub fn set_field(app: &mut PhotocraftApp, id: u64, field: &str, value: &Value) -> Result<(), String> {
    let record = app.ui.hosted(id).ok_or_else(|| format!("no dialog {id}"))?;
    let (kind, did) = (record.kind, record.did);
    let missing = || format!("no dialog {id}");
    if kind == HostedKind::ImageSize {
        let dialog = app.ui.host.get_mut_by_id::<ImageSizeDialog>(did).ok_or_else(missing)?;
        match field {
            "width" => dialog.fields_mut().set_width(number(field, value)?),
            "height" => dialog.fields_mut().set_height(number(field, value)?),
            "resolution" => dialog.set_resolution(number(field, value)?),
            "unit" => dialog.set_unit(choice(field, value, &UNIT_NAMES)?),
            "resolutionUnit" => dialog.set_resolution_unit(choice(field, value, &RESOLUTION_UNITS)?),
            "resample" => dialog.set_resample(choice(field, value, &RESAMPLES)?),
            "scaleStyles" => dialog.set_scale_styles(flag(field, value)?),
            _ => return Err(format!("`{field}` is not a field of Image Size")),
        }
    } else {
        let dialog = app.ui.host.get_mut_by_id::<CanvasSizeDialog>(did).ok_or_else(missing)?;
        match field {
            "width" => dialog.fields_mut().set_width(number(field, value)?),
            "height" => dialog.fields_mut().set_height(number(field, value)?),
            "unit" => dialog.set_unit(choice(field, value, &UNIT_NAMES)?),
            "relative" => dialog.set_relative(flag(field, value)?),
            "anchor" => dialog.set_anchor(
                ANCHORS[ANCHORS
                    .iter()
                    .position(|a| anchor_name(*a) == value.as_str().unwrap_or_default())
                    .ok_or_else(|| format!("`{field}`: {value} is not an anchor"))?],
            ),
            "extensionColor" => dialog.set_extension(extension_of(value).ok_or_else(|| format!("`{field}`: {value} is not a canvas extension color"))?),
            _ => return Err(format!("`{field}` is not a field of Canvas Size")),
        }
    }
    Ok(())
}

const RESAMPLES: [(&str, Resample); 8] = [
    ("none", Resample::None),
    ("automatic", Resample::Automatic),
    ("preserveDetails", Resample::PreserveDetails),
    ("bicubicSmoother", Resample::BicubicSmoother),
    ("bicubicSharper", Resample::BicubicSharper),
    ("bicubic", Resample::Bicubic),
    ("bilinear", Resample::Bilinear),
    ("nearest", Resample::NearestNeighbor),
];

fn extension_of(value: &Value) -> Option<CanvasExtension> {
    Some(match value.as_str()? {
        "foreground" => CanvasExtension::Foreground,
        "background" => CanvasExtension::Background,
        "white" => CanvasExtension::White,
        "black" => CanvasExtension::Black,
        "transparent" => CanvasExtension::Transparent,
        "gray" => CanvasExtension::Gray,
        hex => CanvasExtension::Other(crate::color_picker_ui::parse_hex(hex)?),
    })
}

/// Opens Image Size or Canvas Size for the active document.
pub fn open(app: &mut PhotocraftApp, command: &str) -> Option<u64> {
    let prefs = app.session.prefs();
    let per_inch = prefs.units_and_rulers.point_size.per_inch();
    let unit = unit_of(prefs.units_and_rulers.rulers);
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
        let indexed = d.mode == photocraft_color::ColorMode::Indexed;
        if indexed && resample != "automatic" {
            resample = "nearest";
        }
        let mut dialog = ImageSizeDialog::new(original).labels(labels()).unit(unit);
        if indexed {
            dialog = dialog.resample_methods(&[Resample::Automatic, Resample::NearestNeighbor]);
        }
        let dialog = dialog.bytes_per_pixel(bytes).resample(resample_of(resample)).points_per_inch(per_inch);
        Some(host(app, HostedKind::ImageSize, dialog, command, label, private))
    } else {
        let has_background = d.background().is_some();
        let dialog =
            CanvasSizeDialog::new(original).labels(labels()).unit(unit).bytes_per_pixel(bytes).has_background(has_background).points_per_inch(per_inch);
        Some(host(app, HostedKind::CanvasSize, dialog, command, label, private))
    }
}

/// Opens `dialog` and records it. One of each type is open: opening over it replaces it, with its
/// cancel cleanup.
fn host<D: StackDialog>(app: &mut PhotocraftApp, kind: HostedKind, dialog: D, command: &str, label: &str, private: Fields) -> u64 {
    let id = app.ui.alloc_id();
    let replaced = app.ui.hosted.iter().find(|h| h.kind == kind).map(|h| h.id);
    let did = app.ui.host.open(dialog);
    if app.ui.host.take::<D>().is_some()
        && let Some(old) = replaced
    {
        super::hosted::cancelled(app, old);
    }
    app.ui.hosted.push(Hosted {
        kind,
        id,
        did,
        key: command.to_owned(),
        params: Vec::new(),
        title: label.trim_end_matches('…').to_owned(),
        extra: Fields::new(),
        private,
        documents: Vec::new(),
        table: Pending::default(),
        preview: Default::default(),
        remembers: false,
    });
    id
}

/// Closes dialog `id` without an answer.
pub(super) fn close(app: &mut PhotocraftApp, ctx: &egui::Context, id: u64) {
    let Some((kind, did)) = app.ui.hosted(id).map(|h| (h.kind, h.did)) else { return };
    if kind == HostedKind::ImageSize {
        app.ui.host.cancel_by_id::<ImageSizeDialog>(ctx, did);
        app.ui.host.take_by_id::<ImageSizeDialog>(did);
    } else {
        app.ui.host.cancel_by_id::<CanvasSizeDialog>(ctx, did);
        app.ui.host.take_by_id::<CanvasSizeDialog>(did);
    }
}

/// The answer of dialog `id`, after `DialogHost::show`: OK runs the command, Cancel runs the
/// cleanup.
pub fn settle(app: &mut PhotocraftApp, id: u64) -> Option<Result<Value, String>> {
    let record = app.ui.hosted(id)?.clone();
    let did = record.did;
    let answer = if record.kind == HostedKind::ImageSize {
        app.ui.host.take_by_id::<ImageSizeDialog>(did)?.ok().map(|result| image_params(&result))
    } else {
        app.ui.host.take_by_id::<CanvasSizeDialog>(did)?.ok().map(|result| canvas_params(&result))
    };
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
