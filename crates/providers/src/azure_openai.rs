//! The Azure OpenAI HTTP adapter for both ports.
//!
//! Azure serves the same wire shapes as OpenAI behind a different address and a
//! different door:
//!
//! - the model is named by a **deployment** in the URL path, not by a `model`
//!   field in the body — `{endpoint}/openai/deployments/{deployment}/{route}`;
//! - every request carries an **`api-version`** query parameter, and the value
//!   differs per route in practice (chat moves faster than embeddings);
//! - authentication is **either** an `api-key` header **or** an Entra bearer
//!   token, never both — and a resource can have key auth switched off
//!   entirely, which is a 403 rather than a 401.
//!
//! Those three facts are the whole adapter. Everything else is the OpenAI
//! response shape, which is why the summarizer borrows [`OpenAiSummarizer`]'s
//! prompt verbatim rather than growing a second one that can drift from it.
//!
//! ## Snap-in
//!
//! Wiring happens at adoption, by the integrating stream — this crate holds
//! adapters only, and never reads config. The recipe, for whoever does it:
//!
//! ```ignore
//! // fs3-core::config — one new variant
//! pub enum ProviderConfig {
//!     Fake,
//!     OpenAi { model: String, api_base: Option<String>, api_key_env: String },
//!     AzureOpenAi {
//!         endpoint: String,           // resource root, e.g. https://X.openai.azure.com
//!         deployment: String,         // the deployment name, NOT the model name
//!         api_version: String,        // e.g. "2024-12-01-preview" (chat) / "2024-02-01" (embeddings)
//!         /// `Some(var)` reads the key from that environment variable;
//!         /// `None` authenticates with Entra (managed identity, then `az login`).
//!         api_key_env: Option<String>,
//!         dimensions: Option<usize>,  // embeddings only; Azure honours it
//!     },
//! }
//!
//! // fs3-daemon composition root — one new match arm
//! ProviderConfig::AzureOpenAi { endpoint, deployment, api_version, api_key_env, dimensions } => {
//!     let credential = match api_key_env {
//!         Some(var) => AzureCredential::api_key_from_env(var)?,
//!         None => AzureCredential::from_environment()?,
//!     };
//!     let config = AzureOpenAiConfig::new(endpoint, deployment, api_version, credential);
//!     (
//!         Arc::new(AzureOpenAiEmbedder::new(config.clone(), dimensions)) as Arc<dyn Embedder>,
//!         Arc::new(AzureOpenAiSummarizer::new(config)) as Arc<dyn Summarizer>,
//!     )
//! }
//! ```
//!
//! The two ports usually want *different* deployments and api-versions, so a
//! real config carries one [`AzureOpenAiConfig`] per port rather than sharing
//! one — the struct is deliberately cheap enough to build twice.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use azure_core::{
    credentials::{AccessToken, Secret, TokenCredential},
    time::OffsetDateTime,
};
use fs3_core::{Element, Embedder, Error, Result, Summarizer, Summary};
use serde::{Deserialize, Serialize};

use crate::{
    OpenAiEmbedder, OpenAiSummarizer,
    retry::{self, PostFailure, Rejection, RetryPolicy},
};

/// The Entra scope every Azure OpenAI resource in the public cloud is guarded
/// by. Sovereign clouds use a different one; a resource in those needs its own
/// scope, which is why this is public and not buried.
pub const COGNITIVE_SERVICES_SCOPE: &str = "https://cognitiveservices.azure.com/.default";

/// A tested `api-version` for the chat-completions route.
///
/// Azure pins behaviour to this string, so it is data the caller supplies, not
/// a constant baked into the request. These two exist so a caller who has no
/// opinion still starts from a value that is known to serve
/// `response_format: json_object`.
pub const CHAT_API_VERSION: &str = "2024-12-01-preview";

/// A tested `api-version` for the embeddings route.
pub const EMBEDDINGS_API_VERSION: &str = "2024-02-01";

/// Refresh an Entra token this long before it actually expires, so a request
/// that is *issued* just inside the window cannot *arrive* just outside it.
const TOKEN_REFRESH_SKEW: std::time::Duration = std::time::Duration::from_secs(300);

/// How the adapter proves who it is.
///
/// The two arms are mutually exclusive at the wire — Azure reads one header or
/// the other — so they are one enum rather than two optional fields that could
/// both be set or both be empty.
#[derive(Clone, Debug)]
pub enum AzureCredential {
    /// The resource's `api-key` header. Held in [`azure_core`]'s [`Secret`],
    /// whose `Debug` prints nothing.
    ApiKey(Secret),
    /// An Entra credential chain. `azure_identity` owns the acquisition —
    /// hand-rolling the OAuth flow here would be reinventing an SDK that
    /// already handles managed identity, workload identity and `az login`.
    Entra(Arc<dyn TokenCredential>),
}

impl AzureCredential {
    /// Use `value` as the `api-key` header.
    pub fn api_key(value: impl Into<String>) -> Self {
        Self::ApiKey(Secret::new(value.into()))
    }

    /// Read the `api-key` value out of the environment variable named `var`.
    ///
    /// Config carries the *name*; the value is read here, once, and never
    /// stored or logged. A missing variable is an error that names it.
    pub fn api_key_from_env(var: &str) -> Result<Self> {
        let value = std::env::var(var).map_err(|_| {
            Error::Provider(format!(
                "Azure OpenAI: environment variable {var} is not set; export it with the \
                 resource's key, or configure Entra authentication instead"
            ))
        })?;
        if value.trim().is_empty() {
            return Err(Error::Provider(format!(
                "Azure OpenAI: environment variable {var} is set but empty; export the \
                 resource's key, or configure Entra authentication instead"
            )));
        }
        Ok(Self::api_key(value))
    }

    /// Authenticate with Entra using the ambient environment: managed identity
    /// where one exists, otherwise a signed-in `az login`.
    ///
    /// This is the arm to use against a resource with key authentication
    /// disabled — a common, and correct, hardening.
    pub fn from_environment() -> Result<Self> {
        let credential = azure_identity::DeveloperToolsCredential::new(None).map_err(|e| {
            Error::Provider(format!(
                "Azure OpenAI: could not build an Entra credential: {e}; run `az login`, \
                 or supply an api key instead"
            ))
        })?;
        Ok(Self::Entra(credential))
    }

    /// Authenticate with a credential the caller built — a managed identity, a
    /// service principal, or a test double.
    pub fn entra(credential: Arc<dyn TokenCredential>) -> Self {
        Self::Entra(credential)
    }
}

/// Where one Azure OpenAI deployment lives, and how to open it.
///
/// One per *port*, not one per resource: the chat and embeddings deployments
/// are different names, and in practice different `api-version`s too.
#[derive(Clone, Debug)]
pub struct AzureOpenAiConfig {
    endpoint: String,
    pub(crate) deployment: String,
    api_version: String,
    credential: AzureCredential,
}

impl AzureOpenAiConfig {
    /// `endpoint` is the resource root (`https://NAME.openai.azure.com`); the
    /// `/openai/deployments/…` path is this adapter's business, not the
    /// caller's. A trailing slash is fine.
    pub fn new(
        endpoint: impl Into<String>,
        deployment: impl Into<String>,
        api_version: impl Into<String>,
        credential: AzureCredential,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            deployment: deployment.into(),
            api_version: api_version.into(),
            credential,
        }
    }
}

/// Shared HTTP plumbing: one client, one deployment, one credential.
#[derive(Debug)]
pub(crate) struct AzureOpenAiClient {
    http: reqwest::Client,
    pub(crate) config: AzureOpenAiConfig,
    /// The most recent Entra token, so a batch of requests costs one token
    /// acquisition rather than one per call. This matters more than it looks:
    /// `AzureCliCredential` shells out to `az` and does not cache, so an
    /// uncached adapter would spawn a process per embedding batch.
    token: Mutex<Option<AccessToken>>,
}

impl AzureOpenAiClient {
    pub(crate) fn new(config: AzureOpenAiConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
            token: Mutex::new(None),
        }
    }

    /// `{endpoint}/openai/deployments/{deployment}/{route}?api-version=…`
    pub(crate) fn url(&self, route: &str) -> String {
        format!(
            "{}/openai/deployments/{}/{route}?api-version={}",
            self.config.endpoint.trim_end_matches('/'),
            self.config.deployment,
            self.config.api_version
        )
    }

    /// The cached token, if it will still be valid by the time it lands.
    fn cached_token(&self) -> Option<String> {
        let guard = self.token.lock().unwrap_or_else(|e| e.into_inner());
        let token = guard.as_ref()?;
        let remaining = token.expires_on - OffsetDateTime::now_utc();
        (remaining > TOKEN_REFRESH_SKEW).then(|| token.token.secret().to_string())
    }

    async fn bearer(&self, credential: &Arc<dyn TokenCredential>) -> Result<String> {
        if let Some(token) = self.cached_token() {
            return Ok(token);
        }

        let token = credential
            .get_token(&[COGNITIVE_SERVICES_SCOPE], None)
            .await
            .map_err(|e| {
                Error::Provider(format!(
                    "Azure OpenAI: no Entra token for {COGNITIVE_SERVICES_SCOPE}: {e}; run \
                     `az login`, assign the resource's Cognitive Services OpenAI User role, \
                     or supply an api key instead"
                ))
            })?;

        let value = token.token.secret().to_string();
        *self.token.lock().unwrap_or_else(|e| e.into_inner()) = Some(token);
        Ok(value)
    }

    /// Attach whichever single header this credential speaks through.
    async fn authorize(&self, request: reqwest::RequestBuilder) -> Result<reqwest::RequestBuilder> {
        match &self.config.credential {
            AzureCredential::ApiKey(key) => Ok(request.header("api-key", key.secret())),
            AzureCredential::Entra(credential) => {
                Ok(request.bearer_auth(self.bearer(credential).await?))
            }
        }
    }

    /// A POST that retries transient failures and gives up on everything else.
    ///
    /// The deployment names the provider in any [`Error::RateLimited`] this
    /// produces, because that is the thing whose quota was actually hit — an
    /// Azure resource can hold several deployments with separate quotas.
    async fn post<Req: Serialize, Res: for<'de> Deserialize<'de>>(
        &self,
        route: &str,
        body: &Req,
    ) -> Result<Res> {
        retry::with_retry(RetryPolicy::default(), &self.config.deployment, || {
            self.try_post(route, body)
        })
        .await
        .map_err(PostFailure::into_error)
    }

    /// `post`, but keeping a rejection distinguishable from a transport
    /// failure so a caller can decide whether the *request* was the problem.
    pub(crate) async fn try_post<Req: Serialize, Res: for<'de> Deserialize<'de>>(
        &self,
        route: &str,
        body: &Req,
    ) -> std::result::Result<Res, PostFailure> {
        let url = self.url(route);
        let request = self
            .authorize(self.http.post(&url).json(body))
            .await
            .map_err(PostFailure::Fatal)?;

        let response = request
            .send()
            .await
            .map_err(|e| PostFailure::Fatal(Error::Provider(format!("POST {url}: {e}"))))?;

        let status = response.status();
        if !status.is_success() {
            // Read the header before the body: consuming the body moves the
            // response, and the advice is on the header.
            let retry_after = retry::retry_after_of(&response);
            let detail = response.text().await.unwrap_or_default();
            let detail = detail.trim().to_string();
            return Err(PostFailure::Rejected(Rejection {
                status,
                error: self.failure(&url, status, &detail),
                detail,
                retry_after,
            }));
        }

        response.json::<Res>().await.map_err(|e| {
            PostFailure::Fatal(Error::Provider(format!(
                "POST {url}: unreadable response: {e}"
            )))
        })
    }

    /// Turn an Azure failure into an error that names the fix.
    ///
    /// Azure's four common rejections are indistinguishable at a glance and
    /// have four different remedies, so the status is read here rather than
    /// left for a human to decode from a raw body.
    fn failure(&self, url: &str, status: reqwest::StatusCode, detail: &str) -> Error {
        let hint = match status.as_u16() {
            401 => format!(
                "the credential was rejected: check the api-key value, or (for Entra) that \
                 `az login` is current and holds the Cognitive Services OpenAI User role on {}",
                self.config.endpoint
            ),
            403 if detail.contains("AuthenticationTypeDisabled") => format!(
                "{} has key authentication disabled; use Entra authentication \
                 (AzureCredential::from_environment)",
                self.config.endpoint
            ),
            403 => format!(
                "access to {} is forbidden for this identity: grant it the Cognitive \
                 Services OpenAI User role",
                self.config.endpoint
            ),
            404 => format!(
                "deployment `{}` does not exist on {} — this is the DEPLOYMENT name, which \
                 need not match the model name; check the resource's deployments",
                self.config.deployment, self.config.endpoint
            ),
            400 if detail.contains("api-version") => format!(
                "api-version `{}` is not accepted for this route; pick one the resource \
                 supports",
                self.config.api_version
            ),
            429 => "rate limited: retry later, raise the deployment's quota, or lower the \
                    batch size"
                .to_string(),
            _ => "see the response body".to_string(),
        };
        Error::Provider(format!("POST {url}: {status}: {detail} — {hint}"))
    }
}

/// [`Embedder`] backed by an Azure OpenAI embeddings deployment.
#[derive(Debug, Clone)]
pub struct AzureOpenAiEmbedder {
    client: Arc<AzureOpenAiClient>,
    dimensions: Option<usize>,
}

impl AzureOpenAiEmbedder {
    /// Build an embedder for the deployment named in `config`.
    ///
    /// `dimensions` asks Azure to shorten the vectors (the Matryoshka models
    /// support it); `None` takes the deployment's native width. Whatever is
    /// asked for, the response is checked against it — a model that silently
    /// ignores the request would otherwise poison a store whose column width
    /// was chosen from this number.
    pub fn new(config: AzureOpenAiConfig, dimensions: Option<usize>) -> Self {
        Self {
            client: Arc::new(AzureOpenAiClient::new(config)),
            dimensions,
        }
    }
}

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    input: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<usize>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingDatum>,
}

#[derive(Deserialize)]
struct EmbeddingDatum {
    index: usize,
    embedding: Vec<f32>,
}

#[async_trait]
impl Embedder for AzureOpenAiEmbedder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let response: EmbeddingResponse = self
            .client
            .post(
                "embeddings",
                &EmbeddingRequest {
                    input: texts,
                    dimensions: self.dimensions,
                },
            )
            .await?;

        let vectors = order_embeddings(response.data, texts.len())?;
        if let Some(expected) = self.dimensions {
            for (index, vector) in vectors.iter().enumerate() {
                if vector.len() != expected {
                    return Err(Error::Provider(format!(
                        "embeddings: asked for {expected} dimensions but vector {index} has \
                         {}; the deployment ignored the `dimensions` request, so either drop \
                         it or use a model that honours it",
                        vector.len()
                    )));
                }
            }
        }
        Ok(vectors)
    }

    /// `deployment@dimensions`, or the deployment alone at the model's native
    /// width.
    ///
    /// The DEPLOYMENT names this key, not the model: on Azure the deployment
    /// is what actually served the request, it is the only name the caller
    /// controls, and two deployments of the same model can differ in version
    /// and in content filter. The width joins it whenever one was asked for,
    /// because a narrowed vector lives in a different space from a native one.
    fn key(&self) -> String {
        match self.dimensions {
            Some(dimensions) => format!("{}@{dimensions}", self.client.config.deployment),
            None => self.client.config.deployment.clone(),
        }
    }

    /// High: an Azure deployment is sized by its own provisioned quota, and
    /// exceeding it produces 429s carrying `Retry-After` — which the retry loop
    /// absorbs and, past that, the scheduler parks on. Throttling by connection
    /// count here would leave provisioned capacity unused.
    fn concurrency_ceiling(&self) -> usize {
        32
    }

    /// [`OpenAiEmbedder::MAX_INPUT_TOKENS`] — the deployment serves an OpenAI
    /// embedding model, so it has that family's per-input cap.
    ///
    /// This is the cap the live index actually hit: 59 elements of a real repo
    /// were answered with
    /// `400 Invalid 'input[0]': maximum input length is 8192 tokens`, three
    /// times each, and then failed for good.
    fn max_input_tokens(&self) -> usize {
        OpenAiEmbedder::MAX_INPUT_TOKENS
    }
}

/// Place each returned vector at the index the API claims for it.
///
/// The response is a *mapping*, not a list — the API documents `index` rather
/// than guaranteeing order — and a mapping can be wrong in ways a length check
/// cannot see: a duplicated index would overwrite one slot and leave another
/// empty, which would then be returned as if it were an embedding.
fn order_embeddings(data: Vec<EmbeddingDatum>, expected: usize) -> Result<Vec<Vec<f32>>> {
    if data.len() != expected {
        return Err(Error::Provider(format!(
            "embeddings: asked for {expected} vectors, got {}",
            data.len()
        )));
    }

    let mut slots: Vec<Option<Vec<f32>>> = vec![None; expected];
    for datum in data {
        let slot = slots.get_mut(datum.index).ok_or_else(|| {
            Error::Provider(format!(
                "embeddings: index {} out of range for a batch of {expected}",
                datum.index
            ))
        })?;
        if slot.is_some() {
            return Err(Error::Provider(format!(
                "embeddings: index {} returned twice",
                datum.index
            )));
        }
        *slot = Some(datum.embedding);
    }

    slots
        .into_iter()
        .enumerate()
        .map(|(index, slot)| {
            slot.ok_or_else(|| {
                Error::Provider(format!("embeddings: no vector returned for index {index}"))
            })
        })
        .collect()
}

/// [`Summarizer`] backed by an Azure OpenAI chat-completions deployment.
#[derive(Debug)]
pub struct AzureOpenAiSummarizer {
    client: Arc<AzureOpenAiClient>,
    /// Whether to keep asking for a schema-constrained response. Cleared the
    /// first time this deployment's `api-version` rejects it — older Azure
    /// api-versions predate structured outputs entirely.
    structured: std::sync::atomic::AtomicBool,
}

impl AzureOpenAiSummarizer {
    /// Build a summarizer for the deployment named in `config`.
    pub fn new(config: AzureOpenAiConfig) -> Self {
        Self {
            client: Arc::new(AzureOpenAiClient::new(config)),
            structured: std::sync::atomic::AtomicBool::new(true),
        }
    }

    /// One chat round trip in the requested response format.
    /// One chat round trip, retrying transient failures.
    ///
    /// The retry wraps the single attempt rather than `summarize`, so a blip
    /// costs milliseconds instead of an unwound job — while a rejection of the
    /// SCHEMA still reaches the downgrade untouched, because the loop hands
    /// non-transient rejections back unchanged.
    async fn chat(
        &self,
        user: &str,
        response_format: serde_json::Value,
    ) -> std::result::Result<ChatResponse, PostFailure> {
        retry::with_retry(
            RetryPolicy::default(),
            &self.client.config.deployment,
            || self.attempt_chat(user, &response_format),
        )
        .await
    }

    async fn attempt_chat(
        &self,
        user: &str,
        response_format: &serde_json::Value,
    ) -> std::result::Result<ChatResponse, PostFailure> {
        self.client
            .try_post(
                "chat/completions",
                &ChatRequest {
                    messages: vec![
                        ChatMessage {
                            role: "system",
                            content: OpenAiSummarizer::SYSTEM_PROMPT,
                        },
                        ChatMessage {
                            role: "user",
                            content: user,
                        },
                    ],
                    response_format,
                },
            )
            .await
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    messages: Vec<ChatMessage<'a>>,
    response_format: &'a serde_json::Value,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: String,
}

#[async_trait]
impl Summarizer for AzureOpenAiSummarizer {
    async fn summarize(&self, element: &Element) -> Result<Summary> {
        use std::sync::atomic::Ordering;

        // Deliberately the OpenAI adapter's prompt and schema, not a second
        // copy of them: the wire format is identical, PRD req 36's tag band is
        // identical, and two copies would drift apart the first time either is
        // tuned — taking the shared PROMPT_VERSION's honesty with them.
        let user = OpenAiSummarizer::user_prompt(element);

        // Structured outputs arrived in a specific `api-version`; a resource
        // pinned to an older one answers 400. Downgrade once per process, and
        // only for that answer.
        if self.structured.load(Ordering::Relaxed) {
            match self.chat(&user, OpenAiSummarizer::response_schema()).await {
                Ok(response) => return summary_from(response, element),
                Err(failure) if failure.rejects_structured_output() => {
                    self.structured.store(false, Ordering::Relaxed);
                }
                Err(failure) => return Err(failure.into_error()),
            }
        }

        let response = self
            .chat(&user, OpenAiSummarizer::json_object_format())
            .await
            .map_err(PostFailure::into_error)?;
        summary_from(response, element)
    }

    /// `deployment@prompt_version` — what served the request, and what it was
    /// asked. The deployment stands in for the model because on Azure it is
    /// the only name that is knowable; the version is
    /// [`OpenAiSummarizer::PROMPT_VERSION`] because the prompt is shared, so a
    /// prompt change moves both adapters' keys together, which is exactly what
    /// a shared prompt should do.
    fn key(&self) -> String {
        format!(
            "{}@{}",
            self.client.config.deployment,
            OpenAiSummarizer::PROMPT_VERSION
        )
    }

    /// See [`AzureOpenAiEmbedder::concurrency_ceiling`].
    fn concurrency_ceiling(&self) -> usize {
        32
    }

    /// [`OpenAiSummarizer::MAX_INPUT_TOKENS`] — same models, same prompt, so
    /// the same prompt budget.
    fn max_input_tokens(&self) -> usize {
        OpenAiSummarizer::MAX_INPUT_TOKENS
    }
}

/// The first choice, parsed and validated into a [`Summary`].
fn summary_from(response: ChatResponse, element: &Element) -> Result<Summary> {
    let content = response
        .choices
        .first()
        .map(|choice| choice.message.content.as_str())
        .ok_or_else(|| Error::Provider("chat/completions: no choices returned".into()))?;

    parse_summary(content, element.kind.as_str())
}

/// Turn the model's JSON into a [`Summary`] that satisfies the port contract.
///
/// Normalise what can be normalised — trim, drop blank tags, enforce PRD req
/// 36's band — and reject what cannot. Returning `Ok` with blank text or a
/// blank tag would hand the caller a value the shared contract harness rejects;
/// the provider boundary is where that has to stop.
fn parse_summary(content: &str, fallback_tag: &str) -> Result<Summary> {
    let mut summary: Summary = serde_json::from_str(content).map_err(|e| {
        Error::Provider(format!(
            "chat/completions: summary was not the requested JSON: {e}"
        ))
    })?;

    summary.text = summary.text.trim().to_string();
    if summary.text.is_empty() {
        return Err(Error::Provider(
            "chat/completions: summary text was blank".into(),
        ));
    }

    summary.tags = std::mem::take(&mut summary.tags)
        .into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .take(*Summary::TAG_RANGE.end())
        .collect();
    if summary.tags.is_empty() {
        summary.tags.push(fallback_tag.trim().to_string());
    }

    if !summary.has_valid_tags() || summary.tags.iter().any(|tag| tag.is_empty()) {
        return Err(Error::Provider(format!(
            "chat/completions: tags violate PRD req 36's 1-5 non-blank band: {:?}",
            summary.tags
        )));
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(credential: AzureCredential) -> AzureOpenAiConfig {
        AzureOpenAiConfig::new(
            "https://example.openai.azure.com/",
            "gpt-4o-deployment",
            CHAT_API_VERSION,
            credential,
        )
    }

    /// The deployment lives in the path and the api-version in the query — the
    /// two ways Azure differs from OpenAI's addressing, pinned without a
    /// network call.
    #[test]
    fn the_url_names_the_deployment_and_the_api_version() {
        let client = AzureOpenAiClient::new(config(AzureCredential::api_key("k")));
        assert_eq!(
            client.url("chat/completions"),
            "https://example.openai.azure.com/openai/deployments/gpt-4o-deployment/\
             chat/completions?api-version=2024-12-01-preview"
        );
    }

    /// A trailing slash on the endpoint must not become a double slash: Azure
    /// answers `/openai//deployments/...` with a 404 that reads like a missing
    /// deployment, which sends the reader hunting the wrong thing.
    #[test]
    fn a_trailing_slash_on_the_endpoint_does_not_double_up() {
        for endpoint in [
            "https://example.openai.azure.com",
            "https://example.openai.azure.com/",
            "https://example.openai.azure.com///",
        ] {
            let client = AzureOpenAiClient::new(AzureOpenAiConfig::new(
                endpoint,
                "d",
                EMBEDDINGS_API_VERSION,
                AzureCredential::api_key("k"),
            ));
            assert_eq!(
                client.url("embeddings"),
                "https://example.openai.azure.com/openai/deployments/d/embeddings\
                 ?api-version=2024-02-01"
            );
        }
    }

    /// An empty batch must not become an HTTP request — proved without one.
    #[tokio::test]
    async fn an_empty_batch_costs_no_request() {
        let embedder = AzureOpenAiEmbedder::new(config(AzureCredential::api_key("k")), None);
        assert_eq!(embedder.embed(&[]).await.unwrap(), Vec::<Vec<f32>>::new());
    }

    /// The key must not survive a `Debug` render — of the config, the client,
    /// or either public provider. `{:#?}` too: pretty-printing walks the same
    /// fields by a different path.
    #[test]
    fn debug_never_prints_the_api_key() {
        const KEY: &str = "azure-live-DO-NOT-LEAK-0123456789";
        let credential = AzureCredential::api_key(KEY);
        let embedder = AzureOpenAiEmbedder::new(config(credential.clone()), Some(1024));
        let summarizer = AzureOpenAiSummarizer::new(config(credential.clone()));

        for rendered in [
            format!("{credential:?}"),
            format!("{embedder:?}"),
            format!("{embedder:#?}"),
            format!("{summarizer:?}"),
            format!("{summarizer:#?}"),
            format!("{:?}", config(credential)),
        ] {
            assert!(
                !rendered.contains(KEY),
                "Debug leaked the api key: {rendered}"
            );
        }
    }

    #[test]
    fn a_missing_key_variable_is_named_in_the_error() {
        // A name no environment sets, so the test cannot pass by accident.
        let error = AzureCredential::api_key_from_env("FS3_AZURE_KEY_THAT_IS_NOT_SET")
            .expect_err("the variable is not set");
        assert!(
            error.to_string().contains("FS3_AZURE_KEY_THAT_IS_NOT_SET"),
            "the error must name the variable: {error}"
        );
    }

    fn datum(index: usize, value: f32) -> EmbeddingDatum {
        EmbeddingDatum {
            index,
            embedding: vec![value],
        }
    }

    #[test]
    fn embeddings_are_placed_at_the_index_the_api_claims() {
        let ordered = order_embeddings(vec![datum(2, 3.0), datum(0, 1.0), datum(1, 2.0)], 3)
            .expect("a complete, unique mapping is valid");
        assert_eq!(ordered, vec![vec![1.0], vec![2.0], vec![3.0]]);
    }

    #[test]
    fn a_duplicated_index_is_rejected_rather_than_leaving_an_empty_slot() {
        let error = order_embeddings(vec![datum(1, 1.0), datum(1, 2.0)], 2)
            .expect_err("index 1 was returned twice");
        assert!(
            error.to_string().contains("index 1 returned twice"),
            "{error}"
        );
    }

    #[test]
    fn an_out_of_range_index_is_rejected() {
        let error = order_embeddings(vec![datum(0, 1.0), datum(9, 2.0)], 2)
            .expect_err("index 9 is out of range for a batch of 2");
        assert!(error.to_string().contains("out of range"), "{error}");
    }

    #[test]
    fn a_short_response_is_rejected() {
        let error = order_embeddings(vec![datum(0, 1.0)], 2).expect_err("one vector for two texts");
        assert!(error.to_string().contains("asked for 2"), "{error}");
    }

    #[test]
    fn a_well_formed_summary_survives_the_boundary() {
        let summary = parse_summary(
            r#"{"text":"Classifies a node.","tags":["classification"]}"#,
            "callable",
        )
        .expect("valid JSON, valid summary");
        assert_eq!(summary.text, "Classifies a node.");
        assert_eq!(summary.tags, vec!["classification".to_string()]);
    }

    #[test]
    fn blank_summary_text_is_rejected_not_returned() {
        let error = parse_summary(r#"{"text":"   ","tags":["a"]}"#, "callable")
            .expect_err("blank text violates the shared contract");
        assert!(error.to_string().contains("blank"), "{error}");
    }

    /// Whatever leaves the boundary must satisfy the same properties the shared
    /// contract harness asserts over the fakes.
    #[test]
    fn a_repaired_summary_satisfies_the_shared_contract() {
        for content in [
            r#"{"text":"A summary.","tags":[]}"#,
            r#"{"text":" A summary. ","tags":["one","","three"]}"#,
            r#"{"text":"A summary.","tags":["a","b","c","d","e","f"]}"#,
        ] {
            let summary = parse_summary(content, "callable").expect("repairable");
            assert!(summary.has_valid_tags(), "{summary:?}");
            assert!(!summary.text.trim().is_empty(), "{summary:?}");
            assert!(
                summary.tags.iter().all(|tag| !tag.trim().is_empty()),
                "{summary:?}"
            );
        }
    }
}
