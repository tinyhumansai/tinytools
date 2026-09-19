//! Unit tests for protected fenced-block detection.

use super::{fence_ranges, is_protected, protected_end};

#[test]
fn a_fence_indented_more_than_three_spaces_is_not_a_fence() {
    // CommonMark treats a fence indented four or more spaces as an
    // indented code block, not a fence, so it must not open a protected
    // range even though it carries a language tag.
    let text = "    ```rust\nfn main() {}\n    ```\n";
    assert!(fence_ranges(text).is_empty());
}

#[test]
fn a_two_backtick_run_is_not_a_fence() {
    // A fence needs at least three backticks (or tildes); shorter runs are
    // inline code spans, not fence delimiters.
    let text = "``json\n{\"a\":1}\n``\n";
    assert!(fence_ranges(text).is_empty());
}

#[test]
fn is_protected_reports_positions_inside_and_outside_a_fence() {
    let text = "before\n```json\n<tool_call>x</tool_call>\n```\nafter";
    let ranges = fence_ranges(text);
    assert_eq!(ranges.len(), 1);

    let fence_start = text.find("```json").expect("fence marker present");
    let inside = fence_start + 4;
    assert!(is_protected(&ranges, inside));

    let outside = text.find("before").expect("prefix present");
    assert!(!is_protected(&ranges, outside));
}

#[test]
fn protected_end_locates_the_close_of_the_containing_fence() {
    let text = "```json\n<tool_call>x</tool_call>\n```\nafter";
    let ranges = fence_ranges(text);
    let fence_start = 0;
    let inside = text.find("<tool_call>").expect("marker present");

    assert_eq!(protected_end(&ranges, inside), Some(ranges[0].end));
    assert_eq!(protected_end(&ranges, fence_start), Some(ranges[0].end));

    let after = text.rfind("after").expect("suffix present");
    assert_eq!(protected_end(&ranges, after), None);
}
