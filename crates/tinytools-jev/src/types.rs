//! Configuration and the detailed answer a Jev ranking produces.

use std::{fmt, sync::Arc, time::Duration};

use tinytools::{Bm25Ranker, RankHit, ToolRanker};

/// How [`JevRanker`][crate::JevRanker] retrieves and decides.
#[derive(Clone)]
pub struct JevRankerConfig {
    /// Ranks the full catalogue down to a shortlist before Jev sees it.
    /// [`Bm25Ranker`] by default; a host with an embedding index passes that.
    pub retriever: Arc<dyn ToolRanker>,
    /// How many candidates the retriever hands to Jev. Twenty is the
    /// documented sweet spot: small enough that one Choice question decides
    /// in ~150 ms, large enough that a lexical retriever's recall is not the
    /// bottleneck. Never above [`Self::MAX_OPTIONS`].
    pub retrieval_k: usize,
    /// Hits whose probability falls below this are dropped, so a caller
    /// never sees the long tail of a distribution as if it were a match.
    pub min_probability: f64,
    /// Deadline for the decision call, on top of the client's own per-attempt
    /// timeout and retries. A tool search sits in a model's turn; a slow
    /// answer is worse than a fallback.
    pub timeout: Duration,
    /// System One model id. `jev-latest` unless a host pins one.
    pub model: String,
}

impl JevRankerConfig {
    /// The most options one Jev Choice question accepts.
    pub const MAX_OPTIONS: usize = 255;

    /// Defaults: BM25 retrieval to 20, `min_probability` 0.05, 3 s deadline,
    /// `jev-latest`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            retriever: Arc::new(Bm25Ranker),
            retrieval_k: 20,
            min_probability: 0.05,
            timeout: Duration::from_secs(3),
            model: "jev-latest".to_owned(),
        }
    }

    /// Replaces the retriever.
    #[must_use]
    pub fn with_retriever(mut self, retriever: Arc<dyn ToolRanker>) -> Self {
        self.retriever = retriever;
        self
    }

    /// Sets the shortlist size, clamped to `1..=MAX_OPTIONS`.
    #[must_use]
    pub fn with_retrieval_k(mut self, k: usize) -> Self {
        self.retrieval_k = k.clamp(1, Self::MAX_OPTIONS);
        self
    }

    /// Sets the probability floor, clamped to `0.0..=1.0`.
    #[must_use]
    pub fn with_min_probability(mut self, p: f64) -> Self {
        self.min_probability = p.clamp(0.0, 1.0);
        self
    }

    /// Sets the decision deadline.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the model id.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

impl Default for JevRankerConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for JevRankerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JevRankerConfig")
            .field("retriever", &self.retriever.kind())
            .field("retrieval_k", &self.retrieval_k)
            .field("min_probability", &self.min_probability)
            .field("timeout", &self.timeout)
            .field("model", &self.model)
            .finish()
    }
}

/// Everything one Jev ranking learned, beyond the hits the trait returns.
///
/// A caller that only wants the hits uses [`tinytools::ToolRanker::rank`];
/// one that gates on "does this need a tool at all", or that reports cost,
/// calls [`JevRanker::rank_detailed`][crate::JevRanker::rank_detailed].
#[derive(Clone, Debug, PartialEq)]
pub struct JevRanking {
    /// Ranked hits, best first; each `confidence` is Jev's probability for
    /// that option.
    pub hits: Vec<RankHit>,
    /// Jev's confidence in the Choice as a whole, `0.0..=1.0`. Low when the
    /// distribution is flat — the signal to prefer asking over acting.
    pub choice_confidence: f64,
    /// Probability that the request needs a tool at all, from the `Noul`
    /// asked alongside. `None` when Jev did not answer it.
    pub needs_tool: Option<f64>,
    /// Probability Jev put on "none of these", the option every request
    /// carries so an off-catalogue intent is not forced onto a tool.
    pub none_probability: f64,
    /// How many candidates the retriever handed to Jev.
    pub shortlisted: usize,
    /// Input tokens billed, when the provider reports them.
    pub input_tokens: Option<u64>,
    /// Wall time of the decision call, including the client's retries.
    pub latency: Duration,
    /// Attempts the client made.
    pub attempts: u32,
}
