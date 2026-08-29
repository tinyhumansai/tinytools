# TinyTools: the agent tool vocabulary

- **Status:** Implemented
- **Owner:** Maintainers
- **Plan:** [`../plans/tinytools-vocabulary.md`](../plans/tinytools-vocabulary.md)

## Problem

Two consumers need the same tool vocabulary and neither can own it. An agent
harness (`tinyagents`) has to name a tool's result to run a loop over it; a
host application has to name the same result to implement one. Before this
crate existed, each declared its own `Tool` trait and result type, and the
conversions between them were written by hand at every seam — which is how an
error flag ends up inverted in one direction with nothing to catch it.

## Goals

- Define the `Tool` trait every agent capability implements: four required
  methods, plus a set of defaulted declarations describing what a tool needs
  and what it touches (privilege, scope, category, concurrency safety,
  external effect, timeout, result size cap, human-facing rendering).
- Define `ToolResult` / `ToolContent`, the block-list result shape a tool
  hands back, plus `ToolSpec`, the declaration a model is shown.
- Define the permission ladder (`PermissionLevel`), the classification types
  (`ToolScope`, `ToolCategory`), and the per-invocation inputs that are not
  arguments (`ToolCallOptions`, `ToolTimeout`).
- Provide `ToolRunContext`, a narrow trait erasing a harness's run-scoped
  context (the isolated-workspace root being the common case) so a tool can
  read run facts without this crate naming the harness type that carries them.
- Provide `WorkspaceDescriptor` / `SandboxMode`, describing the isolated
  execution environment a tool may operate in.
- Provide naming helpers (`humanize_tool_name`, `context_detail_from_args`) for
  rendering a tool call in a human-facing timeline.
- Stay dependency-light: `anyhow`, `async-trait`, `serde`, `serde_json` only,
  with `tokio` as a dev-dependency for async test bodies. CI asserts the full
  forward dependency tree against a reviewed allowlist.

## Non-goals

- **No enforcement.** Nothing in this crate checks a `PermissionLevel`,
  applies a `ToolTimeout`, or decides whether an `external_effect` needs
  approval. A tool describes itself; a host decides, because the decision
  depends on that host's threat model, configuration, and caller — none of
  which generalize.
- **No registry, no dispatch, no execution loop.** Those belong to whoever
  owns the run.
- **No dependency on an agent harness.** The harness depends on this crate,
  never the reverse; `context` module exists precisely to make that
  unnecessary. CI asserts the edge stays pointing one way.
- **No canonicalizing filesystem enforcement in `WorkspaceDescriptor::allows`.**
  It is a lexical policy gate that must also answer for paths that do not yet
  exist; a host that must be robust against symlink escapes is expected to
  layer its own canonicalizing check on top (see
  `crates/tinytools/src/workspace/README.md`).

## Proposed behavior

```rust
use tinytools::{Tool, ToolResult};

struct Echo;

#[async_trait::async_trait]
impl Tool for Echo {
    fn name(&self) -> &str { "echo" }
    fn description(&self) -> &str { "Returns its input unchanged." }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": { "text": { "type": "string" } },
            "required": ["text"],
        })
    }
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let text = args.get("text").and_then(|v| v.as_str()).unwrap_or_default();
        Ok(ToolResult::success(text))
    }
}
```

That is a complete tool. Most other methods on `Tool` default to the cautious
answer, but `external_effect`, `max_result_size_chars`, and `permission_level`
fail *open* rather than closed — see the "defaults are not uniformly safe"
section on the trait itself in `crates/tinytools/src/tool/types.rs`, and
`crates/tinytools/src/tool/README.md`.

## Invariants and constraints

- `tinyagents` (or any transport, runtime, HTTP client, or native library)
  never appears in this crate's forward dependency tree. CI's dependency-light
  gate asserts an allowlist of the reviewed tree, not a blocklist of forbidden
  names, so an unreviewed addition fails the gate rather than merely a named
  one.
- `unsafe_code` is `forbid`-level workspace-wide.
- Library code paths do not `unwrap()`, `expect()`, or `panic!()`; tests and
  examples may.
- Every public fallible/panicking API documents its failure mode
  (`# Errors` / `# Panics`).
- Every public item carries rustdoc; `missing_docs` is a CI-blocking warning.
- Wire-shape-bearing types (`PermissionLevel`, `ToolSpec`, `ToolResult`,
  `WorkspaceDescriptor`) are pinned by a literal-JSON test, not merely a
  round-trip, so a silent field rename fails a test instead of a downstream
  consumer's persisted data.

## Acceptance criteria

- `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo build --all-targets --all-features`, and `cargo test --all-features`
  all pass.
- Every source file carries at least 90% line coverage
  (`.github/scripts/check-file-coverage.sh 90 coverage.json`).
- `cargo doc --no-deps --all-features` and `cargo deny check all` pass.
- The dependency-light CI gate passes against the reviewed allowlist.
- `README.md` (the repository root's, which is this crate's packaged
  README — see `crates/tinytools/Cargo.toml`'s `readme` field) and this
  specification stay aligned with the public surface as it evolves.

## Open questions

- Whether `ToolResult`/`ToolContent` should ever adopt an actual MCP
  `CallToolResult` wire shape (camelCase `isError`, `structuredContent`) is
  deferred: today this type is this crate's own internal transcript/RPC shape,
  conceptually MCP-shaped but not byte-compatible, and any real MCP-server
  interop is expected to translate at the point that actually speaks the MCP
  protocol. Revisit if a maintainer decides byte-level interop through this
  exact type is a goal.
