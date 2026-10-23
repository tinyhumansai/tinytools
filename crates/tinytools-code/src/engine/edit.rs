//! Minimal contiguous UTF-8 edits for incremental parser reuse.

use tree_sitter::{InputEdit, Point};

fn point(source: &str, byte: usize) -> Point {
    let prefix = &source[..byte];
    let row = prefix.bytes().filter(|value| *value == b'\n').count();
    let column = prefix.rfind('\n').map_or(byte, |offset| byte - offset - 1);
    Point { row, column }
}

pub(super) fn difference(old: &str, new: &str) -> InputEdit {
    let mut start = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
        start -= 1;
    }
    let suffix = old.as_bytes()[start..]
        .iter()
        .rev()
        .zip(new.as_bytes()[start..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut old_end = old.len() - suffix;
    let mut new_end = new.len() - suffix;
    while !old.is_char_boundary(old_end) || !new.is_char_boundary(new_end) {
        old_end += 1;
        new_end += 1;
    }
    InputEdit {
        start_byte: start,
        old_end_byte: old_end,
        new_end_byte: new_end,
        start_position: point(old, start),
        old_end_position: point(old, old_end),
        new_end_position: point(new, new_end),
    }
}
