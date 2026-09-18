//! Inputs a caller supplies alongside a tool's declared arguments.

mod injected;
mod types;

pub use injected::{
    InjectedToolArguments, ToolArgumentPreparationError, ToolCall, ToolCallId,
    ToolInjectedArgument, ToolInjectedArgumentSource, prepare_tool_arguments,
    project_injected_arguments,
};
pub use types::{ToolCallOptions, ToolTimeout};

#[cfg(test)]
mod test;
