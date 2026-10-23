//! Synthetic parse contexts expose only generated source to constraints.

use crate::{ChunkTarget, CodeEngine, CodeError, Language, error::enforce_limit};
use tree_sitter::Tree;

pub(super) struct Context {
    pub tree: Tree,
    pub prefix: usize,
    pub indentation: usize,
}

impl Context {
    pub(super) fn offset(&self, source: &str, byte: usize) -> usize {
        self.prefix
            + byte
            + self.indentation * source[..byte].bytes().filter(|byte| *byte == b'\n').count()
    }
}

fn wrapped(
    engine: &CodeEngine,
    language: Language,
    source: &str,
    prefix: &str,
    suffix: &str,
    indentation: usize,
) -> Result<Context, CodeError> {
    let newlines = source.bytes().filter(|byte| *byte == b'\n').count();
    let context_bytes = prefix
        .len()
        .saturating_add(source.len())
        .saturating_add(indentation.saturating_mul(newlines))
        .saturating_add(suffix.len());
    enforce_limit(
        "parse context bytes",
        context_bytes,
        engine.limits().max_source_bytes,
    )?;
    let source = if indentation == 0 {
        source.to_owned()
    } else {
        source.replace('\n', &format!("\n{}", " ".repeat(indentation)))
    };
    let tree = engine.parse(language, &format!("{prefix}{source}{suffix}"))?;
    if tree.root_node().has_error() {
        return Err(CodeError::InvalidSyntax { language });
    }
    Ok(Context {
        tree,
        prefix: prefix.len(),
        indentation,
    })
}

pub(super) fn parse(
    engine: &CodeEngine,
    language: Language,
    target: ChunkTarget,
    source: &str,
) -> Result<Context, CodeError> {
    match (language, target) {
        (_, ChunkTarget::File) => wrapped(engine, language, source, "", "", 0),
        (Language::Rust, ChunkTarget::FunctionBody) => {
            wrapped(engine, language, source, "fn __chunk() {\n", "\n}", 0)
        }
        (Language::Rust, ChunkTarget::Member) => {
            wrapped(engine, language, source, "struct __Chunk {\n", "\n}", 0)
                .or_else(|_| wrapped(engine, language, source, "impl __Chunk {\n", "\n}", 0))
        }
        (Language::Python, ChunkTarget::FunctionBody) => {
            wrapped(engine, language, source, "def __chunk():\n    ", "\n", 4)
        }
        (Language::Python, ChunkTarget::Member) => {
            wrapped(engine, language, source, "class __Chunk:\n    ", "\n", 4)
        }
        (Language::Go, ChunkTarget::FunctionBody) => wrapped(
            engine,
            language,
            source,
            "package chunk\nfunc __chunk() {\n",
            "\n}\n",
            0,
        ),
        (Language::Go, ChunkTarget::Member) => wrapped(
            engine,
            language,
            source,
            "package chunk\ntype __Chunk struct {\n",
            "\n}\n",
            0,
        )
        .or_else(|_| {
            wrapped(
                engine,
                language,
                source,
                "package chunk\ntype __Chunk interface {\n",
                "\n}\n",
                0,
            )
        }),
        (
            Language::JavaScript | Language::TypeScript | Language::Tsx,
            ChunkTarget::FunctionBody,
        ) => wrapped(engine, language, source, "function __chunk() {\n", "\n}", 0),
        (Language::JavaScript | Language::TypeScript | Language::Tsx, ChunkTarget::Member) => {
            wrapped(engine, language, source, "class __Chunk {\n", "\n}", 0)
        }
        _ => Err(CodeError::UnsupportedOperation(
            "data formats only support file chunks".to_owned(),
        )),
    }
}
