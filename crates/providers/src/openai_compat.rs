//! The generic OpenAI-compatible adapter — **summarizer only**.
//!
//! One `base_url`, one optional key, and the `/chat/completions` shape that
//! Ollama, vLLM, LM Studio and `llama.cpp`'s server all speak. What separates
//! this from [`crate::OpenAiSummarizer`] is not the wire format — it is the
//! assumption set:
//!
//! - **Embeddings may not exist.** The reference endpoint answers
//!   `/v1/embeddings` with `501`, so an embedder pointed here must be refused
//!   with an instruction ([`embeddings_unsupported`]) rather than discovered at
//!   the first batch.
//! - **The served model is discovered, not configured.** These servers ignore
//!   the `model` field and serve whatever is loaded, so the identity that
//!   matters is read from `/v1/models` at [`OpenAiCompatSummarizer::connect`].
//! - **A reasoning model can answer with nothing at all.** Thinking and answer
//!   share one token budget: too small a `max_tokens` and the reply is HTTP
//!   200, `finish_reason: "length"`, and `content: ""` — with the whole budget
//!   spent in `reasoning_content`. That must be a named failure, never an empty
//!   summary. This is the single most expensive thing to learn the hard way.
//!
//! Structured outputs are attempted exactly as for OpenAI and Azure; the
//! remembered downgrade in [`crate::OpenAiSummarizer`]'s shape covers the
//! servers that reject them. (`llama.cpp` does not: it converts the schema into
//! a sampling grammar, which is stricter than either cloud.)
//!
//! ## Snap-in
//!
//! Wiring happens at adoption. The recipe:
//!
//! ```ignore
//! // fs3-core::config — kind = "openai-compat"
//! ProviderInstance::OpenAiCompat {
//!     base_url: String,             // e.g. http://192.168.1.134:8080/v1
//!     api_key_env: Option<String>,  // most of these servers want no auth
//!     max_tokens: Option<usize>,    // DEFAULT_MAX_TOKENS; raise for reasoning models
//! }
//!
//! // fs3-daemon composition root — summarizer arm
//! ProviderInstance::OpenAiCompat { base_url, api_key_env, max_tokens } => {
//!     let mut config = OpenAiCompatConfig::new(base_url);
//!     if let Some(var) = api_key_env {
//!         config = config.with_api_key_from_env(var)?;
//!     }
//!     if let Some(max) = max_tokens {
//!         config = config.with_max_tokens(max);
//!     }
//!     Arc::new(OpenAiCompatSummarizer::connect(config).await?) as Arc<dyn Summarizer>
//! }
//!
//! // fs3-daemon composition root — EMBEDDER arm: refuse, do not attempt
//! ProviderInstance::OpenAiCompat { base_url, .. } => {
//!     return Err(fs3_providers::embeddings_unsupported(&base_url));
//! }
//! ```

use async_trait::async_trait;
use fs3_core::{Element, Error, Result, Summarizer, Summary};
use serde::{Deserialize, Serialize};

use crate::{OpenAiSummarizer, openai::PostFailure};

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

/// The refusal an embedder configured against an OpenAI-compatible endpoint
/// must get — at wiring time, not at the first batch.
///
/// Public because the composition root is what owes the caller this message,
/// and the message should be written once rather than paraphrased there.
pub fn embeddings_unsupported(base_url: &str) -> Error {
    Error::Provider(format!(
        "{base_url} is configured as an embedding provider, but the \
         openai-compat adapter is summarizer-only: these servers commonly \
         serve /chat/completions and answer /embeddings with 404 or 501. Point \
         the embedder at a provider that embeds — the in-process local \
         embedder needs no server at all — and keep this endpoint for \
         summaries."
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
    max_tokens: usize,
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
        }
    }

    /// Send `model` instead of [`DEFAULT_MODEL`]. Ignored by servers that host
    /// one model; required by those that host several.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
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

    /// Raise or lower the token budget. **Raise it for reasoning models**: the
    /// thinking and the answer share this number.
    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    fn url(&self, route: &str) -> String {
        format!("{}/{route}", self.base_url.trim_end_matches('/'))
    }
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
        };
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

    async fn chat(
        &self,
        user: &str,
        response_format: serde_json::Value,
    ) -> std::result::Result<ChatResponse, PostFailure> {
        let url = self.config.url("chat/completions");
        let body = ChatRequest {
            model: &self.config.model,
            max_tokens: self.config.max_tokens,
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
        };

        let response = request
            .json(&body)
            .send()
            .await
            .map_err(|e| PostFailure::Fatal(Error::Provider(format!("POST {url}: {e}"))))?;

        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(PostFailure::Rejected {
                url,
                status,
                detail: detail.trim().to_string(),
            });
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
    max_tokens: usize,
    messages: Vec<ChatMessage<'a>>,
    response_format: serde_json::Value,
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
