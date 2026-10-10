//! Bounded host observations and provider-neutral advisory answers.
use crate::JevOption;
use std::{collections::BTreeMap, fmt, time::Duration};
use tinytools::RankError;

/// Closed advisory failure vocabulary; never a host policy verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecoveryClass {
    /// An evidenced temporary condition may clear without a prerequisite.
    Transient,
    /// A producer reports a request rate limit.
    RateLimited,
    /// Credentials require external intervention.
    Authentication,
    /// Access is unavailable; this does not grant permission.
    Permission,
    /// The call's arguments need correction.
    WrongArguments,
    /// The chosen capability does not address the operation.
    WrongTool,
    /// The service or tool/module is unavailable.
    Unavailable,
    /// The requested operation is unsupported.
    Unsupported,
    /// Evidence is insufficient to classify.
    Unknown,
}
impl RecoveryClass {
    /// All accepted classification options, in stable order.
    pub const ALL: [Self; 9] = [
        Self::Transient,
        Self::RateLimited,
        Self::Authentication,
        Self::Permission,
        Self::WrongArguments,
        Self::WrongTool,
        Self::Unavailable,
        Self::Unsupported,
        Self::Unknown,
    ];
    /// Stable provider option key.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Transient => "transient",
            Self::RateLimited => "rate_limited",
            Self::Authentication => "authentication",
            Self::Permission => "permission",
            Self::WrongArguments => "wrong_arguments",
            Self::WrongTool => "wrong_tool",
            Self::Unavailable => "unavailable",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
        }
    }
}
/// Producer/host-normalized invocation stage, never inferred from prose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryPhase {
    /// Name/schema rejection before any execution.
    PreExecution,
    /// The tool began execution.
    Execution,
    /// The host cannot establish whether execution began.
    Unknown,
}
/// Host-trusted external-effect facts, not remote tool annotations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryEffect {
    /// The operation does not write external state.
    ReadOnly,
    /// The host has proof no external effect occurred.
    NotApplied,
    /// An external effect is known to have occurred.
    Applied,
    /// A write may have happened and must be reconciled first.
    UncertainWrite,
    /// Effects are unknown; no safety inference is permitted.
    Unknown,
}
/// Trusted outcomes which the adviser must never reinterpret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryFact {
    /// The call succeeded.
    Success,
    /// The host denied execution under policy.
    PolicyDenied,
    /// The user denied approval.
    ApprovalDenied,
    /// Approval expired without permission.
    ApprovalExpired,
    /// The run was canceled.
    Cancelled,
    /// A producer reports a process-terminal fault.
    TerminalFault,
    /// A trusted transport reports invalid credentials.
    Authentication,
    /// A trusted transport reports forbidden access.
    Permission,
}
/// Host-established reason to ask an optional alternate question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryAlternateReason {
    /// An earlier validated classification advised that the chosen tool was wrong.
    WrongToolAdvice,
    /// The host ledger reached its calibrated repeated-blocker threshold.
    RepeatedBlocker,
}
/// Minimal observation explicitly scrubbed by the host before construction.
///
/// Bounds are not sanitization. Callers must remove credentials, identifying data,
/// secret URL queries, raw argument values and unrelated output. Text remains
/// untrusted evidence and must not be interpreted as instructions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryObservation {
    /// Trusted phase.
    pub phase: RecoveryPhase,
    /// Relevant capability description; no transcript.
    pub tool_description: String,
    /// Bounded, scrubbed failure excerpt or normalized description.
    pub failure: String,
    /// Argument names/types only, never raw values by default.
    pub argument_shape: String,
    /// Host-trusted effect state.
    pub effect: RecoveryEffect,
    /// Facts that settle the case without a model evaluation.
    pub facts: Vec<RecoveryFact>,
    /// Attempts already counted by the host ledger.
    pub attempts: u32,
    /// Equivalent failures already counted, independent of predicted class.
    pub repeated_failures: u32,
    /// Scrubbed, evidenced concrete correction; absent means no Score question.
    pub concrete_correction: Option<String>,
    /// Host-authorized, session-discoverable candidates; exclude failed/refused operations.
    pub candidates: Vec<JevOption>,
    /// Explicit prior advice or counted blocker; candidates alone do not enable a question.
    pub alternate_reason: Option<RecoveryAlternateReason>,
}
impl RecoveryObservation {
    /// Maximum bytes of each prose field (including a candidate description).
    pub const MAX_TEXT_BYTES: usize = 2048;
    /// Maximum bytes of one opaque candidate key.
    pub const MAX_KEY_BYTES: usize = 128;
    /// Maximum alternate candidates, excluding `none`.
    pub const MAX_CANDIDATES: usize = 16;
    /// Constructs a minimal observation; the host remains responsible for scrubbing.
    #[must_use]
    pub fn new(
        phase: RecoveryPhase,
        tool_description: impl Into<String>,
        failure: impl Into<String>,
    ) -> Self {
        Self {
            phase,
            tool_description: tool_description.into(),
            failure: failure.into(),
            argument_shape: String::new(),
            effect: RecoveryEffect::Unknown,
            facts: vec![],
            attempts: 0,
            repeated_failures: 0,
            concrete_correction: None,
            candidates: vec![],
            alternate_reason: None,
        }
    }
}
/// Independent typed decision question, mapped mechanically to a provider's dialect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecoveryQuestion {
    /// Closed mutually exclusive alternatives.
    Choice {
        /// Stable answer ID.
        id: String,
        /// Exact question wording.
        prompt: String,
        /// All valid option keys/descriptions.
        options: Vec<JevOption>,
    },
    /// Binary probability; it does not grant authorization.
    Noul {
        /// Stable answer ID.
        id: String,
        /// Exact question wording.
        prompt: String,
    },
    /// Ordered correction-feasibility categories, not probability of success.
    Score {
        /// Stable answer ID.
        id: String,
        /// Exact question wording.
        prompt: String,
        /// Ordered rubric labels, lowest to highest.
        rubric: Vec<String>,
    },
}
impl RecoveryQuestion {
    /// Stable expected answer ID.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Choice { id, .. } | Self::Noul { id, .. } | Self::Score { id, .. } => id,
        }
    }
}
/// One bounded decision batch; all questions are independent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryRequest {
    /// Fixed wording/rubric revision for telemetry and calibration.
    pub version: &'static str,
    /// Host-scrubbed facts treated as untrusted data where textual.
    pub observation: RecoveryObservation,
    /// Complete questions the evaluator must answer exactly once.
    pub questions: Vec<RecoveryQuestion>,
}
/// A typed provider answer; unknown IDs/types/options are rejected.
#[derive(Clone, Debug, PartialEq)]
pub enum RecoveryAnswer {
    /// Complete normalized distribution over the Choice's options.
    Choice {
        /// Probability mass by opaque option key.
        probabilities: BTreeMap<String, f64>,
        /// Distribution concentration, not probability of correctness.
        confidence: f64,
    },
    /// Probability of the explicitly worded binary proposition.
    Noul(f64),
    /// Complete normalized ordered-rubric distribution.
    Score {
        /// One mass per rubric category, in rubric order.
        probabilities: Vec<f64>,
        /// Distribution concentration, not probability of correctness.
        confidence: f64,
    },
}
/// Provider result prior to validation against its request.
#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryDecision {
    /// Exactly the requested answer IDs, with no extras.
    pub answers: BTreeMap<String, RecoveryAnswer>,
    /// Provider-reported token usage, when available.
    pub input_tokens: Option<u64>,
    /// Provider-reported generated tokens, when available.
    pub output_tokens: Option<u64>,
    /// Total evaluator duration including retries, reported by the host provider.
    pub latency: Duration,
    /// Provider attempt count inside the host's total deadline.
    pub attempts: u32,
}
/// Evaluates advisory requests without a transport, runtime or execution loop.
#[async_trait::async_trait]
pub trait RecoveryEvaluator: Send + Sync + fmt::Debug {
    /// Answers the complete independent question batch.
    ///
    /// Hosts enforce cancellation, deadlines and retry limits around this future.
    /// Dropping it must not initiate further provider work.
    ///
    /// # Errors
    /// Returns provider/unavailable/deadline errors; the adviser abstains on these.
    async fn evaluate(&self, request: &RecoveryRequest) -> Result<RecoveryDecision, RankError>;
}
/// Host-selected thresholds, calibrated and versioned outside this crate.
#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryThresholds {
    /// Host calibration policy revision, nonempty and bounded.
    pub version: String,
    /// Minimum class Choice concentration for advice.
    pub class_confidence: f64,
    /// Minimum Noul probability before any corrective/alternate advice.
    pub recoverability: f64,
    /// Minimum optional Score/alternate Choice concentration.
    pub advice_confidence: f64,
    /// Minimum counted equivalent failures for alternate advice outside wrong-tool classification.
    pub repeated_blocker: u32,
}
/// Reason for retaining host fallback and accounting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryAbstention {
    /// Success or a trusted hard fact settled the case.
    TrustedFact,
    /// The provider was unavailable, failed, canceled or exceeded its deadline.
    EvaluatorFailure,
    /// Answer IDs/types/distributions or semantics were invalid.
    InvalidDecision,
    /// A valid answer did not satisfy host-selected thresholds.
    LowConfidence,
    /// The classifier explicitly selected unknown or tied classes.
    UnknownClass,
}
/// Validated advisory outcome; contains no executable action or permission.
#[derive(Clone, Debug, PartialEq)]
pub enum RecoveryAdvice {
    /// Leave recovery to existing host facts/keywords, with unchanged accounting.
    Abstained(RecoveryAbstention),
    /// Valid advisory classification and optional concrete guidance.
    Classified {
        /// Closed class; the host maps this without resetting stable budget keys.
        class: RecoveryClass,
        /// Validated provider data including usage, distributions and latency.
        decision: RecoveryDecision,
        /// Optional candidate key; `none` is represented by absence.
        alternate: Option<String>,
        /// Ordered rubric expectation in `0..=2`; not a success probability.
        correction_score: Option<f64>,
    },
}
