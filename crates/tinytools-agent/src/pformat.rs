//! P-Format ("Parameter-Format") tool calls — compact, positional,
//! pipe-delimited tool invocations designed to slash the token cost of
//! text-based tool calling.
//!
//! # Why
//!
//! Standard JSON tool calls are heavy on tokens for what's actually a
//! simple instruction:
//!
//! ```text
//! {"name": "get_weather", "arguments": {"location": "London", "unit": "metric"}}
//! ```
//!
//! That's roughly 25 tokens. The same call in P-Format:
//!
//! ```text
//! get_weather[London|metric]
//! ```
//!
//! is ~5 tokens — an 80% reduction. Across a long agent loop with many
//! tool calls per turn, that compounds dramatically.
//!
//! # Spec
//!
//! - One call per `<tool_call>...</tool_call>` tag body.
//! - Form: `name[index|value|index|value|...]` — each argument carries the
//!   slot index it belongs to, so **only the arguments actually being sent
//!   appear**.
//! - `name` is the tool's registered name (alphanumerics + `_`).
//! - Slot indices number the parameters **required first** (in the order the
//!   schema declares them), then the optional ones alphabetically. Both halves
//!   are deterministic across rebuilds and workspaces: a JSON array preserves
//!   order, and `Map` iterates as a `BTreeMap` because this build does not
//!   enable `preserve_order`.
//! - The renderer exposes the numbering in the tool catalogue, each slot marked
//!   as a placeholder to fill:
//!   `get_weather[0|<location>|1|<unit>]`, `math[0|<verbose>|1|<x>|2|<y>]`.
//!   The brackets matter: rendered as bare names the signature reads as a call
//!   to copy, and a live model duly sent the parameter names as the argument
//!   values.
//! - Empty calls: `tool_name[]` for zero-arg tools, and for a call that sends
//!   no arguments at all.
//!
//!   ## Why indices, rather than counting empty slots
//!
//!   The form used to be bare positional — `name[arg1|arg2|...]` — with skipped
//!   arguments written as empty slots (`name[||value]`). That made the *count
//!   of leading delimiters* load-bearing, and it is the single thing models get
//!   wrong most. Two failures observed on a live host:
//!
//!   - `GMAIL_LIST_THREADS[||50|<query>]` failed schema validation **12 times in
//!     one turn** before the turn was cut short.
//!   - A `GMAIL_LIST_THREADS` call wrote four leading empties where three were
//!     needed, so `query` and `user_id` each landed one slot late, in `user_id`
//!     and `verbose`. The call **ran**, with the search text as the account id.
//!
//!   Both are off-by-one on a delimiter, and both bound arguments to the wrong
//!   parameter **silently**. Indices remove the counting: a sparse call names
//!   its slots, and there is nothing to miscount. An index that is missing,
//!   non-numeric, or out of range is **rejected** rather than guessed at, so the
//!   failure mode moves from a wrong call that succeeds to a malformed call the
//!   model is told about.
//!
//!   Required-first ordering is why the natural minimal call — the one required
//!   value — is `name[0|value]` rather than an arbitrary index. An alphabetical
//!   layout put the optional parameters first for most tools, and a live model
//!   wrote `memory_recall[Colorado]` six times in one turn against
//!   `[limit|namespace|query]` and never got a tool to run.
//! - Escapes: `\|` → `|`, `\]` → `]`, `\\` → `\`. Other backslashes
//!   pass through verbatim so URLs and Windows paths remain readable.
//! - Type coercion: schema property `type: integer | number | boolean`
//!   triggers parsing the string into the matching JSON value. Failed
//!   coercion falls back to a string so the model still gets *something*
//!   useful into the tool argument.
//!
//! # Trade-offs
//!
//! - **Positional only** — nested objects or arrays can't be expressed
//!   directly. Tools that need rich payloads should either flatten their
//!   schema, accept a JSON-blob string parameter, or be invoked via the
//!   legacy JSON-in-tag fallback (which the dispatcher attempts when
//!   p-format parsing returns `None`).
//! - **Tool registry required at parse time** — without the schema we
//!   can't reconstruct named arguments. The dispatcher caches a
//!   pre-computed `name → params` map at construction time so this
//!   stays fast and avoids holding a reference to the live tool slice.

use serde_json::{Map, Value};
use std::collections::HashMap;

/// JSON-schema primitive type used for argument coercion. Anything we
/// don't recognise (objects, arrays, custom types) is treated as
/// `Other`, which preserves the raw string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PFormatParamType {
    /// A string value or a schema without a stronger primitive type.
    String,
    /// A signed integer value.
    Integer,
    /// A floating-point or general JSON number.
    Number,
    /// A boolean value.
    Boolean,
    /// A complex or unsupported schema type retained as text.
    Other,
}

impl PFormatParamType {
    /// Map a JSON-schema `type` value to the coercion enum. Schemas may
    /// expose `type` as either a single string (`"integer"`) or an
    /// array (`["integer", "null"]`); we accept both and pick the first
    /// non-`null` entry.
    #[must_use]
    pub fn from_schema_type(value: Option<&Value>) -> Self {
        let label = match value {
            Some(Value::String(s)) => s.as_str(),
            Some(Value::Array(items)) => items
                .iter()
                .find_map(|v| v.as_str().filter(|s| *s != "null"))
                .unwrap_or(""),
            _ => "",
        };
        match label {
            "string" => Self::String,
            "integer" => Self::Integer,
            "number" => Self::Number,
            "boolean" => Self::Boolean,
            _ => Self::Other,
        }
    }
}

/// One tool's positional parameter list, as the dispatcher needs it
/// at parse time.
#[derive(Debug, Clone)]
pub struct PFormatToolParams {
    /// Parameter names in declaration order.
    pub names: Vec<String>,
    /// Parallel slice of JSON types for coercion.
    pub types: Vec<PFormatParamType>,
}

impl PFormatToolParams {
    /// Pull the ordered parameter names + types out of a tool's
    /// JSON schema. Non-object schemas (rare, but possible for
    /// shell-style tools) return an empty list — the renderer falls
    /// back to `name[]`.
    ///
    /// Order is required-first, then optional alphabetically. The renderer
    /// always shows the resulting order in the tool catalogue so the model — and
    /// the parser — agree on the layout; both read it from here, so they cannot
    /// disagree. See the module-level docs for why required comes first.
    #[must_use]
    pub fn from_schema(schema: &Value) -> Self {
        let Some(props) = schema.get("properties").and_then(|p| p.as_object()) else {
            return Self {
                names: Vec::new(),
                types: Vec::new(),
            };
        };
        // Required parameters first, in the order the schema declares them, then
        // the optional ones alphabetically. Both halves are deterministic (a JSON
        // array preserves order; `Map` is a `BTreeMap` in this build), which is the
        // property the layout actually needs.
        let required: Vec<&str> = schema
            .get("required")
            .and_then(|r| r.as_array())
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();

        let mut ordered: Vec<&String> = Vec::with_capacity(props.len());
        for name in &required {
            if let Some((key, _)) = props.get_key_value(*name)
                && !ordered.contains(&key)
            {
                ordered.push(key);
            }
        }
        for key in props.keys() {
            if !ordered.contains(&key) {
                ordered.push(key);
            }
        }

        let mut names = Vec::with_capacity(ordered.len());
        let mut types = Vec::with_capacity(ordered.len());
        for key in ordered {
            names.push(key.clone());
            types.push(PFormatParamType::from_schema_type(props[key].get("type")));
        }
        Self { names, types }
    }
}

/// Pre-computed lookup of every tool's parameter list. Built once at
/// dispatcher construction time so the parser doesn't need to hold a
/// reference to the live tool list (which the host owns).
///
/// The map preserves the spec contract: the parser refuses to invent
/// argument names for an unknown tool, so an LLM can't tunnel
/// arbitrary JSON in by guessing tool names that don't exist.
pub type PFormatRegistry = HashMap<String, PFormatToolParams>;

/// Build a [`PFormatRegistry`] from `(name, schema)` pairs.
///
/// Takes schemas rather than a tool trait object on purpose: a host's tool
/// type is its own vocabulary, and requiring it here would make this module
/// depend on the very thing it exists to stay independent of. Hosts keep a
/// one-line adapter over their own tool slice.
///
/// The schema is `Borrow<Value>` rather than `&Value` so a host whose tool
/// trait *returns* a schema by value — the common shape — can map straight
/// into this without collecting into a temporary first.
#[must_use]
pub fn build_registry<I, N, S>(tools: I) -> PFormatRegistry
where
    I: IntoIterator<Item = (N, S)>,
    N: Into<String>,
    S: std::borrow::Borrow<Value>,
{
    tools
        .into_iter()
        .map(|(name, schema)| (name.into(), PFormatToolParams::from_schema(schema.borrow())))
        .collect()
}

/// Render a single tool's p-format signature, e.g. `get_weather[0|<location>|1|<unit>]`.
///
/// This signature is included in the tool catalogue within the system prompt
/// to tell the LLM exactly how to order positional arguments for a tool.
#[must_use]
pub fn render_signature(name: &str, params: &PFormatToolParams) -> String {
    if params.names.is_empty() {
        format!("{name}[]")
    } else {
        // Each slot carries its index, and the name is wrapped in angle brackets
        // so it reads as a placeholder to fill rather than a call to copy. Bare
        // names do get copied: a live model answered `memory_recall[limit|
        // namespace|query]` — the signature verbatim, the parameter names sent as
        // the argument *values*. Backticking the whole signature does not help
        // either; that made it copy the backticks instead. `<…>` marks the slot
        // without decorating the form.
        let slots: Vec<String> = params
            .names
            .iter()
            .enumerate()
            .map(|(i, n)| format!("{i}|<{n}>"))
            .collect();
        format!("{name}[{}]", slots.join("|"))
    }
}

/// Render a signature straight from a tool's JSON schema.
///
/// The schema-taking counterpart to [`render_signature`], for callers that
/// have a schema but no prebuilt [`PFormatToolParams`].
#[must_use]
pub fn render_signature_from_schema(name: &str, schema: &Value) -> String {
    render_signature(name, &PFormatToolParams::from_schema(schema))
}

/// Parse a single p-format call body and reconstruct named JSON arguments.
///
/// This function:
/// 1. Locates the positional arguments within the `[...]` brackets.
/// 2. Splits them by the `|` delimiter (respecting escapes).
/// 3. Maps each positional value to its parameter name from the tool registry.
/// 4. Performs type coercion (e.g., string to integer) based on the tool's schema.
///
/// Returns `(tool_name, args_json)` on success, or `None` if the format is invalid
/// or the tool is unknown.
#[must_use]
pub fn parse_call(body: &str, registry: &PFormatRegistry) -> Option<(String, Value)> {
    let trimmed = body.trim();

    // Locate the opening bracket. The closing bracket must be the
    // **last** character of the trimmed body — anything trailing it
    // (e.g. extra whitespace, JSON, prose) means this isn't a valid
    // p-format call and we leave it for the JSON fallback.
    let open = trimmed.find('[')?;
    if !trimmed.ends_with(']') {
        return None;
    }

    let name = trimmed[..open].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }

    let inner = &trimmed[open + 1..trimmed.len() - 1];

    // Look up the parameter spec — required so we can map positional
    // values back to named JSON keys with the correct types.
    let params = registry.get(name)?;

    let tokens = split_pipes(inner);
    // Index/value pairs, so an odd token count means the model dropped or added
    // a delimiter. Reject rather than guess: the whole point of the indices is
    // that a miscounted delimiter can no longer bind a value to the wrong
    // parameter, and silently keeping the pairs that happen to line up would put
    // that failure right back.
    if !tokens.len().is_multiple_of(2) {
        crate::telemetry::debug!(
            tool = name,
            tokens = tokens.len(),
            "[pformat] odd token count — not index/value pairs, refusing to parse"
        );
        return None;
    }

    let mut args = Map::with_capacity(tokens.len() / 2);
    // `as_chunks` rather than `chunks_exact`: the length is already known even,
    // so the remainder is empty by construction and the pair is a fixed-size
    // array the compiler can index without a bounds check.
    let (pairs, _empty_remainder) = tokens.as_chunks::<2>();
    for [raw_index, raw] in pairs {
        let raw_index = raw_index.trim();
        let Ok(slot) = raw_index.parse::<usize>() else {
            // A non-numeric index is a call in the old bare-positional form (or
            // simply malformed). Refusing is deliberate: parsing it positionally
            // would silently resurrect the off-by-one this format exists to end.
            crate::telemetry::debug!(
                tool = name,
                index = raw_index,
                "[pformat] slot index is not a number — refusing to parse"
            );
            return None;
        };
        let Some(param_name) = params.names.get(slot) else {
            crate::telemetry::debug!(
                tool = name,
                slot,
                slots = params.names.len(),
                "[pformat] slot index out of range — refusing to parse"
            );
            return None;
        };
        // An empty value is an argument the model did not send, so the key is
        // left out entirely rather than set to `""`. Inserting `""` makes every
        // non-string parameter fail schema validation — a typed `max_results`
        // arriving as `""` means the tool never runs, and the error names a field
        // the model deliberately left blank, which it cannot satisfy.
        if raw.trim().is_empty() {
            crate::telemetry::debug!(
                tool = name,
                slot,
                param = param_name.as_str(),
                "[pformat] empty value for a named slot — argument omitted"
            );
            // `remove`, not `continue`. A repeated slot takes its *last* value,
            // and "empty" is a value — the model saying it is not sending this
            // one. Skipping would leave an earlier `[0|London|0|]` bound to
            // `London`, which is the last-write rule silently not applying to
            // the one case where the last write is a retraction.
            args.remove(param_name.as_str());
            continue;
        }
        let coerced = coerce_value(
            raw,
            params
                .types
                .get(slot)
                .copied()
                .unwrap_or(PFormatParamType::String),
        );
        // Last write wins on a repeated slot. Rare enough not to be worth
        // rejecting the whole call over, and the later value is the model's
        // latest intent.
        args.insert(param_name.clone(), coerced);
    }

    Some((name.to_string(), Value::Object(args)))
}

/// Split a p-format argument body on unescaped `|`. Honours `\|`,
/// `\]`, and `\\` escapes. An empty body produces an empty `Vec` (NOT
/// `vec![""]`) so a tool with zero parameters parses cleanly.
fn split_pipes(input: &str) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('|') => {
                    current.push('|');
                    chars.next();
                }
                Some(']') => {
                    current.push(']');
                    chars.next();
                }
                Some('\\') => {
                    current.push('\\');
                    chars.next();
                }
                _ => current.push('\\'),
            }
        } else if c == '|' {
            out.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }

    out.push(current);
    out
}

/// Coerce a raw string argument into the JSON type the schema expects.
/// Falls back to `Value::String` for any failed coercion so the model
/// still gets a usable value into the tool argument map.
fn coerce_value(raw: &str, ty: PFormatParamType) -> Value {
    match ty {
        PFormatParamType::Integer => raw.trim().parse::<i64>().map_or_else(
            |_| Value::String(raw.to_string()),
            |n| Value::Number(n.into()),
        ),
        PFormatParamType::Number => raw
            .trim()
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map_or_else(|| Value::String(raw.to_string()), Value::Number),
        PFormatParamType::Boolean => match raw.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" => Value::Bool(true),
            "false" | "no" | "0" => Value::Bool(false),
            _ => Value::String(raw.to_string()),
        },
        PFormatParamType::String | PFormatParamType::Other => Value::String(raw.to_string()),
    }
}

// ──────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "pformat_test.rs"]
mod tests;
