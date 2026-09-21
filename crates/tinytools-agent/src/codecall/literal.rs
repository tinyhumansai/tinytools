//! The character-level scanner: whitespace and comments, identifiers, and
//! the literal values an argument can be.
//!
//! Nothing here knows about tools or registries. It reads Python and
//! JavaScript/TypeScript literals with one grammar — the two languages agree
//! on everything a tool argument needs except the spelling of booleans and
//! null, and both spellings are accepted everywhere.

use serde_json::{Map, Number, Value};

use super::types::{Literal, Refuse};

/// A position in the source text, with the small vocabulary of lookahead
/// the grammar needs.
#[derive(Debug)]
pub(crate) struct Cursor<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    /// A cursor at the start of `src`.
    pub(crate) fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    /// Whether every character has been consumed.
    pub(crate) fn at_end(&self) -> bool {
        self.pos >= self.src.len()
    }

    /// The unread remainder.
    pub(crate) fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    /// The next character without consuming it.
    pub(crate) fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    /// Consumes and returns the next character.
    pub(crate) fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    /// Consumes `literal` if the remainder starts with it.
    pub(crate) fn eat(&mut self, literal: &str) -> bool {
        if self.rest().starts_with(literal) {
            self.pos += literal.len();
            true
        } else {
            false
        }
    }

    /// Skips spaces, tabs, and comments — but **not** newlines, which separate
    /// statements at the top level. A comment runs to the end of its line
    /// (`#`, `//`) or to its closer (`/* … */`); the newline that ends a line
    /// comment is left for the caller.
    pub(crate) fn skip_inline_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(' ' | '\t' | '\r') => {
                    self.bump();
                }
                Some('#') => self.skip_line_comment(),
                Some('/') if self.rest().starts_with("//") => self.skip_line_comment(),
                Some('/') if self.rest().starts_with("/*") => {
                    self.pos += 2;
                    match self.rest().find("*/") {
                        Some(end) => self.pos += end + 2,
                        None => self.pos = self.src.len(),
                    }
                }
                _ => return,
            }
        }
    }

    /// Skips everything [`Self::skip_inline_trivia`] does plus newlines and
    /// stray `;` — the trivia *between* statements and inside brackets.
    pub(crate) fn skip_trivia(&mut self) {
        loop {
            self.skip_inline_trivia();
            match self.peek() {
                Some('\n' | ';') => {
                    self.bump();
                }
                _ => return,
            }
        }
    }

    fn skip_line_comment(&mut self) {
        match self.rest().find('\n') {
            Some(end) => self.pos += end,
            None => self.pos = self.src.len(),
        }
    }

    /// Reads an identifier (`[A-Za-z_$][A-Za-z0-9_$]*`), or `None` when the
    /// remainder does not start with one.
    pub(crate) fn identifier(&mut self) -> Option<&'a str> {
        let rest = self.rest();
        let mut end = 0;
        for (idx, c) in rest.char_indices() {
            let ok = if idx == 0 {
                c.is_alphabetic() || c == '_' || c == '$'
            } else {
                c.is_alphanumeric() || c == '_' || c == '$'
            };
            if !ok {
                break;
            }
            end = idx + c.len_utf8();
        }
        if end == 0 {
            return None;
        }
        self.pos += end;
        Some(&rest[..end])
    }

    /// Whether the remainder starts with a single `=` — an assignment or a
    /// keyword argument — as opposed to `==`.
    pub(crate) fn at_single_equals(&self) -> bool {
        self.rest().starts_with('=') && !self.rest().starts_with("==")
    }

    /// Reads one literal value: a string, number, boolean, null, list,
    /// tuple, or dict. Anything else — a bare identifier, an expression — is
    /// refused.
    ///
    /// # Errors
    ///
    /// [`Refuse`] when no literal starts here or the one that does is
    /// unterminated.
    pub(crate) fn literal(&mut self) -> Result<Literal, Refuse> {
        self.skip_trivia();
        match self.peek().ok_or(Refuse)? {
            '"' | '\'' | '`' => self.string(false).map(Literal::Str),
            '[' => self.sequence('[', ']').map(Literal::List),
            '(' => self.sequence('(', ')').map(Literal::List),
            '{' => self.dict().map(Literal::Dict),
            c if c == '-' || c == '+' || c.is_ascii_digit() => self.number(),
            c if c.is_alphabetic() || c == '_' => self.word(),
            _ => Err(Refuse),
        }
    }

    /// A bare word in literal position: a boolean / null spelling, or a
    /// string prefix (`r"…"`, `b"…"`, `f"…"`) — never a variable.
    fn word(&mut self) -> Result<Literal, Refuse> {
        let start = self.pos;
        let word = self.identifier().ok_or(Refuse)?;
        match word {
            "True" | "true" => Ok(Literal::Bool(true)),
            "False" | "false" => Ok(Literal::Bool(false)),
            "None" | "null" | "undefined" => Ok(Literal::Null),
            "r" | "R" | "b" | "f" | "u" | "rb" | "br" | "fr" | "rf"
                if matches!(self.peek(), Some('"' | '\'')) =>
            {
                let raw = word.contains(['r', 'R']);
                self.string(raw).map(Literal::Str)
            }
            _ => {
                self.pos = start;
                Err(Refuse)
            }
        }
    }

    /// A quoted string: `"…"`, `'…'`, a backtick template (no interpolation),
    /// or a triple-quoted `"""…"""` / `'''…'''`. Escapes are decoded unless
    /// `raw`.
    fn string(&mut self, raw: bool) -> Result<String, Refuse> {
        let quote = self.peek().ok_or(Refuse)?;
        let triple = quote != '`' && self.rest().starts_with(&quote.to_string().repeat(3));
        if triple {
            self.pos += 3;
        } else {
            self.bump();
        }
        let mut out = String::new();
        loop {
            if triple {
                if self.rest().starts_with(&quote.to_string().repeat(3)) {
                    self.pos += 3;
                    return Ok(out);
                }
            } else if self.peek() == Some(quote) {
                self.bump();
                return Ok(out);
            }
            let c = self.bump().ok_or(Refuse)?;
            if c == '\\' && !raw {
                self.escape(&mut out)?;
            } else {
                out.push(c);
            }
        }
    }

    /// Decodes the escape after a backslash. Unknown escapes keep the
    /// backslash so a Windows path or a regex survives verbatim.
    fn escape(&mut self, out: &mut String) -> Result<(), Refuse> {
        let c = self.bump().ok_or(Refuse)?;
        match c {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            '0' => out.push('\0'),
            '\\' | '\'' | '"' | '`' | '/' => out.push(c),
            '\n' => {}
            'x' => {
                let hex = self.rest().get(..2).ok_or(Refuse)?;
                if let Some(decoded) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) {
                    self.pos += 2;
                    out.push(decoded);
                } else {
                    out.push('\\');
                    out.push('x');
                }
            }
            'u' => {
                let (hex, consumed) = if self.rest().starts_with('{') {
                    let end = self.rest().find('}').ok_or(Refuse)?;
                    (&self.rest()[1..end], end + 1)
                } else {
                    (self.rest().get(..4).ok_or(Refuse)?, 4)
                };
                if let Some(decoded) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) {
                    self.pos += consumed;
                    out.push(decoded);
                } else {
                    out.push('\\');
                    out.push('u');
                }
            }
            other => {
                out.push('\\');
                out.push(other);
            }
        }
        Ok(())
    }

    /// An integer or float in either language's spelling, with an optional
    /// sign. Integers that overflow `i64` become floats.
    fn number(&mut self) -> Result<Literal, Refuse> {
        let rest = self.rest();
        let mut end = 0;
        let mut is_float = false;
        for (idx, c) in rest.char_indices() {
            let ok = match c {
                '0'..='9' | '_' => true,
                '+' | '-' => idx == 0 || matches!(rest[..idx].chars().last(), Some('e' | 'E')),
                '.' | 'e' | 'E' => {
                    is_float = true;
                    true
                }
                _ => false,
            };
            if !ok {
                break;
            }
            end = idx + c.len_utf8();
        }
        let text: String = rest[..end].chars().filter(|c| *c != '_').collect();
        if text.is_empty() || text == "-" || text == "+" {
            return Err(Refuse);
        }
        self.pos += end;
        if !is_float && let Ok(n) = text.parse::<i64>() {
            return Ok(Literal::Int(n));
        }
        text.parse::<f64>().map(Literal::Float).map_err(|_| Refuse)
    }

    /// A bracketed, comma-separated sequence with an optional trailing comma.
    fn sequence(&mut self, open: char, close: char) -> Result<Vec<Literal>, Refuse> {
        if self.bump() != Some(open) {
            return Err(Refuse);
        }
        let mut items = Vec::new();
        loop {
            self.skip_trivia();
            if self.peek() == Some(close) {
                self.bump();
                return Ok(items);
            }
            items.push(self.literal()?);
            self.skip_trivia();
            match self.bump() {
                Some(',') => {}
                Some(c) if c == close => return Ok(items),
                _ => return Err(Refuse),
            }
        }
    }

    /// `{key: value, …}` with quoted-string or bare-identifier keys.
    fn dict(&mut self) -> Result<Vec<(String, Literal)>, Refuse> {
        if self.bump() != Some('{') {
            return Err(Refuse);
        }
        let mut entries = Vec::new();
        loop {
            self.skip_trivia();
            if self.peek() == Some('}') {
                self.bump();
                return Ok(entries);
            }
            let key = match self.peek() {
                Some('"' | '\'') => self.string(false)?,
                _ => self.identifier().ok_or(Refuse)?.to_string(),
            };
            self.skip_trivia();
            if self.bump() != Some(':') {
                return Err(Refuse);
            }
            let value = self.literal()?;
            entries.push((key, value));
            self.skip_trivia();
            match self.bump() {
                Some(',') => {}
                Some('}') => return Ok(entries),
                _ => return Err(Refuse),
            }
        }
    }
}

impl From<Literal> for Value {
    fn from(literal: Literal) -> Self {
        match literal {
            Literal::Str(s) => Value::String(s),
            Literal::Int(n) => Value::Number(n.into()),
            Literal::Float(f) => Number::from_f64(f).map_or(Value::Null, Value::Number),
            Literal::Bool(b) => Value::Bool(b),
            Literal::Null => Value::Null,
            Literal::List(items) => Value::Array(items.into_iter().map(Value::from).collect()),
            Literal::Dict(entries) => {
                let mut map = Map::with_capacity(entries.len());
                for (key, value) in entries {
                    map.insert(key, Value::from(value));
                }
                Value::Object(map)
            }
        }
    }
}
