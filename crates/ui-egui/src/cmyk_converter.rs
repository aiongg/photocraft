//! The CMYK converter of the Color panel and Color Picker: PhotoCraft's working CMYK profile
//! (Color Settings), so the CMYK sliders and the printable warning follow it instead of the
//! library's coated default.

use std::sync::Mutex;

use craft::cms::convert::CmykConverter;
use photocraft_doc::ColorMode;

use crate::PhotocraftApp;

/// The converter for the working CMYK profile. Kept per working-CMYK setting, so a frame does
/// not resolve the profile again. A profile that cannot link to sRGB gives the library's coated
/// converter.
pub(crate) fn working(app: &PhotocraftApp) -> CmykConverter {
    static CACHE: Mutex<Option<(String, CmykConverter)>> = Mutex::new(None);
    let spec = &app.session.color.settings.working_cmyk;
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((cached, converter)) = cache.as_ref()
        && cached == spec
    {
        return converter.clone();
    }
    let converter = CmykConverter::new(app.session.color.working(ColorMode::Cmyk)).unwrap_or_else(|_| CmykConverter::coated().clone());
    *cache = Some((spec.clone(), converter.clone()));
    converter
}
