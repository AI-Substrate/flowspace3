//! Retry behaviour at the adapter boundary, proved against a stub that fails
//! on purpose.
//!
//! The unit tests in `src/retry.rs` cover the arithmetic — which statuses,
//! which backoff, how `Retry-After` parses. These cover the thing that
//! actually matters to a caller: a blip becomes a success it never saw, a
//! sustained squeeze becomes a *schedulable* error, and nothing else is
//! retried at all.

use std::time::{Duration, Instant};

use axum::http::StatusCode;
use fs3_core::{Element, ElementKind, Embedder, Error, Span, Summarizer};
use fs3_providers::{AzureCredential, AzureOpenAiConfig, AzureOpenAiEmbedder, OpenAiSummarizer};

mod common;
use common::{Reply, StubServer};

const TWO_VECTORS: &str =
    r#"{"data":[{"index":0,"embedding":[1.0,0.0]},{"index":1,"embedding":[0.0,1.0]}]}"#;
const SUMMARY_REPLY: &str = r#"{"choices":[{"message":{"content":"{\"text\":\"Classifies a node.\",\"tags\":[\"classification\"]}"}}]}"#;

fn texts() -> Vec<String> {
    vec!["alpha".to_string(), "beta".to_string()]
}

fn embedder(endpoint: &str) -> AzureOpenAiEmbedder {
    AzureOpenAiEmbedder::new(
        AzureOpenAiConfig::new(endpoint, "dep", "2024-02-01", AzureCredential::api_key("k")),
        None,
    )
}

fn element() -> Element {
    Element::new(
        ElementKind::Function,
        "function_item",
        "classify",
        "core/src/classify.rs::classify",
        Span::new(120, 127),
        "pub fn classify(ts_kind: &str) -> Option<ElementKind> { .. }",
    )
}

/// The whole point: one 429, then success, and the caller never knows.
#[tokio::test]
async fn a_single_rate_limit_is_absorbed_and_the_caller_sees_a_success() {
    let stub =
        StubServer::failing_then(1, StatusCode::TOO_MANY_REQUESTS, Some("0"), TWO_VECTORS).await;

    let vectors = embedder(&stub.endpoint)
        .embed(&texts())
        .await
        .expect("the retry absorbs a single 429");

    assert_eq!(vectors, vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
    assert_eq!(stub.requests().len(), 2, "one refusal, then one success");
}

/// 502/503/504 are the same shape of transient — a proxy or backend that was
/// briefly not there.
#[tokio::test]
async fn a_bad_gateway_is_also_retried() {
    for status in [
        StatusCode::BAD_GATEWAY,
        StatusCode::SERVICE_UNAVAILABLE,
        StatusCode::GATEWAY_TIMEOUT,
    ] {
        let stub = StubServer::failing_then(1, status, None, TWO_VECTORS).await;

        embedder(&stub.endpoint)
            .embed(&texts())
            .await
            .unwrap_or_else(|e| panic!("{status} should be retried, got {e}"));

        assert_eq!(stub.requests().len(), 2, "{status} was not retried");
    }
}

/// The service's own number is honoured, not merely parsed. A `Retry-After` of
/// 1 second must actually delay the second attempt by about a second — the
/// backoff's own first step is 500ms with full jitter, so a run this slow
/// cannot be explained by the default schedule.
#[tokio::test]
async fn the_services_retry_after_actually_delays_the_next_attempt() {
    let stub =
        StubServer::failing_then(1, StatusCode::TOO_MANY_REQUESTS, Some("1"), TWO_VECTORS).await;

    let started = Instant::now();
    embedder(&stub.endpoint)
        .embed(&texts())
        .await
        .expect("the retry absorbs it");
    let elapsed = started.elapsed();

    assert!(
        elapsed >= Duration::from_secs(1),
        "Retry-After: 1 must be waited out, but the call took {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "and must not be waited out several times over: {elapsed:?}"
    );
}

/// A squeeze that outlives our patience becomes `RateLimited` — a *typed* fact
/// the scheduler can park on, carrying the service's own wait. A formatted
/// string here would be unusable to the thing that has to make the decision.
#[tokio::test]
async fn a_sustained_squeeze_surfaces_as_a_schedulable_rate_limit() {
    // A one-second wait rather than a realistic sixty: the property under test
    // is that the service's number SURVIVES to the error, and a slow test is a
    // test people learn to skip.
    let stub = StubServer::answering_with(|_| Reply {
        status: StatusCode::TOO_MANY_REQUESTS,
        body:
            r#"{"error":{"code":"429","message":"Requests to the deployment exceeded the quota"}}"#
                .to_string(),
        retry_after: Some("1".to_string()),
    })
    .await;

    let error = embedder(&stub.endpoint)
        .embed(&texts())
        .await
        .expect_err("the stub never stops refusing");

    match error {
        Error::RateLimited {
            provider,
            retry_after,
            attempts,
        } => {
            assert_eq!(
                provider, "dep",
                "the DEPLOYMENT owns the quota, so it is named"
            );
            assert_eq!(
                retry_after,
                Some(Duration::from_secs(1)),
                "the service's own wait must survive to whoever parks the claim"
            );
            assert_eq!(attempts, 3, "and how hard we already tried");
        }
        other => panic!("expected a typed rate limit, got {other:?}"),
    }
    assert_eq!(stub.requests().len(), 3, "exactly the declared attempts");
}

/// Rate-limited without advice is the common case; `None` must reach the
/// scheduler rather than becoming a number this layer invented.
#[tokio::test]
async fn a_rate_limit_without_advice_carries_no_invented_wait() {
    let stub =
        StubServer::answering(StatusCode::TOO_MANY_REQUESTS, r#"{"error":{"code":"429"}}"#).await;

    let error = embedder(&stub.endpoint)
        .embed(&texts())
        .await
        .expect_err("the stub never stops refusing");

    assert!(
        matches!(
            error,
            Error::RateLimited {
                retry_after: None,
                ..
            }
        ),
        "expected no invented wait, got {error:?}"
    );
}

/// **The narrowness that keeps this safe.** The daemon's runner retries any
/// failed job three times, so anything retried here is retried again around
/// it. A wrong deployment name must therefore cost exactly one request.
#[tokio::test]
async fn a_permanent_failure_is_never_retried() {
    for (status, body) in [
        (
            StatusCode::NOT_FOUND,
            r#"{"error":{"code":"DeploymentNotFound"}}"#,
        ),
        (StatusCode::UNAUTHORIZED, r#"{"error":{"code":"401"}}"#),
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            r#"{"error":{"code":"500"}}"#,
        ),
        (StatusCode::BAD_REQUEST, r#"{"error":{"code":"400"}}"#),
    ] {
        let stub = StubServer::answering(status, body).await;

        embedder(&stub.endpoint)
            .embed(&texts())
            .await
            .expect_err("a permanent failure");

        assert_eq!(
            stub.requests().len(),
            1,
            "{status} must cost exactly one request — the runner retries around us"
        );
    }
}

/// The summarizer retries too, and the retry must not disturb the
/// structured-output downgrade: a schema rejection is a client error, so it
/// leaves the loop untouched and reaches the downgrade.
#[tokio::test]
async fn the_summarizer_retries_transients_without_breaking_the_downgrade() {
    let stub =
        StubServer::failing_then(1, StatusCode::SERVICE_UNAVAILABLE, None, SUMMARY_REPLY).await;
    let summarizer = fs3_providers::AzureOpenAiSummarizer::new(AzureOpenAiConfig::new(
        &stub.endpoint,
        "chat-dep",
        "2024-12-01-preview",
        AzureCredential::api_key("k"),
    ));

    let summary = summarizer
        .summarize(&element())
        .await
        .expect("the retry absorbs the 503");

    assert_eq!(summary.text, "Classifies a node.");
    let formats: Vec<String> = stub
        .requests()
        .iter()
        .map(|request| request.response_format().unwrap_or("<none>").to_string())
        .collect();
    assert_eq!(
        formats,
        ["json_schema", "json_schema"],
        "a transient retry re-sends the SAME request; it must not silently downgrade"
    );
}

/// Ceilings are declarations the scheduler reads. They are asserted here
/// because a wrong one is silent: too high thrashes a small box, too low
/// wastes a big one, and neither shows up as an error.
#[test]
fn every_provider_declares_a_ceiling_that_matches_its_shape() {
    let azure = embedder("https://example.openai.azure.com");
    assert!(
        azure.concurrency_ceiling() > 1,
        "a quota-sized cloud deployment should not be driven one request at a time"
    );

    let openai = OpenAiSummarizer::new("gpt-4o-mini", None, "k");
    assert!(openai.concurrency_ceiling() > 1);
}
