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
use fs3_providers::{
    AzureCredential, AzureOpenAiConfig, AzureOpenAiEmbedder, AzureOpenAiSummarizer, OpenAiEmbedder,
    OpenAiSummarizer,
};
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
    /// The central store.
    ///
    /// The pool is lazy — connections are established on first use, so WIRING
    /// never touches the network. That is a runtime property only, not a
    /// startup one: since boot-migrate landed, `main` migrates immediately
    /// after wiring and refuses to serve if it cannot, so a daemon that is
    /// answering has already reached Postgres at least once. Laziness buys the
    /// ordering (wire, then connect), not tolerance of an absent store.
    pub db: PgPool,
    /// The configuration these were wired from.
    pub config: Config,
    /// This daemon's own resolved binary path — WHICH installation it is.
    ///
    /// Resolved once here rather than per response, like every other wiring
    /// decision. It is the scope of the user messages this daemon carries: the
    /// queue is shared by every install pointed at the store, and a message
    /// about somebody else's path is unactionable on a surface whose
    /// `next_action` is mandatory (Jordan, per-install update truth,
    /// 2026-08-27).
    ///
    /// Empty when this process cannot resolve its own executable — vanishingly
    /// rare, and it degrades honestly: an empty scope matches no install, so
    /// the daemon carries only the messages that concern every installation
    /// rather than guessing at somebody else's.
    pub install_path: String,
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

        // Not an error worth refusing to serve over: a daemon that cannot name
        // its own binary can still index, search and answer. It just has no
        // install to speak for, so it carries only store-wide messages.
        let install_path = crate::update::install_path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "cannot resolve this daemon's own binary path");
                String::new()
            });

        Ok(Self {
            embedder,
            summarizer,
            repo_embedders,
            repo_summarizers,
            db,
            config,
            install_path,
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

    /// The `model_key` enrichment rows for `repo` are written under.
    ///
    /// This namespace is what makes a model bump non-destructive: a new key
    /// leaves every existing summary intact, so a rollback is instant
    /// (workshop 002, decision D2). It therefore comes from the instance that
    /// actually answers the call — `model@prompt_version` — rather than from a
    /// config label, which could be renamed without the answers changing.
    #[must_use]
    pub fn summarizer_key(&self, repo: &str) -> String {
        self.summarizer_for(repo).key()
    }

    /// The `model_key` embedding rows for `repo` are written under.
    ///
    /// Vectors are only comparable within one model's space, so this is also
    /// the predicate a search runs under: the key that wrote the rows and the
    /// key that reads them come from the same call, and cannot drift apart.
    #[must_use]
    pub fn embedder_key(&self, repo: &str) -> String {
        self.embedder_for(repo).key()
    }

    /// The kind of the instance a port uses by default (`fake`, `openai`), for
    /// logs and `/health`.
    #[must_use]
    pub fn active_kind(&self, port: Port) -> &'static str {
        self.config
            .provider(self.config.selected(port, None))
            .map_or("unknown", ProviderInstance::kind)
    }

    /// The live user messages every envelope carries, for THIS installation
    /// (PRD req 59).
    ///
    /// Scoped to [`AppState::install_path`] plus everything that concerns the
    /// whole store. A daemon must not repeat another install's news: the queue
    /// is shared, `next_action` is mandatory, and "restart the daemon" is
    /// unactionable advice for a user whose daemon is not the one that said it.
    ///
    /// Best-effort by design: a store that cannot answer this must not turn a
    /// working command into a failing one, and the failure the user actually
    /// needs to see in that situation is the one their own verb produces. So a
    /// broken read logs and yields nothing rather than propagating.
    ///
    /// Read fresh per response rather than cached. It is one partial-index
    /// scan of a table that holds single-digit rows, against a pool the
    /// handler is already using — measurably nothing next to the query or the
    /// embedding call beside it — and a cache would buy staleness plus an
    /// invalidation rule for no gain.
    pub async fn messages(&self) -> Vec<fs3_core::messages::UserMessage> {
        match fs3_store::live_messages(&self.db, &self.install_path).await {
            Ok(messages) => messages,
            Err(error) => {
                tracing::debug!(%error, "could not read the user messages queue");
                Vec::new()
            }
        }
    }
}

fn build_store(database: &DatabaseConfig) -> Result<PgPool> {
    connect_lazy(&database.url).with_context(|| format!("database.url = {}", database.url))
}

fn build_embedder(name: &str, instance: &ProviderInstance) -> Result<Arc<dyn Embedder>> {
    Ok(match instance {
        // The fake is built at the STORE's width, not its own default. A
        // 32-wide vector is easy to read in a failing assertion and impossible
        // to insert into `embeddings_1024`, so an offline stack would index
        // everything and then fail at the last step. The composition root is
        // the only place that can see both halves, so it is where they are made
        // to agree.
        ProviderInstance::Fake => Arc::new(FakeEmbedder {
            dimensions: fs3_store::EMBEDDING_DIMENSIONS,
            ..FakeEmbedder::default()
        }),
        ProviderInstance::OpenAi {
            model,
            api_base,
            api_key_env,
        } => Arc::new(OpenAiEmbedder::new(
            model,
            api_base.clone(),
            api_key(api_key_env, name)?,
        )),
        ProviderInstance::AzureOpenAi {
            endpoint,
            deployment,
            api_version,
            api_key_env,
            dimensions,
        } => Arc::new(AzureOpenAiEmbedder::new(
            azure_config(
                name,
                endpoint,
                deployment,
                api_version,
                api_key_env.as_deref(),
            )?,
            *dimensions,
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
        ProviderInstance::AzureOpenAi {
            endpoint,
            deployment,
            api_version,
            api_key_env,
            ..
        } => Arc::new(AzureOpenAiSummarizer::new(azure_config(
            name,
            endpoint,
            deployment,
            api_version,
            api_key_env.as_deref(),
        )?)),
    })
}

/// Build one Azure deployment's config, choosing the door it opens.
///
/// `api_key_env` naming a variable takes the api-key header; absent takes
/// Entra. Exactly one is ever sent, which is why the credential is an enum
/// rather than two optional fields — and why this function returns the
/// credential already chosen rather than passing both along.
///
/// The live resource this was proved against has key auth DISABLED (403
/// `AuthenticationTypeDisabled`), so the Entra arm is the one that matters here
/// and the key arm is the one that is easy to configure by accident.
fn azure_config(
    name: &str,
    endpoint: &str,
    deployment: &str,
    api_version: &str,
    api_key_env: Option<&str>,
) -> Result<AzureOpenAiConfig> {
    let credential = match api_key_env {
        Some(variable) => AzureCredential::api_key_from_env(variable).with_context(|| {
            format!(
                "provider instance `{name}` names api_key_env = \"{variable}\"; export it, put it \
                 in secrets.env, or REMOVE api_key_env to authenticate with Entra (az login)"
            )
        })?,
        None => AzureCredential::from_environment().with_context(|| {
            format!(
                "provider instance `{name}` authenticates with Entra; run `az login` with an \
                 identity holding the Cognitive Services OpenAI User role on {endpoint}, or set \
                 api_key_env to use a key instead"
            )
        })?,
    };
    Ok(AzureOpenAiConfig::new(
        endpoint,
        deployment,
        api_version,
        credential,
    ))
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
