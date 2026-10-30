//! History › New Snapshot (⌥-click the History panel's camera button, #2966): Photoshop's dialog
//! for `history.newSnapshot`, with the snapshot's Name and what it is taken From (the full
//! document, its layers merged, or only the current layer). The command stays directly callable
//! with the same params by agents and Actions.

use craft_ui::composites::FormGrid;
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;

pub const COMMAND: &str = "history.newSnapshot";
const MARK: &str = "__newSnapshot";

pub fn owns(fields: &Map<String, Value>) -> bool {
    fields.contains_key(MARK)
}

/// Open the dialog on the active document, suggesting the name the camera button would give.
pub fn open(app: &mut PhotocraftApp) -> Option<u64> {
    let name = photocraft_engine::history_cmds::next_snapshot_name(app.session.active()?);
    let mut f = Map::new();
    f.insert(MARK.into(), json!(true));
    f.insert("__command".into(), json!(COMMAND));
    f.insert("__label".into(), json!("New Snapshot"));
    f.insert("name".into(), json!(name));
    f.insert("from".into(), json!("fullDocument"));
    Some(app.ui.open_dialog(crate::state::DialogKind::Command, f))
}

pub fn confirm(app: &mut PhotocraftApp, f: &Map<String, Value>) -> Result<Value, String> {
    app.run(COMMAND, json!({"name": f.get("name"), "from": f.get("from")}))
}

pub fn body(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    use craft_ui::controls::Select;
    FormGrid::new("new-snapshot").show(ui, |form| {
        form.row(tl!("Name:"), |ui, _| {
            let mut name = f.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
            let edit = crate::widgets::dialog_text(ui, "new-snapshot-name", tl!("Name:"), &mut name);
            if edit.changed {
                f.insert("name".into(), json!(name));
            }
            edit.response
        });
        form.row(tl!("From:"), |ui, _| {
            let mut from = f.get("from").and_then(Value::as_str).unwrap_or("fullDocument").to_string();
            let opts = [
                ("fullDocument".to_string(), tl!("Full Document")),
                ("mergedLayers".to_string(), tl!("Merged Layers")),
                ("currentLayer".to_string(), tl!("Current Layer")),
            ];
            let edit = Select::new("new-snapshot-from", tl!("From"), &mut from).rows(opts.len(), |i| opts[i].0.clone(), |i| tl!(opts[i].1)).show(ui);
            if edit.response.changed() {
                f.insert("from".into(), json!(from));
            }
            edit
        });
    });
}
