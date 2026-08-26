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
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use azure_core::{
    credentials::{AccessToken, TokenCredential, TokenRequestOptions},
    time::OffsetDateTime,
};
use fs3_core::{BlobRef, Element, ElementKind, Embedder, Summarizer};
use fs3_providers::{
    AzureCredential, AzureOpenAiConfig, AzureOpenAiEmbedder, AzureOpenAiSummarizer,
    COGNITIVE_SERVICES_SCOPE,
};

/// One request, as the service saw it.
#[derive(Clone, Debug)]
struct Captured {
    path: String,
    query: String,
    headers: HeaderMap,
    body: serde_json::Value,
}

#[derive(Clone)]
struct StubState {
    status: StatusCode,
    body: String,
    seen: Arc<Mutex<Vec<Captured>>>,
}

/// A local Azure-shaped endpoint that answers every route the same way.
struct StubServer {
    endpoint: String,
    seen: Arc<Mutex<Vec<Captured>>>,
}

impl StubServer {
    async fn answering(status: StatusCode, body: &str) -> Self {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let state = StubState {
            status,
            body: body.to_string(),
            seen: Arc::clone(&seen),
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("an ephemeral port");
        let endpoint = format!("http://{}", listener.local_addr().expect("a bound address"));

        let app = Router::new().fallback(record).with_state(state);
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        Self { endpoint, seen }
    }

    async fn ok(body: &str) -> Self {
        Self::answering(StatusCode::OK, body).await
    }

    fn requests(&self) -> Vec<Captured> {
        self.seen.lock().expect("no panicking handler").clone()
    }

    fn only_request(&self) -> Captured {
        let requests = self.requests();
        assert_eq!(requests.len(), 1, "expected exactly one request");
        requests.into_iter().next().expect("length checked")
    }
}

async fn record(State(state): State<StubState>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let bytes = to_bytes(body, usize::MAX).await.expect("a readable body");
    state.seen.lock().expect("no poisoned lock").push(Captured {
        path: parts.uri.path().to_string(),
        query: parts.uri.query().unwrap_or_default().to_string(),
        headers: parts.headers,
        body: serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    });

    Response::builder()
        .status(state.status)
        .header("content-type", "application/json")
        .body(Body::from(state.body.clone()))
        .expect("a valid response")
}

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

fn header(captured: &Captured, name: &str) -> Option<String> {
    captured
        .headers
        .get(name)
        .map(|value| value.to_str().expect("an ascii header").to_string())
}

fn texts() -> Vec<String> {
    vec!["alpha".to_string(), "beta".to_string()]
}

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
    assert_eq!(header(&request, "api-key").as_deref(), Some("test-key"));
    assert_eq!(
        header(&request, "authorization"),
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
        header(&request, "authorization").as_deref(),
        Some("Bearer token-0")
    );
    assert_eq!(
        header(&request, "api-key"),
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
        .map(|request| header(request, "authorization"))
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
    Element {
        path: "core/src/classify.rs".into(),
        blob: BlobRef::new("0123456789abcdef").expect("a valid digest"),
        ts_kind: "function_item".into(),
        kind: ElementKind::Callable,
        qualified_name: "classify".into(),
        start_line: 120,
        end_line: 127,
        text: "pub fn classify(ts_kind: &str) -> Option<ElementKind> { .. }".into(),
        has_error: false,
    }
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
    assert_eq!(request.body["response_format"]["type"], "json_object");
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
            .contains("core/src/classify.rs lines 120-127"),
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
