//! The outcome of running a tool, and the content blocks it carries.

use serde::{Deserialize, Serialize};

/// Result of executing a tool: content blocks plus an error flag.
///
/// The block list mirrors the Model Context Protocol's result shape, which is
/// what makes a tool backed by an MCP server and one implemented in Rust
/// interchangeable to the caller. [`Self::is_error`] is a *reported* failure —
/// the tool ran and said no — and is distinct from the `Err` arm of
/// [`Tool::execute`][crate::Tool::execute], which means the tool could not run
/// at all.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// List of content blocks returned by the tool.
    pub content: Vec<ToolContent>,
    /// Indicates if the tool encountered an error during execution.
    #[serde(default)]
    pub is_error: bool,
    /// Optional markdown rendering of the result.
    ///
    /// When the agent loop is configured with
    /// [`prefer_markdown`][crate::ToolCallOptions::prefer_markdown], this is
    /// sent to the model instead of the JSON-serialised content blocks:
    /// markdown is significantly cheaper than JSON in the context window. The
    /// wire name matches Composio's `markdownFormatted` response field so a
    /// proxied result needs no renaming.
    #[serde(
        default,
        rename = "markdownFormatted",
        skip_serializing_if = "Option::is_none"
    )]
    pub markdown_formatted: Option<String>,
}

impl ToolResult {
    /// A successful result carrying a single text block.
    pub fn success(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text { text: text.into() }],
            is_error: false,
            markdown_formatted: None,
        }
    }

    /// A failed result carrying the message as its only text block.
    ///
    /// This is the *reported* failure path: the tool ran and refused, and the
    /// model is expected to read the message and adapt.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text {
                text: message.into(),
            }],
            is_error: true,
            markdown_formatted: None,
        }
    }

    /// A successful result carrying a single JSON block.
    #[must_use]
    pub fn json(data: serde_json::Value) -> Self {
        Self {
            content: vec![ToolContent::Json { data }],
            is_error: false,
            markdown_formatted: None,
        }
    }

    /// A successful result carrying both a JSON payload (for programmatic
    /// consumers and debugging) and a markdown rendering (preferred by the
    /// agent loop when `prefer_markdown` is on).
    pub fn success_with_markdown(data: serde_json::Value, markdown: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Json { data }],
            is_error: false,
            markdown_formatted: Some(markdown.into()),
        }
    }

    /// Attaches (or replaces) the markdown rendering on an existing result.
    #[must_use]
    pub fn with_markdown(mut self, markdown: impl Into<String>) -> Self {
        self.markdown_formatted = Some(markdown.into());
        self
    }

    /// The markdown rendering when present and non-blank, otherwise
    /// [`Self::output`].
    ///
    /// A blank markdown field falls back rather than sending the model an empty
    /// turn: a tool that set the field but rendered nothing is a bug in the
    /// tool, and swallowing the real output would hide it.
    #[must_use]
    pub fn output_for_llm(&self, prefer_markdown: bool) -> String {
        if prefer_markdown
            && let Some(md) = self.markdown_formatted.as_deref()
            && !md.trim().is_empty()
        {
            return md.to_string();
        }
        self.output()
    }

    /// The text blocks alone, newline-joined. JSON blocks are skipped.
    #[must_use]
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                ToolContent::Text { text } => Some(text.as_str()),
                ToolContent::Json { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Every block rendered and newline-joined, with JSON blocks
    /// pretty-printed. This is what a model sees when no markdown rendering is
    /// preferred.
    #[must_use]
    pub fn output(&self) -> String {
        self.content
            .iter()
            .map(|c| match c {
                ToolContent::Text { text } => text.clone(),
                ToolContent::Json { data } => {
                    serde_json::to_string_pretty(data).unwrap_or_default()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A single content block within a [`ToolResult`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ToolContent {
    /// Plain text, rendered verbatim.
    Text {
        /// The text body.
        text: String,
    },
    /// Structured data, pretty-printed when rendered for a model.
    Json {
        /// The JSON body.
        data: serde_json::Value,
    },
}
