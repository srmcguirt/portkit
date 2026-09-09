//! Core abstractions for portkit.
//!
//! Everything hangs off one trait, [`Tool`]. Implement it once and the tool is
//! reachable three ways — as a CLI subcommand, as an MCP tool an agent can
//! call, and as a fixture the parity harness replays against the Python
//! implementation you are porting away from.
//!
//! ```
//! use portkit_core::{Registry, Result, Tool, ToolSpec};
//! use serde_json::{json, Value};
//!
//! struct Upper;
//!
//! #[portkit_core::async_trait]
//! impl Tool for Upper {
//!     fn spec(&self) -> ToolSpec {
//!         ToolSpec::new(
//!             "upper",
//!             "Uppercase a string.",
//!             json!({
//!                 "type": "object",
//!                 "properties": {"text": {"type": "string"}},
//!                 "required": ["text"]
//!             }),
//!         )
//!     }
//!
//!     async fn call(&self, input: Value) -> Result<Value> {
//!         let text = input.get("text").and_then(Value::as_str).unwrap_or_default();
//!         Ok(json!({"text": text.to_uppercase()}))
//!     }
//! }
//!
//! # tokio_test_shim(async {
//! let registry = Registry::new().with(Upper);
//! let out = registry.call("upper", json!({"text": "hi"})).await.unwrap();
//! assert_eq!(out, json!({"text": "HI"}));
//! # });
//! # fn tokio_test_shim<F: std::future::Future>(f: F) { let _ = f; }
//! ```

pub mod config;
pub mod diff;
pub mod error;
pub mod registry;
pub mod tool;

pub use async_trait::async_trait;

pub use config::Config;
pub use diff::{diff, matches, DiffKind, DiffOptions, Difference};
pub use error::{Error, Result};
pub use registry::Registry;
pub use tool::{Tool, ToolSpec};
