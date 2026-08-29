#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::{
    ContextDetailOptions, context_detail_from_args, context_detail_from_args_with,
    humanize_tool_name,
};

#[test]
fn snake_and_kebab_case_become_title_case() {
    assert_eq!(
        humanize_tool_name("gmail_read_message"),
        "Gmail Read Message"
    );
    assert_eq!(humanize_tool_name("web_fetch"), "Web Fetch");
    assert_eq!(humanize_tool_name("shell"), "Shell");
    assert_eq!(humanize_tool_name("read-diff"), "Read Diff");
}

#[test]
fn machine_prefixes_are_stripped() {
    // A timeline row should read as the action, not the transport that carried
    // it.
    assert_eq!(
        humanize_tool_name("composio_gmail_send_email"),
        "Gmail Send Email"
    );
    assert_eq!(
        humanize_tool_name("mcp_notion_create_page"),
        "Notion Create Page"
    );
}

#[test]
fn degenerate_names_fall_back_to_the_input() {
    assert_eq!(humanize_tool_name(""), "");
    assert_eq!(humanize_tool_name("___"), "___");
}

#[test]
fn the_most_specific_recognized_key_wins() {
    // A messaging call carries both; the recipient is the useful half.
    assert_eq!(
        context_detail_from_args(&json!({ "name": "ignored", "to": "steven@example.com" }))
            .as_deref(),
        Some("steven@example.com")
    );
}

#[test]
fn non_objects_and_unrecognized_keys_yield_nothing() {
    assert!(context_detail_from_args(&serde_json::Value::Null).is_none());
    assert!(context_detail_from_args(&json!(["a"])).is_none());
    assert!(context_detail_from_args(&json!({ "unrecognized": "x" })).is_none());
    assert!(context_detail_from_args(&json!({ "path": "" })).is_none());
    assert!(context_detail_from_args(&json!({ "path": { "nested": 1 } })).is_none());
}

#[test]
fn scalars_and_string_arrays_render() {
    assert_eq!(
        context_detail_from_args(&json!({ "id": 42 })).as_deref(),
        Some("42")
    );
    assert_eq!(
        context_detail_from_args(&json!({ "name": true })).as_deref(),
        Some("true")
    );
    assert_eq!(
        context_detail_from_args(&json!({ "to": ["a@x.com", "b@x.com"] })).as_deref(),
        Some("a@x.com, b@x.com")
    );
}

#[test]
fn whitespace_is_collapsed() {
    assert_eq!(
        context_detail_from_args(&json!({ "command": "  ls   -la  " })).as_deref(),
        Some("ls -la")
    );
}

#[test]
fn long_values_are_elided_within_the_cap() {
    let long = "x".repeat(200);
    let detail = context_detail_from_args(&json!({ "query": long })).expect("a detail");
    assert!(detail.chars().count() <= 80);
    assert!(detail.ends_with("..."));
}

#[test]
fn a_custom_ellipsis_and_cap_are_honoured() {
    let long = "x".repeat(200);
    let detail = context_detail_from_args_with(
        &json!({ "query": long }),
        ContextDetailOptions::new(10, "…"),
    )
    .expect("a detail");
    assert_eq!(detail.chars().count(), 10);
    assert!(detail.ends_with('…'));
}

#[test]
fn an_ellipsis_longer_than_the_cap_cannot_overflow_it() {
    // A misconfigured caller must not be able to push the rendered value past
    // the cap it asked for.
    let long = "x".repeat(200);
    let detail = context_detail_from_args_with(
        &json!({ "query": long }),
        ContextDetailOptions::new(2, "..."),
    )
    .expect("a detail");
    assert_eq!(detail.chars().count(), 2);
}

#[test]
fn the_default_options_are_eighty_chars_and_three_dots() {
    let defaults = ContextDetailOptions::default();
    assert_eq!(defaults.max_chars, 80);
    assert_eq!(defaults.ellipsis, "...");
}
