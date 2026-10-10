//! Shared transport-free fixtures for recovery contract and validation tests.
#![allow(clippy::unwrap_used)]
use super::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tinytools::RankError;
pub(super) fn observation() -> RecoveryObservation {
    RecoveryObservation::new(
        RecoveryPhase::Execution,
        "Search documents",
        "upstream temporarily unavailable",
    )
}
pub(super) fn thresholds() -> RecoveryThresholds {
    RecoveryThresholds {
        version: "test-calibration-v1".into(),
        class_confidence: 0.7,
        recoverability: 0.6,
        advice_confidence: 0.7,
        repeated_blocker: 2,
    }
}
pub(super) fn decision(request: &RecoveryRequest) -> RecoveryDecision {
    let answers = request
        .questions
        .iter()
        .map(|question| {
            let answer = match question {
                RecoveryQuestion::Choice { id, options, .. } => RecoveryAnswer::Choice {
                    probabilities: options
                        .iter()
                        .map(|option| {
                            (
                                option.key.clone(),
                                if option.key == if id == "class" { "transient" } else { "none" } {
                                    1.0
                                } else {
                                    0.0
                                },
                            )
                        })
                        .collect(),
                    confidence: 0.9,
                },
                RecoveryQuestion::Noul { .. } => RecoveryAnswer::Noul(0.8),
                RecoveryQuestion::Score { .. } => RecoveryAnswer::Score {
                    probabilities: vec![0.0, 0.0, 1.0],
                    confidence: 0.9,
                },
            };
            (question.id().to_owned(), answer)
        })
        .collect();
    RecoveryDecision {
        answers,
        input_tokens: Some(12),
        output_tokens: Some(4),
        latency: Duration::from_millis(25),
        attempts: 1,
    }
}
#[derive(Debug)]
pub(super) struct Fake {
    pub(super) calls: AtomicUsize,
    pub(super) result: Result<RecoveryDecision, ()>,
}
#[async_trait::async_trait]
impl RecoveryEvaluator for Fake {
    async fn evaluate(&self, _: &RecoveryRequest) -> Result<RecoveryDecision, RankError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.result
            .clone()
            .map_err(|()| RankError::invalid_input("fake failure"))
    }
}
pub(super) fn adviser(result: Result<RecoveryDecision, ()>) -> (RecoveryAdviser, Arc<Fake>) {
    let fake = Arc::new(Fake {
        calls: AtomicUsize::new(0),
        result,
    });
    (
        RecoveryAdviser::new(fake.clone(), thresholds()).unwrap(),
        fake,
    )
}
pub(super) fn choose(answer: &mut RecoveryDecision, id: &str, key: &str) {
    if let Some(RecoveryAnswer::Choice { probabilities, .. }) = answer.answers.get_mut(id) {
        for (option, value) in probabilities {
            *value = if option == key { 1.0 } else { 0.0 };
        }
    }
}
