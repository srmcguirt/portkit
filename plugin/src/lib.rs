//! Tools that live outside the binary.
//!
//! The contract already existed. `pk port capture` speaks JSON-on-stdin,
//! JSON-on-stdout to a reference implementation; a plugin is the same
//! conversation, plus a way to describe itself:
//!
//! ```text
//! $ mytool --portkit-spec
//! {"name":"mytool","description":"...","input_schema":{...}}
//!
//! $ echo '{"tool":"mytool","input":{...}}' | mytool
//! {"result":"..."}
//! ```
//!
//! That symmetry is the point. A Python tool can be a plugin today and a Rust
//! tool tomorrow, with the same fixtures proving the two agree:
//!
//! ```text
//! register as plugin → capture fixtures → port to Rust → replay → swap
//! ```
//!
//! # Discovery is explicit, never a PATH scan
//!
//! Plugins are listed in a manifest. Scanning a directory and executing what
//! it finds turns a dropped file into code execution; the convenience is not
//! worth it.

pub mod manifest;
pub mod tool;

pub use manifest::{load_manifest, Manifest, PluginEntry};
pub use tool::PluginTool;

use std::path::Path;

use portkit_core::{Registry, Result};

/// Load a manifest and register every plugin it declares.
///
/// Returns the problems rather than failing: one broken plugin should not stop
/// a server from starting, and a tool that is absent is easier to notice than
/// a server that would not boot.
pub async fn register_from(registry: &mut Registry, manifest: &Path) -> Result<Vec<String>> {
    let manifest = load_manifest(manifest)?;
    let mut problems = Vec::new();

    for entry in manifest.plugins {
        match PluginTool::discover(entry.clone()).await {
            Ok(tool) => {
                registry.register(tool);
            }
            Err(err) => problems.push(format!("{}: {err}", entry.label())),
        }
    }

    Ok(problems)
}
