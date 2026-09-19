//! Turning model text back into tool calls.
//!
//! A model that supports native tool use hands back structured calls and none
//! of this is needed. Everything else — prompt-guided models, local models,
//! providers whose native mode is unavailable, and native models that narrate
//! a call as text anyway — emits tool calls as *text*, in whatever shape the
//! model was trained to produce. This module turns that text back into calls.
//!
//! # How a response is read
//!
//! 1. If the **whole** response is one JSON value, it is read as a call
//!    envelope ([`grammar::bare_json`]) and nothing else runs.
//! 2. Otherwise the text is scanned left to right. At each step every
//!    [`grammar::Grammar`] reports its next block; the earliest one wins, its
//!    calls are collected, and the scan resumes past it. Text between blocks
//!    is the narrative. Blocks inside a [`protected`] code fence are skipped.
//! 3. If the scan found nothing, the GLM line grammar is tried on the
//!    narrative.
//! 4. Every call's name is resolved against the offered tools
//!    ([`crate::repair::name`]) when the caller supplied them.
//!
//! # Why it is this forgiving, and where it stops
//!
//! Each accommodation exists because a model actually produced it and the
//! alternative was dropping a well-formed call and burning an iteration. The
//! permissiveness is bounded on purpose:
//!
//! * **A call needs a marker.** Only the bare-JSON path runs without one, and
//!   it requires the entire response to be the value.
//! * **Argument keys are aliased; tool names are not.** `args` / `parameters`
//!   / `input` are honoured behind a marker. On a bare object they need the
//!   canonical `arguments` key or a name the caller offered, so a plain JSON
//!   answer is never a phantom call.
//! * **Names are repaired only to a unique offered tool.** Nothing is invented.
//! * **Code fences with a language are examples**, not calls.
//! * **P-Format refuses to invent argument names** for a tool it does not know.
//!
//! # What the host still owns
//!
//! Ids: this module never mints one. Execution: permission, sandboxing,
//! timeouts, and the unknown-tool policy are host decisions; a call whose
//! name did not resolve is still returned, flagged in the diagnostics.

pub(crate) mod call_object;
pub(crate) mod grammar;
pub(crate) mod json_values;
pub mod protected;

use std::ops::Range;

use crate::repair;
use crate::types::{CallSource, ParseDiagnostic, ParseOptions, ParseOutcome, ParsedToolCall};
use grammar::{Decoded, GRAMMARS, Probe, ScanMode};

pub use call_object::{parse_tool_call_value, parse_tool_calls_from_json_value};
pub use grammar::glm::{build_curl_command, map_glm_tool_alias, parse_glm_style_tool_calls};
pub use json_values::extract_json_values;

/// Parses a complete model response.
#[must_use]
pub fn parse_text(text: &str, options: &ParseOptions<'_>) -> ParseOutcome {
    if options.allow_bare_json
        && let Some((content, calls)) = grammar::bare_json::parse(text, options)
    {
        return finalize(content, calls, Vec::new(), options);
    }

    let scan = scan(text, options, ScanMode::Batch);
    let parts: Vec<&str> = scan
        .kept
        .iter()
        .map(|range| text[range.clone()].trim())
        .filter(|part| !part.is_empty())
        .collect();
    let mut calls = scan.calls;
    let diagnostics = scan.diagnostics;

    if calls.is_empty() {
        let joined = parts.join("\n");
        let (cleaned, glm_calls) = grammar::glm::parse_and_strip(&joined);
        if !glm_calls.is_empty() {
            calls = glm_calls;
            return finalize(cleaned.trim().to_string(), calls, diagnostics, options);
        }
    }
    finalize(parts.join("\n"), calls, diagnostics, options)
}

/// Name resolution and the diagnostics it produces.
fn finalize(
    text: String,
    calls: Vec<ParsedToolCall>,
    diagnostics: Vec<ParseDiagnostic>,
    options: &ParseOptions<'_>,
) -> ParseOutcome {
    let (calls, diagnostics) = resolve_names(calls, diagnostics, options);
    ParseOutcome {
        text,
        calls,
        diagnostics,
    }
}

/// Resolves every text-recovered call's name against the offered tools.
pub(crate) fn resolve_names(
    mut calls: Vec<ParsedToolCall>,
    mut diagnostics: Vec<ParseDiagnostic>,
    options: &ParseOptions<'_>,
) -> (Vec<ParsedToolCall>, Vec<ParseDiagnostic>) {
    for call in &mut calls {
        if call.source == CallSource::Native {
            continue;
        }
        let resolution = repair::name::resolve(&call.name, options.known_tools);
        if resolution.repaired {
            crate::telemetry::debug!(
                from_chars = call.name.chars().count(),
                to = resolution.name.as_str(),
                "[agent_parse] repaired tool name"
            );
            diagnostics.push(ParseDiagnostic::NameRepaired {
                from: std::mem::take(&mut call.name),
                to: resolution.name.clone(),
            });
            call.name = resolution.name;
        } else if options.has_known_tools() && !resolution.known {
            diagnostics.push(ParseDiagnostic::UnknownTool {
                name: call.name.clone(),
            });
        }
    }
    (calls, diagnostics)
}

/// The result of one scan pass.
#[derive(Debug, Default)]
pub(crate) struct Scan {
    /// Byte ranges of the text kept as narrative, in order.
    pub(crate) kept: Vec<Range<usize>>,
    /// Calls in source order.
    pub(crate) calls: Vec<ParsedToolCall>,
    /// What was dropped or left unterminated.
    pub(crate) diagnostics: Vec<ParseDiagnostic>,
    /// In [`ScanMode::Stream`], the offset of an opener whose block has not
    /// closed yet. Text from there on must be held back.
    pub(crate) pending: Option<usize>,
}

/// One left-to-right pass over `text`.
pub(crate) fn scan(text: &str, options: &ParseOptions<'_>, mode: ScanMode) -> Scan {
    let ranges = protected::fence_ranges(text);
    let mut out = Scan::default();
    let mut from = 0usize;

    loop {
        let mut best: Option<Probe> = None;
        let mut best_source = CallSource::TaggedJson;
        let mut best_start = usize::MAX;
        for grammar in GRAMMARS {
            let mut cursor = from;
            let candidate = loop {
                match grammar.probe(text, cursor, options, mode) {
                    Probe::None => break None,
                    Probe::Found(block) => {
                        if let Some(end) = protected::protected_end(&ranges, block.start) {
                            if end <= cursor {
                                break None;
                            }
                            cursor = end;
                            continue;
                        }
                        break Some((block.start, Probe::Found(block)));
                    }
                    Probe::Pending { start } => {
                        if let Some(end) = protected::protected_end(&ranges, start) {
                            if end <= cursor {
                                break None;
                            }
                            cursor = end;
                            continue;
                        }
                        break Some((start, Probe::Pending { start }));
                    }
                }
            };
            if let Some((start, probe)) = candidate
                && start < best_start
            {
                best_start = start;
                best_source = grammar.source();
                best = Some(probe);
            }
        }

        match best {
            None => {
                out.kept.push(from..text.len());
                break;
            }
            Some(Probe::Pending { start }) => {
                out.kept.push(from..start);
                out.pending = Some(start);
                break;
            }
            Some(Probe::Found(block)) => {
                let source = best_source;
                match block.decoded {
                    Decoded::Calls(calls) => {
                        out.kept.push(from..block.start);
                        out.calls.extend(calls);
                    }
                    Decoded::Malformed { body_chars } => {
                        out.kept.push(from..block.start);
                        crate::telemetry::warn!(
                            body_chars,
                            "[agent_parse] malformed tool-call block: body did not decode to a call"
                        );
                        out.diagnostics.push(ParseDiagnostic::MalformedBlock {
                            source,
                            body_chars,
                        });
                    }
                    Decoded::Noise => {
                        out.kept.push(from..block.start);
                    }
                    Decoded::Verbatim => {
                        out.kept.push(from..block.end);
                        crate::telemetry::warn!(
                            "[agent_parse] unterminated tool-call block kept as text"
                        );
                        out.diagnostics
                            .push(ParseDiagnostic::UnterminatedBlock { source });
                    }
                }
                from = block.end.max(block.start + 1).min(text.len());
            }
            Some(Probe::None) => break,
        }
    }
    out
}

/// Parses a response with default options.
///
/// Kept for callers that predate [`parse_text`]; equivalent to
/// `parse_text(response, &ParseOptions::new()).into_parts()`.
#[must_use]
pub fn parse_tool_calls(response: &str) -> (String, Vec<ParsedToolCall>) {
    parse_text(response, &ParseOptions::new()).into_parts()
}

/// Parses a response with a P-Format registry, preferring a positional body
/// inside each tag and falling back to JSON per tag.
#[must_use]
pub fn parse_tool_calls_with_pformat(
    response: &str,
    registry: &crate::PFormatRegistry,
) -> (String, Vec<ParsedToolCall>) {
    let options = ParseOptions::new().with_registry(registry);
    parse_text(response, &options).into_parts()
}

/// Normalizes an argument value, decoding stringified JSON when possible.
#[must_use]
pub fn parse_arguments_value(raw: Option<&serde_json::Value>) -> serde_json::Value {
    repair::args::decode(raw)
}

#[cfg(test)]
mod test;
