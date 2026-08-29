//! Where a tool may run, and which belt it belongs to.

use serde::{Deserialize, Serialize};

/// Controls where a tool is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ToolScope {
    /// Available in the agent loop, the CLI, and over RPC.
    #[default]
    All,
    /// Intended to mark tools available only in the autonomous agent loop.
    ///
    /// Not yet enforced by any known host: no execution path filters on this
    /// variant, so it currently behaves like [`Self::All`]. It is kept because
    /// tools already annotate themselves with it, and losing those annotations
    /// would mean re-deriving them when the filter lands.
    AgentOnly,
    /// Only available via explicit CLI or RPC invocation, never from the
    /// autonomous loop.
    CliRpcOnly,
}

/// Category of a tool — used to scope which tools a given sub-agent may see.
///
/// The distinction is about *where the work happens*: a [`Self::System`] tool
/// is a built-in implementation running in the host process with direct host
/// access, while a [`Self::Workflow`] tool reaches an external service on the
/// user's behalf. A host typically spawns dedicated tool-execution sub-agents
/// per category, because the two want different models and different
/// approval policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    /// Built-in tools with direct host access.
    #[default]
    System,
    /// Integration-facing tools that reach external services.
    ///
    /// The wire format is pinned to `"skill"` rather than the variant name:
    /// agent definition files on disk already carry that string, and renaming
    /// it would stop those files parsing. The Rust ident was swept to
    /// `Workflow` during a naming change the wire format deliberately did not
    /// follow.
    #[serde(rename = "skill")]
    Workflow,
}

impl std::fmt::Display for ToolCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Matches the serde representation, including the pinned `skill`.
        let name = match self {
            Self::System => "system",
            Self::Workflow => "skill",
        };
        f.write_str(name)
    }
}
