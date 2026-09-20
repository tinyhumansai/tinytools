//! GLM's line-oriented `tool/param>value` calls.
//!
//! GLM models prompted for tools sometimes answer with one call per line:
//!
//! ```text
//! browser_open/url>https://example.com
//! shell/command>ls -la
//! http_request/url>https://api.example.com
//! custom/{"answer":42}
//! ```
//!
//! This is not a marker-delimited grammar, so it never runs on arbitrary text
//! in the scan: it is tried on a tag body that decoded to nothing else, and
//! on the whole response only when no other grammar found a call. A handful
//! of GLM's own tool names are mapped onto the host's (`browser_open` →
//! `shell` with a `curl`), which is the one place this crate knows a tool
//! name.

use serde_json::Value;

use crate::types::{CallSource, ParsedToolCall};

/// Maps GLM's built-in tool names onto the host's.
#[must_use]
pub fn map_glm_tool_alias(tool_name: &str) -> &str {
    match tool_name {
        "browser_open" | "browser" | "web_search" | "shell" | "bash" => "shell",
        "http_request" | "http" => "http_request",
        _ => tool_name,
    }
}

/// A `curl` command fetching `url`, or `None` when it is not a plain
/// `http(s)` URL without whitespace.
#[must_use]
pub fn build_curl_command(url: &str) -> Option<String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    if url.chars().any(char::is_whitespace) {
        return None;
    }
    let escaped = url.replace('\'', "'\\''");
    Some(format!("curl -s '{escaped}'"))
}

/// Every GLM-style call in `text`, as `(name, arguments, raw line)`.
#[must_use]
pub fn parse_glm_style_tool_calls(text: &str) -> Vec<(String, Value, Option<String>)> {
    let mut calls = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(pos) = line.find('/') else {
            continue;
        };
        let tool_part = &line[..pos];
        let rest = &line[pos + 1..];
        if tool_part.is_empty() || !tool_part.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let tool_name = map_glm_tool_alias(tool_part);

        if let Some(gt_pos) = rest.find('>') {
            let param_name = rest[..gt_pos].trim();
            let value = rest[gt_pos + 1..].trim();
            let arguments = match tool_name {
                "shell" => {
                    if param_name == "url" {
                        let Some(command) = build_curl_command(value) else {
                            continue;
                        };
                        serde_json::json!({ "command": command })
                    } else if value.starts_with("http://") || value.starts_with("https://") {
                        match build_curl_command(value) {
                            Some(command) => serde_json::json!({ "command": command }),
                            None => serde_json::json!({ "command": value }),
                        }
                    } else {
                        serde_json::json!({ "command": value })
                    }
                }
                "http_request" => serde_json::json!({ "url": value, "method": "GET" }),
                _ => serde_json::json!({ param_name: value }),
            };
            calls.push((tool_name.to_string(), arguments, Some(line.to_string())));
            continue;
        }

        if rest.starts_with('{')
            && let Ok(json_args) = serde_json::from_str::<Value>(rest)
        {
            calls.push((tool_name.to_string(), json_args, Some(line.to_string())));
        }
    }

    calls
}

/// [`parse_glm_style_tool_calls`] as [`ParsedToolCall`]s.
pub(crate) fn parse_lines(text: &str) -> Vec<ParsedToolCall> {
    parse_glm_style_tool_calls(text)
        .into_iter()
        .map(|(name, arguments, _)| ParsedToolCall::new(name, arguments, CallSource::Glm))
        .collect()
}

/// The calls in `text` plus the text with their lines removed.
pub(crate) fn parse_and_strip(text: &str) -> (String, Vec<ParsedToolCall>) {
    let parsed = parse_glm_style_tool_calls(text);
    if parsed.is_empty() {
        return (text.to_string(), Vec::new());
    }
    let mut cleaned = text.to_string();
    let mut calls = Vec::with_capacity(parsed.len());
    for (name, arguments, raw) in parsed {
        calls.push(ParsedToolCall::new(name, arguments, CallSource::Glm));
        if let Some(raw) = raw {
            cleaned = cleaned.replace(&raw, "");
        }
    }
    (cleaned, calls)
}
