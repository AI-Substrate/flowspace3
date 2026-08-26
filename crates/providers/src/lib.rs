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
mod local;
mod openai;

pub use azure_openai::{
    AzureCredential, AzureOpenAiConfig, AzureOpenAiEmbedder, AzureOpenAiSummarizer,
    CHAT_API_VERSION, COGNITIVE_SERVICES_SCOPE, EMBEDDINGS_API_VERSION,
};
pub use local::{
    DEFAULT_LOCAL_MODEL, LocalEmbedder, LocalEmbedderConfig, LocalModelInfo, default_cache_dir,
    supported_models,
};
pub use openai::{OpenAiEmbedder, OpenAiSummarizer};

/// The default OpenAI API base. Overridable for Azure or a gateway.
pub const DEFAULT_API_BASE: &str = "https://api.openai.com/v1";
