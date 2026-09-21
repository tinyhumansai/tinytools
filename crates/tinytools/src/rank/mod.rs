//! Ranking a catalogue of tools against an intent.
//!
//! A host that registers more tools than a model should see on every request
//! advertises a few and lets the model *search* for the rest. The search is a
//! ranking problem — "which of these tools is the one for *send Alex a note
//! that I'm late*?" — and this module is the vocabulary for it: the
//! [`ToolRanker`] trait, the [`RankCandidate`] a ranker reads, the
//! [`RankHit`] it returns, and one implementation every host can use without
//! a network, [`Bm25Ranker`].
//!
//! # Why a trait
//!
//! Lexical ranking is free and answers most queries whose wording overlaps a
//! tool's description. It misses the paraphrase — "ping" for a tool described
//! as "send a message" — and it has no notion of confidence, so a host cannot
//! tell a strong match from the least-bad one. A decision model (`TypeSafe`'s
//! Jev, or an embedding index) answers both, at the cost of a network call the
//! vocabulary crate must not make. The trait lets a harness ask "rank these"
//! without knowing which kind is answering, and lets a host compose them:
//! retrieve a shortlist lexically, then let the model decide.
//!
//! # Contract
//!
//! - `rank` returns at most `limit` hits, best first, and only candidates it
//!   considers relevant: an empty result means "nothing here fits", never
//!   "I padded to `limit`".
//! - Every returned key names a candidate the caller passed. A ranker never
//!   invents one.
//! - A ranker that cannot answer returns [`RankError`]; the caller decides
//!   what to fall back to. It never panics on caller input.
//! - `rank` is `async` because the interesting implementations talk to a
//!   service; [`Bm25Ranker`] completes without yielding.

mod bm25;
#[cfg(test)]
mod test;
mod types;

pub use bm25::{Bm25Index, Bm25Ranker, tokenize};
pub use types::{RankCandidate, RankContext, RankError, RankHit};

/// Ranks tool candidates against an intent.
///
/// See the [module docs](self) for the contract every implementation keeps.
#[async_trait::async_trait]
pub trait ToolRanker: Send + Sync {
    /// A short stable name for logs and telemetry: `"bm25"`, `"jev"`.
    fn kind(&self) -> &'static str;

    /// Ranks `candidates` against `intent`, best first, at most `limit` hits.
    ///
    /// # Errors
    ///
    /// Returns [`RankError`] when the ranker cannot produce an answer. A
    /// caller should treat every variant as "fall back", and read the variant
    /// only to say why in a log line.
    async fn rank(
        &self,
        intent: &str,
        context: &RankContext,
        candidates: &[RankCandidate],
        limit: usize,
    ) -> Result<Vec<RankHit>, RankError>;
}

#[async_trait::async_trait]
impl<T: ToolRanker + ?Sized> ToolRanker for std::sync::Arc<T> {
    fn kind(&self) -> &'static str {
        (**self).kind()
    }

    async fn rank(
        &self,
        intent: &str,
        context: &RankContext,
        candidates: &[RankCandidate],
        limit: usize,
    ) -> Result<Vec<RankHit>, RankError> {
        (**self).rank(intent, context, candidates, limit).await
    }
}
