# Plan: Tool rules

- **Status:** Implemented
- **Specification:** [`../specs/tool-rules.md`](../specs/tool-rules.md)

## Goal

Add a declarative, serializable rule language for tool visibility and call
admission, with no new dependency and no enforcement in this crate.

## Task 1: Pattern matching

**Files:** `crates/tinytools/src/rules/glob.rs`

1. Write failing tests for `*`, `?`, ASCII case-insensitivity and the empty
   pattern.
2. Implement `glob_matches` as a single-backtrack matcher.

## Task 2: Vocabulary

**Files:** `crates/tinytools/src/rules/{types,subject}.rs`

1. Write failing tests pinning the wire form: `Patterns` as a string or a list,
   snake_case effects and surfaces, `match` / `except` / `when` / `on`, and
   unknown matcher fields rejected.
2. Implement `ToolRule`, `ToolMatcher`, `ArgMatcher`, `ToolRules`, `ToolRuleSet`,
   `RuleContext`, `RuleDecision` and `ToolSubject`.
3. Derive serde on `ToolExposure` so a matcher can name it.

## Task 3: Evaluation

**Files:** `crates/tinytools/src/rules/eval.rs`

1. Write failing tests for each precedence rule:
   - deny beats allow
   - a default deny needs an allow
   - hide is listing-only
   - `require_approval` beats `auto_approve`
2. Write failing tests for surface scoping and for argument conditions off the
   call surface: optimistic for an allow, skipped for every other effect.
3. Write failing tests for layering (intersecting allowlists, the first
   refusal reported, the strictest approval) and for refusal text.
4. Implement `ToolRules::evaluate` and `ToolRuleSet::evaluate`.

## Task 4: Tool seams

**Files:** `crates/tinytools/src/tool/types.rs`, `crates/tinytools/src/shared/types.rs`

1. Write failing tests for `ToolSubject::of` / `of_call` and for
   `ToolRuleSet::evaluate_call` refusing a dispatcher's denied target.
2. Add the defaulted `Tool::tags` and `Tool::indirect_target`.
3. Write a failing test that `SharedTool` forwards both; implement the
   forwarding.

## Task 5: Exports and docs

1. Re-export from `lib.rs`.
2. Add the module README, the spec, the README module-table row and the
   AGENTS.md layout entry.
3. Run the four contract commands.
