# tinytools-jev

A `tinytools::ToolRanker` backed by a host-provided Jev evaluator. The host
owns its HTTP client, credentials, retry policy, and deadline.

## Retrieve, then decide

Jev answers a `Choice` question with a calibrated probability for every
option in ~150 ms, accepts at most 255 options, and loses accuracy as the
option list fills with entries unrelated to the request. So `JevRanker`
never shows it a whole catalogue:

1. A retriever (`Bm25Ranker` unless the host supplies one) narrows the
   catalogue to `retrieval_k` candidates (20 by default). Skipped when the
   catalogue already fits. When the retriever finds nothing and the catalogue
   fits one Choice, Jev sees all of it — a lexical miss on a paraphrase is
   exactly the case a decision model is for.
2. One request: a `Choice` over the shortlist plus a `none` option, and a
   `Noul` asking whether the request needs a tool at all.
3. Hits are the options by probability, `none` removed, anything below
   `min_probability` dropped. Each hit's `confidence` is its probability;
   `rank_detailed` also returns the Choice confidence, the `needs_tool`
   probability, the `none` probability, tokens, latency and attempts.

Every evaluator failure is a `RankError` the caller falls back from.

## Limits that shape the design

| Limit                        | Value  | Consequence                                   |
|------------------------------|--------|-----------------------------------------------|
| Options per Choice           | 255    | `retrieval_k` is clamped to it                 |
| Summary shown per option     | 240 ch | clipped, with the family named after           |
| Context per request          | 64k    | `RankContext` stays to a few recent turns      |
| Pricing (jev-1.13)           | $0.042 / M input, output free | a search is ~1–2k tokens |

## Wording

Jev reads literally. The instructions name the user's `request` and ask which
tool accomplishes it "by what each tool does, not by shared words"; each
option is `name: first sentence (from family)`; the state carries the request
and at most the caller's few recent turns.

## Building a ranker

Implement `JevEvaluator` in the host by translating `JevRequest` into the
client's wire request and translating its answer into `JevDecision`. Pass that
implementation to `JevRanker::new`. This keeps transport and runtime choices
at the host boundary where their policy belongs.

## Strategies

`JevRankerConfig::with_strategy` picks how the catalogue is narrowed before
the evaluator decides:

- `RetrieveThenDecide` (default): the retriever shortlists `retrieval_k`
  candidates, one evaluation decides. Bounded by the retriever's recall.
- `FamilyThenDecide`: one evaluation over the candidates' families (a
  toolkit, a pack; candidates without one form `core`), then one evaluation
  per chosen family (`max_families`, default 2, run concurrently) over all
  its members. No retrieval for a family that fits one choice, so a paraphrase
  is judged semantically at both steps; a larger family is cut to
  `MAX_CANDIDATES` by the retriever. The family stage sets
  `JevRequest::instructions` so the evaluator asks "which group" rather than
  "which tool"; `JevRanking::families` reports what it chose.
