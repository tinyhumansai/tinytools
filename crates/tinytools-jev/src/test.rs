//! Tests for provider-neutral Jev ranking.

use super::*;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Debug)]
struct FakeEvaluator {
    decision: JevDecision,
    seen: Mutex<Vec<JevRequest>>,
}
#[async_trait::async_trait]
impl JevEvaluator for FakeEvaluator {
    async fn evaluate(&self, request: &JevRequest) -> Result<JevDecision, RankError> {
        if let Ok(mut seen) = self.seen.lock() {
            seen.push(request.clone());
        }
        Ok(self.decision.clone())
    }
}

fn candidates() -> Vec<RankCandidate> {
    vec![
        RankCandidate::new("slack", "Send a Slack message").with_family("chat"),
        RankCandidate::new("gmail", "Send an email"),
    ]
}

fn ranker(
    probabilities: [(&str, f64); 3],
    needs_tool: Option<f64>,
) -> (JevRanker, Arc<FakeEvaluator>) {
    let evaluator = Arc::new(FakeEvaluator {
        decision: JevDecision {
            probabilities: probabilities
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect::<BTreeMap<_, _>>(),
            choice_confidence: 0.8,
            needs_tool,
            input_tokens: Some(10),
            attempts: 1,
        },
        seen: Mutex::new(vec![]),
    });
    (
        JevRanker::new(evaluator.clone(), JevRankerConfig::new()),
        evaluator,
    )
}

#[tokio::test]
async fn ranks_candidates_and_builds_none_option() {
    let (ranker, evaluator) = ranker([("slack", 0.8), ("gmail", 0.1), ("none", 0.1)], Some(0.9));
    let result = ranker
        .rank_detailed("message Alex", &RankContext::empty(), &candidates(), 2)
        .await;
    assert_eq!(
        result
            .as_ref()
            .ok()
            .and_then(|ranking| ranking.hits.first())
            .map(|hit| hit.key.as_str()),
        Some("slack")
    );
    let options = evaluator
        .seen
        .lock()
        .ok()
        .and_then(|seen| seen.first().map(|request| request.options.len()));
    assert_eq!(options, Some(3));
}

#[tokio::test]
async fn suppresses_hits_when_none_wins_or_no_tool_is_needed() {
    let (none_ranker, _) = ranker([("slack", 0.2), ("gmail", 0.1), ("none", 0.7)], Some(0.9));
    let none_hits = none_ranker
        .rank("question", &RankContext::empty(), &candidates(), 2)
        .await
        .unwrap_or_default();
    assert!(none_hits.is_empty());
    let (no_tool_ranker, _) = ranker([("slack", 0.8), ("gmail", 0.1), ("none", 0.1)], Some(0.2));
    let no_tool_hits = no_tool_ranker
        .rank("question", &RankContext::empty(), &candidates(), 2)
        .await
        .unwrap_or_default();
    assert!(no_tool_hits.is_empty());
}

#[test]
fn configuration_reserves_none_slot_and_rejects_nan() {
    let config = JevRankerConfig::new()
        .with_retrieval_k(usize::MAX)
        .with_min_probability(f64::NAN);
    assert_eq!(config.retrieval_k, JevRankerConfig::MAX_CANDIDATES);
    assert!((config.min_probability - 0.05).abs() < f64::EPSILON);
}

#[test]
fn option_text_clips_by_characters() {
    let candidate = RankCandidate::new("key", "é".repeat(300)).with_family("family");
    let text = option_text(&candidate);
    assert_eq!(
        text.chars().filter(|character| *character == 'é').count(),
        MAX_SUMMARY_CHARS
    );
    assert!(text.ends_with("… (from family)"));
}

#[tokio::test]
async fn validates_inputs_and_short_circuits_empty_work() {
    let (ranker, _) = ranker([("slack", 0.8), ("gmail", 0.1), ("none", 0.1)], Some(0.9));
    let empty = ranker
        .rank("request", &RankContext::empty(), &[], 2)
        .await
        .unwrap_or_default();
    assert!(empty.is_empty());
    let zero = ranker
        .rank("request", &RankContext::empty(), &candidates(), 0)
        .await
        .unwrap_or_default();
    assert!(zero.is_empty());
    assert!(matches!(
        ranker
            .rank("  ", &RankContext::empty(), &candidates(), 2)
            .await,
        Err(RankError::InvalidInput { .. })
    ));
    let reserved = vec![RankCandidate::new("none", "reserved")];
    assert!(matches!(
        ranker
            .rank("request", &RankContext::empty(), &reserved, 2)
            .await,
        Err(RankError::InvalidInput { .. })
    ));
    let duplicate = vec![
        RankCandidate::new("x", "one"),
        RankCandidate::new("x", "two"),
    ];
    assert!(matches!(
        ranker
            .rank("request", &RankContext::empty(), &duplicate, 2)
            .await,
        Err(RankError::InvalidInput { .. })
    ));
}

#[tokio::test]
async fn retrieves_large_catalogues_and_handles_a_retrieval_miss() {
    let (_initial_ranker, evaluator) =
        ranker([("slack", 0.8), ("gmail", 0.1), ("none", 0.1)], Some(0.9));
    let configured_ranker = JevRanker::new(evaluator, JevRankerConfig::new().with_retrieval_k(1));
    let result = configured_ranker
        .rank("Slack message", &RankContext::empty(), &candidates(), 2)
        .await
        .unwrap_or_default();
    assert_eq!(result.first().map(|hit| hit.key.as_str()), Some("slack"));

    let (_unused_ranker, evaluator) =
        ranker([("slack", 0.8), ("gmail", 0.1), ("none", 0.1)], Some(0.9));
    let miss_ranker = JevRanker::new(
        evaluator.clone(),
        JevRankerConfig::new().with_retrieval_k(1),
    );
    let _ = miss_ranker
        .rank("unrelated", &RankContext::empty(), &candidates(), 2)
        .await;
    let shown = evaluator
        .seen
        .lock()
        .ok()
        .and_then(|seen| seen.first().map(|request| request.options.len()));
    assert_eq!(shown, Some(3));
}

#[derive(Debug)]
struct FailingEvaluator;
#[async_trait::async_trait]
impl JevEvaluator for FailingEvaluator {
    async fn evaluate(&self, _request: &JevRequest) -> Result<JevDecision, RankError> {
        Err(RankError::backend("provider unavailable"))
    }
}

#[tokio::test]
async fn forwards_evaluator_errors() {
    let ranker = JevRanker::new(Arc::new(FailingEvaluator), JevRankerConfig::default());
    let result = ranker
        .rank("message", &RankContext::empty(), &candidates(), 2)
        .await;
    assert!(matches!(result, Err(RankError::Backend { .. })));
    assert_eq!(ranker.kind(), JevRanker::KIND);
    assert!(format!("{:?}", ranker.config()).contains("bm25"));
}

#[test]
fn configuration_builders_are_observable() {
    let retriever: Arc<dyn ToolRanker> = Arc::new(tinytools::Bm25Ranker);
    let config = JevRankerConfig::new()
        .with_retriever(retriever)
        .with_retrieval_k(0)
        .with_min_probability(2.0)
        .with_model("jev-pinned");
    assert_eq!(config.retrieval_k, 1);
    assert!((config.min_probability - 1.0).abs() < f64::EPSILON);
    assert_eq!(config.model, "jev-pinned");
}
