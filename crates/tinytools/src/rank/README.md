# `rank`

Ranking a catalogue of tools against an intent: the `ToolRanker` trait, the
`RankCandidate` / `RankHit` / `RankContext` vocabulary, and `Bm25Ranker`, the
lexical implementation every host gets without a network.

## Design

A host that registers more tools than a model should see on every request
advertises a few and lets the model search for the rest. That search is a
ranking problem, and the interesting rankers (a decision model such as
TypeSafe's Jev, an embedding index) talk to a service this crate must never
depend on. So the crate owns the *question* — here are candidates, here is an
intent, order them — and one free answer, BM25. A harness asks through the
trait and never learns which kind answered; a host composes them, typically
BM25 to retrieve a shortlist and a model to decide (see `tinytools-jev`).

`Bm25Index` and `tokenize` moved here from the `tinyagents` harness's
discovery module so both crates rank with one arithmetic.

## Contract

- `rank` returns at most `limit` hits, best first, and only candidates it
  considers relevant. Empty means "nothing fits", never "padded to `limit`".
- Every returned key names a candidate the caller passed.
- Failure is a `RankError`; the caller falls back. Never a panic on input.
- `RankHit::confidence` is a calibrated probability or `None`. BM25 returns
  `None`: a BM25 score is not a probability, and a caller gating on
  confidence must treat `None` as unknown rather than zero.
- `RankContext` is deliberately small. A model-backed ranker pays for every
  byte on every search, and unrelated context is a distractor.

## Files

| File       | Owns                                                        |
|------------|-------------------------------------------------------------|
| `mod.rs`   | `ToolRanker`, the `Arc<T>` blanket impl, re-exports          |
| `types.rs` | `RankCandidate`, `RankHit`, `RankContext`, `RankError`       |
| `bm25.rs`  | `tokenize`, `Bm25Index`, `Bm25Ranker`                        |
| `test.rs`  | Unit tests                                                   |
