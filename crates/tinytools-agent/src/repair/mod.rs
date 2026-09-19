//! Repairs applied to what a model wrote *after* a call has been located.
//!
//! Locating a call and reading it are separate problems. The grammars in
//! [`crate::parse`] answer "where is the call and what did the model write
//! there"; this module answers "what did it mean" when the answer is not
//! strict JSON, not the exact tool name, or not the argument shape the schema
//! declares. Keeping the two apart is what lets every grammar share one repair
//! path, so a fix for a Kimi quote sentinel helps a DSML block equally.
//!
//! Three concerns, three submodules:
//!
//! * [`json`] — a JSON object from relaxed or damaged text;
//! * [`name`] — the offered tool a damaged name refers to;
//! * [`args`] — the argument object's shape against its schema.

pub mod args;
pub mod json;
pub mod name;

#[cfg(test)]
mod test;
