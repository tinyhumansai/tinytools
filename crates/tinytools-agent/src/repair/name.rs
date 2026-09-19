//! Resolving the tool name a model wrote to the tool it meant.
//!
//! Small models damage names in a handful of recurring ways, all observed on
//! live hosts: XML attributes leaked into the name (`terminal" parameter="command"`),
//! a namespace prefix the model saw in a chat template (`functions.read`,
//! `tools/read`), a stray suffix (`TodoTool_tool`), the wrong case or
//! separator (`Write File`, `write-file`), or one typo (`serach`).
//!
//! Resolution is conservative by construction. Nothing is rewritten unless the
//! result is a **unique** offered tool, and nothing is invented: with no
//! known-tool set the only change is dropping trailing junk after a quote or
//! angle bracket, which can never *create* a match. That keeps the property
//! the parser depends on — a plain JSON answer that happens to carry a `name`
//! is not turned into a call by fuzzy matching.

/// How a name was resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameResolution {
    /// The name to dispatch.
    pub name: String,
    /// Whether it differs from what the model wrote.
    pub repaired: bool,
    /// Whether it matches an offered tool. Always `false` when no tools were
    /// supplied.
    pub known: bool,
}

/// Namespace prefixes a chat template may have taught the model.
const NAMESPACE_PREFIXES: &[&str] = &["functions.", "functions/", "tools.", "tools/", "tool."];

/// Suffixes a model appends when it confuses the tool with its class name.
const TOOL_SUFFIXES: &[&str] = &["_tool", "-tool", "Tool", "tool"];

/// Maximum edit distance accepted for a fuzzy match.
const MAX_EDIT_DISTANCE: usize = 2;

/// Resolves `raw` against `known` tools.
///
/// The steps run in order and stop at the first that yields a known tool:
/// exact match, junk trimmed, namespace prefix dropped, separators and case
/// normalised, class suffix dropped, then a unique edit-distance match. A raw
/// name that resolves to nothing is returned trimmed of junk so the host's
/// unknown-tool policy sees something sensible.
#[must_use]
pub fn resolve(raw: &str, known: &[String]) -> NameResolution {
    let original = raw.trim();
    let trimmed = trim_junk(original);

    if known.is_empty() {
        return NameResolution {
            repaired: trimmed != original,
            name: trimmed.to_string(),
            known: false,
        };
    }

    let found = |candidate: &str| known.iter().any(|k| k == candidate);

    if found(original) {
        return resolved(original, original);
    }
    if found(trimmed) {
        return resolved(original, trimmed);
    }

    let unprefixed = strip_namespace(trimmed);
    if found(unprefixed) {
        return resolved(original, unprefixed);
    }

    let normalized = normalize(unprefixed);
    if let Some(hit) = known.iter().find(|k| normalize(k) == normalized) {
        return resolved(original, hit);
    }

    for suffix in TOOL_SUFFIXES {
        if let Some(stem) = unprefixed.strip_suffix(suffix) {
            let stem_norm = normalize(stem);
            if let Some(hit) = known.iter().find(|k| normalize(k) == stem_norm) {
                return resolved(original, hit);
            }
        }
    }

    if let Some(hit) = unique_fuzzy(&normalized, known) {
        return resolved(original, hit);
    }

    NameResolution {
        repaired: trimmed != original,
        name: trimmed.to_string(),
        known: false,
    }
}

fn resolved(original: &str, name: &str) -> NameResolution {
    NameResolution {
        name: name.to_string(),
        repaired: name != original,
        known: true,
    }
}

/// Cuts the name at the first character that cannot be part of one.
fn trim_junk(s: &str) -> &str {
    let end = s
        .find(|c: char| matches!(c, '"' | '\'' | '<' | '>' | '(' | ')' | '\n' | '\r' | ':' | '=' | '{' | '['))
        .unwrap_or(s.len());
    s[..end].trim()
}

fn strip_namespace(s: &str) -> &str {
    for prefix in NAMESPACE_PREFIXES {
        if let Some(rest) = s.strip_prefix(prefix)
            && !rest.is_empty()
        {
            return rest;
        }
    }
    s
}

/// Lower-case, `snake_case`, separators unified: `Write File` / `write-file` /
/// `WriteFile` all become `write_file`.
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut prev_lower = false;
    for ch in s.chars() {
        match ch {
            '-' | ' ' | '.' | '/' => {
                if !out.ends_with('_') {
                    out.push('_');
                }
                prev_lower = false;
            }
            c if c.is_uppercase() => {
                if prev_lower && !out.ends_with('_') {
                    out.push('_');
                }
                out.extend(c.to_lowercase());
                prev_lower = false;
            }
            c => {
                out.push(c);
                prev_lower = c.is_lowercase() || c.is_ascii_digit();
            }
        }
    }
    out.trim_matches('_').to_string()
}

/// The single known tool within [`MAX_EDIT_DISTANCE`] of `needle`, or `None`
/// when there are zero or several — an ambiguous match must not dispatch.
fn unique_fuzzy<'a>(needle: &str, known: &'a [String]) -> Option<&'a str> {
    if needle.len() < 4 {
        return None;
    }
    let budget = MAX_EDIT_DISTANCE.min(needle.len() / 3);
    if budget == 0 {
        return None;
    }
    let mut best: Option<(&str, usize)> = None;
    let mut ambiguous = false;
    for candidate in known {
        let distance = edit_distance(needle, &normalize(candidate));
        if distance > budget {
            continue;
        }
        match best {
            Some((_, d)) if d == distance => ambiguous = true,
            Some((_, d)) if d < distance => {}
            _ => {
                best = Some((candidate, distance));
                ambiguous = false;
            }
        }
    }
    if ambiguous { None } else { best.map(|(name, _)| name) }
}

/// Levenshtein distance over chars.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}
