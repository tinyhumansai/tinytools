//! Unit tests for `ToolResult` and `ToolContent`: constructing successes
//! and errors, rendering content for a model, and the JSON wire shape.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::{ToolContent, ToolResult};

#[test]
fn success_carries_one_text_block() {
    let r = ToolResult::success("done");
    assert!(!r.is_error);
    assert_eq!(r.text(), "done");
    assert_eq!(r.output(), "done");
}

#[test]
fn error_sets_the_flag_and_keeps_the_message() {
    let r = ToolResult::error("failed");
    assert!(r.is_error);
    assert_eq!(r.text(), "failed");
}

#[test]
fn text_skips_json_blocks_but_output_renders_them() {
    let r = ToolResult::json(json!({"key": "value"}));
    assert!(!r.is_error);
    assert!(r.text().is_empty());
    assert!(r.output().contains("key"));
}

#[test]
fn mixed_content_joins_in_order() {
    let r = ToolResult {
        content: vec![
            ToolContent::Text {
                text: "line1".into(),
            },
            ToolContent::Json {
                data: json!({"a": 1}),
            },
            ToolContent::Text {
                text: "line2".into(),
            },
        ],
        is_error: false,
        markdown_formatted: None,
    };
    assert_eq!(r.text(), "line1\nline2");
    let output = r.output();
    assert!(output.contains("line1"));
    assert!(output.contains("line2"));
    assert!(output.contains("\"a\""));
}

#[test]
fn empty_content_renders_empty() {
    let r = ToolResult {
        content: vec![],
        is_error: false,
        markdown_formatted: None,
    };
    assert!(r.text().is_empty());
    assert!(r.output().is_empty());
}

#[test]
fn result_round_trips_through_json() {
    let r = ToolResult::success("hello");
    let encoded = serde_json::to_string(&r).expect("serializable");
    let back: ToolResult = serde_json::from_str(&encoded).expect("deserializable");
    assert!(!back.is_error);
    assert_eq!(back.text(), "hello");
}

#[test]
fn result_is_pinned_to_its_literal_wire_shape() {
    // Same reasoning as the permission and spec pinning tests: a round-trip
    // alone doesn't catch a silent field rename, since the same serializer and
    // deserializer that changed still agree with each other. Assert the exact
    // JSON a persisted transcript or RPC reply would carry, in both
    // directions.
    let r = ToolResult::success_with_markdown(json!({"a": 1}), "**a**: 1");
    let encoded: serde_json::Value = serde_json::to_value(&r).expect("serializable");
    assert_eq!(
        encoded,
        json!({
            "content": [{ "type": "json", "data": { "a": 1 } }],
            "is_error": false,
            "markdownFormatted": "**a**: 1",
        })
    );

    let literal = r#"{"content":[{"type":"text","text":"hi"}],"is_error":true}"#;
    let decoded: ToolResult = serde_json::from_str(literal).expect("deserializable");
    assert!(decoded.is_error);
    assert_eq!(decoded.text(), "hi");
    assert_eq!(decoded.markdown_formatted, None);
}

#[test]
fn content_blocks_are_tagged_by_type() {
    let text = serde_json::to_string(&ToolContent::Text {
        text: "test".into(),
    })
    .expect("serializable");
    assert!(text.contains("\"type\":\"text\""));

    let data = serde_json::to_string(&ToolContent::Json {
        data: json!({"x": 1}),
    })
    .expect("serializable");
    assert!(data.contains("\"type\":\"json\""));

    match serde_json::from_str::<ToolContent>(&text).expect("deserializable") {
        ToolContent::Text { text } => assert_eq!(text, "test"),
        ToolContent::Json { .. } => unreachable!("tagged as text"),
    }
    match serde_json::from_str::<ToolContent>(&data).expect("deserializable") {
        ToolContent::Json { data } => assert_eq!(data["x"], 1),
        ToolContent::Text { .. } => unreachable!("tagged as json"),
    }
}

#[test]
fn output_for_llm_prefers_markdown_when_requested() {
    let r = ToolResult::success_with_markdown(json!({"items": [{"id": 1}, {"id": 2}]}), "- 1\n- 2");
    assert_eq!(r.output_for_llm(true), "- 1\n- 2");
    assert!(r.output_for_llm(false).contains("\"items\""));
}

#[test]
fn output_for_llm_falls_back_when_markdown_is_absent_or_blank() {
    let plain = ToolResult::success("plain");
    assert_eq!(plain.output_for_llm(true), "plain");
    assert_eq!(plain.output_for_llm(false), "plain");

    // A tool that set the field but rendered nothing is a bug in the tool;
    // sending the model an empty turn would hide it.
    let blank = ToolResult::success("plain").with_markdown("   \n  ");
    assert_eq!(blank.output_for_llm(true), "plain");
}

#[test]
fn the_markdown_field_keeps_its_composio_wire_name() {
    let r = ToolResult::success_with_markdown(json!({"a": 1}), "**a**: 1");
    let encoded = serde_json::to_string(&r).expect("serializable");
    assert!(encoded.contains("markdownFormatted"));
    let back: ToolResult = serde_json::from_str(&encoded).expect("deserializable");
    assert_eq!(back.markdown_formatted.as_deref(), Some("**a**: 1"));
}
