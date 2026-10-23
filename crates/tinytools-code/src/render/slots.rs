//! Only original, standalone syntax comments can introduce code slots.

use super::parameters::{Expanded, append};
use crate::{
    ChunkBindings, ChunkManifest, CodeEngine, CodeError,
    analysis::node_text,
    error::{enforce_limit, invalid},
};
use std::collections::BTreeSet;

pub(super) struct SlotRange {
    pub name: String,
    pub start: usize,
    pub end: usize,
}

fn marker(text: &str) -> Option<&str> {
    let text = text.trim();
    let text = text
        .strip_prefix("//")
        .or_else(|| text.strip_prefix('#'))
        .or_else(|| {
            text.strip_prefix("/*")
                .and_then(|text| text.strip_suffix("*/"))
        })?
        .trim();
    let name = text.strip_prefix("@slot ")?.trim();
    (!name.is_empty() && !name.chars().any(char::is_whitespace)).then_some(name)
}

fn indent_binding(value: &str, indentation: &str, limit: usize) -> Result<String, CodeError> {
    let mut output = String::new();
    for (index, line) in value.trim_end().lines().enumerate() {
        let added = line
            .len()
            .saturating_add(if line.is_empty() {
                0
            } else {
                indentation.len()
            })
            .saturating_add(usize::from(index > 0));
        enforce_limit(
            "indented slot bytes",
            output.len().saturating_add(added),
            limit,
        )?;
        if index > 0 {
            output.push('\n');
        }
        if !line.is_empty() {
            output.push_str(indentation);
        }
        output.push_str(line);
    }
    Ok(output)
}

pub(super) fn expand(
    engine: &CodeEngine,
    manifest: &ChunkManifest,
    expanded: &Expanded,
    bindings: &ChunkBindings,
) -> Result<(String, Vec<SlotRange>), CodeError> {
    let template = &expanded.source;
    let tree = engine.parse(manifest.language, template)?;
    let mut stack = vec![tree.root_node()];
    let mut slots = Vec::new();
    while let Some(node) = stack.pop() {
        if node.kind().contains("comment")
            && !expanded
                .protected
                .iter()
                .any(|(start, end)| *start < node.end_byte() && *end > node.start_byte())
            && let Some(name) = marker(node_text(node, template))
        {
            let start = template[..node.start_byte()]
                .rfind('\n')
                .map_or(0, |offset| offset + 1);
            let end = template[node.end_byte()..]
                .find('\n')
                .map_or(template.len(), |offset| node.end_byte() + offset);
            let indent = &template[start..node.start_byte()];
            if indent.trim().is_empty() && template[node.end_byte()..end].trim().is_empty() {
                enforce_limit("slot name bytes", name.len(), 128)?;
                enforce_limit("slot occurrences", slots.len() + 1, 32)?;
                slots.push((start, node.end_byte(), name.to_owned(), indent.to_owned()));
            }
        }
        for index in 0..node.named_child_count() {
            if let Some(child) = node.named_child(index) {
                stack.push(child);
            }
        }
    }
    slots.sort_by_key(|(start, _, _, _)| *start);
    let mut used = BTreeSet::new();
    let mut output = String::new();
    let mut ranges = Vec::new();
    let mut copied = 0;
    let limit = engine.limits().max_source_bytes;
    for (start, end, name, indent) in slots {
        if !used.insert(name.clone()) {
            return Err(invalid("slot appears more than once"));
        }
        let slot = manifest
            .slots
            .iter()
            .find(|slot| slot.name == name)
            .ok_or_else(|| invalid(format!("undeclared slot: {name}")))?;
        let value = bindings.slots.get(&name).map_or("", String::as_str);
        if slot.required && value.trim().is_empty() {
            return Err(invalid(format!("missing required slot: {name}")));
        }
        enforce_limit("slot bytes", value.len(), limit)?;
        append(&mut output, &template[copied..start], limit)?;
        let begin = output.len();
        append(&mut output, &indent_binding(value, &indent, limit)?, limit)?;
        ranges.push(SlotRange {
            name,
            start: begin,
            end: output.len(),
        });
        copied = end;
    }
    append(&mut output, &template[copied..], limit)?;
    if manifest.slots.iter().any(|slot| !used.contains(&slot.name)) {
        return Err(invalid(
            "declared slot is not an original standalone comment",
        ));
    }
    Ok((output, ranges))
}
