//! Pure, bounded structural code tools.
//!
//! [`CodeEngine`] parses explicitly selected grammars, produces lexical outlines
//! and applies revision-checked edits. [`ChunkCatalog`] and [`render_chunk`]
//! provide typed, one-pass boilerplate. Filesystem access and host permissions
//! belong to adapters: this crate performs no I/O or semantic typechecking.
//!
//! ```
//! use tinytools_code::{CodeEngine, Language};
//! let mut engine = CodeEngine::default();
//! let report = engine.analyze("example.rs", Language::Rust, "fn greet() {}")?;
//! assert_eq!(report.symbols[0].name, "greet");
//! # Ok::<(), tinytools_code::CodeError>(())
//! ```

mod analysis;
mod catalog;
mod engine;
mod error;
mod language;
mod patch;
mod render;

pub use analysis::{Analysis, Diagnostic, Reference, SourceRange, Symbol, select_symbol};
pub use catalog::{
    ChunkCatalog, ChunkManifest, ChunkParameter, ChunkSlot, ChunkSummary, ChunkTarget,
    ParameterKind,
};
pub use engine::{CacheStats, CacheStatus, CodeEngine, CodeLimits, revision};
pub use error::CodeError;
pub use language::Language;
pub use patch::{PatchOperation, PatchRequest, PatchResult};
pub use render::{ChunkBindings, RenderedChunk, render_chunk};
