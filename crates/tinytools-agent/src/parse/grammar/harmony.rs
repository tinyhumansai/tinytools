//! `OpenAI` Harmony (gpt-oss) tool calls rendered as text.
//!
//! A gpt-oss model served without Harmony decoding writes its channel tokens
//! straight into the content:
//!
//! ```text
//! <|channel|>commentary to=functions.get_weather <|constrain|>json<|message|>{"city":"Paris"}<|call|>
//! ```
//!
//! The call is the `to=` target of a `commentary` (or `analysis`) channel
//! message; the arguments are the `<|message|>` payload up to `<|call|>`
//! (or `<|end|>`). Any leading `<|start|>assistant` is furniture. A channel
//! message with no `to=` is ordinary reasoning or text, not a call, and is
//! left alone.

use super::{Block, Decoded, Grammar, Probe, ScanMode, find_ci};
use crate::repair::json::recover_object;
use crate::types::{CallSource, ParseOptions, ParsedToolCall};

/// The Harmony grammar.
#[derive(Debug)]
pub(crate) struct Harmony;

const CHANNEL: &str = "<|channel|>";
const MESSAGE: &str = "<|message|>";
const TERMINATORS: &[&str] = &["<|call|>", "<|end|>", "<|return|>"];
/// The Harmony template's per-turn preamble, always immediately before the
/// first channel. Furniture, not narrative — see the module doc.
const START_PREFIX: &str = "<|start|>assistant";

impl Grammar for Harmony {
    fn source(&self) -> CallSource {
        CallSource::Harmony
    }

    fn probe(&self, text: &str, from: usize, _options: &ParseOptions<'_>, mode: ScanMode) -> Probe {
        let mut cursor = from;
        while let Some(idx) = find_ci(text, CHANNEL, cursor) {
            let start = absorb_start_prefix(text, idx);
            let header_start = idx + CHANNEL.len();
            let Some(message_rel) = find_ci(text, MESSAGE, header_start) else {
                if mode == ScanMode::Stream {
                    return Probe::Pending { start };
                }
                return Probe::None;
            };
            let header = &text[header_start..message_rel];
            let Some(name) = target_name(header) else {
                // A channel message that is not a call: skip past it.
                cursor = message_rel + MESSAGE.len();
                continue;
            };
            let payload_start = message_rel + MESSAGE.len();
            let after = &text[payload_start..];
            let terminator = TERMINATORS
                .iter()
                .filter_map(|t| after.find(t).map(|i| (i, i + t.len())))
                .min_by_key(|(i, _)| *i);
            let Some((payload_end, term_end)) = terminator else {
                if mode == ScanMode::Stream {
                    return Probe::Pending { start };
                }
                // Batch: the payload runs to the end of the text.
                return found(start, text.len(), &name, after);
            };
            return found(start, payload_start + term_end, &name, &after[..payload_end]);
        }
        Probe::None
    }

    fn openers(&self) -> &'static [&'static str] {
        &["<|channel|>", "<|start|>"]
    }
}

fn found(start: usize, end: usize, name: &str, payload: &str) -> Probe {
    let arguments = recover_object(payload).unwrap_or_else(|| serde_json::json!({}));
    Probe::Found(Block {
        start,
        end,
        decoded: Decoded::Calls(vec![ParsedToolCall::new(
            name,
            arguments,
            CallSource::Harmony,
        )]),
    })
}

/// The `to=` target of a channel header, with the `functions.` namespace
/// removed. `None` when the header carries no target.
fn target_name(header: &str) -> Option<String> {
    let idx = header.find("to=")?;
    let rest = &header[idx + 3..];
    let raw: String = rest
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '<')
        .collect();
    let name = raw
        .strip_prefix("functions.")
        .or_else(|| raw.strip_prefix("tools."))
        .unwrap_or(&raw)
        .trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}
