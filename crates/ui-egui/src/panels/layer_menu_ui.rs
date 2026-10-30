//! Layers panel right-click menu in Photoshop's order. Items are command ids from the menu catalog;
//! unavailable ones are greyed using the same enablement as the main menus. Items marked as menu
//! invocations (`Value::Null` params) go through `menus::invoke`, so dialogs open like in the menu bar.

use craft_ui::menu::{ContextMenu, MenuEntry, MenuItem};
use photocraft_doc::{LabelColor, Layer, LayerContent};
use serde_json::{Value, json};

/// One entry: label, command id. `None` = separator.
pub type Entry = Option<(&'static str, &'static str)>;

/// English action key; each menu translates it using its existing display context.
pub(crate) fn mask_toggle_label(enabled: bool) -> &'static str {
    if enabled { "Disable Layer Mask" } else { "Enable Layer Mask" }
}

/// The command behind "Add Layer Mask" (the panel button and this menu). Like Photoshop, an active
/// selection becomes the mask; ⌥ inverts it (Hide Selection, or Hide All without a selection).
pub fn add_mask_command(has_selection: bool, alt: bool) -> &'static str {
    match (has_selection, alt) {
        (true, false) => "layer.layerMask.revealSelection",
        (true, true) => "layer.layerMask.hideSelection",
        (false, false) => "layer.layerMask.revealAll",
        (false, true) => "layer.layerMask.hideAll",
    }
}

/// What the Layers panel "Add a mask" button does for `layer` (#2075). Like Photoshop it never
/// replaces a mask: a layer without a layer mask gets one ([`add_mask_command`]); a layer that
/// already has one gets a vector mask instead (⌥: Hide All); a layer with both (or a shape layer,
/// whose path is already its vector mask, with a layer mask) gets nothing (`None`).
pub fn mask_button_command(layer: Option<&Layer>, has_selection: bool, alt: bool) -> Option<&'static str> {
    let l = layer?;
    if l.mask.is_none() {
        Some(add_mask_command(has_selection, alt))
    } else if l.vector_mask.is_none() && !matches!(l.content, LayerContent::Shape(_)) {
        Some(if alt { "layer.vectorMask.hideAll" } else { "layer.vectorMask.revealAll" })
    } else {
        None
    }
}

/// Commands that act on the active layer only and have no sensible meaning for a selection: Blending
/// Options (one dialog), Copy Layer Style (copy from which layer?) and the per-layer mask items.
/// With several layers selected they would hit whichever layer is active, not the one that was
/// right-clicked, so the menu greys them. Adding a mask, Paste/Clear Layer Style, Rasterize and
/// Export As (one file per layer, #3629) run on every selected layer instead.
pub fn single_layer_only(id: &str) -> bool {
    let adds_mask = matches!(id, "layer.layerMask.revealAll" | "layer.layerMask.hideAll" | "layer.layerMask.revealSelection" | "layer.layerMask.hideSelection");
    matches!(id, "layer.layerStyle.blendingOptions" | "layer.layerStyle.copyLayerStyle") || (id.starts_with("layer.layerMask.") && !adds_mask)
}

/// The context menu entries for a layer (Photoshop 2026 order, trimmed to the layer kind).
pub fn entries(l: &Layer, multi: bool, has_selection: bool) -> Vec<Entry> {
    let mut v: Vec<Entry> = vec![Some((tl!("Blending Options…"), "layer.layerStyle.blendingOptions"))];
    v.push(None);
    v.push(Some((if multi { tl!("Duplicate Layers…") } else { tl!("Duplicate Layer…") }, "layer.duplicate")));
    v.push(Some((if multi { tl!("Delete Layers") } else { tl!("Delete Layer") }, "layer.delete")));
    // Groups the clicked layer, or the whole selection when several layers are selected, after
    // Photoshop's New Group dialog.
    v.push(Some((tl!("Group from Layers…"), "layer.new.groupFromLayers")));
    v.push(None);
    v.push(Some((tl!("Quick Export As PNG"), "layer.quickExportAsPng")));
    v.push(Some((tl!("Export As…"), "layer.exportAs")));
    v.push(None);
    v.push(Some((tl!("Artboard from Layers…"), "layer.new.artboardFromLayers")));
    v.push(Some((tl!("Frame from Layers…"), "layer.new.frameFromLayers")));
    v.push(None);
    v.push(Some((tl!("Convert to Smart Object"), "layer.smartObjects.convertToSmartObject")));
    if multi {
        // One item for the whole selection, whatever the clicked layer is: Rasterize acts on every
        // selected layer. Edit Contents and Convert to Layers work on one layer, so they are not
        // offered here.
        v.push(Some(("Rasterize Layers", "layer.rasterize.layer")));
    } else {
        match &l.content {
            LayerContent::Text(_) => v.push(Some(("Rasterize Type", "layer.rasterize.type"))),
            LayerContent::Shape(_) => v.push(Some(("Rasterize Layer", "layer.rasterize.shape"))),
            LayerContent::Smart(_) => {
                v.push(Some((tl!("Edit Contents"), "layer.smartObjects.editContents")));
                v.push(Some((tl!("Replace Contents…"), "layer.smartObjects.replaceContents")));
                v.push(Some((tl!("Relink to File…"), "layer.smartObjects.relinkToFile")));
                v.push(Some((tl!("Convert to Layers"), "layer.smartObjects.convertToLayers")));
                v.push(Some(("Rasterize Layer", "layer.rasterize.smartObject")));
            }
            LayerContent::Fill(_) => v.push(Some(("Rasterize Layer", "layer.rasterize.fillContent"))),
            _ => v.push(Some(("Rasterize Layer", "layer.rasterize.layer"))),
        }
    }
    // Photoshop offers it under Rasterize Layer for a layer with a style (it bakes the effects).
    if multi || !l.effects.items.is_empty() {
        v.push(Some(("Rasterize Layer Style", "layer.rasterize.layerStyle")));
    }
    v.push(None);
    // A selection has no single mask to toggle, apply or delete, so its menu offers Add Layer Mask only.
    if let Some(mask) = l.mask.as_ref().filter(|_| !multi) {
        v.push(Some((tl!(mask_toggle_label(mask.enabled)), "layer.layerMask.enabled")));
        v.push(Some((tl!("Apply Layer Mask"), "layer.layerMask.apply")));
        v.push(Some((tl!("Delete Layer Mask"), "layer.layerMask.delete")));
    } else {
        v.push(Some((tl!("Add Layer Mask"), add_mask_command(has_selection, false))));
    }
    v.push(Some((
        if l.clipped { tl!("Release Clipping Mask") } else { tl!("Create Clipping Mask") },
        if l.clipped { "layer.releaseClippingMask" } else { "layer.createClippingMask" },
    )));
    v.push(None);
    v.push(Some((tl!("Link Layers"), "layer.linkLayers")));
    v.push(Some((tl!("Select Linked Layers"), "layer.selectLinkedLayers")));
    v.push(None);
    v.push(Some((tl!("Copy Layer Style"), "layer.layerStyle.copyLayerStyle")));
    v.push(Some((tl!("Paste Layer Style"), "layer.layerStyle.pasteLayerStyle")));
    v.push(Some((tl!("Clear Layer Style"), "layer.layerStyle.clear")));
    v.push(None);
    if multi {
        v.push(Some((tl!("Merge Layers"), "layer.mergeLayers")));
    } else if matches!(l.content, LayerContent::Group(_)) {
        // Photoshop's Merge Group: a group merges itself, not into the layer below (#3832).
        v.push(Some((tl!("Merge Group"), "layer.mergeGroup")));
    } else {
        v.push(Some((tl!("Merge Down"), "layer.mergeDown")));
    }
    v.push(Some((tl!("Merge Visible"), "layer.mergeVisible")));
    v.push(Some((tl!("Flatten Image"), "layer.flattenImage")));
    v
}

/// The layer style menu of an effect row ("Effects" or one effect) under a layer (#2846), in
/// Photoshop's order. It acts on that layer, like Layer ▸ Layer Style.
pub fn effects_entries() -> Vec<Entry> {
    vec![
        Some((tl!("Blending Options…"), "layer.layerStyle.blendingOptions")),
        None,
        Some((tl!("Copy Layer Style"), "layer.layerStyle.copyLayerStyle")),
        Some((tl!("Paste Layer Style"), "layer.layerStyle.pasteLayerStyle")),
        Some((tl!("Clear Layer Style"), "layer.layerStyle.clear")),
        None,
        Some((tl!("Global Light…"), "layer.layerStyle.globalLight")),
        Some((tl!("Create Layer"), "layer.layerStyle.createLayer")),
        Some((tl!("Hide All Effects"), "layer.layerStyle.hideAllEffects")),
        Some((tl!("Scale Effects…"), "layer.layerStyle.scaleEffects")),
    ]
}

/// What a row of a layer menu asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
    /// A command by id; `Value::Null` params, so the command's dialog opens like from the menu bar.
    Command(&'static str),
    /// Rename the layer in place.
    Rename,
    /// Set the layer (or the selection) label colour.
    Color(LabelColor),
    /// Load the layer mask as a selection with this operation.
    LoadMask(&'static str),
    /// Select and Mask… on the layer mask.
    SelectAndMask,
    /// Layer Mask Display Options.
    MaskOptions,
}

/// The menu rows of `entries`: separators collapsed, commands this build lacks left out, and
/// each command enabled the way the main menus enable it.
fn command_rows(app: &crate::PhotocraftApp, l: &Layer, on_set: bool, entries: Vec<Entry>) -> Vec<MenuEntry<Choice>> {
    let mut rows: Vec<MenuEntry<Choice>> = Vec::new();
    for e in entries {
        match e {
            None => {
                if !matches!(rows.last(), None | Some(MenuEntry::Separator)) {
                    rows.push(MenuEntry::Separator);
                }
            }
            Some((label, id)) => {
                // Skip commands this build doesn't have rather than showing dead items.
                if photocraft_engine::commands::find(id).is_none() && !crate::menu_catalog::CATALOG.iter().any(|m| m.3 == id) {
                    continue;
                }
                // Enablement is exact for the active layer (or the selection); another row is
                // selected first when clicked, so its items stay available.
                let is_active = app.session.active().is_some_and(|s| s.active_layer == Some(l.id));
                let enabled = !(on_set && single_layer_only(id)) && if on_set || is_active { crate::menus::is_enabled(app, id) } else { true };
                rows.push(MenuItem::new(tl!(&label), Choice::Command(id)).enabled(enabled).into());
            }
        }
    }
    if matches!(rows.last(), Some(MenuEntry::Separator)) {
        rows.pop();
    }
    rows
}

/// The rows of the layer menu for `l`; `on_set` when it acts on the whole selection.
pub fn menu_rows(app: &crate::PhotocraftApp, ctx: &egui::Context, l: &Layer, on_set: bool) -> Vec<MenuEntry<Choice>> {
    let has_selection = app.session.active().is_some_and(|s| s.doc.selection.is_some());
    let mut rows = command_rows(app, l, on_set, entries(l, on_set, has_selection));
    rows.push(MenuEntry::Separator);
    rows.push(MenuItem::new(tl!("Rename Layer…"), Choice::Rename).into());
    rows.push(MenuItem::submenu(tl!("Color"), color_rows(app, ctx, l, on_set)).into());
    rows
}

/// The rows of `l`'s layer-mask thumbnail menu (#4575), in Photoshop's order; empty without a mask.
pub fn mask_rows(app: &crate::PhotocraftApp, l: &Layer) -> Vec<MenuEntry<Choice>> {
    let Some(mask) = &l.mask else { return Vec::new() };
    let edits = vec![
        Some((tl!(mask_toggle_label(mask.enabled)), "layer.layerMask.enabled")),
        Some((tl!("Delete Layer Mask"), "layer.layerMask.delete")),
        Some((tl!("Apply Layer Mask"), "layer.layerMask.apply")),
    ];
    let mut rows = command_rows(app, l, false, edits);
    rows.push(MenuEntry::Separator);
    // ⌘⇧-, ⌘⌥- and ⌘⇧⌥-clicking the thumbnail, as menu items.
    for (label, operation) in [(tl!("Add Mask to Selection"), "add"), (tl!("Subtract Mask from Selection"), "subtract"), (tl!("Intersect Mask with Selection"), "intersect")] {
        rows.push(MenuItem::new(label, Choice::LoadMask(operation)).into());
    }
    rows.push(MenuEntry::Separator);
    rows.push(MenuItem::new(tl!("Select and Mask…"), Choice::SelectAndMask).enabled(crate::menus::is_enabled(app, "select.selectAndMask")).into());
    // Layer Mask Display Options: the rubylith's colour and opacity (`channel.options`).
    rows.push(MenuItem::new(tl!("Mask Options…"), Choice::MaskOptions).into());
    rows
}

/// The rows of `l`'s effect-row menu ([`effects_entries`]).
pub fn effects_rows(app: &crate::PhotocraftApp, l: &Layer) -> Vec<MenuEntry<Choice>> {
    command_rows(app, l, false, effects_entries())
}

/// The actions for a chosen row of `l`'s menu. Returns true when the row asks to rename `l`.
pub fn run_choice(choice: Choice, l: &Layer, on_set: bool, actions: &mut Vec<(String, Value)>) -> bool {
    let select = |actions: &mut Vec<(String, Value)>| actions.push(("layer.select".into(), json!({"layer": l.id.0})));
    match choice {
        Choice::Command(id) => {
            if !on_set {
                select(actions);
            }
            actions.push((id.into(), Value::Null));
        }
        Choice::Rename => return true,
        Choice::Color(color) => {
            let params = if on_set {
                json!({"color": color.id()})
            } else {
                select(actions);
                json!({"layer": l.id.0, "color": color.id()})
            };
            actions.push(("layer.setLabelColor".into(), params));
        }
        Choice::LoadMask(operation) => {
            select(actions);
            actions.push(("select.loadSelection".into(), json!({"channel": "mask", "layer": l.id.0, "operation": operation})));
        }
        Choice::SelectAndMask => {
            // Like double-clicking the thumbnail with that preference on: it refines the mask.
            select(actions);
            actions.push(("ui.maskTarget".into(), json!(true)));
            actions.push(("select.selectAndMask".into(), Value::Null));
        }
        Choice::MaskOptions => {
            select(actions);
            actions.push(("ui.maskTarget".into(), json!(true)));
            actions.push(("channel.options".into(), Value::Null));
        }
    }
    false
}

/// The layer style menu of `l`'s effect rows on `widget` (right-click); the actions select `l`
/// first when it isn't the active layer.
pub fn show_effects(app: &crate::PhotocraftApp, ui: &egui::Ui, widget: &egui::Response, l: &Layer, actions: &mut Vec<(String, Value)>) {
    let rows = effects_rows(app, l);
    if let Some(choice) = ContextMenu::new(&rows).show(ui, widget).chosen {
        run_choice(choice, l, false, actions);
    }
}

pub(crate) fn color_name(color: LabelColor) -> &'static str {
    crate::i18n::tr_ctx(crate::i18n::current(), "layerLabel", color.label())
}

fn common_color(app: &crate::PhotocraftApp, l: &Layer, on_set: bool) -> Option<LabelColor> {
    if !on_set {
        return Some(l.label);
    }
    let st = app.session.active()?;
    let ids = st.selected_layers();
    let mut labels = ids.iter().filter_map(|id| st.doc.layer(*id).map(|l| l.label));
    let first = labels.next()?;
    labels.all(|c| c == first).then_some(first)
}

/// The label-colour rows: the current colour checked, each colour tinting its row.
pub(crate) fn color_rows(app: &crate::PhotocraftApp, ctx: &egui::Context, l: &Layer, on_set: bool) -> Vec<MenuEntry<Choice>> {
    let current = common_color(app, l, on_set);
    let t = crate::theme::Tokens::get(ctx);
    let mut rows = Vec::new();
    for color in LabelColor::ALL {
        if color == LabelColor::Red {
            rows.push(MenuEntry::Separator);
        }
        let mut item = MenuItem::new(color_name(color), Choice::Color(color)).check(current == Some(color));
        if let Some((swatch, _)) = t.layer_label_colors(color) {
            item = item.tint(swatch);
        }
        rows.push(item.into());
    }
    rows
}

#[cfg(test)]
mod color_tests;

#[cfg(test)]
mod tests;
