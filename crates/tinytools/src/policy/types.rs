//! Data types for a tool's declarative policy.

use serde::{Deserialize, Serialize};

use crate::{SandboxMode, ToolTimeout};

/// How a tool is allowed to reach the caller's workspace or filesystem roots.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceAccess {
    /// The tool needs no filesystem or workspace access.
    #[default]
    None,
    /// The tool may only touch explicitly declared trusted roots.
    Scoped,
    /// The tool may touch any path the host permits the process to reach.
    Any,
}

/// Declared side effects a tool may cause.
///
/// These flags are declarations, not policy decisions. A host uses them to
/// select its own approval, sandbox, and audit behaviour.
#[expect(
    clippy::struct_excessive_bools,
    reason = "Each independently auditable side effect is an explicit host-readable declaration."
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSideEffects {
    /// The tool reads state but does not mutate any observable state.
    pub read_only: bool,
    /// The tool creates, modifies, or deletes files.
    pub writes_files: bool,
    /// The tool performs network I/O.
    pub network: bool,
    /// The tool installs packages or otherwise mutates a toolchain.
    pub installs_dependencies: bool,
    /// The tool can perform irreversible or destructive actions.
    pub destructive: bool,
    /// The tool calls an external third-party service.
    pub external_service: bool,
    /// The tool can move money or incur a charge.
    pub payment: bool,
}

/// Human-facing presentation metadata for a tool invocation.
///
/// This is not sent to a model as part of the tool schema. Hosts can use it
/// for compact timeline rows and audit records.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDisplay {
    /// Short verb phrase or title-cased label for the call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Optional static detail for the call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl ToolDisplay {
    /// Returns `true` when neither a label nor detail was declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.label.is_none() && self.detail.is_none()
    }

    /// Creates display metadata with optional label and detail fields.
    #[must_use]
    pub fn new(label: Option<impl Into<String>>, detail: Option<impl Into<String>>) -> Self {
        Self {
            label: label.map(Into::into),
            detail: detail.map(Into::into),
        }
    }

    /// Creates display metadata with only a label.
    #[must_use]
    pub fn label(label: impl Into<String>) -> Self {
        Self {
            label: Some(label.into()),
            detail: None,
        }
    }

    /// Sets the static display detail.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Whether an orphaned in-flight call may be safely re-executed after a
/// crash.
///
/// A host that persists an in-flight call and recovers after a crash has to
/// decide whether to replay it. Pi's `replay` classification is the reference
/// design: most tools are not safe to blindly re-run (a payment, a send), so
/// [`Self::Never`] is the default and a tool must opt into [`Self::Safe`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolReplay {
    /// An orphaned call must not be re-executed after a crash.
    #[default]
    Never,
    /// An orphaned call may be safely re-executed after a crash — the tool is
    /// idempotent or otherwise safe to repeat.
    Safe,
}

/// Runtime requirements a tool declares for safe execution.
///
/// A host decides how to apply these requirements. In particular, this type
/// never creates a deadline, cancellation handle, sandbox, retry loop, or
/// stream by itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRuntime {
    /// Suggested per-call wall-clock timeout in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// Invocation timeout behaviour when a numeric timeout is not enough.
    #[serde(default, skip_serializing_if = "ToolTimeout::is_inherit")]
    pub timeout: ToolTimeout,
    /// Maximum automatic retries a host may permit for this tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    /// Whether repeating a call with identical arguments is safe.
    pub idempotent: bool,
    /// Whether the tool honours cooperative cancellation.
    pub cancelable: bool,
    /// How strictly the tool must be sandboxed.
    pub sandbox: SandboxMode,
    /// Maximum result payload a host should accept, in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_result_bytes: Option<usize>,
    /// Whether the tool can emit streaming result fragments.
    pub streaming: bool,
    /// Whether an orphaned in-flight call for this tool may be safely
    /// re-executed after a crash. See [`ToolReplay`].
    #[serde(default)]
    pub replay: ToolReplay,
}

/// Access requirements a tool declares before a host exposes or runs it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolAccess {
    /// Workspace or filesystem reach the tool needs.
    pub workspace: WorkspaceAccess,
    /// Filesystem roots the tool needs for [`WorkspaceAccess::Scoped`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_roots: Vec<String>,
    /// Named credentials that must be available to run the tool.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<String>,
    /// Whether the host should require explicit human approval per call.
    pub approval_required: bool,
    /// Whether the tool is safe in a background or non-interactive run.
    pub background_safe: bool,
}

/// A complete safety, runtime, access, and display declaration for a tool.
///
/// `classified` is deliberately independent from the fields. A host that
/// wants fail-closed admission can refuse the default unclassified declaration
/// even when its fields happen to look benign.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolPolicy {
    /// Whether the tool author explicitly classified this declaration.
    pub classified: bool,
    /// Declared side effects.
    pub side_effects: ToolSideEffects,
    /// Declared runtime requirements.
    pub runtime: ToolRuntime,
    /// Declared access requirements.
    pub access: ToolAccess,
    /// Human-facing presentation metadata.
    #[serde(default, skip_serializing_if = "ToolDisplay::is_empty")]
    pub display: ToolDisplay,
}

impl ToolPolicy {
    /// Creates a classified, side-effect-free read-only declaration.
    #[must_use]
    pub fn read_only() -> Self {
        Self {
            classified: true,
            side_effects: ToolSideEffects {
                read_only: true,
                ..ToolSideEffects::default()
            },
            runtime: ToolRuntime {
                idempotent: true,
                cancelable: true,
                ..ToolRuntime::default()
            },
            access: ToolAccess {
                background_safe: true,
                ..ToolAccess::default()
            },
            display: ToolDisplay::default(),
        }
    }

    /// Creates a classified declaration with otherwise-default fields.
    #[must_use]
    pub fn classified() -> Self {
        Self {
            classified: true,
            ..Self::default()
        }
    }

    /// Replaces the declared side effects and marks the declaration classified.
    #[must_use]
    pub fn with_side_effects(mut self, side_effects: ToolSideEffects) -> Self {
        self.classified = true;
        self.side_effects = side_effects;
        self
    }

    /// Replaces the declared runtime requirements and marks it classified.
    #[must_use]
    pub fn with_runtime(mut self, runtime: ToolRuntime) -> Self {
        self.classified = true;
        self.runtime = runtime;
        self
    }

    /// Replaces the declared access requirements and marks it classified.
    #[must_use]
    pub fn with_access(mut self, access: ToolAccess) -> Self {
        self.classified = true;
        self.access = access;
        self
    }

    /// Replaces the human-facing display metadata.
    #[must_use]
    pub fn with_display(mut self, display: ToolDisplay) -> Self {
        self.display = display;
        self
    }

    /// Marks the declaration as requiring explicit human approval.
    #[must_use]
    pub fn requiring_approval(mut self) -> Self {
        self.classified = true;
        self.access.approval_required = true;
        self
    }

    /// Returns whether any declared effect goes beyond a read-only operation.
    #[must_use]
    pub fn has_side_effects(&self) -> bool {
        let effects = &self.side_effects;
        effects.writes_files
            || effects.network
            || effects.installs_dependencies
            || effects.destructive
            || effects.external_service
            || effects.payment
    }
}
