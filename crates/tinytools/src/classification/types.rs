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

/// Where a tool is exposed to the model.
///
/// Every tool a host registers is dispatchable. This says which of them the
/// model is *told about* up front, and it is a property of the tool rather than
/// of a config posture, because the answer rarely varies by deployment: a tool
/// the model needs on most turns is direct, and one it needs on a handful of
/// turns a week is not, whoever is running the host.
///
/// The distinction exists because tool schemas are a fixed per-turn cost paid
/// on every request, and on a large tool surface they dominate it — measured on
/// OpenHuman's orchestrator, 45 KB of schema against 34 KB of system prompt.
/// A schema the model reads on one turn in five hundred is not worth its place
/// on the other four hundred and ninety-nine.
///
/// Modelled on Codex's `ToolExposure` (`codex-rs/tools/src/tool_executor.rs`),
/// which pairs `Deferred` with a BM25-indexed `tool_search`. This enum is
/// deliberately the smaller half of that design: Codex additionally
/// distinguishes its Code Mode surface, which has no equivalent here yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolExposure {
    /// Advertise the tool's schema on every request.
    ///
    /// The default, and deliberately so: a tool that has thought about its own
    /// exposure will say so, and one that has not should keep behaving exactly
    /// as it did before this existed.
    #[default]
    Direct,
    /// Register the tool and keep its schema off the wire, reachable through
    /// the host's tool-search facility.
    ///
    /// A host that offers no such facility must treat this as [`Self::Direct`]
    /// rather than hiding the tool — a capability the model cannot see *and*
    /// cannot look up is simply gone, which is a bigger regression than the
    /// tokens it saves.
    Deferred,
    /// Keep the tool dispatchable but never show it to the model.
    ///
    /// For tools a host calls on the model's behalf, or that exist only to be
    /// invoked by another tool.
    Hidden,
}

impl ToolExposure {
    /// Whether this tool's schema belongs in the initial tool list.
    pub fn is_direct(self) -> bool {
        matches!(self, Self::Direct)
    }

    /// Whether tool search may surface this tool.
    pub fn is_searchable(self) -> bool {
        matches!(self, Self::Deferred)
    }
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
