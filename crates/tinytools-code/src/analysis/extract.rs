//! Bounded capture extraction without semantic name resolution.

use super::{Diagnostic, Reference, SourceRange, Symbol};
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator, Tree};

pub(crate) fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or_default()
}

pub(crate) fn bounded_text(value: &str, max_bytes: usize) -> (&str, bool) {
    let mut end = value.len().min(max_bytes);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (&value[..end], end < value.len())
}

pub(crate) fn body_node(node: Node<'_>) -> Option<Node<'_>> {
    if let Some(body) = node.child_by_field_name("body") {
        return Some(body);
    }
    if node.kind() == "type_alias_declaration" {
        return node
            .child_by_field_name("value")
            .filter(|value| value.kind() == "object_type");
    }
    if node.kind() == "type_spec" {
        let value = node.child_by_field_name("type")?;
        if value.kind() == "struct_type" {
            return value.named_child(0);
        }
        if value.kind() == "interface_type" {
            return Some(value);
        }
    }
    if node.kind() == "variable_declarator" {
        return node
            .child_by_field_name("value")
            .and_then(|value| value.child_by_field_name("body"));
    }
    None
}

fn name_node(node: Node<'_>) -> Option<Node<'_>> {
    node.child_by_field_name("name")
        .or_else(|| node.child_by_field_name("key"))
        .or_else(|| {
            (node.kind() == "impl_item")
                .then(|| node.child_by_field_name("type"))
                .flatten()
        })
        .or_else(|| {
            matches!(
                node.kind(),
                "table" | "table_array_element" | "pair" | "field_declaration"
            )
            .then(|| node.named_child(0))
            .flatten()
        })
}

fn name(node: Node<'_>, source: &str) -> Option<String> {
    let value = node_text(name_node(node)?, source).trim_matches(['\'', '"']);
    (!value.is_empty() && value.len() <= 1024).then(|| value.to_owned())
}

fn qualified_name(node: Node<'_>, source: &str, own_name: &str) -> String {
    let mut parts = vec![own_name.to_owned()];
    let mut length = own_name.len();
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        if matches!(
            parent.kind(),
            "impl_item"
                | "trait_item"
                | "mod_item"
                | "class_declaration"
                | "class_definition"
                | "interface_declaration"
                | "function_definition"
                | "function_item"
                | "function_declaration"
        ) && let Some(value) = name(parent, source)
        {
            if length + value.len() + 2 > 4096 {
                break;
            }
            length += value.len() + 2;
            parts.push(value);
        }
        ancestor = parent.parent();
    }
    parts.reverse();
    parts.join("::")
}

fn symbol(node: Node<'_>, source: &str, revision: &str) -> Option<Symbol> {
    let name = name(node, source)?;
    let body = body_node(node);
    let end = body.map_or(node.end_byte(), |body| body.start_byte());
    let (signature, signature_truncated) = bounded_text(
        source
            .get(node.start_byte()..end)
            .unwrap_or_default()
            .trim_end(),
        4096,
    );
    Some(Symbol {
        id: format!("{revision}:{}:{}", node.start_byte(), node.end_byte()),
        qualified_name: qualified_name(node, source, &name),
        name,
        kind: node.kind().to_owned(),
        range: node.into(),
        body: body.map(SourceRange::from),
        signature: signature.to_owned(),
        signature_truncated,
    })
}

pub(super) fn captures(
    tree: &Tree,
    query: &Query,
    source: &str,
    revision: &str,
    limit: usize,
) -> (Vec<Symbol>, Vec<Reference>, bool, bool) {
    let mut symbols = Vec::new();
    let mut references = Vec::new();
    let mut symbols_truncated = false;
    let mut references_truncated = false;
    let mut cursor = QueryCursor::new();
    cursor.set_match_limit(4096);
    let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());
    while let Some(found) = matches.next() {
        for capture in found.captures {
            let kind = query.capture_names()[capture.index as usize];
            if kind == "symbol" {
                if symbols.len() < limit {
                    if let Some(symbol) = symbol(capture.node, source, revision) {
                        symbols.push(symbol);
                    } else {
                        symbols_truncated = true;
                    }
                } else {
                    symbols_truncated = true;
                }
            } else if references.len() < limit {
                let (text, text_truncated) = bounded_text(node_text(capture.node, source), 1024);
                references.push(Reference {
                    kind: kind.to_owned(),
                    text: text.to_owned(),
                    range: capture.node.into(),
                    text_truncated,
                });
            } else {
                references_truncated = true;
            }
        }
    }
    if cursor.did_exceed_match_limit() {
        symbols_truncated = true;
        references_truncated = true;
    }
    symbols.sort_by_key(|symbol| symbol.range.start_byte);
    symbols.dedup_by(|a, b| a.id == b.id);
    references.sort_by_key(|reference| reference.range.start_byte);
    (symbols, references, symbols_truncated, references_truncated)
}

pub(super) fn diagnostics(tree: &Tree, limit: usize) -> (Vec<Diagnostic>, bool) {
    let mut diagnostics = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            if diagnostics.len() == limit {
                return (diagnostics, true);
            }
            let kind = if node.is_missing() {
                "missing"
            } else {
                "error"
            };
            diagnostics.push(Diagnostic {
                kind: kind.to_owned(),
                message: format!("{kind} syntax node: {}", node.kind()),
                range: node.into(),
            });
        }
        for index in (0..node.child_count()).rev() {
            if let Some(child) = node.child(index) {
                stack.push(child);
            }
        }
    }
    (diagnostics, false)
}
