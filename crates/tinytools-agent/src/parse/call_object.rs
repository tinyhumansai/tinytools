//! Reading a tool call out of an already-parsed JSON value.
//!
//! The shapes accepted, all seen from real models:
//!
//! * `{"name": "x", "arguments": {…}}` — canonical;
//! * `{"function": {"name": "x", "arguments": "{…}"}}` — the OpenAI wire
//!   entry, with stringified arguments;
//! * `{"tool_calls": [ … ]}` — a whole wire message (Minimax);
//! * `[ {…}, {…} ]` — a bare array of calls;
//! * the argument-key aliases in [`crate::repair::args::ARGUMENT_KEYS`].
//!
//! # The one rule that keeps this safe
//!
//! **Argument keys are aliased; tool names are not, and aliases need a
//! marker.** A model drifting from `arguments` to `args` still yields a usable
//! call. But a *bare* object — one the caller reached without a `<tool_call>`
//! tag, a `tool_calls` array, or a `function` wrapper — only counts as a call
//! when it carries the canonical `arguments` key or names a tool the caller
//! offered. Otherwise `{"name":"Alice","input":"hi"}`, an ordinary JSON
//! answer, would be dispatched as a phantom invocation.

use serde_json::Value;

use crate::repair::args;
use crate::types::{CallSource, ParsedToolCall};

/// Whether alias keys may be honoured on a bare `{"name": …}` object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AliasPolicy {
    /// Reached through an explicit marker: honour every alias.
    Marked,
    /// A bare object: require `arguments`, unless `name` is a known tool.
    Bare,
}

/// Reads one call object.
pub(crate) fn read_call(
    value: &Value,
    policy: AliasPolicy,
    is_known: &dyn Fn(&str) -> bool,
    source: CallSource,
) -> Option<ParsedToolCall> {
    if let Some(function) = value.get("function") {
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if !name.is_empty() {
            return Some(ParsedToolCall::new(
                name,
                args::from_call_object(function),
                source,
            ));
        }
    }

    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        return None;
    }

    let arguments = match policy {
        AliasPolicy::Marked => args::from_call_object(value),
        AliasPolicy::Bare if is_known(name) => args::from_call_object(value),
        AliasPolicy::Bare => args::decode(Some(value.get("arguments")?)),
    };
    Some(ParsedToolCall::new(name, arguments, source))
}

/// Reads every call in `value`: a `tool_calls` envelope, an array, or a
/// single object. The envelope is itself a marker, so its entries are always
/// read with [`AliasPolicy::Marked`].
pub(crate) fn read_calls(
    value: &Value,
    policy: AliasPolicy,
    is_known: &dyn Fn(&str) -> bool,
    source: CallSource,
) -> Vec<ParsedToolCall> {
    let mut calls = Vec::new();

    if let Some(tool_calls) = value.get("tool_calls").and_then(Value::as_array) {
        for call in tool_calls {
            if let Some(parsed) = read_call(call, AliasPolicy::Marked, is_known, source) {
                calls.push(parsed);
            }
        }
        if !calls.is_empty() {
            return calls;
        }
    }

    if let Some(array) = value.as_array() {
        for item in array {
            if let Some(parsed) = read_call(item, policy, is_known, source) {
                calls.push(parsed);
            }
        }
        return calls;
    }

    if let Some(parsed) = read_call(value, policy, is_known, source) {
        calls.push(parsed);
    }
    calls
}

/// Public, marker-context form of [`read_call`].
#[must_use]
pub fn parse_tool_call_value(value: &Value) -> Option<ParsedToolCall> {
    read_call(
        value,
        AliasPolicy::Marked,
        &|_| false,
        CallSource::TaggedJson,
    )
}

/// Public, marker-context form of [`read_calls`].
#[must_use]
pub fn parse_tool_calls_from_json_value(value: &Value) -> Vec<ParsedToolCall> {
    read_calls(
        value,
        AliasPolicy::Marked,
        &|_| false,
        CallSource::TaggedJson,
    )
}
