//! The OpenAI HTTP adapter for both ports.

use async_trait::async_trait;
use fs3_core::{Element, Embedder, Error, Result, Summarizer, Summary};
use serde::{Deserialize, Serialize};

use crate::DEFAULT_API_BASE;

/// An API key that never appears in `Debug` output.
///
/// The redaction lives in the type, not in each struct that happens to hold a
/// key: any struct can keep `#[derive(Debug)]` and stay safe, and a field added
/// later cannot re-open the leak by forgetting a hand-written `Debug`.
#[derive(Clone)]
struct Secret(String);

impl Secret {
    /// The only way to read the key. Named so that leaks are greppable.
    fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// Shared HTTP plumbing: one client, one base, one key.
#[derive(Debug, Clone)]
struct OpenAiClient {
    http: reqwest::Client,
    api_base: String,
    api_key: Secret,
}

impl OpenAiClient {
    fn new(api_base: Option<String>, api_key: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_base: api_base.unwrap_or_else(|| DEFAULT_API_BASE.to_string()),
            api_key: Secret(api_key),
        }
    }

    async fn post<Req: Serialize, Res: for<'de> Deserialize<'de>>(
        &self,
        route: &str,
        body: &Req,
    ) -> Result<Res> {
        self.try_post(route, body).await.map_err(Error::from)
    }

    /// `post`, but keeping a rejection distinguishable from a transport
    /// failure so a caller can decide whether the *request* was the problem.
    async fn try_post<Req: Serialize, Res: for<'de> Deserialize<'de>>(
        &self,
        route: &str,
        body: &Req,
    ) -> std::result::Result<Res, PostFailure> {
        let url = format!("{}/{route}", self.api_base.trim_end_matches('/'));
        let response = self
            .http
            .post(&url)
            .bearer_auth(self.api_key.expose())
            .json(body)
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

        response.json::<Res>().await.map_err(|e| {
            PostFailure::Fatal(Error::Provider(format!(
                "POST {url}: unreadable response: {e}"
            )))
        })
    }
}

/// Why a POST did not produce a body.
///
/// The distinction exists for exactly one caller: the summarizer, which must
/// tell "this endpoint does not understand structured outputs" (retry in the
/// older shape) from "the network is down" (do not).
pub(crate) enum PostFailure {
    Fatal(Error),
    Rejected {
        url: String,
        status: reqwest::StatusCode,
        detail: String,
    },
}

impl From<PostFailure> for Error {
    fn from(failure: PostFailure) -> Self {
        match failure {
            PostFailure::Fatal(error) => error,
            PostFailure::Rejected {
                url,
                status,
                detail,
            } => Error::Provider(format!("POST {url}: {status}: {detail}")),
        }
    }
}

impl PostFailure {
    /// Whether this rejection means "I do not support that `response_format`".
    ///
    /// Endpoints disagree about how to say it — OpenAI names the parameter,
    /// Azure on an older `api-version` calls it unknown, and compat servers
    /// (Ollama, vLLM, LM Studio) each phrase it their own way — so the test is
    /// a client error that mentions the thing we asked for. Anything else is
    /// a real failure and must not be retried into a weaker request.
    pub(crate) fn rejects_structured_output(&self) -> bool {
        match self {
            Self::Fatal(_) => false,
            Self::Rejected { status, detail, .. } => {
                status.is_client_error() && {
                    let detail = detail.to_ascii_lowercase();
                    detail.contains("response_format") || detail.contains("json_schema")
                }
            }
        }
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

        order_embeddings(response.data, texts.len())
    }

    /// The model alone: this adapter never asks for a narrowed width, so the
    /// model's native dimensionality is the only vector space it produces and
    /// naming it would add a number that can never vary.
    fn key(&self) -> String {
        self.model.clone()
    }
}

/// Place each returned vector at the index the API claims for it.
///
/// The response is a *mapping*, not a list — the API documents `index` rather
/// than guaranteeing order — and a mapping can be wrong in ways a length check
/// cannot see. A duplicated index used to overwrite one slot and leave another
/// as an empty vector, which was then returned as if it were an embedding.
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

    // An unfilled slot is an error, never an empty vector. (With the length and
    // uniqueness checks above this fold cannot fail, which is exactly why it is
    // written as a fold and not an `expect`.)
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

/// [`Summarizer`] backed by an OpenAI-compatible `/chat/completions` endpoint.
#[derive(Debug)]
pub struct OpenAiSummarizer {
    client: OpenAiClient,
    model: String,
    /// Whether to keep asking for a schema-constrained response.
    ///
    /// Cleared the first time an endpoint rejects the request, so a compat
    /// server that will never understand structured outputs costs one wasted
    /// round trip in the life of the process rather than one per element.
    structured: std::sync::atomic::AtomicBool,
}

impl OpenAiSummarizer {
    /// The instruction that makes PRD req 36's tag band a property of the
    /// request rather than a hope about the model.
    ///
    /// It survives alongside [`Self::response_schema`] because the schema
    /// cannot express everything the band needs: OpenAI's `strict` subset has
    /// no `minItems`/`maxItems`, and the fallback path has no schema at all.
    pub const SYSTEM_PROMPT: &'static str = concat!(
        "You summarize one code element or document section. ",
        "Reply with JSON only: {\"text\": string, \"tags\": [string]}. ",
        "`text` is at most three sentences. ",
        "`tags` names between 1 and 5 of the element's most important concepts."
    );

    /// The version of [`Self::SYSTEM_PROMPT`] + [`Self::response_schema`], as
    /// the second half of every summarizer's [`Summarizer::key`].
    ///
    /// **Bump this whenever either changes.** That is the entire migration
    /// story for prompt work: a new version is a new `model_key`, the
    /// reconciler re-enriches under it, and the rows written by the old prompt
    /// stay exactly where they are, still readable, still rollback-able.
    ///
    /// The Azure adapter shares this constant because it shares the prompt and
    /// the schema — one prompt, one version, or the two drift and the keys
    /// lie about it.
    pub const PROMPT_VERSION: &'static str = "1";

    /// Build a summarizer for `model`.
    pub fn new(
        model: impl Into<String>,
        api_base: Option<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            client: OpenAiClient::new(api_base, api_key.into()),
            model: model.into(),
            structured: std::sync::atomic::AtomicBool::new(true),
        }
    }

    /// The user message for an element. Public so the prompt is inspectable
    /// without making a network call.
    pub fn user_prompt(element: &Element) -> String {
        format!(
            "{} `{}` at {} lines {}:\n\n{}",
            element.kind, element.name, element.address, element.span, element.raw_text
        )
    }

    /// The `response_format` that makes a malformed summary unrepresentable
    /// rather than merely unwelcome.
    ///
    /// `strict: true` is what turns the schema from a hint into a constraint —
    /// the model is decoded against it, so "the model replied with prose"
    /// stops being a failure class. Its subset has no `minItems`/`maxItems`,
    /// which is why the 1–5 band stays in the prompt and stays enforced by
    /// [`parse_summary`] on the way out.
    ///
    /// Shared with the Azure adapter: same wire format, same schema, and a
    /// second copy would be a second thing to forget to version.
    pub fn response_schema() -> serde_json::Value {
        serde_json::json!({
            "type": "json_schema",
            "json_schema": {
                "name": "element_summary",
                "strict": true,
                "schema": {
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "At most three sentences summarising the element.",
                        },
                        "tags": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Between 1 and 5 of the element's most important concepts.",
                        },
                    },
                    "required": ["text", "tags"],
                    "additionalProperties": false,
                },
            },
        })
    }

    /// The pre-structured-outputs request shape, still understood everywhere.
    pub fn json_object_format() -> serde_json::Value {
        serde_json::json!({ "type": "json_object" })
    }

    /// One chat round trip in the requested response format.
    async fn chat(
        &self,
        user: &str,
        response_format: serde_json::Value,
    ) -> std::result::Result<ChatResponse, PostFailure> {
        self.client
            .try_post(
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
    model: &'a str,
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
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: String,
}

#[async_trait]
impl Summarizer for OpenAiSummarizer {
    async fn summarize(&self, element: &Element) -> Result<Summary> {
        use std::sync::atomic::Ordering;

        let user = Self::user_prompt(element);

        // Ask for a schema-constrained answer first. If this endpoint does not
        // understand structured outputs, downgrade ONCE — for the whole
        // process, not for this element — and carry on in the shape everything
        // understands. The fallback is not weaker in what it accepts: both
        // paths leave through `parse_summary`, which is what makes the
        // downgrade safe rather than merely convenient.
        if self.structured.load(Ordering::Relaxed) {
            match self.chat(&user, Self::response_schema()).await {
                Ok(response) => return summary_from(response, element),
                Err(failure) if failure.rejects_structured_output() => {
                    self.structured.store(false, Ordering::Relaxed);
                }
                Err(failure) => return Err(failure.into()),
            }
        }

        summary_from(self.chat(&user, Self::json_object_format()).await?, element)
    }

    fn key(&self) -> String {
        format!("{}@{}", self.model, Self::PROMPT_VERSION)
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
/// blank tag would hand the caller a value that the shared contract harness
/// rejects; the provider boundary is where that has to stop.
pub(crate) fn parse_summary(content: &str, fallback_tag: &str) -> Result<Summary> {
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
    use fs3_core::{ElementKind, Span};

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

    #[test]
    fn the_prompt_carries_the_address_and_the_body() {
        let prompt = OpenAiSummarizer::user_prompt(&element());
        assert!(prompt.contains("function `classify`"));
        assert!(prompt.contains("core/src/classify.rs::classify lines 120-127"));
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

    /// The key must not survive a `Debug` render — of the client, or of either
    /// public provider that holds one. `{:#?}` too: pretty-printing walks the
    /// same fields by a different path.
    #[test]
    fn debug_never_prints_the_api_key() {
        const KEY: &str = "sk-live-DO-NOT-LEAK-0123456789";
        let embedder = OpenAiEmbedder::new("text-embedding-3-small", None, KEY);
        let summarizer = OpenAiSummarizer::new("gpt-4o-mini", None, KEY);

        for rendered in [
            format!("{embedder:?}"),
            format!("{embedder:#?}"),
            format!("{summarizer:?}"),
            format!("{summarizer:#?}"),
            format!("{:?}", OpenAiClient::new(None, KEY.to_string())),
        ] {
            assert!(
                !rendered.contains(KEY),
                "Debug leaked the API key: {rendered}"
            );
            assert!(
                rendered.contains("redacted"),
                "Debug should say the key was withheld: {rendered}"
            );
        }
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

    /// The bug this guards: a duplicated index overwrote one slot and left
    /// another as an empty vector, returned as `Ok`.
    #[test]
    fn a_duplicated_index_is_rejected_rather_than_leaving_an_empty_slot() {
        let error = order_embeddings(vec![datum(1, 1.0), datum(1, 2.0)], 2)
            .expect_err("index 1 was returned twice");
        assert!(
            error.to_string().contains("index 1 returned twice"),
            "the error must name the duplicate: {error}"
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

    /// Blank text cannot be repaired, so it must not be returned as `Ok`.
    #[test]
    fn blank_summary_text_is_rejected_not_returned() {
        let error = parse_summary(r#"{"text":"   ","tags":["a"]}"#, "callable")
            .expect_err("blank text violates the shared contract");
        assert!(error.to_string().contains("blank"), "{error}");
    }

    #[test]
    fn blank_tags_are_dropped_and_the_band_is_restored() {
        let summary = parse_summary(r#"{"text":"A summary.","tags":["  ",""]}"#, "callable")
            .expect("blank tags are repairable");
        assert_eq!(summary.tags, vec!["callable".to_string()]);
    }

    #[test]
    fn more_than_five_tags_are_cut_to_the_band() {
        let summary = parse_summary(
            r#"{"text":"A summary.","tags":["a","b","c","d","e","f","g"]}"#,
            "callable",
        )
        .expect("too many tags are repairable");
        assert_eq!(summary.tags.len(), *Summary::TAG_RANGE.end());
        assert!(summary.has_valid_tags());
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
