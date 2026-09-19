//! `<invoke>` XML: Claude, `DeepSeek` DSML, namespaced, and `<function=>` forms.

use super::{parse, parse_known};
use crate::types::CallSource;
use crate::{PFormatRegistry, parse_tool_calls_with_pformat};

#[test]
fn claude_invoke_blocks_preserve_typed_parameters() {
    let source = concat!(
        "before <invoke name=\"run\">",
        "<parameter name=\"number\">42</parameter>",
        "<parameter name=\"flag\">true</parameter>",
        "<parameter name=\"empty\"> </parameter>",
        "<parameter name=\"\">ignored</parameter></invoke> after"
    );
    let (text, calls) = parse(source);
    assert_eq!(text, "before\nafter");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "run");
    assert_eq!(calls[0].arguments["number"], 42);
    assert_eq!(calls[0].arguments["flag"], true);
    assert_eq!(calls[0].arguments["empty"], "");
    assert_eq!(calls[0].source, CallSource::InvokeXml);
}

#[test]
fn unclosed_invoke_is_kept_as_text() {
    let malformed = "lead <invoke name=\"broken\"><parameter name=\"x\">1</parameter>";
    let (text, calls) = parse(malformed);
    assert_eq!(
        text,
        "lead <invoke name=\"broken\"><parameter name=\"x\">1</parameter>"
    );
    assert!(calls.is_empty());
}

#[test]
fn anthropic_function_calls_wrapper_is_stripped() {
    let source = "<function_calls>\n<invoke name=\"read\">\n<parameter name=\"path\">a.txt</parameter>\n</invoke>\n</function_calls>";
    let (text, calls) = parse(source);
    assert!(text.is_empty(), "{text:?}");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["path"], "a.txt");
}

#[test]
fn namespaced_invoke_from_muse_spark_parses() {
    let source = "<atem:function_calls><atem:invoke name=\"default.terminal\"><atem:parameter name=\"command\">echo hi</atem:parameter></atem:invoke></atem:function_calls>";
    let (text, calls) = parse(source);
    assert!(text.is_empty());
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "default.terminal");
    assert_eq!(calls[0].arguments["command"], "echo hi");
}

#[test]
fn function_equals_form_with_parameter_children_parses() {
    let source = "<function=get_weather><parameter=city>Paris</parameter><parameter=days>3</parameter></function>";
    let (_, calls) = parse(source);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "get_weather");
    assert_eq!(
        calls[0].arguments,
        serde_json::json!({"city": "Paris", "days": 3})
    );
}

#[test]
fn function_equals_form_with_json_body_parses() {
    let (_, calls) = parse("<function=get_weather>{\"city\":\"Paris\"}</function>");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["city"], "Paris");
}

#[test]
fn gemma_function_name_attribute_form_parses() {
    let (_, calls) =
        parse("<function name=\"read\"><parameter name=\"path\">x</parameter></function>");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "read");
}

// ── DeepSeek DSML ───────────────────────────────────────────────────────────

#[test]
fn dsml_parameter_with_arguments_envelope_parses() {
    let response = concat!(
        "<｜｜DSML｜｜ calls>\n",
        "<｜｜DSML｜｜ invoke name=\"GMAIL_FETCH_EMAILS\">\n",
        "<｜｜DSML｜｜ parameter name=\"arguments\" string=\"false\">{\"max_results\": 10, \"query\": \"in:inbox\", \"user_id\": \"me\"}</｜｜DSML｜｜ parameter>\n",
        "</｜｜DSML｜｜ invoke>\n",
        "</｜｜DSML｜｜ calls>"
    );
    let (narrative, calls) = parse(response);
    assert!(narrative.is_empty(), "{narrative:?}");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "GMAIL_FETCH_EMAILS");
    assert_eq!(calls[0].arguments["max_results"], 10);
    assert_eq!(calls[0].arguments["query"], "in:inbox");
    assert_eq!(calls[0].arguments["user_id"], "me");
}

#[test]
fn dsml_invoke_with_direct_json_body_parses() {
    let response = concat!(
        "<｜｜DSML｜｜ calls>\n",
        "<｜｜DSML｜｜ invoke name=\"composio_list_tools\">\n",
        "{\"toolkits\":[\"gmail\"]}\n",
        "</｜｜DSML｜｜ invoke>\n",
        "</｜｜DSML｜｜ calls>"
    );
    let (_narrative, calls) = parse(response);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "composio_list_tools");
    assert_eq!(calls[0].arguments["toolkits"], serde_json::json!(["gmail"]));
}

#[test]
fn dsml_invoke_with_orphan_closing_parameter_tag_parses() {
    let response = concat!(
        "<｜｜DSML｜｜ calls>\n",
        "<｜｜DSML｜｜ invoke name=\"GMAIL_FETCH_EMAILS\">\n",
        "{\"label_ids\": [\"INBOX\"], \"ids_only\": true, \"max_results\": 500, \"include_payload\": false, \"verbose\": false}\n",
        "</｜｜DSML｜｜ parameter>\n",
        "</｜｜DSML｜｜ invoke>\n",
        "</｜｜DSML｜｜ calls>"
    );
    let (_narrative, calls) = parse(response);
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].arguments["label_ids"],
        serde_json::json!(["INBOX"])
    );
    assert_eq!(calls[0].arguments["max_results"], 500);
}

#[test]
fn dsml_multiple_invokes_with_empty_args_and_narrative_text_parses() {
    let response = concat!(
        "I'll verify Gmail access by fetching the profile and listing recent inbox messages.\n\n",
        "<｜｜DSML｜｜ calls>\n",
        "<｜｜DSML｜｜ invoke name=\"GMAIL_GET_PROFILE\">\n\n",
        "</｜｜DSML｜｜ invoke>\n",
        "<｜｜DSML｜｜ invoke name=\"GMAIL_FETCH_EMAILS\">\n",
        "{\"label_ids\": [\"INBOX\"], \"max_results\": 5, \"verbose\": false}\n",
        "</｜｜DSML｜｜ invoke>\n",
        "</｜｜DSML｜｜ calls>"
    );
    let (narrative, calls) = parse(response);
    assert_eq!(
        narrative,
        "I'll verify Gmail access by fetching the profile and listing recent inbox messages."
    );
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].name, "GMAIL_GET_PROFILE");
    assert_eq!(calls[0].arguments, serde_json::json!({}));
    assert_eq!(calls[1].name, "GMAIL_FETCH_EMAILS");
    assert_eq!(calls[1].arguments["max_results"], 5);
}

#[test]
fn dsml_parameter_with_named_arguments_parses() {
    let response = concat!(
        "<｜｜DSML｜｜ calls>\n",
        "<｜｜DSML｜｜ invoke name=\"composio_list_tools\">\n",
        "<｜｜DSML｜｜ parameter name=\"toolkits\" string=\"true\">[\"twitter\"]</｜｜DSML｜｜ parameter>\n",
        "</｜｜DSML｜｜ invoke>\n",
        "</｜｜DSML｜｜ calls>"
    );
    let (_narrative, calls) = parse(response);
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].arguments["toolkits"],
        serde_json::json!(["twitter"])
    );
}

#[test]
fn dsml_mixed_tool_call_closing_tag_parses() {
    let response = concat!(
        "<｜｜DSML｜｜ calls>\n",
        "<｜｜DSML｜｜ invoke name=\"GMAIL_FETCH_EMAILS\">\n",
        "{\"label_ids\": [\"INBOX\"], \"max_results\": 2}\n",
        "</tool_call>"
    );
    let (_narrative, calls) = parse(response);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "GMAIL_FETCH_EMAILS");
    assert_eq!(calls[0].arguments["max_results"], 2);
}

#[test]
fn dsml_with_pformat_registry_recovers_cleanly() {
    let reg = PFormatRegistry::new();
    let response = concat!(
        "<｜｜DSML｜｜ calls>\n",
        "<｜｜DSML｜｜ invoke name=\"GMAIL_FETCH_EMAILS\">\n",
        "<｜｜DSML｜｜ parameter name=\"arguments\">{\"label_ids\": [\"INBOX\"], \"max_results\": 3}</｜｜DSML｜｜ parameter>\n",
        "</｜｜DSML｜｜ invoke>\n",
        "</｜｜DSML｜｜ calls>"
    );
    let (_narrative, calls) = parse_tool_calls_with_pformat(response, &reg);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["max_results"], 3);
}

#[test]
fn dsml_single_bar_ascii_form_parses() {
    let response = "<|DSML|tool_calls><|DSML|invoke name=\"read\">{\"path\":\"/tmp/repro.md\"}</|DSML|invoke></|DSML|tool_calls>";
    let (text, calls) = parse(response);
    assert!(text.is_empty(), "{text:?}");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "read");
    assert_eq!(calls[0].arguments["path"], "/tmp/repro.md");
}

#[test]
fn dsml_fullwidth_single_bar_with_string_parameter_keeps_markers_inside_values() {
    let response = concat!(
        "<｜DSML｜tool_calls><｜DSML｜invoke name=\"message\">",
        "<｜DSML｜parameter name=\"text\" string=\"true\">literal <｜DSML｜tool_calls> marker</｜DSML｜parameter>",
        "</｜DSML｜invoke></｜DSML｜tool_calls>"
    );
    let (_, calls) = parse(response);
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].arguments["text"],
        "literal <｜DSML｜tool_calls> marker"
    );
}

#[test]
fn dsml_incomplete_invoke_prefix_before_a_valid_invoke_is_ignored() {
    let response = "<|DSML|tool_calls>literal <|DSML|invoke marker <|DSML|invoke name=\"read\">{\"path\":\"/tmp/valid.md\"}</|DSML|invoke></|DSML|tool_calls>";
    let (_, calls) = parse(response);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["path"], "/tmp/valid.md");
}

#[test]
fn dsml_markers_never_leak_into_the_narrative() {
    let response = "Sure.\n<｜DSML｜tool_calls>\n<｜DSML｜invoke name=\"x\">{}</｜DSML｜invoke>\n</｜DSML｜tool_calls>\nDone.";
    let outcome = parse_known(response, &["x"]);
    assert!(!outcome.text.contains("DSML"), "{}", outcome.text);
    assert_eq!(outcome.text, "Sure.\nDone.");
}
