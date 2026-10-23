//! Grammar, lexical selection, cache and diagnostic boundary regressions.

use tinytools_code::{
    Analysis, CacheStatus, CodeEngine, CodeError, CodeLimits, Language, select_symbol,
};

#[test]
fn all_explicit_grammars_produce_lexical_outlines() -> Result<(), CodeError> {
    for (language, source, name) in [
        (
            Language::Rust,
            "use std::fmt; fn greet() { other(); }",
            "greet",
        ),
        (
            Language::TypeScript,
            "import { x } from 'x'; interface Item { value: string; } function greet(): void { x(); }",
            "greet",
        ),
        (
            Language::Tsx,
            "export function Action() { return <button>Run</button>; }",
            "Action",
        ),
        (
            Language::JavaScript,
            "import x from 'x'; function greet() { x(); }",
            "greet",
        ),
        (
            Language::Python,
            "import os\ndef greet():\n    print('hi')\n",
            "greet",
        ),
        (
            Language::Go,
            "package main\nimport \"fmt\"\nfunc greet() { fmt.Println(1) }",
            "greet",
        ),
        (Language::Json, "{\"name\": \"value\"}", "name"),
        (Language::Toml, "[service]\nname = 'value'\n", "name"),
    ] {
        let report = CodeEngine::default().analyze("file", language, source)?;
        assert!(
            report.diagnostics.is_empty(),
            "{language:?}: {:?}",
            report.diagnostics
        );
        assert!(
            report.symbols.iter().any(|symbol| symbol.name == name),
            "{language:?}: {:?}",
            report.symbols
        );
        if matches!(
            language,
            Language::Rust
                | Language::TypeScript
                | Language::JavaScript
                | Language::Python
                | Language::Go
        ) {
            assert!(
                report
                    .references
                    .iter()
                    .any(|reference| reference.kind == "call")
            );
            assert!(
                report
                    .references
                    .iter()
                    .any(|reference| reference.kind == "import")
            );
        }
    }
    Ok(())
}

#[test]
fn symbol_selection_requires_unique_names_or_revision_ids() -> Result<(), CodeError> {
    let source = "mod first { fn same() {} } mod second { fn same() {} }";
    let report = CodeEngine::default().analyze("file", Language::Rust, source)?;
    assert!(matches!(
        select_symbol(&report, "same"),
        Err(CodeError::AmbiguousSymbol(_))
    ));
    assert_eq!(select_symbol(&report, "first::same")?.name, "same");
    assert!(matches!(
        select_symbol(&report, "missing"),
        Err(CodeError::SymbolNotFound(_))
    ));
    let symbol = select_symbol(&report, "second::same")?;
    assert_eq!(select_symbol(&report, &symbol.id)?, symbol);
    assert!(symbol.range.start_line > 0 && symbol.range.start_column > 0);
    assert!(symbol.range.end_byte > symbol.range.start_byte);
    Ok(())
}

#[test]
fn unicode_incremental_edits_match_fresh_parsing() -> Result<(), CodeError> {
    let mut engine = CodeEngine::default();
    for (index, source) in [
        "fn café() { let value = \"é\"; }",
        "fn café() {\n let value = \"ê\";\n}",
        "fn tea() {}",
        "",
        "fn tea() {}",
    ]
    .iter()
    .enumerate()
    {
        let mut report = engine.analyze("file", Language::Rust, source)?;
        assert_eq!(
            report.cache_status,
            if index == 0 {
                CacheStatus::Fresh
            } else {
                CacheStatus::Incremental
            }
        );
        report.cache_status = CacheStatus::Fresh;
        assert_eq!(
            report,
            CodeEngine::default().analyze("file", Language::Rust, source)?
        );
    }
    assert_eq!(
        engine
            .analyze("file", Language::Rust, "fn tea() {}")?
            .cache_status,
        CacheStatus::Cached
    );
    let stats = engine.cache_stats();
    assert_eq!(
        (stats.entries, stats.hits, stats.incremental_parses),
        (1, 1, 4)
    );
    engine.clear_cache();
    assert_eq!(
        (
            engine.cache_stats().entries,
            engine.cache_stats().source_bytes
        ),
        (0, 0)
    );
    Ok(())
}

#[test]
fn incremental_edits_expand_partial_utf8_prefixes_and_suffixes() -> Result<(), CodeError> {
    for (before, after) in [("café", "cafê"), ("é", "Ω")] {
        let old = format!("fn {before}() -> i32 {{\n    1\n}}\n");
        let new = format!("fn {after}() -> i32 {{\n    1\n}}\n");
        let mut engine = CodeEngine::default();
        engine.analyze("unicode.rs", Language::Rust, &old)?;
        let mut incremental = engine.analyze("unicode.rs", Language::Rust, &new)?;
        assert_eq!(incremental.cache_status, CacheStatus::Incremental);
        assert_eq!(select_symbol(&incremental, after)?.name, after);
        incremental.cache_status = CacheStatus::Fresh;
        assert_eq!(
            incremental,
            CodeEngine::default().analyze("unicode.rs", Language::Rust, &new)?
        );
    }
    Ok(())
}

#[test]
fn lru_and_zero_budgets_enforce_retained_source_bounds() -> Result<(), CodeError> {
    let mut engine = CodeEngine::new(CodeLimits {
        max_cache_entries: 2,
        max_cache_bytes: 30,
        ..CodeLimits::default()
    })?;
    for key in ["a", "b", "a", "c"] {
        engine.analyze(key, Language::Rust, "fn run() {}")?;
    }
    assert_eq!(engine.cache_stats().evictions, 1);
    assert_eq!(
        engine
            .analyze("b", Language::Rust, "fn run() {}")?
            .cache_status,
        CacheStatus::Fresh
    );
    assert!(engine.cache_stats().source_bytes <= 30);
    for limits in [
        CodeLimits {
            max_cache_entries: 0,
            ..CodeLimits::default()
        },
        CodeLimits {
            max_cache_bytes: 0,
            ..CodeLimits::default()
        },
        CodeLimits {
            max_cache_bytes: 1,
            ..CodeLimits::default()
        },
    ] {
        let mut engine = CodeEngine::new(limits)?;
        for source in ["", "fn run() {}"] {
            engine.analyze("file", Language::Rust, source)?;
            if !source.is_empty() || limits.max_cache_bytes == 0 || limits.max_cache_entries == 0 {
                assert_eq!(
                    engine.analyze("file", Language::Rust, source)?.cache_status,
                    CacheStatus::Fresh
                );
            }
        }
    }
    Ok(())
}

#[test]
fn record_caps_never_imply_name_uniqueness() -> Result<(), CodeError> {
    let mut engine = CodeEngine::new(CodeLimits {
        max_records: 1,
        ..CodeLimits::default()
    })?;
    let report = engine.analyze(
        "file",
        Language::Rust,
        "fn same() { one(); two(); } mod nested { fn same() {} }",
    )?;
    assert!(report.symbols_truncated && report.references_truncated);
    assert_eq!((report.symbols.len(), report.references.len()), (1, 1));
    assert!(matches!(
        select_symbol(&report, "same"),
        Err(CodeError::AmbiguousSymbol(_))
    ));
    assert_eq!(select_symbol(&report, &report.symbols[0].id)?.name, "same");
    Ok(())
}

#[test]
fn diagnostic_caps_and_invalid_source_are_explicit() -> Result<(), CodeError> {
    let source = "fn run() { let = ; let = ; let = ; }";
    let mut engine = CodeEngine::new(CodeLimits {
        max_diagnostics: 1,
        ..CodeLimits::default()
    })?;
    let report = engine.analyze("file", Language::Rust, source)?;
    assert_eq!(report.diagnostics.len(), 1);
    assert!(report.diagnostics_truncated);
    assert!(matches!(
        engine.validate(Language::Rust, source),
        Err(CodeError::InvalidSyntax { .. })
    ));
    Ok(())
}

#[test]
fn engine_configuration_and_request_budgets_are_checked() -> Result<(), CodeError> {
    for limits in [
        CodeLimits {
            max_source_bytes: 0,
            ..CodeLimits::default()
        },
        CodeLimits {
            max_records: 0,
            ..CodeLimits::default()
        },
        CodeLimits {
            max_diagnostics: 0,
            ..CodeLimits::default()
        },
        CodeLimits {
            max_source_bytes: 16_777_217,
            ..CodeLimits::default()
        },
        CodeLimits {
            max_cache_entries: 1025,
            ..CodeLimits::default()
        },
        CodeLimits {
            max_cache_bytes: 134_217_729,
            ..CodeLimits::default()
        },
        CodeLimits {
            max_records: 65_537,
            ..CodeLimits::default()
        },
        CodeLimits {
            max_diagnostics: 4097,
            ..CodeLimits::default()
        },
    ] {
        assert!(CodeEngine::new(limits).is_err());
    }
    let mut engine = CodeEngine::new(CodeLimits {
        max_source_bytes: 8,
        ..CodeLimits::default()
    })?;
    assert!(matches!(
        engine.analyze("file", Language::Rust, "fn run() {}"),
        Err(CodeError::LimitExceeded { .. })
    ));
    assert!(matches!(
        engine.analyze(&"x".repeat(4097), Language::Rust, ""),
        Err(CodeError::LimitExceeded { .. })
    ));
    Ok(())
}

#[test]
fn folded_views_preserve_docs_and_surrounding_source() -> Result<(), CodeError> {
    let source =
        "// license\n/// Docs\nfn run() {\n let first = 1;\n let second = 2;\n let third = 3;\n}\n";
    let report = CodeEngine::default().analyze("rust", Language::Rust, source)?;
    assert!(
        report
            .skeleton
            .starts_with("// license\n/// Docs\nfn run()")
    );
    assert!(report.skeleton.contains("body folded"));
    assert!(report.skeleton.len() < source.len());
    for literal in ["\"doc\"", "r\"doc\"", "\"\"\"long docs\"\"\""] {
        let source = format!(
            "def run():\n    {literal}\n    value = 123456\n    other = 98765\n    return value + other\n"
        );
        let report = CodeEngine::default().analyze("python", Language::Python, &source)?;
        assert!(report.skeleton.contains(literal));
        assert!(!report.skeleton.contains("98765"));
    }
    Ok(())
}

#[test]
fn language_hints_and_analysis_wire_form_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    for (path, language) in [
        ("test.RS", Language::Rust),
        ("a.mts", Language::TypeScript),
        ("a.cts", Language::TypeScript),
        ("a.tsx", Language::Tsx),
        ("a.jsx", Language::JavaScript),
        ("a.cjs", Language::JavaScript),
        ("a.mjs", Language::JavaScript),
        ("a.pyi", Language::Python),
        ("a.go", Language::Go),
        ("a.json", Language::Json),
        ("a.toml", Language::Toml),
    ] {
        assert_eq!(Language::from_path(path), Some(language));
    }
    assert_eq!(Language::from_path("file"), None);
    assert_eq!(Language::from_path("file.unknown"), None);
    assert_eq!(
        serde_json::from_str::<Language>("\"ts\"")?,
        Language::TypeScript
    );
    assert_eq!(
        serde_json::from_str::<Language>("\"js\"")?,
        Language::JavaScript
    );
    let report = CodeEngine::default().analyze(
        "fixture",
        Language::Rust,
        include_str!("fixtures/analysis.rs"),
    )?;
    assert_eq!(
        report,
        serde_json::from_str::<Analysis>(&serde_json::to_string(&report)?)?
    );
    Ok(())
}
