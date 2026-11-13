//! Theme ids in the preferences: the dark and light slots and the favorites hold the ids of the
//! Craft suite's themes. The theme names PhotoCraft saved before ids (`proMedium`, `studioLight`,
//! ...) read as their ids.

use serde::{Deserialize, Deserializer};

/// How many favorite themes the appearance menu keeps.
pub const MAX_FAVORITES: usize = 64;

/// The theme ids are at most this long (craft-theme's limit).
const MAX_ID_LEN: usize = 64;

/// The theme ids and mode a pre-id theme name stands for: the theme id, and whether it is the dark
/// variant.
pub fn legacy(name: &str) -> Option<(&'static str, bool)> {
    Some(match name {
        "pro" => ("craft.pro", true),
        "proMedium" => ("craft.artcraft", true),
        "proLightGray" => ("craft.pro-light-gray", false),
        "proLight" => ("craft.pro", false),
        "studio" => ("craft.studio", true),
        "studioLight" => ("craft.studio", false),
        "classic" => ("craft.classic", false),
        "solarizedDark" => ("craft.solarized-dark", true),
        "adwaita" => ("craft.adwaita", false),
        "adwaitaDark" => ("craft.adwaita", true),
        "breezeLight" => ("craft.breeze", false),
        "breezeDark" => ("craft.breeze", true),
        "softDark" => ("craft.soft", true),
        "softLight" => ("craft.soft", false),
        _ => return None,
    })
}

/// The id a saved value names: a pre-id theme name's id, else the value itself.
pub fn id_of(value: &str) -> String {
    legacy(value).map_or_else(|| value.to_owned(), |(id, _)| id.to_owned())
}

/// Whether `value` is a theme id or a pre-id theme name.
pub fn is_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_LEN && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}

pub(super) fn theme<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    String::deserialize(d).map(|s| id_of(&s))
}

pub(super) fn themes<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    Vec::<String>::deserialize(d).map(|v| v.iter().map(|s| id_of(s)).collect())
}
