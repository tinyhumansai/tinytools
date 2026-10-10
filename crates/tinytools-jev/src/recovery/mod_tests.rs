//! Conservative recovery contract tests with a transport-free fake evaluator.
// Tests deliberately assert successful fixture construction before behavior.
#![allow(clippy::unwrap_used)]
use super::test_support::*;
use super::*;
use crate::JevOption;
use std::sync::atomic::Ordering;

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
