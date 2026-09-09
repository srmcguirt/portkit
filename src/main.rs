//! Reference binary for the portkit template.
//!
//! Serves the example tools from `portkit-demo`, plus the schema tools when
//! snapshots are configured. When you fork, swap the registry for your own —
//! this file should stay about this short.

use std::process::ExitCode;

use portkit_core::{Config, Registry};

#[tokio::main]
async fn main() -> ExitCode {
    portkit_cli::run_with(registry).await
}

/// Built from the effective config, so snapshots are loaded before
/// `tools/list` is answered — an agent's first view of the interface should
/// already be grounded rather than corrected after its first wrong guess.
fn registry(config: &Config) -> Registry {
    let mut registry = portkit_demo::registry();

    if !config.schema.snapshots.is_empty() {
        let (sources, problems) =
            portkit_schema::SchemaRegistry::from_paths(&config.schema.snapshots);
        for p in &problems {
            eprintln!("warning: could not load schema snapshot {p}");
        }
        if !sources.is_empty() {
            portkit_schema::register(&mut registry, sources);
        }
    }

    registry
}
