//! The outcome of running a tool, and the content blocks it carries.

use serde::{Deserialize, Serialize};

/// Result of executing a tool: content blocks plus an error flag.
///
/// The block list is *conceptually* shaped like the Model Context Protocol's
/// result — a list of content blocks plus a reported-error flag — which is what
/// lets a tool backed by an MCP server and one implemented in Rust share one
/// internal representation. **This is this crate's own on-the-wire shape for
/// agent transcripts, RPC replies and JSONL session records, not a literal MCP
/// `CallToolResult`**: field names are `snake_case` here (`is_error`, not MCP's
/// `isError`) to match every other type in this vocabulary, and
/// [`ToolContent::Json`] is a block kind of this crate's own, not MCP's
/// `structuredContent`. A host that actually speaks the MCP wire protocol to a
/// real MCP server is responsible for translating between that server's
/// `CallToolResult` and this type — same as it already must for whichever
/// content types each specific server chooses to send — rather than this crate
/// picking one server's exact JSON casing as its own internal format.
/// [`Self::is_error`] is a *reported* failure — the tool ran and said no — and
/// is distinct from the `Err` arm of [`Tool::execute`][crate::Tool::execute],
/// which means the tool could not run at all.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// Content the caller should present to the model as a *separate* user
    /// message after the tool result, rather than folding it into the result
    /// itself — a screenshot a vision-capable model should look at, a document
    /// a follow-up turn should read.
    ///
    /// This is deliberately not part of [`Self::content`]: the tool-result
    /// message answers the call, while follow-up content is context handed to
    /// the model *afterwards*. [`Self::text`], [`Self::output`] and
    /// [`Self::output_for_llm`] never include it — a host that wants to honour
    /// it reads this field directly and decides how to place it on the wire.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub follow_up: Vec<ToolContent>,
    /// Host-only metadata: never shown to the model, but available to the host
    /// for events, persistence, or telemetry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// Loop-control hints a harness may honour, such as ending the loop
    /// immediately or steering a graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<ToolControl>,
    /// Distinguishes a reported failure the model should retry from one it
    /// should not. `None` (the historical shape) means the caller has not
    /// classified the failure either way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<ToolErrorKind>,
}

impl ToolResult {
    /// A successful result carrying a single text block.
    pub fn success(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text { text: text.into() }],
            ..Self::default()
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
            ..Self::default()
        }
    }

    /// A reported failure the model should be told to retry, distinct from a
    /// permanent one — Pydantic AI's `ModelRetry`.
    ///
    /// Sets [`Self::is_error`] and tags [`Self::error_kind`] as
    /// [`ToolErrorKind::Retry`]; a harness that reads the tag can choose to
    /// coach the model to try again rather than giving up.
    pub fn retry(message: impl Into<String>) -> Self {
        let mut result = Self::error(message);
        result.error_kind = Some(ToolErrorKind::Retry);
        result
    }

    /// A reported failure that must not be retried — permanent, distinct from
    /// [`Self::retry`].
    ///
    /// Sets [`Self::is_error`] and tags [`Self::error_kind`] as
    /// [`ToolErrorKind::Failed`].
    pub fn failed(message: impl Into<String>) -> Self {
        let mut result = Self::error(message);
        result.error_kind = Some(ToolErrorKind::Failed);
        result
    }

    /// A successful result carrying a single JSON block.
    #[must_use]
    pub fn json(data: serde_json::Value) -> Self {
        Self {
            content: vec![ToolContent::Json { data }],
            ..Self::default()
        }
    }

    /// A successful result carrying both a JSON payload (for programmatic
    /// consumers and debugging) and a markdown rendering (preferred by the
    /// agent loop when `prefer_markdown` is on).
    pub fn success_with_markdown(data: serde_json::Value, markdown: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Json { data }],
            markdown_formatted: Some(markdown.into()),
            ..Self::default()
        }
    }

    /// Attaches (or replaces) the markdown rendering on an existing result.
    #[must_use]
    pub fn with_markdown(mut self, markdown: impl Into<String>) -> Self {
        self.markdown_formatted = Some(markdown.into());
        self
    }

    /// Appends content the caller should present to the model as a separate,
    /// follow-up message. See [`Self::follow_up`].
    #[must_use]
    pub fn with_follow_up(mut self, content: impl IntoIterator<Item = ToolContent>) -> Self {
        self.follow_up.extend(content);
        self
    }

    /// Appends an image block to [`Self::content`].
    #[must_use]
    pub fn with_image(mut self, media_type: impl Into<String>, data: ImageData) -> Self {
        self.content.push(ToolContent::Image {
            media_type: media_type.into(),
            data,
        });
        self
    }

    /// Attaches (or replaces) host-only metadata never shown to the model.
    #[must_use]
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Marks the result as one the harness should return directly to the
    /// caller without further model interaction.
    #[must_use]
    pub fn return_direct(mut self) -> Self {
        self.control_mut().return_direct = true;
        self
    }

    /// Marks the result as one that should end the agent loop.
    #[must_use]
    pub fn terminate(mut self) -> Self {
        self.control_mut().terminate = true;
        self
    }

    /// Requests that the harness route to a named node or step next.
    #[must_use]
    pub fn with_goto(mut self, node: impl Into<String>) -> Self {
        self.control_mut().goto = Some(node.into());
        self
    }

    /// Attaches a state update the harness may fold into its graph or session
    /// state.
    #[must_use]
    pub fn with_state_update(mut self, update: serde_json::Value) -> Self {
        self.control_mut().state_update = Some(update);
        self
    }

    /// Returns the [`ToolControl`], creating a default one if absent.
    fn control_mut(&mut self) -> &mut ToolControl {
        self.control.get_or_insert_with(ToolControl::default)
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

    /// The text blocks alone, newline-joined, with a short placeholder in
    /// place of non-text blocks other than JSON, which is skipped entirely.
    #[must_use]
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(ToolContent::text_or_placeholder)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Every block rendered and newline-joined, with JSON blocks
    /// pretty-printed and other non-text blocks rendered as a short
    /// placeholder. This is what a model sees when no markdown rendering is
    /// preferred.
    #[must_use]
    pub fn output(&self) -> String {
        self.content
            .iter()
            .map(ToolContent::render)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// How image bytes are referenced in a [`ToolContent::Image`] block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum ImageData {
    /// Base64-encoded image bytes, inline.
    Base64(String),
    /// A URL the host may fetch the image from.
    Url(String),
}

/// How file bytes are referenced in a [`ToolContent::File`] block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum FileData {
    /// Base64-encoded file bytes, inline.
    Base64(String),
    /// A URL the host may fetch the file from.
    Url(String),
    /// A path on a filesystem the host and tool both have access to.
    Path(String),
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
    /// Image bytes or a reference to them.
    Image {
        /// The image's MIME type, e.g. `image/png`.
        media_type: String,
        /// The image bytes or reference.
        data: ImageData,
    },
    /// File bytes or a reference to them.
    File {
        /// The file's display name.
        name: String,
        /// The file's MIME type.
        media_type: String,
        /// The file bytes or reference.
        data: FileData,
    },
}

impl ToolContent {
    /// Renders this block as a model would see it: text verbatim, JSON
    /// pretty-printed, and a short placeholder for an image or file.
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::Text { text } => text.clone(),
            Self::Json { data } => serde_json::to_string_pretty(data).unwrap_or_default(),
            Self::Image { media_type, .. } => format!("[image {media_type}]"),
            Self::File {
                name, media_type, ..
            } => format!("[file {name} ({media_type})]"),
        }
    }

    /// Like [`Self::render`], but returns `None` for a JSON block so
    /// [`ToolResult::text`] can skip it entirely rather than rendering it.
    #[must_use]
    fn text_or_placeholder(&self) -> Option<String> {
        match self {
            Self::Json { .. } => None,
            other => Some(other.render()),
        }
    }
}

/// Distinguishes a reported tool failure the model should retry from one it
/// should not.
///
/// Modelled on Pydantic AI's `ModelRetry` versus a permanent tool failure: both
/// set [`ToolResult::is_error`], but a harness that reads this tag can decide
/// whether to loop the model back in or surface the failure as final.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolErrorKind {
    /// The failure is transient or correctable; ask the model to try again.
    Retry,
    /// The failure is permanent; do not retry.
    Failed,
}

/// Loop-control hints a harness may honour after a tool call.
///
/// These are hints, not enforcement — same as [`crate::ToolPolicy`], a harness
/// decides whether and how to act on them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolControl {
    /// Per-call override for returning this result directly to the caller
    /// without further model interaction.
    ///
    /// Tri-state, not a defaulted `bool`: `None` means this call did not
    /// express an opinion, so a harness should fall back to the tool's
    /// static [`Tool::return_direct`][crate::Tool::return_direct] default
    /// rather than treating an absent override as an explicit `false`. A
    /// call that only used [`ToolResult::with_goto`],
    /// [`ToolResult::with_state_update`], or [`ToolResult::terminate`] — none
    /// of which touch this field — must not silently suppress a tool's
    /// static `true` declaration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_direct: Option<bool>,
    /// End the agent loop after this call.
    #[serde(default)]
    pub terminate: bool,
    /// Route to a named node or step next, for a harness with a graph or
    /// state machine underneath it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goto: Option<String>,
    /// A state update the harness may fold into its graph or session state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_update: Option<serde_json::Value>,
}
