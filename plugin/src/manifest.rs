//! What plugins exist, declared rather than discovered.

use std::path::Path;

use serde::{Deserialize, Serialize};

use portkit_core::{Error, Result};

/// Default location, relative to the working directory.
pub const DEFAULT_PATH: &str = ".portkit/plugins.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default, rename = "plugin")]
    pub plugins: Vec<PluginEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEntry {
    /// Executable to run. Not looked up on `PATH` unless it has no separator,
    /// so a manifest can pin an absolute path.
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Overrides the name the plugin reports. Useful when two plugins collide
    /// or a name needs a prefix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Seconds before a call is abandoned. A plugin that hangs would otherwise
    /// hang the agent waiting on it.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Working directory for the subprocess.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

fn default_timeout() -> u64 {
    30
}

impl PluginEntry {
    /// Human-readable identifier for error messages.
    pub fn label(&self) -> String {
        match &self.name {
            Some(n) => n.clone(),
            None => self.command.clone(),
        }
    }
}

pub fn load_manifest(path: &Path) -> Result<Manifest> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| Error::Config(format!("{}: {e}", path.display())))?;
    toml::from_str(&raw).map_err(|e| Error::Config(format!("{}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Manifest {
        toml::from_str(s).unwrap()
    }

    #[test]
    fn an_empty_manifest_is_valid() {
        assert!(parse("").plugins.is_empty());
    }

    #[test]
    fn a_minimal_entry_gets_a_timeout() {
        // A plugin without a timeout would hang the agent waiting on it.
        let m = parse("[[plugin]]\ncommand = \"mytool\"\n");
        assert_eq!(m.plugins[0].timeout_secs, 30);
        assert_eq!(m.plugins[0].label(), "mytool");
    }

    #[test]
    fn a_named_entry_reports_its_override() {
        let m = parse("[[plugin]]\ncommand = \"node\"\nargs = [\"t.js\"]\nname = \"browse\"\n");
        assert_eq!(m.plugins[0].label(), "browse");
        assert_eq!(m.plugins[0].args, ["t.js"]);
    }
}
