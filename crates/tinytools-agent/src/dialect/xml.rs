//! The JSON-in-tag dialect: `<tool_call>{"name":…,"arguments":{…}}</tool_call>`.
//!
//! The baseline for any model that was never trained to call tools but can
//! follow an instruction. It costs the most tokens of the three — every call
//! spells out its argument names, and the catalogue carries full schemas — and
//! it is the one that works everywhere, which is why it stays the fallback
//! rather than being retired.
//!
//! Parsing is not limited to the advertised form. A model told to write
//! `<tool_call>` may answer in whatever its template prefers, so the response
//! goes through every grammar in [`crate::parse`].

use super::ToolDialect;
use super::types::{DialectMessage, DialectResponse, ToolCallFormat, ToolOutcome, TranscriptEntry};
use crate::parse::parse_text;
use crate::render;
use crate::types::{ParseOptions, ParsedToolCall};
use tinytools::ToolSpec;

/// JSON-in-tag tool calling.
#[derive(Debug, Default, Clone, Copy)]
pub struct XmlDialect;

impl XmlDialect {
    /// Recover tool calls from raw model text with default options.
    ///
    /// Shared with the other two dialects: p-format falls back to it per tag,
    /// and the native dialect uses it to recover calls a model narrated as text
    /// despite having a structured channel available.
    #[must_use]
    pub fn parse_text(text: &str) -> (String, Vec<ParsedToolCall>) {
        parse_text(text, &ParseOptions::new()).into_parts()
    }

    /// The protocol block plus the full-schema catalogue.
    #[must_use]
    pub fn instructions(tools: &[ToolSpec]) -> String {
        render::json_instructions(tools)
    }
}

impl ToolDialect for XmlDialect {
    fn parse_response(&self, response: &DialectResponse) -> (String, Vec<ParsedToolCall>) {
        let (text, calls) = Self::parse_text(response.text_or_empty());
        crate::telemetry::debug!(
            parse_mode = "text_fallback",
            parsed_tool_calls = calls.len(),
            "xml dialect parsed response"
        );
        (text, calls)
    }

    fn format_results(&self, results: &[ToolOutcome]) -> Vec<TranscriptEntry> {
        render::format_results(results)
    }

    fn prompt_instructions(&self, tools: &[ToolSpec]) -> String {
        Self::instructions(tools)
    }

    fn to_provider_messages(&self, history: &[TranscriptEntry]) -> Vec<DialectMessage> {
        render::to_provider_messages(history)
    }

    fn should_send_tool_specs(&self) -> bool {
        // The schemas are already in the prompt; sending them again as native
        // specs would double the cost and invite the model to use a channel
        // this dialect cannot read.
        false
    }

    fn embeds_tool_catalogue(&self) -> bool {
        true
    }

    fn tool_call_format(&self) -> ToolCallFormat {
        ToolCallFormat::Json
    }
}
