//! One-pass rendering, typed placeholders, syntax contexts and slot constraints.

use serde_json::json;
use tinytools_code::{
    ChunkBindings, ChunkCatalog, ChunkManifest, ChunkTarget, CodeEngine, CodeError, CodeLimits,
    Language, RenderedChunk, render_chunk,
};

fn manifest(language: Language, target: ChunkTarget) -> ChunkManifest {
    ChunkManifest {
        id: "custom.example.v1".to_owned(),
        summary: "An example".to_owned(),
        language,
        target,
        template_path: "template.txt".to_owned(),
        parameters: Vec::new(),
        slots: Vec::new(),
        tags: Vec::new(),
        dependencies: Vec::new(),
        forbidden_nodes: Vec::new(),
    }
}

#[test]
fn every_builtin_pattern_renders_with_its_declared_contract()
-> Result<(), Box<dyn std::error::Error>> {
    let catalog = ChunkCatalog::builtin()?;
    let mut engine = CodeEngine::default();
    for summary in catalog.search("", None, 50) {
        let metadata = catalog.inspect(&summary.id)?;
        let Some(template) = catalog.builtin_source(&summary.id) else {
            return Err("missing built-in template".into());
        };
        let mut bindings = ChunkBindings::default();
        for slot in &metadata.slots {
            if slot.required {
                let source = match (metadata.language, slot.name.as_str()) {
                    (Language::Python, "setup") => "resource = object()",
                    (Language::Python, _) => "del resource",
                    (Language::Go, _) => "w.WriteHeader(http.StatusOK)",
                    (_, "assertions") => "expect(1 + 1).toBe(2);",
                    _ => "return value;",
                };
                bindings.slots.insert(slot.name.clone(), source.to_owned());
            }
        }
        let result = render_chunk(&mut engine, metadata, template, &bindings)?;
        assert!(!result.source.contains("@slot"));
        assert_eq!(result.id, metadata.id);
        assert_eq!(result.dependencies, metadata.dependencies);
        assert_eq!(
            result,
            serde_json::from_str::<RenderedChunk>(&serde_json::to_string(&result)?)?
        );
    }
    Ok(())
}

#[test]
fn inserted_string_markers_and_quotes_are_never_rescanned() -> Result<(), Box<dyn std::error::Error>>
{
    let mut metadata = manifest(Language::Rust, ChunkTarget::File);
    metadata.parameters =
        serde_json::from_value(json!([{"name":"text","kind":"string","required":true}]))?;
    let mut bindings = ChunkBindings::default();
    bindings
        .parameters
        .insert("text".to_owned(), json!("{{evil}}\"\n// @slot evil"));
    let result = render_chunk(
        &mut CodeEngine::default(),
        &metadata,
        "const TEXT: &str = {{text}};",
        &bindings,
    )?;
    assert!(result.source.contains("{{evil}}"));
    assert!(result.source.contains("\\\"\\n"));
    Ok(())
}

#[test]
fn rust_strings_escape_control_characters_with_rust_literal_syntax()
-> Result<(), Box<dyn std::error::Error>> {
    let mut metadata = manifest(Language::Rust, ChunkTarget::File);
    metadata.parameters =
        serde_json::from_value(json!([{"name":"text","kind":"string","required":true}]))?;
    let mut bindings = ChunkBindings::default();
    bindings
        .parameters
        .insert("text".to_owned(), json!("\0\u{0008}\u{000c}é\"\\\n"));
    let result = render_chunk(
        &mut CodeEngine::default(),
        &metadata,
        "const TEXT: &str = {{text}};",
        &bindings,
    )?;
    assert!(result.source.contains("\\u{0}"));
    assert!(result.source.contains("\\u{8}"));
    assert!(result.source.contains("\\u{c}"));
    assert!(result.source.contains("\\\"\\\\\\n"));
    Ok(())
}

#[test]
fn identifiers_and_type_expressions_reject_statement_injection()
-> Result<(), Box<dyn std::error::Error>> {
    let catalog = ChunkCatalog::builtin()?;
    let metadata = catalog.inspect("rust.public-field.v1")?;
    let Some(template) = catalog.builtin_source(&metadata.id) else {
        return Err("missing template".into());
    };
    for (name, value) in [
        ("name", "field; injected"),
        ("type", "i32; fn injected() {}"),
        ("type", ""),
        ("type", "Vec<"),
    ] {
        let mut bindings = ChunkBindings::default();
        bindings.parameters.insert(name.to_owned(), json!(value));
        assert!(render_chunk(&mut CodeEngine::default(), metadata, template, &bindings).is_err());
    }
    let metadata = catalog.inspect("tsx.accessible-action.v1")?;
    let Some(template) = catalog.builtin_source(&metadata.id) else {
        return Err("missing template".into());
    };
    let mut bindings = ChunkBindings::default();
    bindings
        .parameters
        .insert("name".to_owned(), json!("lower"));
    assert!(render_chunk(&mut CodeEngine::default(), metadata, template, &bindings).is_err());
    Ok(())
}

#[test]
fn slots_must_be_original_standalone_comments_and_are_one_pass()
-> Result<(), Box<dyn std::error::Error>> {
    let mut metadata = manifest(Language::Rust, ChunkTarget::File);
    metadata.slots = serde_json::from_value(json!([{"name":"body","required":true}]))?;
    let mut bindings = ChunkBindings::default();
    bindings.slots.insert(
        "body".to_owned(),
        "let text = \"// @slot injected\";".to_owned(),
    );
    let result = render_chunk(
        &mut CodeEngine::default(),
        &metadata,
        "fn run() {\n    // @slot body\n}",
        &bindings,
    )?;
    assert!(result.source.contains("// @slot injected"));
    for template in [
        "fn run() { let text = \"// @slot body\"; }",
        "fn run() {} // @slot body",
        "fn run() {\n// @slot body\n// @slot body\n}",
        "fn run() {\n// @slot unknown\n}",
    ] {
        assert!(render_chunk(&mut CodeEngine::default(), &metadata, template, &bindings).is_err());
    }
    let result = render_chunk(
        &mut CodeEngine::default(),
        &metadata,
        "fn run() {\n    /* @slot body */\n}",
        &bindings,
    )?;
    assert!(result.source.contains("let text"));
    Ok(())
}

#[test]
fn required_unknown_and_unused_bindings_are_refused() -> Result<(), Box<dyn std::error::Error>> {
    let mut metadata = manifest(Language::Rust, ChunkTarget::File);
    metadata.parameters =
        serde_json::from_value(json!([{"name":"name","kind":"identifier","required":true}]))?;
    let mut bindings = ChunkBindings::default();
    assert!(
        render_chunk(
            &mut CodeEngine::default(),
            &metadata,
            "fn {{name}}() {}",
            &bindings
        )
        .is_err()
    );
    bindings
        .parameters
        .insert("unknown".to_owned(), json!("run"));
    assert!(
        render_chunk(
            &mut CodeEngine::default(),
            &metadata,
            "fn {{name}}() {}",
            &bindings
        )
        .is_err()
    );
    bindings.parameters.clear();
    bindings.parameters.insert("name".to_owned(), json!("run"));
    for template in ["fn run() {}", "fn {{name() {}", "fn {{missing}}() {}"] {
        assert!(render_chunk(&mut CodeEngine::default(), &metadata, template, &bindings).is_err());
    }
    metadata.parameters.clear();
    bindings.parameters.clear();
    metadata.slots = serde_json::from_value(json!([{"name":"body","required":true}]))?;
    assert!(
        render_chunk(
            &mut CodeEngine::default(),
            &metadata,
            "fn run() {\n// @slot body\n}",
            &bindings
        )
        .is_err()
    );
    bindings
        .slots
        .insert("unknown".to_owned(), "let x=1;".to_owned());
    assert!(
        render_chunk(
            &mut CodeEngine::default(),
            &metadata,
            "fn run() {\n// @slot body\n}",
            &bindings
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn exact_slot_nodes_and_generated_node_restrictions_are_enforced()
-> Result<(), Box<dyn std::error::Error>> {
    let mut metadata = manifest(Language::Rust, ChunkTarget::File);
    metadata.slots = serde_json::from_value(
        json!([{"name":"body","required":true,"node_kind":"let_declaration"}]),
    )?;
    let mut bindings = ChunkBindings::default();
    bindings
        .slots
        .insert("body".to_owned(), "let value = 1;".to_owned());
    let template = "fn run() {\n    // @slot body\n}";
    render_chunk(&mut CodeEngine::default(), &metadata, template, &bindings)?;
    bindings
        .slots
        .insert("body".to_owned(), "call();".to_owned());
    assert!(render_chunk(&mut CodeEngine::default(), &metadata, template, &bindings).is_err());
    bindings.slots.insert(
        "body".to_owned(),
        "let value = 1; let other = 2;".to_owned(),
    );
    assert!(render_chunk(&mut CodeEngine::default(), &metadata, template, &bindings).is_err());
    metadata.slots[0].node_kind = None;
    metadata.forbidden_nodes.push("call_expression".to_owned());
    bindings
        .slots
        .insert("body".to_owned(), "call();".to_owned());
    assert!(render_chunk(&mut CodeEngine::default(), &metadata, template, &bindings).is_err());
    Ok(())
}

#[test]
fn member_contexts_support_methods_fields_and_interfaces() -> Result<(), CodeError> {
    for (language, source) in [
        (Language::Rust, "pub value: i32,"),
        (Language::Rust, "fn added(&self) {}"),
        (Language::Go, "Value string"),
        (Language::Go, "Run() error"),
        (Language::TypeScript, "run(): void {}"),
        (Language::JavaScript, "run() {}"),
        (Language::Python, "def run(self):\n    return 1"),
    ] {
        let metadata = manifest(language, ChunkTarget::Member);
        render_chunk(
            &mut CodeEngine::default(),
            &metadata,
            source,
            &ChunkBindings::default(),
        )?;
    }
    for language in [Language::Json, Language::Toml] {
        assert!(matches!(
            render_chunk(
                &mut CodeEngine::default(),
                &manifest(language, ChunkTarget::Member),
                "",
                &ChunkBindings::default()
            ),
            Err(CodeError::UnsupportedOperation(_))
        ));
    }
    Ok(())
}

#[test]
fn synthetic_wrapper_nodes_are_excluded_from_generated_restrictions() -> Result<(), CodeError> {
    let mut metadata = manifest(Language::Rust, ChunkTarget::FunctionBody);
    metadata.forbidden_nodes = vec!["function_item".to_owned()];
    render_chunk(
        &mut CodeEngine::default(),
        &metadata,
        "let value = 1;",
        &ChunkBindings::default(),
    )?;
    assert!(
        render_chunk(
            &mut CodeEngine::default(),
            &metadata,
            "fn nested() {}",
            &ChunkBindings::default()
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn python_slots_keep_multiline_node_coordinates_in_body_context()
-> Result<(), Box<dyn std::error::Error>> {
    let mut metadata = manifest(Language::Python, ChunkTarget::FunctionBody);
    metadata.slots = serde_json::from_value(
        json!([{"name":"body","required":true,"node_kind":"if_statement"}]),
    )?;
    let mut bindings = ChunkBindings::default();
    bindings
        .slots
        .insert("body".to_owned(), "if True:\n    return 1".to_owned());
    render_chunk(
        &mut CodeEngine::default(),
        &metadata,
        "# @slot body",
        &bindings,
    )?;
    for (language, source) in [
        (Language::Go, "return"),
        (Language::JavaScript, "return 1;"),
        (Language::TypeScript, "return 1;"),
    ] {
        render_chunk(
            &mut CodeEngine::default(),
            &manifest(language, ChunkTarget::FunctionBody),
            source,
            &ChunkBindings::default(),
        )?;
    }
    Ok(())
}

#[test]
fn typed_values_cover_boolean_number_and_language_type_contexts()
-> Result<(), Box<dyn std::error::Error>> {
    for (language, template, value) in [
        (Language::Rust, "type Alias = {{type}};", "Vec<String>"),
        (
            Language::TypeScript,
            "type Alias = {{type}};",
            "Array<string>",
        ),
        (Language::Python, "value: {{type}}", "list[str]"),
        (
            Language::Go,
            "package main\ntype Alias {{type}}\n",
            "map[string]int",
        ),
    ] {
        let mut metadata = manifest(language, ChunkTarget::File);
        metadata.parameters =
            serde_json::from_value(json!([{"name":"type","kind":"type_expr","required":true}]))?;
        let mut bindings = ChunkBindings::default();
        bindings.parameters.insert("type".to_owned(), json!(value));
        render_chunk(&mut CodeEngine::default(), &metadata, template, &bindings)?;
    }
    let mut metadata = manifest(Language::Python, ChunkTarget::File);
    metadata.parameters = serde_json::from_value(
        json!([{"name":"flag","kind":"boolean"},{"name":"count","kind":"number"}]),
    )?;
    let mut bindings = ChunkBindings::default();
    bindings.parameters.insert("flag".to_owned(), json!(false));
    bindings.parameters.insert("count".to_owned(), json!(1.5));
    let rendered = render_chunk(
        &mut CodeEngine::default(),
        &metadata,
        "flag = {{flag}}\ncount = {{count}}",
        &bindings,
    )?;
    assert!(rendered.source.contains("False"));
    for (name, wrong) in [("flag", json!("true")), ("count", json!("1.5"))] {
        let mut invalid = bindings.clone();
        invalid.parameters.insert(name.to_owned(), wrong);
        assert!(
            render_chunk(
                &mut CodeEngine::default(),
                &metadata,
                "flag = {{flag}}\ncount = {{count}}",
                &invalid
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn unsupported_type_contexts_invalid_code_and_binding_wire_errors_are_refused()
-> Result<(), Box<dyn std::error::Error>> {
    for language in [Language::JavaScript, Language::Json, Language::Toml] {
        let mut metadata = manifest(language, ChunkTarget::File);
        metadata.parameters =
            serde_json::from_value(json!([{"name":"type","kind":"type_expr","default":"string"}]))?;
        assert!(matches!(
            render_chunk(
                &mut CodeEngine::default(),
                &metadata,
                "{{type}}",
                &ChunkBindings::default()
            ),
            Err(CodeError::UnsupportedOperation(_))
        ));
    }
    assert!(
        render_chunk(
            &mut CodeEngine::default(),
            &manifest(Language::Rust, ChunkTarget::File),
            "fn {",
            &ChunkBindings::default()
        )
        .is_err()
    );
    let bindings = ChunkBindings::default();
    assert_eq!(
        bindings,
        serde_json::from_str::<ChunkBindings>(&serde_json::to_string(&bindings)?)?
    );
    assert!(serde_json::from_str::<ChunkBindings>("{\"shell\":true}").is_err());
    Ok(())
}

#[test]
fn rendered_expansion_and_binding_names_are_bounded() -> Result<(), Box<dyn std::error::Error>> {
    let mut metadata = manifest(Language::Rust, ChunkTarget::File);
    metadata.slots = serde_json::from_value(json!([{"name":"body","required":true}]))?;
    let template = format!("fn run() {{\n{}// @slot body\n}}", " ".repeat(256));
    let mut bindings = ChunkBindings::default();
    bindings
        .slots
        .insert("body".to_owned(), "let next = 2;\n".repeat(16));
    let mut engine = CodeEngine::new(CodeLimits {
        max_source_bytes: 1024,
        ..CodeLimits::default()
    })?;
    assert!(matches!(
        render_chunk(&mut engine, &metadata, &template, &bindings),
        Err(CodeError::LimitExceeded { .. })
    ));
    bindings.slots.insert(
        "body".to_owned(),
        "let next = 2;\n\nlet last = 3;".to_owned(),
    );
    assert!(
        render_chunk(&mut engine, &metadata, &template, &bindings)?
            .source
            .len()
            <= 1024
    );
    bindings
        .parameters
        .insert("x".repeat(129), json!("anything"));
    assert!(matches!(
        render_chunk(&mut engine, &metadata, &template, &bindings),
        Err(CodeError::LimitExceeded { .. })
    ));
    bindings.parameters.clear();
    bindings.slots = (0..33)
        .map(|index| (format!("slot{index}"), String::new()))
        .collect();
    assert!(matches!(
        render_chunk(&mut engine, &metadata, &template, &bindings),
        Err(CodeError::LimitExceeded { .. })
    ));
    Ok(())
}
