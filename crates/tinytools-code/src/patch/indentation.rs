//! Bounded indentation and container edits preserve bytes outside the edit.

use crate::{
    CodeError, Language,
    error::{enforce_limit, invalid},
};
use tree_sitter::Node;

pub(super) fn line_indent(source: &str, offset: usize) -> &str {
    let start = source[..offset]
        .rfind('\n')
        .map_or(0, |position| position + 1);
    let prefix = &source[start..offset];
    let end = prefix
        .find(|value: char| value != ' ' && value != '\t')
        .unwrap_or(prefix.len());
    &prefix[..end]
}

pub(super) fn indented(code: &str, indent: &str, limit: usize) -> Result<String, CodeError> {
    let mut output = String::new();
    for (index, line) in code.trim().lines().enumerate() {
        let added = line
            .len()
            .saturating_add(if line.is_empty() { 0 } else { indent.len() })
            .saturating_add(usize::from(index > 0));
        enforce_limit("indented bytes", output.len().saturating_add(added), limit)?;
        if index > 0 {
            output.push('\n');
        }
        if !line.is_empty() {
            output.push_str(indent);
        }
        output.push_str(line);
    }
    Ok(output)
}

pub(super) fn braces(node: Node<'_>) -> Option<(usize, usize)> {
    let mut open = None;
    let mut close = None;
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            if child.kind() == "{" {
                open = Some(child.end_byte());
            }
            if child.kind() == "}" {
                close = Some(child.start_byte());
            }
        }
    }
    open.zip(close)
}

pub(super) fn replace_body(
    body: Node<'_>,
    source: &str,
    language: Language,
    code: &str,
    limit: usize,
) -> Result<(usize, usize, String), CodeError> {
    let outer = line_indent(source, body.start_byte());
    if language == Language::Python {
        let line_start = source[..body.start_byte()]
            .rfind('\n')
            .map_or(0, |position| position + 1);
        let inline = !source[line_start..body.start_byte()].trim().is_empty();
        let indent = if inline {
            format!("{outer}    ")
        } else {
            outer.to_owned()
        };
        let replacement = indented(
            if code.trim().is_empty() { "pass" } else { code },
            &indent,
            limit,
        )?;
        return if inline {
            Ok((
                body.start_byte(),
                body.end_byte(),
                format!("\n{replacement}"),
            ))
        } else {
            Ok((line_start, body.end_byte(), replacement))
        };
    }
    let (start, end) = braces(body).ok_or_else(|| {
        CodeError::UnsupportedOperation("body has no brace-delimited block".to_owned())
    })?;
    let child_indent = format!("{outer}    ");
    let replacement = indented(code, &child_indent, limit)?;
    Ok((start, end, format!("\n{replacement}\n{outer}")))
}

fn last_member(body: Node<'_>) -> Option<Node<'_>> {
    (0..body.named_child_count())
        .rev()
        .filter_map(|index| body.named_child(index))
        .find(|node| !node.kind().contains("comment"))
}

pub(super) fn append_member(
    body: Node<'_>,
    source: &str,
    language: Language,
    code: &str,
    limit: usize,
) -> Result<(usize, usize, String), CodeError> {
    if code.trim().is_empty() {
        return Err(invalid("member source must not be empty"));
    }
    if language == Language::Python {
        let original = &source[body.byte_range()];
        let outer = line_indent(source, body.start_byte());
        let line_start = source[..body.start_byte()]
            .rfind('\n')
            .map_or(0, |position| position + 1);
        let inline = !source[line_start..body.start_byte()].trim().is_empty();
        if inline {
            let indent = format!("{outer}    ");
            let original = indented(original, &indent, limit)?;
            let added = indented(code, &indent, limit)?;
            return Ok((
                body.start_byte(),
                body.end_byte(),
                format!("\n{original}\n{added}"),
            ));
        }
        let added = indented(code, outer, limit)?;
        return Ok((body.end_byte(), body.end_byte(), format!("\n{added}")));
    }
    let (_, close) = braces(body).ok_or_else(|| {
        CodeError::UnsupportedOperation("selected node has no member container".to_owned())
    })?;
    let outer = line_indent(source, body.start_byte());
    let indent = format!("{outer}    ");
    let added = indented(code, &indent, limit)?;
    let line_start = source[..close]
        .rfind('\n')
        .map_or(0, |position| position + 1);
    let inline_close = !source[line_start..close].trim().is_empty();
    let insertion = if inline_close { close } else { line_start };
    let comma = language == Language::Rust
        && matches!(body.kind(), "field_declaration_list" | "enum_variant_list");
    let added_comma = if comma && !code.trim_end().ends_with(',') {
        ","
    } else {
        ""
    };
    // The untouched closing line already contains its indentation when the
    // insertion begins at that line's start.
    let closing_indent = if inline_close { outer } else { "" };
    if comma
        && let Some(last) = last_member(body)
        && !source[last.end_byte()..close].trim_start().starts_with(',')
    {
        let suffix = &source[last.end_byte()..insertion];
        let separator = if suffix.ends_with('\n') { "" } else { "\n" };
        return Ok((
            last.end_byte(),
            insertion,
            format!(",{suffix}{separator}{added}{added_comma}\n{closing_indent}"),
        ));
    }
    let separator = if insertion > 0 && source.as_bytes()[insertion - 1] == b'\n' {
        ""
    } else {
        "\n"
    };
    Ok((
        insertion,
        insertion,
        format!("{separator}{added}{added_comma}\n{closing_indent}"),
    ))
}
