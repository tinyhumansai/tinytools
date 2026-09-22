//! [`JevStrategy::FamilyThenDecide`]: the evaluator picks the family first,
//! then decides among every member of the top families.
//!
//! One evaluation over the families (a toolkit, a pack — a small choice),
//! then one evaluation per chosen family over all its members, run
//! concurrently. No retrieval for a family that fits one choice, so a
//! paraphrase ("ping alex" for `SLACK_SEND_MESSAGE`) is judged semantically
//! at both steps. A family larger than one choice is cut to fit by the
//! configured retriever — the one place recall can still be lost, and the
//! reason a host should give the ranker a semantic retriever.

use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};

use tinytools::{RankCandidate, RankContext, RankError, RankHit};

use crate::{
    JevDecision, JevOption, JevRanker, JevRankerConfig, JevRanking, JevRequest, JevStrategy,
    NONE_OPTION, option_text_clipped, validate_candidates,
};

/// Shorter clip for a whole-family choice, which can hold 254 members and
/// has to stay under a provider's per-request token and cost ceilings.
const FAMILY_SUMMARY_CHARS: usize = 150;
/// The family candidates without one are grouped into.
const CORE_FAMILY: &str = "core";
/// Member names shown per family in the first stage.
const FAMILY_SAMPLE: usize = 12;
/// A decision whose `needs_tool` is below this abstains, as `decode` does.
const NEEDS_TOOL_FLOOR: f64 = 0.5;

type Families<'a> = BTreeMap<&'a str, Vec<&'a RankCandidate>>;

/// The strategy's entry point; see the module docs.
pub(crate) async fn rank(
    ranker: &JevRanker,
    intent: &str,
    context: &RankContext,
    candidates: &[RankCandidate],
    limit: usize,
) -> Result<JevRanking, RankError> {
    debug_assert_eq!(ranker.config().strategy(), JevStrategy::FamilyThenDecide);
    validate_candidates(candidates)?;
    let started = std::time::Instant::now();
    let mut families: Families<'_> = BTreeMap::new();
    for candidate in candidates {
        families
            .entry(candidate.family.as_deref().unwrap_or(CORE_FAMILY))
            .or_default()
            .push(candidate);
    }
    let chosen = match choose_families(ranker, intent, context, &families).await? {
        Chosen::Families(chosen) => chosen,
        Chosen::Nothing(mut empty) => {
            empty.latency = started.elapsed();
            return Ok(empty);
        }
    };

    // Second stage: every chosen family at once.
    let mut requests: Vec<(String, f64, Vec<&RankCandidate>)> = Vec::new();
    for (family, p_family) in &chosen {
        let Some(members) = families.get(family.as_str()) else {
            continue;
        };
        let members = fit_one_choice(ranker, intent, context, members).await?;
        requests.push((family.clone(), *p_family, members));
    }
    let decisions = join_all(
        requests
            .iter()
            .map(|(family, _, members)| {
                let request = family_request(ranker, intent, context, family, members);
                async move { ranker.evaluator().evaluate(&request).await }
            })
            .collect(),
    )
    .await;

    let mut ranking = JevRanking::empty();
    ranking.families = chosen;
    for ((_, p_family, members), decision) in requests.iter().zip(decisions) {
        let decision = decision?;
        merge(
            &mut ranking,
            &decision,
            *p_family,
            members,
            ranker.config().min_probability,
        );
        ranking.shortlisted += members.len();
        ranking.input_tokens = match (ranking.input_tokens, decision.input_tokens) {
            (Some(a), Some(b)) => Some(a + b),
            (a, b) => a.or(b),
        };
        ranking.attempts = ranking.attempts.max(decision.attempts);
        ranking.needs_tool = match (ranking.needs_tool, decision.needs_tool) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
    }
    if ranking.needs_tool.is_some_and(|p| p < NEEDS_TOOL_FLOOR) {
        ranking.hits.clear();
    }
    ranking
        .hits
        .sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.key.cmp(&b.key)));
    ranking.hits.truncate(limit);
    ranking.latency = started.elapsed();
    Ok(ranking)
}

enum Chosen {
    Families(Vec<(String, f64)>),
    Nothing(JevRanking),
}

/// First stage: which families could answer, best first, at most
/// `max_families`, each above `min_probability`.
async fn choose_families(
    ranker: &JevRanker,
    intent: &str,
    context: &RankContext,
    families: &Families<'_>,
) -> Result<Chosen, RankError> {
    if families.len() == 1 {
        return Ok(Chosen::Families(
            families.keys().map(|f| ((*f).to_owned(), 1.0)).collect(),
        ));
    }
    if families.len() > JevRankerConfig::MAX_CANDIDATES {
        return Err(RankError::invalid_input("too many families for one choice"));
    }
    let mut options: Vec<JevOption> = families
        .iter()
        .map(|(family, members)| {
            if *family == NONE_OPTION {
                return Err(RankError::invalid_input("family `none` is reserved"));
            }
            Ok(JevOption {
                key: (*family).to_owned(),
                description: family_summary(family, members),
            })
        })
        .collect::<Result<_, _>>()?;
    options.push(JevOption {
        key: NONE_OPTION.into(),
        description: "No listed group of tools is relevant to the request.".into(),
    });
    let request = JevRequest {
        intent: intent.into(),
        recent_turns: context.recent_turns.clone(),
        options,
        model: ranker.config().model.clone(),
        instructions: Some(
            "Which group of tools would accomplish the user's request? Each option names \
             a service or a category and lists what its tools do. Pick `none` when no \
             group applies."
                .into(),
        ),
    };
    let decision = ranker.evaluator().evaluate(&request).await?;
    let none = decision
        .probabilities
        .get(NONE_OPTION)
        .copied()
        .unwrap_or(0.0);
    let mut ordered: Vec<(String, f64)> = decision
        .probabilities
        .iter()
        .filter(|(name, _)| name.as_str() != NONE_OPTION && families.contains_key(name.as_str()))
        .map(|(name, p)| (name.clone(), *p))
        .collect();
    ordered.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ordered.truncate(ranker.config().max_families);
    ordered.retain(|(_, p)| *p >= ranker.config().min_probability);
    let abstained = ordered.first().is_none_or(|(_, best)| none >= *best)
        || decision.needs_tool.is_some_and(|p| p < NEEDS_TOOL_FLOOR);
    if abstained {
        let mut empty = JevRanking::empty();
        empty.attempts = decision.attempts;
        empty.input_tokens = decision.input_tokens;
        empty.none_probability = none;
        empty.needs_tool = decision.needs_tool;
        empty.choice_confidence = decision.choice_confidence;
        return Ok(Chosen::Nothing(empty));
    }
    Ok(Chosen::Families(ordered))
}

/// The second-stage request for one family: every member (already cut to
/// fit) plus `none`.
fn family_request(
    ranker: &JevRanker,
    intent: &str,
    context: &RankContext,
    family: &str,
    members: &[&RankCandidate],
) -> JevRequest {
    let mut options: Vec<JevOption> = members
        .iter()
        .map(|m| JevOption {
            key: m.key.clone(),
            description: option_text_clipped(m, FAMILY_SUMMARY_CHARS),
        })
        .collect();
    options.push(JevOption {
        key: NONE_OPTION.into(),
        description: "No listed tool accomplishes the request.".into(),
    });
    JevRequest {
        intent: intent.into(),
        recent_turns: context.recent_turns.clone(),
        options,
        model: ranker.config().model.clone(),
        instructions: Some(format!(
            "Which `{family}` tool accomplishes the user's request? Judge by what each \
             tool does, not by shared words. Pick `none` when no listed tool does it."
        )),
    }
}

/// A family's members, cut to one choice by the retriever when larger;
/// caller order otherwise.
async fn fit_one_choice<'a>(
    ranker: &JevRanker,
    intent: &str,
    context: &RankContext,
    members: &[&'a RankCandidate],
) -> Result<Vec<&'a RankCandidate>, RankError> {
    let room = JevRankerConfig::MAX_CANDIDATES;
    if members.len() <= room {
        return Ok(members.to_vec());
    }
    let owned: Vec<RankCandidate> = members.iter().map(|m| (*m).clone()).collect();
    let hits = ranker
        .config()
        .retriever
        .rank(intent, context, &owned, room)
        .await?;
    let keep: HashSet<&str> = hits.iter().map(|h| h.key.as_str()).collect();
    let kept: Vec<&RankCandidate> = members
        .iter()
        .copied()
        .filter(|m| keep.contains(m.key.as_str()))
        .take(room)
        .collect();
    if kept.is_empty() {
        return Ok(members.iter().copied().take(room).collect());
    }
    Ok(kept)
}

/// Folds one family's decision into `ranking` as `P(family) · P(member)`;
/// a family whose `none` beats its best member contributes nothing.
fn merge(
    ranking: &mut JevRanking,
    decision: &JevDecision,
    p_family: f64,
    members: &[&RankCandidate],
    floor: f64,
) {
    let none = decision
        .probabilities
        .get(NONE_OPTION)
        .copied()
        .unwrap_or(0.0);
    ranking.none_probability = ranking.none_probability.max(none);
    ranking.choice_confidence = ranking
        .choice_confidence
        .max(decision.choice_confidence * p_family);
    let best = members
        .iter()
        .filter_map(|m| decision.probabilities.get(&m.key).copied())
        .fold(0.0_f64, f64::max);
    if none >= best {
        return;
    }
    for member in members {
        let Some(p) = decision.probabilities.get(&member.key).copied() else {
            continue;
        };
        let joint = p * p_family;
        if joint >= floor {
            ranking.hits.push(RankHit {
                key: member.key.clone(),
                score: joint,
                confidence: Some(joint),
            });
        }
    }
}

/// What a family is, for the first stage: its name, its size, and a sample
/// of member names so a toolkit reads as what it does.
fn family_summary(family: &str, members: &[&RankCandidate]) -> String {
    let sample: Vec<String> = members
        .iter()
        .take(FAMILY_SAMPLE)
        .map(|m| m.key.to_ascii_lowercase().replace('_', " "))
        .collect();
    let mut text = format!(
        "{family}: {} tool(s), e.g. {}",
        members.len(),
        sample.join("; ")
    );
    if text.chars().count() > 600 {
        text = text.chars().take(600).collect::<String>() + "…";
    }
    text
}

/// Awaits every future, in order, without an executor dependency.
async fn join_all<F: Future>(futures: Vec<F>) -> Vec<F::Output> {
    let mut pending: Vec<Option<Pin<Box<F>>>> =
        futures.into_iter().map(|f| Some(Box::pin(f))).collect();
    let mut outputs: Vec<Option<F::Output>> = (0..pending.len()).map(|_| None).collect();
    std::future::poll_fn(|cx: &mut TaskContext<'_>| {
        let mut all_done = true;
        for (slot, out) in pending.iter_mut().zip(outputs.iter_mut()) {
            if let Some(future) = slot.as_mut() {
                match future.as_mut().poll(cx) {
                    Poll::Ready(value) => {
                        *out = Some(value);
                        *slot = None;
                    }
                    Poll::Pending => all_done = false,
                }
            }
        }
        if all_done {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
    // Every slot was filled before `poll_fn` resolved; flattening is the
    // panic-free way to say so.
    outputs.into_iter().flatten().collect()
}
