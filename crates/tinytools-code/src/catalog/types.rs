//! Portable, typed chunk metadata; dependency declarations are informational.

use crate::Language;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where rendered source is intended to be inserted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkTarget {
    /// A complete source file.
    #[default]
    File,
    /// Statements replacing a function body.
    FunctionBody,
    /// A field, method or member inside a supported container.
    Member,
}

/// Validation and escaping rule for one parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterKind {
    /// Conservative ASCII identifier.
    Identifier,
    /// ASCII identifier beginning with an uppercase letter.
    PascalCaseIdent,
    /// Exactly one grammar type expression.
    TypeExpr,
    /// Escaped string literal.
    String,
    /// JSON finite numeric value.
    Number,
    /// Boolean literal for the selected language.
    Boolean,
}

/// One named typed placeholder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkParameter {
    /// Placeholder name in `{{name}}` syntax.
    pub name: String,
    /// Validation and escaping rule.
    pub kind: ParameterKind,
    /// Whether omission without a default is refused.
    #[serde(default)]
    pub required: bool,
    /// Optional typed default.
    #[serde(default)]
    pub default: Option<Value>,
    /// Human-readable purpose.
    #[serde(default)]
    pub description: String,
}

/// One standalone comment slot for code supplied by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkSlot {
    /// Name following `@slot` in a standalone comment.
    pub name: String,
    /// Whether an omitted or empty binding is refused.
    #[serde(default)]
    pub required: bool,
    /// Optional exact visible, named grammar node kind for this slot's expansion.
    #[serde(default)]
    pub node_kind: Option<String>,
    /// Human-readable purpose.
    #[serde(default)]
    pub description: String,
}

/// A versioned chunk's metadata, separate from template contents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkManifest {
    /// Stable, versioned catalog ID.
    pub id: String,
    /// Short human-readable summary.
    pub summary: String,
    /// Grammar used for validation.
    pub language: Language,
    /// Insertion context.
    #[serde(default)]
    pub target: ChunkTarget,
    /// Portable relative template path, interpreted by an adapter.
    pub template_path: String,
    /// Typed parameters.
    #[serde(default)]
    pub parameters: Vec<ChunkParameter>,
    /// Standalone code slots.
    #[serde(default)]
    pub slots: Vec<ChunkSlot>,
    /// Search metadata.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Informational package requirements; never automatically installed.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Visible, named grammar node kinds forbidden in generated source.
    #[serde(default)]
    pub forbidden_nodes: Vec<String>,
}

/// Metadata-only search result, excluding template bodies and defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkSummary {
    /// Versioned catalog ID.
    pub id: String,
    /// Human-readable purpose.
    pub summary: String,
    /// Grammar supported by this pattern.
    pub language: Language,
    /// Intended insertion context.
    pub target: ChunkTarget,
    /// Search tags.
    pub tags: Vec<String>,
}
