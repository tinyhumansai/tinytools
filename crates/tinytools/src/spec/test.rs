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
