//! Serializable lexical analysis results and source coordinates.

use crate::{CacheStatus, Language};
use serde::{Deserialize, Serialize};

/// UTF-8 byte offsets and one-based line / byte-column coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRange {
    /// Inclusive byte start.
    pub start_byte: usize,
    /// Exclusive byte end.
    pub end_byte: usize,
    /// One-based start line.
    pub start_line: usize,
    /// One-based start byte column.
    pub start_column: usize,
    /// One-based end line.
    pub end_line: usize,
    /// One-based exclusive end byte column.
    pub end_column: usize,
}

impl From<tree_sitter::Node<'_>> for SourceRange {
    fn from(node: tree_sitter::Node<'_>) -> Self {
        let start = node.start_position();
        let end = node.end_position();
        Self {
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            start_line: start.row + 1,
            start_column: start.column + 1,
            end_line: end.row + 1,
            end_column: end.column + 1,
        }
    }
}

/// A lexical declaration captured by the language's bundled query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    /// Revision-addressed, exact-range identifier.
    pub id: String,
    /// Local declaration name.
    pub name: String,
    /// Lexical ancestor names joined with `::`; not semantic resolution.
    pub qualified_name: String,
    /// Grammar node kind.
    pub kind: String,
    /// Entire declaration range.
    pub range: SourceRange,
    /// Body/container range, when this declaration has one.
    pub body: Option<SourceRange>,
    /// Declaration prefix, bounded to 4096 UTF-8 bytes.
    pub signature: String,
    /// Whether the signature exceeded its byte limit.
    pub signature_truncated: bool,
}

/// A lexical call or import occurrence, without resolved target identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    /// Query classification such as `call` or `import`.
    pub kind: String,
    /// Captured source text, at most 1024 bytes.
    pub text: String,
    /// Captured range.
    pub range: SourceRange,
    /// Whether the text was shortened.
    pub text_truncated: bool,
}

/// A missing or erroneous syntax node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// `error` or `missing`.
    pub kind: String,
    /// Syntax-only description.
    pub message: String,
    /// Node location.
    pub range: SourceRange,
}

/// Bounded analysis for one explicit source snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Analysis {
    /// Grammar used.
    pub language: Language,
    /// BLAKE3 content revision.
    pub revision: String,
    /// Lexical declarations in source order.
    pub symbols: Vec<Symbol>,
    /// Lexical call/import captures.
    pub references: Vec<Reference>,
    /// Syntax diagnostics.
    pub diagnostics: Vec<Diagnostic>,
    /// Source with captured declaration bodies folded.
    pub skeleton: String,
    /// Symbol count or query execution limit was reached.
    pub symbols_truncated: bool,
    /// Reference count or query execution limit was reached.
    pub references_truncated: bool,
    /// Diagnostic count limit was reached.
    pub diagnostics_truncated: bool,
    /// Tree reuse outcome for this request.
    pub cache_status: CacheStatus,
}
