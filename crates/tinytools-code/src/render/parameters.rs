//! Typed literal escaping and exact type-expression boundaries.

use crate::{
    ChunkBindings, ChunkManifest, CodeEngine, CodeError, Language, ParameterKind,
    catalog::identifier,
    error::{enforce_limit, invalid},
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Expanded {
    pub source: String,
    pub protected: Vec<(usize, usize)>,
}

pub(super) fn append(output: &mut String, text: &str, limit: usize) -> Result<(), CodeError> {
    enforce_limit(
        "rendered bytes",
        output.len().saturating_add(text.len()),
        limit,
    )?;
    output.push_str(text);
    Ok(())
}

fn type_expression(
    engine: &CodeEngine,
    language: Language,
    value: &str,
) -> Result<String, CodeError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid("type expression is empty"));
    }
    let (prefix, suffix) = match language {
        Language::Rust | Language::TypeScript | Language::Tsx => ("type __Chunk = ", ";"),
        Language::Python => ("value: ", "\n"),
        Language::Go => ("package chunk\ntype __Chunk ", "\n"),
        _ => {
            return Err(CodeError::UnsupportedOperation(
                "this grammar has no typed parameter context".to_owned(),
            ));
        }
    };
    let source = format!("{prefix}{value}{suffix}");
    let tree = engine.parse(language, &source)?;
    if tree.root_node().has_error() {
        return Err(CodeError::InvalidSyntax { language });
    }
    let exact = tree
        .root_node()
        .descendant_for_byte_range(prefix.len(), prefix.len() + value.len());
    if !exact.is_some_and(|node| {
        node.start_byte() == prefix.len() && node.end_byte() == prefix.len() + value.len()
    }) {
        return Err(invalid(
            "type expression must occupy exactly one syntax node",
        ));
    }
    Ok(value.to_owned())
}

fn value(
    engine: &CodeEngine,
    language: Language,
    kind: ParameterKind,
    value: &Value,
) -> Result<String, CodeError> {
    match kind {
        ParameterKind::Number => value
            .as_number()
            .map(ToString::to_string)
            .ok_or_else(|| invalid("number parameter needs a JSON number")),
        ParameterKind::Boolean => value
            .as_bool()
            .map(|value| {
                if language == Language::Python {
                    if value { "True" } else { "False" }
                } else if value {
                    "true"
                } else {
                    "false"
                }
                .to_owned()
            })
            .ok_or_else(|| invalid("boolean parameter needs a JSON boolean")),
        _ => {
            let value = value
                .as_str()
                .ok_or_else(|| invalid("text parameter needs a JSON string"))?;
            enforce_limit("parameter bytes", value.len(), 4096)?;
            match kind {
                ParameterKind::Identifier | ParameterKind::PascalCaseIdent => {
                    if !identifier(value)
                        || (kind == ParameterKind::PascalCaseIdent
                            && !value.as_bytes()[0].is_ascii_uppercase())
                    {
                        return Err(invalid("identifier parameter violates its declared kind"));
                    }
                    Ok(value.to_owned())
                }
                ParameterKind::TypeExpr => type_expression(engine, language, value),
                ParameterKind::String => {
                    if language == Language::Rust {
                        Ok(format!("\"{}\"", value.escape_default()))
                    } else {
                        serde_json::to_string(value).map_err(|error| invalid(error.to_string()))
                    }
                }
                _ => Err(invalid("text value has a non-text parameter kind")),
            }
        }
    }
}

pub(super) fn expand(
    engine: &CodeEngine,
    manifest: &ChunkManifest,
    template: &str,
    bindings: &ChunkBindings,
) -> Result<Expanded, CodeError> {
    enforce_limit("parameter bindings", bindings.parameters.len(), 32)?;
    enforce_limit("slot bindings", bindings.slots.len(), 32)?;
    for name in bindings.parameters.keys().chain(bindings.slots.keys()) {
        enforce_limit("binding name bytes", name.len(), 128)?;
    }
    for name in bindings.parameters.keys() {
        if !manifest
            .parameters
            .iter()
            .any(|parameter| parameter.name == *name)
        {
            return Err(invalid(format!("unknown parameter: {name}")));
        }
    }
    for name in bindings.slots.keys() {
        if !manifest.slots.iter().any(|slot| slot.name == *name) {
            return Err(invalid(format!("unknown slot: {name}")));
        }
    }
    let mut values = BTreeMap::new();
    for parameter in &manifest.parameters {
        let supplied = bindings
            .parameters
            .get(&parameter.name)
            .or(parameter.default.as_ref());
        if let Some(supplied) = supplied {
            values.insert(
                parameter.name.as_str(),
                value(engine, manifest.language, parameter.kind, supplied)?,
            );
        } else if parameter.required {
            return Err(invalid(format!("missing parameter: {}", parameter.name)));
        }
    }
    let limit = engine.limits().max_source_bytes;
    let mut source = String::new();
    let mut protected = Vec::new();
    let mut used = BTreeSet::new();
    let mut remaining = template;
    while let Some(start) = remaining.find("{{") {
        append(&mut source, &remaining[..start], limit)?;
        let marker = &remaining[start + 2..];
        let end = marker
            .find("}}")
            .ok_or_else(|| invalid("unterminated parameter marker"))?;
        let name = &marker[..end];
        enforce_limit("parameter marker bytes", name.len(), 128)?;
        let value = values
            .get(name)
            .ok_or_else(|| invalid(format!("missing or undeclared parameter: {name}")))?;
        let begin = source.len();
        append(&mut source, value, limit)?;
        enforce_limit("parameter occurrences", protected.len() + 1, 16_384)?;
        protected.push((begin, source.len()));
        used.insert(name);
        remaining = &marker[end + 2..];
    }
    append(&mut source, remaining, limit)?;
    if manifest
        .parameters
        .iter()
        .any(|parameter| !used.contains(parameter.name.as_str()))
    {
        return Err(invalid("declared parameter is unused by the template"));
    }
    Ok(Expanded { source, protected })
}
