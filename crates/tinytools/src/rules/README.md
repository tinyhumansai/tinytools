# `rules`: declarative tool rules

Allow, deny, hide and approval rules over tools, written by a host and
evaluated by a harness on every surface a tool reaches the model on.
Specification: [`docs/specs/tool-rules.md`](../../../../docs/specs/tool-rules.md).

## Design

A host restricts tools from many places, such as an agent's allowlist, a
channel's permission ceiling, an MCP server filter or a connector's curated
actions. This module gives all of them one vocabulary. It holds **no policy**:
the rules are data the host writes, and evaluation is mechanical. That is the
same line `deferral` draws.

| File | Holds |
|---|---|
| `glob.rs` | `glob_matches`: `*` and `?`, ASCII case-insensitive, with no dependency |
| `types.rs` | `ToolRule`, `ToolMatcher`, `ArgMatcher`, `ToolRules`, `ToolRuleSet`, `RuleContext`, `RuleDecision`, `RuleRef`, `ApprovalDirective` |
| `subject.rs` | `ToolSubject`: what is evaluated, built from a live tool or by hand |
| `eval.rs` | Evaluation of a rule, a layer, a set, and a concrete call |

## Public surface

- **`ToolRule`** has an effect (`allow`, `deny`, `hide`, `require_approval`,
  `auto_approve`), the surfaces it applies on (`catalog`, `search`, `call`), a
  `match`, an optional `except` carve-out, `when` context conditions, and an
  `id` and `reason` for refusals.
- **`ToolRules`** is one layer with a default. Inside a layer, effects combine
  without regard to order:
  - `deny` wins.
  - With a `deny` default, a tool needs a matching `allow`.
  - `hide` only clears visibility on the listing surfaces.
  - `require_approval` beats `auto_approve`.
- **`ToolRuleSet`** stacks layers, and every layer must admit a tool. Adding a
  layer can only narrow, so two allowlists intersect.
- **`ToolRuleSet::evaluate_call`** evaluates a call with the tool's
  argument-aware permission. When the tool reports an `indirect_target`, an
  `IndirectCall { target, arguments }`, it evaluates that target too. It uses
  the target's own arguments when the dispatcher wraps them in an envelope, so
  an argument-scoped rule cannot be sidestepped through the dispatcher.

## Operational constraints

- A matcher field naming an attribute the subject does not know does not
  match. That fails open for `deny` and closed for `allow`, so evaluate
  against the live tool wherever you have one.
- Off the call surface an `arg` condition cannot be decided. An `allow` reads
  it optimistically and every other effect does not apply.
- A wrapper tool must forward `tags` and `indirect_target`, as `SharedTool`
  does. Otherwise tag rules miss and a dispatcher's target escapes its rules.
