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

/// Answers the family stage and each family stage from a script keyed by
/// the request's instructions, so the two-stage flow is observable.
#[derive(Debug)]
struct FamilyEvaluator {
    seen: Mutex<Vec<JevRequest>>,
}
#[async_trait::async_trait]
impl JevEvaluator for FamilyEvaluator {
    async fn evaluate(&self, request: &JevRequest) -> Result<JevDecision, RankError> {
        if let Ok(mut seen) = self.seen.lock() {
            seen.push(request.clone());
        }
        let instructions = request.instructions.clone().unwrap_or_default();
        let probabilities: BTreeMap<String, f64> = if instructions.starts_with("Which group") {
            [
                ("slack", 0.7),
                ("gmail", 0.2),
                ("core", 0.05),
                ("none", 0.05),
            ]
        } else if instructions.contains("`slack`") {
            [
                ("SLACK_SEND_MESSAGE", 0.8),
                ("SLACK_LIST", 0.1),
                ("none", 0.1),
                ("_", 0.0),
            ]
        } else {
            [
                ("GMAIL_SEND_EMAIL", 0.6),
                ("GMAIL_FETCH", 0.3),
                ("none", 0.1),
                ("_", 0.0),
            ]
        }
        .into_iter()
        .filter(|(k, _)| *k != "_")
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        Ok(JevDecision {
            probabilities,
            choice_confidence: 0.7,
            needs_tool: Some(0.9),
            input_tokens: Some(100),
            attempts: 1,
        })
    }
}

fn family_catalogue() -> Vec<RankCandidate> {
    vec![
        RankCandidate::new("SLACK_SEND_MESSAGE", "Send a message").with_family("slack"),
        RankCandidate::new("SLACK_LIST", "List channels").with_family("slack"),
        RankCandidate::new("GMAIL_SEND_EMAIL", "Send an email").with_family("gmail"),
        RankCandidate::new("GMAIL_FETCH", "Fetch emails").with_family("gmail"),
        RankCandidate::new("file_read", "Read a file"),
    ]
}

#[tokio::test]
async fn family_then_decide_asks_the_family_first_then_each_chosen_family() {
    let evaluator = Arc::new(FamilyEvaluator {
        seen: Mutex::new(vec![]),
    });
    let ranker = JevRanker::new(
        evaluator.clone(),
        JevRankerConfig::new().with_strategy(JevStrategy::FamilyThenDecide),
    );
    let ranking = ranker
        .rank_detailed("ping alex", &RankContext::empty(), &family_catalogue(), 3)
        .await
        .ok();

    let seen: Vec<JevRequest> = evaluator
        .seen
        .lock()
        .map(|seen| seen.clone())
        .unwrap_or_default();
    assert_eq!(seen.len(), 3, "one family stage, then slack and gmail");
    assert_eq!(
        seen.first().map(|r| r.options.len()),
        Some(4),
        "slack, gmail, core, none"
    );
    assert!(seen.first().is_some_and(|r| {
        r.options
            .iter()
            .any(|o| o.key == "core" && o.description.contains("file read"))
    }));
    assert!(
        seen.get(1)
            .and_then(|r| r.instructions.as_deref())
            .is_some_and(|i| i.contains("`slack`"))
    );
    assert!(
        seen.get(2)
            .and_then(|r| r.instructions.as_deref())
            .is_some_and(|i| i.contains("`gmail`"))
    );

    assert_eq!(
        ranking.as_ref().map(|r| r.families.clone()),
        Some(vec![("slack".to_owned(), 0.7), ("gmail".to_owned(), 0.2)])
    );
    let keys: Option<Vec<&str>> = ranking
        .as_ref()
        .map(|r| r.hits.iter().map(|h| h.key.as_str()).collect());
    assert_eq!(
        keys,
        Some(vec!["SLACK_SEND_MESSAGE", "GMAIL_SEND_EMAIL", "SLACK_LIST"])
    );
    assert!(
        ranking
            .as_ref()
            .and_then(|r| r.hits.first())
            .is_some_and(|h| (h.score - 0.56).abs() < 1e-9)
    );
    assert_eq!(ranking.as_ref().map(|r| r.shortlisted), Some(4));
    assert_eq!(ranking.as_ref().and_then(|r| r.input_tokens), Some(200));
}

#[tokio::test]
async fn family_then_decide_with_one_family_skips_the_family_stage() {
    let evaluator = Arc::new(FamilyEvaluator {
        seen: Mutex::new(vec![]),
    });
    let ranker = JevRanker::new(
        evaluator.clone(),
        JevRankerConfig::new().with_strategy(JevStrategy::FamilyThenDecide),
    );
    let only_slack: Vec<RankCandidate> = family_catalogue()
        .into_iter()
        .filter(|c| c.family.as_deref() == Some("slack"))
        .collect();
    let ranking = ranker
        .rank_detailed("ping alex", &RankContext::empty(), &only_slack, 3)
        .await
        .ok();
    assert_eq!(evaluator.seen.lock().map_or(0, |s| s.len()), 1);
    assert_eq!(
        ranking.as_ref().map(|r| r.families.clone()),
        Some(vec![("slack".to_owned(), 1.0)])
    );
    assert_eq!(
        ranking
            .as_ref()
            .and_then(|r| r.hits.first())
            .map(|h| h.key.as_str()),
        Some("SLACK_SEND_MESSAGE")
    );
}
