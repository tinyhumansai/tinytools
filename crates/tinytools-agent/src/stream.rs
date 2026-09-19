//! Stripping tool-call markup from a text stream *as fragments arrive*.
//!
//! A terminal-response parse only cleans the aggregated answer. A consumer
//! rendering live text deltas would still watch `<tool_call>{"name":…` stream
//! through character by character. This scrubber closes that gap: it emits
//! only the text that is provably not part of a tool-call block, holding back
//! any tail that could still become one — a partial opener such as `<tool_ca`,
//! an opener whose block has not closed — until more input resolves it. When
//! a block completes mid-stream the calls it held are returned too, so a
//! consumer may dispatch without waiting for the stream to end.
//!
//! It shares every grammar with [`crate::parse`], so a format added there is
//! scrubbed here with no further work. Two paths are deliberately not
//! streamed, because they need the whole response to be safe: the bare-JSON
//! path and the GLM line grammar. The terminal parse still recovers those.
//!
//! Feed each fragment through [`StreamScrubber::feed`] and call
//! [`StreamScrubber::flush`] once when the stream ends to drain the remainder.
//! Only the visible-text channel should pass through here; reasoning and
//! structured tool-call channels are unaffected.

use std::sync::Arc;

use crate::PFormatRegistry;
use crate::parse::grammar::{ScanMode, all_openers, find_ci};
use crate::parse::{resolve_names, scan};
use crate::types::{ParseDiagnostic, ParseOptions, ParsedToolCall};

/// What one fragment released.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StreamStep {
    /// Text safe to show now.
    pub text: String,
    /// Calls whose blocks completed within the buffered text.
    pub calls: Vec<ParsedToolCall>,
    /// Diagnostics from blocks that completed.
    pub diagnostics: Vec<ParseDiagnostic>,
}

/// Stateful, grammar-aware scrubber for streamed visible text.
#[derive(Debug, Default)]
pub struct StreamScrubber {
    buf: String,
    known_tools: Vec<String>,
    registry: Option<Arc<PFormatRegistry>>,
}

impl StreamScrubber {
    /// An empty scrubber with no known tools and no registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the tools offered this turn, enabling name repair.
    #[must_use]
    pub fn with_known_tools(mut self, tools: Vec<String>) -> Self {
        self.known_tools = tools;
        self
    }

    /// Sets the P-Format registry.
    #[must_use]
    pub fn with_registry(mut self, registry: Arc<PFormatRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }

    fn options(&self) -> ParseOptions<'_> {
        let mut options = ParseOptions::new()
            .with_known_tools(&self.known_tools)
            .without_bare_json();
        if let Some(registry) = self.registry.as_deref() {
            options = options.with_registry(registry);
        }
        options
    }

    /// Feeds the next fragment and returns what is safe to release now.
    pub fn feed(&mut self, fragment: &str) -> StreamStep {
        self.buf.push_str(fragment);
        let options = self.options();
        let scan = scan(&self.buf, &options, ScanMode::Stream);

        let mut text = String::new();
        let consumed = match scan.pending {
            Some(start) => {
                for range in &scan.kept {
                    text.push_str(&self.buf[range.clone()]);
                }
                start
            }
            None => {
                let mut consumed = self.buf.len();
                let last = scan.kept.len().saturating_sub(1);
                for (index, range) in scan.kept.iter().enumerate() {
                    if index == last {
                        let tail = &self.buf[range.clone()];
                        let hold = hold_from(tail);
                        text.push_str(&tail[..hold]);
                        consumed = range.start + hold;
                    } else {
                        text.push_str(&self.buf[range.clone()]);
                    }
                }
                consumed
            }
        };
        let (calls, diagnostics) = resolve_names(scan.calls, scan.diagnostics, &options);
        self.buf.drain(..consumed);
        StreamStep {
            text,
            calls,
            diagnostics,
        }
    }

    /// Drains the remainder once no more fragments will arrive. A complete
    /// block still buffered yields its calls; a dangling opener is released
    /// verbatim — with the stream ended it was never a call.
    pub fn flush(&mut self) -> StreamStep {
        let options = self.options();
        let scan = scan(&self.buf, &options, ScanMode::Batch);
        let mut text = String::new();
        for range in &scan.kept {
            text.push_str(&self.buf[range.clone()]);
        }
        let (calls, diagnostics) = resolve_names(scan.calls, scan.diagnostics, &options);
        self.buf.clear();
        StreamStep {
            text,
            calls,
            diagnostics,
        }
    }
}

/// Byte index in `tail` from which the trailing bytes must be held because
/// they could still grow into a block opener. `tail.len()` when the whole
/// tail is safe.
///
/// Two shapes are held: a complete opener literal that no grammar accepted
/// yet (its attributes or `>` have not arrived), provided it is not merely
/// the prefix of a longer word (`<tool_calls>` is prose, not a held
/// `<tool_call`); and a trailing run that is a proper prefix of an opener.
fn hold_from(tail: &str) -> usize {
    let mut hold = tail.len();
    for opener in all_openers() {
        let mut cursor = 0;
        while let Some(idx) = find_ci(tail, opener, cursor) {
            let after = tail[idx + opener.len()..].chars().next();
            let word_continues = after.is_some_and(|c| c.is_ascii_alphanumeric());
            if !word_continues && idx < hold {
                hold = idx;
            }
            cursor = idx + opener.len();
        }
    }
    if hold < tail.len() {
        return hold;
    }
    let len = tail.len();
    let mut best = len;
    for opener in all_openers() {
        let max = (opener.len() - 1).min(len);
        for k in (1..=max).rev() {
            if opener.is_char_boundary(k)
                && tail.is_char_boundary(len - k)
                && tail.as_bytes()[len - k..].eq_ignore_ascii_case(&opener.as_bytes()[..k])
            {
                best = best.min(len - k);
                break;
            }
        }
    }
    best
}

#[cfg(test)]
mod test;
