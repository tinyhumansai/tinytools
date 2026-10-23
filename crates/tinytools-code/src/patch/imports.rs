//! Import insertion preserves module prologues and attached outer documentation.

use crate::{
    CodeEngine, CodeError, Language,
    analysis::{is_docstring, node_text},
    error::invalid,
};
use tree_sitter::{Node, Tree};

fn import_kind(kind: &str) -> bool {
    matches!(
        kind,
        "use_declaration"
            | "import_statement"
            | "import_from_statement"
            | "future_import_statement"
            | "import_declaration"
    )
}

fn candidate(engine: &CodeEngine, language: Language, code: &str) -> Result<Tree, CodeError> {
    let candidate = if language == Language::Go {
        format!("package chunk\n{code}")
    } else {
        code.to_owned()
    };
    let tree = engine.parse(language, &candidate)?;
    if tree.root_node().has_error() {
        return Err(CodeError::InvalidSyntax { language });
    }
    let nodes: Vec<_> = (0..tree.root_node().named_child_count())
        .filter_map(|index| tree.root_node().named_child(index))
        .filter(|node| node.kind() != "package_clause")
        .collect();
    if nodes.len() != 1 || !import_kind(nodes[0].kind()) {
        return Err(invalid(
            "insert_import requires exactly one complete import declaration",
        ));
    }
    Ok(tree)
}

fn attached_doc(language: Language, text: &str) -> bool {
    match language {
        Language::Rust => {
            (text.starts_with("///") && !text.starts_with("////"))
                || (text.starts_with("/**") && !text.starts_with("/***"))
        }
        Language::JavaScript | Language::TypeScript | Language::Tsx => text.starts_with("/**"),
        _ => false,
    }
}

fn directive(node: Node<'_>) -> bool {
    node.kind() == "expression_statement"
        && node
            .named_child(0)
            .is_some_and(|value| value.kind() == "string")
}

pub(super) fn insert(
    engine: &CodeEngine,
    tree: &Tree,
    language: Language,
    source: &str,
    code: &str,
) -> Result<(usize, usize, String), CodeError> {
    if matches!(language, Language::Json | Language::Toml) {
        return Err(CodeError::UnsupportedOperation(
            "data formats do not have imports".to_owned(),
        ));
    }
    let candidate = candidate(engine, language, code)?;
    let future = candidate
        .root_node()
        .named_child(0)
        .is_some_and(|node| node.kind() == "future_import_statement");
    let mut offset = 0;
    let mut python_statement_seen = false;
    for index in 0..tree.root_node().named_child_count() {
        let Some(node) = tree.root_node().named_child(index) else {
            continue;
        };
        let text = node_text(node, source);
        if import_kind(node.kind()) && text.trim() == code.trim() {
            return Ok((0, 0, String::new()));
        }
        if node.kind().contains("comment")
            || node.kind() == "hash_bang_line"
            || node.kind() == "shebang"
        {
            if attached_doc(language, text) {
                break;
            }
            offset = node.end_byte();
            continue;
        }
        if language == Language::Python && !python_statement_seen && is_docstring(node, source) {
            python_statement_seen = true;
            offset = node.end_byte();
            continue;
        }
        python_statement_seen = true;
        if import_kind(node.kind()) {
            if future && node.kind() != "future_import_statement" {
                break;
            }
            offset = node.end_byte();
        } else if matches!(node.kind(), "package_clause" | "inner_attribute_item")
            || (matches!(
                language,
                Language::JavaScript | Language::TypeScript | Language::Tsx
            ) && directive(node))
        {
            offset = node.end_byte();
        } else {
            break;
        }
    }
    let prefix = if offset > 0 && !source[..offset].ends_with('\n') {
        "\n"
    } else {
        ""
    };
    Ok((offset, offset, format!("{prefix}{}\n", code.trim())))
}
