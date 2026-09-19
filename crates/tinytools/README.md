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
facts: workspace, thread id, and output cap — plus `host_extension()`, the
same type-erased escape hatch `Tool::host_extension` offers, so a tool written
against one specific harness can downcast to that harness's full context.

## Rich tool returns

`ToolContent` has four block kinds: `Text`, `Json`, `Image`, and `File`.
`Image` carries a MIME `media_type` plus `ImageData::{Base64, Url}`; `File`
carries a display `name`, a `media_type`, and `FileData::{Base64, Url, Path}`.
`ToolResult::text()`, `output()`, and `output_for_llm()` render `Text`
verbatim, pretty-print `Json`, skip `Json` from `text()` specifically (as
before), and render a short placeholder for `Image`/`File` — `[image
image/png]`, `[file report.pdf (application/pdf)]` — so a model still gets a
sensible turn even when a renderer does not special-case the new block kinds.

`ToolResult` carries three more optional surfaces beyond `content` and
`markdown_formatted`:

- `follow_up: Vec<ToolContent>` — content the caller should present to the
  model as a *separate* user message after the tool result (a screenshot, a
  generated document). It is never included in `text()`, `output()`, or
  `output_for_llm()`; a host that wants to honour it reads the field directly.
  Attach it with `with_follow_up(..)`.
- `metadata: Option<serde_json::Value>` — host-only data (trace ids, raw
  provider payloads) that is never shown to the model. Attach it with
  `with_metadata(..)`.
- `control: Option<ToolControl>` — loop-control hints a harness may honour:
  `return_direct`, `terminate`, `goto: Option<String>`, and
  `state_update: Option<serde_json::Value>`. Set them with the builders
  `return_direct()`, `terminate()`, `with_goto(..)`, and
  `with_state_update(..)`, which lazily create the `ToolControl`.

`ToolResult::retry(message)` and `ToolResult::failed(message)` both set
`is_error`, same as `error(message)`, but additionally tag
`error_kind: Option<ToolErrorKind>` as `Retry` or `Failed` — Pydantic AI's
`ModelRetry` versus a permanent tool failure — so a harness can decide whether
to loop the model back in or surface the failure as final.

Every new field is `#[serde(default)]` and, where it can be empty or absent,
`skip_serializing_if`, so a `ToolResult` persisted before these fields existed
still decodes, and a plain result's wire shape is unchanged.

## Static and per-call return-direct

`Tool::return_direct()` is a static, per-tool default (`false`) for a tool
whose entire purpose is to hand the model's answer straight back — a
final-answer or handoff tool overrides it to `true`. `ToolResult::control`'s
`return_direct` is the per-*call* override on `ToolControl`; a harness should
prefer the per-call value on the result it just received over the tool's
static declaration.

## Replay after a crash

`ToolPolicy`'s `ToolRuntime` carries `replay: ToolReplay`, mirroring pi's
`replay` classification: whether an orphaned in-flight call for a tool may be
safely re-executed after a crash. It defaults to `ToolReplay::Never`; a tool
that is idempotent or otherwise safe to repeat declares `ToolReplay::Safe`
through its `ToolPolicy`. This lives on the existing declarative policy
surface rather than as a new `Tool` trait method, consistent with how every
other runtime requirement (timeout, retries, cancellation, sandboxing) is
already expressed there.

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

## Injected arguments and call identity

`ToolCall` and its `ToolCallId` carry the identity of one parsed model request.
That identity is invocation metadata, not a `ToolResult` field, so a harness can
correlate events and results without mutating tool-owned output.

A tool declares host-owned schema keys with `Tool::injected_arguments()`, using
`ToolInjectedArgument::host` or `ToolInjectedArgument::tool_call_id`. The host
uses `project_injected_arguments` for the model-facing schema, keeps
authoritative values in runtime-only `InjectedToolArguments`, and calls
`prepare_tool_arguments` before schema validation. The helper strips the
model-supplied protected keys, inserts authoritative values, and returns the
object to validate. It never serializes host values, and it ignores values for
undeclared keys. This ordering prevents a model from forging a protected
argument while keeping model arguments model-owned.

`ToolTimeout` is serializable and has exactly three wire forms:

| Rust value | JSON |
| --- | --- |
| `ToolTimeout::Inherit` | `{ "mode": "inherit" }` |
| `ToolTimeout::Unbounded` | `{ "mode": "unbounded" }` |
| `ToolTimeout::Millis(250)` | `{ "mode": "millis", "timeout_ms": 250 }` |

All policy vocabulary uses the documented serde field names. These forms are
part of the public persistence and introspection contract; change them only
with an intentional compatibility migration and updated literal-wire tests.
`ToolCall` and `ToolInjectedArgument` are likewise serialized vocabulary;
`InjectedToolArguments` intentionally is not, because it may hold credentials.

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
