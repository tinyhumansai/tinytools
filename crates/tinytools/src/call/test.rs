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
    assert!(!ToolTimeout::Secs(30).is_inherit());
}

#[test]
fn timeout_variants_are_distinct() {
    assert_ne!(ToolTimeout::Inherit, ToolTimeout::Unbounded);
    assert_ne!(ToolTimeout::Secs(1), ToolTimeout::Secs(2));
}
