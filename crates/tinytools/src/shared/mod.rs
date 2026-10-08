//! Sharing one built tool across many owned belts.
//!
//! A host commonly builds an agent's tools **once** and keeps them as
//! `Arc<dyn Tool>`, because the same instance is wanted in several places — a
//! per-agent pool, an MCP server re-exporting the belt, a catalogue index. A
//! harness, meanwhile, often asks for an owned `Vec<Box<dyn Tool>>` and asks
//! for it **per turn**, because the session it hands the belt to is rebuilt
//! between turns and a `Box<dyn Tool>` cannot outlive it.
//!
//! [`SharedTool`] bridges the two: a thin `Box` around the `Arc`, minted per
//! turn, delegating every call to the one shared instance. No tool is rebuilt,
//! no state is duplicated, and a tool holding a connection or a cache keeps
//! holding exactly one. [`share_belt`] and [`owned_belt`] convert a whole belt
//! in each direction.

mod types;

use std::sync::Arc;

use crate::tool::Tool;

pub use types::SharedTool;

/// Moves a built belt into shared handles, so a host can keep one copy and
/// hand others out without rebuilding any tool.
#[must_use]
pub fn share_belt(belt: Vec<Box<dyn Tool>>) -> Vec<Arc<dyn Tool>> {
    belt.into_iter().map(Arc::from).collect()
}

/// The shared belt as an owned one, for a single turn.
///
/// Call it wherever a harness wants owned tools — typically once per turn from
/// an agent's tool factory. Each entry is a [`SharedTool`] pointing at the
/// same instance; the tools themselves are not rebuilt.
#[must_use]
pub fn owned_belt(shared: &[Arc<dyn Tool>]) -> Vec<Box<dyn Tool>> {
    shared
        .iter()
        .map(|tool| Box::new(SharedTool::new(Arc::clone(tool))) as Box<dyn Tool>)
        .collect()
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
