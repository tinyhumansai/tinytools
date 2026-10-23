//! Readable folded bodies retain surrounding declarations and Python docstrings.

use super::{Symbol, node_text};
use crate::Language;
use tree_sitter::{Node, Tree};

pub(crate) fn is_docstring(node: Node<'_>, source: &str) -> bool {
    if node.kind() != "expression_statement" || node.named_child_count() != 1 {
        return false;
    }
    let Some(value) = node.named_child(0) else {
        return false;
    };
    if !matches!(value.kind(), "string" | "concatenated_string") {
        return false;
    }
    let mut stack = vec![value];
    while let Some(part) = stack.pop() {
        if part.kind() == "string_start" {
            let prefix = node_text(part, source).to_ascii_lowercase();
            if prefix.contains('b') || prefix.contains('f') {
                return false;
            }
        }
        for index in 0..part.named_child_count() {
            if let Some(child) = part.named_child(index) {
                stack.push(child);
            }
        }
    }
    true
}

fn replacement(tree: &Tree, source: &str, language: Language, start: usize, end: usize) -> String {
    if language == Language::Python {
        let node = tree.root_node().descendant_for_byte_range(start, end);
        if let Some(first) = node
            .and_then(|body| body.named_child(0))
            .filter(|first| is_docstring(*first, source))
        {
            let line_start = source[..start].rfind('\n').map_or(0, |offset| offset + 1);
            let indent = &source[line_start..start];
            let indent = if indent.chars().all(char::is_whitespace) {
                indent
            } else {
                "    "
            };
            return format!("{}\n{indent}pass  # body folded", node_text(first, source));
        }
        return "pass  # body folded".to_owned();
    }
    "{ /* body folded */ }".to_owned()
}

pub(super) fn fold(tree: &Tree, source: &str, language: Language, symbols: &[Symbol]) -> String {
    let mut output = String::new();
    let mut copied = 0;
    for body in symbols.iter().filter_map(|symbol| symbol.body.as_ref()) {
        if body.start_byte < copied {
            continue;
        }
        let folded = replacement(tree, source, language, body.start_byte, body.end_byte);
        if folded.len() >= body.end_byte - body.start_byte {
            continue;
        }
        output.push_str(&source[copied..body.start_byte]);
        output.push_str(&folded);
        copied = body.end_byte;
    }
    output.push_str(&source[copied..]);
    output
}
