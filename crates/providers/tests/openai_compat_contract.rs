//! The real OpenAI-compatible endpoint runs the *same* contract harness the
//! fake does.
//!
//! Ignored by default: it needs a specific server on a specific network, and
//! CI has neither. A plain `cargo test` never touches it.
//!
//! # Running it
//!
//! ```bash
//! # The endpoint's base URL, including the /v1 prefix it exposes:
//! export FS3_OPENAI_COMPAT_BASE_URL=http://192.168.1.134:8080/v1
//! # optional — most of these servers want no auth at all:
//! export FS3_OPENAI_COMPAT_API_KEY=…
//! # optional — RAISE IT for a reasoning model; thinking and answer share it:
//! export FS3_OPENAI_COMPAT_MAX_TOKENS=4000
//!
//! cargo test -p fs3-providers --test openai_compat_contract -- --ignored --nocapture
//! ```
//!
//! A missing base URL fails by name rather than skipping quietly: a run that
//! passes because it did nothing is worse than a red one.

use fs3_core::Summarizer;
use fs3_providers::{OpenAiCompatConfig, OpenAiCompatSummarizer};
use fs3_testkit::{sample_element, summarizer_contract};

fn config() -> OpenAiCompatConfig {
    let base_url = std::env::var("FS3_OPENAI_COMPAT_BASE_URL").unwrap_or_else(|_| {
        panic!(
            "this test is #[ignore]d precisely because it needs a real endpoint; run it with \
             FS3_OPENAI_COMPAT_BASE_URL set (see this file's header)"
        )
    });

    let mut config = OpenAiCompatConfig::new(base_url);
    if std::env::var("FS3_OPENAI_COMPAT_API_KEY").is_ok_and(|key| !key.trim().is_empty()) {
        config = config
            .with_api_key_from_env("FS3_OPENAI_COMPAT_API_KEY")
            .expect("the variable was just checked");
    }
    if let Ok(max_tokens) = std::env::var("FS3_OPENAI_COMPAT_MAX_TOKENS") {
        config = config.with_max_tokens(
            max_tokens
                .parse()
                .expect("FS3_OPENAI_COMPAT_MAX_TOKENS must be a positive integer"),
        );
    }
    config
}

#[tokio::test]
#[ignore = "keyed: needs a LAN OpenAI-compatible endpoint; see this file's header"]
async fn openai_compat_summarizer_honours_the_summarizer_contract() {
    let summarizer = OpenAiCompatSummarizer::connect(config())
        .await
        .expect("the endpoint should report a loaded model");

    println!("served model: {}", summarizer.served_model());
    println!("row key:      {}", summarizer.key());

    summarizer_contract(&summarizer).await;

    let summary = summarizer
        .summarize(&sample_element())
        .await
        .expect("a real summary of a real element");
    println!("text: {}", summary.text);
    println!("tags: {:?}", summary.tags);
    println!("extras: {:?}", summary.extras);
}

/// The reasoning-budget trap, against the real thing.
///
/// A budget sized for the answer alone is spent entirely on the model's
/// thinking, and the server reports success with empty content. The adapter
/// must refuse it by name. This test FAILS if the endpoint quietly starts
/// behaving — which is the point: the workaround should not outlive the quirk.
#[tokio::test]
#[ignore = "keyed: needs a LAN OpenAI-compatible endpoint; see this file's header"]
async fn too_small_a_budget_is_refused_rather_than_returned_as_an_empty_summary() {
    let summarizer = OpenAiCompatSummarizer::connect(config().with_max_tokens(50))
        .await
        .expect("the endpoint should report a loaded model");

    let error = summarizer
        .summarize(&sample_element())
        .await
        .expect_err("50 tokens cannot hold a reasoning model's thinking AND an answer");

    let message = error.to_string();
    println!("refusal: {message}");
    assert!(message.contains("EMPTY summary"), "{message}");
    assert!(message.contains("max_tokens"), "{message}");
}
