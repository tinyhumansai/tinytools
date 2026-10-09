# Tool rules

- **Status:** Implemented
- **Owner:** Maintainers

## Problem

Hosts restrict tools in many places: an agent definition's allowlist, a
channel's permission ceiling, an MCP server's tool filter, a connector's
curated actions, a user's toggles, an approval allowlist. Each carried its own
exact-name matcher and was applied on whichever surface its author thought of,
so a tool denied by call-time middleware could still be found through tool
search, and a wildcard worked in one list but not the next.

## Goals

- One serializable vocabulary for allow / deny / hide / approval decisions
  over tools, with glob patterns.
- The same rules decide every surface a tool reaches the model on: the
  up-front catalogue, on-demand search, and the call.
- Independent sources compose without one widening another.
- A dispatcher tool (a connector's generic execute tool, a skill runner)
  cannot be used to reach a target a rule forbids.

## Non-goals

- Choosing a policy. Rules are data the host writes from its own
  configuration; this crate only evaluates them mechanically, the same way
  `deferral` subtracts what a tool declares.
- Enforcement. The harness calls the evaluator at its listing and admission
  points and reports a refusal.
- Stateful decisions (rate limits, budgets). Those stay host middleware.
- Argument, path and sandbox safety floors.

## Proposed behavior

- `ToolRule { id, effect, on, match, except, when, reason }`.
  - `effect`: `allow`, `deny` (not listed, not callable), `hide` (not listed
    or searchable, still callable), `require_approval`, `auto_approve`.
  - `on`: `catalog`, `search`, `call`; empty means all three.
  - `match` (`ToolMatcher`, every set field must match): `name`, `family`,
    `tags` (globs, a string or a list); `category`; `exposure`;
    `permission_at_least` / `permission_at_most`; `side_effects` (any of);
    `arg { pointer, value }`.
  - `except`: a carve-out matcher.
  - `when`: context attributes (`channel`, `agent`, …) that must be present
    and match.
- `ToolRules { name, default, rules }` is a layer. Effects combine without
  regard to order: `deny` beats everything; with `default = "deny"` a tool
  needs an `allow`; `hide` clears visibility only; `require_approval` beats
  `auto_approve`.
- `ToolRuleSet` stacks layers; every layer must admit, and the strictest
  approval directive wins.
- `ToolSubject` is what is evaluated: `ToolSubject::of(&dyn Tool)` or built by
  hand. `RuleDecision { visible, callable, approval, blocked_by }` names the
  layer and rule behind a refusal; `refusal(tool)` renders it for the model.
- `ToolRuleSet::evaluate_call(tool, ctx, args)` evaluates the tool with its
  argument-aware permission and, when `Tool::indirect_target` names a target,
  the target too.
- `Tool` gains two defaulted, descriptive methods: `tags()` and
  `indirect_target(args)`.

Globs support `*` and `?` and ignore ASCII case, so `gmail_*` matches the
upper-case Composio slug `GMAIL_SEND_EMAIL`.

## Invariants and constraints

- Adding a rule layer can only narrow what is admitted.
- A matcher field naming an attribute the subject does not know does not
  match: fail-open for `deny`, fail-closed for `allow`. Hosts evaluate against
  the live tool wherever they have one.
- Off the call surface an `arg` condition cannot be decided: an `allow` reads
  it optimistically (some call may be allowed, so the tool stays listed) and
  every other effect does not apply until there is a call.
- No new dependency: the crate stays dependency-light.

## Acceptance criteria

The unit tests in `crates/tinytools/src/rules/mod_tests.rs` pin precedence,
surfaces, `except`, `when`, argument handling, layering, indirect targets,
refusal text and the serde wire form.
