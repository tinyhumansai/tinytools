//! Rendering a structured call back into the text form a prompt-guided model
//! is shown on replay.
//!
//! A model without a native tool channel cannot read an assistant turn's
//! `tool_calls` field, so on the next request the calls it made are written
//! back into its transcript as the same `<tool_call>` markup it was taught.
//! This is the one place that markup is *written*, so the replay can never
//! drift from what [`crate::parse`] reads.

use std::fmt::Write as _;

use serde_json::Value;

/// One call as `<tool_call>{"name":…,"arguments":…}</tool_call>`.
#[must_use]
pub fn render_json_call(name: &str, arguments: &Value) -> String {
    let body = serde_json::json!({ "name": name, "arguments": arguments });
    format!(
        "<tool_call>{}</tool_call>",
        serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_string())
    )
}

/// Several calls, one per line.
#[must_use]
pub fn render_json_calls<'a>(calls: impl IntoIterator<Item = (&'a str, &'a Value)>) -> String {
    let mut out = String::new();
    for (name, arguments) in calls {
        let _ = writeln!(out, "{}", render_json_call(name, arguments));
    }
    out
}
