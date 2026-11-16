//! The File Type section of the Image Processor dialog. Its rows (`<format>/<option>`, one group per format) are
//! built with the rest of the form by `view_cmds::dialogs::params_for`; this module folds their values into the
//! per-format objects the command takes and refuses an OK with no format ticked.

use craft::ui::patterns::form_dialogs::ParamResult;
use craft::ui::runtime::env::Label;
use serde_json::{Map, Value};

use crate::PhotocraftApp;

/// The command id of the dialog.
pub(crate) const COMMAND: &str = "file.scripts.imageProcessor";

/// The formats the Image Processor saves.
const FORMATS: [&str; 3] = ["jpeg", "psd", "tiff"];

/// Folds the `<format>/<option>` values into the `{jpeg: {..}, psd: {..}, tiff: {..}}` objects the command takes.
pub(crate) fn nest(map: &mut Map<String, Value>) {
    for format in FORMATS {
        let prefix = format!("{format}/");
        let keys: Vec<String> = map.keys().filter(|key| key.starts_with(&prefix)).cloned().collect();
        let mut options = Map::new();
        for key in keys {
            if let Some(value) = map.remove(&key) {
                options.insert(key[prefix.len()..].to_owned(), value);
            }
        }
        if !options.is_empty() {
            map.insert(format.to_owned(), Value::Object(options));
        }
    }
}

/// The `ParamFamily::check` of the dialog: OK is refused, and the dialog stays open, while no file type is ticked.
pub(crate) fn check(_app: &mut PhotocraftApp, result: &ParamResult) -> Result<(), Label> {
    if FORMATS.iter().any(|format| result.fields.bool(&format!("{format}/enabled")) == Some(true)) {
        Ok(())
    } else {
        Err(Label::from(tl!("You must save to at least one file type.")))
    }
}

#[cfg(test)]
mod tests;
