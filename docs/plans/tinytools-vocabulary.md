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

## Task 6: Full verification

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
