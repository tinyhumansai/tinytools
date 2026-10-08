#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use super::*;
use crate::network::test_support::{DEFAULT_LIMITS, TestHtml, TestNetGate};

fn test_security() -> Arc<TestNetGate> {
    TestNetGate::supervised()
}

fn fetch(
    gate: Arc<TestNetGate>,
    allowed: Vec<String>,
    max: Option<usize>,
    timeout: Option<u64>,
) -> WebFetchTool {
    WebFetchTool::new(
        gate,
        allowed,
        max,
        timeout,
        DEFAULT_LIMITS,
        Arc::new(TestHtml),
    )
}

#[test]
fn web_fetch_name_and_schema() {
    let tool = fetch(test_security(), vec!["example.com".into()], None, None);
    assert_eq!(tool.name(), "web_fetch");
    let schema = tool.parameters_schema();
    assert!(schema["properties"]["url"].is_object());
    assert!(
        schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("url"))
    );
}

#[test]
fn a_host_can_opt_the_schema_into_extra_optional_arguments() {
    let tool = fetch(test_security(), vec!["example.com".into()], None, None).with_schema_property(
        "summary_focus",
        json!({"type": "string", "description": "What you need."}),
    );
    let schema = tool.parameters_schema();
    assert_eq!(schema["properties"]["summary_focus"]["type"], "string");
    assert!(
        !schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("summary_focus"))
    );
    // Without the opt-in the schema is exactly the base one.
    let plain = fetch(test_security(), vec![], None, None).parameters_schema();
    assert!(plain["properties"].get("summary_focus").is_none());
}

#[test]
fn zero_and_none_limits_fall_back_to_defaults() {
    // Callers wire these from `[http_request]`; a stale `Some(0)` is a
    // 0-byte cap (empty bodies) and a 0-second timeout (instant failure).
    // Both `None` and `Some(0)` must coerce to the shared schema defaults.
    let defaults = DEFAULT_LIMITS;
    let from_zero = fetch(
        test_security(),
        vec!["example.com".into()],
        Some(0),
        Some(0),
    );
    assert_eq!(from_zero.max_bytes, defaults.max_response_size);
    assert_eq!(from_zero.timeout_secs, defaults.timeout_secs);
    assert_ne!(from_zero.timeout_secs, 0);
    assert_ne!(from_zero.max_bytes, 0);

    let from_none = fetch(test_security(), vec!["example.com".into()], None, None);
    assert_eq!(from_none.max_bytes, defaults.max_response_size);
    assert_eq!(from_none.timeout_secs, defaults.timeout_secs);
}

#[test]
fn nonzero_limits_are_preserved() {
    let tool = fetch(
        test_security(),
        vec!["example.com".into()],
        Some(4096),
        Some(15),
    );
    assert_eq!(tool.max_bytes, 4096);
    assert_eq!(tool.timeout_secs, 15);
}

#[tokio::test]
async fn web_fetch_rejects_disallowed_domain() {
    let tool = fetch(test_security(), vec!["example.com".into()], None, None);
    let result = tool
        .execute(json!({ "url": "https://evil.test/path" }))
        .await
        .unwrap();
    assert!(result.is_error);
    assert!(result.output().contains("URL rejected"));
}

#[tokio::test]
async fn web_fetch_rejects_invalid_url() {
    let tool = fetch(test_security(), vec!["example.com".into()], None, None);
    let result = tool.execute(json!({ "url": "not-a-url" })).await.unwrap();
    assert!(result.is_error);
}

#[tokio::test]
async fn web_fetch_blocked_under_local_only_privacy_mode() {
    // Privacy epic S7 (#4441): under LocalOnly the fetch is refused with a
    // `[policy-blocked]` result before any URL validation / network.
    let tool = fetch(
        TestNetGate::local_only(),
        vec!["example.com".into()],
        None,
        None,
    );
    let result = tool
        .execute(json!({ "url": "https://example.com/data" }))
        .await
        .unwrap();
    assert!(result.is_error);
    assert!(
        result.output().contains("[policy-blocked]"),
        "got: {}",
        result.output()
    );
    assert!(
        result.output().contains("Local-only"),
        "got: {}",
        result.output()
    );
}

#[test]
fn test_web_fetch_truncation_utf8() {
    // Mock body with multi-byte char exactly at budget
    let body = "Hello 🦀 World"; // 🦀 is at index 6-9
    let max_bytes = 8;
    // Should truncate at index 6
    let cut = floor_char_boundary(body, max_bytes);
    assert_eq!(cut, 6);
    assert_eq!(&body[..cut], "Hello ");
}

// --- content extraction ----------------------------------------------------

fn html(body: &str, content_type: Option<&str>) -> bool {
    is_html(&TestHtml, body, content_type)
}

#[test]
fn an_explicit_html_content_type_selects_markdown_conversion() {
    assert!(html("<p>hi</p>", Some("text/html; charset=utf-8")));
    assert!(html("<p>hi</p>", Some("application/xhtml+xml")));
}

#[test]
fn an_explicit_non_html_content_type_is_taken_at_its_word() {
    // A JSON API that happens to quote markup must come back verbatim —
    // the server said what it sent, so we don't second-guess it by sniffing.
    let body = r#"{"html": "<div><p>one</p><span>two</span><a href="/x">two</a></div>"}"#;
    assert!(!html(body, Some("application/json")));
    assert!(!html("<p>x</p>", Some("text/plain")));
}

#[test]
fn a_missing_content_type_falls_back_to_content_detection() {
    assert!(html(
        "<!DOCTYPE html><html><body><p>hi</p></body></html>",
        None
    ));
    assert!(!html("# Just a README\n\nSome prose.\n", None));
}

#[test]
fn an_empty_content_type_does_not_veto_detection() {
    assert!(html("<!DOCTYPE html><html><body>x</body></html>", Some("")));
}

#[test]
fn the_schema_offers_the_raw_escape_hatch() {
    let tool = fetch(test_security(), vec![], None, None);
    let schema = tool.parameters_schema();
    assert!(
        schema["properties"].get("raw").is_some(),
        "a caller must be able to opt out of conversion: {schema}"
    );
    assert!(
        tool.description().contains("Markdown"),
        "the model needs to know what it will get back: {}",
        tool.description()
    );
}

#[test]
fn the_declared_cap_is_sized_for_extracted_markdown_not_raw_markup() {
    let tool = fetch(test_security(), vec![], None, None);
    let cap = tool
        .max_result_size_chars()
        .expect("web_fetch declares a cap");
    assert!(
        (8_000..=32_000).contains(&cap),
        "cap should sit in the same range as Hermes (15k chars) and Codex \
         (~10k tokens) budget for one result, got {cap}"
    );
}

#[test]
fn html_is_converted_through_the_host_extractor_only_when_it_is_html() {
    let body = "<!DOCTYPE html><html><body><p>hi</p></body></html>";
    assert!(html(body, None));
    assert_eq!(TestHtml.to_markdown(body), "hi");
}

#[tokio::test]
async fn execute_blocks_when_rate_limited() {
    let tool = fetch(
        TestNetGate::with(crate::network::test_support::AutonomyLevel::Supervised, 0),
        vec!["example.com".into()],
        None,
        None,
    );
    let result = tool
        .execute(json!({ "url": "https://example.com/data" }))
        .await
        .unwrap();
    assert!(result.is_error);
    assert!(result.output().contains("Rate limit exceeded"));
}

// --- HTTP error statuses ---------------------------------------------------
//
// Loopback is refused by the SSRF guard, so these drive `fetch_validated`
// (everything after validation) against a one-shot local server.

/// Serves one canned response and returns the base URL.
async fn serve_once(response: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let response = response.to_string();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 8192];
        let _ = socket.read(&mut buf).await.unwrap();
        socket.write_all(response.as_bytes()).await.unwrap();
        let _ = socket.shutdown().await;
    });
    format!("http://{addr}/page")
}

fn http_response(status_line: &str, headers: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status_line}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

async fn fetch_canned(response: String) -> ToolResult {
    let url = serve_once(&response).await;
    fetch(test_security(), vec![], None, None)
        .fetch_validated(&url, 1_000_000, false)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_200_is_a_successful_result_with_the_status_header() {
    let result = fetch_canned(http_response(
        "200 OK",
        "Content-Type: text/plain\r\n",
        "hello",
    ))
    .await;
    assert!(!result.is_error, "got: {}", result.output());
    assert!(result.output().starts_with("status=200 url="));
    assert!(result.output().ends_with("hello"));
}

#[tokio::test]
async fn a_403_is_an_error_naming_the_status_and_suggesting_another_source() {
    let result = fetch_canned(http_response(
        "403 Forbidden",
        "Content-Type: text/plain\r\n",
        "Access denied by bot protection",
    ))
    .await;
    assert!(result.is_error, "got: {}", result.output());
    let out = result.output();
    assert!(
        out.contains("HTTP 403 Forbidden from 127.0.0.1"),
        "got: {out}"
    );
    assert!(out.contains("refused the request"), "got: {out}");
    assert!(out.contains("another source"), "got: {out}");
    assert!(
        out.contains("Access denied by bot protection"),
        "got: {out}"
    );
}

#[tokio::test]
async fn a_429_is_an_error_that_reports_rate_limiting_and_retry_after() {
    let result = fetch_canned(http_response(
        "429 Too Many Requests",
        "Retry-After: 120\r\n",
        "",
    ))
    .await;
    assert!(result.is_error, "got: {}", result.output());
    let out = result.output();
    assert!(out.contains("HTTP 429 Too Many Requests"), "got: {out}");
    assert!(out.contains("rate limit"), "got: {out}");
    assert!(out.contains("Retry-After: 120"), "got: {out}");
}

#[tokio::test]
async fn a_429_without_retry_after_still_reports_rate_limiting() {
    let result = fetch_canned(http_response("429 Too Many Requests", "", "")).await;
    assert!(result.is_error);
    assert!(result.output().contains("rate limit"));
    assert!(!result.output().contains("Retry-After"));
}

#[tokio::test]
async fn a_404_is_an_error() {
    let result = fetch_canned(http_response(
        "404 Not Found",
        "Content-Type: text/html\r\n",
        "<!DOCTYPE html><html><body><p>No such page</p></body></html>",
    ))
    .await;
    assert!(result.is_error, "got: {}", result.output());
    let out = result.output();
    assert!(out.contains("HTTP 404 Not Found"), "got: {out}");
    // The excerpt is the page's text, not its markup.
    assert!(out.contains("No such page"), "got: {out}");
    assert!(!out.contains("<p>"), "got: {out}");
}

#[tokio::test]
async fn a_5xx_is_an_error() {
    let result = fetch_canned(http_response("503 Service Unavailable", "", "")).await;
    assert!(result.is_error);
    assert!(result.output().contains("HTTP 503 Service Unavailable"));
}

#[tokio::test]
async fn an_error_body_excerpt_is_short() {
    let long = "x".repeat(5_000);
    let result = fetch_canned(http_response(
        "500 Internal Server Error",
        "Content-Type: text/plain\r\n",
        &long,
    ))
    .await;
    assert!(result.is_error);
    assert!(
        result.output().len() < 1_000,
        "excerpt must be bounded, got {} bytes",
        result.output().len()
    );
}

#[tokio::test]
async fn a_redirect_is_still_reported_as_a_successful_result() {
    let result = fetch_canned(http_response(
        "301 Moved Permanently",
        "Location: https://example.com/new\r\n",
        "",
    ))
    .await;
    assert!(!result.is_error, "got: {}", result.output());
    assert!(result.output().contains("status=301"));
    assert!(result.output().contains("location=https://example.com/new"));
}
