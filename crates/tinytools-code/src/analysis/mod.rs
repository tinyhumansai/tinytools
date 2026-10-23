//! Lexical query captures, syntax diagnostics and source-preserving folded views.

mod extract;
mod fold;
mod types;

use crate::{CacheStatus, CodeError, CodeLimits, Language, error::enforce_limit, revision};
pub(crate) use extract::{body_node, bounded_text, node_text};
pub(crate) use fold::is_docstring;
use tree_sitter::{Query, Tree};
pub use types::{Analysis, Diagnostic, Reference, SourceRange, Symbol};

/// Resolve an exact revision-addressed ID or a provably unique lexical name.
///
/// # Errors
/// Refuses oversized selectors, missing symbols, ambiguous names and name-based
/// selection from truncated outlines. Exact IDs remain usable after truncation.
pub fn select_symbol<'a>(analysis: &'a Analysis, selector: &str) -> Result<&'a Symbol, CodeError> {
    enforce_limit("symbol selector bytes", selector.len(), 4096)?;
    if let Some(symbol) = analysis.symbols.iter().find(|symbol| symbol.id == selector) {
        return Ok(symbol);
    }
    if analysis.symbols_truncated {
        return Err(CodeError::AmbiguousSymbol(selector.to_owned()));
    }
    let mut matches = analysis
        .symbols
        .iter()
        .filter(|symbol| symbol.name == selector || symbol.qualified_name == selector);
    let first = matches
        .next()
        .ok_or_else(|| CodeError::SymbolNotFound(selector.to_owned()))?;
    if matches.next().is_some() {
        Err(CodeError::AmbiguousSymbol(selector.to_owned()))
    } else {
        Ok(first)
    }
}

pub(crate) fn analyze(
    tree: &Tree,
    query: &Query,
    language: Language,
    source: &str,
    limits: CodeLimits,
    cache_status: CacheStatus,
) -> Analysis {
    let revision = revision(source);
    let (symbols, references, symbols_truncated, references_truncated) =
        extract::captures(tree, query, source, &revision, limits.max_records);
    let (diagnostics, diagnostics_truncated) = extract::diagnostics(tree, limits.max_diagnostics);
    let skeleton = fold::fold(tree, source, language, &symbols);
    Analysis {
        language,
        revision,
        symbols,
        references,
        diagnostics,
        skeleton,
        symbols_truncated,
        references_truncated,
        diagnostics_truncated,
        cache_status,
    }
}
