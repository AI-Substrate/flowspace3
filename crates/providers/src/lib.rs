//! Implementations of fs3's ports: OpenAI-shaped HTTP services, a local ONNX
//! model that runs in this process, and the readers for the native
//! agent-session stores.
//!
//! This crate holds *adapters only*. It does not choose which one runs — that
//! is the composition root's single `match` in `fs3-daemon` (workshop 001
//! rule 4). Nothing here reads config files or environment beyond the API key
//! it is told to look up.
//!
//! The local `ort` embedder is the second real implementation the [`Embedder`]
//! port was created for (`fs3_core::ports`, rule 3), and the port shape did not
//! move when it landed — that is what rule 3 buys. It needs no key, no server
//! and, after its first model download, no network.
//!
//! Plan 005 widened this crate from the two enrichment ports to port impls
//! GENERALLY: [`conversation_sources`] holds the four
//! [`ConversationSource`](fs3_core::ConversationSource) readers. They belong
//! here for the same reason the local embedder does — they are the side of the
//! system that touches the world. `fs3-parsers` was the impl-guide's first
//! placement and was ruled against on 2026-08-28 (prime, SA1): that crate's
//! scan is a pure function that opens nothing, and a reader that stats inodes,
//! re-globs sidecar directories and opens a sqlite database would have made
//! its documented invariant false.
//!
//! [`Embedder`]: fs3_core::Embedder

mod azure_openai;
mod chat;
pub mod conversation_sources;
mod github_copilot;
mod local;
mod openai;
mod openai_compat;
mod retry;

pub use azure_openai::{
    AzureCredential, AzureOpenAiConfig, AzureOpenAiEmbedder, AzureOpenAiSummarizer,
    CHAT_API_VERSION, COGNITIVE_SERVICES_SCOPE, EMBEDDINGS_API_VERSION,
};
pub use chat::{
    AzureOpenAiChatClient, ChatChoice, ChatCompletionRequest, ChatCompletionResponse,
    ChatFunctionCall, ChatMessage, ChatRole, ChatTool, ChatToolCall, ChatToolDefinition,
};
pub use github_copilot::{
    COPILOT_API_VERSION, COPILOT_USER_AGENT, CredentialSource, DEFAULT_BASE_URL, DeviceCode,
    GitHubCopilotChatClient, GitHubCopilotConfig, GitHubCopilotCredential, GitHubCopilotEmbedder,
    GitHubCopilotModel, GitHubCopilotModelList, GitHubCopilotSummarizer, LoginState, TOKEN_ENV,
    finish_device_login, list_models, start_device_login,
};
pub use local::{
    DEFAULT_LOCAL_MODEL, LocalEmbedder, LocalEmbedderConfig, LocalModelInfo, default_cache_dir,
    supported_models,
};
pub use openai::{OpenAiEmbedder, OpenAiSummarizer};
pub use openai_compat::{
    DEFAULT_MAX_TOKENS, DEFAULT_MODEL, OpenAiCompatChatClient, OpenAiCompatConfig,
    OpenAiCompatEmbedder, OpenAiCompatSummarizer, embeddings_unsupported,
};

/// The default OpenAI API base. Overridable for Azure or a gateway.
pub const DEFAULT_API_BASE: &str = "https://api.openai.com/v1";
