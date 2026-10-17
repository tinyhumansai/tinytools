//! A response that is entirely one JSON value.

use super::{parse, parse_known};
use crate::types::{CallSource, ParseOptions, Termination};

#[test]
fn a_wire_message_with_tool_calls_array_parses() {
    let native = serde_json::json!({
        "content": "native text",
        "tool_calls": [ { "name": "echo", "arguments": { "value": "one" } } ]
    })
    .to_string();
    let (text, calls) = parse(&native);
    assert_eq!(text, "native text");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].source, CallSource::BareJson);
}

#[test]
fn a_bare_object_with_canonical_arguments_parses() {
    let (text, calls) = parse(r#"{"name":"echo","arguments":{"value":"hi"}}"#);
    assert_eq!(text.len(), 0);
    assert_eq!(calls.len(), 1);
}

#[test]
fn a_bare_object_with_only_an_alias_is_plain_text() {
    let (text, calls) = parse(r#"{"name":"Alice","input":{"value":"hi"}}"#);
    assert!(
        calls.is_empty(),
        "a JSON answer must not become a phantom call"
    );
    assert_eq!(text, r#"{"name":"Alice","input":{"value":"hi"}}"#);
}

#[test]
fn a_bare_object_naming_a_known_tool_may_use_an_alias() {
    let outcome = parse_known(
        r#"{"name":"get_weather","parameters":{"city":"Paris"}}"#,
        &["get_weather"],
    );
    assert_eq!(outcome.calls.len(), 1);
    assert_eq!(outcome.calls[0].arguments["city"], "Paris");
}

#[test]
fn llama_bare_object_with_mismatched_quotes_is_repaired() {
    let outcome = parse_known(
        r#"{"name":"get_weather","parameters':{'city':"Paris"}}"#,
        &["get_weather"],
    );
    assert_eq!(outcome.calls.len(), 1);
    assert_eq!(
        outcome.calls[0].arguments,
        serde_json::json!({ "city": "Paris" })
    );
    assert_eq!(outcome.text.len(), 0);
}

#[test]
fn a_bare_object_inside_a_code_fence_parses() {
    let (_, calls) =
        parse("```json\n{\"name\":\"get_weather\",\"arguments\":{\"city\":\"Paris\"}}\n```");
    assert_eq!(calls.len(), 1);
}

#[test]
fn bare_recovery_never_swallows_a_genuine_text_answer() {
    for text in [
        "The weather in Paris is mild today.",
        r#"You could send {"name":"get_weather"} to that endpoint."#,
        r#"{"city":"Paris","temperature":17}"#,
        r#"{"name":42}"#,
        r#""just a string""#,
        "[1, 2, 3]",
        // A damaged leading object followed by unrelated trailing prose (or
        // another object) must not be recovered via the trailing-noise rung
        // that `recover_object` allows for marker-delimited call bodies —
        // bare JSON has no such marker, so the whole response must be the
        // call.
        r#"{"name":"shell","arguments":{"command":"x"}} explanation {}"#,
    ] {
        let (cleaned, calls) = parse(text);
        assert!(
            calls.is_empty(),
            "{text:?} must not be recovered as a tool call"
        );
        assert_eq!(cleaned, text);
    }
}

#[test]
fn parse_options_default_matches_new_and_allows_bare_json() {
    // The struct doc says the default allows bare JSON; a derived `Default`
    // would instead leave `allow_bare_json` at `bool`'s `false`.
    let outcome = crate::parse::parse_text(
        r#"{"name":"echo","arguments":{}}"#,
        &ParseOptions::default(),
    );
    assert_eq!(outcome.calls.len(), 1);
}

#[test]
fn bare_json_can_be_disabled() {
    let options = ParseOptions::new().without_bare_json();
    let outcome = crate::parse::parse_text(r#"{"name":"echo","arguments":{}}"#, &options);
    assert_eq!(outcome.calls.len(), 0);
}

#[test]
fn a_bare_array_of_calls_parses() {
    let array = serde_json::json!([
        { "name": "echo", "arguments": { "value": "two" } },
        { "name": "   " }
    ])
    .to_string();
    let (_, calls) = parse(&array);
    assert_eq!(calls.len(), 1);
}

// ── Termination (tinyhumansai/tinytools#50) ────────────────────────────────
//
// `recover_whole_object` closes any `{` or `[` a cut landed inside of, so a
// bare-JSON reply the provider stopped at its output limit still reads as a
// call. The call records that repair so a host can refuse it.

#[test]
fn a_complete_bare_object_is_terminated() {
    let (_, calls) = parse(r#"{"name":"echo","arguments":{"value":"hi"}}"#);
    assert_eq!(calls[0].termination, Termination::Terminated);
    assert!(!calls[0].may_be_truncated());
}

#[test]
fn a_bare_object_whose_closers_were_balanced_is_flagged() {
    // The cut landed between values — after the inner `}` and before the
    // outer one — which is the only cut this gated grammar recovers from.
    let (_, calls) = parse(r#"{"name":"echo","arguments":{"value":"hi"}"#);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["value"], "hi");
    assert_eq!(calls[0].termination, Termination::ClosersBalanced);
    assert!(calls[0].may_be_truncated());
}

#[test]
fn every_call_in_a_closer_balanced_envelope_is_flagged() {
    // The cut ended the whole reply; no call in it can be told apart.
    let (_, calls) =
        parse(r#"{"tool_calls":[{"name":"a","arguments":{}},{"name":"b","arguments":{"x":1}}"#);
    assert_eq!(calls.len(), 2);
    assert!(
        calls
            .iter()
            .all(|call| call.termination == Termination::ClosersBalanced)
    );
}

#[test]
fn a_bare_object_repaired_by_another_rung_is_not_flagged() {
    let outcome = parse_known(
        r#"{"name":"get_weather","parameters':{'city':"Paris"}}"#,
        &["get_weather"],
    );
    assert_eq!(outcome.calls.len(), 1);
    assert_eq!(outcome.calls[0].termination, Termination::Terminated);
    assert!(!outcome.calls[0].may_be_truncated());
}

#[test]
fn a_bare_object_with_surplus_closers_is_not_flagged() {
    // A trimmed surplus `}` is the model overshooting, not a cut.
    let (_, calls) = parse(r#"{"name":"echo","arguments":{"value":"hi"}}}"#);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].termination, Termination::Terminated);
}
