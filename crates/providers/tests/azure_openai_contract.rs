//! The real Azure deployment runs the *same* contract harness the fake does.
//!
//! Ignored by default because it costs money and needs a resource. That
//! symmetry is the whole reason fs3 needs no mocking framework: when this runs
//! green, the fake in CI is proving something real.
//!
//! # Running it
//!
//! ```bash
//! export AZURE_OPENAI_ENDPOINT=https://YOUR-RESOURCE.openai.azure.com
//! export AZURE_OPENAI_CHAT_DEPLOYMENT=gpt-4o            # the DEPLOYMENT name
//! export AZURE_OPENAI_EMBEDDING_DEPLOYMENT=text-embedding-3-small
//! # optional — these default to the versions pinned in the adapter:
//! export AZURE_OPENAI_CHAT_API_VERSION=2024-12-01-preview
//! export AZURE_OPENAI_EMBEDDING_API_VERSION=2024-02-01
//! # optional — ask for shortened vectors:
//! export AZURE_OPENAI_EMBEDDING_DIMENSIONS=1024
//! # optional — WITHOUT it, the run authenticates with Entra (`az login`),
//! # which is the only way in to a resource that has key auth disabled:
//! export AZURE_OPENAI_API_KEY=…
//!
//! cargo test -p fs3-providers --test azure_openai_contract -- --ignored
//! ```
//!
//! A missing variable fails the test by name rather than skipping quietly: a
//! keyed run that silently passes because it did nothing is worse than a red
//! one.

use fs3_providers::{
    AzureCredential, AzureOpenAiConfig, AzureOpenAiEmbedder, AzureOpenAiSummarizer,
    CHAT_API_VERSION, EMBEDDINGS_API_VERSION,
};
use fs3_testkit::{embedder_contract, summarizer_contract};

fn required(var: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| {
        panic!(
            "this test is #[ignore]d precisely because it needs a real Azure OpenAI \
             deployment; run it with {var} set (see this file's header)"
        )
    })
}

fn optional(var: &str, fallback: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| fallback.to_string())
}

/// Key authentication when a key is exported, Entra otherwise.
///
/// Entra is the default rather than the fallback on purpose: an Azure OpenAI
/// resource may have key authentication disabled entirely, and then a key is
/// not merely absent but meaningless.
fn credential() -> AzureCredential {
    match std::env::var("AZURE_OPENAI_API_KEY") {
        Ok(key) if !key.trim().is_empty() => AzureCredential::api_key(key),
        _ => AzureCredential::from_environment()
            .expect("an Entra credential — is `az login` current?"),
    }
}

#[tokio::test]
#[ignore = "makes real Azure OpenAI calls; run on demand — see this file's header"]
async fn azure_openai_embedder_honours_the_embedder_contract() {
    let config = AzureOpenAiConfig::new(
        required("AZURE_OPENAI_ENDPOINT"),
        required("AZURE_OPENAI_EMBEDDING_DEPLOYMENT"),
        optional("AZURE_OPENAI_EMBEDDING_API_VERSION", EMBEDDINGS_API_VERSION),
        credential(),
    );
    let dimensions = std::env::var("AZURE_OPENAI_EMBEDDING_DIMENSIONS")
        .ok()
        .map(|value| {
            value
                .parse()
                .expect("AZURE_OPENAI_EMBEDDING_DIMENSIONS must be a positive integer")
        });

    embedder_contract(&AzureOpenAiEmbedder::new(config, dimensions)).await;
}

#[tokio::test]
#[ignore = "makes real Azure OpenAI calls; run on demand — see this file's header"]
async fn azure_openai_summarizer_honours_the_summarizer_contract() {
    let config = AzureOpenAiConfig::new(
        required("AZURE_OPENAI_ENDPOINT"),
        required("AZURE_OPENAI_CHAT_DEPLOYMENT"),
        optional("AZURE_OPENAI_CHAT_API_VERSION", CHAT_API_VERSION),
        credential(),
    );

    summarizer_contract(&AzureOpenAiSummarizer::new(config)).await;
}
