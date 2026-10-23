//! Public error messages identify the failed operation and corrective boundary.

use tinytools_code::{
    ChunkCatalog, CodeEngine, CodeError, CodeLimits, Language, PatchOperation, PatchRequest,
    revision, select_symbol,
};

#[test]
fn public_refusals_produce_actionable_messages() -> Result<(), CodeError> {
    let mut engine = CodeEngine::default();
    let report = engine.analyze("file", Language::Rust, "fn run() {}")?;
    let errors = [
        CodeEngine::new(CodeLimits {
            max_records: 0,
            ..CodeLimits::default()
        })
        .err(),
        engine.validate(Language::Rust, "fn {").err(),
        select_symbol(&report, "missing").err(),
        select_symbol(&report, &"x".repeat(4097)).err(),
        ChunkCatalog::new().inspect("missing").err(),
        engine
            .patch(
                "file",
                Language::Rust,
                "fn run() {}",
                &PatchRequest {
                    expected_revision: revision("other"),
                    selector: Some("run".to_owned()),
                    operation: PatchOperation::ReplaceBody,
                    code: String::new(),
                },
            )
            .err(),
    ];
    for error in errors {
        assert!(error.is_some_and(|error| error.to_string().len() > 10));
    }
    for error in [
        CodeError::Parser("compiled grammar mismatch".to_owned()),
        CodeError::AmbiguousSymbol("name".to_owned()),
        CodeError::UnsupportedOperation("data imports".to_owned()),
    ] {
        assert!(error.to_string().contains(':'));
        assert!(std::error::Error::source(&error).is_none());
    }
    Ok(())
}

#[test]
fn missing_tokens_are_reported_with_locations() -> Result<(), CodeError> {
    let report =
        CodeEngine::default().analyze("file", Language::Rust, "fn run() { let value = 1 }")?;
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == "missing")
    );
    assert!(
        report
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.range.start_line == 1)
    );
    Ok(())
}
