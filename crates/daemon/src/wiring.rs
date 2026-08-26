//! The composition root — the entire IoC container (workshop 001 rule 4).
//!
//! Configuration declares a *registry* of named provider instances; the ports
//! (and, per repo, the overrides) name one of them. This module turns those
//! names into `Arc<dyn Port>` once, at startup. There is no container
//! framework, no registry lookup at call time, and no service locator. If you
//! find yourself wanting one, the answer is another arm in
//! [`build_embedder`]/[`build_summarizer`].
//!
//! # How services receive configuration
//!
//! Every builder takes the **narrow section** it needs (`&ProviderInstance`,
//! `&DatabaseConfig`) — never the whole [`Config`], and never a lookup. A
//! service that receives `&ProviderInstance` cannot reach the database URL, so
//! its dependencies are its signature: read the function, know the blast
//! radius. The composition root is the only code that chooses; everything else
//! is handed what it needs.
//!
//! `AppState` keeps the whole [`Config`] because it *is* the composition root's
//! record of what it wired — `/health` and `config show` report from it. It is
//! not a service locator: nothing constructs itself by reaching into it.
//!
//! # Per-repo selection
//!
//! A repo that names a different instance gets a different `Arc` — resolved
//! here, once, and looked up per job or query through
//! [`AppState::embedder_for`]. Two repos naming the same instance share one
//! client. An instance nobody names is never constructed, so declaring a
//! provider you have no key for costs nothing.
//!
//! Adding a service? See "Adding a new injected service" in
//! `docs/how/configuration.md`.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use fs3_core::{Config, DatabaseConfig, Embedder, Port, ProviderInstance, Summarizer};
use fs3_providers::{OpenAiEmbedder, OpenAiSummarizer};
// `PgPool` reaches the daemon through `fs3-store`, which owns the sqlx edge.
// The daemon has no direct `sqlx` dependency, and the arch-check enforces that.
use fs3_store::{PgPool, connect_lazy};
use fs3_testkit::{FakeEmbedder, FakeSummarizer};

/// Everything an HTTP handler or worker needs, wired once at startup.
#[derive(Clone)]
pub struct AppState {
    /// The embedder every repo uses unless it says otherwise.
    pub embedder: Arc<dyn Embedder>,
    /// The summarizer every repo uses unless it says otherwise.
    pub summarizer: Arc<dyn Summarizer>,
    /// Repos that named a different embedder, by repo identity.
    repo_embedders: BTreeMap<String, Arc<dyn Embedder>>,
    /// Repos that named a different summarizer, by repo identity.
    repo_summarizers: BTreeMap<String, Arc<dyn Summarizer>>,
    /// The central store. Lazy: the daemon starts and reports health without
    /// requiring Postgres to be up yet.
    pub db: PgPool,
    /// The configuration these were wired from.
    pub config: Config,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("embedder", &self.config.selected(Port::Embedder, None))
            .field("summarizer", &self.config.selected(Port::Summarizer, None))
            .field("repos", &self.config.repos.len())
            .field(
                "database",
                &fs3_core::redact_url_password(&self.config.database.url),
            )
            .finish()
    }
}

impl AppState {
    /// Wire the whole application from configuration.
    ///
    /// Must be called from inside a Tokio runtime: the connection pool owns a
    /// background reaper task even when it is lazy.
    ///
    /// # Errors
    /// When a referenced provider instance cannot be constructed — e.g. an
    /// OpenAI instance whose API-key variable is not set — or the database URL
    /// is unusable.
    pub fn from_config(config: Config) -> Result<Self> {
        // Resolve name -> instance once, then construct each referenced
        // instance exactly once. Two repos naming the same instance share the
        // Arc, and therefore the HTTP client behind it.
        let mut embedders: BTreeMap<&str, Arc<dyn Embedder>> = BTreeMap::new();
        for name in config.referenced_providers(Port::Embedder) {
            let instance = config.provider(name)?;
            embedders.insert(name, build_embedder(name, instance)?);
        }

        let mut summarizers: BTreeMap<&str, Arc<dyn Summarizer>> = BTreeMap::new();
        for name in config.referenced_providers(Port::Summarizer) {
            let instance = config.provider(name)?;
            summarizers.insert(name, build_summarizer(name, instance)?);
        }

        let embedder = Arc::clone(&embedders[config.selected(Port::Embedder, None)]);
        let summarizer = Arc::clone(&summarizers[config.selected(Port::Summarizer, None)]);

        // Flatten repo -> instance -> Arc now, so a query does one map lookup.
        let mut repo_embedders = BTreeMap::new();
        let mut repo_summarizers = BTreeMap::new();
        for (repo, selection) in &config.repos {
            if let Some(name) = selection.embedder.as_deref() {
                repo_embedders.insert(repo.clone(), Arc::clone(&embedders[name]));
            }
            if let Some(name) = selection.summarizer.as_deref() {
                repo_summarizers.insert(repo.clone(), Arc::clone(&summarizers[name]));
            }
        }

        let db = build_store(&config.database)?;

        Ok(Self {
            embedder,
            summarizer,
            repo_embedders,
            repo_summarizers,
            db,
            config,
        })
    }

    /// The embedder to use for `repo` — its override, or the active default.
    ///
    /// Total by construction: every name was resolved at startup, so a query
    /// never fails on configuration and never builds a client mid-flight.
    #[must_use]
    pub fn embedder_for(&self, repo: &str) -> &Arc<dyn Embedder> {
        self.repo_embedders.get(repo).unwrap_or(&self.embedder)
    }

    /// The summarizer to use for `repo` — its override, or the active default.
    #[must_use]
    pub fn summarizer_for(&self, repo: &str) -> &Arc<dyn Summarizer> {
        self.repo_summarizers.get(repo).unwrap_or(&self.summarizer)
    }

    /// The kind of the instance a port uses by default (`fake`, `openai`), for
    /// logs and `/health`.
    #[must_use]
    pub fn active_kind(&self, port: Port) -> &'static str {
        self.config
            .provider(self.config.selected(port, None))
            .map_or("unknown", ProviderInstance::kind)
    }
}

fn build_store(database: &DatabaseConfig) -> Result<PgPool> {
    connect_lazy(&database.url).with_context(|| format!("database.url = {}", database.url))
}

fn build_embedder(name: &str, instance: &ProviderInstance) -> Result<Arc<dyn Embedder>> {
    Ok(match instance {
        ProviderInstance::Fake => Arc::new(FakeEmbedder::default()),
        ProviderInstance::OpenAi {
            model,
            api_base,
            api_key_env,
        } => Arc::new(OpenAiEmbedder::new(
            model,
            api_base.clone(),
            api_key(api_key_env, name)?,
        )),
    })
}

fn build_summarizer(name: &str, instance: &ProviderInstance) -> Result<Arc<dyn Summarizer>> {
    Ok(match instance {
        ProviderInstance::Fake => Arc::new(FakeSummarizer::default()),
        ProviderInstance::OpenAi {
            model,
            api_base,
            api_key_env,
        } => Arc::new(OpenAiSummarizer::new(
            model,
            api_base.clone(),
            api_key(api_key_env, name)?,
        )),
    })
}

fn api_key(variable: &str, instance: &str) -> Result<String> {
    std::env::var(variable).with_context(|| {
        format!(
            "provider instance `{instance}` is `kind = \"openai\"`, which needs an API key in \
             ${variable}. Export it, put it in secrets.env, point `api_key_env` at another \
             variable, or select an instance with `kind = \"fake\"` to run offline."
        )
    })
}
