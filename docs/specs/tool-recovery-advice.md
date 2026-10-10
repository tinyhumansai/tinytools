# Advisory tool failure decisions

TinyTools exposes a provider-neutral, additive recovery contract in tinytools-jev.
The host supplies scrubbed, normalized observations and preauthorized alternate
candidates. No raw transcripts or argument values are requested. Byte and count
limits reject oversized input rather than silently changing its meaning.

Class Choice, recoverability Noul and optional concrete-correction Score are
independent questions. Alternate Choice includes none and only supplied candidates.
All required answers must have correct IDs/types, finite probabilities and complete
normalized distributions. Invalid or contradictory answers abstain. Confidence is
distribution concentration, not the probability that a decision is correct.

Success and trusted terminal/authorization/uncertain-effect facts skip evaluation.
The advisory layer never authorizes, executes or retries tools, alters the ledger,
or supplies a transport/runtime. Hosts own deadlines, cancellation, scrubbing,
candidate admission, accounting, calibration and keyword fallback. Existing search
APIs retain their meaning. No default policy thresholds are introduced.
