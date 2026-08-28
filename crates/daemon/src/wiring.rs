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
use fs3_core::{
    ChatProvider, Config, DatabaseConfig, Embedder, Event, EventKind, Port, ProviderInstance,
    Summarizer,
};
use fs3_providers::{
    AzureCredential, AzureOpenAiChatClient, AzureOpenAiConfig, AzureOpenAiEmbedder,
    AzureOpenAiSummarizer, OpenAiCompatChatClient, OpenAiCompatConfig, OpenAiCompatEmbedder,
    OpenAiCompatSummarizer, OpenAiEmbedder, OpenAiSummarizer,
};
// `PgPool` reaches the daemon through `fs3-store`, which owns the sqlx edge.
// The daemon has no direct `sqlx` dependency, and the arch-check enforces that.
use fs3_store::{PgPool, connect_lazy};
use tokio::sync::broadcast;

/// Events retained per subscriber before a lagging watcher is disconnected.
///
/// Producers only call [`broadcast::Sender::send`], which never waits. A
/// subscriber that falls more than this many events behind receives `Lagged`
/// and the HTTP handler closes its stream rather than slowing indexing.
const EVENT_CAPACITY: usize = 256;
use fs3_testkit::{FakeChatProvider, FakeEmbedder, FakeSummarizer};
use tokio::sync::RwLock;

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
    /// One non-blocking fan-out for every live event-stream subscriber.
    events: broadcast::Sender<Event>,
    /// The chat model the `ask` verb drives unless a repo says otherwise.
    pub agent: Arc<dyn ChatProvider>,
    /// Repos that named a different chat model, by repo identity.
    repo_agents: BTreeMap<String, Arc<dyn ChatProvider>>,
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
    /// One `ddocs` tooling snapshot per registered worktree, keyed by
    /// `worktree_id` (workshop 008).
    ///
    /// Per-worktree rather than singular because graph paths are normalised
    /// against each root's own `data.root`: a shared snapshot would not merely
    /// go stale for a second root, it would be permanently WRONG for it, and a
    /// wrong answer is worse than an absent one.
    ///
    /// Refreshed only at corpus events (`add_root`, `rescan_root`) — never per
    /// file, because `ddocs --json graph` walks the whole corpus per call.
    /// A batch therefore sees the graph as of that event; a row added mid-batch
    /// may lack edges until the next one. That staleness is accepted
    /// deliberately: the alternative is quadratic, and no cheap corpus-change
    /// detector exists.
    pub ddocs: Arc<RwLock<BTreeMap<i64, Arc<crate::ddoc::DdocTooling>>>>,
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

        let mut agents: BTreeMap<&str, Arc<dyn ChatProvider>> = BTreeMap::new();
        for name in config.referenced_providers(Port::Agent) {
            let instance = config.provider(name)?;
            agents.insert(name, build_agent(name, instance)?);
        }

        let embedder = Arc::clone(&embedders[config.selected(Port::Embedder, None)]);
        let summarizer = Arc::clone(&summarizers[config.selected(Port::Summarizer, None)]);
        let agent = Arc::clone(&agents[config.selected(Port::Agent, None)]);

        // Flatten repo -> instance -> Arc now, so a query does one map lookup.
        let mut repo_embedders = BTreeMap::new();
        let mut repo_summarizers = BTreeMap::new();
        let mut repo_agents = BTreeMap::new();
        for (repo, selection) in &config.repos {
            if let Some(name) = selection.embedder.as_deref() {
                repo_embedders.insert(repo.clone(), Arc::clone(&embedders[name]));
            }
            if let Some(name) = selection.summarizer.as_deref() {
                repo_summarizers.insert(repo.clone(), Arc::clone(&summarizers[name]));
            }
            if let Some(name) = selection.agent.as_deref() {
                repo_agents.insert(repo.clone(), Arc::clone(&agents[name]));
            }
        }

        let db = build_store(&config.database)?;
        let (events, _) = broadcast::channel(EVENT_CAPACITY);

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
            events,
            agent,
            repo_agents,
            db,
            config,
            install_path,
            ddocs: Arc::new(RwLock::new(BTreeMap::new())),
        })
    }
    /// Attach one live watcher to the daemon event fan-out.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    /// Publish one event without ever waiting for a watcher.
    ///
    /// `send` failing means nobody is attached, which is the common idle
    /// shape, not an indexing failure. The event is deliberately not retained:
    /// `/status` is the snapshot for consumers that need current truth.
    pub fn emit(&self, kind: EventKind) {
        let _ = self.events.send(self.event(kind));
    }

    /// Stamp an event for one connection without broadcasting it.
    #[must_use]
    pub(crate) fn event(&self, kind: EventKind) -> Event {
        Event::new(now(), kind)
    }

    /// Number of events one watcher may trail before the stream drops it.
    #[must_use]
    pub const fn event_capacity() -> usize {
        EVENT_CAPACITY
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

    /// The `ddocs` snapshot for a worktree, or the absent one.
    ///
    /// A missing entry is not an error: it means no corpus event has probed
    /// this worktree yet, or the binary was unavailable when one did. Rows
    /// still index; edges, gate membership and derived state are absent.
    pub async fn ddoc_tooling(&self, worktree_id: i64) -> Arc<crate::ddoc::DdocTooling> {
        self.ddoc_snapshot(worktree_id)
            .await
            .unwrap_or_else(|| Arc::new(crate::ddoc::DdocTooling::absent()))
    }

    /// The snapshot for a worktree, or `None` when it has never been PROBED.
    ///
    /// The distinction matters to exactly one caller and not at all to the
    /// others. For indexing, unprobed and absent are the same thing: neither
    /// can supply edges, so [`AppState::ddoc_tooling`] flattens them. For a
    /// DIAGNOSTIC they are opposite claims — "nobody has looked" is not
    /// evidence that the binary is missing, and a daemon restarted against an
    /// indexed corpus starts with every entry unprobed. Anything that reports
    /// tooling absence to a user must use THIS method and stay silent on
    /// `None`.
    pub async fn ddoc_snapshot(&self, worktree_id: i64) -> Option<Arc<crate::ddoc::DdocTooling>> {
        self.ddocs.read().await.get(&worktree_id).cloned()
    }

    /// Replace one worktree's snapshot after a corpus event.
    pub async fn set_ddoc_tooling(&self, worktree_id: i64, tooling: crate::ddoc::DdocTooling) {
        self.ddocs
            .write()
            .await
            .insert(worktree_id, Arc::new(tooling));
    }

    /// The chat model to use for `repo` — its override, or the active default.
    ///
    /// Takes `Option` because a question is not always asked from inside a
    /// repository: `ask` can be scoped to every indexed repo at once, and there
    /// is no per-repo override to consult in that case.
    #[must_use]
    pub fn agent_for(&self, repo: Option<&str>) -> &Arc<dyn ChatProvider> {
        repo.and_then(|repo| self.repo_agents.get(repo))
            .unwrap_or(&self.agent)
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
            api_key(api_key_env, name, "openai")?,
        )),
        ProviderInstance::OpenAiCompat {
            base_url,
            model,
            api_key_env,
            dimensions,
            max_tokens,
        } => Arc::new(OpenAiCompatEmbedder::new(openai_compat_config(
            name,
            base_url,
            model,
            api_key_env.as_deref(),
            *dimensions,
            *max_tokens,
        )?)),
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

/// Build the chat model behind the `ask` verb.
///
/// Only two arms are real. `fake` keeps the offline stack whole — `ask` must
/// work keyless, like every other verb — and Azure is the hosted case. An
/// `openai` instance is refused rather than silently mis-wired: fs3 has no
/// OpenAI chat adapter yet, and answering questions with the wrong client is a
/// worse failure than a startup error that names the gap.
fn build_agent(name: &str, instance: &ProviderInstance) -> Result<Arc<dyn ChatProvider>> {
    Ok(match instance {
        ProviderInstance::Fake => Arc::new(FakeChatProvider::default()),
        ProviderInstance::AzureOpenAi {
            endpoint,
            deployment,
            api_version,
            api_key_env,
            ..
        } => Arc::new(AzureOpenAiChatClient::new(azure_config(
            name,
            endpoint,
            deployment,
            api_version,
            api_key_env.as_deref(),
        )?)),
        ProviderInstance::OpenAiCompat {
            base_url,
            model,
            api_key_env,
            dimensions,
            max_tokens,
        } => Arc::new(OpenAiCompatChatClient::new(openai_compat_config(
            name,
            base_url,
            model,
            api_key_env.as_deref(),
            *dimensions,
            *max_tokens,
        )?)),
        ProviderInstance::OpenAi { .. } => anyhow::bail!(
            "provider instance `{name}` is kind = \"openai\", which cannot serve the agent \
             port: fs3 has no OpenAI chat adapter yet. Name an `azure_openai` instance in \
             [agent] active, or `fake` to answer offline."
        ),
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
            api_key(api_key_env, name, "openai")?,
        )),
        ProviderInstance::OpenAiCompat {
            base_url,
            model,
            api_key_env,
            dimensions,
            max_tokens,
        } => Arc::new(OpenAiCompatSummarizer::configured(openai_compat_config(
            name,
            base_url,
            model,
            api_key_env.as_deref(),
            *dimensions,
            *max_tokens,
        )?)),
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

fn api_key(variable: &str, instance: &str, kind: &str) -> Result<String> {
    std::env::var(variable).with_context(|| {
        format!(
            "provider instance `{instance}` is kind = \"{kind}\", which needs an API key in \
             ${variable}. Export it or put {variable}=… in secrets.env (the secrets file named \
             by `flowspace3 config show`); secrets never belong in config.toml. Select a \
             `kind = \"fake\"` instance to run offline."
        )
    })
}

fn openai_compat_config(
    name: &str,
    base_url: &str,
    model: &str,
    api_key_env: Option<&str>,
    dimensions: Option<usize>,
    max_tokens: Option<usize>,
) -> Result<OpenAiCompatConfig> {
    let mut config = OpenAiCompatConfig::new(base_url).with_model(model);
    if let Some(api_key_env) = api_key_env {
        config = config.with_api_key_from_env(api_key_env).with_context(|| {
            format!(
                "provider instance `{name}` is kind = \"openai_compat\" and needs \
                 {api_key_env}=… in the environment or secrets.env (the secrets file named by \
                 `flowspace3 config show`); never put the key in config.toml"
            )
        })?;
    }
    if let Some(dimensions) = dimensions {
        config = config.with_dimensions(dimensions);
    }
    if let Some(max_tokens) = max_tokens {
        config = config.with_max_tokens(max_tokens);
    }
    Ok(config)
}

/// Current UTC time in the frozen event-wire spelling, without a date crate.
fn now() -> String {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = elapsed.as_secs();
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60,
        elapsed.subsec_millis()
    )
}

/// Days since the Unix epoch to a civil date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}
