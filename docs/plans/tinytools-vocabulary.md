# Plan: TinyTools vocabulary crate

- **Status:** Implemented
- **Specification:**
  [`../specs/tinytools-vocabulary.md`](../specs/tinytools-vocabulary.md)

This plan documents the implementation sequence actually followed
(post-hoc, since the crate was reshaped from `rust-template` in one change and
this document is being added to satisfy the repository's spec-then-plan
convention for an already-landed public contract). Use it as the reference
sequence for the next module added to this crate.

## Task 1: Establish the workspace and remove the template's module surface

**Files:** `Cargo.toml`, `.gitmodules`, `crates/template*`, `.github/workflows/ci.yml`, `AGENTS.md`

1. Repoint the virtual workspace at `crates/tinytools`, removing the TinyBus
   module template (`crates/template`, `crates/template-bus`,
   `vendor/tinybus`) and its release workflow — a `Tool` is an async trait
   returning `anyhow::Result` and cannot cross a bus wire, so the module half
   of the template does not apply here.
2. Replace the dependency-light CI check's target crate and forbidden-name
   list with `tinytools` and this crate's actual constraints (no harness, no
   transport, no async runtime beyond the `async-trait` shim).

## Task 2: Add the core tool vocabulary contracts

**Files:** `crates/tinytools/Cargo.toml`,
`crates/tinytools/src/{call,classification,permission,result,spec}/*`

1. Add `ToolCallOptions` / `ToolTimeout` (`call`), `ToolScope` / `ToolCategory`
   (`classification`), `PermissionLevel` (`permission`), `ToolResult` /
   `ToolContent` (`result`), and `ToolSpec` (`spec`) — each as a
   `mod.rs` / `types.rs` / `test.rs` triple.
2. Pin every serializable type's wire shape: not just a round-trip (which only
   proves the encoder and decoder still agree with each other after a rename),
   but the literal encoded JSON in both directions.
3. Run `cargo test` after each module and `cargo clippy --all-targets --all-features -- -D warnings`.

## Task 3: Add the execution context and workspace contracts

**Files:** `crates/tinytools/src/{context,workspace}/*`

1. Add `ToolRunContext`, a narrow trait erasing a harness's run-scoped context,
   with a trait-object test proving a real implementor is reachable through
   it.
2. Add `WorkspaceDescriptor` / `SandboxMode`, with `allows` implemented as a
   lexical, non-canonicalizing containment check — anchored to the current
   working directory for a relative path or root, normalizing `.`/`..`
   components without touching the filesystem.
3. Add tests for parent-traversal spoofing, filesystem-root traversal, and the
   lexical-vs-canonicalizing tradeoff documented on `allows` itself.
4. Add `crates/tinytools/src/workspace/README.md` covering the module's design
   and the symlink-resolution limitation explicitly, since AGENTS.md requires
   a module README for complex modules.

## Task 4: Add the `Tool` trait and display integration

**Files:** `crates/tinytools/src/{tool,naming}/*`

1. Define `Tool` with four required methods and defaulted declarations
   layered so each forwards to the next (`execute` ← `execute_with_options` ←
   `execute_with_context`).
2. Add `humanize_tool_name` and `context_detail_from_args` /
   `context_detail_from_args_with` in `naming`, with tests covering prefix
   stripping, title-casing, key-scanning precedence, trimming, and the
   empty-value/zero-cap edge cases that must yield `None` rather than an
   empty `Some`.
3. Add `crates/tinytools/src/tool/README.md` covering the trait's public
   surface and the two argument-aware-vs-argument-less override rules.

## Task 5: Publish the crate surface and project guidance

**Files:** `crates/tinytools/src/lib.rs`, `README.md`, `AGENTS.md`, `CONTRIBUTING.md`, `deny.toml`

1. Re-export the public surface from `src/lib.rs` with a crate-level overview,
   a runnable doctest, and an explicit "what is deliberately not here"
   section.
2. Point `crates/tinytools/Cargo.toml`'s `readme` at the actual README (the
   repo root's, since this is a single-crate workspace) and verify with
   `cargo package --list -p tinytools`.
3. Retarget every repository-identity reference (`CONTRIBUTING.md`,
   `.github/ISSUE_TEMPLATE/config.yml`, `docs/README.md`) from the
   `rust-template` origin to `tinyhumansai/tinytools`, and remove any
   remaining template-only instructions (a deleted example, a nonexistent
   error-type variant) rather than leaving them to bit-rot.

## Task 7: Rich `ToolResult`, replay classification, and a host escape hatch

**Files:** `crates/tinytools/src/result/*`, `crates/tinytools/src/policy/*`,
`crates/tinytools/src/context/*`, `crates/tinytools/README.md`,
`docs/specs/tinytools-vocabulary.md`

This task documents the sequence actually followed (post-hoc, added while
addressing review feedback on the pull request that landed it), for the
extension specified in
[`../specs/tinytools-vocabulary.md`](../specs/tinytools-vocabulary.md#extension-rich-results-replay-classification-and-a-host-escape-hatch).

1. Add `ToolContent::Image` / `ToolContent::File` and their `ImageData` /
   `FileData` payload types, each literal-wire tested for both directions.
2. Add `ToolResult::follow_up`, `::metadata`, `::control: Option<ToolControl>`,
   and `::error_kind: Option<ToolErrorKind>`, each `#[serde(default)]` and, for
   the ones that can be empty or absent, `skip_serializing_if`, so a
   `ToolResult` persisted before these fields existed still decodes and a
   plain result's wire shape is unchanged.
3. Add `ToolControl` (`return_direct`, `terminate`, `goto`, `state_update`)
   and the builders `return_direct()`, `terminate()`, `with_goto(..)`,
   `with_state_update(..)` that lazily create it.
4. Add `ToolReplay` and `ToolRuntime::replay`, defaulting to `Never`.
5. Add `ToolRunContext::host_extension`, defaulting to `None`.
6. Bump `[workspace.package].version` from `0.2.0` to `0.3.0`: the new fields
   on public structs (`ToolResult`, `ToolRuntime`) are additive on the wire
   but break an external struct literal that does not use `..Default::default()`
   or `..Self::default()`, which `AGENTS.md`'s versioning policy treats as a
   non-additive, minor-bump-worthy change pre-1.0.
7. Fix `ToolControl::return_direct` to `Option<bool>` (review finding, both
   CodeRabbit and Codex): a call that only used `with_goto`,
   `with_state_update`, or `terminate` created a `ToolControl` whose
   `return_direct` defaulted to `false`, so a harness following the
   documented "prefer the per-call value" rule would silently suppress a
   tool's static `true` declaration even though the call never touched
   `return_direct`. Add `dont_return_direct()` as the explicit `Some(false)`
   builder, and add regression tests asserting the field stays `None` when no
   builder touches it.
8. Update `crates/tinytools/README.md`'s "Static and per-call return-direct"
   section and field list to describe the tri-state semantics.

## Task 8: Full verification

All items below were run and passed locally as of this commit, and CI
re-verifies the same commands on every push:

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`
- [x] `cargo build --all-targets --all-features`
- [x] `cargo test --all-features`
- [x] `.github/scripts/check-file-coverage.sh 90 coverage.json` (≥ 90% per file)
- [x] `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features`
- [x] `cargo deny check all`
- [x] the dependency-light CI gate passes against the reviewed allowlist
