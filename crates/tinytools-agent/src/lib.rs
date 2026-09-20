//! Agent-facing tool-call protocols.
//!
//! `tinytools` owns the vocabulary a callable tool exposes. This crate owns the
//! model-facing protocol around those declarations — and owns it **once**, for
//! every consumer:
//!
//! * [`parse`] turns model text back into calls, through every surface syntax
//!   a model has been seen to use;
//! * [`repair`] recovers damaged JSON, damaged tool names, and mis-shaped
//!   arguments after a call has been located;
//! * [`stream`] scrubs the same markup from a live text stream;
//! * [`render`] produces what the model reads: the catalogue, the protocol
//!   block, the result envelope;
//! * [`dialect`] binds one rendering to one parser so they cannot drift.
//!
//! A model that supports native tool use hands back structured calls and only
//! [`dialect::NativeDialect`] is involved. Everything else — prompt-guided
//! models, local models, providers whose native mode is unavailable, and
//! native models that narrate a call as text anyway — goes through the rest.
//!
//! ## What the host still owns
//!
//! This crate takes **schemas**, never a tool trait object, and never executes
//! anything. Permission checks, sandboxing, approval gates, timeouts, the
//! unknown-tool policy, and the minting of call ids are the host's, where they
//! can be audited. See [`parse`] for the bounds on how forgiving the parsers
//! are and why.

pub mod dialect;
pub mod parse;
pub(crate) mod pformat;
pub mod render;
pub mod repair;
pub mod stream;
mod telemetry;
pub mod types;

/// The tool vocabulary this crate renders and parses against, re-exported so
/// a consumer that only speaks the protocol need not name `tinytools` itself.
pub use tinytools;

pub use parse::{
    extract_json_values, parse_arguments_value, parse_glm_style_tool_calls, parse_text,
    parse_tool_call_value, parse_tool_calls, parse_tool_calls_from_json_value,
    parse_tool_calls_with_pformat,
};
pub use pformat::{
    PFormatParamType, PFormatRegistry, PFormatToolParams, build_registry, parse_call,
    render_signature, render_signature_from_schema,
};
pub use stream::{StreamScrubber, StreamStep};
pub use types::{CallSource, ParseDiagnostic, ParseOptions, ParseOutcome, ParsedToolCall};
