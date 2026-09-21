//! The vocabulary of the code-call grammar: the two surface styles, the
//! literal values a call can carry, and the call itself before binding.

/// Which programming language the catalogue is spelled in.
///
/// The parser is the same for both — a Python call and a TypeScript call are
/// read by one grammar — so this only decides what the model *reads*: the
/// signature syntax, the type words, and the protocol block's example.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum CodeStyle {
    /// `def read_file(path: str, limit: int = None) -> str`, called as
    /// `read_file(path="src/main.rs", limit=20)`.
    #[default]
    Python,
    /// `function read_file(path: string, limit?: number): string;`, called as
    /// `read_file({path: "src/main.rs", limit: 20})`.
    TypeScript,
}

impl CodeStyle {
    /// The name a config or a log line uses for this style.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            CodeStyle::Python => "python",
            CodeStyle::TypeScript => "typescript",
        }
    }
}

/// A literal argument value as the model wrote it, before it becomes JSON.
///
/// Deliberately a closed set: a bare identifier that is not one of the
/// boolean / null spellings is a variable reference, and a call that depends
/// on a variable cannot be executed — it is refused, never guessed at.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Literal {
    /// A quoted string, escapes already decoded.
    Str(String),
    /// An integer that fits `i64`.
    Int(i64),
    /// Any other number.
    Float(f64),
    /// `True` / `False` / `true` / `false`.
    Bool(bool),
    /// `None` / `null` / `undefined`.
    Null,
    /// `[…]` or a Python tuple `(…)`.
    List(Vec<Literal>),
    /// `{…}` with string or bare-identifier keys, in source order.
    Dict(Vec<(String, Literal)>),
}

/// One `name(args)` statement as parsed, before the registry binds it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Call {
    /// The callee, dots included (`functions.read_file`).
    pub(crate) name: String,
    /// Arguments given by position, in order.
    pub(crate) positional: Vec<Literal>,
    /// Arguments given by keyword, in order.
    pub(crate) keywords: Vec<(String, Literal)>,
}

/// The parser's one failure: the text is not a code call this grammar will
/// accept. It carries no detail on purpose — the caller falls through to the
/// next decoder, and the reason is logged where it is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Refuse;
