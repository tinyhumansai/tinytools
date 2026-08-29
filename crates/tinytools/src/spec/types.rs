//! The declaration a model is shown for a tool.

use serde::{Deserialize, Serialize};

/// A tool as the model sees it: a name, a description, and a JSON Schema for
/// its arguments.
///
/// This is the *host-facing* declaration. It is deliberately narrower than a
/// harness's model-visible schema type, which additionally carries the
/// tool-call dialect a provider should be given. A host builds one of these
/// from a [`Tool`][crate::Tool] and lets the harness decide how to render it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Canonical tool name, ASCII `snake_case` by convention.
    pub name: String,
    /// Human- and model-readable description of what the tool does.
    pub description: String,
    /// JSON Schema describing the tool's arguments.
    pub parameters: serde_json::Value,
}
