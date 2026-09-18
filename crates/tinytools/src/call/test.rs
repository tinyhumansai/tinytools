//! Unit tests for `ToolCallOptions` and `ToolTimeout`: their defaults and
//! that each variant stays distinct.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{ToolCallOptions, ToolTimeout};

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
