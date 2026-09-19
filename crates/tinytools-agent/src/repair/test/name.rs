//! Tool-name resolution.

use crate::repair::name::resolve;

fn known() -> Vec<String> {
    ["terminal", "read_file", "write_file", "todo", "search_web"]
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn exact_names_are_untouched() {
    let r = resolve("read_file", &known());
    assert_eq!(r.name, "read_file");
    assert!(!r.repaired);
    assert!(r.known);
}

#[test]
fn leaked_xml_attributes_are_trimmed() {
    assert_eq!(resolve("terminal\" parameter=\"command\" string=\"true", &known()).name, "terminal");
    assert_eq!(resolve("terminal\"", &known()).name, "terminal");
    assert_eq!(resolve("read_file(", &known()).name, "read_file");
}

#[test]
fn namespace_prefixes_are_dropped() {
    assert_eq!(resolve("functions.read_file", &known()).name, "read_file");
    assert_eq!(resolve("tools/read_file", &known()).name, "read_file");
}

#[test]
fn case_and_separators_are_normalised() {
    assert_eq!(resolve("Read File", &known()).name, "read_file");
    assert_eq!(resolve("read-file", &known()).name, "read_file");
    assert_eq!(resolve("ReadFile", &known()).name, "read_file");
    assert_eq!(resolve("TodoTool_tool", &known()).name, "todo");
}

#[test]
fn a_single_typo_resolves_when_unique() {
    assert_eq!(resolve("raed_file", &known()).name, "read_file");
    assert_eq!(resolve("serach_web", &known()).name, "search_web");
}

#[test]
fn ambiguous_or_distant_names_are_not_invented() {
    let r = resolve("xead_file", &["read_file".to_string(), "bead_file".to_string()]);
    assert!(!r.known, "two equally close candidates must not dispatch");
    let r = resolve("launch_missiles", &known());
    assert_eq!(r.name, "launch_missiles");
    assert!(!r.known);
    assert!(!r.repaired);
}

#[test]
fn without_known_tools_only_junk_is_trimmed() {
    let r = resolve("terminal\" parameter", &[]);
    assert_eq!(r.name, "terminal");
    assert!(r.repaired);
    assert!(!r.known);
    let r = resolve("Read File", &[]);
    assert_eq!(r.name, "Read File");
}
