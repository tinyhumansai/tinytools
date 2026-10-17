//! gpt-oss Harmony and Mistral `[TOOL_CALLS]`.

use super::{parse, parse_known};
use crate::types::{CallSource, ParseDiagnostic, Termination};

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
    assert_eq!(calls.len(), 0);
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
fn harmony_start_prefix_is_consumed_as_furniture() {
    // `<|start|>assistant` is documented as furniture that precedes the
    // first channel of a turn; it must not leak into the narrative.
    let response = "<|start|>assistant<|channel|>commentary to=functions.read<|message|>{\"path\":\"a\"}<|call|>";
    let (text, calls) = parse(response);
    assert!(text.is_empty(), "{text:?}");
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
    assert_eq!(calls.len(), 0);
    assert_eq!(text, response);
}

#[test]
fn harmony_channel_with_empty_target_is_skipped_and_next_call_found() {
    // `to=` with nothing but whitespace after it names no tool, so the
    // grammar skips past that channel message and keeps scanning for a
    // later one that does.
    let response = "<|channel|>commentary to= <|message|>ignored<|end|><|channel|>commentary to=functions.read<|message|>{\"path\":\"a\"}<|call|>tail";
    let (text, calls) = parse(response);
    assert_eq!(
        text,
        "<|channel|>commentary to= <|message|>ignored<|end|>\ntail"
    );
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
fn mistral_v3_array_of_non_call_objects_yields_no_calls() {
    // The array parses as JSON but contains no `name`/`arguments` call
    // shape, so the v3 branch must not treat it as a found block — it
    // falls through to the (here, also empty) v11 scan and the response
    // is reported as malformed rather than silently dropped.
    let response = "[TOOL_CALLS] [{\"foo\":1}]";
    let outcome = parse_known(response, &[]);
    assert_eq!(outcome.calls.len(), 0);
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|d| matches!(d, ParseDiagnostic::MalformedBlock { .. }))
    );
}

#[test]
fn mistral_v11_name_with_invalid_characters_is_not_a_call() {
    let response = "[TOOL_CALLS]get-weather[ARGS]{\"city\":\"Paris\"}";
    let outcome = parse_known(response, &[]);
    assert_eq!(outcome.calls.len(), 0);
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
    assert_eq!(outcome.calls.len(), 0);
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|d| matches!(d, ParseDiagnostic::MalformedBlock { .. }))
    );
}

#[test]
fn mistral_v11_non_object_arguments_are_recovered_into_an_object() {
    let response = "[TOOL_CALLS]get_weather[ARGS][1,2,3]";
    let (_, calls) = parse(response);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "get_weather");
    assert!(calls[0].arguments.is_object());
}

#[test]
fn mistral_marker_with_no_parseable_call_is_malformed_in_batch_mode() {
    let response = "[TOOL_CALLS] this trails off with no call shape";
    let outcome = parse_known(response, &[]);
    assert_eq!(outcome.calls.len(), 0);
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
    // The trailing `!` cannot be part of a v11 tool name, so it disambiguates
    // the block as finished; trailing alphanumeric text alone would still be
    // ambiguous with a directly-appended continuation call and must be held
    // (see `mistral_v11_second_call_split_across_fragments_is_not_lost`).
    let second = s.feed("ther[ARGS]{\"city\":\"Paris\"}tail!");
    assert_eq!(second.text, "tail!");
    assert_eq!(second.calls[0].name, "get_weather");
}

#[test]
fn mistral_v11_second_call_split_mid_args_bracket_is_not_lost() {
    // The split falls after the second call's opening `[`, inside the
    // `[ARGS]` marker itself rather than inside its name — a stream
    // fragment boundary a naive name-only predicate does not recognize as
    // still-ambiguous.
    use crate::stream::StreamScrubber;

    let mut s = StreamScrubber::new();
    let first = s.feed("[TOOL_CALLS]a[ARGS]{}");
    assert!(first.calls.is_empty(), "must hold until disambiguated");
    let second = s.feed("b[");
    assert!(second.calls.is_empty(), "must still hold: {second:?}");
    let third = s.feed("ARGS]{}");
    assert!(
        third.calls.is_empty(),
        "the block ends exactly at the fragment boundary, still ambiguous: {third:?}"
    );
    let flushed = s.flush();
    assert_eq!(flushed.calls.len(), 2);
    assert_eq!(flushed.calls[0].name, "a");
    assert_eq!(flushed.calls[1].name, "b");
}

#[test]
fn mistral_v11_second_call_split_across_fragments_is_not_lost() {
    // The v11 form lets a second call follow directly with no fresh
    // `[TOOL_CALLS]` marker. A naive streamer that finalizes the block the
    // moment the first `NAME[ARGS]{...}` completes drops the second call
    // the instant a fragment boundary falls between them.
    use crate::stream::StreamScrubber;

    let mut s = StreamScrubber::new();
    let first = s.feed("[TOOL_CALLS]a[ARGS]{\"x\":1}");
    assert!(first.calls.is_empty(), "must hold until disambiguated");
    let second = s.feed("b[ARGS]{\"y\":2} done!");
    assert_eq!(second.calls.len(), 2);
    assert_eq!(second.calls[0].name, "a");
    assert_eq!(second.calls[1].name, "b");
    assert_eq!(second.text, " done!");
}

// ── Termination (tinyhumansai/tinytools#50) ────────────────────────────────
//
// Harmony in batch mode reads a payload with no `<|call|>` to the end of the
// text, which recovers a call the model may never have finished. The call
// records how its block ended so a host that also saw an output-limit stop
// can refuse exactly those calls.

#[test]
fn harmony_call_closed_by_its_terminator_is_terminated() {
    let response = "<|channel|>commentary to=functions.read<|message|>{\"path\":\"a\"}<|call|>";
    let (_, calls) = parse(response);
    assert_eq!(calls[0].termination, Termination::Terminated);
    assert!(!calls[0].may_be_truncated());
}

#[test]
fn harmony_call_that_ran_to_end_of_text_is_flagged() {
    let response = "<|channel|>commentary to=functions.read<|message|>{\"path\":\"a\"}";
    let (_, calls) = parse(response);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["path"], "a");
    assert_eq!(calls[0].termination, Termination::EndOfText);
    assert!(calls[0].may_be_truncated());
}

#[test]
fn harmony_end_of_text_wins_over_balanced_closers() {
    // Cut mid-object *and* no terminator: the stronger evidence is reported.
    let response = "<|channel|>commentary to=functions.read<|message|>{\"path\":\"a\",\"n\":[1";
    let (_, calls) = parse(response);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["n"], serde_json::json!([1]));
    assert_eq!(calls[0].termination, Termination::EndOfText);
}

#[test]
fn harmony_terminated_call_whose_payload_needed_closers_is_flagged() {
    let response =
        "<|channel|>commentary to=functions.read<|message|>{\"path\":\"a\",\"n\":[1<|call|>";
    let (_, calls) = parse(response);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].termination, Termination::ClosersBalanced);
    assert!(calls[0].may_be_truncated());
}

#[test]
fn harmony_payload_repaired_by_another_rung_is_not_flagged() {
    // A trailing comma is something the model wrote, not something it
    // never got to write; only appended closers mark a possible cut.
    let response = "<|channel|>commentary to=functions.read<|message|>{\"path\":\"a\",}<|call|>";
    let (_, calls) = parse(response);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["path"], "a");
    assert_eq!(calls[0].termination, Termination::Terminated);
}

#[test]
fn harmony_call_held_while_streaming_is_flagged_on_flush() {
    use crate::stream::StreamScrubber;

    let mut s = StreamScrubber::new();
    let held = s.feed("<|channel|>commentary to=functions.read<|message|>{\"path\":\"a\"}");
    assert!(held.calls.is_empty(), "no terminator yet: {held:?}");
    let flushed = s.flush();
    assert_eq!(flushed.calls.len(), 1);
    assert_eq!(flushed.calls[0].termination, Termination::EndOfText);
}
