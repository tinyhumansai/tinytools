//! Recovering a JSON **object** from the relaxed, damaged, or noise-wrapped
//! text a model emits where strict JSON was asked for.
//!
//! Every stage here is a shape a real model produced for tool arguments:
//!
//! * leaked chat-template markers glued to the value — `{"a":1}<tool_call|>`
//!   (OpenAI-compatible gateways that fail to strip the template);
//! * the model's string-delimiter token emitted as text — `[<|">x<|">]`
//!   (Kimi-family via some gateways);
//! * a surrounding markdown fence;
//! * raw control characters inside strings (llama.cpp, tabs and newlines);
//! * trailing commas, and objects cut off before their closing brace
//!   (streams that ended mid-call);
//! * redundant outer braces `{{…}}` the model piles on each time the previous
//!   attempt bounced, and unquoted keys `{tool:"x"}`;
//! * typographic quotes `“…”`.
//!
//! The ladder is applied **only after strict parsing has failed**, and the
//! result is accepted only when it parses strictly *and* is an object, so a
//! scalar scraped out of noise can never masquerade as arguments. Stages are
//! cumulative and cheap: each one is a pass over the string, and most inputs
//! exit at the first or second rung.

use serde_json::Value;

/// Chat-template tool-call delimiters that gateways sometimes fail to strip
/// before placing a call in `function.arguments`, or that a model narrating a
/// call leaves glued to the JSON. Removed outright — they are structure, not
/// content.
pub const TEMPLATE_MARKERS: &[&str] = &[
    "<|tool_calls_section_begin|>",
    "<|tool_calls_section_end|>",
    "<|tool_call_argument_begin|>",
    "<|tool_call_begin|>",
    "<|tool_call_end|>",
    "<|tool_call|>",
    "<|tool_sep|>",
    "<tool_call|>",
    "</tool_call>",
    "<tool_call>",
];

/// Chat-template string-delimiter tokens emitted as literal text in place of
/// a `"` (Kimi-family models via GMI: `[<|">discord<|">]`). Substituted to
/// `"`, not deleted. Longer forms first so a substitution never leaves a
/// partial token behind.
pub const LEAKED_QUOTE_TOKENS: &[&str] = &["<|\"|>", "<|\">"];

/// Maximum redundant outer brace layers to peel. Bounds work on adversarial
/// `{{{{…}}}}` blobs while comfortably covering every depth seen in the wild.
const MAX_BRACE_PEEL: usize = 16;

/// Maximum excess closing brackets trimmed from the tail.
const MAX_EXCESS_CLOSERS: usize = 50;

/// Attempts to recover a strict-JSON **object** from relaxed or damaged text.
///
/// Returns `None` when no conservative repair yields an object. Call this only
/// after `serde_json::from_str` has already failed on `raw`; a well-formed
/// object is returned unchanged by the first rung anyway, but the ladder is not
/// free.
///
/// The final rung accepts a valid object followed by trailing noise — correct
/// when `raw` is already known to be *inside* a call (a marker-delimited
/// argument payload), where anything after the object is furniture, not data.
/// A whole-response candidate has no such delimiter and must use
/// [`recover_whole_object`] instead, which holds out for the entire candidate.
#[must_use]
pub fn recover_object(raw: &str) -> Option<Value> {
    recover_ladder(raw, true)
}

/// [`recover_object`]'s ladder, but without the trailing-noise rung: the
/// repaired object must account for the **entire** candidate.
///
/// For a whole-response grammar (bare JSON), accepting a valid leading object
/// followed by unrelated trailing text would dispatch a call out of ordinary
/// prose that merely starts with one — `{"name":"shell","arguments":{}}
/// explanation follows` is not a call, it is prose that happens to start with
/// one.
#[must_use]
pub fn recover_whole_object(raw: &str) -> Option<Value> {
    recover_ladder(raw, false)
}

fn recover_ladder(raw: &str, allow_trailing_noise: bool) -> Option<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(object) = parse_object(trimmed) {
        return Some(object);
    }

    // Rung 1: unwrap and de-noise, then retry strictly.
    let mut candidate = strip_code_fence(trimmed).to_string();
    candidate = strip_template_markers(&candidate);
    candidate = normalize_leaked_quote_tokens(&candidate);
    if let Some(object) = parse_object(candidate.trim()) {
        return Some(object);
    }

    // Rung 2: character-level damage a strict parser rejects outright.
    candidate = escape_control_characters(&candidate);
    candidate = strip_trailing_commas(&candidate);
    if let Some(object) = parse_object(candidate.trim()) {
        return Some(object);
    }
    if let Some(object) = parse_object(&balance_closers(candidate.trim())) {
        return Some(object);
    }

    // Rung 3: structural relaxations, retried at each brace depth.
    if let Some(object) = peel_and_quote(candidate.trim()) {
        return Some(object);
    }

    // Rung 4: typographic quotes, then the whole ladder once more.
    let straightened = straighten_quotes(&candidate);
    if straightened != candidate {
        if let Some(object) = parse_object(straightened.trim()) {
            return Some(object);
        }
        if let Some(object) = peel_and_quote(&balance_closers(straightened.trim())) {
            return Some(object);
        }
    }

    // Rung 5: a valid leading object followed by trailing noise. Only when
    // the caller has already established that trailing noise is expected.
    if allow_trailing_noise {
        return leading_object(candidate.trim());
    }
    None
}

/// Parses `s` strictly and keeps it only when it is an object.
fn parse_object(s: &str) -> Option<Value> {
    match serde_json::from_str::<Value>(s) {
        Ok(value @ Value::Object(_)) => Some(value),
        _ => None,
    }
}

/// The first complete JSON object at the front of `s`, ignoring what follows.
fn leading_object(s: &str) -> Option<Value> {
    let mut values = serde_json::Deserializer::from_str(s).into_iter::<Value>();
    match values.next() {
        Some(Ok(value @ Value::Object(_))) => Some(value),
        _ => None,
    }
}

/// Alternates brace peeling and bare-key quoting until one parses.
fn peel_and_quote(s: &str) -> Option<Value> {
    let mut layer = s.to_string();
    for _ in 0..=MAX_BRACE_PEEL {
        if let Some(object) = parse_object(&layer) {
            return Some(object);
        }
        let quoted = quote_bare_keys(&layer);
        if quoted != layer
            && let Some(object) = parse_object(&quoted)
        {
            return Some(object);
        }
        match peel_redundant_brace(&layer) {
            Some(inner) => layer = inner,
            None => break,
        }
    }
    None
}

/// Strips one surrounding markdown code fence, with or without a language tag.
#[must_use]
pub fn strip_code_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    let Some(after_open) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    let body = match after_open.find('\n') {
        Some(newline)
            if after_open[..newline]
                .trim()
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') =>
        {
            &after_open[newline + 1..]
        }
        Some(_) => after_open,
        None => return trimmed,
    };
    body.trim_end()
        .strip_suffix("```")
        .map_or(trimmed, str::trim)
}

/// Removes every [`TEMPLATE_MARKERS`] occurrence.
#[must_use]
pub fn strip_template_markers(raw: &str) -> String {
    let mut out = raw.to_string();
    for marker in TEMPLATE_MARKERS {
        if out.contains(marker) {
            out = out.replace(marker, "");
        }
    }
    out
}

/// Substitutes every [`LEAKED_QUOTE_TOKENS`] occurrence with `"`.
#[must_use]
pub fn normalize_leaked_quote_tokens(raw: &str) -> String {
    let mut out = raw.to_string();
    for token in LEAKED_QUOTE_TOKENS {
        if out.contains(token) {
            out = out.replace(token, "\"");
        }
    }
    out
}

/// Escapes raw control characters (U+0000..U+001F) that appear *inside* JSON
/// string literals, which strict parsers reject. Characters outside strings
/// are left alone.
#[must_use]
pub fn escape_control_characters(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_string = false;
    let mut escaped = false;
    for ch in s.chars() {
        if in_string {
            if escaped {
                escaped = false;
                out.push(ch);
                continue;
            }
            match ch {
                '\\' => {
                    escaped = true;
                    out.push(ch);
                }
                '"' => {
                    in_string = false;
                    out.push(ch);
                }
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => {
                    use std::fmt::Write as _;
                    let _ = write!(out, "\\u{:04x}", c as u32);
                }
                c => out.push(c),
            }
        } else {
            if ch == '"' {
                in_string = true;
            }
            out.push(ch);
        }
    }
    out
}

/// Removes a `,` that directly precedes a `}` or `]` (whitespace allowed),
/// outside string literals.
#[must_use]
pub fn strip_trailing_commas(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_string = false;
    let mut escaped = false;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if ch == '"' {
            in_string = true;
            out.push(ch);
            i += 1;
            continue;
        }
        if ch == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && (chars[j] == '}' || chars[j] == ']') {
                i += 1;
                continue;
            }
        }
        out.push(ch);
        i += 1;
    }
    out
}

/// Appends the closers an unterminated value is missing, or trims a bounded
/// run of excess closers, so a call cut off mid-stream still parses.
#[must_use]
pub fn balance_closers(s: &str) -> String {
    let mut stack: Vec<char> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut excess = 0usize;
    for ch in s.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => stack.push('}'),
            '[' => stack.push(']'),
            '}' | ']' => {
                if stack.last() == Some(&ch) {
                    stack.pop();
                } else {
                    excess += 1;
                }
            }
            _ => {}
        }
    }
    if in_string {
        // A value cut off mid-string cannot be completed honestly: guessing
        // where it ended would hand a tool a silently truncated argument.
        return s.to_string();
    }
    let mut out = s.to_string();
    if excess > 0 && excess <= MAX_EXCESS_CLOSERS && stack.is_empty() {
        let mut trimmed = out.trim_end().to_string();
        for _ in 0..excess {
            if trimmed.ends_with('}') || trimmed.ends_with(']') {
                trimmed.pop();
                trimmed = trimmed.trim_end().to_string();
            }
        }
        return trimmed;
    }
    while let Some(closer) = stack.pop() {
        out.push(closer);
    }
    out
}

/// Replaces typographic double and single quotes with their ASCII forms.
#[must_use]
pub fn straighten_quotes(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => '"',
            '\u{2018}' | '\u{2019}' => '\'',
            other => other,
        })
        .collect()
}

/// Peels one redundant outer brace layer wrapping exactly one object.
fn peel_redundant_brace(s: &str) -> Option<String> {
    let trimmed = s.trim();
    let inner = trimmed.strip_prefix('{')?.strip_suffix('}')?.trim();
    if inner.starts_with('{') && object_spans_all(inner) {
        Some(inner.to_string())
    } else {
        None
    }
}

/// Whether the object opened at byte 0 closes exactly at the end of `s`.
fn object_spans_all(s: &str) -> bool {
    if !s.starts_with('{') {
        return false;
    }
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (idx, ch) in s.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = match depth.checked_sub(1) {
                    Some(d) => d,
                    None => return false,
                };
                if depth == 0 {
                    return idx + ch.len_utf8() == s.len();
                }
            }
            _ => {}
        }
    }
    false
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Container {
    Object,
    Array,
}

/// Reads a key opened by `"` or `'` up to a closing quote that is followed by
/// `:`. Returns the key text and the bytes consumed. `None` when the quoting
/// is already correct (so the caller leaves it alone) or when the run does
/// not look like a key at all.
fn take_quoted_key(rest: &str) -> Option<(String, usize)> {
    let mut chars = rest.char_indices();
    let (_, open) = chars.next()?;
    let mut key = String::new();
    for (idx, ch) in chars {
        match ch {
            '"' | '\'' => {
                let after = &rest[idx + ch.len_utf8()..];
                if after.trim_start().starts_with(':') {
                    if open == '"' && ch == '"' {
                        return None;
                    }
                    return Some((key, idx + ch.len_utf8()));
                }
                key.push(ch);
            }
            '\n' | '\r' | '{' | '}' | '[' | ']' | ':' => return None,
            _ => key.push(ch),
        }
    }
    None
}

/// Quotes bare and single-quoted object **keys**, string- and array-aware so
/// values and array literals are never rewritten.
#[must_use]
pub fn quote_bare_keys(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let mut stack: Vec<Container> = Vec::new();
    let mut expect_key = false;
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = s.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' | '\'' if expect_key && matches!(stack.last(), Some(Container::Object)) => {
                if let Some((key, consumed)) = take_quoted_key(&s[idx..]) {
                    out.push('"');
                    out.push_str(&key.replace('\\', r"\\").replace('"', "\\\""));
                    out.push('"');
                    while chars.peek().is_some_and(|&(next, _)| next < idx + consumed) {
                        chars.next();
                    }
                    expect_key = false;
                } else {
                    in_string = true;
                    expect_key = false;
                    out.push(ch);
                }
            }
            '"' => {
                in_string = true;
                expect_key = false;
                out.push(ch);
            }
            '{' => {
                stack.push(Container::Object);
                expect_key = true;
                out.push(ch);
            }
            '}' | ']' => {
                stack.pop();
                expect_key = false;
                out.push(ch);
            }
            '[' => {
                stack.push(Container::Array);
                expect_key = false;
                out.push(ch);
            }
            ',' => {
                expect_key = matches!(stack.last(), Some(Container::Object));
                out.push(ch);
            }
            ':' => {
                expect_key = false;
                out.push(ch);
            }
            c if c.is_whitespace() => out.push(ch),
            c if expect_key
                && matches!(stack.last(), Some(Container::Object))
                && (c.is_ascii_alphabetic() || c == '_') =>
            {
                let start = idx;
                let mut end = idx + c.len_utf8();
                while let Some(&(next_idx, next_ch)) = chars.peek() {
                    if next_ch.is_ascii_alphanumeric()
                        || next_ch == '_'
                        || next_ch == '-'
                        || next_ch == '.'
                    {
                        end = next_idx + next_ch.len_utf8();
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push('"');
                out.push_str(&s[start..end]);
                out.push('"');
                expect_key = false;
            }
            _ => {
                expect_key = false;
                out.push(ch);
            }
        }
    }
    out
}
