//! OpenAI-shaped implementations of the two ports.
//!
//! This crate holds *adapters only*. It does not choose which one runs — that
//! is the composition root's single `match` in `fs3-daemon` (workshop 001
//! rule 4). Nothing here reads config files or environment beyond the API key
//! it is told to look up.
//!
//! Local/`ort` implementations are the second real implementation these ports
//! were created for, and they land with the provider plan. The port shape does
//! not move when they do — that is what rule 3 buys.

mod openai;

pub use openai::{OpenAiEmbedder, OpenAiSummarizer};

/// The default OpenAI API base. Overridable for Azure or a gateway.
pub const DEFAULT_API_BASE: &str = "https://api.openai.com/v1";
