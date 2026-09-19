//! The slot-indexed dialect: `<tool_call>read_file[0|src/main.rs]</tool_call>`.
//!
//! Roughly an 80% token saving over the JSON form on the call side, and more
//! than that on the catalogue side, since a signature replaces a schema. See
//! [`crate::pformat`] for the grammar itself.
//!
//! The interesting property is that it degrades rather than fails: a body that
//! is not a well-formed p-format call falls through to the JSON parser per tag,
//! so a model that mixes the two forms in one response — or ignores the protocol
//! entirely and emits JSON — is still understood. That fallback is also what
//! makes the parser's strictness affordable: a call with a miscounted or
//! non-numeric index is refused here and retried as JSON, rather than being
//! bound to whichever parameters it happens to line up with.

use std::sync::Arc;

use super::ToolDialect;
use super::types::{DialectMessage, DialectResponse, ToolCallFormat, ToolOutcome, TranscriptEntry};
use crate::parse::parse_text;
use crate::render;
use crate::types::ParseOptions;
use crate::{PFormatRegistry, ParsedToolCall};
use tinytools::ToolSpec;

/// Positional tool calling, driven by a registry of parameter layouts.
#[derive(Debug, Clone)]
pub struct PFormatDialect {
    /// Name → parameter layout, built once from the agent's real tools.
    ///
    /// This is the safety boundary the grammar depends on, not just a lookup:
    /// the parser refuses to invent argument names for a tool it does not know,
    /// so a model cannot tunnel arbitrary JSON through by guessing a name. A
    /// registry built from anything but the agent's own tools would widen that.
    registry: Arc<PFormatRegistry>,
}

impl PFormatDialect {
    /// Build the dialect over a prepared registry.
    #[must_use]
    pub fn new(registry: PFormatRegistry) -> Self {
        Self {
            registry: Arc::new(registry),
        }
    }

    /// Share an already-`Arc`'d registry rather than cloning the map.
    #[must_use]
    pub fn from_shared(registry: Arc<PFormatRegistry>) -> Self {
        Self { registry }
    }

    /// The registry backing this dialect.
    #[must_use]
    pub fn registry(&self) -> &PFormatRegistry {
        self.registry.as_ref()
    }

    /// The protocol block — **protocol only**, no catalogue.
    ///
    /// The signatures live in the prompt's tool section, rendered by
    /// [`crate::render::render_pformat_catalogue`] from the same schemas
    /// this dialect parses against. Repeating them here is the "tools listed
    /// twice" pattern the JSON dialect is stuck with, and it means adding a
    /// tool changes the prompt in one place instead of two.
    #[must_use]
    pub fn instructions() -> String {
        render::pformat_instructions()
    }
}

impl ToolDialect for PFormatDialect {
    fn parse_response(&self, response: &DialectResponse) -> (String, Vec<ParsedToolCall>) {
        let options = ParseOptions::new().with_registry(self.registry.as_ref());
        let (text, calls) = parse_text(response.text_or_empty(), &options).into_parts();
        crate::telemetry::debug!(
            parse_mode = "pformat_combined",
            parsed_tool_calls = calls.len(),
            "pformat dialect parsed response"
        );
        (text, calls)
    }

    fn format_results(&self, results: &[ToolOutcome]) -> Vec<TranscriptEntry> {
        render::format_results(results)
    }

    fn prompt_instructions(&self, _tools: &[ToolSpec]) -> String {
        Self::instructions()
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
        ToolCallFormat::PFormat
    }
}
