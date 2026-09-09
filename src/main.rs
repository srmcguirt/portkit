//! Reference binary for the portkit template.
//!
//! It serves the example tools from `portkit-demo`. When you fork, swap that
//! registry for your own — this file should stay about this short.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    portkit_cli::run(portkit_demo::registry()).await
}
