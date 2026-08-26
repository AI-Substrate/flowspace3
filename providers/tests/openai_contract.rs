//! The real provider runs the *same* contract harness the fake does.
//!
//! Ignored by default because it costs money and needs a key. That symmetry is
//! the whole reason fs3 needs no mocking framework: when this runs green, the
//! fake in CI is proving something real.
//!
//! ```bash
//! OPENAI_API_KEY=sk-… cargo test -p fs3-providers -- --ignored
//! ```

use fs3_providers::{OpenAiEmbedder, OpenAiSummarizer};
use fs3_testkit::{embedder_contract, summarizer_contract};

fn api_key() -> String {
    std::env::var("OPENAI_API_KEY").expect(
        "this test is #[ignore]d precisely because it needs a real key; \
         run it with OPENAI_API_KEY set",
    )
}

#[tokio::test]
#[ignore = "makes real OpenAI API calls; run on demand with OPENAI_API_KEY set"]
async fn openai_embedder_honours_the_embedder_contract() {
    let embedder = OpenAiEmbedder::new("text-embedding-3-small", None, api_key());
    embedder_contract(&embedder).await;
}

#[tokio::test]
#[ignore = "makes real OpenAI API calls; run on demand with OPENAI_API_KEY set"]
async fn openai_summarizer_honours_the_summarizer_contract() {
    let summarizer = OpenAiSummarizer::new("gpt-4o-mini", None, api_key());
    summarizer_contract(&summarizer).await;
}
