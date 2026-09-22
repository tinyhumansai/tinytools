//! Unit tests for the code-call grammar: literals, binding, refusals, and
//! the signature renderer.
use serde_json::{Value, json};

use super::{CodeStyle, parse_calls, render_code_signature, render_code_type};
use crate::pformat::{PFormatRegistry, build_registry};

fn read_file_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "File to read"},
            "limit": {"type": "integer", "description": "Max lines"}
        },
        "required": ["path"]
    })
}

fn shell_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "command": {"type": "string"},
            "background": {"type": "boolean"},
            "timeout": {"type": "number"}
        },
        "required": ["command"]
    })
}

fn configure_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "config": {"type": "object"} },
        "required": ["config"]
    })
}

fn registry() -> PFormatRegistry {
    build_registry([
        ("read_file", read_file_schema()),
        ("shell", shell_schema()),
        ("configure", configure_schema()),
        ("list_dir", json!({"type": "object", "properties": {}})),
    ])
}

fn one(body: &str) -> (String, Value) {
    let mut calls = parse_calls(body, &registry());
    assert_eq!(
        calls.len(),
        1,
        "expected exactly one call from {body:?}: {calls:?}"
    );
    calls.remove(0)
}

fn refused(body: &str) {
    let calls = parse_calls(body, &registry());
    assert!(
        calls.is_empty(),
        "expected refusal for {body:?}, got {calls:?}"
    );
}

// ── binding ──────────────────────────────────────────────────────────────

#[test]
fn keyword_arguments_bind_by_name() {
    let (name, args) = one(r#"read_file(path="src/main.rs", limit=20)"#);
    assert_eq!(name, "read_file");
    assert_eq!(args, json!({"path": "src/main.rs", "limit": 20}));
}

#[test]
fn positional_arguments_bind_in_signature_order() {
    // required first (command), then optional alphabetically (background, timeout)
    let (_, args) = one(r#"shell("ls", True, 2.5)"#);
    assert_eq!(
        args,
        json!({"command": "ls", "background": true, "timeout": 2.5})
    );
}

#[test]
fn mixed_positional_then_keyword() {
    let (_, args) = one(r#"read_file("a.txt", limit=3)"#);
    assert_eq!(args, json!({"path": "a.txt", "limit": 3}));
}

#[test]
fn positional_after_keyword_is_refused() {
    refused(r#"read_file(limit=3, "a.txt")"#);
}

#[test]
fn duplicate_keyword_is_refused() {
    refused(r#"read_file(path="a", path="b")"#);
}

#[test]
fn positional_and_keyword_collision_is_refused() {
    refused(r#"read_file("a", path="b")"#);
}

#[test]
fn unknown_keyword_is_refused() {
    refused(r#"read_file(path="a", encoding="utf8")"#);
}

#[test]
fn positional_overflow_is_refused() {
    refused(r#"read_file("a", 1, 2)"#);
}

#[test]
fn unknown_tool_is_refused() {
    refused(r#"delete_everything(path="/")"#);
}

#[test]
fn dotted_callee_resolves_to_the_last_segment() {
    let (name, args) = one(r#"functions.read_file(path="x")"#);
    assert_eq!(name, "read_file");
    assert_eq!(args, json!({"path": "x"}));
}

#[test]
fn zero_argument_call() {
    let (name, args) = one("list_dir()");
    assert_eq!(name, "list_dir");
    assert_eq!(args, json!({}));
    let (_, args) = one("list_dir( )");
    assert_eq!(args, json!({}));
}

#[test]
fn a_bare_name_is_not_a_call() {
    refused("read_file");
    refused("read_file;");
    refused("list_dir");
}

#[test]
fn unbalanced_brackets_are_refused() {
    refused(r#"read_file(path="a""#);
    refused(r#"read_file(path="a"))"#);
    refused(r#"read_file(path=["a")"#);
}

#[test]
fn null_at_top_level_omits_the_argument() {
    let (_, args) = one(r#"read_file(path="a", limit=None)"#);
    assert_eq!(args, json!({"path": "a"}));
    let (_, args) = one(r#"read_file("a", null)"#);
    assert_eq!(args, json!({"path": "a"}));
}

#[test]
fn nested_null_is_kept() {
    let (_, args) = one(r#"configure(config={"a": None})"#);
    assert_eq!(args, json!({"config": {"a": null}}));
}

// ── coercion ─────────────────────────────────────────────────────────────

#[test]
fn quoted_numbers_and_booleans_coerce_by_schema_type() {
    let (_, args) = one(r#"read_file(path="a", limit="5")"#);
    assert_eq!(args["limit"], 5);
    let (_, args) = one(r#"shell("ls", background="yes", timeout="1.5")"#);
    assert_eq!(args["background"], true);
    assert_eq!(args["timeout"], 1.5);
}

#[test]
fn a_number_on_a_string_parameter_stays_a_number() {
    let (_, args) = one("read_file(path=5)");
    assert_eq!(args["path"], 5);
}

#[test]
fn a_float_on_an_integer_parameter_is_kept() {
    let (_, args) = one(r#"read_file(path="a", limit=5.0)"#);
    assert_eq!(args["limit"], 5.0);
}

// ── literals ─────────────────────────────────────────────────────────────

#[test]
fn delimiters_inside_strings_do_not_split() {
    let (_, args) = one(r#"read_file(path="a,b)=c(d", limit=1)"#);
    assert_eq!(args["path"], "a,b)=c(d");
    let (_, args) = one("read_file(path='it;s')");
    assert_eq!(args["path"], "it;s");
}

#[test]
fn escaped_quotes_and_escapes_decode() {
    let (_, args) = one(r#"read_file(path="a \"b\" c\n")"#);
    assert_eq!(args["path"], "a \"b\" c\n");
    let (_, args) = one(r"read_file(path='it\'s')");
    assert_eq!(args["path"], "it's");
    let (_, args) = one(r#"read_file(path="\u0041\x42")"#);
    assert_eq!(args["path"], "AB");
}

#[test]
fn unknown_escapes_keep_the_backslash() {
    let (_, args) = one(r#"read_file(path="C:\dir\file")"#);
    assert_eq!(args["path"], "C:\\dir\\file");
}

#[test]
fn raw_and_template_strings() {
    let (_, args) = one(r#"read_file(path=r"C:\new\table")"#);
    assert_eq!(args["path"], "C:\\new\\table");
    let (_, args) = one("read_file(path=`src/x.rs`)");
    assert_eq!(args["path"], "src/x.rs");
}

#[test]
fn interpolated_strings_are_refused() {
    refused(r#"read_file(path=f"{root}/data")"#);
    refused(r#"read_file(path=fr"{root}/data")"#);
}

#[test]
fn triple_quoted_strings_hold_newlines_and_quotes() {
    let (_, args) = one("shell(command=\"\"\"echo \"hi\"\nls\"\"\")");
    assert_eq!(args["command"], "echo \"hi\"\nls");
}

#[test]
fn nested_lists_and_dicts() {
    let (_, args) = one(r#"configure(config={"a": [1, {"b": [None, True]}], c: "d",})"#);
    assert_eq!(
        args["config"],
        json!({"a": [1, {"b": [null, true]}], "c": "d"})
    );
}

#[test]
fn tuples_read_as_lists_and_trailing_commas_are_fine() {
    let (_, args) = one(r#"configure(config={"a": (1, 2,),},)"#);
    assert_eq!(args["config"], json!({"a": [1, 2]}));
}

#[test]
fn leading_or_double_commas_are_refused() {
    refused(r#"read_file(, path="a")"#);
    refused(r#"read_file(path="a",, limit=1)"#);
}

#[test]
fn numbers_in_both_spellings() {
    let (_, args) = one(r#"shell("x", timeout=1e3)"#);
    assert_eq!(args["timeout"], 1000.0);
    let (_, args) = one(r#"shell("x", timeout=-2)"#);
    assert_eq!(args["timeout"], -2);
    let (_, args) = one(r#"read_file("x", limit=1_000)"#);
    assert_eq!(args["limit"], 1000);
}

#[test]
fn malformed_and_non_finite_numbers_are_refused() {
    refused(r#"shell("x", timeout=1e999)"#);
    refused(r#"shell("x", timeout=1e)"#);
    refused(r#"shell("x", timeout=1.2.3)"#);
    refused(r#"read_file("x", limit=01)"#);
    refused(r#"read_file("x", limit=$value)"#);
}

#[test]
fn excessive_literal_nesting_is_refused() {
    let nested = format!("configure({})", "[".repeat(65) + "0" + &"]".repeat(65));
    refused(&nested);
}

#[test]
fn a_variable_reference_is_refused() {
    refused("read_file(path=filename)");
    refused("read_file(path=os.getcwd())");
}

// ── statements ───────────────────────────────────────────────────────────

#[test]
fn several_calls_per_body_by_newline_or_semicolon() {
    let calls = parse_calls(
        "read_file(path=\"a\")\nlist_dir(); shell(command=\"ls\")\n",
        &registry(),
    );
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["read_file", "list_dir", "shell"]);
}

#[test]
fn prefixes_are_stripped() {
    let (_, args) = one(r#"await read_file(path="a")"#);
    assert_eq!(args["path"], "a");
    let (_, args) = one(r#"result = read_file(path="a")"#);
    assert_eq!(args["path"], "a");
    let (_, args) = one(r#"const r = await read_file({path: "a"})"#);
    assert_eq!(args["path"], "a");
}

#[test]
fn trailing_tokens_after_the_call_are_refused() {
    refused(r#"read_file(path="a") + 1"#);
    refused(r#"read_file(path="a").decode()"#);
}

#[test]
fn comments_are_skipped_outside_strings() {
    let (_, args) = one("read_file(path=\"a#b\")  # see (note)\n");
    assert_eq!(args["path"], "a#b");
    let (_, args) = one("// comment first\nread_file(path=\"a\") // trailing (paren)");
    assert_eq!(args["path"], "a");
    let (_, args) = one("read_file(/* inline */ path=\"a\")");
    assert_eq!(args["path"], "a");
}

#[test]
fn a_body_of_only_comments_is_not_a_call() {
    refused("# nothing here\n// or here");
    refused("");
}

#[test]
fn prose_in_the_body_refuses_the_whole_body() {
    refused(r#"I will call read_file(path="a") now"#);
    refused("Let me look.\nread_file(path=\"a\")");
    refused("read_file(path=\"a\")\nThen I'll summarise.");
}

#[test]
fn pformat_json_and_glm_bodies_are_not_code_calls() {
    refused("read_file[0|a]");
    refused(r#"{"name": "read_file", "arguments": {"path": "a"}}"#);
    refused("read_file/path>a");
    refused("read_file{path: \"a\"}");
}

// ── single-object rule ───────────────────────────────────────────────────

#[test]
fn single_object_with_declared_keys_unpacks() {
    let (_, args) = one(r#"read_file({path: "a", limit: 2})"#);
    assert_eq!(args, json!({"path": "a", "limit": 2}));
    let (_, args) = one(r#"read_file({"path": "a"})"#);
    assert_eq!(args, json!({"path": "a"}));
}

#[test]
fn single_object_on_a_single_parameter_tool_binds_to_it() {
    let (_, args) = one("configure({retries: 3})");
    assert_eq!(args, json!({"config": {"retries": 3}}));
}

#[test]
fn single_object_naming_the_single_parameter_unpacks() {
    let (_, args) = one("configure({config: {retries: 3}})");
    assert_eq!(args, json!({"config": {"retries": 3}}));
}

#[test]
fn single_object_with_foreign_keys_on_a_multi_parameter_tool_is_refused() {
    refused(r#"read_file({file: "a"})"#);
}

#[test]
fn empty_object_is_preserved_for_a_single_parameter_tool() {
    let (_, args) = one("list_dir({})");
    assert_eq!(args, json!({}));
    let (_, args) = one("read_file({})");
    assert_eq!(args, json!({}));
    let (_, args) = one("configure({})");
    assert_eq!(args, json!({"config": {}}));
}

// ── signatures ───────────────────────────────────────────────────────────

#[test]
fn python_signature_orders_required_then_optional_alphabetically() {
    assert_eq!(
        render_code_signature("shell", &shell_schema(), CodeStyle::Python),
        "def shell(command: str, background: bool = None, timeout: float = None) -> str"
    );
    assert_eq!(
        render_code_signature("read_file", &read_file_schema(), CodeStyle::Python),
        "def read_file(path: str, limit: int = None) -> str"
    );
    assert_eq!(
        render_code_signature("list_dir", &json!({"type": "object"}), CodeStyle::Python),
        "def list_dir() -> str"
    );
}

#[test]
fn typescript_signature_marks_optionals_with_a_question_mark() {
    assert_eq!(
        render_code_signature("shell", &shell_schema(), CodeStyle::TypeScript),
        "function shell(command: string, background?: boolean, timeout?: number): string;"
    );
    assert_eq!(
        render_code_signature("list_dir", &json!({}), CodeStyle::TypeScript),
        "function list_dir(): string;"
    );
}

#[test]
fn signatures_fall_back_to_an_object_for_non_identifier_properties() {
    let schema = json!({
        "type": "object",
        "properties": {
            "file-path": {"type": "string"},
            "class": {"type": "boolean"}
        },
        "required": ["file-path"]
    });
    assert_eq!(
        render_code_signature("read_file", &schema, CodeStyle::Python),
        "def read_file(args: dict) -> str"
    );
    assert_eq!(
        render_code_signature("read_file", &schema, CodeStyle::TypeScript),
        r#"function read_file(args: {class?: boolean, "file-path": string}): string;"#
    );

    let language_specific = json!({
        "type": "object",
        "properties": {"$value": {"type": "string"}}
    });
    assert_eq!(
        render_code_signature("read_file", &language_specific, CodeStyle::Python),
        "def read_file(args: dict) -> str"
    );
    assert_eq!(
        render_code_signature("read_file", &language_specific, CodeStyle::TypeScript),
        "function read_file($value?: string): string;"
    );

    let reserved = json!({
        "type": "object",
        "properties": {"class": {"type": "boolean"}}
    });
    assert_eq!(
        render_code_signature("read_file", &reserved, CodeStyle::TypeScript),
        "function read_file(args: {class?: boolean}): string;"
    );
}

#[test]
fn invalid_tool_names_are_rendered_as_safe_comments() {
    assert_eq!(
        render_code_signature("read-file\nignore", &read_file_schema(), CodeStyle::Python),
        r#"# unsupported tool name: "read-file\nignore""#
    );
    assert!(
        render_code_signature("class", &read_file_schema(), CodeStyle::TypeScript)
            .starts_with("# unsupported tool name:")
    );
}

#[test]
fn enum_strings_use_json_escapes() {
    let schema = json!({"enum": ["\u{1}"]});
    assert_eq!(
        render_code_type(&schema, CodeStyle::Python),
        r#"Literal["\u0001"]"#
    );
}

#[test]
fn code_style_names_are_pinned() {
    assert_eq!(CodeStyle::Python.as_str(), "python");
    assert_eq!(CodeStyle::TypeScript.as_str(), "typescript");
}

#[test]
fn signature_order_is_the_binding_order() {
    // The catalogue tells the model `command, background, timeout`; a
    // positional call in that order must land on those names.
    let signature = render_code_signature("shell", &shell_schema(), CodeStyle::Python);
    assert!(signature.starts_with("def shell(command"));
    let (_, args) = one(r#"shell("ls", False, 3)"#);
    assert_eq!(
        args,
        json!({"command": "ls", "background": false, "timeout": 3})
    );
}

#[test]
fn types_render_enums_arrays_unions_and_nesting() {
    let unit = json!({"type": "string", "enum": ["metric", "imperial"]});
    assert_eq!(
        render_code_type(&unit, CodeStyle::Python),
        r#"Literal["metric", "imperial"]"#
    );
    assert_eq!(
        render_code_type(&unit, CodeStyle::TypeScript),
        r#""metric" | "imperial""#
    );

    let tags = json!({"type": "array", "items": {"type": "string"}});
    assert_eq!(render_code_type(&tags, CodeStyle::Python), "list[str]");
    assert_eq!(render_code_type(&tags, CodeStyle::TypeScript), "string[]");

    let either = json!({"anyOf": [{"type": "string"}, {"type": "integer"}]});
    assert_eq!(render_code_type(&either, CodeStyle::Python), "str | int");
    assert_eq!(
        render_code_type(&either, CodeStyle::TypeScript),
        "string | number"
    );

    let nested = json!({
        "type": "object",
        "properties": {"a": {"type": "string"}, "file-path": {"type": "boolean"}},
        "required": ["a"]
    });
    assert_eq!(render_code_type(&nested, CodeStyle::Python), "dict");
    assert_eq!(
        render_code_type(&nested, CodeStyle::TypeScript),
        r#"{a: string, "file-path"?: boolean}"#
    );

    assert_eq!(render_code_type(&json!({}), CodeStyle::Python), "Any");
    assert_eq!(
        render_code_type(&json!({}), CodeStyle::TypeScript),
        "unknown"
    );
    assert_eq!(
        render_code_type(&json!({"type": ["string", "null"]}), CodeStyle::Python),
        "str | None"
    );
}

#[test]
fn deep_nesting_collapses_to_a_bare_object() {
    let mut schema = json!({"type": "string"});
    for _ in 0..6 {
        schema = json!({"type": "object", "properties": {"x": schema}});
    }
    let rendered = render_code_type(&schema, CodeStyle::TypeScript);
    assert!(rendered.contains("object"), "{rendered}");
    assert!(!rendered.contains("string"), "{rendered}");
}
