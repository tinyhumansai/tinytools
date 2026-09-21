//! A tool's JSON schema rendered as a function signature.
//!
//! ```text
//! def read_file(path: str, limit: int = None) -> str
//! function read_file(path: string, limit?: number): string;
//! ```
//!
//! Parameter order is [`PFormatToolParams::from_schema`]'s — required first
//! in schema order, then optional alphabetically — which is also the order
//! [`super::parse_calls`] binds positional arguments in. The two read the
//! same function, so they cannot disagree.
//!
//! The type rendering is lossy on purpose: constraints (`minimum`,
//! `pattern`), nested descriptions, and anything past [`MAX_DEPTH`] are
//! dropped. The native path never uses it; a prompt-guided model only needs
//! the shape.

use std::fmt::Write as _;

use serde_json::Value;

use super::CodeStyle;
use crate::pformat::PFormatToolParams;

/// Nesting past this depth renders as a bare `dict` / `object`.
pub(crate) const MAX_DEPTH: usize = 4;
/// Properties past this count in one nested object render as `…`.
pub(crate) const MAX_PROPERTIES: usize = 16;

/// Renders `name` with `schema`'s parameters as a signature in `style`.
///
/// Zero-parameter tools render as `def name() -> str` / `function name(): string;`.
#[must_use]
pub fn render_code_signature(name: &str, schema: &Value, style: CodeStyle) -> String {
    let params = PFormatToolParams::from_schema(schema);
    let properties = schema.get("properties").and_then(Value::as_object);
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let mut out = String::new();
    match style {
        CodeStyle::Python => out.push_str("def "),
        CodeStyle::TypeScript => out.push_str("function "),
    }
    out.push_str(name);
    out.push('(');
    for (index, param) in params.names.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        let property = properties
            .and_then(|props| props.get(param))
            .unwrap_or(&Value::Null);
        let ty = render_code_type(property, style);
        let is_required = required.contains(&param.as_str());
        // Infallible: writing to a String never errors.
        let _ = match (style, is_required) {
            (CodeStyle::Python, true) => write!(out, "{param}: {ty}"),
            (CodeStyle::Python, false) => write!(out, "{param}: {ty} = None"),
            (CodeStyle::TypeScript, true) => write!(out, "{}: {ty}", ts_name(param)),
            (CodeStyle::TypeScript, false) => write!(out, "{}?: {ty}", ts_name(param)),
        };
    }
    match style {
        CodeStyle::Python => out.push_str(") -> str"),
        CodeStyle::TypeScript => out.push_str("): string;"),
    }
    out
}

/// Renders one JSON-Schema value as a type in `style`.
#[must_use]
pub fn render_code_type(schema: &Value, style: CodeStyle) -> String {
    render(schema, style, 0)
}

fn render(schema: &Value, style: CodeStyle, depth: usize) -> String {
    let unknown = || match style {
        CodeStyle::Python => "Any".to_string(),
        CodeStyle::TypeScript => "unknown".to_string(),
    };
    let Some(object) = schema.as_object() else {
        return unknown();
    };
    if let Some(values) = object.get("enum").and_then(Value::as_array) {
        let members: Vec<String> = values.iter().map(literal).collect();
        return match style {
            CodeStyle::Python => format!("Literal[{}]", members.join(", ")),
            CodeStyle::TypeScript => members.join(" | "),
        };
    }
    if let Some(value) = object.get("const") {
        return match style {
            CodeStyle::Python => format!("Literal[{}]", literal(value)),
            CodeStyle::TypeScript => literal(value),
        };
    }
    for key in ["anyOf", "oneOf"] {
        if let Some(variants) = object.get(key).and_then(Value::as_array)
            && !variants.is_empty()
        {
            let mut seen = Vec::new();
            for variant in variants {
                let rendered = render(variant, style, depth);
                if !seen.contains(&rendered) {
                    seen.push(rendered);
                }
            }
            return seen.join(" | ");
        }
    }
    if let Some(variants) = object.get("allOf").and_then(Value::as_array)
        && let Some(first) = variants.first()
    {
        return render(first, style, depth);
    }

    let kind = match object.get("type") {
        Some(Value::String(kind)) => kind.clone(),
        Some(Value::Array(kinds)) => {
            return kinds
                .iter()
                .filter_map(Value::as_str)
                .map(|kind| {
                    let mut single = object.clone();
                    single.insert("type".to_string(), Value::String(kind.to_string()));
                    render(&Value::Object(single), style, depth)
                })
                .collect::<Vec<_>>()
                .join(" | ");
        }
        _ if object.contains_key("properties") => "object".to_string(),
        _ if object.contains_key("items") => "array".to_string(),
        _ => return unknown(),
    };

    match (kind.as_str(), style) {
        ("string", CodeStyle::Python) => "str".to_string(),
        ("string", CodeStyle::TypeScript) => "string".to_string(),
        ("integer", CodeStyle::Python) => "int".to_string(),
        ("number", CodeStyle::Python) => "float".to_string(),
        ("integer" | "number", CodeStyle::TypeScript) => "number".to_string(),
        ("boolean", CodeStyle::Python) => "bool".to_string(),
        ("boolean", CodeStyle::TypeScript) => "boolean".to_string(),
        ("null", CodeStyle::Python) => "None".to_string(),
        ("null", CodeStyle::TypeScript) => "null".to_string(),
        ("object", _) => render_object(object, style, depth),
        ("array", _) => {
            let items = object.get("items").map_or_else(unknown, |items| {
                if depth >= MAX_DEPTH {
                    unknown()
                } else {
                    render(items, style, depth + 1)
                }
            });
            match style {
                CodeStyle::Python => format!("list[{items}]"),
                CodeStyle::TypeScript if items.contains(' ') || items.contains('|') => {
                    format!("Array<{items}>")
                }
                CodeStyle::TypeScript => format!("{items}[]"),
            }
        }
        _ => unknown(),
    }
}

fn render_object(
    object: &serde_json::Map<String, Value>,
    style: CodeStyle,
    depth: usize,
) -> String {
    let bare = || match style {
        CodeStyle::Python => "dict".to_string(),
        CodeStyle::TypeScript => "object".to_string(),
    };
    let Some(properties) = object.get("properties").and_then(Value::as_object) else {
        return bare();
    };
    if properties.is_empty() || depth >= MAX_DEPTH {
        return bare();
    }
    // Python has no inline object type worth the tokens; `dict` plus the
    // key names is what a model needs to write the literal.
    if style == CodeStyle::Python {
        return bare();
    }
    let required: Vec<&str> = object
        .get("required")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut out = String::from("{");
    for (index, (name, property)) in properties.iter().enumerate() {
        if index == MAX_PROPERTIES {
            out.push_str(", …");
            break;
        }
        if index > 0 {
            out.push_str(", ");
        }
        let optional = if required.contains(&name.as_str()) {
            ""
        } else {
            "?"
        };
        // Infallible: writing to a String never errors.
        let _ = write!(
            out,
            "{}{optional}: {}",
            ts_name(name),
            render(property, style, depth + 1)
        );
    }
    out.push('}');
    out
}

/// A JSON value spelled as a type-level literal (`"a"`, `1`, `true`).
fn literal(value: &Value) -> String {
    match value {
        Value::String(s) => format!("{s:?}"),
        other => other.to_string(),
    }
}

/// A property name as a TypeScript member: bare when it is an identifier,
/// JSON-quoted otherwise (`{"file-path": string}`).
fn ts_name(name: &str) -> String {
    let is_identifier = name
        .chars()
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_' || first == '$')
        && name
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '$');
    if is_identifier {
        name.to_string()
    } else {
        serde_json::to_string(name).unwrap_or_else(|_| format!("{name:?}"))
    }
}
