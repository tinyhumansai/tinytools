//! The isolated execution environment a tool is allowed to operate in.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How strictly a tool must be sandboxed when it executes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxMode {
    /// Inherit whatever the run's execution environment provides.
    #[default]
    Inherit,
    /// The tool is safe to run without any sandbox.
    Disabled,
    /// The tool must run inside an isolated execution environment; policy
    /// enforcement fails closed if no sandbox is available.
    Required,
}

/// Describes the isolated execution environment a tool may operate in.
///
/// A tool discovers its allowed root from this descriptor — reached through
/// [`ToolRunContext::workspace`][crate::ToolRunContext::workspace] — instead of
/// reaching for an application global. That is what lets two agents run over
/// the same repository in separate worktrees without either one's tools knowing
/// anything about the arrangement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceDescriptor {
    /// The primary root the agent or tool may read and write under.
    pub root: PathBuf,
    /// Additional roots the tool is explicitly trusted to touch.
    #[serde(default)]
    pub trusted_roots: Vec<PathBuf>,
    /// Identity of the policy that produced this descriptor, for audit.
    #[serde(default)]
    pub policy_id: String,
    /// How strictly the environment is sandboxed.
    #[serde(default)]
    pub sandbox: SandboxMode,
}

impl WorkspaceDescriptor {
    /// A descriptor rooted at `root` with no extra trusted roots.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            trusted_roots: Vec::new(),
            policy_id: String::new(),
            sandbox: SandboxMode::Inherit,
        }
    }

    /// Adds a trusted root the tool may also touch.
    #[must_use]
    pub fn with_trusted_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.trusted_roots.push(root.into());
        self
    }

    /// Sets the audit policy identity.
    #[must_use]
    pub fn with_policy_id(mut self, id: impl Into<String>) -> Self {
        self.policy_id = id.into();
        self
    }

    /// Sets the sandbox mode.
    #[must_use]
    pub fn with_sandbox(mut self, sandbox: SandboxMode) -> Self {
        self.sandbox = sandbox;
        self
    }

    /// Returns `true` when `path` is contained within the root or any trusted
    /// root.
    ///
    /// Comparison is lexical, after normalizing `.` and `..` components, so it
    /// does not require the path to exist: this is a policy gate, not a
    /// canonicalizing filesystem call. Relative candidates and roots are first
    /// anchored to the current working directory, so a relative path cannot use
    /// leading `..` components to spoof re-entry into a same-named sibling of
    /// the root. If the current directory cannot be read, the gate fails closed.
    #[must_use]
    pub fn allows(&self, path: &Path) -> bool {
        let Some(candidate) = anchored_normalize(path) else {
            return false;
        };
        std::iter::once(&self.root)
            .chain(self.trusted_roots.iter())
            .filter_map(|root| anchored_normalize(root))
            .any(|root| candidate.starts_with(&root))
    }
}

/// Anchors `path` to an absolute base (the current working directory when
/// relative) and lexically normalizes it. Returns `None` when a relative path
/// cannot be anchored because the current directory is unavailable, so callers
/// fail closed.
fn anchored_normalize(path: &Path) -> Option<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    Some(normalize(&absolute))
}

/// Lexically normalizes a path by resolving `.` and `..` components without
/// touching the filesystem.
///
/// A `..` only pops a preceding *named* segment; a `..` that would escape the
/// accumulated prefix (leading, or after another `..`) is preserved rather than
/// discarded. Dropping such components would let a relative path like
/// `ws/../../ws/secret` collapse back onto `ws` and spoof re-entry into a
/// same-named sibling directory outside the workspace.
fn normalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {
                    // At a filesystem root; `..` cannot go higher.
                }
                _ => out.push(Component::ParentDir),
            },
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}
