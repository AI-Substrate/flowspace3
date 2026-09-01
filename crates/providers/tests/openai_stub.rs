//! What the OpenAI adapter puts on the wire, proved without OpenAI.
//!
//! The OpenAI adapter shipped with unit tests over its parsing but nothing
//! over its *requests*, so the structured-output work landed here first: the
//! schema, and the downgrade for the OpenAI-compatible servers (Ollama, vLLM,
//! LM Studio, llama.cpp) that do not implement it.
//!
//! The stub is a fake service — see `tests/common/mod.rs`. No key, no network.

use axum::http::StatusCode;
use fs3_core::{Element, ElementKind, Embedder, Error, Span, Summarizer};
use fs3_providers::{OpenAiEmbedder, OpenAiSummarizer};

mod common;
use common::StubServer;

const SUMMARY_RESPONSE: &str = r#"{"choices":[{"message":{"content":"{\"text\":\"Classifies a node.\",\"tags\":[\"classification\",\"parsing\"]}"}}]}"#;

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

fn summarizer(endpoint: &str) -> OpenAiSummarizer {
    OpenAiSummarizer::new("gpt-4o-mini", Some(endpoint.to_string()), "test-key")
}

/// The request shape: bearer auth, the model in the body (unlike Azure, which
/// names it in the path), and a schema-constrained response format.
#[tokio::test]
async fn the_summarizer_sends_the_key_the_model_and_the_schema() {
    let stub = StubServer::ok(SUMMARY_RESPONSE).await;

    summarizer(&stub.endpoint)
        .summarize(&element())
        .await
        .expect("the stub answers");

    let request = stub.only_request();
    assert_eq!(request.path, "/chat/completions");
    assert_eq!(
        request.header("authorization").as_deref(),
        Some("Bearer test-key")
    );
    assert_eq!(request.body["model"], "gpt-4o-mini");

    let schema = &request.body["response_format"]["json_schema"];
    assert_eq!(request.body["response_format"]["type"], "json_schema");
    assert_eq!(schema["name"], "element_summary");
    assert_eq!(
        schema["strict"], true,
        "without strict the schema is a hint, not a constraint"
    );
    assert_eq!(
        schema["schema"]["additionalProperties"], false,
        "a closed schema is what stops the model inventing fields the parser drops"
    );
}

/// The band lives in the prompt because OpenAI's `strict` schema subset has no
/// `minItems`/`maxItems`. If that ever changes, this test is the reminder that
/// the prompt was carrying the constraint alone.
#[tokio::test]
async fn the_tag_band_is_stated_in_the_prompt_because_the_schema_cannot() {
    let stub = StubServer::ok(SUMMARY_RESPONSE).await;

    summarizer(&stub.endpoint)
        .summarize(&element())
        .await
        .expect("the stub answers");

    let request = stub.only_request();
    let schema = &request.body["response_format"]["json_schema"]["schema"];
    assert!(
        schema["properties"]["tags"].get("minItems").is_none(),
        "strict mode rejects minItems; if this now passes, move the band into the schema"
    );
    assert!(
        request.body["messages"][0]["content"]
            .as_str()
            .expect("a system prompt")
            .contains("between 1 and 5")
    );
}

/// The compat-server path. Ollama, vLLM and LM Studio all answer a schema
/// request with a client error naming the parameter; the adapter must fall
/// back to the shape they do understand rather than fail the element.
#[tokio::test]
async fn a_server_that_rejects_the_schema_falls_back_and_still_validates() {
    let stub = StubServer::rejecting_structured_output(SUMMARY_RESPONSE).await;

    let summary = summarizer(&stub.endpoint)
        .summarize(&element())
        .await
        .expect("the fallback answers");

    let formats: Vec<String> = stub
        .requests()
        .iter()
        .map(|request| request.response_format().unwrap_or("<none>").to_string())
        .collect();
    assert_eq!(formats, ["json_schema", "json_object"]);
    assert_eq!(summary.text, "Classifies a node.");
    assert!(summary.has_valid_tags(), "the fallback path validates too");
}

/// One wasted round trip per process, not per element.
#[tokio::test]
async fn the_downgrade_is_remembered_for_the_rest_of_the_process() {
    let stub = StubServer::rejecting_structured_output(SUMMARY_RESPONSE).await;
    let summarizer = summarizer(&stub.endpoint);

    for _ in 0..3 {
        summarizer
            .summarize(&element())
            .await
            .expect("the fallback answers");
    }

    let schema_attempts = stub
        .requests()
        .iter()
        .filter(|request| request.response_format() == Some("json_schema"))
        .count();
    assert_eq!(schema_attempts, 1);
    assert_eq!(stub.requests().len(), 4);
}

/// A rate limit is not a downgrade signal. Retrying it in a weaker shape would
/// hide a quota problem behind a second identical failure.
#[tokio::test]
async fn an_unrelated_rejection_does_not_trigger_the_fallback() {
    let stub = StubServer::answering(
        StatusCode::TOO_MANY_REQUESTS,
        r#"{"error":{"message":"Rate limit reached for gpt-4o-mini"}}"#,
    )
    .await;

    let message = summarizer(&stub.endpoint)
        .summarize(&element())
        .await
        .expect_err("429")
        .to_string();

    assert!(message.contains("429"), "{message}");
    assert_eq!(stub.requests().len(), 1, "a rate limit is not retried");
}

/// Unknown fields survive to `extras` through a real adapter, not just in
/// core's serde test.
#[tokio::test]
async fn an_unknown_field_in_the_reply_lands_in_extras() {
    let stub = StubServer::ok(
        r#"{"choices":[{"message":{"content":"{\"text\":\"Classifies a node.\",\"tags\":[\"classification\"],\"risk\":\"low\"}"}}]}"#,
    )
    .await;

    let summary = summarizer(&stub.endpoint)
        .summarize(&element())
        .await
        .expect("the stub answers");

    assert_eq!(summary.extras["risk"], serde_json::json!("low"));
}

/// `model@prompt_version` — and the version is the one the Azure adapter
/// shares, so a prompt change moves both keys together.
#[test]
fn the_summarizer_keys_by_model_and_prompt_version() {
    let summarizer = OpenAiSummarizer::new("gpt-4o-mini", None, "k");
    assert_eq!(
        summarizer.key(),
        format!("gpt-4o-mini@{}", OpenAiSummarizer::PROMPT_VERSION)
    );
}

/// The embedder never narrows its width, so its key is the model alone — a
/// dimension suffix here would be a number that can never vary.
#[test]
fn the_embedder_keys_by_model_alone() {
    let embedder = OpenAiEmbedder::new("text-embedding-3-small", None, "k");
    assert_eq!(embedder.key(), "text-embedding-3-small");
}

#[tokio::test]
async fn openai_cap_rejection_is_typed_with_input_index() {
    let detail =
        r#"{"error":{"message":"Invalid 'input[1]': maximum input length is 8192 tokens"}}"#;
    let stub = StubServer::answering(StatusCode::BAD_REQUEST, detail).await;
    let embedder = OpenAiEmbedder::new(
        "text-embedding-3-small",
        Some(stub.endpoint.clone()),
        "test-key",
    );

    let error = embedder
        .embed(&["alpha".to_string(), "too dense".to_string()])
        .await
        .expect_err("the provider rejects input[1]");

    let Error::InputTooLong {
        input_index,
        max_tokens,
        detail: returned,
    } = error
    else {
        panic!("cap rejection must stay typed");
    };
    assert_eq!(input_index, Some(1));
    assert_eq!(max_tokens, 8192);
    assert_eq!(returned, detail);
}

#[tokio::test]
async fn openai_cap_rejection_parses_reported_4096_limit() {
    let detail =
        r#"{"error":{"message":"Invalid 'input[0]': maximum input length is 4096 tokens"}}"#;
    let stub = StubServer::answering(StatusCode::BAD_REQUEST, detail).await;
    let embedder = OpenAiEmbedder::new(
        "future-embedding-model",
        Some(stub.endpoint.clone()),
        "test-key",
    );

    let error = embedder
        .embed(&["too dense".to_string()])
        .await
        .expect_err("the provider reports its own cap");
    assert!(matches!(
        error,
        Error::InputTooLong {
            input_index: Some(0),
            max_tokens: 4096,
            ..
        }
    ));
}

#[tokio::test]
async fn openai_unrelated_400_is_not_a_cap_rejection() {
    let stub = StubServer::answering(
        StatusCode::BAD_REQUEST,
        r#"{"error":{"message":"request body is malformed"}}"#,
    )
    .await;
    let embedder = OpenAiEmbedder::new(
        "text-embedding-3-small",
        Some(stub.endpoint.clone()),
        "test-key",
    );

    assert!(matches!(
        embedder
            .embed(&["alpha".to_string()])
            .await
            .expect_err("the body is rejected"),
        Error::Provider(_)
    ));
}
