//! Provider-neutral request, response, and configuration types.

use std::{collections::BTreeMap, fmt, sync::Arc};
use tinytools::{Bm25Ranker, RankHit, ToolRanker};

/// One candidate presented to an evaluator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JevOption {
    /// Opaque candidate key.
    pub key: String,
    /// Concise description the evaluator judges.
    pub description: String,
}

/// A provider-neutral tool-selection request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JevRequest {
    /// The user's request.
    pub intent: String,
    /// Recent user turns, oldest first.
    pub recent_turns: Vec<String>,
    /// Candidate tools, including `none`.
    pub options: Vec<JevOption>,
    /// Model identifier configured by the host.
    pub model: String,
    /// What the options are and how to choose among them, when the ranker
    /// asks something other than "which tool accomplishes the request" — the
    /// family stage of [`JevStrategy::FamilyThenDecide`] asks which *group*
    /// of tools applies. `None` is the evaluator's default tool wording.
    pub instructions: Option<String>,
}

/// How [`JevRanker`][crate::JevRanker] narrows a catalogue before deciding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JevStrategy {
    /// The retriever shortlists `retrieval_k` candidates, one evaluation
    /// decides. Bounded by the retriever's recall — a paraphrase it cannot
    /// bridge never reaches the evaluator.
    #[default]
    RetrieveThenDecide,
    /// The evaluator first picks the candidates' *family* (a toolkit, a
    /// pack), then decides among every member of the top families, one
    /// evaluation per family. No retrieval for a family that fits one
    /// choice, so a paraphrase is only ever judged semantically; a larger
    /// family is cut to [`JevRankerConfig::MAX_CANDIDATES`] by the retriever.
    /// Candidates without a family form one `core` family.
    FamilyThenDecide,
}

/// The decision returned by a host-provided evaluator.
#[derive(Clone, Debug, PartialEq)]
pub struct JevDecision {
    /// Probability for each option key.
    pub probabilities: BTreeMap<String, f64>,
    /// Confidence in the overall choice.
    pub choice_confidence: f64,
    /// Probability that the request needs a tool.
    pub needs_tool: Option<f64>,
    /// Input tokens billed, when reported.
    pub input_tokens: Option<u64>,
    /// Attempts made by the host client.
    pub attempts: u32,
}

/// Evaluates a request without coupling this workspace to a transport.
#[async_trait::async_trait]
pub trait JevEvaluator: Send + Sync + fmt::Debug {
    /// Evaluates one tool-selection request.
    ///
    /// # Errors
    /// Returns a ranking error for invalid requests, timeouts, or provider failures.
    async fn evaluate(&self, request: &JevRequest) -> Result<JevDecision, tinytools::RankError>;
}

/// How [`JevRanker`][crate::JevRanker] retrieves and decides.
#[derive(Clone)]
pub struct JevRankerConfig {
    pub(crate) strategy: JevStrategy,
    pub(crate) max_families: usize,
    pub(crate) retriever: Arc<dyn ToolRanker>,
    pub(crate) retrieval_k: usize,
    pub(crate) min_probability: f64,
    pub(crate) model: String,
}

impl JevRankerConfig {
    /// Maximum options accepted, including `none`.
    pub const MAX_OPTIONS: usize = 255;
    /// Maximum real candidates, reserving one slot for `none`.
    pub const MAX_CANDIDATES: usize = Self::MAX_OPTIONS - 1;
    /// Returns the default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            strategy: JevStrategy::default(),
            max_families: 2,
            retriever: Arc::new(Bm25Ranker),
            retrieval_k: 20,
            min_probability: 0.05,
            model: "jev-latest".into(),
        }
    }
    /// Sets the narrowing strategy.
    #[must_use]
    pub fn with_strategy(mut self, value: JevStrategy) -> Self {
        self.strategy = value;
        self
    }
    /// The narrowing strategy in force.
    #[must_use]
    pub fn strategy(&self) -> JevStrategy {
        self.strategy
    }
    /// Sets how many top families the second stage decides among, clamped
    /// to `1..=8`.
    #[must_use]
    pub fn with_max_families(mut self, value: usize) -> Self {
        self.max_families = value.clamp(1, 8);
        self
    }
    /// Replaces the retriever.
    #[must_use]
    pub fn with_retriever(mut self, value: Arc<dyn ToolRanker>) -> Self {
        self.retriever = value;
        self
    }
    /// Sets the shortlist size, clamped to the valid candidate range.
    #[must_use]
    pub fn with_retrieval_k(mut self, value: usize) -> Self {
        self.retrieval_k = value.clamp(1, Self::MAX_CANDIDATES);
        self
    }
    /// Sets a finite probability floor, clamped to `0.0..=1.0`.
    #[must_use]
    pub fn with_min_probability(mut self, value: f64) -> Self {
        if value.is_finite() {
            self.min_probability = value.clamp(0.0, 1.0);
        }
        self
    }
    /// Sets the model id.
    #[must_use]
    pub fn with_model(mut self, value: impl Into<String>) -> Self {
        self.model = value.into();
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
            .field("strategy", &self.strategy)
            .field("max_families", &self.max_families)
            .field("retriever", &self.retriever.kind())
            .field("retrieval_k", &self.retrieval_k)
            .field("min_probability", &self.min_probability)
            .field("model", &self.model)
            .finish()
    }
}

/// Everything one ranking learned, beyond its hits.
#[derive(Clone, Debug, PartialEq)]
pub struct JevRanking {
    /// Ranked hits.
    pub hits: Vec<RankHit>,
    /// Confidence in the choice.
    pub choice_confidence: f64,
    /// Probability that the request needs a tool.
    pub needs_tool: Option<f64>,
    /// Probability assigned to `none`.
    pub none_probability: f64,
    /// Candidate count shown.
    pub shortlisted: usize,
    /// [`JevStrategy::FamilyThenDecide`] only: the families the first stage
    /// chose, best first, with their probabilities.
    pub families: Vec<(String, f64)>,
    /// Input tokens billed, when reported.
    pub input_tokens: Option<u64>,
    /// Evaluator wall time.
    pub latency: std::time::Duration,
    /// Host-client attempt count.
    pub attempts: u32,
}
