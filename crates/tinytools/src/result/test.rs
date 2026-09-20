//! Unit tests for `ToolResult` and `ToolContent`: constructing successes
//! and errors, rendering content for a model, and the JSON wire shape.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::{FileData, ImageData, ToolContent, ToolControl, ToolErrorKind, ToolResult};

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
        ..ToolResult::default()
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
        ..ToolResult::default()
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
        other => unreachable!("tagged as text, got {other:?}"),
    }
    match serde_json::from_str::<ToolContent>(&data).expect("deserializable") {
        ToolContent::Json { data } => assert_eq!(data["x"], 1),
        other => unreachable!("tagged as json, got {other:?}"),
    }
}

#[test]
fn image_block_round_trips_through_json() {
    let block = ToolContent::Image {
        media_type: "image/png".into(),
        data: ImageData::Base64("aGVsbG8=".into()),
    };
    let encoded = serde_json::to_value(&block).expect("serializable");
    assert_eq!(
        encoded,
        json!({
            "type": "image",
            "media_type": "image/png",
            "data": { "kind": "base64", "value": "aGVsbG8=" },
        })
    );
    let decoded: ToolContent = serde_json::from_value(encoded).expect("deserializable");
    match decoded {
        ToolContent::Image { media_type, data } => {
            assert_eq!(media_type, "image/png");
            assert!(matches!(data, ImageData::Base64(b) if b == "aGVsbG8="));
        }
        other => unreachable!("tagged as image, got {other:?}"),
    }
}

#[test]
fn image_block_supports_url_data() {
    let block = ToolContent::Image {
        media_type: "image/jpeg".into(),
        data: ImageData::Url("https://example.com/a.jpg".into()),
    };
    let encoded = serde_json::to_string(&block).expect("serializable");
    let decoded: ToolContent = serde_json::from_str(&encoded).expect("deserializable");
    match decoded {
        ToolContent::Image { data, .. } => {
            assert!(matches!(data, ImageData::Url(u) if u == "https://example.com/a.jpg"));
        }
        other => unreachable!("tagged as image, got {other:?}"),
    }
}

#[test]
fn file_block_round_trips_through_json() {
    let block = ToolContent::File {
        name: "report.pdf".into(),
        media_type: "application/pdf".into(),
        data: FileData::Path("/tmp/report.pdf".into()),
    };
    let encoded = serde_json::to_value(&block).expect("serializable");
    assert_eq!(
        encoded,
        json!({
            "type": "file",
            "name": "report.pdf",
            "media_type": "application/pdf",
            "data": { "kind": "path", "value": "/tmp/report.pdf" },
        })
    );
    let decoded: ToolContent = serde_json::from_value(encoded).expect("deserializable");
    match decoded {
        ToolContent::File {
            name,
            media_type,
            data,
        } => {
            assert_eq!(name, "report.pdf");
            assert_eq!(media_type, "application/pdf");
            assert!(matches!(data, FileData::Path(p) if p == "/tmp/report.pdf"));
        }
        other => unreachable!("tagged as file, got {other:?}"),
    }
}

#[test]
fn file_block_supports_base64_and_url_data() {
    let base64 = FileData::Base64("aGk=".into());
    let url = FileData::Url("https://example.com/a.csv".into());
    for data in [base64, url] {
        let block = ToolContent::File {
            name: "a".into(),
            media_type: "text/csv".into(),
            data,
        };
        let encoded = serde_json::to_string(&block).expect("serializable");
        let _: ToolContent = serde_json::from_str(&encoded).expect("deserializable");
    }
}

#[test]
fn text_and_output_render_placeholders_for_image_and_file_blocks() {
    let r = ToolResult {
        content: vec![
            ToolContent::Text {
                text: "before".into(),
            },
            ToolContent::Image {
                media_type: "image/png".into(),
                data: ImageData::Base64("Zm9v".into()),
            },
            ToolContent::File {
                name: "notes.txt".into(),
                media_type: "text/plain".into(),
                data: FileData::Url("https://example.com/notes.txt".into()),
            },
        ],
        ..ToolResult::default()
    };
    assert_eq!(
        r.text(),
        "before\n[image image/png]\n[file notes.txt (text/plain)]"
    );
    assert_eq!(r.output(), r.text());
}

#[test]
fn with_image_appends_an_image_block() {
    let r =
        ToolResult::success("caption").with_image("image/png", ImageData::Base64("Zm9v".into()));
    assert_eq!(r.content.len(), 2);
    assert!(r.text().ends_with("[image image/png]"));
}

#[test]
fn with_follow_up_is_not_included_in_text_or_output() {
    let r = ToolResult::success("primary").with_follow_up(vec![ToolContent::Text {
        text: "secondary".into(),
    }]);
    assert_eq!(r.text(), "primary");
    assert_eq!(r.output(), "primary");
    assert_eq!(r.follow_up.len(), 1);
}

#[test]
fn follow_up_is_omitted_from_wire_shape_when_empty() {
    let r = ToolResult::success("plain");
    let encoded = serde_json::to_value(&r).expect("serializable");
    assert!(encoded.get("follow_up").is_none());
}

#[test]
fn follow_up_round_trips_when_present() {
    let r = ToolResult::success("primary").with_follow_up(vec![ToolContent::Text {
        text: "secondary".into(),
    }]);
    let encoded = serde_json::to_string(&r).expect("serializable");
    let back: ToolResult = serde_json::from_str(&encoded).expect("deserializable");
    assert_eq!(back.follow_up.len(), 1);
}

#[test]
fn with_metadata_is_never_rendered_but_round_trips() {
    let r = ToolResult::success("primary").with_metadata(json!({"trace_id": "abc"}));
    assert_eq!(r.text(), "primary");
    assert!(!r.output().contains("trace_id"));
    let encoded = serde_json::to_string(&r).expect("serializable");
    let back: ToolResult = serde_json::from_str(&encoded).expect("deserializable");
    assert_eq!(back.metadata, Some(json!({"trace_id": "abc"})));
}

#[test]
fn metadata_is_omitted_from_wire_shape_when_absent() {
    let r = ToolResult::success("plain");
    let encoded = serde_json::to_value(&r).expect("serializable");
    assert!(encoded.get("metadata").is_none());
}

#[test]
fn control_builders_set_the_expected_fields() {
    let r = ToolResult::success("done")
        .return_direct()
        .terminate()
        .with_goto("next_node")
        .with_state_update(json!({"count": 1}));
    let control = r.control.as_ref().expect("control set");
    assert_eq!(control.return_direct, Some(true));
    assert!(control.terminate);
    assert_eq!(control.goto.as_deref(), Some("next_node"));
    assert_eq!(control.state_update, Some(json!({"count": 1})));
}

#[test]
fn control_is_omitted_from_wire_shape_when_absent() {
    let r = ToolResult::success("plain");
    let encoded = serde_json::to_value(&r).expect("serializable");
    assert!(encoded.get("control").is_none());
}

#[test]
fn control_round_trips_through_json() {
    let r = ToolResult::success("done").return_direct().with_goto("n");
    let encoded = serde_json::to_string(&r).expect("serializable");
    let back: ToolResult = serde_json::from_str(&encoded).expect("deserializable");
    let control = back.control.expect("control set");
    assert_eq!(control.return_direct, Some(true));
    assert!(!control.terminate);
    assert_eq!(control.goto.as_deref(), Some("n"));
}

#[test]
fn retry_and_failed_both_set_is_error_but_distinct_error_kind() {
    let retry = ToolResult::retry("try again");
    assert!(retry.is_error);
    assert_eq!(retry.error_kind, Some(ToolErrorKind::Retry));
    assert_eq!(retry.text(), "try again");

    let failed = ToolResult::failed("do not retry");
    assert!(failed.is_error);
    assert_eq!(failed.error_kind, Some(ToolErrorKind::Failed));
    assert_eq!(failed.text(), "do not retry");
}

#[test]
fn plain_error_leaves_error_kind_unset() {
    let r = ToolResult::error("failed");
    assert_eq!(r.error_kind, None);
}

#[test]
fn error_kind_round_trips_through_json() {
    let r = ToolResult::retry("again");
    let encoded = serde_json::to_string(&r).expect("serializable");
    let back: ToolResult = serde_json::from_str(&encoded).expect("deserializable");
    assert_eq!(back.error_kind, Some(ToolErrorKind::Retry));
}

#[test]
fn legacy_json_without_new_fields_still_deserializes() {
    // A transcript persisted before these fields existed should still decode,
    // with every new field taking its default.
    let literal = r#"{"content":[{"type":"text","text":"hi"}],"is_error":false}"#;
    let decoded: ToolResult = serde_json::from_str(literal).expect("deserializable");
    assert!(decoded.follow_up.is_empty());
    assert_eq!(decoded.metadata, None);
    assert!(decoded.control.is_none());
    assert_eq!(decoded.error_kind, None);
}

#[test]
fn default_control_round_trips_to_all_false_and_none() {
    let control = ToolControl::default();
    assert!(!control.return_direct);
    assert!(!control.terminate);
    assert_eq!(control.goto, None);
    assert_eq!(control.state_update, None);
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
