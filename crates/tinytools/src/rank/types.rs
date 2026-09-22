//! The vocabulary of tool ranking: what a ranker is given and what it returns.

use std::fmt;

/// One tool as a ranker sees it: an opaque key the caller maps back to the
/// tool, an optional family (the pack, toolkit, or server it belongs to), and
/// a short summary — the text a ranker reads.
///
/// The summary is what the ranker judges *on*. Callers should keep it to the
/// tool's name, its first sentence or two, and its argument names: a decision
/// model degrades as its input fills with content unrelated to the choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankCandidate {
    /// Caller-owned identifier, returned verbatim in [`RankHit::key`]. Must be
    /// unique within one `rank` call.
    pub key: String,
    /// The group this tool belongs to, when the caller has one — a toolpack,
    /// a connector toolkit, an MCP server. Used as ranking text and echoed
    /// back so a caller can render it; a ranker never requires it.
    pub family: Option<String>,
    /// Name plus a one-line description plus argument names: the searchable
    /// text.
    pub summary: String,
}

impl RankCandidate {
    /// A candidate with no family.
    #[must_use]
    pub fn new(key: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            family: None,
            summary: summary.into(),
        }
    }

    /// Sets the family.
    #[must_use]
    pub fn with_family(mut self, family: impl Into<String>) -> Self {
        self.family = Some(family.into());
        self
    }
}

/// One ranked tool.
#[derive(Clone, Debug, PartialEq)]
pub struct RankHit {
    /// The [`RankCandidate::key`] this hit refers to.
    pub key: String,
    /// Ranker-specific relevance, higher is better. Comparable only within one
    /// `rank` call and one ranker: BM25 scores and calibrated probabilities do
    /// not share a scale.
    pub score: f64,
    /// A calibrated probability that this is the right tool, in `0.0..=1.0`,
    /// when the ranker produces one. Lexical rankers return `None`; a caller
    /// gating on confidence treats `None` as "unknown", not as zero.
    pub confidence: Option<f64>,
}

impl RankHit {
    /// A hit with no confidence estimate.
    #[must_use]
    pub fn new(key: impl Into<String>, score: f64) -> Self {
        Self {
            key: key.into(),
            score,
            confidence: None,
        }
    }
}

/// Context a ranker may use beyond the intent itself.
///
/// Deliberately small. Rankers that consult a decision model pay for every
/// byte of this on every search, and unrelated context is a distractor, not
/// a help. The intent is always passed separately and is always what the
/// candidates are judged against.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RankContext {
    /// The most recent user turns, oldest first, each already clipped by the
    /// caller. Lets a ranker resolve "do it again for Bob" against the turn
    /// before. Empty when the caller has nothing worth adding.
    pub recent_turns: Vec<String>,
}

impl RankContext {
    /// A context with nothing beyond the intent.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }
}

/// Why a ranker could not rank.
///
/// A caller treats every variant the same way — fall back to a cheaper ranker
/// or to the candidates' declared order — so the variants exist for logs, not
/// for control flow.
#[derive(Debug)]
#[non_exhaustive]
pub enum RankError {
    /// The ranker's backing service refused or failed the request. The
    /// message never carries a credential.
    Backend {
        /// Stable, log-safe description.
        reason: String,
    },
    /// The request could not be built: too many candidates for the ranker,
    /// an empty intent, a duplicate key.
    InvalidInput {
        /// Stable description of the rejected input.
        reason: String,
    },
    /// The ranker did not answer within its deadline.
    Timeout,
}

impl RankError {
    /// Creates a backend failure with a log-safe description.
    #[must_use]
    pub fn backend(reason: impl Into<String>) -> Self {
        Self::Backend {
            reason: reason.into(),
        }
    }

    /// Creates an invalid-input failure.
    #[must_use]
    pub fn invalid_input(reason: impl Into<String>) -> Self {
        Self::InvalidInput {
            reason: reason.into(),
        }
    }
}

impl fmt::Display for RankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend { reason } => write!(f, "ranker backend failed: {reason}"),
            Self::InvalidInput { reason } => write!(f, "invalid ranking input: {reason}"),
            Self::Timeout => f.write_str("ranker timed out"),
        }
    }
}

impl std::error::Error for RankError {}
