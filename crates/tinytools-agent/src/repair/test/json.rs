//! JSON repair ladder.

use crate::repair::json::{
    balance_closers, escape_control_characters, recover_object, strip_code_fence,
    strip_trailing_commas,
};
use serde_json::json;

#[test]
fn strict_objects_pass_through() {
    assert_eq!(recover_object(r#"{"a":1}"#), Some(json!({"a": 1})));
    assert_eq!(recover_object(""), None);
    assert_eq!(recover_object("[1,2]"), None, "a non-object is never an object");
}

#[test]
fn repairs_single_quoted_and_mismatched_keys() {
    assert_eq!(
        recover_object(r#"{"name":"get_weather","parameters':{'city':"Paris"}}"#),
        Some(json!({ "name": "get_weather", "parameters": { "city": "Paris" } }))
    );
    assert_eq!(recover_object(r#"{'city':"Paris"}"#), Some(json!({ "city": "Paris" })));
}

#[test]
fn single_quoted_values_are_left_unrepaired() {
    // An apostrophe in a value is ordinary English; never rewrite it.
    assert_eq!(recover_object(r#"{'city':'Paris'}"#), None);
}

#[test]
fn an_apostrophe_inside_a_well_formed_key_is_not_a_delimiter() {
    assert_eq!(
        recover_object(r#"{"it's fine":1,bare:2}"#),
        Some(json!({ "it's fine": 1, "bare": 2 }))
    );
}

#[test]
fn quotes_unquoted_keys_and_leaves_literals() {
    assert_eq!(recover_object(r#"{toolkits:["discord"]}"#), Some(json!({ "toolkits": ["discord"] })));
    assert_eq!(
        recover_object(r#"{include_unconnected:true,toolkits:["discord"],n:null}"#),
        Some(json!({ "include_unconnected": true, "toolkits": ["discord"], "n": null }))
    );
    assert_eq!(
        recover_object(r#"{query:"from:john,to:x",n:1}"#),
        Some(json!({ "query": "from:john,to:x", "n": 1 }))
    );
}

#[test]
fn substitutes_leaked_quote_tokens() {
    assert_eq!(recover_object(r#"{toolkits:[<|">discord<|">]}"#), Some(json!({ "toolkits": ["discord"] })));
    assert_eq!(recover_object(r#"{label_ids:[<|"|>INBOX<|"|>]}"#), Some(json!({ "label_ids": ["INBOX"] })));
}

#[test]
fn peels_redundant_brace_layers() {
    assert_eq!(recover_object(r#"{{{"a":1}}}"#), Some(json!({ "a": 1 })));
    assert_eq!(recover_object(r#"{{tool:"X",arguments:{guild_id:"Y"}}}"#), Some(json!({ "tool": "X", "arguments": { "guild_id": "Y" } })));
}

#[test]
fn strips_leaked_template_markers() {
    assert_eq!(recover_object(r#"{"a":1}<tool_call|>"#), Some(json!({ "a": 1 })));
    assert_eq!(recover_object(r#"{"a":1}<|tool_calls_section_end|>"#), Some(json!({ "a": 1 })));
    assert_eq!(recover_object(r#"{"a":1}<tool_call|>{"b":2}"#), Some(json!({ "a": 1 })));
}

#[test]
fn strips_a_code_fence() {
    assert_eq!(recover_object("```json\n{\"a\":1}\n```"), Some(json!({ "a": 1 })));
    assert_eq!(strip_code_fence("```\n{}\n```"), "{}");
    assert_eq!(strip_code_fence("plain"), "plain");
}

#[test]
fn trailing_commas_and_missing_closers_are_repaired() {
    assert_eq!(recover_object(r#"{"a": [1, 2,]}"#), Some(json!({ "a": [1, 2] })));
    assert_eq!(recover_object(r#"{"command": "ls -la", "timeout": 30"#), Some(json!({ "command": "ls -la", "timeout": 30 })));
    assert_eq!(recover_object(r#"{"a":{"b":1}}}}"#), Some(json!({ "a": { "b": 1 } })));
    assert_eq!(strip_trailing_commas(r#"{"a":"x,}","b":1,}"#), r#"{"a":"x,}","b":1}"#);
    assert_eq!(balance_closers(r#"{"a":[1"#), r#"{"a":[1]}"#);
}

#[test]
fn control_characters_inside_strings_are_escaped() {
    let raw = "{\"cmd\":\"ls\n-la\t\"}";
    assert_eq!(recover_object(raw), Some(json!({ "cmd": "ls\n-la\t" })));
    assert_eq!(escape_control_characters("{\"a\":\"x\ny\"}"), "{\"a\":\"x\\ny\"}");
}

#[test]
fn typographic_quotes_are_straightened_last() {
    assert_eq!(recover_object("{“path”: “a.txt”}"), Some(json!({ "path": "a.txt" })));
}

#[test]
fn truly_unrecoverable_input_is_none() {
    assert_eq!(recover_object("not json at all"), None);
    assert_eq!(recover_object(r#"{"truncated": "val"#), None, "an open string cannot be guessed");
}
