//! The code-style dialect: `<tool_call>read_file(path="src/main.rs")</tool_call>`.
//!
//! The catalogue is a list of function signatures and the call is a function
//! call — the two things a code-trained model has written most. Cheaper than
//! JSON on both sides, and unlike P-Format not a syntax the model has to be
//! taught. See [`crate::codecall`] for the grammar.
//!
//! Like P-Format it degrades rather than fails: a tag body that is not a code
//! call falls through to the JSON parser, so a model that mixes forms — or
//! ignores the protocol and emits JSON — is still understood.

use std::sync::Arc;

use super::ToolDialect;
use super::types::{DialectMessage, DialectResponse, ToolCallFormat, ToolOutcome, TranscriptEntry};
use crate::codecall::CodeStyle;
use crate::parse::parse_text;
use crate::render;
use crate::types::ParseOptions;
use crate::{PFormatRegistry, ParsedToolCall};
use tinytools::ToolSpec;

/// Code-style tool calling, in Python or TypeScript spelling, driven by the
/// same registry of parameter layouts P-Format uses.
#[derive(Debug, Clone)]
pub struct CodeDialect {
    style: CodeStyle,
    /// Name → parameter layout, built once from the agent's real tools. The
    /// same safety boundary as [`super::PFormatDialect`]: the parser refuses
    /// to bind arguments for a tool it does not know.
    registry: Arc<PFormatRegistry>,
}

impl CodeDialect {
    /// Build the dialect over a prepared registry.
    #[must_use]
    pub fn new(style: CodeStyle, registry: PFormatRegistry) -> Self {
        Self {
            style,
            registry: Arc::new(registry),
        }
    }

    /// Share an already-`Arc`'d registry rather than cloning the map.
    #[must_use]
    pub fn from_shared(style: CodeStyle, registry: Arc<PFormatRegistry>) -> Self {
        Self { style, registry }
    }

    /// The spelling this dialect renders.
    #[must_use]
    pub fn style(&self) -> CodeStyle {
        self.style
    }

    /// The registry backing this dialect.
    #[must_use]
    pub fn registry(&self) -> &PFormatRegistry {
        self.registry.as_ref()
    }

    /// The protocol block — **protocol only**, no catalogue. The signatures
    /// live in the prompt's tool section, rendered by
    /// [`crate::render::render_code_catalogue`] from the same schemas this
    /// dialect parses against.
    #[must_use]
    pub fn instructions(style: CodeStyle) -> String {
        render::code_instructions(style)
    }
}

impl ToolDialect for CodeDialect {
    fn parse_response(&self, response: &DialectResponse) -> (String, Vec<ParsedToolCall>) {
        let options = ParseOptions::new().with_registry(self.registry.as_ref());
        let (text, calls) = parse_text(response.text_or_empty(), &options).into_parts();
        crate::telemetry::debug!(
            parse_mode = "code_combined",
            style = self.style.as_str(),
            parsed_tool_calls = calls.len(),
            "code dialect parsed response"
        );
        (text, calls)
    }

    fn format_results(&self, results: &[ToolOutcome]) -> Vec<TranscriptEntry> {
        render::format_results(results)
    }

    fn prompt_instructions(&self, _tools: &[ToolSpec]) -> String {
        Self::instructions(self.style)
    }

    fn to_provider_messages(&self, history: &[TranscriptEntry]) -> Vec<DialectMessage> {
        render::to_provider_messages(history)
    }

    fn should_send_tool_specs(&self) -> bool {
        // Text protocol: the model never sees a structured spec, only the
        // catalogue in the system prompt.
        false
    }

    fn tool_call_format(&self) -> ToolCallFormat {
        match self.style {
            CodeStyle::Python => ToolCallFormat::Python,
            CodeStyle::TypeScript => ToolCallFormat::TypeScript,
        }
    }
}
