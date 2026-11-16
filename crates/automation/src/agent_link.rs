//! PhotoCraft's side of agents in the open desktop app: its agent settings as `photocraft-cli mcp`
//! reads them from the saved preferences, and whether the person lets agents in now. The switch,
//! the connection file and the link host are the framework's (`craft::automation::agents`): the
//! app serves its control channel on loopback while agents may connect and publishes where in its
//! settings folder, and `photocraft-cli mcp` ([`crate::live`]) reads that file on every tool call,
//! so MCP client configurations never hold a port or a token. Adding PhotoCraft to MCP clients is
//! the ArtCraft Launcher's job (one entry for every Craft app).

use std::path::Path;

use craft::automation::agents;
use photocraft_engine::prefs::Integrations;
use serde_json::Value;

use crate::AutomationError;

fn io_error(path: &Path, e: impl std::fmt::Display) -> AutomationError {
    AutomationError::Io(format!("{}: {e}", path.display()))
}

/// Largest preferences file read; anything bigger is not ours.
const MAX_CONFIG_BYTES: u64 = 16 << 20;

/// Read a small text file, refusing anything larger than [`MAX_CONFIG_BYTES`] or anything but a
/// regular file (a FIFO would block the read).
fn read_small(path: &Path) -> Result<Option<String>, AutomationError> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(path, e)),
    };
    if !meta.is_file() {
        return Err(io_error(path, "not a regular file"));
    }
    if meta.len() > MAX_CONFIG_BYTES {
        return Err(io_error(path, format!("larger than {MAX_CONFIG_BYTES} bytes")));
    }
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_error(path, e)),
    }
}

/// The MCP preferences saved in `dir/preferences.json` (defaults when the file or
/// the section is missing or unreadable: the CLI must not fail over the app's settings).
pub fn read_integrations(dir: &Path) -> Integrations {
    read_small(&dir.join("preferences.json"))
        .ok()
        .flatten()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|v| v.get("integrations").cloned())
        .map(photocraft_engine::prefs::read_integrations_section)
        .unwrap_or_default()
}

// ---- The agents switch -------------------------------------------------------------------------

/// The id the ArtCraft Launcher's app list names PhotoCraft by.
pub const LAUNCHER_APP_ID: &str = "photocraft";

/// Whether agents may use the open window now ([`agents::agents_allowed`]): with the suite's
/// `AI-MCP` flag the launcher's switch, without it a Control Port saved in the preferences
/// (`connect_agents`).
pub fn agents_allowed_now(integrations: &Integrations) -> bool {
    agents::agents_allowed(agents::mcp_flag(), agents::launcher_allows(LAUNCHER_APP_ID), integrations.connect_agents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp(tag: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!("photocraft-agent-link-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn integrations_come_from_the_saved_preferences_or_default() {
        let dir = temp("prefs");
        assert_eq!(read_integrations(&dir), Integrations::default());
        std::fs::write(dir.join("preferences.json"), "not json").unwrap();
        assert_eq!(read_integrations(&dir), Integrations::default());
        // A section of the wrong type doesn't fail the CLI either.
        std::fs::write(dir.join("preferences.json"), r#"{"integrations": {"connectAgents": "yes"}}"#).unwrap();
        assert_eq!(read_integrations(&dir), Integrations::default());
        std::fs::write(
            dir.join("preferences.json"),
            r#"{"general": {}, "integrations": {"connectAgents": true, "whenAppClosed": "headless", "readRoot": "/in"}}"#,
        )
        .unwrap();
        let i = read_integrations(&dir);
        assert!(i.connect_agents);
        assert_eq!(i.agents.when_app_closed, photocraft_engine::prefs::WhenAppClosed::Headless);
        assert_eq!(i.agents.read_folder(), Some(PathBuf::from("/in")));
        assert_eq!(i.agents.write_folder(), None);
        // A Control Port saved before the agents switch reads as on, as the app reads it.
        std::fs::write(dir.join("preferences.json"), r#"{"integrations": {"controlPort": 7878}}"#).unwrap();
        assert!(read_integrations(&dir).connect_agents);
        std::fs::write(dir.join("preferences.json"), r#"{"integrations": {"controlPort": 7878, "connectAgents": false}}"#).unwrap();
        assert!(!read_integrations(&dir).connect_agents);
        // One bad value resets only itself, as in the app: the saved closed-app choice stays.
        std::fs::write(dir.join("preferences.json"), r#"{"integrations": {"whenAppClosed": "error", "readRoot": 5}}"#).unwrap();
        assert_eq!(read_integrations(&dir).agents.when_app_closed, photocraft_engine::prefs::WhenAppClosed::Error);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
