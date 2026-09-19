//! A response that *is* a JSON value.
//!
//! Two shapes, both observed:
//!
//! * a provider that returns the whole wire message as text —
//!   `{"content":"…","tool_calls":[…]}` (Minimax behind some gateways);
//! * a small model under `tool_choice: "required"` that writes the call
//!   object as its entire reply, no markup, often with damaged quoting
//!   (`llama3.2:3b` via Ollama: `{"name":"get_weather","parameters':{'city':"Paris"}}`).
//!
//! This is the one grammar with no marker, so it is the most tightly gated:
//! the **entire** trimmed response (after one surrounding fence) must be a
//! single JSON object or array, and a bare `{"name": …}` object only counts
//! when it carries the canonical `arguments` key or names a tool the caller
//! offered. Prose that quotes JSON has text outside the value and is left
//! alone; `{"name":"Alice","input":"hi"}` is left alone.

use crate::parse::call_object::{AliasPolicy, read_calls};
use crate::repair::json::{recover_object, strip_code_fence};
use crate::types::{CallSource, ParseOptions, ParsedToolCall};

/// The calls in a whole-response JSON value, plus any `content` text it
/// carried. `None` when the response is not one JSON value.
pub(crate) fn parse(
    text: &str,
    options: &ParseOptions<'_>,
) -> Option<(String, Vec<ParsedToolCall>)> {
    let candidate = strip_code_fence(text.trim());
    let (first, last) = (candidate.chars().next()?, candidate.chars().last()?);
    if !matches!((first, last), ('{', '}') | ('[', ']')) {
        return None;
    }
    let is_known = |name: &str| options.knows(name);

    let value = match serde_json::from_str::<serde_json::Value>(candidate) {
        Ok(value) => value,
        // A non-object that parsed strictly is not a call; do not "repair"
        // it into one. Only an object-shaped candidate is worth recovering.
        Err(_) if first == '{' => recover_object(candidate)?,
        Err(_) => return None,
    };

    let calls = read_calls(&value, AliasPolicy::Bare, &is_known, CallSource::BareJson);
    if calls.is_empty() {
        return None;
    }
    let content = value
        .get("content")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .unwrap_or("")
        .to_string();
    Some((content, calls))
}
