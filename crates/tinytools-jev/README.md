# tinytools-jev

A `tinytools::ToolRanker` backed by TypeSafe's Jev decision model, through
[`tinyjevclient`](https://github.com/tinyhumansai/tinyjevclient).

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

Every failure — transport, a rejected request, the deadline — is a
`RankError` the caller falls back from. The API key never appears in an error
or a log line.

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

```rust,no_run
use tinytools_jev::{ClientConfig, JevRanker, JevRankerConfig};

let client = ClientConfig::tinyhumans_openrouter("<tinyhumans api key>");
let ranker = JevRanker::from_config(client, JevRankerConfig::new())?;
# Ok::<(), tinytools::RankError>(())
```

`ClientConfig::new` targets TypeSafe directly, `::openrouter` OpenRouter's
compatible endpoint, and `::tinyhumans_openrouter` the TinyHumans proxy; the
`base_url` field is public for a self-hosted proxy.
