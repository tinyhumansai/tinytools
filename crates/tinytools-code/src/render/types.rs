//! Caller bindings and rendered source receipts.

use crate::{ChunkTarget, Language};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Caller-provided values; inserted text is never expanded recursively.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkBindings {
    /// Typed parameter values by declared name.
    #[serde(default)]
    pub parameters: BTreeMap<String, Value>,
    /// Code supplied for declared slots.
    #[serde(default)]
    pub slots: BTreeMap<String, String>,
}

/// A validated rendering, without filesystem effects or dependency installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedChunk {
    /// Pattern ID.
    pub id: String,
    /// Grammar used.
    pub language: Language,
    /// Insertion context.
    pub target: ChunkTarget,
    /// Rendered code.
    pub source: String,
    /// Content revision of the rendered code.
    pub revision: String,
    /// Informational dependencies carried through from metadata.
    pub dependencies: Vec<String>,
}
