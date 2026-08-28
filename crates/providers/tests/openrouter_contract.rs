//! Keyed OpenRouter receipts. Ignored in CI; one command runs both legs:
//!
//! ```bash
//! set -a; source ~/.config/flowspace3/secrets.env; set +a; cargo test -p fs3-providers --test openrouter_contract -- --ignored --nocapture
//! ```
//!
//! The key is only read by the adapter and is never printed.

use fs3_core::{ChatMessage, ChatProvider, Embedder};
use fs3_providers::{OpenAiCompatChatClient, OpenAiCompatConfig, OpenAiCompatEmbedder};

const BASE_URL: &str = "https://openrouter.ai/api/v1";
const KEY_ENV: &str = "OPENROUTER_API_KEY";
const CHAT_MODEL: &str = "z-ai/glm-5.3-flash";
const EMBED_MODEL: &str = "openai/text-embedding-3-small";
const EMBED_DIMENSIONS: usize = 1024;

fn config(model: &str) -> OpenAiCompatConfig {
    OpenAiCompatConfig::new(BASE_URL)
        .with_model(model)
        .with_api_key_from_env(KEY_ENV)
        .expect("OPENROUTER_API_KEY must be loaded from secrets.env; see the command above")
}

#[tokio::test]
#[ignore = "keyed: live OpenRouter chat receipt"]
async fn openrouter_chat_returns_content_and_real_usage() {
    let provider = OpenAiCompatChatClient::new(config(CHAT_MODEL));
    let turn = provider
        .turn(
            &[
                ChatMessage::System("Answer in one short sentence.".into()),
                ChatMessage::User("What does a semantic code index retrieve?".into()),
            ],
            &[],
        )
        .await
        .expect("OpenRouter chat should answer");

    assert!(
        turn.content
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty()),
        "the chat model returned no content"
    );
    let tokens = turn
        .tokens_used
        .expect("OpenRouter documents usage.total_tokens in the OpenAI response shape");
    assert!(tokens > 0, "reported usage must be measured, not zero");
    println!("chat model: {CHAT_MODEL}");
    println!("chat tokens_used: {tokens}");
}

#[tokio::test]
#[ignore = "keyed: live OpenRouter embedding receipt"]
async fn openrouter_embedding_returns_the_configured_vector_space() {
    let provider = OpenAiCompatEmbedder::new(config(EMBED_MODEL).with_dimensions(EMBED_DIMENSIONS));
    let vectors = provider
        .embed(&["semantic code search".to_string()])
        .await
        .expect("OpenRouter embeddings should answer");

    assert_eq!(vectors.len(), 1);
    assert_eq!(vectors[0].len(), EMBED_DIMENSIONS);
    println!("embedding model: {EMBED_MODEL}");
    println!("embedding dimensions: {}", vectors[0].len());
}
