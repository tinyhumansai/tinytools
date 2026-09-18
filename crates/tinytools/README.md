# TinyTools

`tinytools` is the dependency-light vocabulary shared by a tool implementation
and the host or harness that runs it. Its primary entry points are `Tool`,
`ToolSpec`, `ToolResult`, `ToolRunContext`, and `ToolPolicy`.

```rust
use tinytools::{Tool, ToolPolicy, ToolResult};

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
        let text = args.get("text").and_then(|value| value.as_str()).unwrap_or_default();
        Ok(ToolResult::success(text))
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::read_only()
    }
}
```

## Public contract

`Tool` describes a callable capability. Its result is a `ToolResult` block list
with a reported-error flag and optional markdown rendering. `ToolSpec` is the
model-visible declaration. `ToolRunContext` exposes only tool-relevant run
facts: workspace, thread id, and output cap.

`ToolPolicy` is the complete host-readable declaration around a call:

- `ToolSideEffects` records filesystem, network, dependency, destructive,
  external-service, payment, and read-only effects.
- `ToolRuntime` records deadline, retry, idempotency, cancellation, sandbox,
  result-size, and streaming requirements.
- `ToolAccess` records workspace reach, trusted roots, credential names,
  approval, and background-safety requirements.
- `ToolDisplay` supplies optional timeline/audit label and detail text.

`Tool::policy()` defaults to an unclassified declaration. A host that admits
tools fail-closed can refuse that default. The policy is descriptive only:
TinyTools does not enforce approvals, credentials, workspace containment,
sandboxing, cancellation, deadlines, retries, or result limits.

`ToolTimeout` is serializable and has exactly three wire forms:

| Rust value | JSON |
| --- | --- |
| `ToolTimeout::Inherit` | `{ "mode": "inherit" }` |
| `ToolTimeout::Unbounded` | `{ "mode": "unbounded" }` |
| `ToolTimeout::Millis(250)` | `{ "mode": "millis", "timeout_ms": 250 }` |

All policy vocabulary uses the documented serde field names. These forms are
part of the public persistence and introspection contract; change them only
with an intentional compatibility migration and updated literal-wire tests.

## Boundaries

TinyTools has no registry, dispatch loop, provider transport, runtime, or agent
harness dependency. `tinytools-agent` owns model-facing tool-call parsing,
dialects, catalogues, rendering, and transcript replay. Hosts own execution and
all policy decisions.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
```

`Tool` implementations require a direct `async-trait` dependency because the
crate uses the macro internally but does not re-export it.
