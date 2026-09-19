//! gpt-oss Harmony and Mistral `[TOOL_CALLS]`.

use super::{parse, parse_known};
use crate::types::{CallSource, ParseDiagnostic};

#[test]
fn harmony_commentary_call_parses() {
    let response = "<|channel|>commentary to=functions.get_weather <|constrain|>json<|message|>{\"city\":\"Paris\"}<|call|>";
    let (text, calls) = parse(response);
    assert!(text.is_empty(), "{text:?}");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "get_weather");
    assert_eq!(calls[0].arguments["city"], "Paris");
    assert_eq!(calls[0].source, CallSource::Harmony);
}

#[test]
fn harmony_channel_without_target_is_not_a_call() {
    let response = "<|channel|>analysis<|message|>thinking hard<|end|>final answer";
    let (text, calls) = parse(response);
    assert!(calls.is_empty());
    assert_eq!(text, response);
}

#[test]
fn harmony_call_with_start_prefix_and_no_terminator_parses_in_batch() {
    let response =
        "<|start|>assistant<|channel|>commentary to=functions.read<|message|>{\"path\":\"a\"}";
    let (_, calls) = parse(response);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "read");
}

#[test]
fn harmony_channel_with_target_but_no_message_is_not_a_call_in_batch_mode() {
    // No `<|message|>` ever arrives, so a batch parse cannot know whether a
    // call is coming; the header is left as ordinary text rather than
    // guessed at.
    let response = "<|channel|>commentary to=functions.read still thinking";
    let (text, calls) = parse(response);
    assert!(calls.is_empty());
    assert_eq!(text, response);
}

#[test]
fn harmony_channel_with_empty_target_is_skipped_and_next_call_found() {
    // `to=` with nothing but whitespace after it names no tool, so the
    // grammar skips past that channel message and keeps scanning for a
    // later one that does.
    let response = "<|channel|>commentary to= <|message|>ignored<|end|><|channel|>commentary to=functions.read<|message|>{\"path\":\"a\"}<|call|>tail";
    let (text, calls) = parse(response);
    assert_eq!(text, "tail");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "read");
    assert_eq!(calls[0].arguments["path"], "a");
}

#[test]
fn mistral_v3_array_form_parses() {
    let response =
        "[TOOL_CALLS] [{\"name\": \"get_weather\", \"arguments\": {\"city\": \"Paris\"}}]";
    let (text, calls) = parse(response);
    assert!(text.is_empty(), "{text:?}");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "get_weather");
    assert_eq!(calls[0].source, CallSource::Mistral);
}

#[test]
fn mistral_v11_name_args_form_parses() {
    let response = "Sure. [TOOL_CALLS]get_weather[ARGS]{\"city\":\"Paris\"}";
    let (text, calls) = parse(response);
    assert_eq!(text, "Sure.");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["city"], "Paris");
}
