//! The composition root — the entire IoC container (workshop 001 rule 4).
//!
//! One `match` per port, wiring a concrete adapter into an `Arc<dyn Port>`.
//! There is no container framework, no registry, and no service locator. If you
//! find yourself wanting one, the answer is another arm here.
//!
//! # How services receive configuration
//!
//! Every builder below takes the **narrow section** it needs (`&ProviderConfig`,
//! `&DatabaseConfig`) — never the whole [`Config`], and never a lookup. A
//! service that receives `&EmbedderConfig` cannot reach the database URL, so
//! its dependencies are its signature: read the function, know the blast
//! radius. The composition root is the only code that chooses; everything else
//! is handed what it needs.
//!
//! `AppState` keeps the whole [`Config`] because it *is* the composition root's
//! record of what it wired — `/health` and `config show` report from it. It is
//! not a service locator: nothing constructs itself by reaching into it.
//!
//! Adding a service? See "Adding a new injected service" in
//! `docs/how/configuration.md`.

use std::sync::Arc;

use anyhow::{Context, Result};
use fs3_core::{Config, DatabaseConfig, Embedder, ProviderConfig, Summarizer, redact_url_password};
use fs3_providers::{OpenAiEmbedder, OpenAiSummarizer};
// `PgPool` reaches the daemon through `fs3-store`, which owns the sqlx edge.
// The daemon has no direct `sqlx` dependency, and the arch-check enforces that.
use fs3_store::{PgPool, connect_lazy};
use fs3_testkit::{FakeEmbedder, FakeSummarizer};

/// Everything an HTTP handler or worker needs, wired once at startup.
#[derive(Clone)]
pub struct AppState {
    /// The chosen embedding implementation.
    pub embedder: Arc<dyn Embedder>,
    /// The chosen summarization implementation.
    pub summarizer: Arc<dyn Summarizer>,
    /// The central store. Lazy: the daemon starts and reports health without
    /// requiring Postgres to be up yet.
    pub db: PgPool,
    /// The configuration these were wired from.
    pub config: Config,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("embedder", &describe(&self.config.embedder))
            .field("summarizer", &describe(&self.config.summarizer))
            .field("database", &redact_url_password(&self.config.database.url))
            .finish()
    }
}

/// A one-word name for a provider arm, for logs and `/health`.
pub fn describe(provider: &ProviderConfig) -> &'static str {
    match provider {
        ProviderConfig::Fake => "fake",
        ProviderConfig::OpenAi { .. } => "openai",
    }
}

impl AppState {
    /// Wire the whole application from configuration.
    ///
    /// Must be called from inside a Tokio runtime: the connection pool owns a
    /// background reaper task even when it is lazy.
    ///
    /// # Errors
    /// When a selected provider cannot be constructed — e.g. an OpenAI arm
    /// whose API-key variable is not set — or the database URL is unusable.
    pub fn from_config(config: Config) -> Result<Self> {
        // Each service is constructed with the one section it needs. Adding a
        // service means adding a line here, not a lookup somewhere else.
        let embedder = build_embedder(&config.embedder)?;
        let summarizer = build_summarizer(&config.summarizer)?;
        let db = build_store(&config.database)?;

        Ok(Self {
            embedder,
            summarizer,
            db,
            config,
        })
    }
}

fn build_store(database: &DatabaseConfig) -> Result<PgPool> {
    connect_lazy(&database.url).with_context(|| format!("database.url = {}", database.url))
}

fn build_embedder(provider: &ProviderConfig) -> Result<Arc<dyn Embedder>> {
    Ok(match provider {
        ProviderConfig::Fake => Arc::new(FakeEmbedder::default()),
        ProviderConfig::OpenAi {
            model,
            api_base,
            api_key_env,
        } => Arc::new(OpenAiEmbedder::new(
            model,
            api_base.clone(),
            api_key(api_key_env, "embedder")?,
        )),
    })
}

fn build_summarizer(provider: &ProviderConfig) -> Result<Arc<dyn Summarizer>> {
    Ok(match provider {
        ProviderConfig::Fake => Arc::new(FakeSummarizer::default()),
        ProviderConfig::OpenAi {
            model,
            api_base,
            api_key_env,
        } => Arc::new(OpenAiSummarizer::new(
            model,
            api_base.clone(),
            api_key(api_key_env, "summarizer")?,
        )),
    })
}

fn api_key(variable: &str, port: &str) -> Result<String> {
    std::env::var(variable).with_context(|| {
        format!(
            "the {port} is configured as `provider = \"openai\"`, which needs an API key in \
             ${variable}. Export it, point `api_key_env` at another variable, or set \
             `provider = \"fake\"` to run the stack offline."
        )
    })
}
