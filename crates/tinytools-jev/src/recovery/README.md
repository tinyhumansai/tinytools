# Recovery advice

`RecoveryObservation` carries a host-normalized phase, effect facts, counts,
argument shape and explicitly scrubbed diagnostic. `RecoveryRequest::new` builds
stable independent Choice/Noul questions; a concrete correction enables Score.
`RecoveryEvaluator` is an async transport-neutral port. `RecoveryAdviser` validates
its result and returns separate advice or an explicit abstention reason.

The nine classes are transient, rate_limited, authentication, permission,
wrong_arguments, wrong_tool, unavailable, unsupported and unknown. Unknown/ties
abstain. Recoverability asks whether a policy-permitted next attempt can recover
using evidenced corrections or transient conditions without an external
prerequisite. Score is a three-category ordered feasibility distribution; its
expectation (0..=2) is **not** a probability of success. Confidence is distribution
concentration and is **not** accuracy. Hosts must calibrate explicit versioned
thresholds, including the counted repeated-blocker threshold.

Candidate descriptions alone do not enable alternate evaluation. The host supplies
`RecoveryAlternateReason::WrongToolAdvice` after prior advice or `RepeatedBlocker`
after its stable ledger reaches the configured threshold. A host can make a second
bounded batch after wrong-tool advice within the **same total deadline**. Alternate
options include `none`; every other key must be supplied by the host, authorized,
known in the session, executable through the existing bridge, and exclude the failed
operation and refused services. An accepted alternate requires recoverability and
wrong-tool classification or a counted repeated blocker. It never executes a tool.

Each prose field is at most 2048 bytes; each candidate key is at most 128 bytes;
there are at most 16 candidates and 8 trusted facts. Oversized/empty required fields
are rejected without truncation. These limits do not scrub data: the host must
minimize identifying/private information, remove secrets and URL queries, and send
argument names/types instead of raw values. Text remains untrusted evidence.

The complete typed answer batch is validated for IDs, variants, finite bounded
probabilities/confidence, exact option sets and normalized distributions (sum within
1e-6). Provider failures, malformed or contradictory advice and insufficient
confidence abstain. Trusted success, denial, expired approval, cancellation,
terminal faults, authentication/permission facts and applied/uncertain writes skip
the evaluator. An unknown effect never proves that a retry is safe.

No transport, credentials, retry loop, deadline scheduler, ledger, prompt mutation,
normalizer or authorization mechanism lives here. Hosts retain all admission checks,
remaining-time/cancellation limits, stable operation/scope budgets, exact-repeat
guards, one accounting observation per failed invocation and keyword fallback.
Advice does not alter the original tool result or durable transcript.

## Existing result boundary

`tinytools::ToolResult` already distinguishes reported `is_error` from an execution
`Err`, carries optional `ToolErrorKind::{Retry,Failed}`, and round-trips producer
facts through host-only `metadata`. Its model rendering excludes metadata, but
serialization retains it. A retry hint is not effect safety, and parsed arbitrary
JSON (`ok:false`) is not globally trusted. Adapter-owned facts must be normalized by
the host; this contract adds no second failure envelope. Metadata also needs explicit
scrubbing before a decision service sees it.
