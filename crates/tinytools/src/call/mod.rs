//! Inputs a caller supplies alongside a tool's declared arguments.

mod types;

pub use types::{ToolCallOptions, ToolTimeout};

#[cfg(test)]
mod test;
