#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn candidates() -> Vec<RankCandidate> {
    vec![
        RankCandidate::new(
            "calendar_invite",
            "calendar_invite Send a calendar invite to attendees. title start attendees",
        )
        .with_family("calendar"),
        RankCandidate::new(
            "pdf_read",
            "pdf_read Read the text of a PDF document. path pages",
        )
        .with_family("documents"),
        RankCandidate::new(
            "stock_quote",
            "stock_quote Fetch the latest price for a ticker symbol. symbol",
        )
        .with_family("finance"),
    ]
}

#[test]
fn tokenize_splits_identifiers_and_camel_case() {
    assert_eq!(
        tokenize("memory_hybrid_search readWorkflowResource v2"),
        vec![
            "memory", "hybrid", "search", "read", "workflow", "resource", "v2"
        ]
    );
}

#[test]
fn index_ranks_by_description_and_breaks_ties_by_key() {
    let index = Bm25Index::build([
        ("b_tool", "send a message"),
        ("a_tool", "send a message"),
        ("c_tool", "read a file"),
    ]);
    assert_eq!(index.len(), 3);
    assert_eq!(index.search("send message", 5), vec![1, 0]);
}

#[test]
fn index_on_one_document_corpus_still_finds_it() {
    let index = Bm25Index::build([("only", "fetch the latest price for a ticker symbol")]);
    assert_eq!(index.search("ticker price", 5), vec![0]);
    assert!(index.search("calendar", 5).is_empty());
}

#[test]
fn index_returns_nothing_for_stopword_only_queries() {
    let index = Bm25Index::build([("a", "send a message"), ("b", "read a file")]);
    assert!(index.search("the a of", 5).is_empty());
    assert!(Bm25Index::default().search("anything", 5).is_empty());
    assert!(Bm25Index::default().is_empty());
}

#[test]
fn bm25_ranker_returns_positive_hits_only_best_first() {
    let hits = Bm25Ranker::rank_sync(&candidates(), "read the text of a pdf", 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].key, "pdf_read");
    assert!(hits[0].score > 0.0);
    assert!(hits[0].confidence.is_none());
}

#[test]
fn bm25_ranker_matches_on_family() {
    let hits = Bm25Ranker::rank_sync(&candidates(), "finance", 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].key, "stock_quote");
}

#[test]
fn bm25_ranker_honours_limit() {
    let hits = Bm25Ranker::rank_sync(&candidates(), "send read fetch", 2);
    assert_eq!(hits.len(), 2);
}

#[tokio::test]
async fn bm25_ranker_rejects_empty_intent() {
    let err = Bm25Ranker
        .rank("   ", &RankContext::empty(), &candidates(), 3)
        .await
        .err()
        .map(|e| e.to_string());
    assert_eq!(
        err.as_deref(),
        Some("invalid ranking input: intent is empty")
    );
}

#[tokio::test]
async fn ranker_is_object_safe_behind_an_arc() {
    let ranker: std::sync::Arc<dyn ToolRanker> = std::sync::Arc::new(Bm25Ranker);
    assert_eq!(ranker.kind(), "bm25");
    let hits = ranker
        .rank("calendar invite", &RankContext::empty(), &candidates(), 3)
        .await
        .unwrap();
    assert_eq!(hits[0].key, "calendar_invite");
}

#[test]
fn rank_error_displays_without_credentials() {
    let err = RankError::Backend {
        reason: "status 401".to_owned(),
    };
    assert_eq!(err.to_string(), "ranker backend failed: status 401");
    assert_eq!(RankError::Timeout.to_string(), "ranker timed out");
}
