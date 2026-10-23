//! Revision-checked targeted syntax edits; no filesystem publication occurs here.

mod imports;
mod indentation;
mod types;

use crate::{
    CodeEngine, CodeError, Language,
    analysis::body_node,
    error::{enforce_limit, invalid},
    revision, select_symbol,
};
pub use types::{PatchOperation, PatchRequest, PatchResult};

impl CodeEngine {
    /// Prepare a targeted edit against an exact source revision.
    ///
    /// # Errors
    /// Refuses stale revisions, invalid original/result syntax, ambiguous or
    /// missing symbols, unsupported containers and exceeded resource budgets.
    pub fn patch(
        &mut self,
        key: &str,
        language: Language,
        source: &str,
        request: &PatchRequest,
    ) -> Result<PatchResult, CodeError> {
        let limit = self.limits().max_source_bytes;
        enforce_limit("source bytes", source.len(), limit)?;
        enforce_limit("patch bytes", request.code.len(), limit)?;
        enforce_limit("revision bytes", request.expected_revision.len(), 128)?;
        let before_revision = revision(source);
        if request.expected_revision != before_revision {
            return Err(CodeError::StaleRevision {
                expected: request.expected_revision.clone(),
                actual: before_revision,
            });
        }
        let tree = self.parse(language, source)?;
        if tree.root_node().has_error() {
            return Err(CodeError::InvalidSyntax { language });
        }
        let (start, end, replacement) = if request.operation == PatchOperation::InsertImport {
            imports::insert(self, &tree, language, source, &request.code)?
        } else {
            let report = self.analyze(key, language, source)?;
            let selector = request
                .selector
                .as_deref()
                .ok_or_else(|| invalid("this operation requires a symbol selector"))?;
            let symbol = select_symbol(&report, selector)?;
            let node = tree
                .root_node()
                .descendant_for_byte_range(symbol.range.start_byte, symbol.range.end_byte)
                .ok_or_else(|| CodeError::SymbolNotFound(selector.to_owned()))?;
            match request.operation {
                PatchOperation::ReplaceSymbol => (
                    symbol.range.start_byte,
                    symbol.range.end_byte,
                    request.code.clone(),
                ),
                PatchOperation::Wrap => {
                    let (prefix, suffix) = request
                        .code
                        .split_once("{{node}}")
                        .ok_or_else(|| invalid("wrap requires exactly one {{node}} marker"))?;
                    if suffix.contains("{{node}}") {
                        return Err(invalid("wrap requires exactly one {{node}} marker"));
                    }
                    let original = &source[symbol.range.start_byte..symbol.range.end_byte];
                    enforce_limit(
                        "wrapped bytes",
                        prefix
                            .len()
                            .saturating_add(original.len())
                            .saturating_add(suffix.len()),
                        limit,
                    )?;
                    (
                        symbol.range.start_byte,
                        symbol.range.end_byte,
                        format!("{prefix}{original}{suffix}"),
                    )
                }
                PatchOperation::ReplaceBody | PatchOperation::AppendMember => {
                    let body = body_node(node).ok_or_else(|| {
                        CodeError::UnsupportedOperation("selected symbol has no body".to_owned())
                    })?;
                    if request.operation == PatchOperation::ReplaceBody {
                        indentation::replace_body(body, source, language, &request.code, limit)?
                    } else {
                        indentation::append_member(body, source, language, &request.code, limit)?
                    }
                }
                PatchOperation::InsertImport => {
                    return Err(invalid("import selection is not a symbol operation"));
                }
            }
        };
        let result_bytes = source
            .len()
            .saturating_sub(end - start)
            .saturating_add(replacement.len());
        enforce_limit("patched bytes", result_bytes, limit)?;
        let mut result = String::with_capacity(result_bytes);
        result.push_str(&source[..start]);
        result.push_str(&replacement);
        result.push_str(&source[end..]);
        self.validate(language, &result)?;
        let after_revision = revision(&result);
        Ok(PatchResult {
            changed: before_revision != after_revision,
            before_revision,
            after_revision,
            source: result,
        })
    }
}
