//! GLM line grammar and its helpers.

use super::parse;
use crate::parse::{build_curl_command, map_glm_tool_alias, parse_glm_style_tool_calls};

#[test]
fn glm_lines_parse_when_nothing_else_matched() {
    let (text, calls) = parse("shell/command>ls -la");
    assert!(text.is_empty());
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "shell");
}

#[test]
fn glm_parser_covers_json_payloads_invalid_urls_and_plain_commands() {
    let calls = parse_glm_style_tool_calls(concat!(
        "\n",
        "custom/{\"answer\":42}\n",
        "shell/url>not-a-url\n",
        "shell/command>https://example.com/has space\n",
        "shell/command>echo hi\n",
        "not a url"
    ));
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].0, "custom");
    assert_eq!(calls[0].1, serde_json::json!({"answer": 42}));
    assert_eq!(calls[1].1, serde_json::json!({"command": "https://example.com/has space"}));
    assert_eq!(calls[2].1, serde_json::json!({"command": "echo hi"}));
}

#[test]
fn glm_helpers_parse_aliases_urls_and_commands() {
    assert_eq!(map_glm_tool_alias("browser_open"), "shell");
    assert_eq!(map_glm_tool_alias("http"), "http_request");
    assert_eq!(map_glm_tool_alias("custom_tool"), "custom_tool");

    assert_eq!(
        build_curl_command("https://example.com?q=1"),
        Some("curl -s 'https://example.com?q=1'".into())
    );
    assert_eq!(
        build_curl_command("https://exa'mple.com"),
        Some("curl -s 'https://exa'\\''mple.com'".into())
    );
    assert!(build_curl_command("ftp://example.com").is_none());
    assert!(build_curl_command("https://example.com/has space").is_none());

    let calls = parse_glm_style_tool_calls(
        "browser_open/url>https://example.com\nhttp_request/url>https://api.example.com\nplain text\nhttps://rust-lang.org",
    );
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].0, "shell");
    assert_eq!(calls[1].0, "http_request");
    assert!(parse_glm_style_tool_calls("https://rust-lang.org").is_empty());
}
