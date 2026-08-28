//! Tool-calling chat against Azure OpenAI.
//!
//! This is separate from the summarizer's deliberately narrow response shape:
//! an assistant can answer with tool calls and `content: null`, then must be
//! replayed verbatim before the corresponding tool results. Keeping that shape
//! here prevents the summarizer from becoming a partial agent protocol.

use fs3_core::Result;
use serde::{Deserialize, Serialize};

use crate::{
    azure_openai::{AzureOpenAiClient, AzureOpenAiConfig},
    retry::{self, PostFailure, RetryPolicy},
};

/// An Azure chat-completions client that preserves tool calls between turns.
#[derive(Debug)]
pub struct AzureOpenAiChatClient {
    client: AzureOpenAiClient,
}

impl AzureOpenAiChatClient {
    /// Build a client for the deployment and credential in `config`.
    pub fn new(config: AzureOpenAiConfig) -> Self {
        Self {
            client: AzureOpenAiClient::new(config),
        }
    }

    /// Complete one turn, retrying only transient HTTP rejections.
    pub async fn complete(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        retry::with_retry(RetryPolicy::default(), "Azure OpenAI chat", || {
            self.client.try_post("chat/completions", request)
        })
        .await
        .map_err(PostFailure::into_error)
    }
}

/// How much room one turn's reply gets.
///
/// A turn in an agent loop is a short reply or a tool call, not an essay, and
/// the loop's own token budget is the real ceiling. This only has to be large
/// enough that a final answer is never truncated mid-sentence.
pub const CHAT_MAX_COMPLETION_TOKENS: u32 = 4_000;

/// The chat deployment, seen as fs3's [`ChatProvider`] port.
///
/// The wire types above stay faithful to Azure; the port speaks core's
/// provider-neutral shapes. This impl is the only place the two meet, which is
/// what keeps the agent loop free of any knowledge of Azure — exactly as
/// [`crate::AzureOpenAiSummarizer`] does for the summarizer port.
#[async_trait::async_trait]
impl fs3_core::ChatProvider for AzureOpenAiChatClient {
    async fn turn(
        &self,
        messages: &[fs3_core::ChatMessage],
        tools: &[fs3_core::ToolSchema],
    ) -> Result<fs3_core::ChatTurn> {
        let wire: Vec<ChatMessage> = messages.iter().map(to_wire_message).collect();
        let offered: Vec<ChatTool> = tools
            .iter()
            .map(|tool| {
                ChatTool::function(ChatToolDefinition::new(
                    tool.name.clone(),
                    tool.description.clone(),
                    tool.parameters.clone(),
                ))
            })
            .collect();

        let response = self
            .complete(&ChatCompletionRequest::new(
                wire,
                offered,
                CHAT_MAX_COMPLETION_TOKENS,
            ))
            .await?;

        // An empty `choices` is a well-formed response that answers nothing.
        // Reporting it as a provider failure is honest; inventing an empty turn
        // would make the loop spin against a model that is saying nothing.
        let message = response
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message)
            .ok_or_else(|| {
                fs3_core::Error::Provider(
                    "Azure OpenAI chat returned no choices; nothing to answer with".to_string(),
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
            // Azure reports usage, but this adapter does not yet read it back.
            // `None` is the honest answer: see `ChatTurn::tokens_used`, where
            // unknown must never be mistaken for free.
            tokens_used: None,
        })
    }

    fn key(&self) -> String {
        // The DEPLOYMENT names the model on Azure, exactly as the embedder and
        // summarizer keys do.
        self.client.config.deployment.clone()
    }

    fn max_input_tokens(&self) -> usize {
        // The deployments fs3 targets are 128k-class. This is a declaration of
        // the model's shape, not a limiter — the loop's own budget is what
        // actually bounds a run.
        128_000
    }
}

/// Translate one core message into the wire shape.
fn to_wire_message(message: &fs3_core::ChatMessage) -> ChatMessage {
    match message {
        fs3_core::ChatMessage::System(text) => ChatMessage::system(text.clone()),
        fs3_core::ChatMessage::User(text) => ChatMessage::user(text.clone()),
        fs3_core::ChatMessage::Assistant {
            content,
            tool_calls,
        } => ChatMessage::assistant(
            content.clone(),
            tool_calls
                .iter()
                .map(|call| ChatToolCall {
                    id: call.id.clone(),
                    kind: "function".to_string(),
                    function: ChatFunctionCall {
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                    },
                })
                .collect(),
        ),
        fs3_core::ChatMessage::ToolResult {
            tool_call_id,
            content,
        } => ChatMessage::tool(tool_call_id.clone(), content.clone()),
    }
}

/// A complete tool-capable chat request.
///
/// GPT-5-class Azure deployments require `max_completion_tokens` and reject
/// the older `max_tokens` name. Temperature is intentionally absent: those
/// deployments also reject a non-default value.
#[derive(Clone, Debug, Serialize)]
pub struct ChatCompletionRequest {
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ChatTool>,
    pub max_completion_tokens: u32,
}

impl ChatCompletionRequest {
    pub fn new(
        messages: Vec<ChatMessage>,
        tools: Vec<ChatTool>,
        max_completion_tokens: u32,
    ) -> Self {
        Self {
            messages,
            tools,
            max_completion_tokens,
        }
    }
}

/// One message in a chat transcript, including the two halves of a tool turn.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ChatToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self::content(ChatRole::System, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::content(ChatRole::User, content)
    }

    /// Echo an assistant reply into the next request, including any tool calls.
    pub fn assistant(content: Option<String>, tool_calls: Vec<ChatToolCall>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content,
            tool_calls,
            tool_call_id: None,
        }
    }

    /// Report one tool call's result back to the model.
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Tool,
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    fn content(role: ChatRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }
}

/// Roles accepted by the chat-completions transcript.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

/// One OpenAI function tool declaration.
#[derive(Clone, Debug, Serialize)]
pub struct ChatTool {
    #[serde(rename = "type")]
    kind: &'static str,
    pub function: ChatToolDefinition,
}

impl ChatTool {
    pub fn function(function: ChatToolDefinition) -> Self {
        Self {
            kind: "function",
            function,
        }
    }
}

/// The function name, guidance, and JSON Schema shown to the model.
#[derive(Clone, Debug, Serialize)]
pub struct ChatToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

impl ChatToolDefinition {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }
}

/// A successful completion. Azure may add fields; only choices drive the loop.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ChatCompletionResponse {
    pub choices: Vec<ChatChoice>,
}

/// One candidate assistant turn.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ChatChoice {
    pub message: ChatMessage,
}

/// A tool invocation emitted by an assistant message.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChatToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ChatFunctionCall,
}

/// A function invocation whose arguments remain JSON text until dispatch.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChatFunctionCall {
    pub name: String,
    pub arguments: String,
}
