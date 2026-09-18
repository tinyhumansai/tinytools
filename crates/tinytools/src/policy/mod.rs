//! Declarative safety, runtime, access, and display metadata for a tool.
//!
//! A [`ToolPolicy`] describes a capability. Hosts read that declaration and
//! decide whether a call is allowed; this module deliberately performs no
//! approval, sandbox, timeout, or credential enforcement.

mod types;

pub use types::{
    ToolAccess, ToolDisplay, ToolPolicy, ToolRuntime, ToolSideEffects, WorkspaceAccess,
};

#[cfg(test)]
mod test;
