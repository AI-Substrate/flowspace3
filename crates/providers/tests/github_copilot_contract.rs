//! Live GitHub Copilot contract leg.
//!
//! Run:
//! `cargo test -p fs3-providers --test github_copilot_contract -- --ignored --nocapture`
//!
//! Authentication precedence: `COPILOT_GITHUB_TOKEN` (including flowspace3's
//! `secrets.env`) > `~/.config/github-copilot/{hosts,apps}.json` > OMP's
//! immutable/read-only `~/.omp/agent/agent.db` OAuth row.

use fs3_providers::{GitHubCopilotConfig, GitHubCopilotEmbedder, GitHubCopilotSummarizer};
use fs3_testkit::{embedder_contract, summarizer_contract};

#[tokio::test]
#[ignore = "keyed: COPILOT_GITHUB_TOKEN or an existing GitHub Copilot/OMP login"]
async fn copilot_embedder_satisfies_the_shared_contract() {
    let model = std::env::var("FS3_COPILOT_EMBED_MODEL")
        .unwrap_or_else(|_| "text-embedding-3-small".to_string());
    let config = GitHubCopilotConfig::discover(model, Some(1536), None)
        .expect("GitHub Copilot credential; run `flowspace3 login github-copilot`");
    embedder_contract(&GitHubCopilotEmbedder::new(config)).await;
}

#[tokio::test]
#[ignore = "keyed: COPILOT_GITHUB_TOKEN or an existing GitHub Copilot/OMP login"]
async fn copilot_summarizer_satisfies_the_shared_contract() {
    let model = std::env::var("FS3_COPILOT_CHAT_MODEL").unwrap_or_else(|_| "gpt-5.4".to_string());
    let config = GitHubCopilotConfig::discover(model, None, Some(4000))
        .expect("GitHub Copilot credential; run `flowspace3 login github-copilot`");
    summarizer_contract(&GitHubCopilotSummarizer::new(config)).await;
}
