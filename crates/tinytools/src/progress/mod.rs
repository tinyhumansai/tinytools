//! What a long-running tool reports while it is still running.

mod types;

pub use types::{ProgressSink, ToolProgress};

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
