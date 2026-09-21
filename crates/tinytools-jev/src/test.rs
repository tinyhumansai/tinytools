#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::needless_pass_by_value
)]

use std::{sync::Arc, time::Duration};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Mutex,
};

use super::*;

fn candidates() -> Vec<RankCandidate> {
    vec![
        RankCandidate::new(
            "SLACK_SEND_MESSAGE",
            "SLACK_SEND_MESSAGE Send a message to a Slack channel or user. channel text",
        )
        .with_family("slack"),
        RankCandidate::new(
            "GMAIL_SEND_EMAIL",
            "GMAIL_SEND_EMAIL Send an email from the connected Gmail account. to subject body",
        )
        .with_family("gmail"),
        RankCandidate::new(
            "stock_quote",
            "stock_quote Fetch the latest price for a ticker symbol. symbol",
        ),
    ]
}

fn response(status: u16, body: &str) -> String {
    let reason = if status == 200 { "OK" } else { "Error" };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn answer(probabilities: Value, confidence: f64, needs_tool: Option<f64>) -> String {
    let choice = probabilities
        .as_object()
        .unwrap()
        .iter()
        .max_by(|a, b| a.1.as_f64().partial_cmp(&b.1.as_f64()).unwrap())
        .map(|(k, _)| k.clone())
        .unwrap();
    let mut answers = json!({
        "tool": {
            "type": "choice",
            "choice": choice,
            "probabilities": probabilities,
            "confidence": confidence
        }
    });
    if let Some(p) = needs_tool {
        answers["needs_tool"] = json!({"type": "noul", "noul": p});
    }
    json!({
        "model": "typesafe/jev-1.13",
        "answers": answers,
        "usage": {"input_tokens": 321, "output_tokens": 4}
    })
    .to_string()
}

/// One-shot loopback server: answers each connection with the next canned
/// response and records the request bodies it saw.
async fn server(responses: Vec<String>) -> (String, Arc<Mutex<Vec<Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&seen);
    tokio::spawn(async move {
        for canned in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0_u8; 65_536];
            let mut raw = Vec::new();
            loop {
                let read = socket.read(&mut buffer).await.unwrap();
                raw.extend_from_slice(&buffer[..read]);
                let text = String::from_utf8_lossy(&raw);
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let length: usize = head
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length: "))
                        .and_then(|v| v.trim().parse().ok())
                        .unwrap_or(0);
                    if body.len() >= length {
                        recorder
                            .lock()
                            .await
                            .push(serde_json::from_str(body).unwrap());
                        break;
                    }
                }
                if read == 0 {
                    break;
                }
            }
            socket.write_all(canned.as_bytes()).await.unwrap();
            socket.shutdown().await.unwrap();
        }
    });
    (format!("http://{address}"), seen)
}

fn ranker(base_url: String, config: JevRankerConfig) -> JevRanker {
    let mut client_config = ClientConfig::openrouter("test-key");
    client_config.base_url = base_url;
    client_config.retry = RetryPolicy {
        max_retries: 0,
        ..RetryPolicy::default()
    };
    JevRanker::from_config(client_config, config).unwrap()
}

#[tokio::test]
async fn small_catalogue_goes_straight_to_jev_with_a_none_option() {
    let (url, seen) = server(vec![response(
        200,
        &answer(
            json!({"SLACK_SEND_MESSAGE": 0.83, "GMAIL_SEND_EMAIL": 0.12, "stock_quote": 0.01, "none": 0.04}),
            0.79,
            Some(0.97),
        ),
    )])
    .await;
    let ranker = ranker(url, JevRankerConfig::new());

    let ranking = ranker
        .rank_detailed(
            "ping alex on slack that I'm ten minutes late",
            &RankContext::empty(),
            &candidates(),
            3,
        )
        .await
        .unwrap();

    assert_eq!(
        ranking
            .hits
            .iter()
            .map(|h| h.key.as_str())
            .collect::<Vec<_>>(),
        vec!["SLACK_SEND_MESSAGE", "GMAIL_SEND_EMAIL"],
        "stock_quote sits below min_probability and none is never a hit"
    );
    assert_eq!(ranking.hits[0].confidence, Some(0.83));
    assert_eq!(ranking.choice_confidence, 0.79);
    assert_eq!(ranking.needs_tool, Some(0.97));
    assert_eq!(ranking.none_probability, 0.04);
    assert_eq!(ranking.shortlisted, 3);
    assert_eq!(ranking.input_tokens, Some(321));
    assert_eq!(ranking.attempts, 1);

    let request = &seen.lock().await[0];
    assert_eq!(request["model"], "jev-latest");
    assert_eq!(
        request["state"]["request"],
        "ping alex on slack that I'm ten minutes late"
    );
    let criteria = request["questions"]["tool"]["criteria"]
        .as_object()
        .unwrap();
    assert_eq!(criteria.len(), 4, "three candidates plus `none`");
    assert!(
        criteria["SLACK_SEND_MESSAGE"]
            .as_str()
            .unwrap()
            .ends_with("(from slack)")
    );
    assert_eq!(request["questions"]["needs_tool"]["type"], "noul");
}

#[tokio::test]
async fn large_catalogue_is_retrieved_first_then_decided() {
    let (url, seen) = server(vec![response(
        200,
        &answer(
            json!({"t_send_7": 0.9, "t_send_3": 0.06, "none": 0.04}),
            0.85,
            Some(0.9),
        ),
    )])
    .await;
    let mut catalogue: Vec<RankCandidate> = (0..40)
        .map(|i| {
            RankCandidate::new(
                format!("t_read_{i}"),
                format!("t_read_{i} Read record {i}."),
            )
        })
        .collect();
    catalogue.push(RankCandidate::new(
        "t_send_7",
        "t_send_7 Send a message to a person.",
    ));
    catalogue.push(RankCandidate::new(
        "t_send_3",
        "t_send_3 Send a message to a channel.",
    ));
    let ranker = ranker(url, JevRankerConfig::new().with_retrieval_k(5));

    let ranking = ranker
        .rank_detailed("send a message", &RankContext::empty(), &catalogue, 3)
        .await;
    let ranking = ranking.unwrap();

    assert_eq!(ranking.hits[0].key, "t_send_7");
    assert_eq!(ranking.needs_tool, Some(0.9));
    let request = &seen.lock().await[0];
    let criteria = request["questions"]["tool"]["criteria"]
        .as_object()
        .unwrap();
    assert!(
        criteria.len() <= 6,
        "shortlist of at most 5 plus `none`, got {}",
        criteria.len()
    );
    assert!(criteria.contains_key("t_send_7"));
    assert!(criteria.contains_key("t_send_3"));
    assert_eq!(ranking.shortlisted, criteria.len() - 1);
}

#[tokio::test]
async fn retriever_miss_on_a_small_catalogue_still_lets_jev_decide() {
    let (url, seen) = server(vec![response(
        200,
        &answer(
            json!({"SLACK_SEND_MESSAGE": 0.7, "GMAIL_SEND_EMAIL": 0.2, "stock_quote": 0.05, "none": 0.05}),
            0.6,
            Some(0.8),
        ),
    )])
    .await;
    // retrieval_k of 1 forces retrieval; "ping" matches nothing lexically.
    let ranker = ranker(url, JevRankerConfig::new().with_retrieval_k(1));

    let hits = ranker
        .rank("ping alex", &RankContext::empty(), &candidates(), 3)
        .await
        .unwrap();

    assert_eq!(hits[0].key, "SLACK_SEND_MESSAGE");
    let request = &seen.lock().await[0];
    let criteria = request["questions"]["tool"]["criteria"]
        .as_object()
        .unwrap();
    assert_eq!(
        criteria.len(),
        4,
        "the whole catalogue was shown after the retriever missed"
    );
}

#[tokio::test]
async fn empty_inputs_never_reach_the_network() {
    let ranker = ranker("http://127.0.0.1:9".to_owned(), JevRankerConfig::new());
    assert!(
        ranker
            .rank("anything", &RankContext::empty(), &[], 3)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        ranker
            .rank("anything", &RankContext::empty(), &candidates(), 0)
            .await
            .unwrap()
            .is_empty()
    );
    let err = ranker
        .rank("  ", &RankContext::empty(), &candidates(), 3)
        .await
        .unwrap_err();
    assert!(matches!(err, RankError::InvalidInput { .. }), "{err}");
}

#[tokio::test]
async fn reserved_and_duplicate_keys_are_rejected_before_sending() {
    let ranker = ranker("http://127.0.0.1:9".to_owned(), JevRankerConfig::new());
    let reserved = vec![RankCandidate::new("none", "none nothing")];
    let err = ranker
        .rank("x", &RankContext::empty(), &reserved, 3)
        .await
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "invalid ranking input: candidate key `none` is reserved"
    );
    let duplicate = vec![
        RankCandidate::new("a", "a one"),
        RankCandidate::new("a", "a two"),
    ];
    let err = ranker
        .rank("x", &RankContext::empty(), &duplicate, 3)
        .await
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "invalid ranking input: duplicate candidate key"
    );
}

#[tokio::test]
async fn provider_failures_become_backend_errors_without_the_key() {
    let (url, _) = server(vec![response(401, r#"{"error":"nope"}"#)]).await;
    let ranker = ranker(url, JevRankerConfig::new());
    let err = ranker
        .rank("send a message", &RankContext::empty(), &candidates(), 3)
        .await
        .unwrap_err();
    let text = err.to_string();
    assert!(matches!(err, RankError::Backend { .. }), "{text}");
    assert!(text.contains("authentication failed"), "{text}");
    assert!(
        !text.contains("test-key"),
        "the key must never surface: {text}"
    );
}

#[tokio::test]
async fn the_deadline_is_enforced() {
    // A listener that accepts and never answers.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(30)).await;
    });
    let ranker = ranker(
        url,
        JevRankerConfig::new().with_timeout(Duration::from_millis(200)),
    );
    let err = ranker
        .rank("send a message", &RankContext::empty(), &candidates(), 3)
        .await
        .unwrap_err();
    assert!(matches!(err, RankError::Timeout), "{err}");
}

#[test]
fn option_text_clips_long_summaries_and_names_the_family() {
    let long = "x".repeat(400);
    let candidate = RankCandidate::new("k", long).with_family("fam");
    let text = option_text(&candidate);
    let text = text.as_str().unwrap();
    assert!(text.starts_with(&"x".repeat(MAX_SUMMARY_CHARS)));
    assert!(text.ends_with("… (from fam)"));
}

#[test]
fn config_debug_never_prints_a_client_and_clamps_knobs() {
    let config = JevRankerConfig::new()
        .with_retrieval_k(9_999)
        .with_min_probability(7.0);
    assert_eq!(config.retrieval_k, JevRankerConfig::MAX_OPTIONS);
    assert_eq!(config.min_probability, 1.0);
    assert!(format!("{config:?}").contains("bm25"));
}
