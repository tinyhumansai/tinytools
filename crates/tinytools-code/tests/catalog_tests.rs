//! Metadata discovery, portable custom manifests and catalog budget regressions.

use serde_json::json;
use tinytools_code::{ChunkCatalog, ChunkManifest, CodeError, Language};

fn manifest() -> Result<ChunkManifest, serde_json::Error> {
    serde_json::from_value(
        json!({"id":"custom.example.v1","summary":"An example","language":"rust","template_path":"templates/example.rs"}),
    )
}

#[test]
fn builtin_catalog_is_metadata_only_and_deterministic() -> Result<(), Box<dyn std::error::Error>> {
    let catalog = ChunkCatalog::builtin()?;
    let all = catalog.search("", None, 100);
    assert_eq!(all.len(), 12);
    assert_eq!(all, catalog.search("", None, 100));
    assert_eq!(catalog.search("", None, 0).len(), 0);
    assert_eq!(catalog.search("nonsenseunmatched", None, 3).len(), 0);
    assert!(
        catalog
            .search("fallible function", Some(Language::Rust), 2)
            .iter()
            .all(|entry| entry.language == Language::Rust)
    );
    for summary in all {
        let metadata = catalog.inspect(&summary.id)?;
        assert_eq!(metadata, catalog.manifest(&summary.id)?);
        assert!(catalog.builtin_source(&summary.id).is_some());
        assert_eq!(metadata.revision()?, metadata.revision()?);
        let wire = serde_json::to_value(&summary)?;
        assert!(wire.get("source").is_none());
        assert!(wire.get("template").is_none());
    }
    Ok(())
}

#[test]
fn custom_entries_cannot_claim_embedded_source_or_overwrite_ids()
-> Result<(), Box<dyn std::error::Error>> {
    let mut catalog = ChunkCatalog::new();
    let mut custom = manifest()?;
    custom.id = "rust.fallible-function.v1".to_owned();
    catalog.insert(custom.clone())?;
    assert_eq!(catalog.builtin_source(&custom.id), None);
    assert!(catalog.insert(custom).is_err());
    assert!(matches!(
        catalog.inspect("missing"),
        Err(CodeError::UnknownChunk(_))
    ));
    assert!(matches!(
        catalog.inspect(&"x".repeat(129)),
        Err(CodeError::LimitExceeded { .. })
    ));
    assert_eq!(catalog.builtin_source("missing"), None);
    Ok(())
}

#[test]
fn unsafe_paths_unknown_fields_and_wrong_defaults_are_refused()
-> Result<(), Box<dyn std::error::Error>> {
    for path in [
        "",
        "/absolute",
        "../parent",
        "a/../b",
        "a/./b",
        "a//b",
        "C:/drive",
        "a\\b",
        "a/space ",
        "a/dot.",
        "a\0b",
    ] {
        let mut custom = manifest()?;
        custom.template_path = path.to_owned();
        assert!(custom.validate().is_err(), "accepted {path:?}");
    }
    let mut value = serde_json::to_value(manifest()?)?;
    value["shell"] = json!("execute something");
    assert!(serde_json::from_value::<ChunkManifest>(value).is_err());
    for (kind, default) in [
        ("identifier", json!("not valid")),
        ("pascal_case_ident", json!("lower")),
        ("type_expr", json!(5)),
        ("string", json!(true)),
        ("number", json!("5")),
        ("boolean", json!(1)),
    ] {
        let mut value = serde_json::to_value(manifest()?)?;
        value["parameters"] = json!([{"name":"value","kind":kind,"default":default}]);
        assert!(
            serde_json::from_value::<ChunkManifest>(value)?
                .validate()
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn manifest_lists_and_names_are_bounded_and_unique() -> Result<(), Box<dyn std::error::Error>> {
    for update in [
        json!({"id":""}),
        json!({"id":"bad/id"}),
        json!({"summary":" "}),
        json!({"parameters":[{"name":"x","kind":"string"},{"name":"x","kind":"string"}]}),
        json!({"slots":[{"name":"x"},{"name":"x"}]}),
        json!({"slots":[{"name":"bad name"}]}),
        json!({"slots":[{"name":"x","node_kind":"bad-kind"}]}),
        json!({"forbidden_nodes":["invalid kind"]}),
        json!({"tags":vec!["tag";33]}),
        json!({"dependencies":vec!["dep";65]}),
        json!({"parameters":(0..33).map(|index| json!({"name":format!("x{index}"),"kind":"string"})).collect::<Vec<_>>()}),
        json!({"slots":(0..33).map(|index| json!({"name":format!("x{index}")})).collect::<Vec<_>>()}),
        json!({"forbidden_nodes":vec!["call_expression";33]}),
        json!({"summary":"x".repeat(1025)}),
    ] {
        let mut value = serde_json::to_value(manifest()?)?;
        if let Some(update) = update.as_object() {
            for (key, item) in update {
                value[key] = item.clone();
            }
        }
        assert!(
            serde_json::from_value::<ChunkManifest>(value)?
                .validate()
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn catalog_capacity_and_metadata_revisions_are_stable() -> Result<(), Box<dyn std::error::Error>> {
    let mut catalog = ChunkCatalog::default();
    for index in 0..512 {
        let mut custom = manifest()?;
        custom.id = format!("custom.{index}.v1");
        catalog.insert(custom)?;
    }
    assert!(matches!(
        catalog.insert(manifest()?),
        Err(CodeError::LimitExceeded { .. })
    ));
    assert_eq!(catalog.search(&"é".repeat(1025), None, 50).len(), 0);
    assert_eq!(catalog.search("", None, 999).len(), 50);
    let mut custom = manifest()?;
    let before = custom.revision()?;
    custom.summary.push_str(" changed");
    assert_ne!(before, custom.revision()?);
    Ok(())
}

#[test]
fn constraint_node_kinds_must_belong_to_the_selected_grammar()
-> Result<(), Box<dyn std::error::Error>> {
    for (language, kind) in [
        (Language::Rust, "function_item"),
        (Language::TypeScript, "interface_declaration"),
        (Language::Tsx, "jsx_element"),
        (Language::JavaScript, "function_declaration"),
        (Language::Python, "function_definition"),
        (Language::Go, "function_declaration"),
        (Language::Json, "pair"),
        (Language::Toml, "table"),
    ] {
        let mut value = serde_json::to_value(manifest()?)?;
        value["language"] = serde_json::to_value(language)?;
        value["forbidden_nodes"] = json!([kind]);
        value["slots"] = json!([{"name":"body","node_kind":kind}]);
        serde_json::from_value::<ChunkManifest>(value.clone())?.validate()?;

        value["forbidden_nodes"] = json!(["not_a_grammar_node"]);
        assert!(matches!(
            serde_json::from_value::<ChunkManifest>(value.clone())?.validate(),
            Err(CodeError::InvalidInput(_))
        ));
        value["forbidden_nodes"] = json!([kind]);
        value["slots"][0]["node_kind"] = json!("not_a_grammar_node");
        assert!(matches!(
            serde_json::from_value::<ChunkManifest>(value)?.validate(),
            Err(CodeError::InvalidInput(_))
        ));
    }

    let mut custom = manifest()?;
    for kind in ["function_definition", "unsafe"] {
        custom.forbidden_nodes = vec![kind.to_owned()];
        assert!(
            custom.validate().is_err(),
            "accepted Rust constraint {kind}"
        );
    }
    Ok(())
}
