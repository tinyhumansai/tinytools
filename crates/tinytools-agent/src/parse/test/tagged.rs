//! `<tool_call>` family: spellings, garbled pipes, fences, Kimi bodies.

use super::parse;
use crate::parse::grammar::tagged::recover_sentinel_body;
use crate::types::CallSource;
use crate::{PFormatRegistry, PFormatToolParams, parse_tool_calls_with_pformat};

#[test]
fn canonical_tag_with_surrounding_prose() {
    let xml = "before\n<tool_call>\n{\"name\":\"echo\",\"arguments\":{\"value\":\"two\"}}\n</tool_call>\nafter";
    let (text, calls) = parse(xml);
    assert_eq!(text, "before\nafter");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments, serde_json::json!({ "value": "two" }));
    assert_eq!(calls[0].source, CallSource::TaggedJson);
    assert!(calls[0].id.is_none(), "text calls never carry an id");
}

#[test]
fn multiple_calls_keep_prose_between_them() {
    let text = r#"a<tool_call>{"name":"one","arguments":{}}</tool_call>b<tool_call>{"name":"two","arguments":{"x":1}}</tool_call>c"#;
    let (cleaned, calls) = parse(text);
    assert_eq!(cleaned, "a\nb\nc");
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].name, "one");
    assert_eq!(calls[1].name, "two");
}

#[test]
fn missing_arguments_default_to_empty_object() {
    let (_, calls) = parse(r#"<tool_call>{"name":"noargs"}</tool_call>"#);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments, serde_json::json!({}));
}

#[test]
fn spelling_variants_and_bare_invoke_literal() {
    let (text, calls) =
        parse("<invoke>{\"name\":\"echo\",\"arguments\":{\"value\":\"three\"}}</invoke>");
    assert!(text.is_empty());
    assert_eq!(calls.len(), 1);

    let (_, calls) = parse("<toolcall>{\"name\":\"a\",\"arguments\":{}}</toolcall>");
    assert_eq!(calls[0].name, "a");
    let (_, calls) = parse("<tool-call>{\"name\":\"b\",\"arguments\":{}}</tool-call>");
    assert_eq!(calls[0].name, "b");
}

#[test]
fn attribute_form_and_pipe_variant_open_a_block() {
    let (cleaned, calls) =
        parse(r#"<tool_call id="call_0">{"name":"foo","arguments":{"a":1}}</tool_call>"#);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "foo");
    assert!(cleaned.is_empty());

    let (_, calls) = parse(r#"<tool_call|>{"name":"bar","arguments":{}}</tool_call>"#);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "bar");
}

#[test]
fn plural_tool_calls_tag_is_not_an_opener() {
    let (cleaned, calls) = parse("the <tool_calls> key holds them");
    assert!(calls.is_empty());
    assert_eq!(cleaned, "the <tool_calls> key holds them");
}

#[test]
fn malformed_body_is_dropped_without_dispatching() {
    let (text, calls) = parse("before <tool_call>not-json</tool_call> after");
    assert_eq!(text, "before\nafter");
    assert!(calls.is_empty());
}

#[test]
fn unclosed_tag_with_balanced_json_still_recovers() {
    let (text, calls) = parse("before <toolcall>{\"name\":\"echo\",\"arguments\":{}}");
    assert_eq!(text, "before");
    assert_eq!(calls.len(), 1);
}

#[test]
fn unclosed_tag_without_json_is_kept_as_text() {
    let (text, calls) = parse("before <tool-call>not-json");
    assert_eq!(text, "before <tool-call>not-json");
    assert!(calls.is_empty());

    let (cleaned, calls) = parse("text <tool_call>{\"name\":\"x\"");
    assert!(calls.is_empty());
    assert_eq!(cleaned, "text <tool_call>{\"name\":\"x\"");
}

#[test]
fn prose_mention_without_closing_angle_is_not_a_tag() {
    let (cleaned, calls) = parse("wrap it in <tool_call and go");
    assert!(calls.is_empty());
    assert_eq!(cleaned, "wrap it in <tool_call and go");
}

#[test]
fn plain_text_is_returned_verbatim() {
    let (cleaned, calls) = parse("just a normal answer");
    assert!(calls.is_empty());
    assert_eq!(cleaned, "just a normal answer");
}

#[test]
fn fenced_tool_call_block_parses() {
    let markdown =
        "lead\n```tool_call\n{\"name\":\"echo\",\"arguments\":{\"value\":\"four\"}}\n```\ntrail";
    let (text, calls) = parse(markdown);
    assert_eq!(text, "lead\ntrail");
    assert_eq!(calls.len(), 1);
}

#[test]
fn fenced_tool_calls_plural_block_parses() {
    // `protected::TOOL_CALL_LANGUAGES` already classifies `tool_calls`
    // (plural) as a call language, not a protected code example; this
    // grammar must recognize it as an opener too, not just decline to
    // protect it.
    let markdown = "before\n```tool_calls\n[{\"name\":\"a\",\"arguments\":{}}]\n```\nafter";
    let (text, calls) = parse(markdown);
    assert_eq!(text, "before\nafter");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "a");
}

#[test]
fn fenced_block_closed_by_stray_tag_parses() {
    let hybrid = "```tool_call\n{\"name\":\"echo\",\"arguments\":{}}\n</tool_call>\nrest";
    let (text, calls) = parse(hybrid);
    assert_eq!(calls.len(), 1);
    assert_eq!(text, "rest");
}

#[test]
fn a_tag_body_may_carry_its_own_json_fence() {
    let text = "<tool_call>\n```json\n{\"name\": \"shell\", \"arguments\": {\"command\": \"ls\"}}\n```\n</tool_call>";
    let (_, calls) = parse(text);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "shell");
}

#[test]
fn a_tag_body_may_hold_several_calls() {
    let response = r#"<tool_call>{"name":"get_weather","arguments":{"city":"London"}}{"name":"get_time","arguments":{"tz":"UTC"}}</tool_call>"#;
    let (_, calls) = parse(response);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].name, "get_weather");
    assert_eq!(calls[1].name, "get_time");
}

#[test]
fn a_tagged_body_honours_argument_key_aliases() {
    for alias in ["args", "parameters", "params", "input"] {
        let text =
            format!(r#"<tool_call>{{"name":"shell","{alias}":{{"command":"ls"}}}}</tool_call>"#);
        let (_, calls) = parse(&text);
        assert_eq!(calls.len(), 1, "{alias}");
        assert_eq!(calls[0].arguments["command"], "ls", "{alias}");
    }
}

#[test]
fn a_tagged_body_with_relaxed_json_is_repaired() {
    let (_, calls) =
        parse(r#"<tool_call>{name:"get_weather",arguments:{city:"Paris"}}</tool_call>"#);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["city"], "Paris");
}

// ── garbled sentinel pipes leaked into the markers ──────────────────────────

#[test]
fn garbled_pipe_tags_with_json_body_and_call_prefix_parse() {
    let garbled = r#"<|tool_call>call:{"name": "GMAIL_LIST_THREADS", "arguments": {"query": "\"University of Colorado\"", "verbose": true}}<tool_call|>"#;
    let (_text, calls) = parse(garbled);
    assert_eq!(calls.len(), 1, "expected the garbled call to be recovered");
    assert_eq!(calls[0].name, "GMAIL_LIST_THREADS");
    assert_eq!(calls[0].arguments["query"], "\"University of Colorado\"");
    assert_eq!(calls[0].arguments["verbose"], true);
}

#[test]
fn garbled_pipe_tags_recover_multiple_parallel_calls() {
    let garbled = concat!(
        r#"<|tool_call>call:{"name": "GMAIL_LIST_THREADS", "arguments": {"query": "a"}}<tool_call|>"#,
        r#"<|tool_call>call:{"name": "GMAIL_LIST_THREADS", "arguments": {"query": "b"}}<tool_call|>"#,
    );
    let (_t, calls) = parse(garbled);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].arguments["query"], "a");
    assert_eq!(calls[1].arguments["query"], "b");
}

#[test]
fn symmetric_pipe_tags_pair_positionally() {
    let garbled = r#"<|tool_call|>{"name":"echo","arguments":{"msg":"hi"}}<|tool_call|>"#;
    let (_t, calls) = parse(garbled);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["msg"], "hi");

    let (_t, calls) = parse(r#"<tool_call>{"name":"b","arguments":{}}</tool_call|>"#);
    assert_eq!(calls.len(), 1);
}

#[test]
fn pformat_pipes_in_a_body_are_not_garbled_tags() {
    let mut registry = PFormatRegistry::new();
    registry.insert(
        "get_weather".into(),
        PFormatToolParams::from_schema(&serde_json::json!({
            "type": "object",
            "properties": { "city": { "type": "string" }, "unit": { "type": "string" } },
            "required": ["city", "unit"]
        })),
    );
    let (_, calls) = parse_tool_calls_with_pformat(
        "<tool_call>get_weather[0|London|1|metric]</tool_call>",
        &registry,
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["city"], "London");
    assert_eq!(calls[0].source, CallSource::PFormat);
}

// ── Kimi `NAME{…}` bodies (#5119) ───────────────────────────────────────────

#[test]
fn kimi_name_brace_body_with_quote_sentinels_parses() {
    let garbled = r#"<|tool_call>call:GMAIL_FETCH_EMAILS{label_ids:[<|"|>INBOX<|"|>],max_results:1,verbose:true}<tool_call|>"#;
    let (_text, calls) = parse(garbled);
    assert_eq!(calls.len(), 1, "the garbled Kimi call must be recovered");
    assert_eq!(calls[0].name, "GMAIL_FETCH_EMAILS");
    assert_eq!(
        calls[0].arguments["label_ids"],
        serde_json::json!(["INBOX"])
    );
    assert_eq!(calls[0].arguments["max_results"], 1);
    assert_eq!(calls[0].arguments["verbose"], true);
}

#[test]
fn kimi_name_brace_body_integer_only_parses() {
    let garbled = r"<|tool_call>call:GMAIL_FETCH_EMAILS{max_results:5}<tool_call|>";
    let (_text, calls) = parse(garbled);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["max_results"], 5);
}

#[test]
fn sentinel_body_recovery_leaves_other_shapes_alone() {
    assert!(recover_sentinel_body(r#"{"name":"echo","arguments":{}}"#).is_none());
    assert!(recover_sentinel_body("get_weather[London|metric]").is_none());
    assert!(recover_sentinel_body("FOO{a:1} trailing").is_none());
}

// ── P-Format registry interplay ─────────────────────────────────────────────

fn echo_registry() -> PFormatRegistry {
    let mut reg = PFormatRegistry::new();
    reg.insert(
        "echo".to_string(),
        PFormatToolParams::from_schema(&serde_json::json!({
            "type": "object",
            "properties": { "value": { "type": "string" } }
        })),
    );
    reg
}

#[test]
fn a_pformat_tag_does_not_suppress_a_sibling_glm_tag() {
    let response =
        "<tool_call>echo[0|hello]</tool_call>\n<tool_call>shell/command>ls -la</tool_call>";
    let (_narrative, calls) = parse_tool_calls_with_pformat(response, &echo_registry());
    let names: Vec<&str> = calls.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["echo", "shell"]);
}

#[test]
fn a_pformat_tag_does_not_suppress_a_sibling_fenced_json_tag() {
    let response = "<tool_call>echo[0|hello]</tool_call>\n<tool_call>\n```json\n{\"name\": \"shell\", \"arguments\": {\"command\": \"ls\"}}\n```\n</tool_call>";
    let (_narrative, calls) = parse_tool_calls_with_pformat(response, &echo_registry());
    let names: Vec<&str> = calls.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["echo", "shell"]);
}

#[test]
fn a_json_body_is_not_double_counted_by_the_glm_fallback() {
    let response = "<tool_call>echo[0|hello]</tool_call>\n<tool_call>{\"name\": \"shell\", \"arguments\": {\"command\": \"cat a/b>c\"}}</tool_call>";
    let (_narrative, calls) = parse_tool_calls_with_pformat(response, &echo_registry());
    assert_eq!(calls.iter().filter(|c| c.name == "shell").count(), 1);
}

#[test]
fn a_tagged_body_with_a_registry_still_honours_argument_key_aliases() {
    let response = "<tool_call>echo[0|hello]</tool_call>\n<tool_call>{\"name\": \"shell\", \"args\": {\"command\": \"ls\"}}</tool_call>";
    let (_narrative, calls) = parse_tool_calls_with_pformat(response, &echo_registry());
    let shell = calls
        .iter()
        .find(|c| c.name == "shell")
        .expect("aliased tagged call");
    assert_eq!(shell.arguments["command"], "ls");
}

#[test]
fn pformat_registry_with_plain_text_yields_nothing() {
    let (text, calls) = parse_tool_calls_with_pformat("plain text", &echo_registry());
    assert_eq!(text, "plain text");
    assert!(calls.is_empty());
}

#[test]
fn pformat_call_and_claude_invoke_both_survive() {
    let (_, calls) = parse_tool_calls_with_pformat(
        "<tool_call>echo[0|hi]</tool_call><invoke name=\"other\"><parameter name=\"x\">y</parameter></invoke>",
        &echo_registry(),
    );
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].name, "echo");
    assert_eq!(calls[1].name, "other");
    assert_eq!(calls[1].arguments, serde_json::json!({"x": "y"}));
}

// ── Code-call bodies ────────────────────────────────────────────────────────

#[test]
fn a_code_call_body_parses_with_its_own_source() {
    let response = "Looking.\n<tool_call>\necho(value=\"hello\")\n</tool_call>\ndone";
    let (narrative, calls) = parse_tool_calls_with_pformat(response, &echo_registry());
    assert_eq!(narrative, "Looking.\ndone");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "echo");
    assert_eq!(calls[0].arguments, serde_json::json!({"value": "hello"}));
    assert_eq!(calls[0].source, CallSource::Code);
    assert!(calls[0].id.is_none(), "text calls never carry an id");
}

#[test]
fn code_calls_need_a_registry() {
    // Without a registry there is no layout to bind against, so the body is
    // a recognised block with no call in it — exactly the P-Format rule.
    let (_, calls) = parse("<tool_call>echo(value=\"hello\")</tool_call>");
    assert!(calls.is_empty());
}

#[test]
fn several_code_calls_in_one_tag_keep_source_order() {
    let response = "<tool_call>\necho(\"a\")\necho(value=\"b\");\n</tool_call>";
    let (_, calls) = parse_tool_calls_with_pformat(response, &echo_registry());
    let values: Vec<&str> = calls
        .iter()
        .map(|c| c.arguments["value"].as_str().unwrap())
        .collect();
    assert_eq!(values, ["a", "b"]);
    assert!(calls.iter().all(|c| c.source == CallSource::Code));
}

#[test]
fn a_code_call_wrapped_in_a_python_fence_inside_the_tag_is_unwrapped() {
    let response = "<tool_call>\n```python\necho(value=\"hi\")\n```\n</tool_call>";
    let (_, calls) = parse_tool_calls_with_pformat(response, &echo_registry());
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].source, CallSource::Code);
}

#[test]
fn a_top_level_python_fence_is_an_example_not_a_call() {
    let response = "Like this:\n```python\necho(value=\"hi\")\n```\n";
    let (text, calls) = parse_tool_calls_with_pformat(response, &echo_registry());
    assert!(calls.is_empty());
    assert!(text.contains("echo(value=\"hi\")"));
}

#[test]
fn a_fenced_tool_call_block_holds_a_code_call() {
    let response = "```tool_call\necho(value=\"hi\")\n```";
    let (_, calls) = parse_tool_calls_with_pformat(response, &echo_registry());
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].source, CallSource::Code);
}

#[test]
fn code_pformat_json_and_glm_siblings_all_survive() {
    let response = "<tool_call>echo(value=\"c\")</tool_call>\n\
                    <tool_call>echo[0|p]</tool_call>\n\
                    <tool_call>{\"name\": \"shell\", \"arguments\": {\"command\": \"ls\"}}</tool_call>\n\
                    <tool_call>shell/command>pwd</tool_call>";
    let (_, calls) = parse_tool_calls_with_pformat(response, &echo_registry());
    let sources: Vec<CallSource> = calls.iter().map(|c| c.source).collect();
    assert_eq!(
        sources,
        [
            CallSource::Code,
            CallSource::PFormat,
            CallSource::TaggedJson,
            CallSource::Glm
        ]
    );
}

#[test]
fn a_prose_body_mentioning_a_call_is_malformed_not_a_call() {
    let response = "<tool_call>I will call echo(value=\"hi\") now</tool_call>";
    let (_, calls) = parse_tool_calls_with_pformat(response, &echo_registry());
    assert!(calls.is_empty());
}

#[test]
fn a_code_call_to_an_unknown_tool_is_not_a_call() {
    let response = "<tool_call>rm_rf(path=\"/\")</tool_call>";
    let (_, calls) = parse_tool_calls_with_pformat(response, &echo_registry());
    assert!(calls.is_empty());
}

// ── Doubled tags ────────────────────────────────────────────────────────────
//
// `DeepSeek` V4 under a code dialect wraps the block twice:
// `<tool_call>\n<tool_call>\nNAME(...)\n</tool_call>\n</tool_call>`. Positional
// pairing used to close the first tag on the empty body and drop the call,
// and the whole thing leaked into the visible reply.

#[test]
fn a_doubled_opener_is_one_block_and_its_extra_closer_is_swallowed() {
    let response = "I'll search.\n\n<tool_call>\n<tool_call>\necho(value=\"kashmir\")\n</tool_call>\n</tool_call>";
    let (narrative, calls) = parse_tool_calls_with_pformat(response, &echo_registry());
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].name, "echo");
    assert_eq!(calls[0].arguments, serde_json::json!({"value": "kashmir"}));
    assert_eq!(narrative, "I'll search.");
    assert!(!narrative.contains("tool_call"), "{narrative:?}");
}

#[test]
fn a_doubled_opener_around_a_json_body_parses_too() {
    let (text, calls) = parse(
        "<tool_call>\n<tool_call>\n{\"name\":\"echo\",\"arguments\":{\"value\":\"x\"}}\n</tool_call>\n</tool_call>\nafter",
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments, serde_json::json!({"value": "x"}));
    assert_eq!(text, "after");
}

#[test]
fn two_adjacent_blocks_are_still_two_blocks() {
    // The doubled-opener rule only fires on a whitespace-only gap; a real
    // body between two openers is still the first block's body.
    let text = "<tool_call>{\"name\":\"one\",\"arguments\":{}}</tool_call><tool_call>{\"name\":\"two\",\"arguments\":{}}</tool_call>";
    let (_, calls) = parse(text);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].name, "one");
    assert_eq!(calls[1].name, "two");
}

#[test]
fn a_doubled_opener_does_not_swallow_an_unrelated_closing_tag() {
    // Only the extra `</tool_call>` a doubled opener leaves behind is
    // protocol furniture; a real closing tag right after it (`</div>`, from
    // whatever markup the model echoed) is narrative and must survive.
    let (text, calls) = parse(
        "<tool_call>\n<tool_call>\n{\"name\":\"echo\",\"arguments\":{}}\n</tool_call>\n</div>visible",
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(text, "</div>visible");
}

#[test]
fn a_doubled_opener_swallows_a_pipe_form_duplicate_closer() {
    // `TAG_RE` matches the pipe-form closer `<|/tool_call|>` too, so the
    // doubled-opener path must recognize it as a closer to swallow, not
    // leave it dangling in `rest` for `strip_leading_close_tags` (which only
    // understands `</...>`) to miss.
    let (text, calls) = parse(
        "<|tool_call|>\n<|tool_call|>\n{\"name\":\"echo\",\"arguments\":{}}\n<|/tool_call|>\n<|/tool_call|>\nafter",
    );
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(text, "after");
}

#[test]
fn a_bare_trailing_opener_is_dropped_not_shown() {
    // An abandoned block at the end of a reply carries no call and no
    // information; showing `<tool_call>` to the user is never right.
    let (text, calls) = parse("Let me fetch a few sites directly.\n\n<tool_call>\n");
    assert!(calls.is_empty());
    assert_eq!(text, "Let me fetch a few sites directly.");
    // A block with real (if unparseable) content is still kept as text.
    let (text, _) = parse("before <tool-call>not-json");
    assert_eq!(text, "before <tool-call>not-json");
}
