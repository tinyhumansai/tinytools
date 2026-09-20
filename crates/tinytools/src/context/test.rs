//! Unit tests for `ToolRunContext`: the trait's defaults, and that a real
//! implementor is reachable through the erased trait object.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use super::ToolRunContext;
use crate::workspace::WorkspaceDescriptor;

/// A context that answers nothing, exercising every default.
struct Bare;
impl ToolRunContext for Bare {}

/// A context shaped like a harness's real one.
struct Isolated {
    workspace: WorkspaceDescriptor,
}

impl ToolRunContext for Isolated {
    fn workspace(&self) -> Option<&WorkspaceDescriptor> {
        Some(&self.workspace)
    }

    fn thread_id(&self) -> Option<&str> {
        Some("thread-7")
    }

    fn max_turn_output_tokens(&self) -> Option<u32> {
        Some(4096)
    }
}

#[test]
fn the_defaults_answer_nothing() {
    let bare = Bare;
    assert!(bare.workspace().is_none());
    assert!(bare.workspace_root().is_none());
    assert!(bare.workspace_policy_id().is_none());
    assert!(bare.thread_id().is_none());
    assert!(bare.max_turn_output_tokens().is_none());
}

#[test]
fn an_implementor_is_readable_through_the_trait_object() {
    let isolated = Isolated {
        workspace: WorkspaceDescriptor::new("/tmp/worktree").with_policy_id("worktree-isolation"),
    };
    let erased: &dyn ToolRunContext = &isolated;
    assert_eq!(erased.workspace_root(), Some(Path::new("/tmp/worktree")));
    assert_eq!(erased.workspace_policy_id(), Some("worktree-isolation"));
    assert_eq!(erased.thread_id(), Some("thread-7"));
    assert_eq!(erased.max_turn_output_tokens(), Some(4096));
    assert_eq!(
        erased.workspace().map(|w| w.root.clone()),
        Some(PathBuf::from("/tmp/worktree"))
    );
}

/// A context carrying a host-owned payload behind the erased hook.
struct Hosted {
    tag: HostTag,
}

#[derive(Debug, PartialEq)]
struct HostTag(&'static str);

impl ToolRunContext for Hosted {
    fn host_extension(&self) -> Option<&(dyn std::any::Any + Send + Sync)> {
        Some(&self.tag)
    }
}

#[test]
fn the_default_host_extension_is_absent() {
    let erased: &dyn ToolRunContext = &Bare;
    assert!(erased.host_extension().is_none());
}

#[test]
fn a_host_recovers_its_own_context_by_downcasting() {
    let hosted = Hosted {
        tag: HostTag("call-7"),
    };
    let erased: &dyn ToolRunContext = &hosted;
    let tag = erased
        .host_extension()
        .and_then(|any| any.downcast_ref::<HostTag>());
    assert_eq!(tag, Some(&HostTag("call-7")));
}
