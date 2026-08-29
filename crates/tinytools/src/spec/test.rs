//! Unit tests for `ToolSpec`: that it round-trips through JSON.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::ToolSpec;

#[test]
fn spec_round_trips_through_json() {
    let spec = ToolSpec {
        name: "echo".into(),
        description: "Returns its input.".into(),
        parameters: serde_json::json!({ "type": "object" }),
    };
    let encoded = serde_json::to_string(&spec).expect("serializable");
    let back: ToolSpec = serde_json::from_str(&encoded).expect("deserializable");
    assert_eq!(back.name, "echo");
    assert_eq!(back.description, "Returns its input.");
    assert_eq!(back.parameters["type"], "object");
}

#[test]
fn spec_is_pinned_to_its_literal_wire_shape() {
    // A round-trip alone only proves the encoder and decoder agree with each
    // other; it still passes if a field is silently renamed. Pin the exact
    // field names a persisted transcript or RPC payload would carry, and also
    // decode a fixed literal, so a rename is caught in both directions.
    let spec = ToolSpec {
        name: "echo".into(),
        description: "Returns its input.".into(),
        parameters: serde_json::json!({ "type": "object" }),
    };
    let encoded: serde_json::Value = serde_json::to_value(&spec).expect("serializable");
    assert_eq!(
        encoded,
        serde_json::json!({
            "name": "echo",
            "description": "Returns its input.",
            "parameters": { "type": "object" },
        })
    );

    let literal =
        r#"{"name":"echo","description":"Returns its input.","parameters":{"type":"object"}}"#;
    let decoded: ToolSpec = serde_json::from_str(literal).expect("deserializable");
    assert_eq!(decoded.name, "echo");
    assert_eq!(decoded.description, "Returns its input.");
    assert_eq!(decoded.parameters, serde_json::json!({ "type": "object" }));
}
