//! Manifest shape, portable paths and typed metadata budgets.

use super::{ChunkManifest, ParameterKind};
use crate::{
    CodeError, Language,
    error::{enforce_limit, invalid},
};
use std::collections::BTreeSet;

pub(crate) fn identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn text(value: &str, resource: &'static str, limit: usize) -> Result<(), CodeError> {
    enforce_limit(resource, value.len(), limit)?;
    if value.contains('\0') {
        return Err(invalid("metadata cannot contain NUL"));
    }
    Ok(())
}

fn named(value: &str) -> Result<(), CodeError> {
    text(value, "binding name bytes", 128)?;
    if !identifier(value) {
        return Err(invalid("binding names must be ASCII identifiers"));
    }
    Ok(())
}

fn node_kind(language: Language, value: &str) -> Result<(), CodeError> {
    named(value)?;
    let grammar = language.grammar();
    let id = grammar.id_for_node_kind(value, true);
    if grammar.node_kind_for_id(id) != Some(value)
        || !grammar.node_kind_is_named(id)
        || !grammar.node_kind_is_visible(id)
    {
        return Err(invalid(format!(
            "constraint {value} is not a visible, named node kind in {language:?}"
        )));
    }
    Ok(())
}

fn template_path(value: &str) -> Result<(), CodeError> {
    text(value, "template path bytes", 4096)?;
    if value.contains(['\\', ':'])
        || value.split('/').any(|part| {
            part.is_empty() || part == "." || part == ".." || part.ends_with(['.', ' '])
        })
    {
        return Err(invalid(
            "template_path must be a portable, relative path without traversal or empty segments",
        ));
    }
    Ok(())
}

pub(super) fn validate(manifest: &ChunkManifest) -> Result<(), CodeError> {
    text(&manifest.id, "chunk ID bytes", 128)?;
    if manifest.id.is_empty()
        || !manifest
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(invalid(
            "chunk ID must contain only ASCII letters, digits, dots, dashes or underscores",
        ));
    }
    text(&manifest.summary, "summary bytes", 1024)?;
    if manifest.summary.trim().is_empty() {
        return Err(invalid("chunk summary must not be empty"));
    }
    template_path(&manifest.template_path)?;
    enforce_limit("parameters", manifest.parameters.len(), 32)?;
    enforce_limit("slots", manifest.slots.len(), 32)?;
    enforce_limit("tags", manifest.tags.len(), 32)?;
    enforce_limit("dependencies", manifest.dependencies.len(), 64)?;
    enforce_limit("forbidden node kinds", manifest.forbidden_nodes.len(), 32)?;
    let mut names = BTreeSet::new();
    for parameter in &manifest.parameters {
        named(&parameter.name)?;
        if !names.insert(&parameter.name) {
            return Err(invalid("duplicate parameter name"));
        }
        text(&parameter.description, "parameter description bytes", 1024)?;
        if let Some(value) = &parameter.default {
            let valid = match parameter.kind {
                ParameterKind::Identifier | ParameterKind::PascalCaseIdent => {
                    value.as_str().is_some_and(|value| {
                        identifier(value)
                            && (parameter.kind != ParameterKind::PascalCaseIdent
                                || value.as_bytes()[0].is_ascii_uppercase())
                    })
                }
                ParameterKind::TypeExpr | ParameterKind::String => value.is_string(),
                ParameterKind::Number => value.is_number(),
                ParameterKind::Boolean => value.is_boolean(),
            };
            if !valid {
                return Err(invalid(
                    "parameter default does not match its declared kind",
                ));
            }
            text(&value.to_string(), "parameter default bytes", 4096)?;
        }
    }
    names.clear();
    for slot in &manifest.slots {
        named(&slot.name)?;
        if !names.insert(&slot.name) {
            return Err(invalid("duplicate slot name"));
        }
        text(&slot.description, "slot description bytes", 1024)?;
        if let Some(kind) = &slot.node_kind {
            node_kind(manifest.language, kind)?;
        }
    }
    for tag in &manifest.tags {
        text(tag, "tag bytes", 128)?;
    }
    for dependency in &manifest.dependencies {
        text(dependency, "dependency bytes", 256)?;
    }
    for kind in &manifest.forbidden_nodes {
        node_kind(manifest.language, kind)?;
    }
    let serialized = serde_json::to_string(manifest).map_err(|error| invalid(error.to_string()))?;
    enforce_limit("manifest bytes", serialized.len(), 65_536)
}
