# TinyTools

The vocabulary an agent tool is written against: the `Tool` trait, the
`ToolResult` it returns, and the classifications a host enforces around a call.

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
        Ok(ToolResult::success(args["text"].as_str().unwrap_or_default()))
    }
}
```

That is a complete tool. Everything else in the trait has a default.

`Tool` is async, so implementing it needs the `async-trait` shim above and
beyond `tinytools` itself — this crate depends on it internally but does not
re-export the macro. Add it as a direct dependency alongside `tinytools`:

```toml
[dependencies]
tinytools = "0.1"
async-trait = "0.1"
```

## Why this is its own crate

Two crates need these types and neither can own them. An agent harness has to
name a tool's result to run a loop over it; a host application has to name the
same result to implement one. When both declare their own, the conversions
between them get written by hand at every seam — which is how an error flag ends
up inverted in one direction with nothing to catch it.

So the vocabulary sits underneath both. A harness depends on this crate and
re-exports it, so `harness::ToolResult` and `tinytools::ToolResult` are the
*same type*, not structural twins. A tool author depends on this crate alone and
compiles neither the harness nor the host.

## What is here

| Module | Holds |
| --- | --- |
| `tool` | `Tool` — four required methods, and defaulted declarations describing what the tool needs and touches |
| `result` | `ToolResult`, `ToolContent` — the MCP-shaped block list a tool hands back |
| `spec` | `ToolSpec` — the declaration a model is shown |
| `permission` | `PermissionLevel` — the privilege ladder, ordered `None` → `Dangerous` |
| `classification` | `ToolScope`, `ToolCategory` — where a tool may run, and which belt it is on |
| `call` | `ToolCallOptions`, `ToolTimeout` — per-invocation inputs that are not arguments |
| `context` | `ToolRunContext` — the narrow seam onto a live run |
| `naming` | `humanize_tool_name`, `context_detail_from_args` — rendering a call for a human |

## What is deliberately not here

**No enforcement.** Nothing in this crate checks a `PermissionLevel`, applies a
`ToolTimeout`, or decides whether an `external_effect` needs approval. A tool
*describes* itself and a host *decides*, because the decision depends on that
host's threat model, its configuration, and who is asking — none of which
generalize. Putting the check here would mean every host inherits one host's
policy.

**No registry, no dispatch, no execution loop.** Those belong to whoever owns
the run.

**No dependency on an agent harness.** The harness depends on this crate.
`ToolRunContext` exists precisely so a tool can read run-scoped facts — the
isolated-workspace root being the common one — without this crate naming the
harness type that carries them. CI asserts the edge stays pointing one way.

## The trait is a declaration, not an enforcement point

Beyond `name` / `description` / `parameters_schema` / `execute`, every method on
`Tool` answers a question a host asks *before* it calls the tool: what privilege
does this need, does it reach outside the machine, how long may it run, how
should it read in a timeline. The defaults are the conservative answer in every
case except `permission_level`, which defaults to `ReadOnly` because most tools
genuinely read.

Two consequences worth knowing:

- **A tool that exposes several actions should declare the *minimum* privilege
  any of them needs** from `permission_level`, and the exact one from
  `permission_level_with_args`. Declaring the maximum statically blocks the tool
  for callers that could legitimately run its read-only half.
- **The argument-aware variants are the ones a host calls** at the enforcement
  point. Overriding only `external_effect` on a tool whose classification
  depends on its arguments leaves the per-call case unhandled.

## Development

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
```

Lint levels live in `[workspace.lints]` so local and CI runs agree. Library code
may not `unwrap`, `expect`, or `panic`; test modules opt out at the top of the
file.

## License

GPL-3.0-only. See [`LICENSE`](LICENSE).
