//! Per-invocation inputs that are not part of a tool's argument schema.

/// Per-invocation options threaded from the agent loop into a tool.
///
/// These let a caller hint at how the tool should shape its output without
/// polluting the tool's model-visible parameter schema — the model never sees
/// these, and never has to be told not to set them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ToolCallOptions {
    /// The caller prefers a markdown rendering of the result, because markdown
    /// is materially cheaper than JSON in model context.
    ///
    /// A tool that honours this populates
    /// [`ToolResult::markdown_formatted`][crate::ToolResult::markdown_formatted]
    /// and advertises the capability from
    /// [`Tool::supports_markdown`][crate::Tool::supports_markdown]. A tool that
    /// ignores it stays correct — the caller falls back to the rendered blocks.
    pub prefer_markdown: bool,
}

impl ToolCallOptions {
    /// Options requesting a markdown rendering.
    #[must_use]
    pub fn prefer_markdown() -> Self {
        Self {
            prefer_markdown: true,
        }
    }
}

/// How the harness should bound a single tool invocation in wall-clock time.
///
/// Returned by [`Tool::timeout_policy`][crate::Tool::timeout_policy]. The three
/// arms exist because scripting tools and network tools want opposite defaults:
/// a hung HTTP call must not wedge a session, but a build or test run
/// legitimately takes minutes and must not be hard-killed by a network-shaped
/// cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolTimeout {
    /// Use the global, operator- and config-driven tool timeout. The right
    /// default for most tools.
    #[default]
    Inherit,
    /// Run without any harness-imposed deadline. Scripting tools return this
    /// when the caller did not request an explicit budget.
    Unbounded,
    /// Enforce exactly this many seconds. A host is expected to clamp the value
    /// into its own valid range rather than trust it.
    Secs(u64),
}

impl ToolTimeout {
    /// Returns `true` for the default inherited behaviour.
    #[must_use]
    pub fn is_inherit(&self) -> bool {
        matches!(self, Self::Inherit)
    }
}
