//! Revision-bound edit request and result contracts.

use serde::{Deserialize, Serialize};

/// A targeted syntactic operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchOperation {
    /// Replace a declaration body, preserving its delimiters and signature.
    ReplaceBody,
    /// Replace the selected declaration in full.
    ReplaceSymbol,
    /// Insert exactly one import into the module prologue.
    InsertImport,
    /// Append a member to a supported declaration container.
    AppendMember,
    /// Expand exactly one `{{node}}` marker around the selected declaration.
    Wrap,
}

/// Every patch requires the caller's exact source revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchRequest {
    /// Expected content revision.
    pub expected_revision: String,
    /// Exact ID, qualified name or unambiguous name; unused for imports.
    pub selector: Option<String>,
    /// Syntactic edit operation.
    pub operation: PatchOperation,
    /// Replacement source or one-pass wrapping template.
    pub code: String,
}

/// A pure patch result; an adapter decides whether and how to persist it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchResult {
    /// Original source revision.
    pub before_revision: String,
    /// Resulting source revision.
    pub after_revision: String,
    /// Entire resulting source.
    pub source: String,
    /// Whether any source byte changed.
    pub changed: bool,
}
