//! Optional answer, contradiction, threshold and bounded-input contract tests.
use super::*;

#[tokio::test]
async fn contradictions_and_invalid_optional_answers_abstain() {
    let mut input = observation();
    input.alternate_reason = Some(RecoveryAlternateReason::WrongToolAdvice);
    input.concrete_correction = Some("Inspect schema".into());
    input.candidates.push(JevOption {
        key: "permitted".into(),
        description: "Authorized alternate lookup".into(),
    });
    let request = RecoveryRequest::new(input.clone()).unwrap();
    let valid = decision(&request);
    let mut cases = vec![];
    for class in ["authentication", "permission", "unsupported"] {
        let mut bad = valid.clone();
        choose(&mut bad, "class", class);
        cases.push(bad);
    }
    let mut bad = valid.clone();
    choose(&mut bad, "alternate", "permitted");
    cases.push(bad);
    let mut bad = valid.clone();
    bad.answers
        .insert("recoverability".into(), RecoveryAnswer::Noul(0.1));
    cases.push(bad);
    for probabilities in [
        vec![],
        vec![1.0],
        vec![0.0, 0.0, 0.5],
        vec![f64::NAN, 0.0, 1.0],
    ] {
        let mut bad = valid.clone();
        bad.answers.insert(
            "correction".into(),
            RecoveryAnswer::Score {
                probabilities,
                confidence: 0.9,
            },
        );
        cases.push(bad);
    }
    let mut bad = valid.clone();
    bad.answers.insert(
        "correction".into(),
        RecoveryAnswer::Score {
            probabilities: vec![0.0, 0.0, 1.0],
            confidence: f64::NAN,
        },
    );
    cases.push(bad);
    let mut bad = valid.clone();
    bad.answers
        .insert("alternate".into(), RecoveryAnswer::Noul(0.8));
    cases.push(bad);
    for bad in cases {
        let (adviser, _) = adviser(Ok(bad));
        assert_eq!(
            adviser.advise(input.clone()).await.unwrap(),
            RecoveryAdvice::Abstained(RecoveryAbstention::InvalidDecision)
        );
    }
    for id in ["alternate", "correction"] {
        let mut bad = valid.clone();
        if let RecoveryAnswer::Choice { confidence, .. }
        | RecoveryAnswer::Score { confidence, .. } = bad.answers.get_mut(id).unwrap()
        {
            *confidence = 0.1;
        }
        let (adviser, _) = adviser(Ok(bad));
        assert_eq!(
            adviser.advise(input.clone()).await.unwrap(),
            RecoveryAdvice::Abstained(RecoveryAbstention::LowConfidence)
        );
    }
}
#[tokio::test]
async fn external_prerequisites_can_be_classified_without_corrective_advice() {
    let input = observation();
    let request = RecoveryRequest::new(input.clone()).unwrap();
    let mut answer = decision(&request);
    choose(&mut answer, "class", "authentication");
    answer
        .answers
        .insert("recoverability".into(), RecoveryAnswer::Noul(0.0));
    let (adviser, _) = adviser(Ok(answer));
    assert!(matches!(
        adviser.advise(input).await.unwrap(),
        RecoveryAdvice::Classified {
            class: RecoveryClass::Authentication,
            alternate: None,
            correction_score: None,
            ..
        }
    ));
}
#[test]
fn recoverability_threshold_requires_a_finite_positive_probability() {
    for value in [
        0.0,
        -0.0,
        -0.1,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        1.1,
    ] {
        let mut policy = thresholds();
        policy.recoverability = value;
        let fake = Arc::new(Fake {
            calls: AtomicUsize::new(0),
            result: Err(()),
        });
        assert!(
            RecoveryAdviser::new(fake, policy).is_err(),
            "invalid recoverability threshold {value}"
        );
    }
    for value in [f64::MIN_POSITIVE, 1.0] {
        let mut policy = thresholds();
        policy.recoverability = value;
        let fake = Arc::new(Fake {
            calls: AtomicUsize::new(0),
            result: Err(()),
        });
        assert!(
            RecoveryAdviser::new(fake, policy).is_ok(),
            "valid recoverability threshold {value}"
        );
    }
}

#[test]
fn concentration_thresholds_can_include_zero_and_one() {
    for value in [0.0, 1.0] {
        let mut policy = thresholds();
        policy.class_confidence = value;
        policy.advice_confidence = value;
        let fake = Arc::new(Fake {
            calls: AtomicUsize::new(0),
            result: Err(()),
        });
        assert!(RecoveryAdviser::new(fake, policy).is_ok());
    }
}

#[test]
fn thresholds_and_observation_boundaries_are_explicit() {
    for invalid in [f64::NAN, -0.1, 1.1] {
        for field in 0..3 {
            let mut policy = thresholds();
            match field {
                0 => policy.class_confidence = invalid,
                1 => policy.recoverability = invalid,
                _ => policy.advice_confidence = invalid,
            }
            let fake = Arc::new(Fake {
                calls: AtomicUsize::new(0),
                result: Err(()),
            });
            assert!(RecoveryAdviser::new(fake, policy).is_err());
        }
    }
    for field in 0..2 {
        let mut policy = thresholds();
        if field == 0 {
            policy.version.clear();
        } else {
            policy.repeated_blocker = 0;
        }
        assert!(
            RecoveryAdviser::new(
                Arc::new(Fake {
                    calls: AtomicUsize::new(0),
                    result: Err(())
                }),
                policy
            )
            .is_err()
        );
    }
    for field in 0..5 {
        let mut input = observation();
        let large = "é".repeat(1025);
        match field {
            0 => input.tool_description = large,
            1 => input.argument_shape = large,
            2 => input.concrete_correction = Some(large),
            3 => input.candidates.push(JevOption {
                key: large,
                description: "Lookup".into(),
            }),
            _ => input.candidates.push(JevOption {
                key: "lookup".into(),
                description: large,
            }),
        }
        assert!(RecoveryRequest::new(input).is_err());
    }
    let mut input = observation();
    input.failure = "x".repeat(RecoveryObservation::MAX_TEXT_BYTES);
    assert!(RecoveryRequest::new(input).is_ok());
    let mut input = observation();
    input.facts = vec![RecoveryFact::Success; 9];
    assert!(RecoveryRequest::new(input).is_err());
    let mut input = observation();
    input.failure = "\0".into();
    assert!(RecoveryRequest::new(input).is_err());
}
#[tokio::test]
async fn candidates_alone_do_not_enable_alternate_questions() {
    let mut input = observation();
    input.candidates.push(JevOption {
        key: "permitted".into(),
        description: "Alternate lookup".into(),
    });
    assert_eq!(
        RecoveryRequest::new(input.clone()).unwrap().questions.len(),
        2
    );
    input.alternate_reason = Some(RecoveryAlternateReason::RepeatedBlocker);
    assert!(RecoveryRequest::new(input.clone()).is_err());
    input.repeated_failures = 1;
    let mut base = input.clone();
    base.alternate_reason = None;
    let (adviser, _) = adviser(Ok(decision(&RecoveryRequest::new(base).unwrap())));
    assert!(matches!(
        adviser.advise(input).await.unwrap(),
        RecoveryAdvice::Classified {
            alternate: None,
            ..
        }
    ));
}
#[tokio::test]
async fn counted_blocker_can_request_alternate_independent_of_class() {
    let mut input = observation();
    input.alternate_reason = Some(RecoveryAlternateReason::RepeatedBlocker);
    input.repeated_failures = 2;
    input.candidates.push(JevOption {
        key: "permitted".into(),
        description: "Alternate lookup".into(),
    });
    let request = RecoveryRequest::new(input.clone()).unwrap();
    let mut answer = decision(&request);
    choose(&mut answer, "alternate", "permitted");
    let (adviser, _) = adviser(Ok(answer));
    assert!(matches!(
        adviser.advise(input).await.unwrap(),
        RecoveryAdvice::Classified {
            class: RecoveryClass::Transient,
            alternate: Some(_),
            ..
        }
    ));
}

#[tokio::test]
async fn valid_concrete_score_is_an_ordered_expectation() {
    let mut input = observation();
    input.concrete_correction =
        Some("Inspect schema and supply the documented required field".into());
    let request = RecoveryRequest::new(input.clone()).unwrap();
    let (adviser, _) = adviser(Ok(decision(&request)));
    assert!(
        matches!(adviser.advise(input).await.unwrap(), RecoveryAdvice::Classified { correction_score: Some(score), .. } if (score - 2.0).abs() < f64::EPSILON)
    );
}
#[test]
fn duplicate_question_ids_do_not_validate() {
    let mut request = RecoveryRequest::new(observation()).unwrap();
    let mut answer = decision(&request);
    request.questions[1] = request.questions[0].clone();
    answer.answers.remove("recoverability");
    answer
        .answers
        .insert("unrequested".into(), RecoveryAnswer::Noul(0.8));
    assert!(answer.validate(&request).is_err());
}
#[test]
fn duplicate_choice_keys_cannot_admit_an_unrequested_answer_key() {
    let mut request = RecoveryRequest::new(observation()).unwrap();
    let mut answer = decision(&request);
    if let RecoveryQuestion::Choice { options, .. } = &mut request.questions[0] {
        options[1].key = options[0].key.clone();
    }
    if let RecoveryAnswer::Choice { probabilities, .. } = answer.answers.get_mut("class").unwrap() {
        let removed = probabilities.remove("rate_limited").unwrap();
        probabilities.insert("unrequested".into(), removed);
    }
    assert!(answer.validate(&request).is_err());
}
#[test]
fn empty_recovery_batches_do_not_validate() {
    let mut request = RecoveryRequest::new(observation()).unwrap();
    let mut answer = decision(&request);
    request.questions.clear();
    answer.answers.clear();
    assert!(answer.validate(&request).is_err());
}
#[test]
fn empty_choice_options_and_score_rubrics_do_not_validate() {
    let mut input = observation();
    input.concrete_correction = Some("Inspect schema".into());
    for id in ["class", "correction"] {
        let mut request = RecoveryRequest::new(input.clone()).unwrap();
        let mut answer = decision(&request);
        for question in &mut request.questions {
            if question.id() == id {
                match question {
                    RecoveryQuestion::Choice { options, .. } => options.clear(),
                    RecoveryQuestion::Score { rubric, .. } => rubric.clear(),
                    RecoveryQuestion::Noul { .. } => unreachable!(),
                }
            }
        }
        match answer.answers.get_mut(id).unwrap() {
            RecoveryAnswer::Choice { probabilities, .. } => probabilities.clear(),
            RecoveryAnswer::Score { probabilities, .. } => probabilities.clear(),
            RecoveryAnswer::Noul(_) => unreachable!(),
        }
        assert!(answer.validate(&request).is_err(), "empty {id} request");
    }
}
#[tokio::test]
async fn concrete_score_weights_all_rubric_positions() {
    let mut input = observation();
    input.concrete_correction = Some("Inspect schema".into());
    let request = RecoveryRequest::new(input.clone()).unwrap();
    let mut answer = decision(&request);
    answer.answers.insert(
        "correction".into(),
        RecoveryAnswer::Score {
            probabilities: vec![0.1, 0.3, 0.6],
            confidence: 0.9,
        },
    );
    let (adviser, _) = adviser(Ok(answer));
    assert!(matches!(
        adviser.advise(input).await.unwrap(),
        RecoveryAdvice::Classified { correction_score: Some(score), .. }
            if (score - 1.5).abs() < 1e-6
    ));
}
#[tokio::test]
async fn invalid_observations_fail_before_evaluation() {
    let mut input = observation();
    input.failure.clear();
    let (adviser, fake) = adviser(Err(()));
    assert!(adviser.advise(input).await.is_err());
    assert_eq!(fake.calls.load(Ordering::Relaxed), 0);
}
#[derive(Debug)]
struct PendingEvaluator {
    entered: AtomicUsize,
}
#[async_trait::async_trait]
impl RecoveryEvaluator for PendingEvaluator {
    async fn evaluate(&self, _: &RecoveryRequest) -> Result<RecoveryDecision, RankError> {
        self.entered.fetch_add(1, Ordering::Relaxed);
        std::future::pending().await
    }
}
#[tokio::test]
async fn dropping_advice_cancels_the_provider_future_without_retry() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let provider = Arc::new(PendingEvaluator {
        entered: AtomicUsize::new(0),
    });
    let adviser = RecoveryAdviser::new(provider.clone(), thresholds()).unwrap();
    let mut future = Box::pin(adviser.advise(observation()));
    let mut context = Context::from_waker(Waker::noop());
    assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
    drop(future);
    assert_eq!(provider.entered.load(Ordering::Relaxed), 1);
}
