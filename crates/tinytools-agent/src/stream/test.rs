//! Unit tests for the stream scrubber.
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::StreamScrubber;
use crate::parse::parse_tool_calls;

/// Feeds every fragment and appends the flush, returning the visible text and
/// every call released along the way.
fn scrub_all(fragments: &[&str]) -> (String, usize) {
    let mut s = StreamScrubber::new();
    let mut out = String::new();
    let mut calls = 0;
    for f in fragments {
        let step = s.feed(f);
        out.push_str(&step.text);
        calls += step.calls.len();
    }
    let step = s.flush();
    out.push_str(&step.text);
    calls += step.calls.len();
    (out, calls)
}

#[test]
fn plain_text_passes_through_unchanged() {
    assert_eq!(scrub_all(&["hello ", "world", " done"]).0, "hello world done");
}

#[test]
fn a_complete_block_in_one_fragment_is_dropped_and_its_call_released() {
    let (out, calls) = scrub_all(&[r#"before <tool_call>{"name":"x","arguments":{}}</tool_call> after"#]);
    assert_eq!(out, "before  after");
    assert_eq!(calls, 1);
}

#[test]
fn markup_split_across_fragments_never_leaks() {
    let (out, calls) = scrub_all(&[
        "answer: ",
        "<tool_",
        "call>{\"name\":",
        "\"x\",\"arguments\":{\"a\":1}}",
        "</tool_",
        "call> end",
    ]);
    assert_eq!(out, "answer:  end");
    assert_eq!(calls, 1);
}

#[test]
fn a_partial_open_marker_is_held_not_emitted() {
    let mut s = StreamScrubber::new();
    let first = s.feed("value <tool");
    assert_eq!(first.text, "value ", "partial `<tool` must be held");
    let second = s.feed("_call>{\"name\":\"x\",\"arguments\":{}}</tool_call>!");
    assert_eq!(second.text, "!");
    assert_eq!(second.calls.len(), 1);
    assert_eq!(s.flush().text, "");
}

#[test]
fn an_attribute_open_form_split_mid_tag_is_held() {
    let mut s = StreamScrubber::new();
    let mut out = String::new();
    out.push_str(&s.feed("ok <tool_call id=\"c").text);
    out.push_str(&s.feed("all_0\">{\"name\":\"x\",\"arguments\":{}}</tool_call>").text);
    out.push_str(&s.flush().text);
    assert_eq!(out, "ok ");
}

#[test]
fn deepseek_delimiters_split_across_fragments_are_scrubbed() {
    let (out, calls) = scrub_all(&[
        "r <｜tool▁ca",
        "ll▁begin｜>{\"name\":\"a\",\"arguments\":{}}<｜tool▁call",
        "▁end｜> s",
    ]);
    assert_eq!(out, "r  s");
    assert!(!out.contains("tool▁call"));
    assert_eq!(calls, 1);
}

#[test]
fn dsml_split_across_fragments_is_scrubbed() {
    let (out, calls) = scrub_all(&[
        "Sure. <｜DS",
        "ML｜tool_calls><｜DSML｜invoke name=\"read\">{\"path\":\"a\"}",
        "</｜DSML｜invoke></｜DSML｜tool_calls> Done.",
    ]);
    assert_eq!(out.trim(), "Sure.  Done.".trim());
    assert!(!out.contains("DSML"), "{out}");
    assert_eq!(calls, 1);
}

#[test]
fn plural_tool_calls_prose_is_not_held() {
    assert_eq!(
        scrub_all(&["the <tool_calls> ", "key"]).0,
        "the <tool_calls> key"
    );
}

#[test]
fn flush_surfaces_a_dangling_open_verbatim_untrimmed() {
    let mut s = StreamScrubber::new();
    let mid = s.feed("  a <tool_call ");
    assert_eq!(mid.text, "  a ", "the in-progress open tag is held");
    assert_eq!(s.flush().text, "<tool_call ");
}

#[test]
fn a_harmony_call_is_held_until_its_terminator() {
    let mut s = StreamScrubber::new();
    let first = s.feed("<|channel|>commentary to=functions.read<|message|>{\"path\":");
    assert_eq!(first.text, "");
    let second = s.feed("\"a\"}<|call|>tail");
    assert_eq!(second.text, "tail");
    assert_eq!(second.calls[0].name, "read");
}

#[test]
fn stream_matches_batch_parser_on_the_visible_text() {
    let full = r#"lead <tool_call>{"name":"a","arguments":{}}</tool_call> mid <tool_call>{"name":"b","arguments":{"k":1}}</tool_call> tail"#;
    let (batch, calls) = parse_tool_calls(full);
    assert_eq!(calls.len(), 2);
    let frags: Vec<String> = full.chars().map(|c| c.to_string()).collect();
    let refs: Vec<&str> = frags.iter().map(String::as_str).collect();
    let (streamed, stream_calls) = scrub_all(&refs);
    assert_eq!(streamed.split_whitespace().collect::<Vec<_>>(), batch.split_whitespace().collect::<Vec<_>>());
    assert_eq!(stream_calls, 2);
}

#[test]
fn known_tools_repair_streamed_names() {
    let mut s = StreamScrubber::new().with_known_tools(vec!["read_file".into()]);
    let step = s.feed("<tool_call>{\"name\":\"functions.read_file\",\"arguments\":{}}</tool_call>");
    assert_eq!(step.calls[0].name, "read_file");
}

#[test]
fn dbg_char_stream() {
    let full = r#"lead <tool_call>{"name":"a","arguments":{}}</tool_call> mid"#;
    let mut s = StreamScrubber::new();
    let mut log = String::new();
    for c in full.chars() {
        let step = s.feed(&c.to_string());
        if !step.calls.is_empty() || !step.text.is_empty() {
            log.push_str(&format!("{c:?} -> text={:?} calls={} buf={:?}\n", step.text, step.calls.len(), s.buf));
        }
    }
    panic!("{log}");
}
