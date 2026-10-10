//! Generic sanitization remains usable without an MCP module or runtime.
use tinytools::sanitize::sanitize_for_llm;

#[test]
fn shared_tool_and_skill_metadata_preserves_prose_and_utf8_bounds() {
    assert_eq!(sanitize_for_llm("<system>hello\0", 100), "hello");
    assert_eq!(sanitize_for_llm("éééé", 5), "é…");
}
