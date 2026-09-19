//! DeepSeek-R1 / V3 and Kimi K2 sentinel tokens.

use super::{parse, parse_known};
use crate::types::{CallSource, ParseDiagnostic};

#[test]
fn deepseek_r1_function_sep_layout_parses() {
    let response = "<｜tool▁calls▁begin｜><｜tool▁call▁begin｜>function<｜tool▁sep｜>get_weather\n```json\n{\"location\": \"Tokyo\"}\n```<｜tool▁call▁end｜><｜tool▁calls▁end｜>";
    let (text, calls) = parse(response);
    assert!(text.is_empty(), "{text:?}");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "get_weather");
    assert_eq!(calls[0].arguments["location"], "Tokyo");
    assert_eq!(calls[0].source, CallSource::Sentinel);
}

#[test]
fn deepseek_v3_name_sep_layout_parses() {
    let (_, calls) = parse(
        "<｜tool▁call▁begin｜>get_weather<｜tool▁sep｜>{\"location\":\"Tokyo\"}<｜tool▁call▁end｜>",
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "get_weather");
}

#[test]
fn deepseek_delimiters_around_a_json_call_object_parse() {
    let text = "prose <｜tool▁call▁begin｜>{\"name\":\"a\",\"arguments\":{\"k\":1}}<｜tool▁call▁end｜> more";
    let (cleaned, calls) = parse(text);
    assert_eq!(cleaned, "prose\nmore");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["k"], 1);
}

#[test]
fn kimi_k2_section_layout_parses() {
    let response = "<|tool_calls_section_begin|><|tool_call_begin|>functions.get_weather:0<|tool_call_argument_begin|>{\"city\": \"Tokyo\"}<|tool_call_end|><|tool_calls_section_end|>";
    let (text, calls) = parse(response);
    assert!(text.is_empty(), "{text:?}");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "get_weather");
    assert_eq!(calls[0].arguments["city"], "Tokyo");
}

#[test]
fn kimi_k2_parallel_calls_parse_in_order() {
    let response = concat!(
        "<|tool_calls_section_begin|>",
        "<|tool_call_begin|>functions.a:0<|tool_call_argument_begin|>{\"x\":1}<|tool_call_end|>",
        "<|tool_call_begin|>functions.b:1<|tool_call_argument_begin|>{\"y\":2}<|tool_call_end|>",
        "<|tool_calls_section_end|>"
    );
    let (_, calls) = parse(response);
    let names: Vec<&str> = calls.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b"]);
}

#[test]
fn unterminated_sentinel_block_is_kept_as_text() {
    let text = "start <｜tool▁call▁begin｜>{\"name\":\"a\"";
    let (cleaned, calls) = parse(text);
    assert!(calls.is_empty());
    assert_eq!(cleaned, text);
}
