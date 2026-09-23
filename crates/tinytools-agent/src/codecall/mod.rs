//! Code-style tool calls: `read_file(path="src/main.rs", limit=20)`.
//!
//! # Why
//!
//! A JSON schema is the right wire format for native tool calling and a poor
//! thing to paste into a prompt; P-Format is compact but invented, and a
//! small model has seen none of it in pretraining. A function signature is
//! both compact *and* the one form every code-trained model already writes
//! fluently:
//!
//! ```text
//! def read_file(path: str, limit: int = None) -> str  # Read a file
//! ```
//!
//! is the whole catalogue entry, and the call the model writes back is plain
//! Python (or TypeScript — one grammar reads both).
//!
//! # Spec
//!
//! - One or more calls per `<tool_call>` body, separated by newlines or `;`.
//! - A call is `NAME(args)`, optionally prefixed by `await`, `x =`, or
//!   `const x =`. Arguments are positional (bound in the catalogue's order —
//!   required first, then optional alphabetically, from
//!   [`PFormatToolParams::from_schema`]), keyword (`name=value`), or both;
//!   positional after keyword is refused as in Python.
//! - Values are literals only: quoted strings (both quote styles, triple
//!   quotes, backticks, `r"…"`), numbers, `True`/`true`, `False`/`false`,
//!   `None`/`null`, lists, dicts. A bare identifier is a variable, and a call
//!   that needs one cannot run — refused.
//! - The TypeScript single-object form `f({a: 1, b: 2})` is unpacked into
//!   keyword arguments when every key is a declared parameter; otherwise, when
//!   the tool has exactly one parameter, the object is that parameter's value;
//!   otherwise refused.
//! - A top-level `None`/`null` omits the argument, the same retraction rule
//!   P-Format uses for an empty slot.
//! - **All or nothing per body.** Every non-blank, non-comment statement must
//!   be a call to a tool in the registry. Prose in the tag — `I'll call
//!   read_file(path="x") now` — is not a call, and a body containing it is
//!   refused whole; arguments are never lifted out of narrative.
//! - Refused, never guessed: unknown tool, unknown keyword, duplicate binding,
//!   positional overflow, unbalanced brackets, trailing tokens after `)`.
//!
//! # Where it sits
//!
//! The grammar has no marker of its own. It decodes the body of the tagged
//! grammar (`<tool_call>`, ```` ```tool_call ````) after P-Format has declined
//! and before the JSON paths, so every consumer — batch, streaming, every
//! dialect — sees it, and a top-level ```` ```python ```` fence stays what it
//! is everywhere else in this crate: an example, not a call.

mod literal;
mod signature;
mod types;

#[cfg(test)]
mod test;

use serde_json::{Map, Value};

use crate::pformat::{PFormatParamType, PFormatRegistry, PFormatToolParams, coerce_value};
use literal::Cursor;
use types::{Call, Literal, Refuse};

/// Positional and keyword arguments as parsed, before binding.
type Arguments = (Vec<Literal>, Vec<(String, Literal)>);

pub use signature::{render_code_signature, render_code_type};
pub use types::CodeStyle;

/// Reads every call in a `<tool_call>` body, or none.
///
/// Returns `(tool_name, arguments)` pairs in source order, bound and coerced
/// against `registry`. Empty when the body is not entirely code calls to
/// known tools — see the module docs for what is and is not accepted.
#[must_use]
pub fn parse_calls(body: &str, registry: &PFormatRegistry) -> Vec<(String, Value)> {
    parse_all(body, registry).unwrap_or_default()
}

fn parse_all(body: &str, registry: &PFormatRegistry) -> Result<Vec<(String, Value)>, Refuse> {
    let mut cursor = Cursor::new(body);
    let mut calls = Vec::new();
    loop {
        cursor.skip_trivia();
        if cursor.at_end() {
            break;
        }
        let call = statement(&mut cursor)?;
        let (name, params) = lookup(registry, &call.name)?;
        let arguments = bind(call, params)?;
        calls.push((name.to_string(), Value::Object(arguments)));
    }
    if calls.is_empty() {
        return Err(Refuse);
    }
    crate::telemetry::debug!(
        calls = calls.len(),
        "[codecall] parsed code-style tool calls"
    );
    Ok(calls)
}

/// One statement: optional prefixes, then `NAME(args)`, then end of line.
fn statement(cursor: &mut Cursor<'_>) -> Result<Call, Refuse> {
    let name = callee(cursor)?;
    if cursor.bump() != Some('(') {
        return Err(Refuse);
    }
    let (positional, keywords) = arguments(cursor)?;
    // Only trivia may follow the call on its line; `f(1) + 1` is an
    // expression, not a call.
    cursor.skip_inline_trivia();
    match cursor.peek() {
        None | Some('\n' | ';') => Ok(Call {
            name,
            positional,
            keywords,
        }),
        Some(_) => Err(Refuse),
    }
}

/// Reads the callee, skipping `await`, `const|let|var x =`, and `x =`
/// prefixes. Stops with the cursor on the `(`.
fn callee(cursor: &mut Cursor<'_>) -> Result<String, Refuse> {
    loop {
        cursor.skip_inline_trivia();
        let word = cursor.identifier().ok_or(Refuse)?;
        cursor.skip_inline_trivia();
        match word {
            "await" => continue,
            "const" | "let" | "var" => {
                cursor.identifier().ok_or(Refuse)?;
                cursor.skip_inline_trivia();
                if !cursor.at_single_equals() {
                    return Err(Refuse);
                }
                cursor.bump();
                continue;
            }
            _ => {}
        }
        if cursor.at_single_equals() {
            cursor.bump();
            continue;
        }
        let mut name = word.to_string();
        while cursor.eat(".") {
            name.push('.');
            name.push_str(cursor.identifier().ok_or(Refuse)?);
        }
        cursor.skip_inline_trivia();
        return match cursor.peek() {
            Some('(') => Ok(name),
            _ => Err(Refuse),
        };
    }
}

/// The argument list after `(`, through the closing `)`.
fn arguments(cursor: &mut Cursor<'_>) -> Result<Arguments, Refuse> {
    let mut positional = Vec::new();
    let mut keywords: Vec<(String, Literal)> = Vec::new();
    loop {
        cursor.skip_trivia();
        if cursor.peek() == Some(')') {
            cursor.bump();
            return Ok((positional, keywords));
        }
        if let Some(key) = keyword_name(cursor) {
            if keywords.iter().any(|(existing, _)| *existing == key) {
                return Err(Refuse);
            }
            keywords.push((key, cursor.literal()?));
        } else {
            if !keywords.is_empty() {
                return Err(Refuse);
            }
            positional.push(cursor.literal()?);
        }
        cursor.skip_trivia();
        match cursor.bump() {
            Some(',') => {}
            Some(')') => return Ok((positional, keywords)),
            _ => return Err(Refuse),
        }
    }
}

/// `name=` at the cursor, consumed only when it really is a keyword argument;
/// otherwise the cursor is left where it was so the value can be read as a
/// positional literal (`True`, `None`, …).
fn keyword_name(cursor: &mut Cursor<'_>) -> Option<String> {
    let probe = cursor.rest();
    let mut lookahead = Cursor::new(probe);
    let word = lookahead.identifier()?;
    lookahead.skip_inline_trivia();
    if !lookahead.at_single_equals() {
        return None;
    }
    lookahead.bump();
    let consumed = probe.len() - lookahead.rest().len();
    // The lookahead walked a prefix of `probe`; consume the same bytes here.
    cursor.eat(&probe[..consumed]);
    Some(word.to_string())
}

/// Resolves the callee against the registry, falling back to the last dotted
/// segment (`functions.read_file` → `read_file`).
fn lookup<'r>(
    registry: &'r PFormatRegistry,
    name: &str,
) -> Result<(&'r str, &'r PFormatToolParams), Refuse> {
    if let Some((key, params)) = registry.get_key_value(name) {
        return Ok((key.as_str(), params));
    }
    if let Some(last) = name.rsplit('.').next()
        && last != name
        && let Some((key, params)) = registry.get_key_value(last)
    {
        return Ok((key.as_str(), params));
    }
    crate::telemetry::debug!(name_len = name.len(), "[codecall] unknown tool — refusing");
    Err(Refuse)
}

/// Binds positional and keyword arguments to parameter names and coerces
/// them to the schema's primitive types.
fn bind(call: Call, params: &PFormatToolParams) -> Result<Map<String, Value>, Refuse> {
    let Call {
        positional,
        keywords,
        ..
    } = call;
    let (positional, mut keywords) = unpack_single_object(positional, keywords, params)?;

    let mut bound: Vec<(String, Literal)> = Vec::with_capacity(positional.len() + keywords.len());
    for (slot, value) in positional.into_iter().enumerate() {
        let name = params.names.get(slot).ok_or(Refuse)?;
        bound.push((name.clone(), value));
    }
    for (key, value) in keywords.drain(..) {
        if !params.names.contains(&key) {
            crate::telemetry::debug!(key_len = key.len(), "[codecall] unknown keyword — refusing");
            return Err(Refuse);
        }
        if bound.iter().any(|(name, _)| *name == key) {
            return Err(Refuse);
        }
        bound.push((key, value));
    }

    let mut out = Map::with_capacity(bound.len());
    for (name, value) in bound {
        if value == Literal::Null {
            // A top-level `None` is the model saying "not sending this one".
            continue;
        }
        let ty = params
            .names
            .iter()
            .position(|n| *n == name)
            .and_then(|slot| params.types.get(slot).copied())
            .unwrap_or(PFormatParamType::String);
        let json = match value {
            Literal::Str(raw) => coerce_value(&raw, ty),
            other => Value::from(other),
        };
        out.insert(name, json);
    }
    Ok(out)
}

/// The TypeScript single-object rule: `f({a: 1})` is keyword arguments when
/// every key is a parameter, that one parameter's value when the tool has
/// exactly one, and otherwise refused — binding a foreign-keyed object to
/// whichever parameter happens to come first would be a guess.
fn unpack_single_object(
    positional: Vec<Literal>,
    keywords: Vec<(String, Literal)>,
    params: &PFormatToolParams,
) -> Result<Arguments, Refuse> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Ok((positional, keywords));
    }
    let Some(Literal::Dict(entries)) = positional.first() else {
        return Ok((positional, keywords));
    };
    let all_declared = entries
        .iter()
        .all(|(key, _)| params.names.iter().any(|name| name == key));
    if (!entries.is_empty() || params.names.len() != 1) && all_declared {
        let Some(Literal::Dict(entries)) = positional.into_iter().next() else {
            return Ok((Vec::new(), Vec::new()));
        };
        return Ok((Vec::new(), entries));
    }
    if params.names.len() == 1 {
        return Ok((positional, keywords));
    }
    crate::telemetry::debug!(
        keys = entries.len(),
        "[codecall] object argument names no parameter — refusing"
    );
    Err(Refuse)
}
