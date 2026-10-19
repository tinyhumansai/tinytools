//! JSON repair ladder.

use crate::repair::json::{
    balance_closers, escape_control_characters, recover_object, recover_object_tracked,
    recover_whole_object, recover_whole_object_tracked, strip_code_fence, strip_trailing_commas,
};
use serde_json::json;

#[test]
fn strict_objects_pass_through() {
    assert_eq!(recover_object(r#"{"a":1}"#), Some(json!({"a": 1})));
    assert_eq!(recover_object(""), None);
    assert_eq!(
        recover_object("[1,2]"),
        None,
        "a non-object is never an object"
    );
}

#[test]
fn repairs_single_quoted_and_mismatched_keys() {
    assert_eq!(
        recover_object(r#"{"name":"get_weather","parameters':{'city':"Paris"}}"#),
        Some(json!({ "name": "get_weather", "parameters": { "city": "Paris" } }))
    );
    assert_eq!(
        recover_object(r#"{'city':"Paris"}"#),
        Some(json!({ "city": "Paris" }))
    );
}

#[test]
fn single_quoted_values_are_left_unrepaired() {
    // An apostrophe in a value is ordinary English; never rewrite it.
    assert_eq!(recover_object(r"{'city':'Paris'}"), None);
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
    assert_eq!(
        recover_object(r#"{toolkits:["discord"]}"#),
        Some(json!({ "toolkits": ["discord"] }))
    );
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
    assert_eq!(
        recover_object(r#"{toolkits:[<|">discord<|">]}"#),
        Some(json!({ "toolkits": ["discord"] }))
    );
    assert_eq!(
        recover_object(r#"{label_ids:[<|"|>INBOX<|"|>]}"#),
        Some(json!({ "label_ids": ["INBOX"] }))
    );
}

#[test]
fn peels_redundant_brace_layers() {
    assert_eq!(recover_object(r#"{{{"a":1}}}"#), Some(json!({ "a": 1 })));
    assert_eq!(
        recover_object(r#"{{tool:"X",arguments:{guild_id:"Y"}}}"#),
        Some(json!({ "tool": "X", "arguments": { "guild_id": "Y" } }))
    );
}

#[test]
fn strips_leaked_template_markers() {
    assert_eq!(
        recover_object(r#"{"a":1}<tool_call|>"#),
        Some(json!({ "a": 1 }))
    );
    assert_eq!(
        recover_object(r#"{"a":1}<|tool_calls_section_end|>"#),
        Some(json!({ "a": 1 }))
    );
    assert_eq!(
        recover_object(r#"{"a":1}<tool_call|>{"b":2}"#),
        Some(json!({ "a": 1 }))
    );
}

#[test]
fn a_marker_that_is_legitimate_string_data_is_preserved() {
    // The trailing comma forces the repair ladder to run; a blind
    // text-level marker strip would corrupt the value from inside its own
    // quotes instead of only removing marker text that leaked in as
    // structure.
    assert_eq!(
        recover_object(r#"{"text":"<tool_call>hi</tool_call>",}"#),
        Some(json!({ "text": "<tool_call>hi</tool_call>" }))
    );
}

#[test]
fn strips_a_code_fence() {
    assert_eq!(
        recover_object("```json\n{\"a\":1}\n```"),
        Some(json!({ "a": 1 }))
    );
    assert_eq!(strip_code_fence("```\n{}\n```"), "{}");
    assert_eq!(strip_code_fence("plain"), "plain");
}

#[test]
fn trailing_commas_and_missing_closers_are_repaired() {
    assert_eq!(
        recover_object(r#"{"a": [1, 2,]}"#),
        Some(json!({ "a": [1, 2] }))
    );
    assert_eq!(
        recover_object(r#"{"command": "ls -la", "timeout": 30"#),
        Some(json!({ "command": "ls -la", "timeout": 30 }))
    );
    assert_eq!(
        recover_object(r#"{"a":{"b":1}}}}"#),
        Some(json!({ "a": { "b": 1 } }))
    );
    assert_eq!(
        strip_trailing_commas(r#"{"a":"x,}","b":1,}"#),
        r#"{"a":"x,}","b":1}"#
    );
    assert_eq!(balance_closers(r#"{"a":[1"#), r#"{"a":[1]}"#);
}

#[test]
fn control_characters_inside_strings_are_escaped() {
    let raw = "{\"cmd\":\"ls\n-la\t\"}";
    assert_eq!(recover_object(raw), Some(json!({ "cmd": "ls\n-la\t" })));
    assert_eq!(
        escape_control_characters("{\"a\":\"x\ny\"}"),
        "{\"a\":\"x\\ny\"}"
    );
}

#[test]
fn typographic_quotes_are_straightened_last() {
    assert_eq!(
        recover_object("{“path”: “a.txt”}"),
        Some(json!({ "path": "a.txt" }))
    );
}

#[test]
fn truly_unrecoverable_input_is_none() {
    assert_eq!(recover_object("not json at all"), None);
    assert_eq!(
        recover_object(r#"{"truncated": "val"#),
        None,
        "an open string cannot be guessed"
    );
}

#[test]
fn strip_trailing_commas_keeps_layout_and_multibyte_text_intact() {
    // Whitespace after the dropped comma survives, so indentation is preserved.
    assert_eq!(
        strip_trailing_commas("{ \"a\": 1, \"b\": [1, 2,\n] ,\n}"),
        "{ \"a\": 1, \"b\": [1, 2\n] \n}"
    );
    // Input with nothing to strip comes back byte-for-byte, including
    // non-ASCII text inside and outside strings.
    let src = "{\"name\": \"café 日本 🦀\", \"tags\": [\"é\", \"ü\"]}";
    assert_eq!(strip_trailing_commas(src), src);
    // A comma inside a string is data, and non-ASCII neighbours do not
    // disturb the one that is stripped.
    assert_eq!(
        strip_trailing_commas("{\"a\": \"é,}\", \"b\": \"日本\",}"),
        "{\"a\": \"é,}\", \"b\": \"日本\"}"
    );
}

#[test]
fn strip_trailing_commas_handles_nested_arrays_and_objects_and_string_commas() {
    assert_eq!(
        strip_trailing_commas("{ \"a\": 1, \"b\": [1, 2,], }"),
        "{ \"a\": 1, \"b\": [1, 2] }"
    );
    let src = "{ \"reason\": \"a, b, c,\" }";
    assert_eq!(strip_trailing_commas(src), src);
}

// ── Tracked recovery (tinyhumansai/tinytools#50) ───────────────────────────

#[test]
fn tracked_recovery_reports_only_appended_closers() {
    let recovered =
        recover_object_tracked(r#"{"command": "ls -la", "timeout": 30"#).expect("recovers");
    assert_eq!(
        recovered.value,
        json!({ "command": "ls -la", "timeout": 30 })
    );
    assert!(recovered.closers_balanced);

    // Strict input, another rung's repair, and a trimmed surplus closer are
    // all things the model wrote; none marks a cut.
    for raw in [
        r#"{"a":1}"#,
        r#"{"a": [1, 2,]}"#,
        r#"{"a":{"b":1}}}}"#,
        r#"{toolkits:["discord"]}"#,
        "{\"a\":1}<tool_call|>{\"b\":2}",
        "{“path”: “a.txt”}",
    ] {
        let recovered = recover_object_tracked(raw).expect(raw);
        assert!(!recovered.closers_balanced, "{raw}");
    }
}

#[test]
fn tracked_recovery_reports_closers_appended_after_straightening_quotes() {
    let recovered = recover_object_tracked("{“path”: “a.txt”").expect("recovers");
    assert_eq!(recovered.value, json!({ "path": "a.txt" }));
    assert!(recovered.closers_balanced);
}

#[test]
fn tracked_whole_object_recovery_matches_the_untracked_ladder() {
    let raw = r#"{"name":"shell","arguments":{"command":"x""#;
    let recovered = recover_whole_object_tracked(raw).expect("recovers");
    assert_eq!(Some(recovered.value), recover_whole_object(raw));
    assert!(recovered.closers_balanced);
    assert!(recover_whole_object_tracked(r#"{"a":1} trailing"#).is_none());
    assert!(recover_object_tracked("").is_none());
}
