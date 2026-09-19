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

#[test]
fn mistral_v3_array_of_non_call_objects_falls_through_to_v11_scan() {
    // The array parses as JSON but contains no `name`/`arguments` call
    // shape, so the v3 branch yields no calls and control must fall
    // through to the v11 `NAME[ARGS]{...}` scan rather than stopping.
    let response = "[TOOL_CALLS] [{\"foo\":1}] get_weather[ARGS]{\"city\":\"Paris\"}";
    let (_, calls) = parse(response);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "get_weather");
}

#[test]
fn mistral_v11_name_with_invalid_characters_is_not_a_call() {
    let response = "[TOOL_CALLS]get-weather[ARGS]{\"city\":\"Paris\"}";
    let outcome = parse_known(response, &[]);
    assert!(outcome.calls.is_empty());
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|d| matches!(d, ParseDiagnostic::MalformedBlock { .. }))
    );
}

#[test]
fn mistral_v11_args_with_unparseable_json_is_not_a_call() {
    let response = "[TOOL_CALLS]get_weather[ARGS]not json at all";
    let outcome = parse_known(response, &[]);
    assert!(outcome.calls.is_empty());
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|d| matches!(d, ParseDiagnostic::MalformedBlock { .. }))
    );
}

#[test]
fn mistral_v11_non_object_arguments_are_recovered_into_an_object() {
    let response = "[TOOL_CALLS]get_weather[ARGS]\"Paris\"";
    let (_, calls) = parse(response);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "get_weather");
    assert!(calls[0].arguments.is_object());
}

#[test]
fn mistral_marker_with_no_parseable_call_is_malformed_in_batch_mode() {
    let response = "[TOOL_CALLS] this trails off with no call shape";
    let outcome = parse_known(response, &[]);
    assert!(outcome.calls.is_empty());
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|d| matches!(d, ParseDiagnostic::MalformedBlock { .. }))
    );
    assert!(outcome.text.contains("this trails off with no call shape"));
}

#[test]
fn mistral_marker_with_no_call_yet_is_held_while_streaming() {
    use crate::stream::StreamScrubber;

    let mut s = StreamScrubber::new();
    let first = s.feed("[TOOL_CALLS]get_wea");
    assert_eq!(first.text, "", "a pending marker must be held");
    let second = s.feed("ther[ARGS]{\"city\":\"Paris\"}tail");
    assert_eq!(second.text, "tail");
    assert_eq!(second.calls[0].name, "get_weather");
}
