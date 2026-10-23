//! Targeted edits preserve neighboring source and reject stale or unsafe inputs.

use tinytools_code::{
    CodeEngine, CodeError, CodeLimits, Language, PatchOperation, PatchRequest, PatchResult,
    revision,
};

fn request(
    source: &str,
    operation: PatchOperation,
    selector: Option<&str>,
    code: &str,
) -> PatchRequest {
    PatchRequest {
        expected_revision: revision(source),
        operation,
        selector: selector.map(str::to_owned),
        code: code.to_owned(),
    }
}

#[test]
fn body_edits_are_revision_checked_and_preserve_siblings() -> Result<(), CodeError> {
    let source = "// license\nfn first() { old(); }\n// untouched\nfn second() {}\n";
    let mut engine = CodeEngine::default();
    let edit = request(source, PatchOperation::ReplaceBody, Some("first"), "new();");
    let result = engine.patch("file", Language::Rust, source, &edit)?;
    assert!(result.changed);
    assert!(result.source.starts_with("// license\nfn first() {"));
    assert!(result.source.ends_with("\n// untouched\nfn second() {}\n"));
    assert!(result.source.contains("new();") && !result.source.contains("old();"));
    assert_eq!(result.after_revision, revision(&result.source));
    assert!(matches!(
        engine.patch("file", Language::Rust, &result.source, &edit),
        Err(CodeError::StaleRevision { .. })
    ));
    Ok(())
}

#[test]
fn replacement_and_one_pass_wrapping_touch_only_the_selected_range() -> Result<(), CodeError> {
    let source = "fn old() {}\nfn keep() {}";
    let mut engine = CodeEngine::default();
    let result = engine.patch(
        "file",
        Language::Rust,
        source,
        &request(
            source,
            PatchOperation::ReplaceSymbol,
            Some("old"),
            "pub fn newer() {}",
        ),
    )?;
    assert_eq!(result.source, "pub fn newer() {}\nfn keep() {}");
    let wrapped = engine.patch(
        "file",
        Language::Rust,
        source,
        &request(
            source,
            PatchOperation::Wrap,
            Some("old"),
            "#[allow(dead_code)]\n{{node}}",
        ),
    )?;
    assert_eq!(
        wrapped.source,
        "#[allow(dead_code)]\nfn old() {}\nfn keep() {}"
    );
    for code in ["no marker", "{{node}} {{node}}"] {
        assert!(
            engine
                .patch(
                    "file",
                    Language::Rust,
                    source,
                    &request(source, PatchOperation::Wrap, Some("old"), code)
                )
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn invalid_ambiguous_or_missing_targets_are_refused() {
    let mut engine = CodeEngine::default();
    for (source, selector, code) in [
        ("fn old() {}", Some("old"), "fn {"),
        ("fn old() {}", Some("missing"), "fn x() {}"),
        ("fn old() {}", None, "fn x() {}"),
        ("fn old( {", Some("old"), "fn x() {}"),
        (
            "mod a { fn old() {} } mod b { fn old() {} }",
            Some("old"),
            "fn x() {}",
        ),
    ] {
        assert!(
            engine
                .patch(
                    "file",
                    Language::Rust,
                    source,
                    &request(source, PatchOperation::ReplaceSymbol, selector, code)
                )
                .is_err()
        );
    }
    let source = "const VALUE: i32 = 1;";
    assert!(matches!(
        engine.patch(
            "file",
            Language::Rust,
            source,
            &request(source, PatchOperation::ReplaceBody, Some("VALUE"), "2")
        ),
        Err(CodeError::UnsupportedOperation(_))
    ));
    let source = "const value = () => 1;";
    assert!(matches!(
        engine.patch(
            "file",
            Language::JavaScript,
            source,
            &request(source, PatchOperation::ReplaceBody, Some("value"), "2")
        ),
        Err(CodeError::UnsupportedOperation(_))
    ));
}

#[test]
fn truncated_outline_requires_exact_ids_for_mutations() -> Result<(), CodeError> {
    let source = "fn same() {} mod nested { fn same() {} }";
    let mut engine = CodeEngine::new(CodeLimits {
        max_records: 1,
        ..CodeLimits::default()
    })?;
    assert!(matches!(
        engine.patch(
            "file",
            Language::Rust,
            source,
            &request(source, PatchOperation::ReplaceBody, Some("same"), "call();")
        ),
        Err(CodeError::AmbiguousSymbol(_))
    ));
    let report = engine.analyze("file", Language::Rust, source)?;
    let result = engine.patch(
        "file",
        Language::Rust,
        source,
        &request(
            source,
            PatchOperation::ReplaceBody,
            Some(&report.symbols[0].id),
            "call();",
        ),
    )?;
    assert!(result.source.ends_with("mod nested { fn same() {} }"));
    Ok(())
}

#[test]
fn imports_preserve_prologues_and_attached_documentation() -> Result<(), CodeError> {
    let mut engine = CodeEngine::default();
    for (language, source, import, preserved) in [
        (
            Language::Rust,
            "//! Module docs\n/// Operation docs\npub fn operation() {}",
            "use std::fmt;",
            "/// Operation docs\npub fn operation() {}",
        ),
        (
            Language::Rust,
            "#![no_std]\n#[cfg(test)]\npub fn operation() {}",
            "use core::fmt;",
            "#[cfg(test)]\npub fn operation() {}",
        ),
        (
            Language::TypeScript,
            "'use strict';\n/** Operation docs */\nfunction operation() {}",
            "import x from 'x';",
            "/** Operation docs */\nfunction operation() {}",
        ),
        (
            Language::JavaScript,
            "#!/usr/bin/env node\n'use strict';\nfunction run() {}",
            "import x from 'x';",
            "function run() {}",
        ),
        (
            Language::Go,
            "package main\nfunc run() {}",
            "import \"fmt\"",
            "func run() {}",
        ),
    ] {
        let result = engine.patch(
            "file",
            language,
            source,
            &request(source, PatchOperation::InsertImport, None, import),
        )?;
        assert!(result.source.contains(preserved));
        assert!(result.source.find(import) < result.source.find(preserved));
        if source.starts_with("#![no_std]") {
            assert!(result.source.starts_with("#![no_std]\nuse core::fmt;"));
        }
        let duplicate = engine.patch(
            "file",
            language,
            &result.source,
            &request(&result.source, PatchOperation::InsertImport, None, import),
        )?;
        assert!(!duplicate.changed);
    }
    Ok(())
}

#[test]
fn python_future_imports_follow_only_the_first_real_docstring() -> Result<(), CodeError> {
    let mut engine = CodeEngine::default();
    for literal in ["\"doc\"", "r\"doc\"", "\"\"\"doc\"\"\""] {
        let source = format!("#!/usr/bin/python\n{literal}\n\"extra\"\nimport os\nvalue = 1\n");
        let result = engine.patch(
            "file",
            Language::Python,
            &source,
            &request(
                &source,
                PatchOperation::InsertImport,
                None,
                "from  __future__ import annotations",
            ),
        )?;
        assert!(result.source.find(literal) < result.source.find("from  __future__"));
        assert!(result.source.find("from  __future__") < result.source.find("\"extra\""));
    }
    for source in [
        "b\"bytes\"\nvalue = 1\n",
        "f\"string\"\nvalue = 1\n",
        "from __future__ import annotations\nimport os\nvalue = 1\n",
    ] {
        let result = engine.patch(
            "file",
            Language::Python,
            source,
            &request(
                source,
                PatchOperation::InsertImport,
                None,
                "from __future__ import division",
            ),
        )?;
        assert!(
            result.source.find("from __future__ import division") < result.source.find("value")
        );
        if source.starts_with(['b', 'f']) && !source.starts_with("from") {
            assert!(result.source.starts_with("from __future__ import division"));
        }
    }
    Ok(())
}

#[test]
fn imports_refuse_non_imports_and_data_grammars() {
    let mut engine = CodeEngine::default();
    for code in ["fn run() {}", "use a; use b;", "use ;"] {
        let source = "fn old() {}";
        assert!(
            engine
                .patch(
                    "file",
                    Language::Rust,
                    source,
                    &request(source, PatchOperation::InsertImport, None, code)
                )
                .is_err()
        );
    }
    for (language, source) in [(Language::Json, "{}"), (Language::Toml, "value=1")] {
        assert!(matches!(
            engine.patch(
                "file",
                language,
                source,
                &request(source, PatchOperation::InsertImport, None, "import x")
            ),
            Err(CodeError::UnsupportedOperation(_))
        ));
    }
}

#[test]
fn member_edits_handle_fields_methods_and_container_syntax() -> Result<(), CodeError> {
    let mut engine = CodeEngine::default();
    for (language, source, name, member) in [
        (
            Language::Rust,
            "struct Thing { pub(crate) first: i32 // attached\n}",
            "Thing",
            "pub second: i32",
        ),
        (
            Language::Rust,
            "enum Kind { First // attached\n}",
            "Kind",
            "Second",
        ),
        (
            Language::Rust,
            "impl Thing { fn first(&self) {} }",
            "Thing",
            "fn second(&self) {}",
        ),
        (
            Language::TypeScript,
            "type Thing = { first: string; };",
            "Thing",
            "second: number;",
        ),
        (
            Language::TypeScript,
            "class Thing {}",
            "Thing",
            "second(): void {}",
        ),
        (
            Language::Python,
            "class Thing: pass\n",
            "Thing",
            "def second(self):\n    return 2",
        ),
        (
            Language::Python,
            "class Thing:\n    first = 1\n",
            "Thing",
            "second = 2",
        ),
        (
            Language::Go,
            "package main\ntype Thing struct { First int }\n",
            "Thing",
            "Second string",
        ),
    ] {
        let result = engine.patch(
            "file",
            language,
            source,
            &request(source, PatchOperation::AppendMember, Some(name), member),
        )?;
        assert!(result.source.contains("second") || result.source.contains("Second"));
        if source.contains("// attached") {
            assert!(result.source.contains(", // attached"));
        }
    }
    Ok(())
}

#[test]
fn python_body_replacement_supports_tabs_inline_suites_and_empty_bodies() -> Result<(), CodeError> {
    let mut engine = CodeEngine::default();
    for source in [
        "def run(): return 1\n",
        "def run():\n\treturn 1\n",
        "def run():\n    return 1\n",
    ] {
        for code in ["value = 2\nreturn value", ""] {
            let result = engine.patch(
                "file",
                Language::Python,
                source,
                &request(source, PatchOperation::ReplaceBody, Some("run"), code),
            )?;
            assert!(!result.source.contains("return 1"));
        }
    }
    Ok(())
}

#[test]
fn comma_terminated_members_preserve_nested_closing_indentation() -> Result<(), CodeError> {
    let source = "mod container {\n    struct Thing {\n        first: i32 // attached\n    }\n}\n";
    let result = CodeEngine::default().patch(
        "file",
        Language::Rust,
        source,
        &request(
            source,
            PatchOperation::AppendMember,
            Some("Thing"),
            "pub second: i32,",
        ),
    )?;
    assert!(result.source.contains("first: i32, // attached"));
    assert!(result.source.contains("        pub second: i32,\n    }\n}"));
    Ok(())
}

#[test]
fn member_insertion_preserves_an_unindented_existing_closing_line() -> Result<(), CodeError> {
    let source = "mod container {\n    struct Thing {\n        first: i32,\n}\n}\n";
    let result = CodeEngine::default().patch(
        "file",
        Language::Rust,
        source,
        &request(
            source,
            PatchOperation::AppendMember,
            Some("Thing"),
            "pub second: i32,",
        ),
    )?;
    assert!(result.source.ends_with("        pub second: i32,\n}\n}\n"));
    Ok(())
}

#[test]
fn patch_wire_form_and_expansion_budgets_are_enforced() -> Result<(), Box<dyn std::error::Error>> {
    let source = "fn run() {}";
    let edit = request(source, PatchOperation::ReplaceBody, Some("run"), "call();");
    assert_eq!(
        edit,
        serde_json::from_str::<PatchRequest>(&serde_json::to_string(&edit)?)?
    );
    assert!(serde_json::from_str::<PatchRequest>("{\"expected_revision\":\"x\",\"operation\":\"replace_body\",\"selector\":null,\"code\":\"\",\"extra\":true}").is_err());
    let result = CodeEngine::default().patch("file", Language::Rust, source, &edit)?;
    assert_eq!(
        result,
        serde_json::from_str::<PatchResult>(&serde_json::to_string(&result)?)?
    );
    let mut engine = CodeEngine::new(CodeLimits {
        max_source_bytes: 1024,
        ..CodeLimits::default()
    })?;
    let source = format!("fn run() {{\n{}let old = 1;\n}}", " ".repeat(256));
    let huge = request(
        &source,
        PatchOperation::ReplaceBody,
        Some("run"),
        &"let next = 2;\n".repeat(100),
    );
    assert!(matches!(
        engine.patch("file", Language::Rust, &source, &huge),
        Err(CodeError::LimitExceeded { .. })
    ));
    let mut stale = request("fn run() {}", PatchOperation::ReplaceBody, Some("run"), "");
    stale.expected_revision = "x".repeat(129);
    assert!(matches!(
        engine.patch("file", Language::Rust, "fn run() {}", &stale),
        Err(CodeError::LimitExceeded { .. })
    ));
    Ok(())
}
