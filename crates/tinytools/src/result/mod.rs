//! What a tool hands back.

mod types;

pub use types::{FileData, ImageData, ToolContent, ToolControl, ToolErrorKind, ToolResult};

#[cfg(test)]
mod test;
