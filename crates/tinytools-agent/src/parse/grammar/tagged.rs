//! `<tool_call>…</tool_call>` and everything models do to it.
//!
//! The baseline text protocol — JSON (or P-Format) inside a tag — arrives in
//! more spellings than any other grammar because every chat template has its
//! own, and gateways garble the tag markers themselves:
//!
//! * spelling variants `<toolcall>`, `<tool-call>`, and the bare `<invoke>`;
//! * an attribute form `<tool_call id="call_0">` (Hermes / DeepSeek
//!   templates);
//! * sentinel pipes leaked into the markers, in any position:
//!   `<|tool_call>…<tool_call|>`, `<|tool_call|>…<|tool_call|>`,
//!   `…</tool_call|>`;
//! * a `call:` prefix before the body;
//! * a fenced block instead of a tag, ```` ```tool_call … ``` ````, sometimes
//!   closed by a stray `</tool_call>`;
//! * a body that is a Kimi `NAME{…}` object with unquoted keys and `<|"|>`
//!   quote sentinels;
//! * a body wrapped in its own ```` ```json ```` fence.
//!
//! Tags are paired **positionally**: the first tag-family marker after the
//! cursor opens a block, the next one closes it. That is what makes the
//! garbled forms parse without a per-variant open/close table, and it is
//! safe because these tags never nest.

use std::sync::LazyLock;

use regex::Regex;

use super::{Block, Decoded, Grammar, Probe, ScanMode, find_ci, pending_opener, prefer_pending};
use crate::parse::call_object::{AliasPolicy, read_calls};
use crate::parse::json_values::{
    extract_first_json_value_with_end, extract_json_values, find_json_end, strip_leading_close_tags,
};
use crate::repair::json::{recover_object, strip_code_fence};
use crate::types::{CallSource, ParseOptions, ParsedToolCall};

/// The tagged-JSON grammar.
#[derive(Debug)]
pub(crate) struct Tagged;

/// Any tag-family marker: `<tool_call>`, `<toolcall>`, `<tool-call>`, with
/// pipes, a slash, or whitespace leaked in, and an optional attribute list.
/// `<tool_calls>` (plural, a JSON key) and `<tool_callable>` do not match:
/// the name must end at a pipe, slash, whitespace, or `>`.
static TAG_RE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)<[|/\s]*tool[_-]?call(?:[|/\s]*|\s+[^>]*)>").ok());

/// Openers a fenced block can carry.
const FENCE_OPENERS: &[&str] = &["```tool_call", "```toolcall", "```tool-call", "```invoke"];

/// Kimi-family argument-quote sentinel that leaks in place of `"`.
const ARG_QUOTE_SENTINEL: &str = "<|\"|>";

/// A located opener.
struct Opener {
    start: usize,
    body_start: usize,
    kind: OpenerKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OpenerKind {
    /// A tag-family marker; closed by the next tag-family marker.
    Tag,
    /// The bare `<invoke>` literal; closed by `</invoke>`.
    Invoke,
    /// A fenced block; closed by ```` ``` ```` or a stray closing tag.
    Fence,
}

impl Grammar for Tagged {
    fn source(&self) -> CallSource {
        CallSource::TaggedJson
    }

    fn probe(&self, text: &str, from: usize, options: &ParseOptions<'_>, mode: ScanMode) -> Probe {
        let pending = pending_opener(
            text,
            from,
            &["<tool_call", "<toolcall", "<tool-call", "<|tool_call"],
            ">",
            mode,
        );
        prefer_pending(self.probe_decided(text, from, options, mode), pending)
    }

    fn openers(&self) -> &'static [&'static str] {
        &[
            "<tool_call",
            "<toolcall",
            "<tool-call",
            "<|tool_call",
            "<invoke>",
            "```tool_call",
            "```toolcall",
            "```tool-call",
            "```invoke",
        ]
    }
}

impl Tagged {
    /// The next block whose opener is fully present.
    fn probe_decided(
        &self,
        text: &str,
        from: usize,
        options: &ParseOptions<'_>,
        mode: ScanMode,
    ) -> Probe {
        let Some(opener) = next_opener(text, from) else {
            return Probe::None;
        };
        let after = &text[opener.body_start..];

        let close = match opener.kind {
            OpenerKind::Tag => TAG_RE
                .as_ref()
                .and_then(|re| re.find(after))
                .map(|m| (m.start(), m.end())),
            OpenerKind::Invoke => after.find("</invoke>").map(|i| (i, i + "</invoke>".len())),
            OpenerKind::Fence => fence_close(after),
        };

        if let Some((body_end, close_end)) = close {
            let body = &after[..body_end];
            let end = opener.body_start + close_end;
            let calls = decode_body(body, options);
            let decoded = if calls.is_empty() {
                Decoded::Malformed {
                    body_chars: body.chars().count(),
                }
            } else {
                Decoded::Calls(calls)
            };
            return Probe::Found(Block {
                start: opener.start,
                end,
                decoded,
            });
        }

        if mode == ScanMode::Stream {
            return Probe::Pending {
                start: opener.start,
            };
        }

        // Batch: no closer. Recover a balanced JSON body if one starts here.
        let recovered = find_json_end(after)
            .and_then(|json_end| {
                serde_json::from_str::<serde_json::Value>(&after[..json_end])
                    .ok()
                    .map(|value| (value, json_end))
            })
            .or_else(|| extract_first_json_value_with_end(after));
        if let Some((value, consumed)) = recovered {
            let calls = read_calls(
                &value,
                AliasPolicy::Marked,
                &|name| options.knows(name),
                CallSource::TaggedJson,
            );
            if !calls.is_empty() {
                let rest = &after[consumed..];
                let stripped = strip_leading_close_tags(rest);
                let end = text.len() - stripped.len();
                return Probe::Found(Block {
                    start: opener.start,
                    end,
                    decoded: Decoded::Calls(calls),
                });
            }
        }
        Probe::Found(Block {
            start: opener.start,
            end: text.len(),
            decoded: Decoded::Verbatim,
        })
    }
}

/// The earliest opener at or after `from`: a non-closing tag-family marker,
/// the bare `<invoke>` literal, or a fence opener.
fn next_opener(text: &str, from: usize) -> Option<Opener> {
    let mut best: Option<Opener> = None;
    let consider = |best: &mut Option<Opener>, candidate: Opener| {
        if best.as_ref().is_none_or(|b| candidate.start < b.start) {
            *best = Some(candidate);
        }
    };

    if let Some(re) = TAG_RE.as_ref() {
        for m in re.find_iter(&text[from..]) {
            // A marker with a slash is a closer, never an opener.
            let inner = &m.as_str()[1..];
            if inner.trim_start_matches(['|', ' ', '\t']).starts_with('/') {
                continue;
            }
            consider(
                &mut best,
                Opener {
                    start: from + m.start(),
                    body_start: from + m.end(),
                    kind: OpenerKind::Tag,
                },
            );
            break;
        }
    }

    if let Some(idx) = find_ci(text, "<invoke>", from) {
        consider(
            &mut best,
            Opener {
                start: idx,
                body_start: idx + "<invoke>".len(),
                kind: OpenerKind::Invoke,
            },
        );
    }

    for fence in FENCE_OPENERS {
        let mut cursor = from;
        while let Some(idx) = find_ci(text, fence, cursor) {
            let after = &text[idx + fence.len()..];
            // The language must end here (`tool_call` not `tool_calls`), and
            // the body starts on the next line.
            let rest = after.trim_start_matches([' ', '\t']);
            if let Some(nl) = rest
                .strip_prefix('\n')
                .or_else(|| rest.strip_prefix("\r\n"))
            {
                consider(
                    &mut best,
                    Opener {
                        start: idx,
                        body_start: text.len() - nl.len(),
                        kind: OpenerKind::Fence,
                    },
                );
                break;
            }
            cursor = idx + fence.len();
        }
    }

    best
}

/// The closer of a fenced block: a closing fence, a stray tag-family closer,
/// or `</invoke>`, whichever comes first.
fn fence_close(after: &str) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut consider = |candidate: Option<(usize, usize)>| {
        if let Some(c) = candidate
            && best.is_none_or(|b| c.0 < b.0)
        {
            best = Some(c);
        }
    };
    consider(after.find("```").map(|i| (i, i + 3)));
    consider(
        TAG_RE
            .as_ref()
            .and_then(|re| re.find(after))
            .filter(|m| {
                m.as_str()[1..]
                    .trim_start_matches(['|', ' '])
                    .starts_with('/')
            })
            .map(|m| (m.start(), m.end())),
    );
    consider(after.find("</invoke>").map(|i| (i, i + "</invoke>".len())));
    best
}

/// Everything a tag body can be, tried in order.
pub(crate) fn decode_body(body: &str, options: &ParseOptions<'_>) -> Vec<ParsedToolCall> {
    let body = strip_call_prefix(body);
    let is_known = |name: &str| options.knows(name);

    if let Some(registry) = options.registry
        && let Some((name, arguments)) = crate::pformat::parse_call(body, registry)
    {
        return vec![ParsedToolCall::new(name, arguments, CallSource::PFormat)];
    }

    if let Some(recovered) = recover_sentinel_body(body)
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&recovered)
    {
        let calls = read_calls(
            &value,
            AliasPolicy::Marked,
            &is_known,
            CallSource::TaggedJson,
        );
        if !calls.is_empty() {
            return calls;
        }
    }

    let unfenced = strip_code_fence(body);
    let mut calls = Vec::new();
    for value in extract_json_values(unfenced) {
        calls.extend(read_calls(
            &value,
            AliasPolicy::Marked,
            &is_known,
            CallSource::TaggedJson,
        ));
    }
    if !calls.is_empty() {
        return calls;
    }

    if let Some(value) = recover_object(unfenced) {
        let calls = read_calls(
            &value,
            AliasPolicy::Marked,
            &is_known,
            CallSource::TaggedJson,
        );
        if !calls.is_empty() {
            return calls;
        }
    }

    super::glm::parse_lines(body)
}

/// Strips a leading `call:` some models emit right after the open tag.
fn strip_call_prefix(body: &str) -> &str {
    let trimmed = body.trim();
    trimmed
        .strip_prefix("call:")
        .map_or(trimmed, str::trim_start)
}

/// Recovers a Kimi-K2-family `NAME{…}` body — the action name before a
/// JSON-ish object with unquoted keys and `<|"|>` in place of string quotes —
/// into canonical `{"name":…,"arguments":…}` JSON. `None` when the shape does
/// not match: a body already starting with `{`, a P-Format `NAME[…]` body,
/// or trailing text after the object all fall through unchanged.
pub(crate) fn recover_sentinel_body(body: &str) -> Option<String> {
    let repaired = body.replace(ARG_QUOTE_SENTINEL, "\"");
    let trimmed = repaired.trim();
    let brace = trimmed.find('{')?;
    let name = trimmed[..brace].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let object = trimmed[brace..].trim_end();
    if !object.starts_with('{') || !object.ends_with('}') {
        return None;
    }
    let arguments = recover_object(object)?;
    serde_json::to_string(&serde_json::json!({ "name": name, "arguments": arguments })).ok()
}
