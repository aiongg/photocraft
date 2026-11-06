//! The file types PhotoCraft's Open and Save panels offer.

use std::path::Path;

/// Everything File › Open reads: PhotoCraft, Photoshop, OpenRaster, Krita and Affinity documents, flat images, and
/// Photoshop brushes (.abr), gradients (.grd), layer styles (.asl), swatches (.aco, .act, .ase) and keyboard shortcut sets
/// (.kys), which go to the preset libraries and Edit › Keyboard Shortcuts.
pub const OPEN_EXTS: &[&str] = &[
    "pcraft", "pdn", "ora", "xcf", "xcfgz", "gz", "kra", "krz", "psd", "psb", "psdt", "png", "jpg", "jpeg", "tif", "tiff", "webp", "gif", "bmp", "dib", "tga",
    "ico", "qoi", "exr", "hdr", "pbm", "pgm", "ppm", "pnm", "pam", "pfm", "pcx", "dds", "xpm", "fits", "fit", "fts", "dpx", "cin", "dcm", "dicom", "dc3",
    "iff", "lbm", "ilbm", "heic", "heif", "hif", "dng", "cr2", "cr3", "nef", "nrw", "arw", "pef", "orf", "rw2", "raf", "abr", "grd", "asl", "svg", "svgz",
    "aco", "act", "ase", "kys", "af", "afdesign", "afphoto", "afpub",
];
pub const SVG_EXTS: &[&str] = &["svg", "svgz"];

/// File › Open's file types after All Formats, as Photoshop lists them: one per format, so a folder
/// of mixed files can be narrowed to one kind (#6337). Together they cover every extension of
/// [`OPEN_EXTS`]. An explicit SVG filter also keeps vector artwork discoverable when a native
/// picker defaults to another filter.
pub const OPEN_FILTERS: &[(&str, &[&str])] = &[
    ("PhotoCraft", &["pcraft"]),
    ("Photoshop", &["psd", "psb", "psdt"]),
    ("Affinity", &["af", "afdesign", "afphoto", "afpub"]),
    ("Krita", &["kra", "krz"]),
    ("GIMP", &["xcf", "xcfgz", "gz"]),
    ("OpenRaster", &["ora"]),
    ("Paint.NET", &["pdn"]),
    ("SVG", SVG_EXTS),
    ("PNG", &["png"]),
    ("JPEG", &["jpg", "jpeg"]),
    ("HEIF", &["heic", "heif", "hif"]),
    ("TIFF", &["tif", "tiff"]),
    ("WebP", &["webp"]),
    ("GIF", &["gif"]),
    ("BMP", &["bmp", "dib"]),
    ("Targa", &["tga"]),
    ("ICO", &["ico"]),
    ("QOI", &["qoi"]),
    ("PCX", &["pcx"]),
    ("DDS", &["dds"]),
    ("XPM", &["xpm"]),
    ("Portable Bitmap", &["pbm", "pgm", "ppm", "pnm", "pam", "pfm"]),
    ("OpenEXR", &["exr"]),
    ("Radiance HDR", &["hdr"]),
    ("FITS", &["fits", "fit", "fts"]),
    ("DPX and Cineon", &["dpx", "cin"]),
    ("Amiga IFF", &["iff", "lbm", "ilbm"]),
    ("DICOM", &["dcm", "dicom", "dc3"]),
    ("Camera Raw", &["dng", "cr2", "cr3", "nef", "nrw", "arw", "pef", "orf", "rw2", "raf"]),
    ("Brushes", &["abr"]),
    ("Gradients", &["grd"]),
    ("Styles", &["asl"]),
    ("Swatches", &["aco", "act", "ase"]),
    ("Keyboard Shortcuts", &["kys"]),
];

/// Whether this build decodes files with extension `ext`: the format's decoder is compiled in
/// (HEIF only with the `heif` feature). Extensions of documents, presets and raws have decoders
/// in every build.
pub fn opens(ext: &str) -> bool {
    photocraft_codecs::from_extension(ext).is_none_or(|f| photocraft_codecs::caps(f).read)
}

/// File › Open's types: All Formats first, then one per format. Only formats this build opens
/// are listed, so the web build (and a desktop build without HEIF) doesn't offer files it can't read.
pub fn open_filters() -> Vec<(&'static str, Vec<String>)> {
    let kept = |exts: &[&str]| exts.iter().filter(|e| opens(e)).map(|e| (*e).to_string()).collect::<Vec<_>>();
    std::iter::once(("All Formats", kept(OPEN_EXTS)))
        .chain(OPEN_FILTERS.iter().map(|(name, exts)| (*name, kept(exts))).filter(|(_, exts)| !exts.is_empty()))
        .collect()
}

/// File › Save As formats: (filter name, extensions). The filter matching the suggested name's
/// extension comes first, so a .pcraft document saves as .pcraft by default and everything else
/// keeps defaulting to PSD.
pub const SAVE_FILTERS: &[(&str, &[&str])] = &[
    ("PSD Document", &["psd", "psb"]),
    ("PDF", &["pdf"]),
    ("PhotoCraft", &["pcraft"]),
    ("OpenRaster", &["ora"]),
    ("PNG", &["png"]),
    ("JPEG", &["jpg", "jpeg"]),
    ("WebP", &["webp"]),
    ("TIFF", &["tif", "tiff"]),
    ("Targa", &["tga"]),
    // Also in Photoshop's Save As list; ICO is PhotoCraft's own (#4357): up to 256×256, with alpha.
    ("BMP", &["bmp", "dib"]),
    ("GIF", &["gif"]),
    ("ICO", &["ico"]),
    ("OpenEXR", &["exr"]),
    // The encoder picks P5, P6, P7 or PFM from the image, not from the name, so only the neutral
    // .pnm is offered: a .pbm or .pgm name would hold pixmap data (CodeRabbit on #9775).
    ("Netpbm", &["pnm"]),
    ("Radiance HDR", &["hdr"]),
    ("FITS", &["fits", "fit"]),
    ("DPX", &["dpx"]),
    // Game and 3D textures; DDS Options asks for the compression and mipmaps (#1327).
    ("DDS", &["dds"]),
    ("Cineon", &["cin"]),
    ("Amiga IFF", &["iff"]),
];

/// Non-document files the shell saves (Swatches panel exports): offered alone, so the dialog
/// never swaps their extension for a document format's.
const OTHER_SAVE_FILTERS: &[(&str, &[&str])] = &[("Color Swatches", &["aco"]), ("Color Table", &["act"]), ("Swatch Exchange", &["ase"])];

/// `suggested` with a Netpbm subtype's extension (an opened .pgm) as the neutral .pnm, the name
/// every save dialog starts from: the encoder picks P5, P6, P7 or PFM from the image, so the old
/// suffix could name other data, and macOS and rfd keep the suggested name as it is.
pub fn netpbm_as_pnm(suggested: &str) -> String {
    let path = Path::new(suggested);
    let subtype = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).is_some_and(|e| ["pbm", "pgm", "ppm", "pam", "pfm"].contains(&e.as_str()));
    if subtype { path.with_extension("pnm").to_string_lossy().into_owned() } else { suggested.to_string() }
}

/// Lists the save dialog's file types with the `suggested` type first (added if unlisted), so the dialog keeps that extension instead of .psd.
pub fn save_filters(suggested: &str) -> Vec<(String, Vec<String>)> {
    let ext = Path::new(suggested).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    // A Netpbm subtype's name (an opened .pgm) starts on the Netpbm type as .pnm: the encoder picks
    // the subtype from the image, so the old name could describe other data (CodeRabbit on #9775).
    let ext = if ["pbm", "pgm", "ppm", "pam", "pfm"].contains(&ext.as_str()) { "pnm".to_string() } else { ext };
    if let Some((name, exts)) = OTHER_SAVE_FILTERS.iter().find(|(_, exts)| exts.contains(&ext.as_str())) {
        return vec![(name.to_string(), exts.iter().map(|e| e.to_string()).collect())];
    }
    let mut v: Vec<(String, Vec<String>)> = SAVE_FILTERS.iter().map(|(name, exts)| (name.to_string(), exts.iter().map(|e| e.to_string()).collect())).collect();
    match v.iter().position(|(_, exts)| exts.contains(&ext)) {
        Some(i) => {
            let (name, mut exts) = v.remove(i);
            // Windows adds the first type's first extension to a bare name: `.psb`, not `.psd`.
            exts.retain(|e| *e != ext);
            exts.insert(0, ext);
            v.insert(0, (name, exts));
        }
        None if !ext.is_empty() => v.insert(0, (ext.to_ascii_uppercase(), vec![ext])),
        None => {}
    }
    v
}
