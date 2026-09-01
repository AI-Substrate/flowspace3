//! What the OpenAI-compatible adapter puts on the wire, proved without a LAN.
//!
//! The assertions here are about the three ways a generic compat endpoint
//! differs from a cloud one: the model is DISCOVERED rather than configured,
//! embeddings may not exist at all, and a reasoning model can answer with
//! nothing while reporting success.
//!
//! The stub is a fake service — see `tests/common/mod.rs`. No key, no network.

use axum::http::StatusCode;
use fs3_core::{
    ChatMessage, ChatProvider, Element, ElementKind, Embedder, Error, Span, Summarizer,
};
use fs3_providers::{
    DEFAULT_MAX_TOKENS, OpenAiCompatChatClient, OpenAiCompatConfig, OpenAiCompatEmbedder,
    OpenAiCompatSummarizer, OpenAiSummarizer, embeddings_unsupported,
};

mod common;
use common::StubServer;

const MODEL_ID: &str = "/models/Qwen3.8-27B-ABLITERATED-Q5_K_M.gguf";

fn models_reply() -> String {
    format!(r#"{{"object":"list","data":[{{"id":"{MODEL_ID}","object":"model"}}]}}"#)
}

const SUMMARY_REPLY: &str = r#"{"choices":[{"finish_reason":"stop","message":{"content":"{\"text\":\"Classifies a node.\",\"tags\":[\"classification\"]}"}}]}"#;

/// The server that answers `/models` with one loaded model and everything else
/// with `chat`.
async fn server(chat: &'static str) -> StubServer {
    StubServer::answering_with(move |request| {
        if request.path.ends_with("/models") {
            (StatusCode::OK, models_reply())
        } else {
            (StatusCode::OK, chat.to_string())
        }
    })
    .await
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

async fn connect(endpoint: &str) -> OpenAiCompatSummarizer {
    OpenAiCompatSummarizer::connect(OpenAiCompatConfig::new(format!("{endpoint}/v1")))
        .await
        .expect("the stub reports a model")
}

/// Connecting reads which model is actually loaded. These servers ignore the
/// requested model, so this round trip is the only way to know what will
/// answer — and it doubles as the readiness probe.
#[tokio::test]
async fn connecting_discovers_the_served_model() {
    let stub = server(SUMMARY_REPLY).await;

    let summarizer = connect(&stub.endpoint).await;

    assert_eq!(summarizer.served_model(), MODEL_ID);
    assert_eq!(stub.only_request().path, "/v1/models");
}

/// The key names the WEIGHTS the server reported, not the model config wished
/// for — and the quant is in that name, which is the whole point.
#[tokio::test]
async fn the_key_is_the_served_model_and_the_prompt_version() {
    let stub = server(SUMMARY_REPLY).await;

    let summarizer = connect(&stub.endpoint).await;

    assert_eq!(
        summarizer.key(),
        format!("{MODEL_ID}@{}", OpenAiSummarizer::PROMPT_VERSION)
    );
    assert!(
        summarizer.key().contains("Q5_K_M"),
        "the quantisation is part of the identity: {}",
        summarizer.key()
    );
}

/// The request: a token budget big enough for a reasoning model, the shared
/// prompt, and the schema attempt.
#[tokio::test]
async fn the_request_carries_a_budget_the_prompt_and_the_schema() {
    let stub = server(SUMMARY_REPLY).await;

    connect(&stub.endpoint)
        .await
        .summarize(&element())
        .await
        .expect("the stub answers");

    let chat = stub
        .requests()
        .into_iter()
        .find(|request| request.path.ends_with("/chat/completions"))
        .expect("a chat request");
    assert_eq!(chat.body["max_tokens"], DEFAULT_MAX_TOKENS);
    assert_eq!(chat.response_format(), Some("json_schema"));
    assert!(
        chat.body["messages"][0]["content"]
            .as_str()
            .expect("a system prompt")
            .contains("between 1 and 5")
    );
}

/// No key configured means no `authorization` header. Most of these servers
/// have no auth at all, and sending a placeholder bearer to one that DOES
/// check would fail in a way that reads like a broken key.
#[tokio::test]
async fn no_configured_key_means_no_authorization_header() {
    let stub = server(SUMMARY_REPLY).await;

    connect(&stub.endpoint)
        .await
        .summarize(&element())
        .await
        .expect("the stub answers");

    for request in stub.requests() {
        assert_eq!(
            request.header("authorization"),
            None,
            "{} carried auth it was never given",
            request.path
        );
    }
}

/// **The expensive one.** A reasoning model spends the shared budget on its
/// thinking and returns HTTP 200, `finish_reason: "length"`, empty content, no
/// error. An empty summary must never leave the boundary as a success — it
/// would write blank enrichment rows that look fine for ever.
#[tokio::test]
async fn an_empty_answer_with_no_error_is_a_named_failure() {
    let stub = server(r#"{"choices":[{"finish_reason":"length","message":{"content":"","reasoning_content":"We need to answer the user's request. The function appears to"}}]}"#).await;

    let message = connect(&stub.endpoint)
        .await
        .summarize(&element())
        .await
        .expect_err("an empty answer is a failure, not a summary")
        .to_string();

    assert!(message.contains("EMPTY summary"), "{message}");
    assert!(
        message.contains("max_tokens"),
        "the error must name the fix: {message}"
    );
    assert!(
        message.contains(&DEFAULT_MAX_TOKENS.to_string()),
        "the error must name the current budget: {message}"
    );
    assert!(
        message.contains("Q5_K_M"),
        "the error must name what answered: {message}"
    );
}

/// Whitespace is not an answer either — a model that replies with a newline
/// must fail the same way, not produce a summary whose text is blank.
#[tokio::test]
async fn a_whitespace_only_answer_is_also_refused() {
    let stub =
        server(r#"{"choices":[{"finish_reason":"stop","message":{"content":"  \n  "}}]}"#).await;

    let message = connect(&stub.endpoint)
        .await
        .summarize(&element())
        .await
        .expect_err("whitespace is not a summary")
        .to_string();

    assert!(message.contains("EMPTY summary"), "{message}");
    assert!(
        message.contains("finished with reason `stop`"),
        "a blank answer that claims to have finished is a different diagnosis: {message}"
    );
}

/// The compat servers that predate structured outputs get the same remembered
/// downgrade the cloud adapters use.
#[tokio::test]
async fn a_server_that_rejects_the_schema_falls_back_and_still_validates() {
    let stub = StubServer::answering_with(move |request| {
        if request.path.ends_with("/models") {
            (StatusCode::OK, models_reply())
        } else if request.response_format() == Some("json_schema") {
            (
                StatusCode::BAD_REQUEST,
                r#"{"error":{"message":"response_format.json_schema is not supported"}}"#
                    .to_string(),
            )
        } else {
            (StatusCode::OK, SUMMARY_REPLY.to_string())
        }
    })
    .await;
    let summarizer = connect(&stub.endpoint).await;

    let summary = summarizer
        .summarize(&element())
        .await
        .expect("the fallback answers");
    summarizer
        .summarize(&element())
        .await
        .expect("and is remembered");

    let attempts: Vec<String> = stub
        .requests()
        .iter()
        .filter(|request| request.path.ends_with("/chat/completions"))
        .map(|request| request.response_format().unwrap_or("<none>").to_string())
        .collect();
    assert_eq!(
        attempts,
        ["json_schema", "json_object", "json_object"],
        "one doomed attempt, then the shape that works, for ever"
    );
    assert!(summary.has_valid_tags(), "the fallback path validates too");
}

/// A server that is up but still loading answers with an empty model list. The
/// caller must be told to wait rather than left to interpret a later timeout.
#[tokio::test]
async fn a_server_with_no_model_loaded_says_so() {
    let stub = StubServer::ok(r#"{"object":"list","data":[]}"#).await;

    let message =
        OpenAiCompatSummarizer::connect(OpenAiCompatConfig::new(format!("{}/v1", stub.endpoint)))
            .await
            .expect_err("no model is loaded")
            .to_string();

    assert!(message.contains("reports no models"), "{message}");
    assert!(message.contains("loaded"), "{message}");
}

/// An unreachable endpoint must name the box and the likely reason, because
/// these endpoints are LAN-only and "connection refused" alone sends the
/// reader looking at the wrong layer.
#[tokio::test]
async fn an_unreachable_endpoint_names_the_network() {
    // Port 1 on loopback: nothing listens, and nothing can be started there.
    let message = OpenAiCompatSummarizer::connect(OpenAiCompatConfig::new("http://127.0.0.1:1/v1"))
        .await
        .expect_err("nothing listens on port 1")
        .to_string();

    assert!(message.contains("127.0.0.1:1"), "{message}");
    assert!(message.contains("LAN-only"), "{message}");
}

/// Embeddings are refused at wiring time, with somewhere else to go.
#[test]
fn an_embedder_pointed_here_is_refused_with_an_alternative() {
    let message = embeddings_unsupported("http://192.168.1.134:8080/v1").to_string();
    assert!(message.contains("summarizer-only"), "{message}");
    assert!(message.contains("local embedder"), "{message}");
}

#[tokio::test]
async fn hosted_embeddings_send_model_and_dimensions_and_verify_the_width() {
    let stub = StubServer::ok(r#"{"data":[{"index":0,"embedding":[0.1,0.2,0.3]}]}"#).await;
    let embedder = OpenAiCompatEmbedder::new(
        OpenAiCompatConfig::new(format!("{}/v1", stub.endpoint))
            .with_model("acme/embed")
            .with_dimensions(3),
    );

    let vectors = embedder
        .embed(&["hello".to_string()])
        .await
        .expect("the recorded OpenAI embedding shape is accepted");

    assert_eq!(vectors, vec![vec![0.1, 0.2, 0.3]]);
    assert_eq!(embedder.key(), "acme/embed@3");
    let request = stub.only_request();
    assert_eq!(request.path, "/v1/embeddings");
    assert_eq!(request.body["model"], "acme/embed");
    assert_eq!(request.body["dimensions"], 3);
}

#[tokio::test]
async fn hosted_embeddings_refuse_a_width_other_than_the_configured_space() {
    let stub = StubServer::ok(r#"{"data":[{"index":0,"embedding":[0.1,0.2]}]}"#).await;
    let embedder = OpenAiCompatEmbedder::new(
        OpenAiCompatConfig::new(format!("{}/v1", stub.endpoint))
            .with_model("acme/embed")
            .with_dimensions(3),
    );

    let message = embedder
        .embed(&["hello".to_string()])
        .await
        .expect_err("a different vector space must never reach the index")
        .to_string();
    assert!(
        message.contains("returned 2 dimensions, configured 3"),
        "{message}"
    );
}

#[tokio::test]
async fn openai_compat_cap_rejection_is_typed_with_input_index() {
    let detail =
        r#"{"error":{"message":"Invalid 'input[1]': maximum input length is 8192 tokens"}}"#;
    let stub = StubServer::answering(StatusCode::BAD_REQUEST, detail).await;
    let embedder = OpenAiCompatEmbedder::new(
        OpenAiCompatConfig::new(format!("{}/v1", stub.endpoint)).with_model("acme/embed"),
    );

    let error = embedder
        .embed(&["alpha".to_string(), "too dense".to_string()])
        .await
        .expect_err("the provider rejects input[1]");
    assert!(matches!(
        error,
        Error::InputTooLong {
            input_index: Some(1),
            max_tokens: 8192,
            ..
        }
    ));
}

#[tokio::test]
async fn openai_compat_unrelated_400_is_not_a_cap_rejection() {
    let stub = StubServer::answering(
        StatusCode::BAD_REQUEST,
        r#"{"error":{"message":"request body is malformed"}}"#,
    )
    .await;
    let embedder = OpenAiCompatEmbedder::new(
        OpenAiCompatConfig::new(format!("{}/v1", stub.endpoint)).with_model("acme/embed"),
    );

    assert!(matches!(
        embedder
            .embed(&["alpha".to_string()])
            .await
            .expect_err("the body is rejected"),
        Error::Provider(_)
    ));
}

#[tokio::test]
async fn hosted_chat_propagates_reported_total_usage() {
    let stub = StubServer::ok(
        r#"{"choices":[{"message":{"content":"done"}}],"usage":{"prompt_tokens":7,"completion_tokens":4,"total_tokens":11}}"#,
    )
    .await;
    let chat = OpenAiCompatChatClient::new(
        OpenAiCompatConfig::new(format!("{}/v1", stub.endpoint)).with_model("acme/chat"),
    );

    let turn = chat
        .turn(&[ChatMessage::User("answer".into())], &[])
        .await
        .expect("the recorded OpenAI chat shape is accepted");

    assert_eq!(turn.content.as_deref(), Some("done"));
    assert_eq!(turn.tokens_used, Some(11));
    assert_eq!(chat.key(), "acme/chat");
    let request = stub.only_request();
    assert_eq!(request.path, "/v1/chat/completions");
    assert_eq!(request.body["model"], "acme/chat");
}

#[tokio::test]
async fn hosted_chat_without_usage_stays_unknown_not_zero() {
    let stub = StubServer::ok(r#"{"choices":[{"message":{"content":"done"}}]}"#).await;
    let chat = OpenAiCompatChatClient::new(
        OpenAiCompatConfig::new(format!("{}/v1", stub.endpoint)).with_model("acme/chat"),
    );

    let turn = chat
        .turn(&[ChatMessage::User("answer".into())], &[])
        .await
        .expect("usage is optional in the OpenAI shape");

    assert_eq!(turn.tokens_used, None);
}
