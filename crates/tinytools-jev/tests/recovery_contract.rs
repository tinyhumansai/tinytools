//! Recovery remains advisory and validates the complete decision contract.
use tinytools_jev::recovery::{RecoveryClass, RecoveryObservation, RecoveryPhase, RecoveryRequest};

#[test]
fn recovery_questions_are_distinct_from_tool_selection() -> Result<(), tinytools::RankError> {
    let observation = RecoveryObservation::new(
        RecoveryPhase::Execution,
        "Lookup",
        "service temporarily unavailable",
    );
    let request = RecoveryRequest::new(observation)?;
    assert_eq!(request.questions.len(), 2);
    assert_eq!(RecoveryClass::Unknown.key(), "unknown");
    Ok(())
}
