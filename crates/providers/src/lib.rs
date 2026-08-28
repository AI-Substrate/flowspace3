//! Implementations of the two ports: OpenAI-shaped HTTP services, and a local
//! ONNX model that runs in this process.
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
//! [`Embedder`]: fs3_core::Embedder

mod azure_openai;
mod chat;
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
pub use local::{
    DEFAULT_LOCAL_MODEL, LocalEmbedder, LocalEmbedderConfig, LocalModelInfo, default_cache_dir,
    supported_models,
};
pub use openai::{OpenAiEmbedder, OpenAiSummarizer};
pub use openai_compat::{
    DEFAULT_MAX_TOKENS, DEFAULT_MODEL, OpenAiCompatConfig, OpenAiCompatSummarizer,
    embeddings_unsupported,
};

/// The default OpenAI API base. Overridable for Azure or a gateway.
pub const DEFAULT_API_BASE: &str = "https://api.openai.com/v1";
