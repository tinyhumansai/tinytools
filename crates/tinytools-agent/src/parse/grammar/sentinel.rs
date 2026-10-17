//! Chat-template sentinel tokens emitted verbatim as text.
//!
//! When an OpenAI-compatible route serves a model through its own chat
//! template without decoding the template's special tokens, the call arrives
//! as the raw tokens. Two families are common enough to matter:
//!
//! * **`DeepSeek`** (R1, V3):
//!   `<｜tool▁calls▁begin｜><｜tool▁call▁begin｜>function<｜tool▁sep｜>NAME\n```json\n{…}\n```<｜tool▁call▁end｜><｜tool▁calls▁end｜>`,
//!   or the shorter `<｜tool▁call▁begin｜>NAME<｜tool▁sep｜>{…}<｜tool▁call▁end｜>`;
//! * **Kimi K2**:
//!   `<|tool_calls_section_begin|><|tool_call_begin|>functions.NAME:0<|tool_call_argument_begin|>{…}<|tool_call_end|><|tool_calls_section_end|>`.
//!
//! Both appear with fullwidth (`｜`) or ASCII (`|`) bars and with `▁` or `_`
//! between words, so the markers are matched by a small regex rather than a
//! literal table. The section wrappers are furniture and are removed; each
//! `call_begin … call_end` block yields one call.

use std::sync::LazyLock;

use regex::Regex;

use super::{Block, Decoded, Grammar, Probe, ScanMode, pending_opener, prefer_pending};
use crate::parse::call_object::{AliasPolicy, read_calls};
use crate::repair::json::{recover_object_tracked, strip_code_fence};
use crate::types::{CallSource, ParseOptions, ParsedToolCall, Termination};

/// The sentinel-token grammar.
#[derive(Debug)]
pub(crate) struct Sentinel;

/// `<|tool_call_begin|>` in every bar/underscore spelling.
static CALL_BEGIN_RE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"<[|｜]tool[▁_]call[▁_]begin[|｜]>").ok());

/// `<|tool_call_end|>`.
static CALL_END_RE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"<[|｜]tool[▁_]call[▁_]end[|｜]>").ok());

/// The section wrappers: `<|tool_calls_begin|>`, `<|tool_calls_end|>`,
/// `<|tool_calls_section_begin|>`, `<|tool_calls_section_end|>`.
static WRAPPER_RE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"<[|｜]tool[▁_]calls(?:[▁_]section)?[▁_](?:begin|end)[|｜]>").ok()
});

/// `DeepSeek`'s name/arguments separator.
static SEP_RE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"<[|｜]tool[▁_]sep[|｜]>").ok());

/// Kimi's name/arguments separator.
static ARGUMENT_BEGIN_RE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"<[|｜]tool[▁_]call[▁_]argument[▁_]begin[|｜]>").ok());

impl Grammar for Sentinel {
    fn source(&self) -> CallSource {
        CallSource::Sentinel
    }

    fn probe(&self, text: &str, from: usize, options: &ParseOptions<'_>, mode: ScanMode) -> Probe {
        let pending = pending_opener(text, from, self.openers(), ">", mode);
        prefer_pending(Self::probe_decided(text, from, options, mode), pending)
    }

    fn openers(&self) -> &'static [&'static str] {
        // The bar style (`|` / `｜`) and the word separator (`_` / `▁`) vary
        // independently — `SEP_RE`/`CALL_BEGIN_RE`/etc. accept all four
        // combinations — so every combination needs its own literal here.
        // Only two of the four were listed before, which let a split after
        // e.g. `<｜tool_` (fullwidth bar, ASCII underscore) leak: neither
        // literal is a substring of it, so `hold_from` released it as plain
        // text and the completed marker was never recognized.
        &[
            "<|tool_call",
            "<|tool▁call",
            "<｜tool_call",
            "<｜tool▁call",
            "<|tool_calls",
            "<|tool▁calls",
            "<｜tool_calls",
            "<｜tool▁calls",
        ]
    }
}

impl Sentinel {
    /// The next block whose opener is fully present.
    fn probe_decided(text: &str, from: usize, options: &ParseOptions<'_>, mode: ScanMode) -> Probe {
        let (Some(begin_re), Some(end_re), Some(wrapper_re)) = (
            CALL_BEGIN_RE.as_ref(),
            CALL_END_RE.as_ref(),
            WRAPPER_RE.as_ref(),
        ) else {
            return Probe::None;
        };
        let hay = &text[from..];
        let wrapper = wrapper_re.find(hay);
        let begin = begin_re.find(hay);

        if let Some(w) = wrapper
            && begin.is_none_or(|b| w.start() < b.start())
        {
            return Probe::Found(Block {
                start: from + w.start(),
                end: from + w.end(),
                decoded: Decoded::Noise,
            });
        }
        let Some(begin) = begin else {
            return Probe::None;
        };

        let start = from + begin.start();
        let body_start = from + begin.end();
        let after = &text[body_start..];
        let Some(end) = end_re.find(after) else {
            if mode == ScanMode::Stream {
                return Probe::Pending { start };
            }
            return Probe::Found(Block {
                start,
                end: text.len(),
                decoded: Decoded::Verbatim,
            });
        };

        let body = &after[..end.start()];
        let block_end = body_start + end.end();
        let calls = decode_body(body, options);
        let decoded = if calls.is_empty() {
            Decoded::Malformed {
                body_chars: body.chars().count(),
            }
        } else {
            Decoded::Calls(calls)
        };
        Probe::Found(Block {
            start,
            end: block_end,
            decoded,
        })
    }
}

/// The body between `call_begin` and `call_end`, in every known layout.
fn decode_body(body: &str, options: &ParseOptions<'_>) -> Vec<ParsedToolCall> {
    let is_known = |name: &str| options.knows(name);

    // Kimi: `functions.NAME:0<|tool_call_argument_begin|>{…}`.
    if let Some(re) = ARGUMENT_BEGIN_RE.as_ref()
        && let Some(sep) = re.find(body)
    {
        let name = kimi_name(&body[..sep.start()]);
        let (arguments, termination) = arguments_from(&body[sep.end()..]);
        if !name.is_empty() {
            return vec![
                ParsedToolCall::new(name, arguments, CallSource::Sentinel)
                    .with_termination(termination),
            ];
        }
        return Vec::new();
    }

    // DeepSeek: `function<|tool_sep|>NAME\n```json\n{…}\n```` or `NAME<|tool_sep|>{…}`.
    if let Some(re) = SEP_RE.as_ref()
        && let Some(sep) = re.find(body)
    {
        let left = body[..sep.start()].trim();
        let right = body[sep.end()..].trim();
        let (name, raw_arguments) = if matches!(left, "function" | "tool" | "functions") {
            match right.split_once('\n') {
                Some((first, rest)) => (first.trim(), rest),
                None => (right, ""),
            }
        } else {
            (left, right)
        };
        if name.is_empty() {
            return Vec::new();
        }
        let (arguments, termination) = arguments_from(raw_arguments);
        return vec![
            ParsedToolCall::new(name, arguments, CallSource::Sentinel)
                .with_termination(termination),
        ];
    }

    // No separator: a `{"name":…,"arguments":…}` object, possibly fenced.
    let unfenced = strip_code_fence(body);
    let (value, termination) = match serde_json::from_str::<serde_json::Value>(unfenced) {
        Ok(value) => (value, Termination::Terminated),
        Err(_) => match recover_object_tracked(unfenced) {
            Some(recovered) => (recovered.value, termination_of(recovered.closers_balanced)),
            None => return Vec::new(),
        },
    };
    read_calls(&value, AliasPolicy::Marked, &is_known, CallSource::Sentinel)
        .into_iter()
        .map(|call| call.with_termination(termination))
        .collect()
}

/// [`Termination::ClosersBalanced`] when the body's object only parsed once
/// its missing closers were appended.
fn termination_of(closers_balanced: bool) -> Termination {
    if closers_balanced {
        Termination::ClosersBalanced
    } else {
        Termination::Terminated
    }
}

/// `functions.NAME:0` → `NAME`.
fn kimi_name(raw: &str) -> &str {
    let trimmed = raw.trim();
    let without_index = trimmed.rsplit_once(':').map_or(trimmed, |(head, tail)| {
        if tail.chars().all(|c| c.is_ascii_digit()) {
            head
        } else {
            trimmed
        }
    });
    without_index
        .strip_prefix("functions.")
        .unwrap_or(without_index)
        .trim()
}

/// Arguments from a possibly fenced JSON object, and how they ended; empty
/// object when absent.
fn arguments_from(raw: &str) -> (serde_json::Value, Termination) {
    let unfenced = strip_code_fence(raw.trim());
    if unfenced.is_empty() {
        return (serde_json::json!({}), Termination::Terminated);
    }
    if let Ok(value @ serde_json::Value::Object(_)) =
        serde_json::from_str::<serde_json::Value>(unfenced)
    {
        return (value, Termination::Terminated);
    }
    match recover_object_tracked(unfenced) {
        Some(recovered) => (recovered.value, termination_of(recovered.closers_balanced)),
        None => (serde_json::json!({}), Termination::Terminated),
    }
}
