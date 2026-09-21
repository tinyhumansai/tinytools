//! A [`ToolRanker`] backed by `TypeSafe`'s Jev decision model.
//!
//! # Retrieve, then decide
//!
//! Jev answers a `Choice` question — "which of these options fits this
//! state?" — with a calibrated probability for every option, in one round
//! trip of ~150 ms. It accepts at most 255 options, and its accuracy falls as
//! the option list fills with entries unrelated to the request. So this
//! ranker never shows Jev the whole catalogue. A cheap retriever (BM25 unless
//! the host supplies something better) narrows the catalogue to a shortlist
//! of `retrieval_k` candidates, and one Jev request decides among them:
//!
//! 1. `retriever.rank(intent, catalogue, retrieval_k)` → shortlist. When the
//!    catalogue already fits, the retriever is skipped and Jev sees all of
//!    it. When the retriever finds *nothing* — the paraphrase it cannot
//!    bridge — and the catalogue is small enough, Jev still sees all of it,
//!    because that is precisely the case a decision model exists for.
//! 2. One request: a `Choice` over the shortlist plus a `none` option, and a
//!    `Noul` asking whether the request needs a tool at all.
//! 3. Hits are the options ordered by probability, `none` removed, below
//!    [`JevRankerConfig::min_probability`] dropped. Each hit's `confidence`
//!    is its probability.
//!
//! Anything that stops the decision — transport, a rejected request, the
//! deadline — is a [`RankError`] the caller falls back from. The API key is
//! never in an error or a log line; `tinyjevclient` redacts it.
//!
//! # Wording
//!
//! Jev reads literally. The instructions name the *user's request* and ask
//! for the tool that accomplishes it, each option is `name: first sentence`,
//! and the state carries the request and at most a few recent turns. Extra
//! context is a distractor, not a help.

#[cfg(test)]
mod test;
mod types;

pub use tinyjevclient::{Client, ClientConfig, Provider, RetryPolicy};
pub use types::{JevRankerConfig, JevRanking};

use std::{collections::BTreeMap, time::Duration};

use serde_json::{Value, json};
use tinyjevclient::{
    Answer, Choice, Error as JevError, EvaluationFailure, EvaluationRequest, Noul, NoulCriteria,
    Question,
};
use tinytools::{RankCandidate, RankContext, RankError, RankHit, ToolRanker};

/// Question id of the tool `Choice`.
const TOOL_QUESTION: &str = "tool";
/// Question id of the needs-a-tool `Noul`.
const NEEDS_TOOL_QUESTION: &str = "needs_tool";
/// The option every Choice carries so an off-catalogue request has somewhere
/// to go other than the least-bad tool.
const NONE_OPTION: &str = "none";
/// Longest summary Jev is shown per option. Descriptions past this are
/// clipped at a character boundary; the model decides on the opening
/// sentence anyway, and the request has to fit under the provider's body cap.
const MAX_SUMMARY_CHARS: usize = 240;

/// Ranks tools with Jev. See the [crate docs](crate) for how.
#[derive(Debug, Clone)]
pub struct JevRanker {
    client: Client,
    config: JevRankerConfig,
}

impl JevRanker {
    /// The stable [`ToolRanker::kind`] of this ranker.
    pub const KIND: &'static str = "jev";

    /// A ranker over an already-built client.
    #[must_use]
    pub fn new(client: Client, config: JevRankerConfig) -> Self {
        Self { client, config }
    }

    /// A ranker over a client built from `client_config`.
    ///
    /// # Errors
    ///
    /// Returns the client's configuration error (empty key, bad base URL,
    /// zero timeout) as [`RankError::InvalidInput`].
    pub fn from_config(
        client_config: ClientConfig,
        config: JevRankerConfig,
    ) -> Result<Self, RankError> {
        let client = Client::new(client_config).map_err(|error| RankError::InvalidInput {
            reason: error.to_string(),
        })?;
        Ok(Self::new(client, config))
    }

    /// The configuration in force.
    #[must_use]
    pub fn config(&self) -> &JevRankerConfig {
        &self.config
    }

    /// Ranks and returns everything the decision learned, not only the hits.
    ///
    /// # Errors
    ///
    /// [`RankError::InvalidInput`] for an empty intent or a duplicate
    /// candidate key; [`RankError::Timeout`] past the configured deadline;
    /// [`RankError::Backend`] for anything the provider or transport did.
    pub async fn rank_detailed(
        &self,
        intent: &str,
        context: &RankContext,
        candidates: &[RankCandidate],
        limit: usize,
    ) -> Result<JevRanking, RankError> {
        let intent = intent.trim();
        if intent.is_empty() {
            return Err(RankError::InvalidInput {
                reason: "intent is empty".to_owned(),
            });
        }
        if candidates.is_empty() || limit == 0 {
            return Ok(JevRanking::empty());
        }
        let shortlist = self.shortlist(intent, context, candidates).await?;
        if shortlist.is_empty() {
            return Ok(JevRanking::empty());
        }

        let request = self.build_request(intent, context, &shortlist)?;
        let started = std::time::Instant::now();
        let evaluated = tokio::time::timeout(self.config.timeout, self.client.evaluate(&request))
            .await
            .map_err(|_elapsed| RankError::Timeout)?;
        let result = evaluated.map_err(map_failure)?;
        let mut ranking = decode(&result.response, &shortlist, self.config.min_probability)?;
        ranking.hits.truncate(limit);
        ranking.shortlisted = shortlist.len();
        ranking.latency = started.elapsed();
        ranking.attempts = result.attempts;
        ranking.input_tokens = result.response.usage.input_tokens;
        #[cfg(feature = "tracing")]
        tracing::debug!(
            target: "tinytools_jev",
            shortlisted = shortlist.len(),
            hits = ranking.hits.len(),
            choice_confidence = ranking.choice_confidence,
            needs_tool = ?ranking.needs_tool,
            latency_ms = ranking.latency.as_millis() as u64,
            attempts = ranking.attempts,
            "jev tool ranking"
        );
        Ok(ranking)
    }

    /// Narrows `candidates` to what Jev will be shown.
    async fn shortlist<'a>(
        &self,
        intent: &str,
        context: &RankContext,
        candidates: &'a [RankCandidate],
    ) -> Result<Vec<&'a RankCandidate>, RankError> {
        let fits_without_retrieval = candidates.len() <= self.config.retrieval_k;
        if fits_without_retrieval {
            return Ok(candidates.iter().collect());
        }
        let hits = self
            .config
            .retriever
            .rank(intent, context, candidates, self.config.retrieval_k)
            .await?;
        let by_key: BTreeMap<&str, &RankCandidate> = candidates
            .iter()
            .map(|candidate| (candidate.key.as_str(), candidate))
            .collect();
        if by_key.len() != candidates.len() {
            return Err(RankError::InvalidInput {
                reason: "duplicate candidate key".to_owned(),
            });
        }
        let shortlist: Vec<&RankCandidate> = hits
            .iter()
            .filter_map(|hit| by_key.get(hit.key.as_str()).copied())
            .collect();
        // A lexical retriever that finds nothing has met a paraphrase. If the
        // whole catalogue fits one Choice, let the decision model see it.
        if shortlist.is_empty() && candidates.len() <= JevRankerConfig::MAX_OPTIONS {
            return Ok(candidates.iter().collect());
        }
        Ok(shortlist)
    }

    fn build_request(
        &self,
        intent: &str,
        context: &RankContext,
        shortlist: &[&RankCandidate],
    ) -> Result<EvaluationRequest, RankError> {
        let mut criteria: BTreeMap<String, Option<Value>> = BTreeMap::new();
        for candidate in shortlist {
            if candidate.key == NONE_OPTION {
                return Err(RankError::InvalidInput {
                    reason: format!("candidate key `{NONE_OPTION}` is reserved"),
                });
            }
            if criteria
                .insert(candidate.key.clone(), Some(option_text(candidate)))
                .is_some()
            {
                return Err(RankError::InvalidInput {
                    reason: "duplicate candidate key".to_owned(),
                });
            }
        }
        criteria.insert(
            NONE_OPTION.to_owned(),
            Some(json!("No listed tool accomplishes the request.")),
        );

        let mut state = json!({ "request": intent });
        if !context.recent_turns.is_empty()
            && let Some(object) = state.as_object_mut()
        {
            object.insert(
                "recent_user_turns".to_owned(),
                Value::Array(
                    context
                        .recent_turns
                        .iter()
                        .map(|turn| Value::String(turn.clone()))
                        .collect(),
                ),
            );
        }

        let questions = BTreeMap::from([
            (
                TOOL_QUESTION.to_owned(),
                Question::Choice(Choice {
                    instructions: json!(
                        "Which tool accomplishes the user's `request`? Judge by what \
                         each tool does, not by shared words. Pick `none` when no \
                         listed tool does it."
                    ),
                    criteria,
                }),
            ),
            (
                NEEDS_TOOL_QUESTION.to_owned(),
                Question::Noul(Noul {
                    instructions: json!(
                        "Does fulfilling the user's `request` require calling a tool \
                         — an action or a lookup outside the assistant's own knowledge?"
                    ),
                    criteria: Some(NoulCriteria {
                        r#true: json!(
                            "The request asks for an action or for information that \
                             must be fetched."
                        ),
                        r#false: json!("The request can be answered by replying, with no tool."),
                    }),
                }),
            ),
        ]);

        Ok(EvaluationRequest {
            state,
            model: self.config.model.clone(),
            questions,
        })
    }
}

#[async_trait::async_trait]
impl ToolRanker for JevRanker {
    fn kind(&self) -> &'static str {
        Self::KIND
    }

    async fn rank(
        &self,
        intent: &str,
        context: &RankContext,
        candidates: &[RankCandidate],
        limit: usize,
    ) -> Result<Vec<RankHit>, RankError> {
        self.rank_detailed(intent, context, candidates, limit)
            .await
            .map(|ranking| ranking.hits)
    }
}

impl JevRanking {
    fn empty() -> Self {
        Self {
            hits: Vec::new(),
            choice_confidence: 0.0,
            needs_tool: None,
            none_probability: 0.0,
            shortlisted: 0,
            input_tokens: None,
            latency: Duration::ZERO,
            attempts: 0,
        }
    }
}

/// `name: summary`, clipped, with the family named so "the Slack one" ranks.
fn option_text(candidate: &RankCandidate) -> Value {
    let mut summary: String = candidate.summary.chars().take(MAX_SUMMARY_CHARS).collect();
    if summary.len() < candidate.summary.len() {
        summary.push('…');
    }
    match &candidate.family {
        Some(family) => json!(format!("{summary} (from {family})")),
        None => json!(summary),
    }
}

fn map_failure(failure: EvaluationFailure) -> RankError {
    match failure.error {
        JevError::InvalidRequest { reason } | JevError::InvalidConfig { reason } => {
            RankError::InvalidInput { reason }
        }
        JevError::Timeout => RankError::Timeout,
        other => RankError::Backend {
            // `Display` on every variant is credential-free by the client's
            // contract; the transport source is dropped, not printed.
            reason: format!("{other} after {} attempt(s)", failure.attempts),
        },
    }
}

/// Turns the provider's answers into hits, best first.
fn decode(
    response: &tinyjevclient::EvaluationResponse,
    shortlist: &[&RankCandidate],
    min_probability: f64,
) -> Result<JevRanking, RankError> {
    let Some(Answer::Choice(choice)) = response.answers.get(TOOL_QUESTION) else {
        return Err(RankError::Backend {
            reason: "response has no choice answer for `tool`".to_owned(),
        });
    };
    let needs_tool = match response.answers.get(NEEDS_TOOL_QUESTION) {
        Some(Answer::Noul(noul)) => Some(noul.noul),
        _ => None,
    };
    let none_probability = choice
        .probabilities
        .get(NONE_OPTION)
        .copied()
        .unwrap_or(0.0);
    let mut hits: Vec<RankHit> = shortlist
        .iter()
        .filter_map(|candidate| {
            let probability = *choice.probabilities.get(&candidate.key)?;
            (probability >= min_probability).then(|| RankHit {
                key: candidate.key.clone(),
                score: probability,
                confidence: Some(probability),
            })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.key.cmp(&b.key))
    });
    Ok(JevRanking {
        hits,
        choice_confidence: choice.confidence,
        needs_tool,
        none_probability,
        shortlisted: shortlist.len(),
        input_tokens: None,
        latency: Duration::ZERO,
        attempts: 0,
    })
}
