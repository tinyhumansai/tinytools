//! Dependency-free Jev-backed tool ranking.
//!
//! A host supplies [`JevEvaluator`], retaining ownership of transport,
//! authentication, retry, and deadline policy.

#[cfg(test)]
mod test;
mod types;
pub use types::{JevDecision, JevEvaluator, JevOption, JevRankerConfig, JevRanking, JevRequest};

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
use tinytools::{RankCandidate, RankContext, RankError, RankHit, ToolRanker};
const NONE_OPTION: &str = "none";
const MAX_SUMMARY_CHARS: usize = 240;

/// Ranks tools using a host-provided evaluator.
#[derive(Debug, Clone)]
pub struct JevRanker {
    evaluator: Arc<dyn JevEvaluator>,
    config: JevRankerConfig,
}
impl JevRanker {
    /// Stable ranker kind.
    pub const KIND: &'static str = "jev";
    /// Creates a ranker.
    #[must_use]
    pub fn new(evaluator: Arc<dyn JevEvaluator>, config: JevRankerConfig) -> Self {
        Self { evaluator, config }
    }
    /// Returns the active configuration.
    #[must_use]
    pub fn config(&self) -> &JevRankerConfig {
        &self.config
    }
    /// Ranks and returns all decision metadata.
    ///
    /// # Errors
    /// Returns invalid-input errors and forwards evaluator/retriever failures.
    pub async fn rank_detailed(
        &self,
        intent: &str,
        context: &RankContext,
        candidates: &[RankCandidate],
        limit: usize,
    ) -> Result<JevRanking, RankError> {
        let intent = intent.trim();
        if intent.is_empty() {
            return Err(RankError::invalid_input("intent is empty"));
        }
        if candidates.is_empty() || limit == 0 {
            return Ok(JevRanking::empty());
        }
        let shortlist = self.shortlist(intent, context, candidates).await?;
        if shortlist.is_empty() {
            return Ok(JevRanking::empty());
        }
        let request = self.build_request(intent, context, &shortlist)?;
        let started = std::time::Instant::now();
        let decision = self.evaluator.evaluate(&request).await?;
        let mut ranking = decode(decision, &shortlist, self.config.min_probability);
        ranking.hits.truncate(limit);
        ranking.latency = started.elapsed();
        Ok(ranking)
    }
    async fn shortlist<'a>(
        &self,
        intent: &str,
        context: &RankContext,
        candidates: &'a [RankCandidate],
    ) -> Result<Vec<&'a RankCandidate>, RankError> {
        validate_candidates(candidates)?;
        let k = self.config.retrieval_k.min(JevRankerConfig::MAX_CANDIDATES);
        if candidates.len() <= k {
            return Ok(candidates.iter().collect());
        }
        let hits = self
            .config
            .retriever
            .rank(intent, context, candidates, k)
            .await?;
        let by_key: BTreeMap<&str, &RankCandidate> =
            candidates.iter().map(|c| (c.key.as_str(), c)).collect();
        let shortlist: Vec<_> = hits
            .iter()
            .filter_map(|h| by_key.get(h.key.as_str()).copied())
            .take(JevRankerConfig::MAX_CANDIDATES)
            .collect();
        if shortlist.is_empty() && candidates.len() <= JevRankerConfig::MAX_CANDIDATES {
            return Ok(candidates.iter().collect());
        }
        Ok(shortlist)
    }
    fn build_request(
        &self,
        intent: &str,
        context: &RankContext,
        shortlist: &[&RankCandidate],
    ) -> Result<JevRequest, RankError> {
        if shortlist.len() > JevRankerConfig::MAX_CANDIDATES {
            return Err(RankError::invalid_input("too many shortlisted candidates"));
        }
        let mut options: Vec<_> = shortlist
            .iter()
            .map(|c| JevOption {
                key: c.key.clone(),
                description: option_text(c),
            })
            .collect();
        options.push(JevOption {
            key: NONE_OPTION.into(),
            description: "No listed tool accomplishes the request.".into(),
        });
        Ok(JevRequest {
            intent: intent.into(),
            recent_turns: context.recent_turns.clone(),
            options,
            model: self.config.model.clone(),
        })
    }
}
#[async_trait::async_trait]
impl ToolRanker for JevRanker {
    fn kind(&self) -> &'static str {
        Self::KIND
    }
    async fn rank(
        &self,
        intent: &str,
        context: &RankContext,
        candidates: &[RankCandidate],
        limit: usize,
    ) -> Result<Vec<RankHit>, RankError> {
        self.rank_detailed(intent, context, candidates, limit)
            .await
            .map(|r| r.hits)
    }
}
impl JevRanking {
    fn empty() -> Self {
        Self {
            hits: vec![],
            choice_confidence: 0.0,
            needs_tool: None,
            none_probability: 0.0,
            shortlisted: 0,
            input_tokens: None,
            latency: Duration::ZERO,
            attempts: 0,
        }
    }
}
fn validate_candidates(candidates: &[RankCandidate]) -> Result<(), RankError> {
    let mut keys = BTreeSet::new();
    for c in candidates {
        if c.key == NONE_OPTION {
            return Err(RankError::invalid_input("candidate key `none` is reserved"));
        }
        if !keys.insert(c.key.as_str()) {
            return Err(RankError::invalid_input("duplicate candidate key"));
        }
    }
    Ok(())
}
fn option_text(candidate: &RankCandidate) -> String {
    let mut summary: String = candidate.summary.chars().take(MAX_SUMMARY_CHARS).collect();
    if candidate.summary.chars().count() > MAX_SUMMARY_CHARS {
        summary.push('…');
    }
    candidate.family.as_ref().map_or(summary.clone(), |family| {
        format!("{summary} (from {family})")
    })
}
fn decode(decision: JevDecision, shortlist: &[&RankCandidate], floor: f64) -> JevRanking {
    let none = decision
        .probabilities
        .get(NONE_OPTION)
        .copied()
        .unwrap_or(0.0);
    let best = shortlist
        .iter()
        .filter_map(|c| decision.probabilities.get(&c.key).copied())
        .fold(0.0_f64, f64::max);
    let abstained = none >= best || decision.needs_tool.is_some_and(|p| p < 0.5);
    let mut hits: Vec<_> = if abstained {
        vec![]
    } else {
        shortlist
            .iter()
            .filter_map(|c| {
                let p = *decision.probabilities.get(&c.key)?;
                (p >= floor).then(|| RankHit {
                    key: c.key.clone(),
                    score: p,
                    confidence: Some(p),
                })
            })
            .collect()
    };
    hits.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.key.cmp(&b.key)));
    JevRanking {
        hits,
        choice_confidence: decision.choice_confidence,
        needs_tool: decision.needs_tool,
        none_probability: none,
        shortlisted: shortlist.len(),
        input_tokens: decision.input_tokens,
        latency: Duration::ZERO,
        attempts: decision.attempts,
    }
}
