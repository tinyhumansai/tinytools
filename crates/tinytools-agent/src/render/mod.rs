//! Everything the model *reads*: the catalogue, the protocol block, and the
//! envelope tool results come back in.
//!
//! Rendering and parsing are two faces of one protocol, and this module is
//! the rendering face for every dialect. A catalogue that advertises one
//! grammar next to a parser that expects another is a silent whole-turn
//! failure, so the protocol text lives here, once, and
//! [`crate::dialect`] only chooses which block to use.
//!
//! * [`catalogue`] — the tool list, as signatures or as full schemas;
//! * [`instructions`] — the protocol block for each dialect;
//! * [`calls`] — a structured call written back as `<tool_call>` markup for
//!   replay to a prompt-guided model;
//! * [`results`] — the `<tool_result>` envelope and transcript replay for
//!   the text dialects, with the boundary-integrity rules that keep a tool
//!   output from forging protocol structure.

pub mod calls;
pub mod catalogue;
pub mod instructions;
pub mod results;

pub use calls::{render_json_call, render_json_calls};
pub use catalogue::{
    CATALOGUE_HEADING, render_code_catalogue, render_json_catalogue, render_pformat_catalogue,
};
pub use instructions::{
    code_instructions, json_instructions, native_instructions, pformat_instructions,
};
pub use results::{
    TOOL_RESULTS_PREFIX, format_results, parse_replayed_results, to_provider_messages,
};
