//! Keyless recorded-wire contracts for GitHub Copilot.

use axum::http::StatusCode;
use fs3_core::{ChatMessage, ChatProvider, Element, ElementKind, Embedder, Span, Summarizer};
use fs3_providers::{
    COPILOT_API_VERSION, COPILOT_USER_AGENT, GitHubCopilotChatClient, GitHubCopilotConfig,
    GitHubCopilotCredential, GitHubCopilotEmbedder, GitHubCopilotSummarizer, list_models,
};

mod common;
use common::StubServer;

const TEST_TOKEN: &str = "fixture-copilot-token-not-a-secret";
const CHAT_REPLY: &str =
    r#"{"choices":[{"message":{"content":"answer","tool_calls":[]}}],"usage":{"total_tokens":17}}"#;
const SUMMARY_REPLY: &str = r#"{"choices":[{"finish_reason":"stop","message":{"content":"{\"text\":\"Classifies a node.\",\"tags\":[\"classification\"]}"}}]}"#;

fn config(endpoint: &str, model: &str, dimensions: Option<usize>) -> GitHubCopilotConfig {
    let credential = GitHubCopilotCredential::from_token(TEST_TOKEN, endpoint).unwrap();
    GitHubCopilotConfig::from_credential(model, dimensions, Some(4000), credential)
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

fn assert_copilot_headers(request: &common::Captured) {
    assert_eq!(
        request.header("authorization").as_deref(),
        Some("Bearer fixture-copilot-token-not-a-secret")
    );
    assert_eq!(
        request.header("user-agent").as_deref(),
        Some(COPILOT_USER_AGENT)
    );
    assert_eq!(
        request.header("x-github-api-version").as_deref(),
        Some(COPILOT_API_VERSION)
    );
    assert_eq!(
        request.header("openai-intent").as_deref(),
        Some("conversation-edits")
    );
    assert_eq!(request.header("x-initiator").as_deref(), Some("user"));
    assert!(
        request.header("copilot-integration-id").is_none(),
        "the measured API does not require the legacy integration id"
    );
}

#[tokio::test]
async fn chat_uses_the_measured_openai_wire_contract() {
    let stub = StubServer::ok(CHAT_REPLY).await;
    let client = GitHubCopilotChatClient::new(config(&stub.endpoint, "gpt-5.4", None));
    let turn = client
        .turn(&[ChatMessage::User("answer briefly".into())], &[])
        .await
        .unwrap();
    assert_eq!(turn.content.as_deref(), Some("answer"));
    assert_eq!(turn.tokens_used, Some(17));
    let request = stub.only_request();
    assert_eq!(request.path, "/chat/completions");
    assert_eq!(request.body["model"], "gpt-5.4");
    assert_eq!(request.body["max_completion_tokens"], 4000);
    assert!(request.body.get("max_tokens").is_none());
    assert_copilot_headers(&request);
}

#[tokio::test]
async fn embeddings_preserve_order_and_verify_the_configured_width() {
    let stub = StubServer::ok(
        r#"{"data":[{"index":1,"embedding":[0.0,1.0]},{"index":0,"embedding":[1.0,0.0]}]}"#,
    )
    .await;
    let embedder =
        GitHubCopilotEmbedder::new(config(&stub.endpoint, "text-embedding-3-small", Some(2)));
    let vectors = embedder
        .embed(&["first".into(), "second".into()])
        .await
        .unwrap();
    assert_eq!(vectors, vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
    assert_eq!(embedder.key(), "text-embedding-3-small@2");
    let request = stub.only_request();
    assert_eq!(request.path, "/embeddings");
    assert_eq!(request.body["dimensions"], 2);
    assert_copilot_headers(&request);
}

#[tokio::test]
async fn an_empty_success_is_not_a_summary() {
    let stub =
        StubServer::ok(r#"{"choices":[{"finish_reason":"length","message":{"content":"   "}}]}"#)
            .await;
    let summarizer = GitHubCopilotSummarizer::new(config(&stub.endpoint, "gpt-5.4-mini", None));
    let error = summarizer.summarize(&element()).await.unwrap_err();
    let message = error.to_string();
    assert!(message.contains("EMPTY summary"), "{message}");
    assert!(message.contains("raise"), "{message}");
    assert_copilot_headers(&stub.only_request());
}

#[tokio::test]
async fn model_listing_is_sorted_filters_non_chat_models_and_is_secret_free() {
    let stub = StubServer::ok(
        r#"{"object":"list","data":[{"id":"z-model","supported_endpoints":["/chat/completions"]},{"id":"responses-only","supported_endpoints":["/responses"]},{"id":"a-model","supported_endpoints":["/chat/completions","/v1/messages"]}]}"#,
    )
    .await;
    let credential = GitHubCopilotCredential::from_token(TEST_TOKEN, &stub.endpoint).unwrap();
    let listing = list_models(&credential).await.unwrap();
    assert_eq!(listing.omitted_non_chat, 1);
    assert_eq!(listing.filter, "/chat/completions");
    assert_eq!(
        listing
            .models
            .into_iter()
            .map(|model| model.id)
            .collect::<Vec<_>>(),
        ["a-model", "z-model"]
    );
    let request = stub.only_request();
    assert_eq!(request.path, "/models");
    assert!(!format!("{credential:?}").contains(TEST_TOKEN));
}

#[tokio::test]
async fn auth_failure_names_the_login_fix_without_echoing_the_token() {
    let stub =
        StubServer::answering(StatusCode::UNAUTHORIZED, r#"{"error":"bad credentials"}"#).await;
    let credential = GitHubCopilotCredential::from_token(TEST_TOKEN, &stub.endpoint).unwrap();
    let error = list_models(&credential).await.unwrap_err().to_string();
    assert!(error.contains("flowspace3 login github-copilot"), "{error}");
    assert!(!error.contains(TEST_TOKEN), "{error}");
}

#[tokio::test]
async fn structured_summary_uses_the_shared_parser() {
    let stub = StubServer::ok(SUMMARY_REPLY).await;
    let summarizer = GitHubCopilotSummarizer::new(config(&stub.endpoint, "gpt-5.4-mini", None));
    let summary = summarizer.summarize(&element()).await.unwrap();
    assert_eq!(summary.text, "Classifies a node.");
    assert!(summary.has_valid_tags());
}
