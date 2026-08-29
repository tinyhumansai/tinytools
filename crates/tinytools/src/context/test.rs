#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use super::ToolRunContext;

/// A context that answers nothing, exercising every default.
struct Bare;
impl ToolRunContext for Bare {}

/// A context shaped like a harness's real one.
struct Isolated {
    root: PathBuf,
}

impl ToolRunContext for Isolated {
    fn workspace_root(&self) -> Option<&Path> {
        Some(&self.root)
    }

    fn workspace_policy_id(&self) -> Option<&str> {
        Some("worktree-isolation")
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
    assert!(bare.workspace_root().is_none());
    assert!(bare.workspace_policy_id().is_none());
    assert!(bare.thread_id().is_none());
    assert!(bare.max_turn_output_tokens().is_none());
}

#[test]
fn an_implementor_is_readable_through_the_trait_object() {
    let isolated = Isolated {
        root: PathBuf::from("/tmp/worktree"),
    };
    let erased: &dyn ToolRunContext = &isolated;
    assert_eq!(erased.workspace_root(), Some(Path::new("/tmp/worktree")));
    assert_eq!(erased.workspace_policy_id(), Some("worktree-isolation"));
    assert_eq!(erased.thread_id(), Some("thread-7"));
    assert_eq!(erased.max_turn_output_tokens(), Some(4096));
}
