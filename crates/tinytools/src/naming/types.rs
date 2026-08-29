//! Turning a machine tool name and its arguments into something a person can
//! read in a timeline row.

use serde_json::Value;

/// How a context detail is trimmed for display.
///
/// Exists because the cap and the ellipsis are **presentation**, and a host
/// that renders tool activity in its own timeline has already picked both. The
/// key-scanning rule underneath is what is actually shared; forcing a host to
/// re-implement the whole function to change one character is how two copies of
/// it end up in a codebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ContextDetailOptions {
    /// Maximum rendered length, in characters, including the ellipsis.
    pub max_chars: usize,
    /// Appended when the value is trimmed.
    pub ellipsis: &'static str,
}

impl ContextDetailOptions {
    /// Options with an explicit cap and ellipsis.
    #[must_use]
    pub fn new(max_chars: usize, ellipsis: &'static str) -> Self {
        Self {
            max_chars,
            ellipsis,
        }
    }
}

impl Default for ContextDetailOptions {
    fn default() -> Self {
        Self {
            max_chars: 80,
            ellipsis: "...",
        }
    }
}

/// Derives a title-cased, human-readable label from a raw tool name.
///
/// Common machine prefixes are stripped and `snake_case` / `kebab-case` becomes
/// spaced title case, so `gmail_read_message` reads as "Gmail Read Message".
/// Degenerate names fall back to the original input, so a caller never receives
/// an empty label unless the input itself was empty.
///
/// The prefix list is shared deliberately: two copies of it is how one of them
/// silently stops stripping a prefix the other does, and the symptom — a
/// timeline row reading `composio_gmail_send_email` — surfaces far from the
/// cause.
#[must_use]
pub fn humanize_tool_name(name: &str) -> String {
    let trimmed = name
        .strip_prefix("composio_")
        .or_else(|| name.strip_prefix("mcp_"))
        .unwrap_or(name);

    let mut out = String::with_capacity(trimmed.len());
    let mut capitalize = true;
    for ch in trimmed.chars() {
        if ch == '_' || ch == '-' {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            capitalize = true;
        } else if capitalize {
            out.extend(ch.to_uppercase());
            capitalize = false;
        } else {
            out.push(ch);
        }
    }

    let label = out.trim();
    if label.is_empty() {
        name.to_string()
    } else {
        label.to_string()
    }
}

/// Extracts a compact human-facing detail from common tool argument keys.
///
/// The first recognized scalar value wins, using keys that usually identify the
/// resource being acted on (`path`, `query`, `to`, `url`, and similar). Returns
/// `None` for non-object arguments, empty values, and complex values.
///
/// Uses [`ContextDetailOptions::default`]; see
/// [`context_detail_from_args_with`] to choose the cap and ellipsis.
#[must_use]
pub fn context_detail_from_args(args: &Value) -> Option<String> {
    context_detail_from_args_with(args, ContextDetailOptions::default())
}

/// [`context_detail_from_args`] with explicit trimming.
#[must_use]
pub fn context_detail_from_args_with(
    args: &Value,
    options: ContextDetailOptions,
) -> Option<String> {
    // Ordered by specificity: a messaging call carries both `to` and `name`,
    // and the recipient is the useful half.
    const CONTEXT_KEYS: &[&str] = &[
        "to",
        "recipient",
        "recipient_email",
        "to_email",
        "email",
        "query",
        "q",
        "search",
        "search_query",
        "url",
        "file_path",
        "path",
        "command",
        "cmd",
        "subject",
        "title",
        "channel",
        "channel_id",
        "repo",
        "repository",
        "name",
        "id",
    ];

    let obj = args.as_object()?;
    CONTEXT_KEYS
        .iter()
        .filter_map(|key| obj.get(*key))
        .find_map(|value| render_context_value(value, options))
}

fn render_context_value(value: &Value, options: ContextDetailOptions) -> Option<String> {
    let raw = match value {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        Value::Null | Value::Object(_) => String::new(),
    };
    let raw = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if raw.is_empty() {
        return None;
    }
    if raw.chars().count() > options.max_chars {
        // Clamp the ellipsis itself to `max_chars` first: an ellipsis longer
        // than the cap (a misconfigured caller) would otherwise survive
        // `saturating_sub`'s zero and still be appended in full, pushing the
        // rendered value past the cap it was supposed to enforce.
        let ellipsis: String = options.ellipsis.chars().take(options.max_chars).collect();
        let keep = options.max_chars.saturating_sub(ellipsis.chars().count());
        let truncated: String = raw.chars().take(keep).collect();
        Some(format!("{truncated}{ellipsis}"))
    } else {
        Some(raw)
    }
}
