# TinyTools Code

Pure, bounded syntax analysis and boilerplate rendering. The engine accepts
caller-provided UTF-8 and an explicit Rust, TypeScript, TSX, JavaScript, Python,
Go, JSON or TOML grammar. It performs no filesystem or network access.

`CodeEngine::analyze` returns lexical declarations/references, syntax diagnostics,
content revisions and a folded view. Cache outcomes disclose fresh, unchanged or
incremental parsing. Cache byte limits measure retained source, excluding native
tree allocations. A language hint can be obtained with `Language::from_path`.

`CodeEngine::patch` requires an exact expected revision. Operations replace bodies
or whole declarations, insert imports, append members and wrap one selected node.
Original and resulting code must parse. Exact IDs identify a revision and source
range. Name selectors must be provably unique; truncated outlines only admit IDs.
No operation promises typechecking, semantic name resolution or behavior changes
equivalent to a compiler-assisted refactor.

`ChunkCatalog::builtin` contains twelve patterns. Search and inspect are metadata
operations. Templates are separate and `builtin_source` only serves genuine
built-in entries. Custom template paths are portable relative paths, with actual
filesystem containment enforced by the native adapter.

`render_chunk` expands typed `{{parameter}}` values and original standalone
`// @slot name`, `# @slot name` or `/* @slot name */` syntax comments once. It
refuses unknown, missing, duplicate or unused declarations. Supplied text is not
rescanned for directives. Parameter kinds are conservative ASCII identifiers,
uppercase-leading identifiers, syntax-checked type expressions, escaped strings,
JSON numbers and booleans. Required slots need nonempty code; optional slots can
be empty. A slot can require exactly one grammar node kind. Forbidden node kinds
apply to generated code, excluding artificial validation wrappers.

File chunks validate as complete sources. Body/member chunks use explicit syntax
contexts. Rust members can be fields or impl items, Go members can be fields or
interface items, JavaScript/TypeScript members use classes, and Python members
use class suites. JSON and TOML support file chunks only. Dependency declarations
are metadata and never install packages.

Default source/result limit: 1 MiB. Cache: 32 entries and 8 MiB of source.
Symbols/references: 2048 each. Diagnostics: 64. Manifests: 64 KiB; 32 parameters,
32 slots and 512 catalog entries. Configured limits have hard caps. All expansion
paths check limits before output growth, including indentation.

The optional `tinytools-std` `code` feature provides FsGate-backed native tools.
Host permission, preview, result paging and atomic publication belong there.

## Validation

Run the repository's four contract commands and 90% per-file coverage gate.
Focused tests use `cargo test -p tinytools-code`. Run the fixed-fixture benchmark
with `cargo bench -p tinytools-code --bench structural`. It reports byte counts
and cold/cache/incremental timings, without token-savings or speed claims.
