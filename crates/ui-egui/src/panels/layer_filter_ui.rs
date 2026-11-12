//! The Layers panel filter's criteria as `UiState` keeps them (`layer_filter*`) and the control
//! channel reads and sets them (`ui.set {layerFilter}`). craft-layers' `LayerFilter` draws the
//! filter row and decides what each type lists (`layers_tree::filter_of` converts); the types are
//! Kind, Name, Effect (#10880), Mode, Attribute (#10881), Color (#5898), Smart Object, Artboard and
//! Group (#10883).

use photocraft_color::BlendMode;
use photocraft_doc::LabelColor;
use serde::{Deserialize, Serialize};

use crate::PhotocraftApp;
use crate::state::UiState;

/// What the Layers panel filters by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterBy {
    #[default]
    Kind,
    Name,
    Effect,
    Mode,
    Attribute,
    Color,
    SmartObject,
    Artboard,
    Group,
}

/// The filter criteria, copied out of `UiState`.
#[derive(Clone, Debug, PartialEq)]
pub struct Filter {
    pub by: FilterBy,
    pub kinds: Vec<String>,
    /// As typed; matched trimmed and ignoring case.
    pub name: String,
    pub mode: BlendMode,
    pub effect: effect::EffectKind,
    pub attribute: attribute::Attribute,
    pub color: LabelColor,
    /// The Smart Object toggles that are on, from [`SMART`].
    pub smart: Vec<String>,
}

impl Filter {
    pub fn of(ui: &UiState) -> Self {
        Self {
            by: ui.layer_filter_by,
            kinds: ui.layer_filter.clone(),
            name: ui.layer_filter_name.clone(),
            mode: ui.layer_filter_mode,
            effect: ui.layer_filter_effect,
            attribute: ui.layer_filter_attribute,
            color: ui.layer_filter_color,
            smart: ui.layer_filter_smart.clone(),
        }
    }
}

/// The kinds the Kind filter toggles.
pub const KINDS: [&str; 5] = ["pixel", "adjustment", "type", "shape", "smart"];

/// The Smart Object filter's toggles: smart objects linked to a file, linked to a file that is
/// not there, and embedded in the document.
pub const SMART: [&str; 3] = ["linked", "missing", "embedded"];

/// `ui.set {layerFilter: {by?, kinds?, name?, effect?, mode?, attribute?, color?, smart?}}`:
/// `current` with the given fields replaced, or an error naming the bad field (nothing changes
/// then). `by` is `kind`, `name`, `effect`, `mode`, `attribute`, `color`, `smartObject`,
/// `artboard` or `group`; `kinds` a list of [`KINDS`]; `effect` an [`effect::EffectKind`] id; `mode` one of
/// the layer blend modes such as `Multiply` (not Pass Through, which the Mode list doesn't offer,
/// #10878); `attribute` an [`attribute::Attribute`] id; `color` a label colour id (`none`, `red`,
/// … `gray`, [`LabelColor::id`]); `smart` a list of [`SMART`]. Artboard has no criteria of its
/// own: the selected layers pick the artboards; nor has Group.
pub fn patched(current: &UiState, v: &serde_json::Value) -> Result<Filter, String> {
    let Some(o) = v.as_object() else { return Err("layerFilter must be an object".into()) };
    if let Some(k) = o.keys().find(|k| !["by", "kinds", "name", "effect", "mode", "attribute", "color", "smart"].contains(&k.as_str())) {
        return Err(format!("unknown layerFilter field `{k}` (fields: by, kinds, name, effect, mode, attribute, color, smart)"));
    }
    let mut f = Filter::of(current);
    if let Some(by) = o.get("by") {
        f.by = serde_json::from_value(by.clone())
            .map_err(|_| "layerFilter.by must be kind, name, effect, mode, attribute, color, smartObject, artboard or group".to_string())?;
    }
    if let Some(kinds) = o.get("kinds") {
        let list = kinds.as_array().ok_or("layerFilter.kinds must be a list")?;
        f.kinds = Vec::new();
        for k in list {
            let Some(k) = k.as_str().filter(|k| KINDS.contains(k)) else {
                return Err(format!("layerFilter.kinds: unknown kind {k} (kinds: {})", KINDS.join(", ")));
            };
            if !f.kinds.iter().any(|have| have == k) {
                f.kinds.push(k.to_string());
            }
        }
    }
    if let Some(name) = o.get("name") {
        f.name = name.as_str().ok_or("layerFilter.name must be a string")?.to_string();
    }
    if let Some(mode) = o.get("mode") {
        let name = mode.as_str().ok_or("layerFilter.mode must be a blend mode name")?;
        f.mode = photocraft_engine::commands::blend_from_str(name).filter(|m| BlendMode::LAYER_MODES.contains(m)).ok_or_else(|| {
            let modes: Vec<&str> = BlendMode::LAYER_MODES.iter().map(|m| m.label()).collect();
            format!("layerFilter.mode: `{name}` is not a layer blend mode the filter lists (modes: {})", modes.join(", "))
        })?;
    }
    if let Some(e) = o.get("effect") {
        f.effect = effect::parse(e)?;
    }
    if let Some(a) = o.get("attribute") {
        f.attribute = attribute::parse(a)?;
    }
    if let Some(color) = o.get("color") {
        let id = color.as_str().ok_or("layerFilter.color must be a label colour id")?;
        f.color = LabelColor::ALL.into_iter().find(|c| c.id() == id).ok_or_else(|| {
            let ids: Vec<&str> = LabelColor::ALL.iter().map(|c| c.id()).collect();
            format!("layerFilter.color: unknown colour `{id}` (colours: {})", ids.join(", "))
        })?;
    }
    if let Some(smart) = o.get("smart") {
        let list = smart.as_array().ok_or("layerFilter.smart must be a list")?;
        f.smart = Vec::new();
        for k in list {
            let Some(k) = k.as_str().filter(|k| SMART.contains(k)) else {
                return Err(format!("layerFilter.smart: unknown toggle {k} (toggles: {})", SMART.join(", ")));
            };
            if !f.smart.iter().any(|have| have == k) {
                f.smart.push(k.to_string());
            }
        }
    }
    Ok(f)
}

/// Write `f` into `ui`'s filter fields.
fn store(ui: &mut UiState, f: Filter) {
    ui.layer_filter_by = f.by;
    ui.layer_filter = f.kinds;
    ui.layer_filter_name = f.name;
    ui.layer_filter_mode = f.mode;
    ui.layer_filter_effect = f.effect;
    ui.layer_filter_attribute = f.attribute;
    ui.layer_filter_color = f.color;
    ui.layer_filter_smart = f.smart;
}

/// Store `f` as the Layers panel's filter; a change deselects what it hides ([`deselect_hidden`]).
pub fn apply(app: &mut PhotocraftApp, f: Filter) {
    let before = super::layers_tree::filter_of(&app.ui);
    store(&mut app.ui, f);
    deselect_hidden(app, &before);
}

/// When the panel's filter is no longer `before`, deselect the layers it hides, as the panel's
/// own filter row does (#10879): the listed ones stay selected, one of them active. Like any layer
/// selection this is no history step.
pub(crate) fn deselect_hidden(app: &mut PhotocraftApp, before: &craft::layers::ui::filter::LayerFilter) {
    let filter = super::layers_tree::filter_of(&app.ui);
    if filter == *before {
        return;
    }
    let Some(st) = app.session.active() else { return };
    let Some(select) = filter.deselect_hidden(&super::layers_tree::PhotoLayers::new(app, st, &filter, false, None)) else { return };
    let mut actions = Vec::new();
    super::layers_events::selection_actions(select.change, &st.selected_layers(), &mut actions);
    for (id, params) in actions {
        let _ = app.run(&id, params);
    }
}

pub mod effect;
pub mod find;

#[cfg(test)]
mod effect_tests;

pub mod attribute;

#[cfg(test)]
mod attribute_tests;
#[cfg(test)]
mod find_tests;
#[cfg(test)]
mod selection_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod kind_mode_tests;

#[cfg(test)]
mod group_tests;
