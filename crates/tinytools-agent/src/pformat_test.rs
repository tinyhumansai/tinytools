#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use serde_json::json;

fn make_registry() -> PFormatRegistry {
    let mut reg = PFormatRegistry::new();
    reg.insert(
        "get_weather".to_string(),
        PFormatToolParams::from_schema(&json!({
            "type": "object",
            "properties": {
                "location": { "type": "string" },
                "unit": { "type": "string" }
            }
        })),
    );
    reg.insert(
        "shell".to_string(),
        PFormatToolParams::from_schema(&json!({
            "type": "object",
            "properties": {
                "command": { "type": "string" }
            }
        })),
    );
    reg.insert(
        "ping".to_string(),
        PFormatToolParams::from_schema(&json!({
            "type": "object",
            "properties": {}
        })),
    );
    reg.insert(
        "math".to_string(),
        PFormatToolParams::from_schema(&json!({
            "type": "object",
            "properties": {
                "x": { "type": "integer" },
                "y": { "type": "number" },
                "verbose": { "type": "boolean" }
            }
        })),
    );
    reg
}

#[test]
fn renders_zero_arg_signature() {
    let reg = make_registry();
    assert_eq!(render_signature("ping", &reg["ping"]), "ping[]");
}

#[test]
fn renders_multi_arg_signature() {
    let reg = make_registry();
    assert_eq!(
        render_signature("get_weather", &reg["get_weather"]),
        "get_weather[0|<location>|1|<unit>]"
    );
}

#[test]
fn parses_simple_call() {
    let reg = make_registry();
    let (name, args) = parse_call("get_weather[0|London|1|metric]", &reg).unwrap();
    assert_eq!(name, "get_weather");
    assert_eq!(args, json!({"location": "London", "unit": "metric"}));
}

#[test]
fn parses_zero_arg_call() {
    let reg = make_registry();
    let (name, args) = parse_call("ping[]", &reg).unwrap();
    assert_eq!(name, "ping");
    assert_eq!(args, json!({}));
}

#[test]
fn parses_single_arg_with_spaces() {
    let reg = make_registry();
    let (name, args) = parse_call("shell[0|ls -la /tmp]", &reg).unwrap();
    assert_eq!(name, "shell");
    assert_eq!(args, json!({"command": "ls -la /tmp"}));
}

#[test]
fn handles_pipe_escape() {
    let reg = make_registry();
    let (_, args) = parse_call(r"shell[0|cat foo \| grep bar]", &reg).unwrap();
    assert_eq!(args, json!({"command": "cat foo | grep bar"}));
}

#[test]
fn handles_bracket_escape() {
    let reg = make_registry();
    let (_, args) = parse_call(r"shell[0|echo \]done\]]", &reg).unwrap();
    assert_eq!(args, json!({"command": "echo ]done]"}));
}

#[test]
fn handles_backslash_escape() {
    let reg = make_registry();
    let (_, args) = parse_call(r"shell[0|C:\\Users\\bob]", &reg).unwrap();
    assert_eq!(args, json!({"command": r"C:\Users\bob"}));
}

#[test]
fn coerces_typed_arguments() {
    let reg = make_registry();
    // No `required` list on `math`, so all three are optional and sort
    // alphabetically: verbose, x, y. The signature the model sees is
    // `math[0|<verbose>|1|<x>|2|<y>]`, so these are the slots it would emit.
    let (_, args) = parse_call("math[0|true|1|42|2|2.75]", &reg).unwrap();
    assert_eq!(args, json!({"verbose": true, "x": 42, "y": 2.75}));
}

#[test]
fn coercion_falls_back_to_string_on_failure() {
    let reg = make_registry();
    let (_, args) = parse_call("math[0|maybe|1|notanumber|2|alsonotanumber]", &reg).unwrap();
    assert_eq!(
        args,
        json!({
            "verbose": "maybe",
            "x": "notanumber",
            "y": "alsonotanumber"
        })
    );
}

#[test]
fn optional_only_signature_is_alphabetical() {
    let reg = make_registry();
    // `math` declares no `required`, so every parameter is optional and the
    // layout falls back to `BTreeMap` order: {verbose, x, y}.
    assert_eq!(
        render_signature("math", &reg["math"]),
        "math[0|<verbose>|1|<x>|2|<y>]"
    );
}

#[test]
fn rejects_unknown_tool() {
    let reg = make_registry();
    assert!(parse_call("nope[arg]", &reg).is_none());
}

#[test]
fn rejects_missing_brackets() {
    let reg = make_registry();
    assert!(parse_call("get_weather London metric", &reg).is_none());
}

#[test]
fn rejects_trailing_garbage() {
    let reg = make_registry();
    // Closing bracket isn't last char → invalid p-format, dispatcher
    // should try the JSON fallback path.
    assert!(parse_call("get_weather[0|London|1|metric] // comment", &reg).is_none());
}

#[test]
fn rejects_a_slot_the_schema_has_no_parameter_for() {
    let reg = make_registry();
    // `get_weather` has two slots, 0 and 1. Slot 2 is not a value to drop —
    // it means the model is working from a layout this schema does not have,
    // so every *other* slot in the call is suspect too.
    assert!(parse_call("get_weather[0|London|1|metric|2|extra]", &reg).is_none());
}

#[test]
fn an_empty_value_omits_the_key_rather_than_sending_a_blank() {
    let reg = make_registry();
    let (_, args) = parse_call("get_weather[0|London|1|]", &reg).unwrap();
    // `unit` is absent, not `""`. A blank string is what the old form sent,
    // and for a typed parameter it fails schema validation naming a field the
    // model deliberately left empty — an error it cannot act on.
    assert_eq!(args, json!({"location": "London"}));
}

// ── Indexed slots: the behaviour the format exists for ──────────────────

fn sparse_registry() -> PFormatRegistry {
    let mut reg = PFormatRegistry::new();
    // Mirrors the shape that produced the live failures: one required
    // parameter, several optional ones that sort ahead of it alphabetically.
    reg.insert(
        "list_threads".to_string(),
        PFormatToolParams::from_schema(&json!({
            "type": "object",
            "properties": {
                "max_results": { "type": "integer" },
                "query": { "type": "string" },
                "user_id": { "type": "string" },
                "verbose": { "type": "boolean" }
            },
            "required": ["query"]
        })),
    );
    reg
}

#[test]
fn required_parameters_are_numbered_first() {
    let reg = sparse_registry();
    // Alphabetically `query` would be slot 1, behind `max_results`. Required
    // first puts it at 0, which is what makes the one-argument call
    // `list_threads[0|…]` rather than an index the model has to look up.
    assert_eq!(
        render_signature("list_threads", &reg["list_threads"]),
        "list_threads[0|<query>|1|<max_results>|2|<user_id>|3|<verbose>]"
    );
}

#[test]
fn a_sparse_call_sends_only_the_slots_it_names() {
    let reg = sparse_registry();
    let (_, args) = parse_call("list_threads[0|from:alice|1|50]", &reg).unwrap();
    // No empty slots to count, and the absent parameters are absent rather
    // than blank.
    assert_eq!(args, json!({"query": "from:alice", "max_results": 50}));
}

#[test]
fn the_live_misbinding_is_now_a_refusal_rather_than_a_wrong_call() {
    let reg = sparse_registry();
    // The failure this format replaces: a bare-positional call whose leading
    // delimiter count was off by one bound the search text to `user_id` and
    // *ran*. In the indexed form the same body has no numeric index at slot
    // 0, so it is rejected and the model is told, instead of a wrong call
    // succeeding.
    assert!(parse_call("list_threads[|||from:alice]", &reg).is_none());
}

#[test]
fn rejects_a_bare_positional_call() {
    let reg = make_registry();
    // Deliberate: parsing this positionally is exactly the silent misbinding
    // the indices exist to end, so the old form is refused rather than
    // accepted for compatibility.
    assert!(parse_call("get_weather[London|metric]", &reg).is_none());
}

#[test]
fn rejects_an_odd_token_count() {
    let reg = make_registry();
    // A dropped or added delimiter. Keeping the pairs that happen to line up
    // would put the off-by-one straight back.
    assert!(parse_call("get_weather[0|London|1]", &reg).is_none());
}

#[test]
fn rejects_a_non_numeric_index() {
    let reg = make_registry();
    assert!(parse_call("get_weather[location|London]", &reg).is_none());
}

#[test]
fn a_repeated_slot_takes_the_last_value() {
    let reg = make_registry();
    // Rare enough not to be worth failing the whole call over, and the later
    // value is the model's latest intent.
    let (_, args) = parse_call("get_weather[0|London|0|Berlin]", &reg).unwrap();
    assert_eq!(args, json!({"location": "Berlin"}));
}

#[test]
fn an_empty_repeated_slot_retracts_the_earlier_value() {
    let reg = make_registry();
    // Both documented rules meet here: a repeated slot takes its last value,
    // and an empty value means "not sent". So the second `0|` retracts the
    // first rather than being skipped — otherwise the last-write rule holds
    // everywhere except the one case where the last write is a retraction.
    let (_, args) = parse_call("get_weather[0|London|0|]", &reg).unwrap();
    assert_eq!(args, json!({}));

    // And it retracts only its own slot.
    let (_, args) = parse_call("get_weather[0|London|1|metric|0|]", &reg).unwrap();
    assert_eq!(args, json!({"unit": "metric"}));
}

#[test]
fn an_empty_body_is_a_call_with_no_arguments() {
    let reg = make_registry();
    // Distinct from `ping[]`: `get_weather` *has* slots, and sending none of
    // them is a valid (if incomplete) call the schema validator should judge,
    // not a parse failure.
    let (name, args) = parse_call("get_weather[]", &reg).unwrap();
    assert_eq!(name, "get_weather");
    assert_eq!(args, json!({}));
}

#[test]
fn an_escaped_pipe_stays_inside_its_value() {
    let reg = make_registry();
    // The index/value split runs over `split_pipes`, which honours escapes —
    // so an escaped pipe is part of one value and does not shift the pairing.
    let (_, args) = parse_call(r"shell[0|cat a \| grep b]", &reg).unwrap();
    assert_eq!(args, json!({"command": "cat a | grep b"}));
}

#[test]
fn signature_round_trips_with_parser() {
    let reg = make_registry();
    let sig = render_signature("get_weather", &reg["get_weather"]);
    // Render uses the same identifier the parser expects.
    assert!(sig.starts_with("get_weather["));
    let synthesised = "get_weather[0|Berlin|1|imperial]";
    let (name, args) = parse_call(synthesised, &reg).unwrap();
    assert_eq!(name, "get_weather");
    assert_eq!(args["location"], json!("Berlin"));
    assert_eq!(args["unit"], json!("imperial"));
}
