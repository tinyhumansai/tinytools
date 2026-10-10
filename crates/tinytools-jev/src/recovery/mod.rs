//! Typed, bounded recovery advice. Hosts retain authorization, execution and accounting.
mod questions;
mod types;
mod validation;
use std::sync::Arc;
use tinytools::RankError;
pub use types::*;

/// Advises on unresolved failures using a host-provided decision evaluator.
#[derive(Debug, Clone)]
pub struct RecoveryAdviser {
    evaluator: Arc<dyn RecoveryEvaluator>,
    thresholds: RecoveryThresholds,
}
impl RecoveryAdviser {
    /// Constructs an adviser with explicitly selected calibration thresholds.
    ///
    /// # Errors
    /// Rejects empty/oversized policy version and nonfinite/out-of-range thresholds.
    pub fn new(
        evaluator: Arc<dyn RecoveryEvaluator>,
        thresholds: RecoveryThresholds,
    ) -> Result<Self, RankError> {
        validation::validate_thresholds(&thresholds)?;
        Ok(Self {
            evaluator,
            thresholds,
        })
    }
    /// Evaluates once, or abstains without interpreting trusted host facts.
    ///
    /// This future has no internal timeout/retry. The host must impose its remaining
    /// deadline and cancellation and count the failed invocation regardless of advice.
    ///
    /// # Errors
    /// Rejects observations outside the bounded input contract before evaluation.
    pub async fn advise(
        &self,
        observation: RecoveryObservation,
    ) -> Result<RecoveryAdvice, RankError> {
        validation::validate_observation(&observation)?;
        if !observation.facts.is_empty()
            || matches!(
                observation.effect,
                RecoveryEffect::UncertainWrite | RecoveryEffect::Applied
            )
        {
            return Ok(RecoveryAdvice::Abstained(RecoveryAbstention::TrustedFact));
        }
        let mut observation = observation;
        if observation.alternate_reason == Some(RecoveryAlternateReason::RepeatedBlocker)
            && observation.repeated_failures < self.thresholds.repeated_blocker
        {
            observation.alternate_reason = None;
        }
        let request = RecoveryRequest::new(observation)?;
        let Ok(decision) = self.evaluator.evaluate(&request).await else {
            return Ok(RecoveryAdvice::Abstained(
                RecoveryAbstention::EvaluatorFailure,
            ));
        };
        Ok(validation::advice(&request, decision, &self.thresholds))
    }
}
#[cfg(test)]
mod test_support;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
