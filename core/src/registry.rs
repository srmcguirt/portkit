//! A name-addressed set of tools, shared by the CLI, MCP, and parity surfaces.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;

use crate::error::{Error, Result};
use crate::tool::{Tool, ToolSpec};

/// The set of tools a binary exposes.
///
/// `BTreeMap` rather than `HashMap` so `pk tools`, MCP `tools/list`, and
/// generated docs all agree on ordering run to run.
#[derive(Default, Clone)]
pub struct Registry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool, replacing any previous tool of the same name.
    pub fn register<T: Tool>(&mut self, tool: T) -> &mut Self {
        let tool: Arc<dyn Tool> = Arc::new(tool);
        self.tools.insert(tool.name(), tool);
        self
    }

    /// Builder-style [`Registry::register`], for `Registry::new().with(A).with(B)`.
    #[must_use]
    pub fn with<T: Tool>(mut self, tool: T) -> Self {
        self.register(tool);
        self
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().map(String::as_str).collect()
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.values().map(|t| t.spec()).collect()
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Look up and invoke a tool by name.
    pub async fn call(&self, name: &str, input: Value) -> Result<Value> {
        let tool = self
            .get(name)
            .ok_or_else(|| Error::UnknownTool(name.to_string()))?
            .clone();
        tool.call(input).await
    }
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("tools", &self.names())
            .finish()
    }
}
