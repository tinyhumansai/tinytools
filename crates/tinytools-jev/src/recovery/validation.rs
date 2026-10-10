//! Strict typed answer validation and conservative advisory interpretation.
use super::{
    RecoveryAbstention, RecoveryAdvice, RecoveryAlternateReason, RecoveryAnswer, RecoveryClass,
    RecoveryDecision, RecoveryObservation, RecoveryQuestion, RecoveryRequest, RecoveryThresholds,
};
use std::collections::BTreeSet;
use tinytools::RankError;

fn probability(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}
fn text(value: &str, required: bool, limit: usize) -> bool {
    value.len() <= limit && (!required || !value.trim().is_empty()) && !value.contains('\0')
}
pub(super) fn validate_thresholds(thresholds: &RecoveryThresholds) -> Result<(), RankError> {
    if !text(
        &thresholds.version,
        true,
        RecoveryObservation::MAX_KEY_BYTES,
    ) || thresholds.repeated_blocker == 0
        || ![
            thresholds.class_confidence,
            thresholds.recoverability,
            thresholds.advice_confidence,
        ]
        .into_iter()
        .all(probability)
    {
        return Err(RankError::invalid_input("invalid recovery thresholds"));
    }
    Ok(())
}
pub(super) fn validate_observation(observation: &RecoveryObservation) -> Result<(), RankError> {
    let limit = RecoveryObservation::MAX_TEXT_BYTES;
    if observation.alternate_reason == Some(RecoveryAlternateReason::RepeatedBlocker)
        && observation.repeated_failures == 0
    {
        return Err(RankError::invalid_input(
            "alternate blocker has no counted failures",
        ));
    }
    if !text(&observation.tool_description, true, limit)
        || !text(&observation.failure, true, limit)
        || !text(&observation.argument_shape, false, limit)
        || observation
            .concrete_correction
            .as_ref()
            .is_some_and(|value| !text(value, true, limit))
        || observation.candidates.len() > RecoveryObservation::MAX_CANDIDATES
        || observation.facts.len() > 8
    {
        return Err(RankError::invalid_input(
            "recovery observation exceeds bounded contract",
        ));
    }
    let mut keys = BTreeSet::new();
    for candidate in &observation.candidates {
        if !text(&candidate.key, true, RecoveryObservation::MAX_KEY_BYTES)
            || candidate.key.chars().any(char::is_control)
            || candidate.key.trim() != candidate.key
            || candidate.key == "none"
            || !keys.insert(&candidate.key)
            || !text(&candidate.description, true, limit)
        {
            return Err(RankError::invalid_input("invalid recovery candidate"));
        }
    }
    Ok(())
}
fn distribution<'a>(values: impl Iterator<Item = &'a f64>) -> bool {
    let mut sum = 0.0;
    for &value in values {
        if !probability(value) {
            return false;
        }
        sum += value;
    }
    (sum - 1.0).abs() <= 1e-6
}
impl RecoveryDecision {
    /// Checks all answer IDs/types, complete distributions and bounded numeric values.
    ///
    /// Providers may use their own wire validators first; this transport-neutral
    /// check enforces the recovery batch contract after mechanical conversion.
    /// Semantic contradictions and calibrated abstention are handled by the adviser.
    ///
    /// # Errors
    /// Rejects empty batches/options/rubrics, duplicate question IDs/option keys,
    /// missing/extra IDs, wrong variants, unknown/missing options, invalid
    /// probabilities/confidence, incomplete Score distributions and zero attempts.
    pub fn validate(&self, request: &RecoveryRequest) -> Result<(), RankError> {
        if self.attempts == 0
            || request.questions.is_empty()
            || self.answers.len() != request.questions.len()
        {
            return Err(RankError::invalid_input(
                "invalid recovery answer IDs or attempts",
            ));
        }
        let mut ids = BTreeSet::new();
        for question in &request.questions {
            if !ids.insert(question.id()) {
                return Err(RankError::invalid_input("duplicate recovery question ID"));
            }
            let valid = match (question, self.answers.get(question.id())) {
                (
                    RecoveryQuestion::Choice { options, .. },
                    Some(RecoveryAnswer::Choice {
                        probabilities,
                        confidence,
                    }),
                ) => {
                    let expected_keys: BTreeSet<_> =
                        options.iter().map(|option| &option.key).collect();
                    !options.is_empty()
                        && expected_keys.len() == options.len()
                        && expected_keys.iter().copied().eq(probabilities.keys())
                        && probability(*confidence)
                        && distribution(probabilities.values())
                }
                (RecoveryQuestion::Noul { .. }, Some(RecoveryAnswer::Noul(value))) => {
                    probability(*value)
                }
                (
                    RecoveryQuestion::Score { rubric, .. },
                    Some(RecoveryAnswer::Score {
                        probabilities,
                        confidence,
                    }),
                ) => {
                    !rubric.is_empty()
                        && probability(*confidence)
                        && probabilities.len() == rubric.len()
                        && distribution(probabilities.iter())
                }
                _ => false,
            };
            if !valid {
                return Err(RankError::invalid_input("invalid typed recovery answer"));
            }
        }
        Ok(())
    }
}
fn winner(probabilities: &std::collections::BTreeMap<String, f64>) -> Option<&str> {
    let (key, best) = probabilities
        .iter()
        .max_by(|(_, a), (_, b)| a.total_cmp(b))?;
    (probabilities
        .values()
        .filter(|value| value.total_cmp(best).is_eq())
        .count()
        == 1)
        .then_some(key.as_str())
}
fn abstain(reason: RecoveryAbstention) -> RecoveryAdvice {
    RecoveryAdvice::Abstained(reason)
}

pub(super) fn advice(
    request: &RecoveryRequest,
    decision: RecoveryDecision,
    thresholds: &RecoveryThresholds,
) -> RecoveryAdvice {
    if decision.validate(request).is_err() {
        return abstain(RecoveryAbstention::InvalidDecision);
    }
    let Some(RecoveryAnswer::Choice {
        probabilities,
        confidence,
    }) = decision.answers.get("class")
    else {
        return abstain(RecoveryAbstention::InvalidDecision);
    };
    let Some(key) = winner(probabilities) else {
        return abstain(RecoveryAbstention::UnknownClass);
    };
    let Some(class) = RecoveryClass::ALL
        .into_iter()
        .find(|class| class.key() == key)
    else {
        return abstain(RecoveryAbstention::InvalidDecision);
    };
    if class == RecoveryClass::Unknown {
        return abstain(RecoveryAbstention::UnknownClass);
    }
    if *confidence < thresholds.class_confidence {
        return abstain(RecoveryAbstention::LowConfidence);
    }
    let Some(RecoveryAnswer::Noul(recoverability)) = decision.answers.get("recoverability") else {
        return abstain(RecoveryAbstention::InvalidDecision);
    };
    let prerequisites = matches!(
        class,
        RecoveryClass::Authentication | RecoveryClass::Permission | RecoveryClass::Unsupported
    );
    if prerequisites && *recoverability >= thresholds.recoverability {
        return abstain(RecoveryAbstention::InvalidDecision);
    }
    let correction_score = if let Some(RecoveryAnswer::Score {
        probabilities,
        confidence,
    }) = decision.answers.get("correction")
    {
        if *confidence < thresholds.advice_confidence {
            return abstain(RecoveryAbstention::LowConfidence);
        }
        // Fixed three-position rubric validated above. Never interpreted as P(success).
        Some(probabilities[1] + 2.0 * probabilities[2])
    } else {
        None
    };
    let alternate = if let Some(RecoveryAnswer::Choice {
        probabilities,
        confidence,
    }) = decision.answers.get("alternate")
    {
        if *confidence < thresholds.advice_confidence {
            return abstain(RecoveryAbstention::LowConfidence);
        }
        let Some(key) = winner(probabilities) else {
            return abstain(RecoveryAbstention::LowConfidence);
        };
        if key == "none" {
            None
        } else {
            if prerequisites
                || *recoverability < thresholds.recoverability
                || (request.observation.alternate_reason
                    == Some(RecoveryAlternateReason::WrongToolAdvice)
                    && class != RecoveryClass::WrongTool)
                || (class != RecoveryClass::WrongTool
                    && request.observation.repeated_failures < thresholds.repeated_blocker)
            {
                return abstain(RecoveryAbstention::InvalidDecision);
            }
            Some(key.to_owned())
        }
    } else {
        None
    };
    if correction_score.is_some_and(|score| score >= 1.0)
        && (prerequisites || *recoverability < thresholds.recoverability)
    {
        return abstain(RecoveryAbstention::InvalidDecision);
    }
    RecoveryAdvice::Classified {
        class,
        decision,
        alternate,
        correction_score,
    }
}
