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
- Define `ToolPolicy` as the complete declarative policy vocabulary: runtime
  requirements (`ToolRuntime`), access requirements (`ToolAccess` and
  `WorkspaceAccess`), independent side-effect declarations
  (`ToolSideEffects`), and host-facing presentation metadata (`ToolDisplay`).
  `Tool::policy()` returns this declaration and defaults to unclassified, so a
  host may admit tools fail-closed without this crate making that decision.
- Define model-call vocabulary independently of provider protocol types:
  `ToolCallId` and `ToolCall` carry a stable request identity outside
  `ToolResult`; `ToolInjectedArgument` declares host- or call-id-owned schema
  keys. `project_injected_arguments` produces the model-facing schema, while
  `prepare_tool_arguments` strips model-supplied values, injects authoritative
  runtime-only `InjectedToolArguments`, and returns the object a host validates
  against the declared tool schema. Injection values intentionally have no
  serde representation because they may contain credentials.
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
  `WorkspaceDescriptor`, `ToolTimeout`, and `ToolPolicy`) are pinned by a
  literal-JSON test, not merely a
  round-trip, so a silent field rename fails a test instead of a downstream
  consumer's persisted data.
- `ToolTimeout` has only three canonical JSON forms:
  `{ "mode": "inherit" }`, `{ "mode": "unbounded" }`, and
  `{ "mode": "millis", "timeout_ms": <u64> }`.
  `ToolPolicy` field names and omission rules are likewise a persisted policy
  and introspection contract, including limits, trusted roots, credentials,
  and display metadata.
- `ToolCall` and `ToolInjectedArgument` are literal-wire tested. A model can
  never supply an injected value: the preparation helper removes every
  declaration name before it reads a host value or derives the call id. Hosts
  validate only the prepared object, never the original model arguments.

## Acceptance criteria

- `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo build --all-targets --all-features`, and `cargo test --all-features`
  all pass.
- Every source file carries at least 90% line coverage
  (`.github/scripts/check-file-coverage.sh 90 coverage.json`).
- `cargo doc --no-deps --all-features` and `cargo deny check all` pass.
- The dependency-light CI gate passes against the reviewed allowlist.
- `crates/tinytools/README.md` (the crate's packaged README, selected by
  `crates/tinytools/Cargo.toml`) and this specification stay aligned with the
  public surface as it evolves.

## Extension: rich results, replay classification, and a host escape hatch

Landed after the crate's initial acceptance (tracked in
[`../plans/tinytools-vocabulary.md`](../plans/tinytools-vocabulary.md#task-7-rich-toolresult-replay-classification-and-a-host-escape-hatch)),
this section specifies the additions below. All are additive to the wire shape
(new fields default and are omitted when absent) but not source-compatible —
see "Versioning" — so they shipped as a `0.2.0` → `0.3.0` minor bump.

- **`ToolContent::Image` / `ToolContent::File`** extend the block-list result
  with image and file blocks (`ImageData` / `FileData`, each `Base64`, `Url`,
  or `Path`), alongside the existing `Text` and `Json` blocks.
- **`ToolResult::follow_up: Vec<ToolContent>`** carries content a caller should
  present to the model as a *separate* message after the tool result — a
  screenshot or document the next turn should read — rather than folding it
  into the result itself. It is deliberately excluded from `text()`,
  `output()`, and `output_for_llm()`; a host that wants to honour it reads the
  field directly.
- **`ToolResult::metadata: Option<serde_json::Value>`** is host-only data
  (trace ids, raw provider payloads) never shown to the model.
- **`ToolResult::control: Option<ToolControl>`** carries loop-control hints a
  harness may honour: `terminate`, `goto: Option<String>`,
  `state_update: Option<serde_json::Value>`, and `return_direct:
  Option<bool>`. `return_direct` is tri-state, not a defaulted `bool`: `None`
  means this call did not express an opinion and a harness falls back to the
  tool's static [`Tool::return_direct`] default; `Some(true)` /
  `Some(false)` are explicit per-call overrides. This tri-state is load
  bearing — a call that only used `with_goto`, `with_state_update`, or
  `terminate` must not be read as silently disabling a tool's static `true`
  declaration. See "Static and per-call return-direct" in
  `crates/tinytools/README.md`.
- **`ToolResult::error_kind: Option<ToolErrorKind>`** distinguishes a reported
  failure the model should retry (`Retry`, set by `ToolResult::retry`) from
  one it should not (`Failed`, set by `ToolResult::failed`) — modelled on
  Pydantic AI's `ModelRetry` versus a permanent failure. `None` (the
  historical shape) means the caller did not classify the failure.
- **`ToolRuntime::replay: ToolReplay`** classifies whether an orphaned
  in-flight call for a tool may be safely re-executed after a crash,
  mirroring pi's `replay` classification. It defaults to `ToolReplay::Never`;
  an idempotent tool declares `ToolReplay::Safe` through its `ToolPolicy`.
  This lives on the existing declarative policy surface rather than a new
  `Tool` trait method, consistent with how every other runtime requirement
  (timeout, retries, cancellation, sandboxing) is already expressed there.
- **`ToolRunContext::host_extension`** is the same escape hatch as
  `Tool::host_extension`: this crate has no business naming the harness's
  context type, so a host implementor returns `Some(self)` as
  `&(dyn Any + Send + Sync)` and a tool that knows which host it runs under
  downcasts. Every other implementor returns `None` and pays nothing. A tool
  that only needs the workspace, thread id, or output cap keeps using the
  typed methods.

## Open questions

- Whether `ToolResult`/`ToolContent` should ever adopt an actual MCP
  `CallToolResult` wire shape (camelCase `isError`, `structuredContent`) is
  deferred: today this type is this crate's own internal transcript/RPC shape,
  conceptually MCP-shaped but not byte-compatible, and any real MCP-server
  interop is expected to translate at the point that actually speaks the MCP
  protocol. Revisit if a maintainer decides byte-level interop through this
  exact type is a goal.
