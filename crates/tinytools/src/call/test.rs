//! Unit tests for `ToolCallOptions` and `ToolTimeout`: their defaults and
//! that each variant stays distinct.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    InjectedToolArguments, ToolArgumentPreparationError, ToolCall, ToolCallId, ToolCallOptions,
    ToolInjectedArgument, ToolTimeout, prepare_tool_arguments, project_injected_arguments,
};

#[test]
fn options_default_to_no_preference() {
    let options = ToolCallOptions::default();
    assert!(!options.prefer_markdown);
    assert!(ToolCallOptions::prefer_markdown().prefer_markdown);
}

#[test]
fn timeout_defaults_to_inherit() {
    assert_eq!(ToolTimeout::default(), ToolTimeout::Inherit);
    assert!(ToolTimeout::Inherit.is_inherit());
    assert!(!ToolTimeout::Unbounded.is_inherit());
    assert!(!ToolTimeout::Millis(30).is_inherit());
}

#[test]
fn timeout_variants_are_distinct() {
    assert_ne!(ToolTimeout::Inherit, ToolTimeout::Unbounded);
    assert_ne!(ToolTimeout::Millis(1), ToolTimeout::Millis(2));
}

#[test]
fn timeout_variants_have_pinned_json_wire_shapes() {
    let cases = [
        (
            ToolTimeout::Inherit,
            serde_json::json!({ "mode": "inherit" }),
        ),
        (
            ToolTimeout::Unbounded,
            serde_json::json!({ "mode": "unbounded" }),
        ),
        (
            ToolTimeout::Millis(250),
            serde_json::json!({ "mode": "millis", "timeout_ms": 250 }),
        ),
    ];

    for (timeout, wire) in cases {
        assert_eq!(serde_json::to_value(timeout).expect("serializable"), wire);
        assert_eq!(
            serde_json::from_value::<ToolTimeout>(wire).expect("deserializable"),
            timeout
        );
    }
}

#[test]
fn tool_call_identity_has_a_pinned_json_wire_shape() {
    let call = ToolCall::new(
        ToolCallId::new("call-42"),
        "write_file",
        serde_json::json!({ "path": "notes.md" }),
    );
    let wire = serde_json::json!({
        "id": "call-42",
        "name": "write_file",
        "arguments": { "path": "notes.md" },
    });

    assert_eq!(serde_json::to_value(&call).expect("serializable"), wire);
    assert_eq!(
        serde_json::from_value::<ToolCall>(wire).expect("deserializable"),
        call
    );
    assert_eq!(call.id.as_str(), "call-42");
}

#[test]
fn injected_argument_declarations_have_pinned_json_wire_shapes() {
    let declarations = vec![
        ToolInjectedArgument::host("account_id"),
        ToolInjectedArgument::tool_call_id("call_id"),
    ];
    let wire = serde_json::json!([
        { "name": "account_id", "source": "host" },
        { "name": "call_id", "source": "tool_call_id" },
    ]);

    assert_eq!(
        serde_json::to_value(&declarations).expect("serializable"),
        wire
    );
    assert_eq!(
        serde_json::from_value::<Vec<ToolInjectedArgument>>(wire).expect("deserializable"),
        declarations
    );
}

#[test]
fn preparation_strips_model_values_then_injects_host_values_before_validation() {
    let call = ToolCall::new(
        ToolCallId::new("trusted-call"),
        "send_message",
        serde_json::json!({
            "message": "hello",
            "account_id": "forged-account",
            "call_id": "forged-call",
        }),
    );
    let declarations = [
        ToolInjectedArgument::host("account_id"),
        ToolInjectedArgument::tool_call_id("call_id"),
    ];
    let mut host_values = InjectedToolArguments::new();
    host_values.insert("account_id", serde_json::json!("trusted-account"));
    host_values.insert(
        "message",
        serde_json::json!("must-not-overwrite-model-input"),
    );

    let prepared = prepare_tool_arguments(&call, &declarations, &host_values)
        .expect("the host supplied every declared value");

    // This is the value a harness passes to its schema validator. The model's
    // forged injected values are gone before validation, while ordinary model
    // arguments stay model-owned and host values for undeclared keys are ignored.
    assert_eq!(
        prepared,
        serde_json::json!({
            "message": "hello",
            "account_id": "trusted-account",
            "call_id": "trusted-call",
        })
    );
}

#[test]
fn projection_hides_injected_keys_from_properties_and_required_schema_keys() {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "message": { "type": "string" },
            "account_id": { "type": "string" },
            "call_id": { "type": "string" },
        },
        "required": ["message", "account_id", "call_id"],
    });
    let declarations = [
        ToolInjectedArgument::host("account_id"),
        ToolInjectedArgument::tool_call_id("call_id"),
    ];

    let projected = project_injected_arguments(&schema, &declarations);

    assert_eq!(
        projected["properties"],
        serde_json::json!({ "message": { "type": "string" } })
    );
    assert_eq!(projected["required"], serde_json::json!(["message"]));
    assert_eq!(
        schema["required"],
        serde_json::json!(["message", "account_id", "call_id"])
    );
}

#[test]
fn preparation_rejects_missing_host_values_duplicate_declarations_and_non_objects() {
    let call = ToolCall::new(ToolCallId::new("c1"), "lookup", serde_json::json!({}));
    let host = InjectedToolArguments::new();
    let missing = prepare_tool_arguments(&call, &[ToolInjectedArgument::host("account_id")], &host)
        .expect_err("a host-owned declaration needs a host value");
    assert_eq!(
        missing,
        ToolArgumentPreparationError::MissingHostValue {
            name: "account_id".into()
        }
    );

    let duplicate = prepare_tool_arguments(
        &call,
        &[
            ToolInjectedArgument::host("account_id"),
            ToolInjectedArgument::tool_call_id("account_id"),
        ],
        &host,
    )
    .expect_err("one argument cannot have two authoritative sources");
    assert_eq!(
        duplicate,
        ToolArgumentPreparationError::DuplicateDeclaration {
            name: "account_id".into()
        }
    );

    let non_object = ToolCall::new(ToolCallId::new("c2"), "lookup", serde_json::json!(["bad"]));
    let error = prepare_tool_arguments(&non_object, &[], &host)
        .expect_err("schema arguments must begin as an object");
    assert_eq!(error, ToolArgumentPreparationError::ArgumentsMustBeObject);
}

#[test]
fn preparation_errors_describe_each_invalid_input() {
    let cases = [
        (
            ToolArgumentPreparationError::ArgumentsMustBeObject,
            "tool arguments must be a JSON object",
        ),
        (
            ToolArgumentPreparationError::DuplicateDeclaration {
                name: "account_id".into(),
            },
            "duplicate injected argument declaration: account_id",
        ),
        (
            ToolArgumentPreparationError::MissingHostValue {
                name: "account_id".into(),
            },
            "missing host value for injected argument: account_id",
        ),
    ];

    for (error, message) in cases {
        assert_eq!(error.to_string(), message);
    }
}

#[test]
fn projection_leaves_non_object_schema_members_unchanged() {
    let declarations = [ToolInjectedArgument::host("account_id")];
    let scalar = serde_json::json!("not an object schema");
    assert_eq!(project_injected_arguments(&scalar, &declarations), scalar);

    let malformed_members = serde_json::json!({
        "properties": "not an object",
        "required": "not an array",
        "title": "preserved",
    });
    assert_eq!(
        project_injected_arguments(&malformed_members, &declarations),
        malformed_members
    );
}
