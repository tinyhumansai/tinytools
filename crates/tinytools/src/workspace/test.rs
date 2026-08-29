#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use super::{SandboxMode, WorkspaceDescriptor};

#[test]
fn a_new_descriptor_is_rooted_with_no_extras() {
    let ws = WorkspaceDescriptor::new("/work/agent-a");
    assert_eq!(ws.root, PathBuf::from("/work/agent-a"));
    assert!(ws.trusted_roots.is_empty());
    assert!(ws.policy_id.is_empty());
    assert_eq!(ws.sandbox, SandboxMode::Inherit);
}

#[test]
fn the_builders_set_each_field() {
    let ws = WorkspaceDescriptor::new("/work/agent-a")
        .with_trusted_root("/shared/cache")
        .with_policy_id("worktree")
        .with_sandbox(SandboxMode::Required);
    assert_eq!(ws.trusted_roots, vec![PathBuf::from("/shared/cache")]);
    assert_eq!(ws.policy_id, "worktree");
    assert_eq!(ws.sandbox, SandboxMode::Required);
}

#[test]
fn paths_inside_the_root_or_a_trusted_root_are_allowed() {
    let ws = WorkspaceDescriptor::new("/work/agent-a").with_trusted_root("/shared/cache");
    assert!(ws.allows(Path::new("/work/agent-a/src/main.rs")));
    assert!(ws.allows(Path::new("/shared/cache/pkg")));
}

#[test]
fn paths_outside_every_root_are_refused() {
    let ws = WorkspaceDescriptor::new("/work/agent-a");
    assert!(!ws.allows(Path::new("/etc/passwd")));
    assert!(!ws.allows(Path::new("/work/agent-b/src/main.rs")));
}

#[test]
fn dot_segments_are_resolved_before_the_comparison() {
    let ws = WorkspaceDescriptor::new("/work/agent-a");
    assert!(ws.allows(Path::new("/work/agent-a/./src/../src/main.rs")));
    assert!(!ws.allows(Path::new("/work/agent-a/../agent-b/secret")));
}

#[test]
fn a_parent_traversal_cannot_spoof_re_entry_into_a_same_named_sibling() {
    // `..` must not collapse a path back onto a same-named directory outside
    // the root: dropping the escaping components is what would let
    // `agent-a/../../agent-a/secret` read as inside `/work/agent-a`.
    let ws = WorkspaceDescriptor::new("/work/nested/agent-a");
    assert!(!ws.allows(Path::new("/work/nested/agent-a/../../agent-a/secret")));
}

#[test]
fn a_parent_traversal_at_the_filesystem_root_cannot_go_higher() {
    let ws = WorkspaceDescriptor::new("/");
    assert!(ws.allows(Path::new("/../etc")));
}

#[test]
fn the_descriptor_round_trips_through_json() {
    let ws = WorkspaceDescriptor::new("/work/agent-a")
        .with_trusted_root("/shared")
        .with_policy_id("worktree")
        .with_sandbox(SandboxMode::Disabled);
    let encoded = serde_json::to_string(&ws).expect("serializable");
    let back: WorkspaceDescriptor = serde_json::from_str(&encoded).expect("deserializable");
    assert_eq!(back, ws);
}

#[test]
fn sandbox_mode_uses_snake_case_on_the_wire() {
    assert_eq!(
        serde_json::to_string(&SandboxMode::Inherit).expect("serializable"),
        "\"inherit\""
    );
    assert_eq!(
        serde_json::to_string(&SandboxMode::Required).expect("serializable"),
        "\"required\""
    );
    assert_eq!(SandboxMode::default(), SandboxMode::Inherit);
}

#[test]
fn omitted_optional_fields_default_on_decode() {
    let back: WorkspaceDescriptor =
        serde_json::from_str(r#"{"root":"/work"}"#).expect("deserializable");
    assert_eq!(back, WorkspaceDescriptor::new("/work"));
}
