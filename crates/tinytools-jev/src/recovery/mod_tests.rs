//! Conservative recovery contract tests with a transport-free fake evaluator.
// Tests deliberately assert successful fixture construction before behavior.
#![allow(clippy::unwrap_used)]
use super::*;
use crate::JevOption;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

fn observation() -> RecoveryObservation {
    RecoveryObservation::new(
        RecoveryPhase::Execution,
        "Search documents",
        "upstream temporarily unavailable",
    )
}
fn thresholds() -> RecoveryThresholds {
    RecoveryThresholds {
        version: "test-calibration-v1".into(),
        class_confidence: 0.7,
        recoverability: 0.6,
        advice_confidence: 0.7,
        repeated_blocker: 2,
    }
}
fn decision(request: &RecoveryRequest) -> RecoveryDecision {
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
                                f64::from(
                                    option.key == if id == "class" { "transient" } else { "none" },
                                ),
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
struct Fake {
    calls: AtomicUsize,
    result: Result<RecoveryDecision, ()>,
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
fn adviser(result: Result<RecoveryDecision, ()>) -> (RecoveryAdviser, Arc<Fake>) {
    let fake = Arc::new(Fake {
        calls: AtomicUsize::new(0),
        result,
    });
    (
        RecoveryAdviser::new(fake.clone(), thresholds()).unwrap(),
        fake,
    )
}
#[test]
fn builds_independent_recovery_questions_with_optional_rubric_and_none() {
    let mut input = observation();
    input.alternate_reason = Some(RecoveryAlternateReason::WrongToolAdvice);
    input.concrete_correction = Some("Inspect schema and use the evidenced required field".into());
    input.candidates = vec![JevOption {
        key: "authorized-search".into(),
        description: "Search documents with another service".into(),
    }];
    let request = RecoveryRequest::new(input).unwrap();
    assert_eq!(request.version, "tool-recovery-v1");
    assert_eq!(
        request
            .questions
            .iter()
            .map(RecoveryQuestion::id)
            .collect::<Vec<_>>(),
        ["class", "recoverability", "correction", "alternate"]
    );
    assert!(
        matches!(&request.questions[2], RecoveryQuestion::Score { rubric, .. } if rubric.len() == 3)
    );
    assert!(
        matches!(&request.questions[3], RecoveryQuestion::Choice { options, .. } if options.last().unwrap().key == "none")
    );
}
#[tokio::test]
async fn skips_success_hard_facts_and_uncertain_or_applied_writes() {
    let (adviser, fake) = adviser(Err(()));
    for fact in [
        RecoveryFact::Success,
        RecoveryFact::PolicyDenied,
        RecoveryFact::ApprovalDenied,
        RecoveryFact::ApprovalExpired,
        RecoveryFact::Cancelled,
        RecoveryFact::TerminalFault,
        RecoveryFact::Authentication,
        RecoveryFact::Permission,
    ] {
        let mut input = observation();
        input.facts.push(fact);
        assert_eq!(
            adviser.advise(input).await.unwrap(),
            RecoveryAdvice::Abstained(RecoveryAbstention::TrustedFact)
        );
    }
    for effect in [RecoveryEffect::UncertainWrite, RecoveryEffect::Applied] {
        let mut input = observation();
        input.effect = effect;
        assert_eq!(
            adviser.advise(input).await.unwrap(),
            RecoveryAdvice::Abstained(RecoveryAbstention::TrustedFact)
        );
    }
    assert_eq!(fake.calls.load(Ordering::Relaxed), 0);
}
#[tokio::test]
async fn validates_decision_and_preserves_provider_usage_without_execution() {
    let input = observation();
    let answer = decision(&RecoveryRequest::new(input.clone()).unwrap());
    let (adviser, fake) = adviser(Ok(answer.clone()));
    assert_eq!(
        adviser.advise(input).await.unwrap(),
        RecoveryAdvice::Classified {
            class: RecoveryClass::Transient,
            decision: answer,
            alternate: None,
            correction_score: None
        }
    );
    assert_eq!(fake.calls.load(Ordering::Relaxed), 1);
}
#[test]
fn rejects_oversized_unbounded_empty_and_reserved_candidate_inputs() {
    let mut input = observation();
    input.failure = "x".repeat(RecoveryObservation::MAX_TEXT_BYTES + 1);
    assert!(RecoveryRequest::new(input).is_err());
    for key in ["none", "", "  ", "\nsecret"] {
        let mut input = observation();
        input.candidates.push(JevOption {
            key: key.into(),
            description: "Search".into(),
        });
        assert!(RecoveryRequest::new(input).is_err());
    }
    let mut input = observation();
    input.candidates = vec![
        JevOption {
            key: "a".into(),
            description: "Search".into()
        };
        2
    ];
    assert!(RecoveryRequest::new(input).is_err());
    let mut input = observation();
    input.candidates = (0..17)
        .map(|n| JevOption {
            key: n.to_string(),
            description: "Search".into(),
        })
        .collect();
    assert!(RecoveryRequest::new(input).is_err());
    let mut input = observation();
    input.alternate_reason = Some(RecoveryAlternateReason::WrongToolAdvice);
    input.concrete_correction = Some(String::new());
    assert!(RecoveryRequest::new(input).is_err());
}
#[tokio::test]
async fn malformed_answers_abstain_instead_of_becoming_advice() {
    let input = observation();
    let request = RecoveryRequest::new(input.clone()).unwrap();
    let valid = decision(&request);
    let mut cases = vec![];
    let mut bad = valid.clone();
    bad.answers.remove("class");
    cases.push(bad);
    let mut bad = valid.clone();
    bad.answers
        .insert("unexpected".into(), RecoveryAnswer::Noul(1.0));
    cases.push(bad);
    let mut bad = valid.clone();
    bad.answers
        .insert("class".into(), RecoveryAnswer::Noul(1.0));
    cases.push(bad);
    for probability in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        let mut bad = valid.clone();
        bad.answers
            .insert("recoverability".into(), RecoveryAnswer::Noul(probability));
        cases.push(bad);
    }
    for confidence in [f64::NAN, -0.1, 1.1] {
        let mut bad = valid.clone();
        if let Some(RecoveryAnswer::Choice {
            confidence: value, ..
        }) = bad.answers.get_mut("class")
        {
            *value = confidence;
        }
        cases.push(bad);
    }
    for (key, value) in [
        ("invented", 0.0),
        ("transient", 0.5),
        ("transient", f64::NAN),
        ("transient", -0.1),
    ] {
        let mut bad = valid.clone();
        if let Some(RecoveryAnswer::Choice { probabilities, .. }) = bad.answers.get_mut("class") {
            probabilities.insert(key.into(), value);
        }
        cases.push(bad);
    }
    let mut bad = valid.clone();
    if let Some(RecoveryAnswer::Choice { probabilities, .. }) = bad.answers.get_mut("class") {
        probabilities.remove("unknown");
    }
    cases.push(bad);
    let mut bad = valid;
    bad.attempts = 0;
    cases.push(bad);
    for bad in cases {
        let (adviser, _) = adviser(Ok(bad));
        assert_eq!(
            adviser.advise(input.clone()).await.unwrap(),
            RecoveryAdvice::Abstained(RecoveryAbstention::InvalidDecision)
        );
    }
}
#[tokio::test]
async fn low_confidence_unknown_ties_and_provider_failure_abstain() {
    let input = observation();
    let request = RecoveryRequest::new(input.clone()).unwrap();
    let mut cases = vec![];
    let mut bad = decision(&request);
    if let Some(RecoveryAnswer::Choice { confidence, .. }) = bad.answers.get_mut("class") {
        *confidence = 0.1;
    }
    cases.push((bad, RecoveryAbstention::LowConfidence));
    let mut bad = decision(&request);
    if let Some(RecoveryAnswer::Choice { probabilities, .. }) = bad.answers.get_mut("class") {
        probabilities.insert("transient".into(), 0.0);
        probabilities.insert("unknown".into(), 1.0);
    }
    cases.push((bad, RecoveryAbstention::UnknownClass));
    let mut bad = decision(&request);
    if let Some(RecoveryAnswer::Choice { probabilities, .. }) = bad.answers.get_mut("class") {
        probabilities.insert("transient".into(), 0.5);
        probabilities.insert("wrong_tool".into(), 0.5);
    }
    cases.push((bad, RecoveryAbstention::UnknownClass));
    for (answer, reason) in cases {
        let (adviser, _) = adviser(Ok(answer));
        assert_eq!(
            adviser.advise(input.clone()).await.unwrap(),
            RecoveryAdvice::Abstained(reason)
        );
    }
    let (adviser, _) = adviser(Err(()));
    assert_eq!(
        adviser.advise(input).await.unwrap(),
        RecoveryAdvice::Abstained(RecoveryAbstention::EvaluatorFailure)
    );
}

fn choose(answer: &mut RecoveryDecision, id: &str, key: &str) {
    if let Some(RecoveryAnswer::Choice { probabilities, .. }) = answer.answers.get_mut(id) {
        for (option, value) in probabilities {
            *value = f64::from(option == key);
        }
    }
}
#[tokio::test]
async fn alternates_are_advice_only_and_none_is_valid() {
    let mut input = observation();
    input.alternate_reason = Some(RecoveryAlternateReason::WrongToolAdvice);
    input.candidates.push(JevOption {
        key: "permitted".into(),
        description: "Authorized alternate lookup".into(),
    });
    let request = RecoveryRequest::new(input.clone()).unwrap();
    let mut answer = decision(&request);
    choose(&mut answer, "class", "wrong_tool");
    choose(&mut answer, "alternate", "permitted");
    let (adviser, fake) = adviser(Ok(answer));
    assert!(
        matches!(adviser.advise(input.clone()).await.unwrap(), RecoveryAdvice::Classified { alternate: Some(key), .. } if key == "permitted")
    );
    assert_eq!(fake.calls.load(Ordering::Relaxed), 1);
    let (adviser, _) = self::adviser(Ok(decision(&request)));
    assert!(matches!(
        adviser.advise(input).await.unwrap(),
        RecoveryAdvice::Classified {
            alternate: None,
            ..
        }
    ));
}

#[path = "validation_tests.rs"]
mod validation;
