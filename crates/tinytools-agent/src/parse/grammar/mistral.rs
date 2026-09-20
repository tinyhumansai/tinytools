//! Mistral `[TOOL_CALLS]` blocks rendered as text.
//!
//! Mistral's templates mark a call with a literal `[TOOL_CALLS]` token
//! followed by either a JSON array of `{"name":…,"arguments":…}` objects
//! (v3 templates) or `NAME[ARGS]{…}` (v11 and later). Served through a route
//! that does not decode the token, both arrive in the content verbatim.

use super::{Block, Decoded, Grammar, Probe, ScanMode};
use crate::parse::call_object::{AliasPolicy, read_calls};
use crate::parse::json_values::extract_first_json_value_with_end;
use crate::repair::json::recover_object;
use crate::types::{CallSource, ParseOptions, ParsedToolCall};

/// The Mistral grammar.
#[derive(Debug)]
pub(crate) struct Mistral;

const MARKER: &str = "[TOOL_CALLS]";
const ARGS: &str = "[ARGS]";

impl Grammar for Mistral {
    fn source(&self) -> CallSource {
        CallSource::Mistral
    }

    fn probe(&self, text: &str, from: usize, options: &ParseOptions<'_>, mode: ScanMode) -> Probe {
        let Some(rel) = text[from..].find(MARKER) else {
            return Probe::None;
        };
        let start = from + rel;
        let body_start = start + MARKER.len();
        let after = &text[body_start..];
        let is_known = |name: &str| options.knows(name);

        // v3: a JSON array (or single object) right after the marker.
        if let Some((value, consumed)) = extract_first_json_value_with_end(after)
            && after[..consumed].trim_start().starts_with(['[', '{'])
            && !after.trim_start().starts_with(ARGS)
        {
            let calls = read_calls(&value, AliasPolicy::Marked, &is_known, CallSource::Mistral);
            if !calls.is_empty() {
                return Probe::Found(Block {
                    start,
                    end: body_start + consumed,
                    decoded: Decoded::Calls(calls),
                });
            }
        }

        // v11+: `NAME[ARGS]{…}`, possibly several in a row.
        let mut calls = Vec::new();
        let mut cursor = 0usize;
        // Set when the loop stopped for a reason a stream fragment boundary
        // can explain — a name (and possibly a partial `[ARGS]`) not yet
        // finished, or a complete `NAME[ARGS]` whose JSON body has not fully
        // arrived — as opposed to text that is definitively not a
        // continuation (an invalid name).
        let mut ambiguous_tail = false;
        loop {
            let rest = &after[cursor..];
            let Some(args_rel) = rest.find(ARGS) else {
                ambiguous_tail = could_be_v11_continuation(rest);
                break;
            };
            let name = rest[..args_rel].trim();
            if name.is_empty()
                || !name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
            {
                break;
            }
            let payload = &rest[args_rel + ARGS.len()..];
            let Some((value, consumed)) = extract_first_json_value_with_end(payload) else {
                // A valid name and a complete `[ARGS]` marker, but the JSON
                // body has not arrived complete yet — streaming cannot tell
                // that apart from a fragment boundary landing mid-object.
                ambiguous_tail = true;
                break;
            };
            let arguments = if value.is_object() {
                value
            } else {
                recover_object(&payload[..consumed]).unwrap_or_else(|| serde_json::json!({}))
            };
            calls.push(ParsedToolCall::new(name, arguments, CallSource::Mistral));
            cursor += args_rel + ARGS.len() + consumed;
        }
        if !calls.is_empty() {
            // The v11 form allows a second call to follow directly with no
            // fresh `[TOOL_CALLS]` marker (`NAME[ARGS]{…}NAME2[ARGS]{…}`).
            // If the buffered text ends right where the trailing bytes are
            // still ambiguous, a stream fragment has not necessarily
            // finished the block — finalizing now would drop the
            // continuation call the moment it arrives split across a
            // fragment boundary. Hold the whole block until either more
            // text disambiguates it or the stream ends.
            if mode == ScanMode::Stream && ambiguous_tail {
                return Probe::Pending { start };
            }
            return Probe::Found(Block {
                start,
                end: body_start + cursor,
                decoded: Decoded::Calls(calls),
            });
        }

        if mode == ScanMode::Stream {
            return Probe::Pending { start };
        }
        Probe::Found(Block {
            start,
            end: body_start,
            decoded: Decoded::Malformed {
                body_chars: after.chars().count(),
            },
        })
    }

    fn openers(&self) -> &'static [&'static str] {
        &["[TOOL_CALLS]"]
    }
}

/// Whether `rest` (the text left over after the last complete v11 call, or
/// the whole body when no call has been read yet) is still consistent with
/// growing into another `NAME[ARGS]` pair: a run of name characters,
/// optionally followed by a proper prefix of the `[ARGS]` marker.
/// `NAME[ARGS]` itself never reaches this check — the caller only calls it
/// once `rest.find(ARGS)` has already failed.
fn could_be_v11_continuation(rest: &str) -> bool {
    let trimmed = rest.trim_start();
    let name_len = trimmed
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
        .unwrap_or(trimmed.len());
    ARGS.starts_with(&trimmed[name_len..])
}
