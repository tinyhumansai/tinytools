//! The run-scoped facts a tool may read, without naming the harness that owns
//! them.

use crate::workspace::WorkspaceDescriptor;

/// The parts of a live agent run a tool is allowed to see.
///
/// # Why this is a trait and not a struct
///
/// A tool sometimes needs to know *where* it is running: an edit-capable worker
/// given an isolated worktree must resolve relative paths against that
/// worktree's root, not against the host's shared action directory. That fact
/// originates in the agent harness's run context.
///
/// Naming the harness's context type here would make this crate depend on the
/// harness — and the harness depends on *this* crate, for the vocabulary every
/// tool is written against. Erasing the context behind a trait keeps that edge
/// pointing one way: the harness implements this for its own context type, and
/// a tool reads the facts it actually uses without either crate having to know
/// the other's shape.
///
/// It is deliberately narrow. The run id, event sink, cancellation token and
/// streaming flag are all absent, because a tool that wanted them would be
/// reaching into the run rather than doing its job. Widening this trait is
/// worth noticing rather than accommodating.
pub trait ToolRunContext: Send + Sync {
    /// The isolated workspace this call may operate in, when the run was
    /// configured with one.
    ///
    /// `None` means no workspace policy is in effect and the tool should fall
    /// back to whatever root its host configured. A tool must not read `None`
    /// as permission to escape a root — the host's own path policy is what
    /// enforces that, and it applies either way.
    fn workspace(&self) -> Option<&WorkspaceDescriptor> {
        None
    }

    /// Caller thread id, when the parent run is threaded.
    fn thread_id(&self) -> Option<&str> {
        None
    }

    /// Maximum output tokens requested for each model turn in the caller's run.
    ///
    /// A tool that itself calls a model — a sub-agent, a summarizer — uses this
    /// to stay inside the caller's budget instead of picking its own.
    fn max_turn_output_tokens(&self) -> Option<u32> {
        None
    }

    /// Root of the isolated workspace, when there is one.
    ///
    /// A convenience over [`Self::workspace`] for the common case: most tools
    /// want the root and nothing else. Not intended to be overridden.
    fn workspace_root(&self) -> Option<&std::path::Path> {
        self.workspace().map(|w| w.root.as_path())
    }

    /// Identifier of the policy that granted the workspace, for logging and
    /// audit.
    fn workspace_policy_id(&self) -> Option<&str> {
        self.workspace().map(|w| w.policy_id.as_str())
    }
}
