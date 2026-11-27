//! The panel's edits as PhotoCraft's own commands, pushed into `layers()`'s `actions` vec so its execution loop runs them.

use craft_rt_layers::{Op, SelectMode};
use photocraft_doc::{Document, LayerId};
use serde_json::{Value, json};

use crate::panels::layer_drag::layer_drop_payload;
use crate::panels::layers::simple_lock_toggle;

/// What the mapping reads of the frame.
pub(super) struct Frame<'a> {
    pub doc: &'a Document,
    /// The `coalesce` key of a running eye-column drag: its rows are one history step, as in the egui rows.
    pub eye_sweep: &'a mut Option<u64>,
    pub pass: u64,
    pub primary_down: bool,
}

/// Maps `ops` (oldest first) onto commands. A toggle reads the layer's state in `doc`, as the egui row does.
pub(super) fn to_actions(ops: Vec<Op>, f: &mut Frame<'_>, out: &mut Vec<(String, Value)>) {
    let mut sweeping = false;
    for op in ops {
        match op {
            Op::Select(id, mode) => {
                let mode = match mode {
                    SelectMode::Replace => "replace",
                    SelectMode::Toggle => "toggle",
                    SelectMode::Range => "range",
                };
                out.push(("layer.select".into(), json!({"layer": id, "mode": mode})));
            }
            Op::SetSelection(ids) => {
                for (i, id) in ids.iter().enumerate() {
                    out.push(("layer.select".into(), json!({"layer": id, "mode": if i == 0 { "replace" } else { "add" }})));
                }
            }
            Op::ClearSelection => out.push(("select.deselectLayers".into(), json!({}))),
            Op::ToggleVisible(id) => {
                let Some(l) = f.doc.layer(LayerId(id)) else { continue };
                sweeping = true;
                let key = *f.eye_sweep.get_or_insert(f.pass);
                out.push(("layer.setProps".into(), json!({"layer": id, "visible": !l.visible, "coalesce": format!("layer-eye-sweep:{key}")})));
            }
            Op::ToggleLock(id) => {
                let Some(l) = f.doc.layer(LayerId(id)) else { continue };
                out.push(simple_lock_toggle(crate::doc_props_ui::is_background(f.doc, l), l, &json!({"layer": id})));
            }
            Op::Rename(id, name) => out.push(("layer.setProps".into(), json!({"layer": id, "name": name}))),
            Op::Move { ids, target, after, copy } => {
                let Some(&first) = ids.first() else { continue };
                let moved: Vec<LayerId> = ids.iter().map(|id| LayerId(*id)).collect();
                let mut payload = layer_drop_payload(first, LayerId(target), if after { "below" } else { "above" }, &moved);
                if copy && let Some(o) = payload.as_object_mut() {
                    o.insert("copy".into(), json!(true));
                }
                out.push(("layer.moveTo".into(), payload));
            }
            Op::AddLayer => out.push(("layer.new.layer".into(), json!({}))),
            Op::DeleteSelected => out.push(("layer.delete".into(), json!({"__trash": true}))),
        }
    }
    // A sweep ends with the button: the next eye click starts a new history step.
    if !sweeping && !f.primary_down {
        *f.eye_sweep = None;
    }
}
