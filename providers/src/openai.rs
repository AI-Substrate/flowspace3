//! The OpenAI HTTP adapter for both ports.

use async_trait::async_trait;
use fs3_core::{Element, Embedder, Error, Result, Summarizer, Summary};
use serde::{Deserialize, Serialize};

use crate::DEFAULT_API_BASE;

/// Shared HTTP plumbing: one client, one base, one key.
#[derive(Debug, Clone)]
struct OpenAiClient {
    http: reqwest::Client,
    api_base: String,
    api_key: String,
}

impl OpenAiClient {
    fn new(api_base: Option<String>, api_key: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_base: api_base.unwrap_or_else(|| DEFAULT_API_BASE.to_string()),
            api_key,
        }
    }

    async fn post<Req: Serialize, Res: for<'de> Deserialize<'de>>(
        &self,
        route: &str,
        body: &Req,
    ) -> Result<Res> {
        let url = format!("{}/{route}", self.api_base.trim_end_matches('/'));
        let response = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(body)
            .send()
            .await
            .map_err(|e| Error::Provider(format!("POST {url}: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(Error::Provider(format!(
                "POST {url}: {status}: {}",
                detail.trim()
            )));
        }

        response
            .json::<Res>()
            .await
            .map_err(|e| Error::Provider(format!("POST {url}: unreadable response: {e}")))
    }
}

/// [`Embedder`] backed by an OpenAI-compatible `/embeddings` endpoint.
#[derive(Debug, Clone)]
pub struct OpenAiEmbedder {
    client: OpenAiClient,
    model: String,
}

impl OpenAiEmbedder {
    /// Build an embedder for `model`.
    ///
    /// `api_base` overrides [`DEFAULT_API_BASE`]. The key is passed in, never
    /// read from a config file — fs3 stores no secrets.
    pub fn new(
        model: impl Into<String>,
        api_base: Option<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            client: OpenAiClient::new(api_base, api_key.into()),
            model: model.into(),
        }
    }
}

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a [String],
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
impl Embedder for OpenAiEmbedder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let response: EmbeddingResponse = self
            .client
            .post(
                "embeddings",
                &EmbeddingRequest {
                    model: &self.model,
                    input: texts,
                },
            )
            .await?;

        if response.data.len() != texts.len() {
            return Err(Error::Provider(format!(
                "embeddings: asked for {} vectors, got {}",
                texts.len(),
                response.data.len()
            )));
        }

        // The API documents `index` rather than guaranteeing order; honour it,
        // because the contract test asserts input order.
        let mut ordered = vec![Vec::new(); texts.len()];
        for datum in response.data {
            let slot = ordered.get_mut(datum.index).ok_or_else(|| {
                Error::Provider(format!("embeddings: index {} out of range", datum.index))
            })?;
            *slot = datum.embedding;
        }
        Ok(ordered)
    }
}

/// [`Summarizer`] backed by an OpenAI-compatible `/chat/completions` endpoint.
#[derive(Debug, Clone)]
pub struct OpenAiSummarizer {
    client: OpenAiClient,
    model: String,
}

impl OpenAiSummarizer {
    /// The instruction that makes PRD req 36's tag band a property of the
    /// request rather than a hope about the model.
    pub const SYSTEM_PROMPT: &'static str = concat!(
        "You summarize one code element or document section. ",
        "Reply with JSON only: {\"text\": string, \"tags\": [string]}. ",
        "`text` is at most three sentences. ",
        "`tags` names between 1 and 5 of the element's most important concepts."
    );

    /// Build a summarizer for `model`.
    pub fn new(
        model: impl Into<String>,
        api_base: Option<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            client: OpenAiClient::new(api_base, api_key.into()),
            model: model.into(),
        }
    }

    /// The user message for an element. Public so the prompt is inspectable
    /// without making a network call.
    pub fn user_prompt(element: &Element) -> String {
        format!(
            "{} `{}` from {} lines {}-{}:\n\n{}",
            element.kind,
            element.qualified_name,
            element.path,
            element.start_line,
            element.end_line,
            element.text
        )
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    response_format: ResponseFormat,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
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
impl Summarizer for OpenAiSummarizer {
    async fn summarize(&self, element: &Element) -> Result<Summary> {
        let user = Self::user_prompt(element);
        let response: ChatResponse = self
            .client
            .post(
                "chat/completions",
                &ChatRequest {
                    model: &self.model,
                    messages: vec![
                        ChatMessage {
                            role: "system",
                            content: Self::SYSTEM_PROMPT,
                        },
                        ChatMessage {
                            role: "user",
                            content: &user,
                        },
                    ],
                    response_format: ResponseFormat {
                        kind: "json_object",
                    },
                },
            )
            .await?;

        let content = response
            .choices
            .first()
            .map(|choice| choice.message.content.as_str())
            .ok_or_else(|| Error::Provider("chat/completions: no choices returned".into()))?;

        let mut summary: Summary = serde_json::from_str(content).map_err(|e| {
            Error::Provider(format!(
                "chat/completions: summary was not the requested JSON: {e}"
            ))
        })?;

        // Enforce the band rather than trusting it (PRD req 36).
        summary.tags.truncate(*Summary::TAG_RANGE.end());
        if summary.tags.is_empty() {
            summary.tags.push(element.kind.as_str().to_string());
        }
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs3_core::{BlobRef, ElementKind};

    fn element() -> Element {
        Element {
            path: "core/src/classify.rs".into(),
            blob: BlobRef::new("0123456789abcdef").unwrap(),
            ts_kind: "function_item".into(),
            kind: ElementKind::Callable,
            qualified_name: "classify".into(),
            start_line: 120,
            end_line: 127,
            text: "pub fn classify(ts_kind: &str) -> Option<ElementKind> { .. }".into(),
            has_error: false,
        }
    }

    #[test]
    fn the_prompt_carries_the_address_and_the_body() {
        let prompt = OpenAiSummarizer::user_prompt(&element());
        assert!(prompt.contains("callable `classify`"));
        assert!(prompt.contains("core/src/classify.rs lines 120-127"));
        assert!(prompt.contains("pub fn classify"));
    }

    #[test]
    fn the_system_prompt_states_the_tag_band() {
        assert!(OpenAiSummarizer::SYSTEM_PROMPT.contains("between 1 and 5"));
    }

    /// An empty batch must not become an HTTP request — proved without one.
    #[tokio::test]
    async fn an_empty_batch_costs_no_request() {
        let embedder = OpenAiEmbedder::new("text-embedding-3-small", None, "not-a-real-key");
        assert_eq!(embedder.embed(&[]).await.unwrap(), Vec::<Vec<f32>>::new());
    }
}
