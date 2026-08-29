//! The privilege a tool call requires.

use serde::{Deserialize, Serialize};

/// Permission level required to execute a tool.
///
/// A caller (a chat channel, a scheduled job, a sub-agent) can declare a
/// maximum level; a tool whose required level exceeds it is rejected before any
/// argument is parsed.
///
/// The ordering is load-bearing: enforcement compares levels with `<`, so the
/// discriminants must stay monotonically increasing in privilege. Adding a
/// variant means deciding where in that order it sits, not appending to the
/// end.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default, Hash,
)]
pub enum PermissionLevel {
    /// No permission needed — metadata-only operations.
    None = 0,
    /// Read-only operations: file reads, memory recall, listing.
    #[default]
    ReadOnly = 1,
    /// Write operations: file writes, memory stores.
    Write = 2,
    /// Command execution: shells, scripts.
    Execute = 3,
    /// Destructive or system-level operations.
    Dangerous = 4,
}

impl std::fmt::Display for PermissionLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::None => "None",
            Self::ReadOnly => "ReadOnly",
            Self::Write => "Write",
            Self::Execute => "Execute",
            Self::Dangerous => "Dangerous",
        };
        f.write_str(name)
    }
}
