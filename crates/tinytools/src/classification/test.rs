#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{ToolCategory, ToolScope};

#[test]
fn scope_variants_are_distinct_and_default_to_all() {
    assert_ne!(ToolScope::All, ToolScope::AgentOnly);
    assert_ne!(ToolScope::All, ToolScope::CliRpcOnly);
    assert_ne!(ToolScope::AgentOnly, ToolScope::CliRpcOnly);
    assert_eq!(ToolScope::default(), ToolScope::All);
}

#[test]
fn category_defaults_to_system() {
    assert_eq!(ToolCategory::default(), ToolCategory::System);
}

#[test]
fn category_display_matches_its_wire_form() {
    assert_eq!(ToolCategory::System.to_string(), "system");
    assert_eq!(ToolCategory::Workflow.to_string(), "skill");
}

#[test]
fn workflow_stays_pinned_to_the_skill_wire_name() {
    // Agent definition files on disk carry `"skill"`. Renaming the wire form to
    // match the Rust ident would stop those files parsing.
    assert_eq!(
        serde_json::to_string(&ToolCategory::System).expect("serializable"),
        "\"system\""
    );
    assert_eq!(
        serde_json::to_string(&ToolCategory::Workflow).expect("serializable"),
        "\"skill\""
    );
    let back: ToolCategory = serde_json::from_str("\"skill\"").expect("deserializable");
    assert_eq!(back, ToolCategory::Workflow);
}
