//! The OpenAI Decisions API: typed questions in, typed answers out.
//!
//! `POST /v1/decisions` evaluates one shared input — text, images, or both —
//! against questions whose answers are bounded in advance: a probability
//! (`predicate`), one of the caller's options (`choice`), or a position on an
//! ordered rubric (`score`). Nothing is generated, so nothing has to be parsed
//! out of prose, and output tokens are never billed.
//!
//! This is not a port: there is exactly one implementation and no second one
//! planned (architecture rule 3), so it is a concrete client. It lives here
//! because it is the side of the system that touches the world, and it rides
//! the same [`OpenAiClient`] as the embedder and summarizer — one key type that
//! cannot leak through `Debug`, one retry loop.
//!
//! The wire types follow the official SDKs (`openai-python`
//! `types/decision*.py`), which are newer than the prose guide: hosted image
//! URLs ARE accepted, `name` IS optional, and choice values may be booleans.

use std::time::Duration;

use base64::Engine as _;
use fs3_core::Error;
use serde::{Deserialize, Serialize};

use crate::{
    openai::OpenAiClient,
    retry::{self, PostFailure, RetryPolicy},
};

/// The only model the endpoint serves at public beta.
pub const DECISIONS_DEFAULT_MODEL: &str = "gpt-6-luna";

/// The route, relative to the API base.
const ROUTE: &str = "decisions";

/// One `POST /v1/decisions` body.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecisionRequest {
    pub model: String,
    pub input: DecisionInput,
    pub questions: Vec<Question>,
    /// A stable pseudonymous id for the END user, for OpenAI's abuse
    /// detection. Never the caller's own identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,
}

/// The evidence every question is asked about.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DecisionInput {
    /// Plain text: the common case, and the cheapest shape on the wire.
    Text(String),
    /// User messages mixing text and image parts, in order.
    Messages(Vec<InputMessage>),
}

/// A user message. The endpoint accepts no other role, so the role is a
/// constant of the type rather than a field someone can get wrong.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename = "user")]
pub struct InputMessage {
    pub content: Vec<InputPart>,
}

/// One ordered piece of a message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputPart {
    InputText {
        text: String,
    },
    InputImage {
        /// A `data:` URL or a public HTTP(S) URL. `file_id` is not accepted.
        image_url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<ImageDetail>,
    },
}

impl InputPart {
    /// A text part.
    pub fn text(text: impl Into<String>) -> Self {
        Self::InputText { text: text.into() }
    }

    /// An image part from raw bytes, inlined as a base64 `data:` URL — the
    /// one image shape every revision of the guide has accepted.
    pub fn image_bytes(mime: &str, bytes: &[u8], detail: Option<ImageDetail>) -> Self {
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        Self::InputImage {
            image_url: format!("data:{mime};base64,{encoded}"),
            detail,
        }
    }

    /// An image part that the service fetches itself.
    pub fn image_url(url: impl Into<String>, detail: Option<ImageDetail>) -> Self {
        Self::InputImage {
            image_url: url.into(),
            detail,
        }
    }
}

/// How closely the model looks at an image. The service defaults to `auto`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageDetail {
    Low,
    High,
    Auto,
    Original,
}

/// One question. Every question shares the request's input; a question whose
/// wording depends on another's answer needs a second request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Question {
    /// The probability that a statement about the input is true.
    Predicate {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        instructions: String,
    },
    /// One of 2–255 unique, unordered options.
    Choice {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        instructions: String,
        choices: Vec<ChoiceOption>,
    },
    /// A position on 2–10 levels ordered lowest to highest.
    Score {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        instructions: String,
        levels: Vec<ScoreLevel>,
    },
}

impl Question {
    /// The caller's name for this question, echoed on its answer.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Predicate { name, .. } | Self::Choice { name, .. } | Self::Score { name, .. } => {
                name.as_deref()
            }
        }
    }

    /// What the question asks.
    #[must_use]
    pub fn instructions(&self) -> &str {
        match self {
            Self::Predicate { instructions, .. }
            | Self::Choice { instructions, .. }
            | Self::Score { instructions, .. } => instructions,
        }
    }
}

/// One option of a `choice` question.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub value: ChoiceValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A choice value. Typed on the wire: `true` and `"true"` are different
/// options, so the type keeps them apart rather than stringifying.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChoiceValue {
    Bool(bool),
    Text(String),
}

impl std::fmt::Display for ChoiceValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool(value) => write!(f, "{value}"),
            Self::Text(value) => f.write_str(value),
        }
    }
}

/// One level of a `score` question.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScoreLevel {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The service's answer to one request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub model: String,
    /// One per question, in question order.
    pub answers: Vec<Answer>,
    /// Absent from some gateways, so optional here rather than a parse failure
    /// over a field nobody branches on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

/// One answer. `type` is the discriminator, and the variant decides which of
/// `probability`, `choice` or `score` is present.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Answer {
    Predicate {
        #[serde(default)]
        name: Option<String>,
        /// 0–1: how likely the statement is to be true.
        probability: f64,
    },
    Choice {
        #[serde(default)]
        name: Option<String>,
        choice: ChoiceValue,
        confidence: f64,
        probabilities: Vec<ChoiceProbability>,
    },
    Score {
        #[serde(default)]
        name: Option<String>,
        /// The probability-weighted mean of the 0-based level indices, so it
        /// can fall between levels.
        score: f64,
        confidence: f64,
        probabilities: Vec<ScoreProbability>,
    },
    /// The model declined this question. Other questions in the same request
    /// still get answers.
    Refusal {
        #[serde(default)]
        name: Option<String>,
    },
}

impl Answer {
    /// The name of the question this answers, as the service echoed it.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Predicate { name, .. }
            | Self::Choice { name, .. }
            | Self::Score { name, .. }
            | Self::Refusal { name } => name.as_deref(),
        }
    }
}

/// One option's probability within a `choice` answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChoiceProbability {
    pub value: ChoiceValue,
    pub probability: f64,
}

/// One level's probability within a `score` answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScoreProbability {
    /// The level's 0-based index.
    pub value: u32,
    pub label: String,
    pub probability: f64,
}

/// What the request cost. Output tokens are always zero on this endpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

/// Why a decision did not come back.
///
/// Kept structured rather than flattened into [`Error::Provider`] because the
/// caller has to tell three things apart: the request was wrong (fix it), the
/// service is busy (wait), or the transport failed (try again).
#[derive(Clone, Debug, PartialEq)]
pub struct DecisionError {
    /// The HTTP status, when the service answered at all.
    pub status: Option<u16>,
    /// The service's own explanation, unwrapped from its error JSON.
    pub message: String,
    /// The request field the service blamed, e.g. `questions[1].name`.
    pub param: Option<String>,
    /// Whether repeating the same request could succeed.
    pub retryable: bool,
    /// What the service asked us to wait, when it said.
    pub retry_after: Option<Duration>,
}

impl std::fmt::Display for DecisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("decisions: ")?;
        if let Some(status) = self.status {
            write!(f, "{status}: ")?;
        }
        f.write_str(&self.message)?;
        if let Some(param) = &self.param {
            write!(f, " (param `{param}`)")?;
        }
        Ok(())
    }
}

impl std::error::Error for DecisionError {}

impl DecisionError {
    fn from_failure(failure: PostFailure) -> Self {
        match failure {
            PostFailure::Rejected(rejection) => {
                let (message, param) = openai_error_of(&rejection.detail);
                let status = rejection.status.as_u16();
                Self {
                    status: Some(status),
                    message,
                    param,
                    retryable: status == 429 || rejection.status.is_server_error(),
                    retry_after: rejection.retry_after,
                }
            }
            PostFailure::Fatal(Error::RateLimited { retry_after, .. }) => Self {
                status: Some(429),
                message: "rate limited, and still rate limited after retrying".into(),
                param: None,
                retryable: true,
                retry_after,
            },
            PostFailure::Fatal(error) => Self {
                status: None,
                message: error.to_string(),
                param: None,
                retryable: true,
                retry_after: None,
            },
        }
    }
}

/// `{"error":{"message":…,"param":…}}` → its message and param, or the body
/// itself when it is not that shape (a gateway's HTML page, say).
fn openai_error_of(detail: &str) -> (String, Option<String>) {
    #[derive(Deserialize)]
    struct Body {
        error: Inner,
    }
    #[derive(Deserialize)]
    struct Inner {
        message: String,
        #[serde(default)]
        param: Option<String>,
    }
    match serde_json::from_str::<Body>(detail) {
        Ok(body) => (body.error.message, body.error.param),
        Err(_) if detail.is_empty() => ("no error body".into(), None),
        Err(_) => (detail.to_string(), None),
    }
}

/// A client for `POST /v1/decisions`.
#[derive(Debug, Clone)]
pub struct DecisionsClient {
    client: OpenAiClient,
}

impl DecisionsClient {
    /// `api_base` overrides [`crate::DEFAULT_API_BASE`] — a gateway that
    /// serves the same route (Vercel's AI Gateway does) works unchanged. The
    /// key is passed in, never read here.
    pub fn new(api_base: Option<String>, api_key: impl Into<String>) -> Self {
        Self {
            client: OpenAiClient::new(api_base, api_key.into()),
        }
    }

    /// Ask. Transient failures (429, 502–504) are retried; a rejected request
    /// comes back once, with the service's reason.
    pub async fn create(&self, request: &DecisionRequest) -> Result<Decision, DecisionError> {
        retry::with_retry(RetryPolicy::default(), "openai", || {
            self.client.try_post(ROUTE, request)
        })
        .await
        .map_err(DecisionError::from_failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_text_request_serialises_to_the_documented_shape() {
        let request = DecisionRequest {
            model: DECISIONS_DEFAULT_MODEL.into(),
            input: DecisionInput::Text("I was charged twice.".into()),
            questions: vec![
                Question::Predicate {
                    name: Some("complaint".into()),
                    instructions: "Is this a complaint?".into(),
                },
                Question::Choice {
                    name: None,
                    instructions: "Which team?".into(),
                    choices: vec![
                        ChoiceOption {
                            value: ChoiceValue::Text("billing".into()),
                            description: Some("charges".into()),
                        },
                        ChoiceOption {
                            value: ChoiceValue::Bool(false),
                            description: None,
                        },
                    ],
                },
                Question::Score {
                    name: Some("severity".into()),
                    instructions: "How bad?".into(),
                    levels: vec![
                        ScoreLevel {
                            label: "low".into(),
                            description: None,
                        },
                        ScoreLevel {
                            label: "high".into(),
                            description: Some("money lost".into()),
                        },
                    ],
                },
            ],
            safety_identifier: None,
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "model": "gpt-6-luna",
                "input": "I was charged twice.",
                "questions": [
                    {"type": "predicate", "name": "complaint", "instructions": "Is this a complaint?"},
                    {"type": "choice", "instructions": "Which team?", "choices": [
                        {"value": "billing", "description": "charges"},
                        {"value": false}
                    ]},
                    {"type": "score", "name": "severity", "instructions": "How bad?", "levels": [
                        {"label": "low"},
                        {"label": "high", "description": "money lost"}
                    ]}
                ]
            })
        );
    }

    #[test]
    fn message_input_carries_the_user_role_and_ordered_parts() {
        let input = DecisionInput::Messages(vec![InputMessage {
            content: vec![
                InputPart::text("Is the mug broken?"),
                InputPart::image_bytes("image/png", b"\x89PNG", Some(ImageDetail::Low)),
                InputPart::image_url("https://example.com/a.png", None),
            ],
        }]);
        assert_eq!(
            serde_json::to_value(&input).unwrap(),
            json!([{
                "role": "user",
                "content": [
                    {"type": "input_text", "text": "Is the mug broken?"},
                    {"type": "input_image", "image_url": "data:image/png;base64,iVBORw==", "detail": "low"},
                    {"type": "input_image", "image_url": "https://example.com/a.png"}
                ]
            }])
        );
    }

    /// Captured from the live endpoint on 2026-10-14, trimmed to one answer of
    /// each kind plus a refusal.
    #[test]
    fn every_answer_kind_parses() {
        let decision: Decision = serde_json::from_value(json!({
            "model": "gpt-6-luna",
            "answers": [
                {"type": "predicate", "name": "angry", "probability": 0.99},
                {"type": "choice", "name": "dept", "choice": "billing", "confidence": 0.95,
                 "probabilities": [{"value": "billing", "probability": 0.96}, {"value": "technical", "probability": 0.04}]},
                {"type": "choice", "name": "truth", "choice": false, "confidence": 0.92,
                 "probabilities": [{"value": true, "probability": 0.04}, {"value": false, "probability": 0.96}]},
                {"type": "score", "name": "churn", "score": 2.0, "confidence": 1.0,
                 "probabilities": [{"value": 0, "label": "low", "probability": 0.0}, {"value": 2, "label": "high", "probability": 1.0}]},
                {"type": "refusal", "name": "c"},
                {"type": "predicate", "name": null, "probability": 1.0}
            ],
            "usage": {"input_tokens": 443, "input_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 0},
                      "output_tokens": 0, "output_tokens_details": {"reasoning_tokens": 0}, "total_tokens": 443}
        }))
        .expect("the live shape parses");

        assert_eq!(decision.answers.len(), 6);
        assert!(matches!(
            &decision.answers[2],
            Answer::Choice {
                choice: ChoiceValue::Bool(false),
                ..
            }
        ));
        assert!(
            matches!(&decision.answers[4], Answer::Refusal { name } if name.as_deref() == Some("c"))
        );
        assert_eq!(decision.answers[5].name(), None);
        assert_eq!(decision.usage.unwrap().input_tokens, 443);
    }

    #[test]
    fn a_service_error_is_unwrapped_to_its_message_and_param() {
        let (message, param) = openai_error_of(
            r#"{"error":{"message":"Question names must be unique within the request.","type":"invalid_request_error","param":"questions[1].name","code":null}}"#,
        );
        assert_eq!(message, "Question names must be unique within the request.");
        assert_eq!(param.as_deref(), Some("questions[1].name"));

        let (message, param) = openai_error_of("<html>bad gateway</html>");
        assert_eq!(message, "<html>bad gateway</html>");
        assert_eq!(param, None);
    }

    #[test]
    fn debug_never_prints_the_api_key() {
        const KEY: &str = "sk-live-DO-NOT-LEAK-0123456789";
        let rendered = format!("{:#?}", DecisionsClient::new(None, KEY));
        assert!(!rendered.contains(KEY), "{rendered}");
        assert!(rendered.contains("redacted"), "{rendered}");
    }
}
