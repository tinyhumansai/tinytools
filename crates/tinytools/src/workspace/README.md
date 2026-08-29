# `workspace`

`WorkspaceDescriptor` and `SandboxMode` — the isolated execution environment a
tool is allowed to operate in, and how strictly it must be sandboxed.

## Design

A tool discovers its allowed root through [`ToolRunContext::workspace`][ctx]
instead of reaching for an application global. That is what lets two agents run
over the same repository in separate worktrees without either one's tools
knowing anything about the arrangement: each gets its own `WorkspaceDescriptor`
naming its own root.

`WorkspaceDescriptor` carries four fields, all serializable so a descriptor
survives a config file, an RPC payload, or a persisted session:

- `root` — the primary directory the tool may read and write under.
- `trusted_roots` — additional directories explicitly trusted alongside `root`
  (a shared cache, a sibling worktree).
- `policy_id` — an opaque identity of the policy that produced this
  descriptor, carried for audit rather than interpreted here.
- `sandbox` — a [`SandboxMode`], set by the host and read, never decided, by a
  tool.

[ctx]: ../context/mod.rs

## Public surface

- `WorkspaceDescriptor::new` / `with_trusted_root` / `with_policy_id` /
  `with_sandbox` — a small builder; every field defaults to the conservative
  answer (no trusted roots, no policy id, `SandboxMode::Inherit`).
- `WorkspaceDescriptor::allows(&self, path: &Path) -> bool` — the one piece of
  logic this module owns.

## Important operational constraint: `allows` is lexical, not a filesystem call

`allows` normalizes `.` and `..` components and checks whether the result falls
under `root` or a trusted root. It does **not** call `canonicalize` and does
**not** resolve symlinks. That is deliberate, not an oversight:

- It has to answer for a path that does not exist yet — a tool about to
  *create* a file — and `canonicalize` requires the target to exist.
- It runs on every tool invocation, so a filesystem syscall per check is real
  cost paid by every consumer, not only the ones with a hostile symlink to
  worry about.

The consequence: a symlink already present inside an allowed root and pointing
outside it (`<root>/outside -> /etc`) makes `allows` return `true` for
`<root>/outside/passwd`, because the check compares path *components*, not
resolved targets. See the doc comment on `allows` in `types.rs` for the full
reasoning.

**This means `allows` is the first, cheap, existence-independent check — never
the last word on containment.** A host that must be robust against a symlink
planted inside the workspace (an untrusted or compromised tool output, a shared
filesystem) is expected to layer its own canonicalizing enforcement on top and
re-check containment before it actually opens the file:

- `tinyagents`'s `enforce_workspace_path` is the fail-closed host-side gate.
- OpenHuman layers its own path policy (`is_workspace_internal_path`, the
  sandbox backends) on top for the same reason.

This module holds no enforcement of its own — see the crate's top-level
`README.md` for why that line is where it is.
