//! Actionable refusals at syntax, revision and resource boundaries.

use crate::Language;
use std::fmt;

/// A structural operation could not safely produce its requested result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeError {
    /// A configured or fixed resource budget was exceeded.
    LimitExceeded {
        /// Resource being limited.
        resource: &'static str,
        /// Maximum accepted value.
        limit: usize,
        /// Requested value.
        actual: usize,
    },
    /// A request or manifest violates its contract.
    InvalidInput(String),
    /// A compiled grammar or parser could not be initialized.
    Parser(String),
    /// Source has missing or erroneous syntax nodes.
    InvalidSyntax {
        /// Explicit grammar used for validation.
        language: Language,
    },
    /// The source differs from the caller's snapshot.
    StaleRevision {
        /// Caller-provided revision.
        expected: String,
        /// Revision of the supplied source.
        actual: String,
    },
    /// No symbol matched the selector.
    SymbolNotFound(String),
    /// More than one symbol can match, or the outline was truncated.
    AmbiguousSymbol(String),
    /// The selected grammar or node does not support this operation.
    UnsupportedOperation(String),
    /// No catalog entry has the requested ID.
    UnknownChunk(String),
}

impl fmt::Display for CodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitExceeded {
                resource,
                limit,
                actual,
            } => write!(f, "{resource} exceeds limit {limit}: {actual}"),
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
            Self::Parser(message) => write!(f, "parser initialization failed: {message}"),
            Self::InvalidSyntax { language } => write!(f, "source does not parse as {language:?}"),
            Self::StaleRevision { expected, actual } => {
                write!(f, "stale revision: expected {expected}, current {actual}")
            }
            Self::SymbolNotFound(name) => write!(f, "symbol not found: {name}"),
            Self::AmbiguousSymbol(name) => write!(
                f,
                "symbol selector is not provably unique: {name}; use an exact revision-addressed ID"
            ),
            Self::UnsupportedOperation(message) => write!(f, "unsupported operation: {message}"),
            Self::UnknownChunk(id) => write!(f, "unknown chunk: {id}"),
        }
    }
}

impl std::error::Error for CodeError {}

pub(crate) fn enforce_limit(
    resource: &'static str,
    actual: usize,
    limit: usize,
) -> Result<(), CodeError> {
    if actual > limit {
        Err(CodeError::LimitExceeded {
            resource,
            actual,
            limit,
        })
    } else {
        Ok(())
    }
}

pub(crate) fn invalid(message: impl Into<String>) -> CodeError {
    CodeError::InvalidInput(message.into())
}
