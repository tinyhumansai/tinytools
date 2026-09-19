//! Argument shape repair.

use crate::repair::args::{
    accepts_object, coerce_to_schema, decode, from_call_object, unwrap_envelope,
};
use serde_json::json;

fn city_schema() -> serde_json::Value {
    json!({ "type": "object", "properties": { "city": { "type": "string" } }, "required": ["city"] })
}

fn valid_city(value: &serde_json::Value) -> bool {
    value.get("city").is_some_and(serde_json::Value::is_string)
}

#[test]
fn decode_handles_strings_fences_and_relaxed_json() {
    assert_eq!(decode(Some(&json!("{\"a\":1}"))), json!({ "a": 1 }));
    assert_eq!(
        decode(Some(&json!("```json\n{\"a\":1}\n```"))),
        json!({ "a": 1 })
    );
    assert_eq!(decode(Some(&json!("{a:1}"))), json!({ "a": 1 }));
    assert_eq!(decode(Some(&json!("garbage"))), json!({}));
    assert_eq!(decode(None), json!({}));
}

#[test]
fn argument_key_aliases_are_read_in_priority_order() {
    assert_eq!(
        from_call_object(&json!({ "args": { "x": 1 } })),
        json!({ "x": 1 })
    );
    assert_eq!(
        from_call_object(&json!({ "arguments": { "x": 1 }, "input": { "x": 2 } })),
        json!({ "x": 1 })
    );
}

#[test]
fn envelopes_are_unwrapped_only_when_the_inner_value_validates() {
    let schema = city_schema();
    for wrapped in [
        json!({ "type": "object", "required": ["city"], "properties": { "city": "Paris" } }),
        json!({ "properties": {}, "required": [], "arguments": { "city": "Paris" } }),
        json!({ "param": { "city": "Paris" } }),
    ] {
        assert_eq!(
            unwrap_envelope(&wrapped, &schema, &valid_city),
            Some(json!({ "city": "Paris" })),
            "{wrapped}"
        );
    }
    assert_eq!(
        unwrap_envelope(
            &json!({ "param": { "town": "Paris" } }),
            &schema,
            &valid_city
        ),
        None
    );
}

#[test]
fn a_declared_parameter_is_never_treated_as_an_envelope() {
    let schema = json!({ "type": "object", "properties": { "input": { "type": "object" } } });
    let arguments = json!({ "input": { "city": "Paris" } });
    assert_eq!(unwrap_envelope(&arguments, &schema, &valid_city), None);
}

#[test]
fn accepts_object_reads_every_schema_spelling() {
    assert!(accepts_object(&json!({ "type": "object" })));
    assert!(accepts_object(&json!({ "type": ["object", "null"] })));
    assert!(accepts_object(&json!({ "properties": {} })));
    assert!(!accepts_object(&json!({ "type": "string" })));
}

#[test]
fn scalars_are_coerced_to_the_declared_type() {
    let schema = json!({
        "type": "object",
        "properties": {
            "n": { "type": "integer" },
            "f": { "type": "number" },
            "b": { "type": "boolean" },
            "list": { "type": "array", "items": { "type": "string" } },
            "nested": { "type": "object", "properties": { "k": { "type": "integer" } } },
            "s": { "type": "string" }
        }
    });
    let out = coerce_to_schema(
        json!({ "n": "42", "f": "3.5", "b": "true", "list": "[\"a\",\"b\"]", "nested": "{\"k\":\"7\"}", "s": 5, "extra": "x" }),
        &schema,
    );
    assert_eq!(
        out,
        json!({ "n": 42, "f": 3.5, "b": true, "list": ["a", "b"], "nested": { "k": 7 }, "s": "5", "extra": "x" })
    );
}

#[test]
fn unconvertible_scalars_are_left_for_the_validator() {
    let schema = json!({ "type": "object", "properties": { "n": { "type": "integer" }, "l": { "type": "array" } } });
    assert_eq!(
        coerce_to_schema(json!({ "n": "many" }), &schema),
        json!({ "n": "many" })
    );
    assert_eq!(
        coerce_to_schema(json!({ "l": "solo" }), &schema),
        json!({ "l": ["solo"] })
    );
    assert_eq!(
        coerce_to_schema(json!({ "l": 3 }), &schema),
        json!({ "l": [3] })
    );
}
