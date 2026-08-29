# `tool`

The `Tool` trait — the single interface every agent capability implements.

## Design

Only four methods are required: `name`, `description`, `parameters_schema`,
and `execute`. Everything else on the trait has a default, so the smallest
useful tool is four short methods, and the rest of the trait is
**declaration**: a tool states what privilege it needs, whether it reaches
outside the machine, how long it may run, and how it should read in a
timeline. A host reads those declarations and decides what to allow — the
trait never enforces policy on itself. See the crate's top-level `README.md`
("What is deliberately not here") for why that split exists.

Most defaults are the cautious answer — `scope` is `ToolScope::All`,
`is_concurrency_safe` is `false`, `timeout_policy` inherits the host's bound.
Three fail *open* instead, and are what to check for when reviewing a `Tool`
impl: `external_effect` defaults to `false` (an effectful tool that doesn't
override it is declaring it has none, and a host honouring that skips its
approval gate), `max_result_size_chars` defaults to `None` (no cap), and
`permission_level` defaults to `PermissionLevel::ReadOnly`, not `None`, because
most tools genuinely read. See the `# The defaults are not uniformly safe, and
two of them fail OPEN` section on the trait itself in `types.rs` for the full
reasoning.

## Public surface

Grouped by what a caller does with the answer:

- **Execution**, layered with defaults that forward inward:
  `execute` (required) ← `execute_with_options` ← `execute_with_context`.
  A tool overrides the outermost layer it cares about; a context-agnostic tool
  needs no change beyond `execute`.
- **Declarations a host reads before calling**: `permission_level` /
  `permission_level_with_args`, `scope`, `category`, `is_concurrency_safe`,
  `external_effect` / `external_effect_with_args`, `max_result_size_chars`,
  `timeout_policy`.
- **Model-facing**: `spec()` builds the `ToolSpec` a model is shown from
  `name` / `description` / `parameters_schema`.
- **Human-facing**: `display_label` / `display_detail` render a call for a
  timeline row; the defaults call into `naming::humanize_tool_name` and
  `naming::context_detail_from_args`.
- **Host escape hatch**: `host_extension` / `host_call_extension` are type-erased
  (`&(dyn Any + Send + Sync)`), because the answer is host policy this crate has
  no business naming — a pack registry handle, a generated-tool provenance
  record. A host downcasts to its own type; every other tool returns `None` and
  pays nothing.

## Two consequences worth knowing before implementing a tool

- **A tool exposing several actions at different privileges should declare the
  *minimum* any of them needs** from `permission_level`, and the exact one from
  `permission_level_with_args`. Declaring the maximum statically blocks the
  tool for callers that could legitimately run its read-only half.
- **The argument-aware variants (`permission_level_with_args`,
  `external_effect_with_args`) are the ones a host calls at its enforcement
  point.** Overriding only the argument-less variant on a tool whose
  classification depends on its arguments leaves the per-call case unhandled.
