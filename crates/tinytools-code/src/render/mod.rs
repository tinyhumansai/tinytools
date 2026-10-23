//! One-pass typed rendering followed by syntax and generated-node validation.

mod context;
mod parameters;
mod slots;
mod types;

use crate::{
    ChunkManifest, CodeEngine, CodeError,
    error::{enforce_limit, invalid},
    revision,
};
pub use types::{ChunkBindings, RenderedChunk};

/// Render typed parameters and original standalone slots, then validate syntax.
///
/// # Errors
/// Refuses invalid metadata, unknown/missing/unused bindings, unsafe type
/// expressions, malformed slots, generated-node violations and resource excess.
pub fn render_chunk(
    engine: &mut CodeEngine,
    manifest: &ChunkManifest,
    template: &str,
    bindings: &ChunkBindings,
) -> Result<RenderedChunk, CodeError> {
    manifest.validate()?;
    enforce_limit(
        "template bytes",
        template.len(),
        engine.limits().max_source_bytes,
    )?;
    let expanded = parameters::expand(engine, manifest, template, bindings)?;
    let (source, ranges) = slots::expand(engine, manifest, &expanded, bindings)?;
    let parsed = context::parse(engine, manifest.language, manifest.target, &source)?;
    for range in ranges {
        let Some(kind) = manifest
            .slots
            .iter()
            .find(|slot| slot.name == range.name)
            .and_then(|slot| slot.node_kind.as_ref())
        else {
            continue;
        };
        let snippet = &source[range.start..range.end];
        let start = range.start + snippet.len() - snippet.trim_start().len();
        let end = range.end - snippet.len() + snippet.trim_end().len();
        if start == end {
            continue;
        }
        let start = parsed.offset(&source, start);
        let end = parsed.offset(&source, end);
        let exact = parsed
            .tree
            .root_node()
            .descendant_for_byte_range(start, end);
        if !exact.is_some_and(|node| {
            node.start_byte() == start && node.end_byte() == end && node.kind() == kind
        }) {
            return Err(invalid(format!(
                "slot {} must produce exactly one {kind} node",
                range.name
            )));
        }
    }
    let generated_start = parsed.offset(&source, 0);
    let generated_end = parsed.offset(&source, source.len());
    let mut stack = vec![parsed.tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.start_byte() >= generated_start
            && node.end_byte() <= generated_end
            && manifest
                .forbidden_nodes
                .iter()
                .any(|kind| kind == node.kind())
        {
            return Err(invalid(format!(
                "generated source contains forbidden node kind: {}",
                node.kind()
            )));
        }
        for index in 0..node.named_child_count() {
            if let Some(child) = node.named_child(index) {
                stack.push(child);
            }
        }
    }
    Ok(RenderedChunk {
        id: manifest.id.clone(),
        language: manifest.language,
        target: manifest.target,
        revision: revision(&source),
        source,
        dependencies: manifest.dependencies.clone(),
    })
}
