//! What the Azure adapter puts on the wire, proved without Azure.
//!
//! The stub is a *fake service*: a real axum server on `127.0.0.1:0` that
//! records what it was asked and answers what the test told it to. No mocking
//! framework, and nothing here needs a credential or a network.
//!
//! These tests pin the three things Azure does differently from OpenAI —
//! deployment-in-the-path, `api-version` in the query, one of two auth headers
//! — plus the error messages that turn Azure's four indistinguishable
//! rejections into four different instructions.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use axum::http::StatusCode;
use azure_core::{
    credentials::{AccessToken, TokenCredential, TokenRequestOptions},
    time::OffsetDateTime,
};
use fs3_core::{Element, ElementKind, Embedder, Error, Span, Summarizer};
use fs3_providers::{
    AzureCredential, AzureOpenAiConfig, AzureOpenAiEmbedder, AzureOpenAiSummarizer,
    COGNITIVE_SERVICES_SCOPE, OpenAiSummarizer,
};

mod common;
use common::StubServer;

/// A credential that hands out tokens the test dictates, and counts how often
/// it was asked. A fake, not a mock: it really implements the trait.
#[derive(Debug)]
struct ScriptedCredential {
    token: String,
    valid_for: std::time::Duration,
    calls: AtomicUsize,
    scopes: Mutex<Vec<String>>,
}

impl ScriptedCredential {
    fn new(token: &str, valid_for: std::time::Duration) -> Arc<Self> {
        Arc::new(Self {
            token: token.to_string(),
            valid_for,
            calls: AtomicUsize::new(0),
            scopes: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl TokenCredential for ScriptedCredential {
    async fn get_token(
        &self,
        scopes: &[&str],
        _options: Option<TokenRequestOptions<'_>>,
    ) -> azure_core::Result<AccessToken> {
        let call = self.calls.fetch_add(1, Ordering::Relaxed);
        self.scopes
            .lock()
            .expect("no poisoned lock")
            .extend(scopes.iter().map(|scope| scope.to_string()));
        Ok(AccessToken::new(
            format!("{}-{call}", self.token),
            OffsetDateTime::now_utc() + self.valid_for,
        ))
    }
}

fn config(endpoint: &str, deployment: &str, credential: AzureCredential) -> AzureOpenAiConfig {
    AzureOpenAiConfig::new(endpoint, deployment, "2024-02-01", credential)
}

fn texts() -> Vec<String> {
    vec!["alpha".to_string(), "beta".to_string()]
}

const SUMMARY_RESPONSE: &str = r#"{"choices":[{"message":{"content":"{\"text\":\"Classifies a node.\",\"tags\":[\"classification\",\"parsing\"]}"}}]}"#;

const TWO_VECTORS: &str =
    r#"{"data":[{"index":0,"embedding":[1.0,0.0]},{"index":1,"embedding":[0.0,1.0]}]}"#;

/// The whole Azure addressing scheme in one assertion: deployment in the path,
/// `api-version` in the query, `api-key` in a header, and no `model` field —
/// the deployment already said which model this is.
#[tokio::test]
async fn an_api_key_request_addresses_the_deployment_and_sends_the_key_header() {
    let stub = StubServer::ok(TWO_VECTORS).await;
    let embedder = AzureOpenAiEmbedder::new(
        config(
            &stub.endpoint,
            "text-embedding-3-small-no-rate",
            AzureCredential::api_key("test-key"),
        ),
        Some(2),
    );

    embedder.embed(&texts()).await.expect("the stub answers");

    let request = stub.only_request();
    assert_eq!(
        request.path,
        "/openai/deployments/text-embedding-3-small-no-rate/embeddings"
    );
    assert_eq!(request.query, "api-version=2024-02-01");
    assert_eq!(request.header("api-key").as_deref(), Some("test-key"));
    assert_eq!(
        request.header("authorization"),
        None,
        "the two auth modes are exclusive: a key request must not also carry a bearer token"
    );
    assert_eq!(request.body["input"][0], "alpha");
    assert_eq!(request.body["input"][1], "beta");
    assert_eq!(request.body["dimensions"], 2);
    assert!(
        request.body.get("model").is_none(),
        "the deployment names the model on Azure: {}",
        request.body
    );
}

/// `dimensions` is omitted rather than sent as null when the caller has no
/// opinion — some deployments reject the key outright.
#[tokio::test]
async fn no_requested_dimensions_means_no_dimensions_field() {
    let stub = StubServer::ok(TWO_VECTORS).await;
    let embedder = AzureOpenAiEmbedder::new(
        config(&stub.endpoint, "d", AzureCredential::api_key("k")),
        None,
    );

    embedder.embed(&texts()).await.expect("the stub answers");

    let body = stub.only_request().body;
    assert!(
        body.get("dimensions").is_none(),
        "dimensions must be absent, not null: {body}"
    );
}

/// The Entra arm: a bearer token, the public-cloud scope, and no `api-key`.
#[tokio::test]
async fn an_entra_request_sends_a_bearer_token_for_the_cognitive_services_scope() {
    let stub = StubServer::ok(TWO_VECTORS).await;
    let credential = ScriptedCredential::new("token", std::time::Duration::from_secs(3600));
    let embedder = AzureOpenAiEmbedder::new(
        config(
            &stub.endpoint,
            "d",
            AzureCredential::entra(Arc::clone(&credential) as Arc<dyn TokenCredential>),
        ),
        None,
    );

    embedder.embed(&texts()).await.expect("the stub answers");

    let request = stub.only_request();
    assert_eq!(
        request.header("authorization").as_deref(),
        Some("Bearer token-0")
    );
    assert_eq!(
        request.header("api-key"),
        None,
        "the two auth modes are exclusive"
    );
    assert_eq!(
        credential
            .scopes
            .lock()
            .expect("no poisoned lock")
            .as_slice(),
        [COGNITIVE_SERVICES_SCOPE.to_string()],
        "a wrong scope yields a token the resource refuses"
    );
}

/// A live token is reused. `AzureCliCredential` shells out to `az` and caches
/// nothing, so without this a batch of calls would spawn a process per call.
#[tokio::test]
async fn a_live_entra_token_is_reused_across_calls() {
    let stub = StubServer::ok(TWO_VECTORS).await;
    let credential = ScriptedCredential::new("token", std::time::Duration::from_secs(3600));
    let embedder = AzureOpenAiEmbedder::new(
        config(
            &stub.endpoint,
            "d",
            AzureCredential::entra(Arc::clone(&credential) as Arc<dyn TokenCredential>),
        ),
        None,
    );

    embedder.embed(&texts()).await.expect("first call");
    embedder.embed(&texts()).await.expect("second call");

    assert_eq!(stub.requests().len(), 2, "both calls reached the service");
    assert_eq!(
        credential.calls(),
        1,
        "the second call must reuse the cached token"
    );
}

/// A token about to expire is replaced. Caching that ignores expiry is worse
/// than no caching: it fails only after the process has been up an hour.
#[tokio::test]
async fn an_almost_expired_entra_token_is_refreshed() {
    let stub = StubServer::ok(TWO_VECTORS).await;
    // Inside the refresh skew, so it is never handed out twice.
    let credential = ScriptedCredential::new("token", std::time::Duration::from_secs(30));
    let embedder = AzureOpenAiEmbedder::new(
        config(
            &stub.endpoint,
            "d",
            AzureCredential::entra(Arc::clone(&credential) as Arc<dyn TokenCredential>),
        ),
        None,
    );

    embedder.embed(&texts()).await.expect("first call");
    embedder.embed(&texts()).await.expect("second call");

    assert_eq!(credential.calls(), 2, "a stale token must be re-acquired");
    let tokens: Vec<Option<String>> = stub
        .requests()
        .iter()
        .map(|request| request.header("authorization"))
        .collect();
    assert_eq!(
        tokens,
        vec![
            Some("Bearer token-0".to_string()),
            Some("Bearer token-1".to_string())
        ]
    );
}

/// The response is a mapping keyed by `index`, not a list, so the adapter must
/// place vectors rather than trust arrival order.
#[tokio::test]
async fn vectors_come_back_in_input_order_whatever_order_azure_sends_them() {
    let stub = StubServer::ok(
        r#"{"data":[{"index":1,"embedding":[0.0,1.0]},{"index":0,"embedding":[1.0,0.0]}]}"#,
    )
    .await;
    let embedder = AzureOpenAiEmbedder::new(
        config(&stub.endpoint, "d", AzureCredential::api_key("k")),
        None,
    );

    let vectors = embedder.embed(&texts()).await.expect("the stub answers");

    assert_eq!(vectors, vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
}

/// A deployment that ignores `dimensions` would otherwise fill a store column
/// sized from that number with vectors of a different width.
#[tokio::test]
async fn a_deployment_that_ignores_requested_dimensions_is_rejected() {
    let stub = StubServer::ok(TWO_VECTORS).await;
    let embedder = AzureOpenAiEmbedder::new(
        config(&stub.endpoint, "d", AzureCredential::api_key("k")),
        Some(1024),
    );

    let error = embedder
        .embed(&texts())
        .await
        .expect_err("the stub returns 2 dimensions, not 1024");

    let message = error.to_string();
    assert!(message.contains("1024"), "{message}");
    assert!(
        message.contains("ignored the `dimensions` request"),
        "{message}"
    );
}

/// Azure answers a wrong deployment name with a 404 that reads like a wrong
/// URL. The error has to say which of the two it is.
#[tokio::test]
async fn a_missing_deployment_is_named_in_the_error() {
    let stub = StubServer::answering(
        StatusCode::NOT_FOUND,
        r#"{"error":{"code":"DeploymentNotFound","message":"The API deployment for this resource does not exist."}}"#,
    )
    .await;
    let embedder = AzureOpenAiEmbedder::new(
        config(&stub.endpoint, "gpt-4o-typo", AzureCredential::api_key("k")),
        None,
    );

    let message = embedder.embed(&texts()).await.expect_err("404").to_string();

    assert!(message.contains("gpt-4o-typo"), "{message}");
    assert!(
        message.contains("DEPLOYMENT name"),
        "the fix is to check the deployment, not the model: {message}"
    );
}

/// The rejection this repo actually hit: the resource has key auth switched
/// off, and answers 403 — not 401 — to a perfectly valid key.
#[tokio::test]
async fn key_authentication_being_disabled_points_at_entra() {
    let stub = StubServer::answering(
        StatusCode::FORBIDDEN,
        r#"{"error":{"code":"AuthenticationTypeDisabled","message":"Key based authentication is disabled for this resource."}}"#,
    )
    .await;
    let embedder = AzureOpenAiEmbedder::new(
        config(&stub.endpoint, "d", AzureCredential::api_key("k")),
        None,
    );

    let message = embedder.embed(&texts()).await.expect_err("403").to_string();

    assert!(message.contains("key authentication disabled"), "{message}");
    assert!(
        message.contains("Entra"),
        "the error must name the way in: {message}"
    );
}

#[tokio::test]
async fn a_rejected_credential_names_both_ways_to_fix_it() {
    let stub = StubServer::answering(
        StatusCode::UNAUTHORIZED,
        r#"{"error":{"code":"401","message":"Access denied due to invalid subscription key."}}"#,
    )
    .await;
    let embedder = AzureOpenAiEmbedder::new(
        config(&stub.endpoint, "d", AzureCredential::api_key("k")),
        None,
    );

    let message = embedder.embed(&texts()).await.expect_err("401").to_string();

    assert!(message.contains("api-key"), "{message}");
    assert!(message.contains("az login"), "{message}");
}

#[tokio::test]
async fn azure_cap_rejection_is_typed_with_input_index() {
    let detail =
        r#"{"error":{"message":"Invalid 'input[1]': maximum input length is 8192 tokens"}}"#;
    let stub = StubServer::answering(StatusCode::BAD_REQUEST, detail).await;
    let embedder = AzureOpenAiEmbedder::new(
        config(&stub.endpoint, "d", AzureCredential::api_key("k")),
        None,
    );

    let error = embedder
        .embed(&texts())
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
async fn azure_unrelated_400_is_not_a_cap_rejection() {
    let stub = StubServer::answering(
        StatusCode::BAD_REQUEST,
        r#"{"error":{"message":"request body is malformed"}}"#,
    )
    .await;
    let embedder = AzureOpenAiEmbedder::new(
        config(&stub.endpoint, "d", AzureCredential::api_key("k")),
        None,
    );

    assert!(matches!(
        embedder
            .embed(&texts())
            .await
            .expect_err("the body is rejected"),
        Error::Provider(_)
    ));
}

/// A credential that cannot produce a token must fail with an instruction, not
/// with an unauthenticated request that returns a confusing 401.
#[tokio::test]
async fn a_credential_that_cannot_get_a_token_fails_before_the_request() {
    #[derive(Debug)]
    struct RefusingCredential;

    #[async_trait]
    impl TokenCredential for RefusingCredential {
        async fn get_token(
            &self,
            _scopes: &[&str],
            _options: Option<TokenRequestOptions<'_>>,
        ) -> azure_core::Result<AccessToken> {
            Err(azure_core::Error::with_message(
                azure_core::error::ErrorKind::Credential,
                "no signed-in account",
            ))
        }
    }

    let stub = StubServer::ok(TWO_VECTORS).await;
    let embedder = AzureOpenAiEmbedder::new(
        config(
            &stub.endpoint,
            "d",
            AzureCredential::entra(Arc::new(RefusingCredential)),
        ),
        None,
    );

    let message = embedder
        .embed(&texts())
        .await
        .expect_err("the credential refuses")
        .to_string();

    assert!(message.contains("az login"), "{message}");
    assert!(
        stub.requests().is_empty(),
        "an unauthenticated request must not be sent"
    );
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

/// The summarizer's request: the chat route under the same deployment scheme,
/// JSON mode asked for explicitly, and the element's address in the prompt.
#[tokio::test]
async fn the_summarizer_asks_azure_for_json_and_parses_what_comes_back() {
    let stub = StubServer::ok(
        r#"{"choices":[{"message":{"content":"{\"text\":\"Classifies a node.\",\"tags\":[\"classification\",\"parsing\"]}"}}]}"#,
    )
    .await;
    let summarizer = AzureOpenAiSummarizer::new(config(
        &stub.endpoint,
        "gpt-4o-deployment",
        AzureCredential::api_key("k"),
    ));

    let summary = summarizer
        .summarize(&element())
        .await
        .expect("the stub answers");

    let request = stub.only_request();
    assert_eq!(
        request.path,
        "/openai/deployments/gpt-4o-deployment/chat/completions"
    );
    assert_eq!(request.body["response_format"]["type"], "json_schema");
    assert_eq!(request.body["messages"][0]["role"], "system");
    assert!(
        request.body["messages"][0]["content"]
            .as_str()
            .expect("a system prompt")
            .contains("between 1 and 5"),
        "PRD req 36's band must be stated in the request: {}",
        request.body
    );
    assert!(
        request.body["messages"][1]["content"]
            .as_str()
            .expect("a user prompt")
            .contains("core/src/classify.rs::classify lines 120-127"),
        "{}",
        request.body
    );

    assert_eq!(summary.text, "Classifies a node.");
    assert_eq!(summary.tags, vec!["classification", "parsing"]);
}

/// Prose where JSON was demanded is a provider error, not a summary with a
/// paragraph where its tags should be.
#[tokio::test]
async fn a_non_json_summary_is_rejected_at_the_boundary() {
    let stub =
        StubServer::ok(r#"{"choices":[{"message":{"content":"Sure! Here is a summary."}}]}"#).await;
    let summarizer =
        AzureOpenAiSummarizer::new(config(&stub.endpoint, "d", AzureCredential::api_key("k")));

    let message = summarizer
        .summarize(&element())
        .await
        .expect_err("not JSON")
        .to_string();

    assert!(message.contains("not the requested JSON"), "{message}");
}

/// Structured outputs: the schema is what makes "the model replied with prose"
/// stop being a failure class, so the request must actually carry it — named,
/// strict, and closed to fields the parser does not know.
#[tokio::test]
async fn the_summarizer_asks_for_a_schema_constrained_response() {
    let stub = StubServer::ok(SUMMARY_RESPONSE).await;
    let summarizer =
        AzureOpenAiSummarizer::new(config(&stub.endpoint, "d", AzureCredential::api_key("k")));

    summarizer
        .summarize(&element())
        .await
        .expect("the stub answers");

    let schema = &stub.only_request().body["response_format"]["json_schema"];
    assert_eq!(schema["name"], "element_summary");
    assert_eq!(
        schema["strict"], true,
        "without strict the schema is a hint, not a constraint"
    );
    assert_eq!(
        schema["schema"]["required"],
        serde_json::json!(["text", "tags"])
    );
    assert_eq!(schema["schema"]["additionalProperties"], false);
    assert_eq!(
        schema["schema"]["properties"]["tags"]["items"]["type"],
        "string"
    );
}

/// An `api-version` too old for structured outputs answers 400. The adapter
/// must retry in the shape that endpoint understands rather than fail — and
/// the retry must still be validated, which is what makes the downgrade safe.
#[tokio::test]
async fn a_deployment_that_rejects_the_schema_falls_back_and_still_validates() {
    let stub = StubServer::rejecting_structured_output(SUMMARY_RESPONSE).await;
    let summarizer =
        AzureOpenAiSummarizer::new(config(&stub.endpoint, "d", AzureCredential::api_key("k")));

    let summary = summarizer
        .summarize(&element())
        .await
        .expect("the fallback answers");

    let formats: Vec<String> = stub
        .requests()
        .iter()
        .map(|request| request.response_format().unwrap_or("<none>").to_string())
        .collect();
    assert_eq!(
        formats,
        ["json_schema", "json_object"],
        "the schema is tried first, then the shape everything understands"
    );
    assert_eq!(summary.text, "Classifies a node.");
    assert!(summary.has_valid_tags(), "the fallback path validates too");
}

/// The downgrade is remembered. An endpoint that will never understand
/// structured outputs must cost one wasted round trip per process, not one per
/// element — at scan scale that difference is the whole feature.
#[tokio::test]
async fn the_downgrade_is_remembered_for_the_rest_of_the_process() {
    let stub = StubServer::rejecting_structured_output(SUMMARY_RESPONSE).await;
    let summarizer =
        AzureOpenAiSummarizer::new(config(&stub.endpoint, "d", AzureCredential::api_key("k")));

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
    assert_eq!(schema_attempts, 1, "the doomed request must be tried once");
    assert_eq!(
        stub.requests().len(),
        4,
        "one rejection, then three summaries"
    );
}

/// A rejection that is NOT about the response format must not be laundered
/// into a downgrade: retrying a 404 in a weaker shape would turn a wrong
/// deployment name into a second identical failure and a confusing error.
#[tokio::test]
async fn an_unrelated_rejection_does_not_trigger_the_fallback() {
    let stub = StubServer::answering(
        StatusCode::NOT_FOUND,
        r#"{"error":{"code":"DeploymentNotFound","message":"The API deployment for this resource does not exist."}}"#,
    )
    .await;
    let summarizer = AzureOpenAiSummarizer::new(config(
        &stub.endpoint,
        "gpt-4o-typo",
        AzureCredential::api_key("k"),
    ));

    let message = summarizer
        .summarize(&element())
        .await
        .expect_err("404")
        .to_string();

    assert!(message.contains("gpt-4o-typo"), "{message}");
    assert_eq!(stub.requests().len(), 1, "a 404 is not retried");
}

/// A field the typed contract has never heard of survives to `extras` instead
/// of being dropped — the promise `Summary::extras` makes, proved through a
/// real adapter rather than only in core's own serde test.
#[tokio::test]
async fn an_unknown_field_in_the_reply_lands_in_extras() {
    let stub = StubServer::ok(
        r#"{"choices":[{"message":{"content":"{\"text\":\"Classifies a node.\",\"tags\":[\"classification\"],\"complexity\":7}"}}]}"#,
    )
    .await;
    let summarizer =
        AzureOpenAiSummarizer::new(config(&stub.endpoint, "d", AzureCredential::api_key("k")));

    let summary = summarizer
        .summarize(&element())
        .await
        .expect("the stub answers");

    assert_eq!(summary.text, "Classifies a node.");
    assert_eq!(summary.extras["complexity"], serde_json::json!(7));
}

/// The enrichment row key. The DEPLOYMENT names it, because that is what
/// served the request; the width joins it only when one was asked for, because
/// only then is there a second vector space to tell apart.
#[test]
fn the_embedder_keys_by_deployment_and_requested_width() {
    let native = AzureOpenAiEmbedder::new(
        config(
            "https://example.openai.azure.com",
            "text-embedding-3-small-no-rate",
            AzureCredential::api_key("k"),
        ),
        None,
    );
    let narrowed = AzureOpenAiEmbedder::new(
        config(
            "https://example.openai.azure.com",
            "text-embedding-3-small-no-rate",
            AzureCredential::api_key("k"),
        ),
        Some(1024),
    );

    assert_eq!(native.key(), "text-embedding-3-small-no-rate");
    assert_eq!(narrowed.key(), "text-embedding-3-small-no-rate@1024");
    assert_ne!(
        native.key(),
        narrowed.key(),
        "1024-wide vectors live in a different space from native ones and must not share a key"
    );
}

/// The summarizer's key carries the prompt version, and it is the SHARED one:
/// Azure and OpenAI send the same prompt and the same schema, so a prompt
/// change must move both keys or one of them is lying.
#[test]
fn the_summarizer_keys_by_deployment_and_shared_prompt_version() {
    let summarizer = AzureOpenAiSummarizer::new(config(
        "https://example.openai.azure.com",
        "gpt-4o-deployment",
        AzureCredential::api_key("k"),
    ));

    assert_eq!(
        summarizer.key(),
        format!("gpt-4o-deployment@{}", OpenAiSummarizer::PROMPT_VERSION)
    );
}
