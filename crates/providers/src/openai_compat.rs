//! Generic OpenAI-shaped embeddings, summaries, and tool-capable chat.
//!
//! Hosted gateways such as OpenRouter configure one `model` per registry
//! entry. The same account can therefore serve independent surfaces through
//! entries that share `base_url` and `api_key_env` but name different models.
//! The configured model rides every request and every provider key.
//!
//! Single-model LAN servers keep a second, explicit posture:
//! [`OpenAiCompatSummarizer::connect`] discovers the loaded model from
//! `/models`, using that call as both readiness proof and row identity. Such a
//! server may not implement embeddings; callers that know this can surface the
//! actionable [`embeddings_unsupported`] refusal instead of probing at runtime.
//!
//! Structured summaries retain the learned compatibility behaviour: schema
//! rejection downgrades once, and a reasoning model that spends `max_tokens`
//! on thinking but returns empty content is refused rather than stored.
//!
//! Configuration and selection live in `fs3-core` and `fs3-daemon`; this crate
//! only implements the existing ports. Secret values arrive through a named
//! environment variable and are held in a redacting type.

use async_trait::async_trait;
use fs3_core::{Element, Error, Result, Summarizer, Summary};
use serde::{Deserialize, Serialize};

use crate::{
    OpenAiSummarizer,
    retry::{self, PostFailure, Rejection, RetryPolicy},
};

/// Token budget when the caller does not choose one.
///
/// Generous on purpose. On a reasoning model the chain of thought and the
/// answer come out of the same allowance, so a budget sized for the answer
/// alone buys an empty reply — see the module docs. 4000 leaves room for both
/// on the models these servers host.
pub const DEFAULT_MAX_TOKENS: usize = 4000;

/// The `model` field these servers ignore, sent because the OpenAI schema
/// requires it and a spec-compliant server behind the same config will not.
pub const DEFAULT_MODEL: &str = "local";

/// Build a wiring-time refusal for a known chat-only endpoint.
///
/// The generic adapter supports embeddings; this helper is for LAN servers
/// whose operator already knows `/embeddings` is absent and wants a config
/// answer instead of a first-batch failure.
pub fn embeddings_unsupported(base_url: &str) -> Error {
    Error::Provider(format!(
        "{base_url} is configured as a summarizer-only endpoint and does not serve \
         /embeddings. Point the embedder at a provider that embeds — the in-process local \
         embedder needs no server at all — and keep this endpoint for summaries."
    ))
}

/// An API key that never appears in `Debug` output.
#[derive(Clone)]
struct Secret(String);

impl Secret {
    fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// Where the endpoint is and how much rope to give it.
#[derive(Debug, Clone)]
pub struct OpenAiCompatConfig {
    base_url: String,
    model: String,
    api_key: Option<Secret>,
    dimensions: Option<usize>,
    max_tokens: usize,
    default_headers: reqwest::header::HeaderMap,
    use_max_completion_tokens: bool,
}

impl OpenAiCompatConfig {
    /// `base_url` includes the version prefix these servers expose, e.g.
    /// `http://host:8080/v1`. A trailing slash is fine.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            model: DEFAULT_MODEL.to_string(),
            api_key: None,
            max_tokens: DEFAULT_MAX_TOKENS,
            dimensions: None,
            default_headers: reqwest::header::HeaderMap::new(),
            use_max_completion_tokens: false,
        }
    }

    /// Send `model` instead of [`DEFAULT_MODEL`]. Ignored by servers that host
    /// one model; required by those that host several.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Request and verify this embedding width.
    #[must_use]
    pub fn with_dimensions(mut self, dimensions: usize) -> Self {
        self.dimensions = Some(dimensions);
        self
    }

    /// Send a bearer token read from the environment variable named `var`.
    ///
    /// Config carries the *name*; the value is read here and never logged.
    ///
    /// # Errors
    /// [`Error::Provider`] naming the variable when it is unset or empty.
    pub fn with_api_key_from_env(mut self, var: &str) -> Result<Self> {
        let value = std::env::var(var)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                Error::Provider(format!(
                    "openai-compat: environment variable {var} is unset or empty; export it, \
                     or drop api_key_env — most of these servers want no auth at all"
                ))
            })?;
        self.api_key = Some(Secret(value));
        Ok(self)
    }

    /// Supply bearer bytes acquired by another authentication flow.
    ///
    /// Kept crate-private so provider-specific adapters can reuse this wire
    /// implementation without making raw-secret construction part of the
    /// public generic API.
    #[must_use]
    pub(crate) fn with_bearer_token(mut self, token: String) -> Self {
        self.api_key = Some(Secret(token));
        self
    }

    /// Add provider-specific, non-secret headers to every request.
    #[must_use]
    pub(crate) fn with_default_headers(mut self, headers: reqwest::header::HeaderMap) -> Self {
        self.default_headers = headers;
        self
    }

    /// Use the modern OpenAI completion-budget field required by Copilot's
    /// newer chat-completions models.
    #[must_use]
    pub(crate) fn with_max_completion_tokens(mut self) -> Self {
        self.use_max_completion_tokens = true;
        self
    }

    /// Raise or lower the token budget. **Raise it for reasoning models**: the
    /// thinking and the answer share this number.
    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    fn token_limits(&self) -> (Option<usize>, Option<usize>) {
        if self.use_max_completion_tokens {
            (None, Some(self.max_tokens))
        } else {
            (Some(self.max_tokens), None)
        }
    }

    fn url(&self, route: &str) -> String {
        format!("{}/{route}", self.base_url.trim_end_matches('/'))
    }

    async fn try_post<Req: Serialize, Res: for<'de> Deserialize<'de>>(
        &self,
        http: &reqwest::Client,
        route: &str,
        body: &Req,
    ) -> std::result::Result<Res, PostFailure> {
        let url = self.url(route);
        let request = match &self.api_key {
            Some(key) => http.post(&url).bearer_auth(key.expose()),
            None => http.post(&url),
        }
        .headers(self.default_headers.clone());
        let response =
            request.json(body).send().await.map_err(|error| {
                PostFailure::Fatal(Error::Provider(format!("POST {url}: {error}")))
            })?;
        let status = response.status();
        if !status.is_success() {
            let retry_after = retry::retry_after_of(&response);
            let detail = response.text().await.unwrap_or_default().trim().to_string();
            return Err(PostFailure::Rejected(Rejection {
                status,
                error: Error::Provider(format!("POST {url}: {status}: {detail}")),
                detail,
                retry_after,
            }));
        }
        response.json::<Res>().await.map_err(|error| {
            PostFailure::Fatal(Error::Provider(format!(
                "POST {url}: unreadable response: {error}"
            )))
        })
    }
}

/// [`fs3_core::Embedder`] backed by an OpenAI-shaped `/embeddings` endpoint.
#[derive(Debug, Clone)]
pub struct OpenAiCompatEmbedder {
    http: reqwest::Client,
    config: OpenAiCompatConfig,
}

impl OpenAiCompatEmbedder {
    /// Build without touching the network; the first batch proves the route.
    pub fn new(config: OpenAiCompatConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
        }
    }
}

#[derive(Serialize)]
struct CompatEmbeddingRequest<'a> {
    model: &'a str,
    input: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<usize>,
}

#[derive(Deserialize)]
struct CompatEmbeddingResponse {
    data: Vec<CompatEmbeddingDatum>,
}

#[derive(Deserialize)]
struct CompatEmbeddingDatum {
    index: usize,
    embedding: Vec<f32>,
}

#[async_trait]
impl fs3_core::Embedder for OpenAiCompatEmbedder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let request = CompatEmbeddingRequest {
            model: &self.config.model,
            input: texts,
            dimensions: self.config.dimensions,
        };
        let response: CompatEmbeddingResponse =
            retry::with_retry(RetryPolicy::default(), &self.config.model, || {
                self.config.try_post(&self.http, "embeddings", &request)
            })
            .await
            .map_err(PostFailure::into_error)?;
        order_compat_embeddings(
            response.data,
            texts.len(),
            self.config.dimensions,
            &self.config.model,
        )
    }

    fn key(&self) -> String {
        match self.config.dimensions {
            Some(dimensions) => format!("{}@{dimensions}", self.config.model),
            None => self.config.model.clone(),
        }
    }

    fn concurrency_ceiling(&self) -> usize {
        16
    }

    fn max_input_tokens(&self) -> usize {
        crate::OpenAiEmbedder::MAX_INPUT_TOKENS
    }
}

fn order_compat_embeddings(
    data: Vec<CompatEmbeddingDatum>,
    expected: usize,
    dimensions: Option<usize>,
    model: &str,
) -> Result<Vec<Vec<f32>>> {
    if data.len() != expected {
        return Err(Error::Provider(format!(
            "embeddings: asked for {expected} vectors, got {}",
            data.len()
        )));
    }
    let mut slots: Vec<Option<Vec<f32>>> = vec![None; expected];
    for datum in data {
        if let Some(width) = dimensions
            && datum.embedding.len() != width
        {
            return Err(Error::Provider(format!(
                "embeddings: model {model} returned {} dimensions, configured {width}",
                datum.embedding.len()
            )));
        }
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

/// [`Summarizer`] backed by any OpenAI-compatible `/chat/completions` server.
#[derive(Debug)]
pub struct OpenAiCompatSummarizer {
    http: reqwest::Client,
    config: OpenAiCompatConfig,
    /// The model id the server reported at connect time. Part of [`Self::key`].
    served_model: String,
    structured: std::sync::atomic::AtomicBool,
}

#[derive(Deserialize)]
struct ModelList {
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

impl OpenAiCompatSummarizer {
    /// Build for a configured multi-model endpoint without probing `/models`.
    ///
    /// Hosted gateways expose a catalogue, not one loaded model; the configured
    /// id is therefore the identity that must ride the model key.
    pub fn configured(config: OpenAiCompatConfig) -> Self {
        let served_model = config.model.clone();
        Self {
            http: reqwest::Client::new(),
            config,
            served_model,
            structured: std::sync::atomic::AtomicBool::new(true),
        }
    }

    /// Connect, and read back which model is actually loaded.
    ///
    /// This is a real round trip on purpose, and it is the closest thing these
    /// servers have to a readiness probe: `llama.cpp` answers `/v1/models` only
    /// once weights are in memory, so a successful `connect` means the next
    /// request will be served rather than hang. Poll this — never sleep and
    /// hope.
    ///
    /// # Errors
    /// [`Error::Provider`] when the endpoint cannot be reached or reports no
    /// model, with the base URL named so the reader knows which box to check.
    pub async fn connect(config: OpenAiCompatConfig) -> Result<Self> {
        let http = reqwest::Client::new();
        let url = config.url("models");

        let request = match &config.api_key {
            Some(key) => http.get(&url).bearer_auth(key.expose()),
            None => http.get(&url),
        }
        .headers(config.default_headers.clone());
        let response = request.send().await.map_err(|e| {
            Error::Provider(format!(
                "openai-compat: GET {url}: {e}; check the host is up and on this network — \
                 these endpoints are typically LAN-only"
            ))
        })?;

        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(Error::Provider(format!(
                "openai-compat: GET {url}: {status}: {}",
                detail.trim()
            )));
        }

        let models: ModelList = response.json().await.map_err(|e| {
            Error::Provider(format!(
                "openai-compat: GET {url}: unreadable model list: {e}"
            ))
        })?;

        let served_model = models
            .data
            .into_iter()
            .next()
            .map(|entry| entry.id)
            .ok_or_else(|| {
                Error::Provider(format!(
                    "openai-compat: {url} reports no models; the server is up but has not \
                     loaded one yet — wait for it to finish loading and retry"
                ))
            })?;

        Ok(Self {
            http,
            config,
            served_model,
            structured: std::sync::atomic::AtomicBool::new(true),
        })
    }

    /// The model id the server reported when this adapter connected.
    pub fn served_model(&self) -> &str {
        &self.served_model
    }

    /// One chat round trip, retrying transient failures.
    ///
    /// The retry wraps `attempt` rather than `summarize`, so a transient blip
    /// costs a few hundred milliseconds here instead of an unwound job — while
    /// a structured-output rejection still reaches the downgrade untouched,
    /// because the loop hands non-transient rejections back unchanged.
    async fn chat(
        &self,
        user: &str,
        response_format: serde_json::Value,
    ) -> std::result::Result<ChatResponse, PostFailure> {
        retry::with_retry(RetryPolicy::default(), self.served_model(), || {
            self.attempt_chat(user, &response_format)
        })
        .await
    }

    async fn attempt_chat(
        &self,
        user: &str,
        response_format: &serde_json::Value,
    ) -> std::result::Result<ChatResponse, PostFailure> {
        let url = self.config.url("chat/completions");
        let (max_tokens, max_completion_tokens) = self.config.token_limits();
        let body = ChatRequest {
            model: &self.config.model,
            max_tokens,
            max_completion_tokens,
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
        };

        let request = match &self.config.api_key {
            Some(key) => self.http.post(&url).bearer_auth(key.expose()),
            None => self.http.post(&url),
        }
        .headers(self.config.default_headers.clone());

        let response = request
            .json(&body)
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
                error: Error::Provider(format!("POST {url}: {status}: {detail}")),
                detail,
                retry_after,
            }));
        }

        response.json::<ChatResponse>().await.map_err(|e| {
            PostFailure::Fatal(Error::Provider(format!(
                "POST {url}: unreadable response: {e}"
            )))
        })
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<usize>,
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
    /// `"length"` when the budget ran out. The one field that tells an empty
    /// answer apart from a broken one.
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    #[serde(default)]
    content: String,
}

#[async_trait]
impl Summarizer for OpenAiCompatSummarizer {
    async fn summarize(&self, element: &Element) -> Result<Summary> {
        use std::sync::atomic::Ordering;

        let user = OpenAiSummarizer::user_prompt(element);

        if self.structured.load(Ordering::Relaxed) {
            match self.chat(&user, OpenAiSummarizer::response_schema()).await {
                Ok(response) => return self.summary_from(response, element),
                Err(failure) if failure.rejects_structured_output() => {
                    self.structured.store(false, Ordering::Relaxed);
                }
                Err(failure) => return Err(failure.into()),
            }
        }

        let response = self
            .chat(&user, OpenAiSummarizer::json_object_format())
            .await?;
        self.summary_from(response, element)
    }

    /// `served_model@prompt_version` — the model the SERVER reported, not the
    /// one config asked for.
    ///
    /// These endpoints ignore the requested model and serve whatever is
    /// loaded, so a configured name would key rows by a wish. The served id
    /// names the weights and usually the quantisation with them, which is
    /// exactly what changes when the box is switched to another mode.
    ///
    /// The honest limit: this is resolved once, at [`Self::connect`]. A switch
    /// *during* a run is invisible to a key that was read before it happened —
    /// reconnect when the box changes mode, and see the service page.
    fn key(&self) -> String {
        format!("{}@{}", self.served_model, OpenAiSummarizer::PROMPT_VERSION)
    }

    /// **One.** These servers host a single model on a single accelerator, so a
    /// second concurrent request does not run in parallel — it queues inside
    /// the server while holding a connection, and on a reasoning model it
    /// queues for a long time. Declaring anything higher would move the wait
    /// from a place we can see (the scheduler) to a place we cannot (the box).
    fn concurrency_ceiling(&self) -> usize {
        1
    }

    /// Much smaller than the hosted adapters declare, because the box behind
    /// this config is much smaller than a hosted deployment.
    ///
    /// A self-hosted OpenAI-compatible server typically serves one open-weight
    /// model with an 8k or 32k context, and the adapter cannot ask which — the
    /// endpoint reports a model id, not a window. So the number has to fit the
    /// SMALLEST of them: six thousand, which the caller's two-thirds fill
    /// margin turns into about four thousand tokens of element body, leaving
    /// the [`DEFAULT_MAX_TOKENS`] the reply may spend comfortably inside an
    /// 8k window.
    ///
    /// Overrun here is not the clean 400 the embeddings endpoints answer with.
    /// A server out of context truncates the prompt silently or refuses the
    /// generation, and a summary of a silently truncated prompt is a summary
    /// nobody can tell is wrong — which is why the conservative number is the
    /// useful one.
    fn max_input_tokens(&self) -> usize {
        6_000
    }
}

impl OpenAiCompatSummarizer {
    /// The first choice, parsed and validated — including the empty answer
    /// that arrives as a success.
    fn summary_from(&self, response: ChatResponse, element: &Element) -> Result<Summary> {
        let choice = response
            .choices
            .first()
            .ok_or_else(|| Error::Provider("chat/completions: no choices returned".into()))?;

        // The quirk that costs a day. A reasoning model spends the shared
        // budget on `reasoning_content` and returns HTTP 200 with an empty
        // `content` and no error at all. Returning that as a summary would
        // write blank enrichment rows that look successful for ever.
        if choice.message.content.trim().is_empty() {
            let cause = match choice.finish_reason.as_deref() {
                Some("length") => format!(
                    "the reply hit the {} token budget before it produced any answer",
                    self.config.max_tokens
                ),
                Some(reason) => format!("the reply finished with reason `{reason}`"),
                None => "the reply carried no content and gave no reason".to_string(),
            };
            return Err(Error::Provider(format!(
                "chat/completions: {} returned an EMPTY summary with no error — {cause}. On a \
                 reasoning model the thinking and the answer share max_tokens, so raise it \
                 (currently {}; 2000 is the floor, {DEFAULT_MAX_TOKENS} the default) rather \
                 than treating this as a summary.",
                self.served_model, self.config.max_tokens
            )));
        }

        crate::openai::parse_summary(&choice.message.content, element.kind.as_str())
    }
}

/// Tool-capable chat against a configured OpenAI-shaped endpoint.
#[derive(Debug)]
pub struct OpenAiCompatChatClient {
    http: reqwest::Client,
    config: OpenAiCompatConfig,
}

impl OpenAiCompatChatClient {
    /// Build without a network probe; configuration and credential resolution
    /// have already succeeded at the composition root.
    pub fn new(config: OpenAiCompatConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
        }
    }
}

#[derive(Serialize)]
struct AgentRequest<'a> {
    model: &'a str,
    messages: Vec<AgentMessage>,
    tools: Vec<AgentTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<usize>,
}

#[derive(Serialize)]
struct AgentMessage {
    role: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<AgentToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize)]
struct AgentTool {
    #[serde(rename = "type")]
    kind: &'static str,
    function: AgentToolDefinition,
}

#[derive(Serialize)]
struct AgentToolDefinition {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Clone, Deserialize, Serialize)]
struct AgentToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    function: AgentFunctionCall,
}

#[derive(Clone, Deserialize, Serialize)]
struct AgentFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct AgentResponse {
    choices: Vec<AgentChoice>,
    #[serde(default)]
    usage: Option<AgentUsage>,
}

#[derive(Deserialize)]
struct AgentChoice {
    message: AgentResponseMessage,
}

#[derive(Deserialize)]
struct AgentResponseMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<AgentToolCall>,
}

#[derive(Deserialize)]
struct AgentUsage {
    total_tokens: u64,
}

fn agent_message(message: &fs3_core::ChatMessage) -> AgentMessage {
    match message {
        fs3_core::ChatMessage::System(content) => AgentMessage {
            role: "system",
            content: Some(content.clone()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        },
        fs3_core::ChatMessage::User(content) => AgentMessage {
            role: "user",
            content: Some(content.clone()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        },
        fs3_core::ChatMessage::Assistant {
            content,
            tool_calls,
        } => AgentMessage {
            role: "assistant",
            content: content.clone(),
            tool_calls: tool_calls
                .iter()
                .map(|call| AgentToolCall {
                    id: call.id.clone(),
                    kind: "function".to_string(),
                    function: AgentFunctionCall {
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                    },
                })
                .collect(),
            tool_call_id: None,
        },
        fs3_core::ChatMessage::ToolResult {
            tool_call_id,
            content,
        } => AgentMessage {
            role: "tool",
            content: Some(content.clone()),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.clone()),
        },
    }
}

#[async_trait]
impl fs3_core::ChatProvider for OpenAiCompatChatClient {
    async fn turn(
        &self,
        messages: &[fs3_core::ChatMessage],
        tools: &[fs3_core::ToolSchema],
    ) -> Result<fs3_core::ChatTurn> {
        let (max_tokens, max_completion_tokens) = self.config.token_limits();
        let request = AgentRequest {
            model: &self.config.model,
            messages: messages.iter().map(agent_message).collect(),
            tools: tools
                .iter()
                .map(|tool| AgentTool {
                    kind: "function",
                    function: AgentToolDefinition {
                        name: tool.name.clone(),
                        description: tool.description.clone(),
                        parameters: tool.parameters.clone(),
                    },
                })
                .collect(),
            max_tokens,
            max_completion_tokens,
        };
        let response: AgentResponse = tokio::time::timeout(
            std::time::Duration::from_secs(180),
            retry::with_retry(RetryPolicy::default(), &self.config.model, || {
                self.config
                    .try_post(&self.http, "chat/completions", &request)
            }),
        )
        .await
        .map_err(|_| {
            Error::Provider(format!(
                "openai-compat chat model {} did not answer within 180s",
                self.config.model
            ))
        })?
        .map_err(PostFailure::into_error)?;

        let usage = response.usage.map(|usage| usage.total_tokens);
        let message = response
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message)
            .ok_or_else(|| {
                Error::Provider(
                    "openai-compat chat returned no choices; nothing to answer with".into(),
                )
            })?;
        Ok(fs3_core::ChatTurn {
            content: message.content,
            tool_calls: message
                .tool_calls
                .into_iter()
                .map(|call| fs3_core::ToolCall {
                    id: call.id,
                    name: call.function.name,
                    arguments: call.function.arguments,
                })
                .collect(),
            tokens_used: usage,
        })
    }

    fn key(&self) -> String {
        self.config.model.clone()
    }

    fn max_input_tokens(&self) -> usize {
        128_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_refusal_names_the_endpoint_and_a_way_forward() {
        let error = embeddings_unsupported("http://192.168.1.134:8080/v1");
        let message = error.to_string();
        assert!(message.contains("192.168.1.134"), "{message}");
        assert!(message.contains("summarizer-only"), "{message}");
        assert!(
            message.contains("local embedder"),
            "a refusal without an alternative is just a wall: {message}"
        );
    }

    #[test]
    fn a_missing_key_variable_is_named_in_the_error() {
        let error = OpenAiCompatConfig::new("http://localhost:8080/v1")
            .with_api_key_from_env("FS3_COMPAT_KEY_THAT_IS_NOT_SET")
            .expect_err("the variable is not set");
        assert!(
            error.to_string().contains("FS3_COMPAT_KEY_THAT_IS_NOT_SET"),
            "{error}"
        );
    }

    #[test]
    fn debug_never_prints_the_api_key() {
        const KEY: &str = "compat-DO-NOT-LEAK-0123456789";
        // SAFETY: single-threaded test process, and the variable is read back
        // immediately below.
        unsafe { std::env::set_var("FS3_COMPAT_KEY_FOR_DEBUG_TEST", KEY) };
        let config = OpenAiCompatConfig::new("http://localhost:8080/v1")
            .with_api_key_from_env("FS3_COMPAT_KEY_FOR_DEBUG_TEST")
            .expect("the variable was just set");

        let rendered = format!("{config:#?}");
        assert!(!rendered.contains(KEY), "Debug leaked the key: {rendered}");
        assert!(rendered.contains("redacted"), "{rendered}");
    }

    #[test]
    fn the_base_url_keeps_one_slash() {
        for base in [
            "http://h:8080/v1",
            "http://h:8080/v1/",
            "http://h:8080/v1///",
        ] {
            assert_eq!(
                OpenAiCompatConfig::new(base).url("chat/completions"),
                "http://h:8080/v1/chat/completions"
            );
        }
    }
}
