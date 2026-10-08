# Plan: Tool progress

- **Status:** Implemented
- **Specification:** [`../specs/tool-progress.md`](../specs/tool-progress.md)

## Goal

Add the progress vocabulary and the `ToolRunContext::report_progress` seam
without new dependencies or any transport.

## Task 1: Vocabulary

**Files:** `crates/tinytools/src/progress/{mod,types,mod_tests}.rs`

1. Write failing tests for builders, fraction clamping/`NaN`, and the wire form.
2. Implement `ToolProgress`.
3. Write a failing test for sink delivery and the no-op sink; implement
   `ProgressSink`.

## Task 2: Context seam

**Files:** `crates/tinytools/src/context/types.rs`

1. Write failing tests for a bare context (ignores) and a streaming context
   (receives through the trait object).
2. Add the default-bodied `report_progress`, documenting the non-blocking,
   re-entrancy and late-update rules.

## Task 3: Exports and docs

Export `progress` and its types from `crates/tinytools/src/lib.rs`; list the
module in `AGENTS.md`.

## Verification

`cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo test --workspace`, and the 90% per-file coverage gate.
