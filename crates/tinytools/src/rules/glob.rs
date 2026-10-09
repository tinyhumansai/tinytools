//! The pattern language rules use: `*` and `?`, ASCII case-insensitive.
//!
//! Hand-rolled rather than pulled from a glob crate. Tool names are not paths,
//! so there is no separator for `*` to stop at and no character classes worth
//! their weight, and this crate's dependency list is reviewed in CI.

/// Whether `text` matches `pattern`.
///
/// `*` matches any run of characters (including none), `?` matches exactly
/// one, and everything else matches itself, ignoring ASCII case so
/// `gmail_*` matches the upper-case Composio slug `GMAIL_SEND_EMAIL`. An empty
/// pattern matches only an empty string.
#[must_use]
pub fn glob_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0usize, 0usize);
    // Where the last `*` was, and the text position it is currently absorbing
    // up to. Backtracking to it is the only backtracking needed: a later `*`
    // subsumes every choice an earlier one could make.
    let mut star: Option<(usize, usize)> = None;

    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, t));
                p += 1;
            }
            Some(&c) if c == '?' || c.eq_ignore_ascii_case(&text[t]) => {
                p += 1;
                t += 1;
            }
            _ => match star {
                Some((star_p, star_t)) => {
                    p = star_p + 1;
                    t = star_t + 1;
                    star = Some((star_p, star_t + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}
