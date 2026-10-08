//! The progress vocabulary: one update, and the sink a host hands a tool.

use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// One incremental report from a tool that has not finished yet.
///
/// A tool that runs for seconds or minutes — a build, a download, a
/// sub-agent — has nothing to say through its [`ToolResult`][crate::ToolResult]
/// until it returns. A `ToolProgress` is how it says something *before* then:
/// a status line, a completion fraction, a chunk of partial output, or any
/// combination. Every field is optional so a tool reports only what it knows.
///
/// The type carries no policy. Whether an update is shown, throttled, recorded
/// or dropped is the host's decision, made when it supplies the
/// [`ToolRunContext::report_progress`][crate::ToolRunContext::report_progress]
/// implementation the tool reports through.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolProgress {
    /// A short human-readable status line ("compiling 14/32").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Completion in `0.0..=1.0`, when the tool can estimate it.
    ///
    /// [`Self::with_fraction`] clamps into range; a value set directly on the
    /// field is the host's to sanitize.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fraction: Option<f32>,
    /// Partial output produced so far, in whatever JSON shape the tool uses.
    ///
    /// This is advisory, not a result: the final [`ToolResult`][crate::ToolResult]
    /// is still the only thing the model is guaranteed to see.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial: Option<serde_json::Value>,
}

impl ToolProgress {
    /// An update carrying only a status line.
    #[must_use]
    pub fn message(message: impl Into<String>) -> Self {
        Self {
            message: Some(message.into()),
            ..Self::default()
        }
    }

    /// Adds a completion fraction, clamped to `0.0..=1.0`.
    ///
    /// `NaN` is not a fraction and leaves the field unset.
    #[must_use]
    pub fn with_fraction(mut self, fraction: f32) -> Self {
        self.fraction = (!fraction.is_nan()).then(|| fraction.clamp(0.0, 1.0));
        self
    }

    /// Adds the partial output produced so far.
    #[must_use]
    pub fn with_partial(mut self, partial: serde_json::Value) -> Self {
        self.partial = Some(partial);
        self
    }

    /// `true` when the update carries no message, fraction, or partial output.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.message.is_none() && self.fraction.is_none() && self.partial.is_none()
    }
}

/// A cloneable, thread-safe destination for [`ToolProgress`] updates.
///
/// A host that implements
/// [`ToolRunContext::report_progress`][crate::ToolRunContext::report_progress]
/// typically keeps one of these per call and forwards to it. It is a thin
/// wrapper over a shared closure so the host chooses the transport (a channel,
/// an event bus, a log line) without this crate naming any of them.
///
/// The default sink is a no-op, so code that needs *a* sink but has no host to
/// report to — a unit test, a tool run outside a loop — pays nothing.
#[derive(Clone, Default)]
pub struct ProgressSink {
    deliver: Option<Arc<dyn Fn(ToolProgress) + Send + Sync>>,
}

impl ProgressSink {
    /// A sink that forwards every update to `deliver`.
    ///
    /// `deliver` may be called from any thread, including a task the tool
    /// spawned, and must not block.
    pub fn new(deliver: impl Fn(ToolProgress) + Send + Sync + 'static) -> Self {
        Self {
            deliver: Some(Arc::new(deliver)),
        }
    }

    /// A sink that discards every update.
    #[must_use]
    pub fn noop() -> Self {
        Self::default()
    }

    /// Delivers one update.
    pub fn report(&self, update: ToolProgress) {
        if let Some(deliver) = &self.deliver {
            deliver(update);
        }
    }
}

impl fmt::Debug for ProgressSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProgressSink")
            .field("connected", &self.deliver.is_some())
            .finish()
    }
}
