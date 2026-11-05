//! A mask dragged onto the Layers panel's Trash asks first, as in Photoshop. A layer mask (#3113):
//! "Apply mask to layer before removing?" Apply runs `layer.layerMask.apply`, Delete
//! `layer.layerMask.delete`, Cancel keeps the mask. A vector mask (#3119): "Delete the vector
//! mask?" OK runs `layer.vectorMask.delete`, Cancel keeps it. A click on the Trash still deletes the
//! targeted mask at once (#2197).
//!
//! The prompt is a library [`StandardPrompt`] with a record in the UI state
//! (`dialogs::standard_prompts`): `ui.inspect` shows it, `ui.dialog.confirm`
//! applies (or deletes the vector mask), `ui.dialog.set` with `"choice": "delete"` then
//! `ui.dialog.confirm` deletes a layer mask, and `ui.dialog.cancel` keeps the mask.

use photocraft_doc::LayerId;
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;
use crate::mask_thumbs_ui::MaskKind;
use craft::ui::dialog::StandardPrompt;

const MARK: &str = "__maskTrash";
const LAYER_MASK_DELETE: &str = "layer.layerMask.delete";
const VECTOR_MASK_DELETE: &str = "layer.vectorMask.delete";

pub fn owns(fields: &Map<String, Value>) -> bool {
    fields.contains_key(MARK)
}

/// Is this the vector mask's prompt (OK / Cancel) rather than the layer mask's (Apply / Cancel / Delete)?
pub fn is_vector(fields: &Map<String, Value>) -> bool {
    fields.get("kind").and_then(Value::as_str) == Some("vector")
}

/// The action a mask thumbnail dropped on the Trash pushes; [`intercept`] turns it into the prompt.
pub fn drop_action(layer: u64, kind: MaskKind) -> (String, Value) {
    let command = if kind == MaskKind::Vector { VECTOR_MASK_DELETE } else { LAYER_MASK_DELETE };
    (command.into(), json!({"layer": layer, MARK: true}))
}

/// Opens the prompt for a [`drop_action`]; `None` for every other command.
pub fn intercept(app: &mut PhotocraftApp, command: &str, params: &Value) -> Option<Result<Value, String>> {
    let vector = match command {
        LAYER_MASK_DELETE => false,
        VECTOR_MASK_DELETE => true,
        _ => return None,
    };
    if params.get(MARK).and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let layer = params.get("layer").and_then(Value::as_u64)?;
    let state = app.session.active()?;
    state.doc.layer(LayerId(layer)).filter(|l| if vector { l.vector_mask.is_some() } else { l.mask.is_some() })?;
    let (fields, prompt, title) = if vector {
        let fields = json!({MARK: true, "kind": "vector", "__label": "Delete Vector Mask", "message": "Delete the vector mask?",
            "document": state.doc.id.0, "layer": layer});
        (fields, StandardPrompt::trash_vector_mask().message(tl!("Delete the vector mask?")), "Delete Vector Mask")
    } else {
        // Apply needs a pixel layer (Layer › Layer Mask › Apply is greyed out otherwise).
        let can_apply = app.session.is_enabled_with("layer.layerMask.apply", &json!({"layer": layer}));
        let fields = json!({MARK: true, "kind": "pixel", "__label": "Delete Layer Mask", "message": "Apply mask to layer before removing?",
            "document": state.doc.id.0, "layer": layer, "canApply": can_apply, "choice": if can_apply { "apply" } else { "delete" }});
        (fields, StandardPrompt::trash_mask().message(tl!("Apply mask to layer before removing?")).can_apply(can_apply), "Delete Layer Mask")
    };
    let prompt = prompt.title(tl!(title));
    let Some(id) = crate::standard_prompts::open(app, prompt, title, fields.as_object().cloned().unwrap_or_default()) else {
        return Some(Ok(json!({"confirmationPending": true})));
    };
    Some(Ok(json!({"dialog": id, "confirmationPending": true})))
}

/// Runs the answer: the vector mask is deleted; for a layer mask `choice` "delete" deletes it and
/// anything else applies it.
pub fn confirm(app: &mut PhotocraftApp, fields: &Map<String, Value>) -> Result<Value, String> {
    let state = app.session.active().ok_or(tl!("The document is closed"))?;
    let vector = is_vector(fields);
    let layer = fields.get("layer").and_then(Value::as_u64);
    let same = fields.get("document").and_then(Value::as_u64) == Some(state.doc.id.0);
    let has_mask = |l: &photocraft_doc::Layer| if vector { l.vector_mask.is_some() } else { l.mask.is_some() };
    let Some(layer) = layer.filter(|id| same && state.doc.layer(LayerId(*id)).is_some_and(has_mask)) else {
        return Err(tl!("The layer or its mask changed. Drag the mask onto the Trash again.").into());
    };
    let command = if vector {
        VECTOR_MASK_DELETE
    } else if fields.get("choice").and_then(Value::as_str) == Some("delete") {
        LAYER_MASK_DELETE
    } else {
        "layer.layerMask.apply"
    };
    app.run_command(command, json!({"layer": layer}))
}
