//! Explicit compiled grammars and fixed query packs; no language auto-detection.

use serde::{Deserialize, Serialize};

/// A supported, explicitly selected grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    /// Rust source.
    Rust,
    /// TypeScript without JSX.
    #[serde(rename = "typescript", alias = "ts")]
    TypeScript,
    /// TypeScript with JSX.
    Tsx,
    /// JavaScript, including JSX.
    #[serde(rename = "javascript", alias = "js")]
    JavaScript,
    /// Python source.
    Python,
    /// Go source.
    Go,
    /// JSON data.
    Json,
    /// TOML data.
    Toml,
}

impl Language {
    /// Infer a supported grammar from a conventional filename extension.
    #[must_use]
    pub fn from_path(path: &str) -> Option<Self> {
        let extension = path.rsplit_once('.')?.1;
        match extension.to_ascii_lowercase().as_str() {
            "rs" => Some(Self::Rust),
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "js" | "jsx" | "mjs" | "cjs" => Some(Self::JavaScript),
            "py" | "pyi" => Some(Self::Python),
            "go" => Some(Self::Go),
            "json" => Some(Self::Json),
            "toml" => Some(Self::Toml),
            _ => None,
        }
    }

    pub(crate) fn grammar(self) -> tree_sitter::Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::Json => tree_sitter_json::LANGUAGE.into(),
            Self::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
        }
    }

    pub(crate) fn query(self) -> &'static str {
        match self {
            Self::Rust => include_str!("queries/rust.scm"),
            Self::TypeScript | Self::Tsx => include_str!("queries/typescript.scm"),
            Self::JavaScript => include_str!("queries/javascript.scm"),
            Self::Python => include_str!("queries/python.scm"),
            Self::Go => include_str!("queries/go.scm"),
            Self::Json => include_str!("queries/json.scm"),
            Self::Toml => include_str!("queries/toml.scm"),
        }
    }
}
