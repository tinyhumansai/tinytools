//! Bringing a model's argument value into the shape the tool's schema wants.
//!
//! Three families of defect, each a real capture from a local model:
//!
//! * the arguments arrive as a JSON **string** (sometimes fenced) instead of
//!   an object — `"{\"city\":\"Paris\"}"`;
//! * the real object is buried one level down inside an envelope the model
//!   invented — `{"properties":{"city":"Paris"}}` (a schema echo),
//!   `{"param":{"city":"Paris"}}`;
//! * scalars are the wrong JSON type for the schema — `"42"` for an integer,
//!   `"true"` for a boolean, `"[1,2]"` for an array.
//!
//! Everything here is pure and schema-driven. The host still decides whether
//! to *run* a repaired call; this module only makes the repair possible.

use serde_json::{Map, Value};

/// Object keys that may carry the tool **arguments** inside a call object,
/// in priority order. `arguments` is canonical; the rest are what a model
/// drifts to when it copies the schema vocabulary.
pub const ARGUMENT_KEYS: &[&str] = &["arguments", "args", "parameters", "params", "input"];

/// Keys under which a model buries the real arguments object one level down.
/// `properties` is the JSON-Schema echo; the rest are invented wrappers.
pub const WRAPPER_KEYS: &[&str] = &[
    "properties",
    "arguments",
    "args",
    "parameters",
    "params",
    "param",
    "input",
];

/// Decodes an argument value that may be a stringified (and possibly fenced)
/// JSON document. Non-string values are cloned; an undecodable string becomes
/// an empty object; a missing value is an empty object.
#[must_use]
pub fn decode(raw: Option<&Value>) -> Value {
    match raw {
        Some(Value::String(s)) => {
            let candidate = super::json::strip_code_fence(s);
            serde_json::from_str::<Value>(candidate)
                .ok()
                .or_else(|| super::json::recover_object(candidate))
                .unwrap_or_else(|| Value::Object(Map::new()))
        }
        Some(value) => value.clone(),
        None => Value::Object(Map::new()),
    }
}

/// The arguments under the first present [`ARGUMENT_KEYS`] entry of a call
/// object, decoded. Empty object when none is present.
#[must_use]
pub fn from_call_object(call: &Value) -> Value {
    for key in ARGUMENT_KEYS {
        if let Some(value) = call.get(*key) {
            return decode(Some(value));
        }
    }
    decode(None)
}

/// Recovers arguments a model buried one level deep inside an envelope.
///
/// For each [`WRAPPER_KEYS`] entry the rewrite applies only when the tool does
/// not itself declare a parameter of that name (for such a tool the key is
/// data, not an envelope) and when `is_valid` accepts the unwrapped value.
/// Returns the unwrapped value, or `None` when no candidate satisfies both —
/// the caller keeps the original so the model sees a precise validation error
/// rather than a rewritten one.
#[must_use]
pub fn unwrap_envelope(
    arguments: &Value,
    schema: &Value,
    is_valid: &dyn Fn(&Value) -> bool,
) -> Option<Value> {
    let declared = schema.get("properties").and_then(Value::as_object);
    for key in WRAPPER_KEYS {
        if declared.is_some_and(|declared| declared.contains_key(*key)) {
            continue;
        }
        let Some(inner) = arguments.get(*key).filter(|inner| inner.is_object()) else {
            continue;
        };
        if is_valid(inner) {
            return Some(inner.clone());
        }
    }
    None
}

/// Whether `schema` can accept an object at all.
#[must_use]
pub fn accepts_object(schema: &Value) -> bool {
    schema.get("type").is_some_and(|kind| {
        kind.as_str() == Some("object")
            || kind
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_str() == Some("object")))
    }) || schema.get("properties").is_some()
        || schema.get("required").is_some()
        || schema
            .get("enum")
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().any(Value::is_object))
}

/// Coerces string scalars to the primitive type each schema property declares.
///
/// `"42"` → `42` for `integer`, `"3.5"` → `3.5` for `number`, `"true"` →
/// `true` for `boolean`, a JSON-encoded string → the decoded value for `array`
/// / `object`, and a bare scalar → `[scalar]` for `array`. A value that does
/// not convert is left as it was, so the schema validator still reports it.
/// Recurses into nested objects and array items with their own schemas.
#[must_use]
pub fn coerce_to_schema(arguments: Value, schema: &Value) -> Value {
    let Value::Object(map) = arguments else {
        return arguments;
    };
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return Value::Object(map);
    };
    let mut out = Map::with_capacity(map.len());
    for (key, value) in map {
        let coerced = match properties.get(&key) {
            Some(property) => coerce_value(value, property),
            None => value,
        };
        out.insert(key, coerced);
    }
    Value::Object(out)
}

fn schema_type(schema: &Value) -> Option<&str> {
    match schema.get("type") {
        Some(Value::String(s)) => Some(s.as_str()),
        Some(Value::Array(items)) => items
            .iter()
            .find_map(|v| v.as_str().filter(|s| *s != "null")),
        _ => None,
    }
}

fn coerce_value(value: Value, schema: &Value) -> Value {
    match (schema_type(schema), value) {
        (Some("integer"), Value::String(s)) => s
            .trim()
            .parse::<i64>()
            .map_or_else(|_| Value::String(s), Value::from),
        (Some("number"), Value::String(s)) => s
            .trim()
            .parse::<f64>()
            .ok()
            .and_then(|n| serde_json::Number::from_f64(n).map(Value::Number))
            .unwrap_or(Value::String(s)),
        (Some("boolean"), Value::String(s)) => match s.trim() {
            "true" | "True" | "TRUE" => Value::Bool(true),
            "false" | "False" | "FALSE" => Value::Bool(false),
            _ => Value::String(s),
        },
        (Some("array"), Value::String(s)) => match serde_json::from_str::<Value>(s.trim()) {
            Ok(Value::Array(items)) => coerce_items(items, schema),
            Ok(other) => Value::Array(vec![other]),
            Err(_) => Value::Array(vec![Value::String(s)]),
        },
        (Some("array"), Value::Array(items)) => coerce_items(items, schema),
        (Some("array"), scalar @ (Value::Number(_) | Value::Bool(_))) => Value::Array(vec![scalar]),
        (Some("object"), Value::String(s)) => match serde_json::from_str::<Value>(s.trim()) {
            Ok(object @ Value::Object(_)) => coerce_to_schema(object, schema),
            _ => Value::String(s),
        },
        (Some("object"), object @ Value::Object(_)) => coerce_to_schema(object, schema),
        (Some("string"), Value::Number(n)) => Value::String(n.to_string()),
        (Some("null"), Value::String(s)) if s.trim() == "null" => Value::Null,
        (_, value) => value,
    }
}

fn coerce_items(items: Vec<Value>, schema: &Value) -> Value {
    let Some(item_schema) = schema.get("items") else {
        return Value::Array(items);
    };
    Value::Array(
        items
            .into_iter()
            .map(|item| {
                if let Value::String(s) = &item
                    && let Ok(decoded) = serde_json::from_str::<Value>(s)
                    && schema_type(item_schema).is_some_and(|t| t == "object" || t == "array")
                {
                    return coerce_value(decoded, item_schema);
                }
                coerce_value(item, item_schema)
            })
            .collect(),
    )
}
