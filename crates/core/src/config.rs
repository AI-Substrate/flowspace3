//! Configuration *types* and the pure layering that produces an effective
//! config. Parsing a string is allowed here; reading a file is not.
//!
//! All configuration lives in `~/.config/flowspace3/` as files — never in the
//! DB (PRD reqs 28, 39). Discovery and file reading belong to the shell
//! (`fs3-daemon`, `fs3-cli`); the shape, its defaults, and the *merge* live
//! here so both read the same types and can never drift apart.
//!
//! # The layers (low to high precedence)
//!
//! 1. serde defaults on the types below — a fresh machine runs offline.
//! 2. `~/.config/flowspace3/config.toml` ([`CONFIG_DIR_ENV`] overrides the dir).
//! 3. `FS3_*` environment variables, `__` for nesting
//!    (`FS3_DATABASE__URL`) — how containers and tests override without files.
//!
//! Global only: there are no per-repo or per-folder overrides. [`resolve`] is
//! shaped so a fourth layer would be another argument, not a redesign.
//!
//! # Secrets are a separate chain
//!
//! Secret *values* never appear in `config.toml`. Config files name the
//! environment variable that holds a key (`api_key_env`), and the variable is
//! supplied by the process environment or by `secrets.env` in the same config
//! directory ([`parse_env_file`]), which the shells load *into* the environment
//! at startup. Process environment wins; a secret is never logged or printed.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use toml::{Table, Value};

use crate::error::{Error, Result};

/// Environment variable that overrides the config directory. Tests and
/// throwaway environments set it; production leaves it unset.
pub const CONFIG_DIR_ENV: &str = "FS3_CONFIG_DIR";

/// The config file inside the config directory.
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// The optional secrets file inside the config directory. `KEY=value` lines,
/// loaded into the environment at startup, never merged into [`Config`].
pub const SECRETS_FILE_NAME: &str = "secrets.env";

/// Subdirectory of the user's config home when [`CONFIG_DIR_ENV`] is unset.
pub const DEFAULT_CONFIG_SUBDIR: &str = "flowspace3";

/// Prefix marking an environment variable as an fs3 config override.
pub const ENV_PREFIX: &str = "FS3_";

/// Separator for nesting inside an override name: `FS3_DATABASE__URL`.
///
/// An override is `FS3_` + section + `__` + key, and every configuration key
/// lives inside a section — so an `FS3_` variable *without* [`ENV_NESTING`] is
/// not a config override at all. That is what keeps the override namespace off
/// the secrets namespace: `FS3_CONFIG_DIR` steers the loader and a key
/// variable called `FS3_MY_API_KEY` is somebody's secret, neither of which
/// should make the daemon refuse to start. A name that *does* nest must match
/// a real key, so a typo is a startup failure rather than an override that
/// silently does nothing.
pub const ENV_NESTING: &str = "__";

/// What replaces a secret when configuration is printed.
pub const REDACTED: &str = "<redacted>";

/// The whole of fs3's configuration.
///
/// ```toml
/// [daemon]
/// url = "http://127.0.0.1:7373"
///
/// [database]
/// url = "postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3"
///
/// # The registry: any number of named provider instances.
/// [providers.offline]
/// kind = "fake"
///
/// [providers.small]
/// kind = "openai"
/// model = "text-embedding-3-small"
///
/// # The ports name one of them.
/// [embedder]
/// active = "small"
///
/// [summarizer]
/// active = "offline"
///
/// # A repo may name a different instance for either port.
/// [repos."github.com/AI-Substrate/flowspace3"]
/// summarizer = "offline"
///
/// [indexing]
/// summary_min_lines = 10
/// debounce_seconds = 10
///
/// [scan]
/// max_file_bytes = 2000000
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Where the daemon listens, and where the CLI looks for it.
    pub daemon: DaemonConfig,
    /// The central Postgres + pgvector store (PRD req 4).
    pub database: DatabaseConfig,
    /// Every provider instance the machine knows about, by name. Declaring one
    /// costs nothing: only the instances a port or a repo actually names are
    /// constructed.
    #[serde(default = "default_providers")]
    pub providers: BTreeMap<String, ProviderInstance>,
    /// Which instance the [`crate::Embedder`] port uses by default.
    pub embedder: PortSelection,
    /// Which instance the [`crate::Summarizer`] port uses by default.
    pub summarizer: PortSelection,
    /// Per-repo overrides of those choices, keyed by repo identity. Global
    /// file, per-repo *data* — not a second config file (PRD req 28).
    pub repos: BTreeMap<String, RepoSelection>,
    /// Knobs the indexing pipeline reads.
    pub indexing: IndexingConfig,
    /// Knobs the filesystem scanner reads.
    pub scan: ScanConfig,
}

impl Default for Config {
    /// A fresh machine: one offline fake in the registry, both ports naming it.
    fn default() -> Self {
        Self {
            daemon: DaemonConfig::default(),
            database: DatabaseConfig::default(),
            providers: default_providers(),
            embedder: PortSelection::default(),
            summarizer: PortSelection::default(),
            repos: BTreeMap::new(),
            indexing: IndexingConfig::default(),
            scan: ScanConfig::default(),
        }
    }
}

/// The name of the provider instance every fresh machine has: the offline
/// deterministic fake, which needs no keys.
pub const DEFAULT_PROVIDER: &str = "fake";

/// The top-level section names, in the order `config show` prints them.
pub const SECTIONS: &[&str] = &[
    "daemon",
    "database",
    "providers",
    "embedder",
    "summarizer",
    "repos",
    "indexing",
    "scan",
];

impl Config {
    /// Parse configuration from TOML text alone — no environment layer.
    ///
    /// # Errors
    /// [`Error::InvalidConfig`] listing *every* problem when the text is not
    /// valid TOML for this shape, or the values cannot describe a usable
    /// system.
    pub fn from_toml_str(toml_text: &str) -> Result<Self> {
        Ok(resolve(Sources {
            file_label: CONFIG_FILE_NAME,
            file_text: Some(toml_text),
            env: &[],
        })?
        .config)
    }

    /// The instance a port uses for a repo: the repo's override if it has one,
    /// otherwise the port's `active`.
    ///
    /// `repo` is a repo identity as the daemon knows it; `None` (or an unknown
    /// one) means "no override", which is the common case.
    #[must_use]
    pub fn selected(&self, port: Port, repo: Option<&str>) -> &str {
        repo.and_then(|repo| self.repos.get(repo))
            .and_then(|selection| selection.get(port))
            .unwrap_or_else(|| self.selection(port).active.as_str())
    }

    /// The port's default selection.
    #[must_use]
    pub fn selection(&self, port: Port) -> &PortSelection {
        match port {
            Port::Embedder => &self.embedder,
            Port::Summarizer => &self.summarizer,
        }
    }

    /// Look up a provider instance by name.
    ///
    /// # Errors
    /// [`Error::InvalidConfig`] naming the missing instance and listing the
    /// ones that *are* configured — the whole point of a registry is that the
    /// available names are knowable.
    pub fn provider(&self, name: &str) -> Result<&ProviderInstance> {
        self.providers.get(name).ok_or_else(|| {
            Error::InvalidConfig(render(&[unknown_instance(
                "provider",
                name,
                &self.providers,
            )]))
        })
    }

    /// Every instance name some port or repo actually names, deduplicated.
    ///
    /// This is the set the composition root constructs: declaring an instance
    /// you never select must not cost an API key.
    #[must_use]
    pub fn referenced_providers(&self, port: Port) -> Vec<&str> {
        let mut names = vec![self.selection(port).active.as_str()];
        for selection in self.repos.values() {
            if let Some(name) = selection.get(port)
                && !names.contains(&name)
            {
                names.push(name);
            }
        }
        names
    }

    /// Every problem in this configuration, collected rather than short-circuited.
    ///
    /// One bad file should cost one edit round-trip, not one per mistake.
    pub fn problems(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        self.daemon.collect(&mut problems);
        self.database.collect(&mut problems);

        for (name, instance) in &self.providers {
            instance.collect(name, &mut problems);
        }

        for port in Port::ALL {
            let active = &self.selection(port).active;
            if !self.providers.contains_key(active.as_str()) {
                problems.push(unknown_instance(
                    &format!("{port}.active"),
                    active,
                    &self.providers,
                ));
            }
        }

        for (repo, selection) in &self.repos {
            for port in Port::ALL {
                let Some(name) = selection.get(port) else {
                    continue;
                };
                if !self.providers.contains_key(name) {
                    problems.push(unknown_instance(
                        &format!("repos.{repo:?}.{port}"),
                        name,
                        &self.providers,
                    ));
                }
            }
        }

        self.indexing.collect(&mut problems);
        self.scan.collect(&mut problems);
        problems
    }

    /// Reject values that parse but cannot work.
    ///
    /// # Errors
    /// [`Error::InvalidConfig`] naming every offending field at once.
    pub fn validate(&self) -> Result<()> {
        let problems = self.problems();
        if problems.is_empty() {
            Ok(())
        } else {
            Err(Error::InvalidConfig(render(&problems)))
        }
    }

    /// A copy safe to print: any secret embedded in a value is masked.
    ///
    /// Config files hold no secrets by design, but `database.url` is a libpq
    /// URL and those carry a password. Redaction lives here so every printer
    /// (`flowspace3 config show`, a future doctor) inherits it.
    #[must_use]
    pub fn redacted(&self) -> Self {
        let mut copy = self.clone();
        copy.database.url = redact_url_password(&copy.database.url);
        copy
    }
}

/// Daemon transport settings. Only localhost HTTP in v1 (PRD req 33).
///
/// ```toml
/// [daemon]
/// url = "http://127.0.0.1:7373"
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DaemonConfig {
    /// Base URL the daemon serves and the CLI calls.
    pub url: String,
}

impl DaemonConfig {
    /// The default daemon endpoint, shared by daemon and CLI.
    pub const DEFAULT_URL: &'static str = "http://127.0.0.1:7373";

    fn collect(&self, problems: &mut Vec<Problem>) {
        if self.url.trim().is_empty() {
            problems.push(Problem::file(
                "daemon.url",
                "must not be empty",
                format!("url = \"{}\"", Self::DEFAULT_URL),
            ));
        } else if !self.url.starts_with("http://") && !self.url.starts_with("https://") {
            problems.push(Problem::file(
                "daemon.url",
                format!("{:?} is not an http(s) URL", self.url),
                format!("url = \"{}\"", Self::DEFAULT_URL),
            ));
        }
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            url: Self::DEFAULT_URL.to_string(),
        }
    }
}

/// The central store's connection settings.
///
/// ```toml
/// [database]
/// url = "postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3"
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DatabaseConfig {
    /// libpq-style connection URL.
    pub url: String,
}

impl DatabaseConfig {
    /// Matches the compose stack in `docker-compose.yml` (host port 5433, kept
    /// off 5432 so a machine-local Postgres is never shadowed).
    pub const DEFAULT_URL: &'static str =
        "postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3";

    fn collect(&self, problems: &mut Vec<Problem>) {
        let url = self.url.trim();
        if url.is_empty() {
            problems.push(Problem::file(
                "database.url",
                "must not be empty",
                format!("url = \"{}\"", Self::DEFAULT_URL),
            ));
        } else if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
            problems.push(Problem::file(
                "database.url",
                format!("{url:?} is not a postgres:// URL — fs3's store is Postgres (PRD req 4)"),
                format!("url = \"{}\"", Self::DEFAULT_URL),
            ));
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: Self::DEFAULT_URL.to_string(),
        }
    }
}

/// Which port a selection is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Port {
    /// [`crate::Embedder`].
    Embedder,
    /// [`crate::Summarizer`].
    Summarizer,
}

impl Port {
    /// Both ports, in the order `config show` prints them.
    pub const ALL: [Port; 2] = [Port::Embedder, Port::Summarizer];
}

impl std::fmt::Display for Port {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Port::Embedder => "embedder",
            Port::Summarizer => "summarizer",
        })
    }
}

/// Which registry instance a port uses.
///
/// The section carries a *name*, not a shape: choosing and configuring are
/// separate concerns, so two ports can share one instance (and one HTTP client)
/// by naming it twice.
///
/// ```toml
/// [embedder]
/// active = "small"
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PortSelection {
    /// The name of an instance in [`Config::providers`].
    pub active: String,
}

impl Default for PortSelection {
    /// Offline by default: a fresh machine runs the stack before it has keys.
    fn default() -> Self {
        Self {
            active: DEFAULT_PROVIDER.to_string(),
        }
    }
}

/// One repo's overrides of the default selections.
///
/// Keyed by repo identity in [`Config::repos`], so a monorepo of Rust can use a
/// different summarizer from a repo of prose without a second config file.
///
/// ```toml
/// [repos."github.com/AI-Substrate/flowspace3"]
/// summarizer = "offline"
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RepoSelection {
    /// Instance name for the embedder port, or `None` to use the default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedder: Option<String>,
    /// Instance name for the summarizer port, or `None` to use the default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summarizer: Option<String>,
}

impl RepoSelection {
    /// This repo's override for `port`, if it has one.
    #[must_use]
    pub fn get(&self, port: Port) -> Option<&str> {
        match port {
            Port::Embedder => self.embedder.as_deref(),
            Port::Summarizer => self.summarizer.as_deref(),
        }
    }
}

/// One configured provider instance — a named entry in the registry.
///
/// This enum *is* the IoC container's input: `daemon`'s composition root
/// matches on it (workshop 001 rule 4). `fake` is a first-class production
/// value, not a test hook — it is what makes the whole stack runnable offline
/// with no API keys (rule 5).
///
/// ```toml
/// [providers.small]
/// kind = "openai"
/// model = "text-embedding-3-small"
/// api_key_env = "OPENAI_API_KEY"   # the NAME of a variable, never a key
///
/// [providers.offline]
/// kind = "fake"
///
/// [providers.azure-luna]
/// kind = "azure_openai"
/// endpoint = "https://luna.openai.azure.com"
/// deployment = "text-embedding-3-large"   # the DEPLOYMENT, not the model
/// api_version = "2024-02-01"
/// dimensions = 1024                        # embeddings only
/// # no api_key_env => authenticate with Entra (az login / managed identity)
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderInstance {
    /// The deterministic fake from `fs3-testkit`. Offline, no keys.
    Fake,
    /// An OpenAI-shaped HTTP API.
    #[serde(rename = "openai")]
    OpenAi {
        /// Model name sent to the API.
        model: String,
        /// Override the API base, e.g. for Azure or a compatible gateway.
        #[serde(default)]
        api_base: Option<String>,
        /// Environment variable holding the API key. Keys never live in config
        /// files (PRD req 39 spirit: fs3 stores no secrets).
        #[serde(default = "ProviderInstance::default_api_key_env")]
        api_key_env: String,
    },
    /// One Azure OpenAI DEPLOYMENT.
    ///
    /// One instance per port, not per resource: Azure names the model by a
    /// deployment in the URL path, and the chat and embeddings deployments are
    /// different names with different `api-version`s in practice.
    #[serde(rename = "azure_openai")]
    AzureOpenAi {
        /// Resource root, e.g. `https://luna.openai.azure.com`.
        endpoint: String,
        /// The deployment name — NOT the model name. Getting this wrong is a
        /// 404 that reads like a wrong URL.
        deployment: String,
        /// Azure pins behaviour to this string, and it differs per route.
        api_version: String,
        /// Environment variable holding the api-key. ABSENT means Entra
        /// (managed identity, then `az login`) — which is the only way into a
        /// resource with key auth disabled.
        #[serde(default)]
        api_key_env: Option<String>,
        /// Requested embedding width, verified against the response. Embeddings
        /// only; ignored by the summarizer.
        #[serde(default)]
        dimensions: Option<usize>,
    },
}

impl ProviderInstance {
    /// The conventional key variable, used when config does not name one.
    pub const DEFAULT_API_KEY_ENV: &'static str = "OPENAI_API_KEY";

    /// The environment variable this instance reads its key from, if it needs
    /// one.
    ///
    /// Printers use it to report *whether* a key is present without ever
    /// touching the value.
    #[must_use]
    pub fn api_key_env(&self) -> Option<&str> {
        match self {
            ProviderInstance::Fake => None,
            ProviderInstance::OpenAi { api_key_env, .. } => Some(api_key_env),
            // `None` here means Entra rather than "needs no credential", which
            // is why the absence is reported honestly instead of as `Fake`'s
            // keyless case: a printer says "Entra", not "no key needed".
            ProviderInstance::AzureOpenAi { api_key_env, .. } => api_key_env.as_deref(),
        }
    }

    /// A one-word name for this instance's kind, for logs and `/health`.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            ProviderInstance::Fake => "fake",
            ProviderInstance::OpenAi { .. } => "openai",
            ProviderInstance::AzureOpenAi { .. } => "azure_openai",
        }
    }

    fn default_api_key_env() -> String {
        Self::DEFAULT_API_KEY_ENV.to_string()
    }

    /// Problems with this instance's own shape, named by registry key so the
    /// message points at the table to edit.
    fn collect(&self, name: &str, problems: &mut Vec<Problem>) {
        match self {
            ProviderInstance::Fake => {}
            ProviderInstance::OpenAi {
                model, api_key_env, ..
            } => {
                if model.trim().is_empty() {
                    problems.push(Problem::file(
                        format!("providers.{name}.model"),
                        "must name a model when kind = \"openai\"",
                        "model = \"text-embedding-3-small\"",
                    ));
                }
                if api_key_env.trim().is_empty() {
                    problems.push(Problem::file(
                        format!("providers.{name}.api_key_env"),
                        "must name the environment variable holding the key (never the key \
                         itself)",
                        format!("api_key_env = \"{}\"", Self::DEFAULT_API_KEY_ENV),
                    ));
                }
            }
            ProviderInstance::AzureOpenAi {
                endpoint,
                deployment,
                api_version,
                ..
            } => {
                if endpoint.trim().is_empty() {
                    problems.push(Problem::file(
                        format!("providers.{name}.endpoint"),
                        "must name the resource root",
                        "endpoint = \"https://NAME.openai.azure.com\"",
                    ));
                }
                // The confusable one: a 404 from Azure reads like a wrong URL,
                // and the cause is nearly always a MODEL name written where a
                // deployment name belongs. Saying so here costs nothing and
                // saves the hunt.
                if deployment.trim().is_empty() {
                    problems.push(Problem::file(
                        format!("providers.{name}.deployment"),
                        "must name the DEPLOYMENT (not the model); a wrong one is a 404 that \
                         reads like a wrong endpoint",
                        "deployment = \"text-embedding-3-large\"",
                    ));
                }
                if api_version.trim().is_empty() {
                    problems.push(Problem::file(
                        format!("providers.{name}.api_version"),
                        "must pin an api-version; Azure ties behaviour to it and it differs \
                         between the chat and embeddings routes",
                        "api_version = \"2024-02-01\"",
                    ));
                }
            }
        }
    }
}

impl Default for ProviderInstance {
    /// Offline by default: a fresh machine runs the stack before it has keys.
    fn default() -> Self {
        ProviderInstance::Fake
    }
}

/// The registry a fresh machine has: one offline fake, named [`DEFAULT_PROVIDER`].
fn default_providers() -> BTreeMap<String, ProviderInstance> {
    BTreeMap::from([(DEFAULT_PROVIDER.to_string(), ProviderInstance::Fake)])
}

/// "That name is not in the registry — here is what is."
fn unknown_instance(
    key: &str,
    name: &str,
    registry: &BTreeMap<String, ProviderInstance>,
) -> Problem {
    let configured: Vec<&str> = registry.keys().map(String::as_str).collect();
    Problem::file(
        key,
        format!(
            "names provider {name:?}, which is not configured; configured providers are: {}",
            if configured.is_empty() {
                "none".to_string()
            } else {
                configured.join(", ")
            }
        ),
        format!("[providers.{name}]\n         kind = \"fake\""),
    )
}

/// Indexing knobs.
///
/// ```toml
/// [indexing]
/// summary_min_lines = 10
/// debounce_seconds = 10
/// worker_concurrency = 4
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IndexingConfig {
    /// Size floor for per-element LLM summaries, in lines (PRD req 32).
    pub summary_min_lines: u32,
    /// How long a dirty file must settle before processing (PRD req 29).
    pub debounce_seconds: u64,
    /// How many jobs the runner claims at once.
    ///
    /// This is the QUEUE's concurrency: `claim_job`'s `SKIP LOCKED` hands N
    /// workers N different jobs, so the queue is the semaphore and the daemon
    /// needs no second one beside it. Four keeps an LLM call, an embedding
    /// batch and a scan in flight together without turning a rate limit into
    /// the normal case.
    ///
    /// It is deliberately NOT a number about provider parallelism, and the
    /// distinction is measured rather than assumed. For a network provider,
    /// in-flight requests are the lever: the first-light run against Azure did
    /// 110 embedding calls at this width, batching 16 texts per call, and both
    /// knobs bought real time because every call is a round trip. For the LOCAL
    /// embedder neither does — 32 concurrent tasks against one session measured
    /// 2.5% SLOWER than sequential (one ONNX session behind one mutex, already
    /// using every core), and one big batch was 4.4% slower than many small
    /// ones (fastembed batches internally at 256, so the kernels are the same).
    /// What works there is a POOL of independent sessions sharded by CHUNK:
    /// -40% at 16 sessions, while sharding by FILE was 12% slower than
    /// sequential because per-file work spans 1 to 23 chunks and one session
    /// took half the corpus. Measurements: pij-thorough-zakalwe, 2026-08-26,
    /// `docs/services/local-embeddings.md`.
    ///
    /// So a future concurrency combinator over `Arc<dyn Embedder>` cannot be
    /// one `max_concurrent` for both: the same number means "requests in
    /// flight" for one implementation and "sessions loaded" for the other, and
    /// it is wrong for one of them whichever way it is read. That knob belongs
    /// beside the provider, not here.
    pub worker_concurrency: usize,
}

impl IndexingConfig {
    fn collect(&self, problems: &mut Vec<Problem>) {
        if self.summary_min_lines == 0 {
            problems.push(Problem::file(
                "indexing.summary_min_lines",
                "must be at least 1",
                "summary_min_lines = 10",
            ));
        }
        if self.worker_concurrency == 0 {
            problems.push(Problem::file(
                "indexing.worker_concurrency",
                "must be at least 1 — zero workers would leave every job pending forever",
                "worker_concurrency = 4",
            ));
        }
    }
}

impl Default for IndexingConfig {
    fn default() -> Self {
        Self {
            summary_min_lines: 10,
            debounce_seconds: 10,
            worker_concurrency: 4,
        }
    }
}

/// Scanner policy: which files on disk are worth indexing at all.
///
/// ```toml
/// [scan]
/// max_file_bytes = 2000000
/// min_file_bytes = 1
/// respect_gitignore = true
/// include_hidden = false
/// follow_symlinks = false
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScanConfig {
    /// Skip files larger than this. Generated bundles and vendored blobs cost
    /// tokens and teach the index nothing.
    pub max_file_bytes: u64,
    /// Skip files smaller than this. The default skips empty files only.
    pub min_file_bytes: u64,
    /// Honour `.gitignore` while walking. Off means indexing build output.
    pub respect_gitignore: bool,
    /// Walk dot-files and dot-directories.
    pub include_hidden: bool,
    /// Follow symlinks. Off by default: a link loop is an infinite scan.
    pub follow_symlinks: bool,
}

impl ScanConfig {
    fn collect(&self, problems: &mut Vec<Problem>) {
        if self.max_file_bytes == 0 {
            problems.push(Problem::file(
                "scan.max_file_bytes",
                "must be at least 1 — zero would skip every file",
                "max_file_bytes = 2000000",
            ));
        } else if self.max_file_bytes <= self.min_file_bytes {
            problems.push(Problem::file(
                "scan.max_file_bytes",
                format!(
                    "({}) must be greater than scan.min_file_bytes ({})",
                    self.max_file_bytes, self.min_file_bytes
                ),
                "max_file_bytes = 2000000",
            ));
        }
    }
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            max_file_bytes: 2_000_000,
            min_file_bytes: 1,
            respect_gitignore: true,
            include_hidden: false,
            follow_symlinks: false,
        }
    }
}

/// Where a section's effective value came from — the debuggability anchor
/// behind `flowspace3 config show`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    /// Nothing said otherwise: the serde defaults above.
    Defaults,
    /// `config.toml` set at least one key in the section.
    File,
    /// An `FS3_*` variable set at least one key in the section.
    Env,
}

impl std::fmt::Display for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Layer::Defaults => "defaults",
            Layer::File => CONFIG_FILE_NAME,
            Layer::Env => "FS3_* environment",
        })
    }
}

/// The raw material [`resolve`] merges. IO-free by construction: the shell
/// reads the file and the environment, this crate decides what they mean.
#[derive(Debug)]
pub struct Sources<'a> {
    /// How to name the config file in error messages — its full path, when the
    /// caller has one.
    pub file_label: &'a str,
    /// The file's text, or `None` when it does not exist (which is not an
    /// error: it means "all defaults").
    pub file_text: Option<&'a str>,
    /// `FS3_*` overrides, already filtered by [`env_overrides`].
    pub env: &'a [(String, String)],
}

/// A merged configuration plus the story of where each section came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Effective {
    /// The configuration to run with.
    pub config: Config,
    /// Highest layer that touched each top-level section.
    pub layers: BTreeMap<String, Layer>,
}

impl Effective {
    /// The layer a section came from, defaulting to [`Layer::Defaults`].
    #[must_use]
    pub fn layer(&self, section: &str) -> Layer {
        self.layers.get(section).copied().unwrap_or(Layer::Defaults)
    }
}

/// One thing wrong with the configuration, said in a way that can be acted on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// Where the problem came from: the file, or an environment override.
    pub origin: Origin,
    /// The offending key — `scan.max_file_bytes` or `FS3_SCAN__MAX_FILE_BYTES`.
    pub key: String,
    /// What is wrong with it.
    pub message: String,
    /// A line that would be right, ready to paste.
    pub example: String,
}

/// Which layer a [`Problem`] belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// `config.toml` (or a default that the file should have overridden).
    File,
    /// An `FS3_*` environment override.
    Env,
}

impl Problem {
    fn file(
        key: impl Into<String>,
        message: impl Into<String>,
        example: impl Into<String>,
    ) -> Self {
        Self {
            origin: Origin::File,
            key: key.into(),
            message: message.into(),
            example: example.into(),
        }
    }

    fn env(key: impl Into<String>, message: impl Into<String>, example: impl Into<String>) -> Self {
        Self {
            origin: Origin::Env,
            key: key.into(),
            message: message.into(),
            example: example.into(),
        }
    }
}

/// Render every problem at once, each with the fix on the next line.
fn render(problems: &[Problem]) -> String {
    let mut out = String::new();
    let count = problems.len();
    out.push_str(&format!(
        "{count} problem{} found:",
        if count == 1 { "" } else { "s" }
    ));
    for problem in problems {
        out.push_str(&format!(
            "\n  - {}: {}\n    try: {}",
            problem.key, problem.message, problem.example
        ));
    }
    out
}

/// Keep only the environment variables that are fs3 config overrides:
/// [`ENV_PREFIX`] *and* [`ENV_NESTING`], e.g. `FS3_DATABASE__URL`.
///
/// Anything else with the prefix belongs to somebody else — `FS3_CONFIG_DIR`
/// steers the loader, `FS3_ACME_API_KEY` is a secret — and is left alone.
/// A name that does nest is passed on to [`resolve`], which refuses it if it
/// matches no key: an override that silently does nothing is worse than a
/// startup failure.
pub fn env_overrides<I, K, V>(vars: I) -> Vec<(String, String)>
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    let mut kept: Vec<(String, String)> = vars
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .filter(|(name, _)| name.starts_with(ENV_PREFIX) && name.contains(ENV_NESTING))
        .collect();
    // Deterministic order so two overrides of the same key resolve the same way
    // every run, and so error lists do not shuffle between runs.
    kept.sort();
    kept
}

/// Merge the layers into one effective configuration.
///
/// Precedence is defaults < file < environment. Every problem found anywhere in
/// the merge is reported together.
///
/// # Errors
/// [`Error::InvalidConfig`] listing every problem: bad TOML, an override naming
/// a key that does not exist, a value of the wrong type, or a parsed value that
/// cannot work.
pub fn resolve(sources: Sources<'_>) -> Result<Effective> {
    let file_table = match sources.file_text {
        None => Table::new(),
        Some(text) => text.parse::<Table>().map_err(|error| {
            Error::InvalidConfig(render(&[Problem::file(
                sources.file_label,
                format!("is not valid TOML: {error}"),
                "url = \"http://127.0.0.1:7373\"",
            )]))
        })?,
    };

    let mut merged = default_table();
    merge_tables(&mut merged, file_table.clone());

    let mut problems = Vec::new();
    let mut env_sections = Vec::new();
    for (name, value) in sources.env {
        match apply_override(&mut merged, name, value) {
            Ok(section) => env_sections.push(section),
            Err(problem) => problems.push(problem),
        }
    }
    if !problems.is_empty() {
        return Err(Error::InvalidConfig(render(&problems)));
    }

    let config: Config = Value::Table(merged).try_into().map_err(|error| {
        Error::InvalidConfig(render(&[Problem::file(
            sources.file_label,
            format!("does not describe fs3 configuration: {error}"),
            "provider = \"fake\"",
        )]))
    })?;
    config.validate()?;

    let mut layers = BTreeMap::new();
    for section in SECTIONS {
        let layer = if env_sections.iter().any(|touched| touched == section) {
            Layer::Env
        } else if file_table.contains_key(*section) {
            Layer::File
        } else {
            Layer::Defaults
        };
        layers.insert((*section).to_string(), layer);
    }

    Ok(Effective { config, layers })
}

/// The defaults as a TOML table — the bottom layer, and the type oracle the
/// environment layer coerces against.
fn default_table() -> Table {
    match Value::try_from(Config::default()) {
        Ok(Value::Table(table)) => table,
        // `Config` is a struct of structs: it always serializes to a table.
        _ => unreachable!("Config serializes to a TOML table"),
    }
}

/// Deep-merge `overlay` onto `base`, key by key.
///
/// Tables recurse; scalars replace. One special case: a table carrying a
/// different `provider` discriminant replaces its counterpart wholesale, because
/// merging the keys of two different enum arms produces a shape that belongs to
/// neither (`provider = "fake"` inheriting a stale `model`).
fn merge_tables(base: &mut Table, overlay: Table) {
    for (key, value) in overlay {
        match (base.get_mut(&key), value) {
            (Some(Value::Table(existing)), Value::Table(incoming)) => {
                if switches_kind(existing, &incoming) {
                    *existing = incoming;
                } else {
                    merge_tables(existing, incoming);
                }
            }
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

/// Does the overlay make this table a *different* tagged shape?
///
/// `[providers.x]` is an internally-tagged enum: merging a `kind = "fake"`
/// default with a `kind = "openai"` override key-by-key would leave a table
/// belonging to neither arm.
fn switches_kind(existing: &Table, incoming: &Table) -> bool {
    match (existing.get("kind"), incoming.get("kind")) {
        (Some(Value::String(before)), Some(Value::String(after))) => before != after,
        _ => false,
    }
}

/// Apply one `FS3_*` override to the merged table, returning the section it
/// touched.
fn apply_override(
    merged: &mut Table,
    name: &str,
    raw: &str,
) -> std::result::Result<String, Problem> {
    let path: Vec<String> = name
        .trim_start_matches(ENV_PREFIX)
        .split(ENV_NESTING)
        .map(str::to_ascii_lowercase)
        .filter(|segment| !segment.is_empty())
        .collect();

    let [section, rest @ ..] = path.as_slice() else {
        return Err(unknown_key(name, "names no configuration key"));
    };
    if rest.is_empty() {
        return Err(unknown_key(
            name,
            format!(
                "names the whole [{section}] section; override a key inside it, e.g. \
                 {ENV_PREFIX}{}{ENV_NESTING}URL",
                section.to_ascii_uppercase()
            ),
        ));
    }
    if !SECTIONS.contains(&section.as_str()) {
        return Err(unknown_key(
            name,
            format!(
                "names no configuration section; sections are: {}",
                SECTIONS.join(", ")
            ),
        ));
    }

    // Walk to the parent table, creating tables for arms that are not present
    // in the defaults (`[embedder] model` exists only under `provider =
    // "openai"`).
    let mut table = merged;
    for segment in &path[..path.len() - 1] {
        let entry = table
            .entry(segment.clone())
            .or_insert_with(|| Value::Table(Table::new()));
        match entry {
            Value::Table(inner) => table = inner,
            _ => {
                return Err(unknown_key(
                    name,
                    format!("{} is a value, not a section", path.join(".")),
                ));
            }
        }
    }

    let leaf = &path[path.len() - 1];
    let coerced = coerce(table.get(leaf), name, raw)?;
    table.insert(leaf.clone(), coerced);
    Ok(section.clone())
}

fn unknown_key(name: &str, message: impl Into<String>) -> Problem {
    Problem::env(
        name,
        message,
        format!("{ENV_PREFIX}DATABASE{ENV_NESTING}URL=postgres://…"),
    )
}

/// Turn an environment string into a TOML value of the type the target key
/// already has.
///
/// The *existing* value is the oracle, so nothing is guessed: `FS3_SCAN__…`
/// keys that are integers parse as integers, and everything unknown to the
/// defaults (provider-arm keys) stays a string.
fn coerce(existing: Option<&Value>, name: &str, raw: &str) -> std::result::Result<Value, Problem> {
    match existing {
        Some(Value::Integer(_)) => raw.trim().parse::<i64>().map(Value::Integer).map_err(|_| {
            Problem::env(
                name,
                format!("{raw:?} is not an integer"),
                format!("{name}=10"),
            )
        }),
        Some(Value::Boolean(_)) => raw.trim().parse::<bool>().map(Value::Boolean).map_err(|_| {
            Problem::env(
                name,
                format!("{raw:?} is not a boolean — use true or false"),
                format!("{name}=true"),
            )
        }),
        Some(Value::Float(_)) => raw.trim().parse::<f64>().map(Value::Float).map_err(|_| {
            Problem::env(
                name,
                format!("{raw:?} is not a number"),
                format!("{name}=1.5"),
            )
        }),
        Some(Value::Table(_)) | Some(Value::Array(_)) => Err(unknown_key(
            name,
            "names a section, not a key inside one".to_string(),
        )),
        _ => Ok(Value::String(raw.to_string())),
    }
}

/// Parse a `KEY=value` environment file — the secrets chain's format.
///
/// Blank lines and `#` comments are skipped, a leading `export ` is tolerated
/// (so a file can be `source`d by a shell too), and a quoted value has its
/// matching outer quotes removed. Values are secrets: this function never logs
/// and callers must never print what it returns.
///
/// # Errors
/// [`Error::InvalidConfig`] naming the line number when a line is not a
/// `KEY=value` pair.
pub fn parse_env_file(text: &str) -> Result<Vec<(String, String)>> {
    let mut pairs = Vec::new();
    let mut problems = Vec::new();

    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
        let Some((key, value)) = line.split_once('=') else {
            problems.push(Problem::file(
                format!("{SECRETS_FILE_NAME}:{}", index + 1),
                "is not a KEY=value line",
                "OPENAI_API_KEY=sk-…",
            ));
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            problems.push(Problem::file(
                format!("{SECRETS_FILE_NAME}:{}", index + 1),
                "has an empty variable name",
                "OPENAI_API_KEY=sk-…",
            ));
            continue;
        }
        pairs.push((key.to_string(), unquote(value.trim()).to_string()));
    }

    if problems.is_empty() {
        Ok(pairs)
    } else {
        Err(Error::InvalidConfig(render(&problems)))
    }
}

fn unquote(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'"' || bytes[0] == b'\'')
        && bytes[bytes.len() - 1] == bytes[0]
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

/// Mask the password in a `scheme://user:password@host/…` URL.
///
/// Everything else survives, because the host and database name are exactly
/// what someone reading `config show` is trying to check.
#[must_use]
pub fn redact_url_password(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let authority_start = scheme_end + 3;
    let authority_end = url[authority_start..]
        .find('/')
        .map_or(url.len(), |offset| authority_start + offset);
    let Some(at) = url[authority_start..authority_end].rfind('@') else {
        return url.to_string();
    };
    let userinfo = &url[authority_start..authority_start + at];
    let Some(colon) = userinfo.find(':') else {
        return url.to_string();
    };
    format!(
        "{}{}:{REDACTED}{}",
        &url[..authority_start],
        &userinfo[..colon],
        &url[authority_start + at..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        env_overrides(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string())),
        )
    }

    fn resolved(toml_text: &str, vars: &[(&str, &str)]) -> Result<Effective> {
        let env = env(vars);
        resolve(Sources {
            file_label: "/tmp/fs3/config.toml",
            file_text: Some(toml_text),
            env: &env,
        })
    }

    #[test]
    fn empty_config_is_the_offline_default() {
        let config = Config::from_toml_str("").unwrap();
        assert_eq!(config.providers, default_providers());
        assert_eq!(config.embedder.active, DEFAULT_PROVIDER);
        assert_eq!(config.summarizer.active, DEFAULT_PROVIDER);
        assert_eq!(
            config
                .provider(config.selected(Port::Embedder, None))
                .unwrap(),
            &ProviderInstance::Fake
        );
        assert_eq!(config.daemon.url, DaemonConfig::DEFAULT_URL);
        // PRD req 29: debounce defaults to 10 seconds.
        assert_eq!(config.indexing.debounce_seconds, 10);
        assert_eq!(config.scan, ScanConfig::default());
    }

    #[test]
    fn the_registry_holds_instances_and_the_ports_name_them() {
        let config = Config::from_toml_str(
            r#"
            [providers.small]
            kind = "openai"
            model = "text-embedding-3-small"

            [embedder]
            active = "small"

            [summarizer]
            active = "fake"
            "#,
        )
        .unwrap();

        assert_eq!(config.selected(Port::Embedder, None), "small");
        assert_eq!(
            config.provider("small").unwrap(),
            &ProviderInstance::OpenAi {
                model: "text-embedding-3-small".into(),
                api_base: None,
                api_key_env: ProviderInstance::DEFAULT_API_KEY_ENV.into(),
            }
        );
        // The offline fake is always in the registry, so `fake` never has to be
        // declared to be selectable.
        assert_eq!(config.provider("fake").unwrap(), &ProviderInstance::Fake);
    }

    #[test]
    fn two_ports_may_share_one_instance() {
        let config = Config::from_toml_str(
            r#"
            [providers.one]
            kind = "openai"
            model = "gpt-4o-mini"

            [embedder]
            active = "one"

            [summarizer]
            active = "one"
            "#,
        )
        .unwrap();

        assert_eq!(config.referenced_providers(Port::Embedder), vec!["one"]);
        assert_eq!(config.referenced_providers(Port::Summarizer), vec!["one"]);
    }

    #[test]
    fn a_repo_may_name_a_different_instance() {
        let config = Config::from_toml_str(
            r#"
            [providers.big]
            kind = "openai"
            model = "gpt-4o"

            [repos."github.com/AI-Substrate/flowspace3"]
            summarizer = "big"
            "#,
        )
        .unwrap();

        let repo = Some("github.com/AI-Substrate/flowspace3");
        assert_eq!(config.selected(Port::Summarizer, repo), "big");
        // The other port, and every other repo, still get the default.
        assert_eq!(config.selected(Port::Embedder, repo), DEFAULT_PROVIDER);
        assert_eq!(
            config.selected(Port::Summarizer, Some("some/other/repo")),
            DEFAULT_PROVIDER
        );
        assert_eq!(config.selected(Port::Summarizer, None), DEFAULT_PROVIDER);

        // Both instances are referenced, so both get constructed — once each.
        assert_eq!(
            config.referenced_providers(Port::Summarizer),
            vec![DEFAULT_PROVIDER, "big"]
        );
    }

    #[test]
    fn an_unknown_instance_name_lists_the_configured_ones() {
        let err = Config::from_toml_str(
            r#"
            [providers.small]
            kind = "openai"
            model = "m"

            [embedder]
            active = "smal"
            "#,
        )
        .unwrap_err();

        let message = err.to_string();
        assert!(message.contains("embedder.active"), "{message}");
        assert!(message.contains("\"smal\""), "{message}");
        assert!(
            message.contains("configured providers are: fake, small"),
            "{message}"
        );
    }

    #[test]
    fn an_unknown_instance_in_a_repo_override_names_the_repo() {
        let err = Config::from_toml_str(
            r#"
            [repos."github.com/acme/thing"]
            embedder = "nope"
            "#,
        )
        .unwrap_err();

        let message = err.to_string();
        assert!(message.contains("github.com/acme/thing"), "{message}");
        assert!(message.contains("embedder"), "{message}");
        assert!(message.contains("\"nope\""), "{message}");
    }

    #[test]
    fn an_instance_failing_its_kind_shape_names_the_bad_key() {
        let err = Config::from_toml_str(
            r#"
            [providers.small]
            kind = "openai"
            model = ""
            api_key_env = ""
            "#,
        )
        .unwrap_err();

        let message = err.to_string();
        assert!(message.contains("providers.small.model"), "{message}");
        assert!(message.contains("providers.small.api_key_env"), "{message}");
        assert!(message.contains("2 problems"), "{message}");
    }

    #[test]
    fn an_unselected_instance_is_still_validated_but_never_referenced() {
        let config = Config::from_toml_str(
            r#"
            [providers.spare]
            kind = "openai"
            model = "gpt-4o"
            api_key_env = "SPARE_KEY"
            "#,
        )
        .unwrap();

        assert!(config.providers.contains_key("spare"));
        // Declaring an instance must not cost an API key: nothing references it,
        // so the composition root never constructs it.
        assert_eq!(
            config.referenced_providers(Port::Embedder),
            vec![DEFAULT_PROVIDER]
        );
    }

    #[test]
    fn unknown_keys_are_a_typo_not_a_feature() {
        let err = Config::from_toml_str(
            r#"
            [daemon]
            url = "http://x"
            prot = 1
            "#,
        )
        .unwrap_err();
        assert!(matches!(err, Error::InvalidConfig(_)), "got {err:?}");
    }

    #[test]
    fn zero_summary_floor_is_rejected() {
        let err = Config::from_toml_str(
            "[indexing]
summary_min_lines = 0
",
        )
        .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("indexing.summary_min_lines"), "{message}");
        assert!(message.contains("must be at least 1"), "{message}");
    }

    #[test]
    fn round_trips_through_toml() {
        let config = Config::default();
        let text = toml::to_string(&config).unwrap();
        assert_eq!(Config::from_toml_str(&text).unwrap(), config);
    }

    #[test]
    fn every_problem_is_reported_at_once() {
        let err = Config::from_toml_str(
            r#"
            [daemon]
            url = ""

            [database]
            url = "mysql://nope"

            [indexing]
            summary_min_lines = 0

            [scan]
            max_file_bytes = 0
            "#,
        )
        .unwrap_err();

        let message = err.to_string();
        for key in [
            "daemon.url",
            "database.url",
            "indexing.summary_min_lines",
            "scan.max_file_bytes",
        ] {
            assert!(message.contains(key), "{key} missing from:\n{message}");
        }
        assert!(message.contains("4 problems"), "{message}");
        // Actionable: every problem carries a line you can paste.
        assert_eq!(message.matches("try: ").count(), 4, "{message}");
    }

    #[test]
    fn the_environment_beats_the_file() {
        let effective = resolved(
            "[database]\nurl = \"postgres://from-file/db\"\n",
            &[("FS3_DATABASE__URL", "postgres://from-env/db")],
        )
        .unwrap();

        assert_eq!(effective.config.database.url, "postgres://from-env/db");
        assert_eq!(effective.layer("database"), Layer::Env);
        assert_eq!(effective.layer("daemon"), Layer::Defaults);
    }

    #[test]
    fn the_file_beats_the_defaults() {
        let effective = resolved("[daemon]\nurl = \"http://127.0.0.1:9999\"\n", &[]).unwrap();
        assert_eq!(effective.config.daemon.url, "http://127.0.0.1:9999");
        assert_eq!(effective.layer("daemon"), Layer::File);
    }

    #[test]
    fn overrides_are_typed_by_the_key_they_target() {
        let effective = resolved(
            "",
            &[
                ("FS3_SCAN__MAX_FILE_BYTES", "4096"),
                ("FS3_SCAN__RESPECT_GITIGNORE", "false"),
                ("FS3_INDEXING__DEBOUNCE_SECONDS", "2"),
            ],
        )
        .unwrap();

        assert_eq!(effective.config.scan.max_file_bytes, 4096);
        assert!(!effective.config.scan.respect_gitignore);
        assert_eq!(effective.config.indexing.debounce_seconds, 2);
    }

    #[test]
    fn an_override_can_switch_the_active_instance() {
        let effective = resolved(
            "[providers.small]\nkind = \"openai\"\nmodel = \"text-embedding-3-small\"\n",
            &[("FS3_EMBEDDER__ACTIVE", "small")],
        )
        .unwrap();

        assert_eq!(effective.config.selected(Port::Embedder, None), "small");
        assert_eq!(effective.layer("embedder"), Layer::Env);
        assert_eq!(effective.layer("providers"), Layer::File);
    }

    #[test]
    fn an_override_can_reshape_an_instance_in_the_registry() {
        let effective = resolved(
            "",
            &[
                ("FS3_PROVIDERS__FAKE__KIND", "openai"),
                ("FS3_PROVIDERS__FAKE__MODEL", "gpt-4o-mini"),
            ],
        )
        .unwrap();

        assert!(matches!(
            effective.config.provider("fake").unwrap(),
            ProviderInstance::OpenAi { .. }
        ));
    }

    #[test]
    fn switching_the_kind_in_the_file_does_not_inherit_the_other_arms_keys() {
        // The default `fake` instance has no `model`; redefining it as openai
        // must replace the table rather than merge into it, or the result is a
        // shape belonging to neither arm.
        let config =
            Config::from_toml_str("[providers.fake]\nkind = \"openai\"\nmodel = \"gpt-4o-mini\"\n")
                .unwrap();
        assert!(matches!(
            config.provider("fake").unwrap(),
            ProviderInstance::OpenAi { .. }
        ));
    }

    #[test]
    fn a_typo_in_an_override_is_refused_by_name() {
        let err = resolved("", &[("FS3_DATABSE__URL", "postgres://x/y")]).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("FS3_DATABSE__URL"), "{message}");
        assert!(message.contains("sections are"), "{message}");
    }

    #[test]
    fn a_mistyped_override_value_says_what_was_expected() {
        let err = resolved("", &[("FS3_SCAN__MAX_FILE_BYTES", "big")]).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("FS3_SCAN__MAX_FILE_BYTES"), "{message}");
        assert!(message.contains("not an integer"), "{message}");
    }

    #[test]
    fn every_bad_override_is_reported_at_once() {
        let err = resolved(
            "",
            &[
                ("FS3_SCAN__MAX_FILE_BYTES", "big"),
                ("FS3_SCAN__RESPECT_GITIGNORE", "yes"),
            ],
        )
        .unwrap_err();
        assert!(err.to_string().contains("2 problems"), "{err}");
    }

    /// The override namespace must not eat the secrets namespace: a user is
    /// free to call their key variable `FS3_ACME_API_KEY`, and the loader's own
    /// `FS3_CONFIG_DIR` is not configuration either.
    #[test]
    fn only_nested_names_are_overrides() {
        let kept = env(&[
            ("FS3_CONFIG_DIR", "/tmp/whatever"),
            ("FS3_ACME_API_KEY", "sk-secret"),
            ("PATH", "/usr/bin"),
            ("FS3_DAEMON__URL", "http://127.0.0.1:1"),
        ]);
        assert_eq!(
            kept,
            vec![(
                "FS3_DAEMON__URL".to_string(),
                "http://127.0.0.1:1".to_string()
            )]
        );
    }

    #[test]
    fn a_missing_file_is_all_defaults() {
        let effective = resolve(Sources {
            file_label: "/tmp/fs3/config.toml",
            file_text: None,
            env: &[],
        })
        .unwrap();
        assert_eq!(effective.config, Config::default());
        assert!(effective.layers.values().all(|l| *l == Layer::Defaults));
    }

    #[test]
    fn secrets_files_parse_shell_shaped_lines() {
        let pairs = parse_env_file(
            "# a comment\n\nOPENAI_API_KEY=sk-123\nexport AZURE_KEY=\"quoted value\"\nEMPTY=\n",
        )
        .unwrap();

        assert_eq!(
            pairs,
            vec![
                ("OPENAI_API_KEY".to_string(), "sk-123".to_string()),
                ("AZURE_KEY".to_string(), "quoted value".to_string()),
                ("EMPTY".to_string(), String::new()),
            ]
        );
    }

    #[test]
    fn a_secrets_line_without_an_equals_is_refused_by_line_number() {
        let err = parse_env_file("OPENAI_API_KEY=sk-1\njust-a-word\n").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("secrets.env:2"), "{message}");
        // The value must never appear in the complaint.
        assert!(!message.contains("sk-1"), "{message}");
    }

    #[test]
    fn printing_a_config_masks_the_database_password() {
        let config = Config::default();
        let printed = toml::to_string(&config.redacted()).unwrap();
        assert!(!printed.contains("flowspace3:flowspace3@"), "{printed}");
        assert!(printed.contains("127.0.0.1:5433/flowspace3"), "{printed}");
        assert!(printed.contains(REDACTED), "{printed}");
    }

    #[test]
    fn redaction_leaves_passwordless_urls_alone() {
        assert_eq!(
            redact_url_password("postgres://host:5433/db"),
            "postgres://host:5433/db"
        );
        assert_eq!(
            redact_url_password("postgres://user@host/db"),
            "postgres://user@host/db"
        );
        assert_eq!(
            redact_url_password("postgres://user:pw@host/db"),
            format!("postgres://user:{REDACTED}@host/db")
        );
    }
}
