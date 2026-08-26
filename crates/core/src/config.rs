//! Configuration *types*. Pure: parsing a string is allowed here, reading a
//! file is not.
//!
//! All configuration lives in `~/.config/flowspace3/` as files — never in the
//! DB (PRD reqs 28, 39). Discovery and file reading belong to the shell
//! (`fs3-daemon`, `fs3-cli`); the shape and its defaults live here so both
//! read the *same* types and can never drift apart.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Environment variable that overrides the config directory. Tests and
/// throwaway environments set it; production leaves it unset.
pub const CONFIG_DIR_ENV: &str = "FS3_CONFIG_DIR";

/// The config file inside the config directory.
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// Subdirectory of the user's config home when [`CONFIG_DIR_ENV`] is unset.
pub const DEFAULT_CONFIG_SUBDIR: &str = "flowspace3";

/// The whole of fs3's configuration.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Where the daemon listens, and where the CLI looks for it.
    pub daemon: DaemonConfig,
    /// The central Postgres + pgvector store (PRD req 4).
    pub database: DatabaseConfig,
    /// Which [`crate::Embedder`] the composition root wires.
    pub embedder: ProviderConfig,
    /// Which [`crate::Summarizer`] the composition root wires.
    pub summarizer: ProviderConfig,
    /// Knobs the indexing pipeline reads.
    pub indexing: IndexingConfig,
}

impl Config {
    /// Parse configuration from TOML text.
    ///
    /// # Errors
    /// [`Error::InvalidConfig`] when the text is not valid TOML for this shape,
    /// or when the values do not describe a usable system.
    pub fn from_toml_str(toml_text: &str) -> Result<Self> {
        let config: Config =
            toml::from_str(toml_text).map_err(|e| Error::InvalidConfig(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    /// Reject values that parse but cannot work.
    ///
    /// # Errors
    /// [`Error::InvalidConfig`] naming the offending field.
    pub fn validate(&self) -> Result<()> {
        if self.daemon.url.trim().is_empty() {
            return Err(Error::InvalidConfig("daemon.url must not be empty".into()));
        }
        if self.database.url.trim().is_empty() {
            return Err(Error::InvalidConfig(
                "database.url must not be empty".into(),
            ));
        }
        if self.indexing.summary_min_lines == 0 {
            return Err(Error::InvalidConfig(
                "indexing.summary_min_lines must be at least 1".into(),
            ));
        }
        Ok(())
    }
}

/// Daemon transport settings. Only localhost HTTP in v1 (PRD req 33).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DaemonConfig {
    /// Base URL the daemon serves and the CLI calls.
    pub url: String,
}

impl DaemonConfig {
    /// The default daemon endpoint, shared by daemon and CLI.
    pub const DEFAULT_URL: &'static str = "http://127.0.0.1:7373";
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            url: Self::DEFAULT_URL.to_string(),
        }
    }
}

/// The central store's connection settings.
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
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: Self::DEFAULT_URL.to_string(),
        }
    }
}

/// Which implementation of a port to wire.
///
/// This enum *is* the IoC container's input: `daemon`'s composition root
/// matches on it (workshop 001 rule 4). `fake` is a first-class production
/// value, not a test hook — it is what makes the whole stack runnable offline
/// with no API keys (rule 5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderConfig {
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
        #[serde(default = "ProviderConfig::default_api_key_env")]
        api_key_env: String,
    },
}

impl ProviderConfig {
    /// The conventional key variable, used when config does not name one.
    pub const DEFAULT_API_KEY_ENV: &'static str = "OPENAI_API_KEY";

    fn default_api_key_env() -> String {
        Self::DEFAULT_API_KEY_ENV.to_string()
    }
}

impl Default for ProviderConfig {
    /// Offline by default: a fresh machine runs the stack before it has keys.
    fn default() -> Self {
        ProviderConfig::Fake
    }
}

/// Indexing knobs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IndexingConfig {
    /// Size floor for per-element LLM summaries, in lines (PRD req 32).
    pub summary_min_lines: u32,
    /// How long a dirty file must settle before processing (PRD req 29).
    pub debounce_seconds: u64,
}

impl Default for IndexingConfig {
    fn default() -> Self {
        Self {
            summary_min_lines: 10,
            debounce_seconds: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_is_the_offline_default() {
        let config = Config::from_toml_str("").unwrap();
        assert_eq!(config.embedder, ProviderConfig::Fake);
        assert_eq!(config.summarizer, ProviderConfig::Fake);
        assert_eq!(config.daemon.url, DaemonConfig::DEFAULT_URL);
        // PRD req 29: debounce defaults to 10 seconds.
        assert_eq!(config.indexing.debounce_seconds, 10);
    }

    #[test]
    fn provider_tables_select_an_arm() {
        let config = Config::from_toml_str(
            r#"
            [embedder]
            provider = "openai"
            model = "text-embedding-3-small"

            [summarizer]
            provider = "fake"
            "#,
        )
        .unwrap();

        assert_eq!(
            config.embedder,
            ProviderConfig::OpenAi {
                model: "text-embedding-3-small".into(),
                api_base: None,
                api_key_env: ProviderConfig::DEFAULT_API_KEY_ENV.into(),
            }
        );
        assert_eq!(config.summarizer, ProviderConfig::Fake);
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
        assert_eq!(
            err,
            Error::InvalidConfig("indexing.summary_min_lines must be at least 1".into())
        );
    }

    #[test]
    fn round_trips_through_toml() {
        let config = Config::default();
        let text = toml::to_string(&config).unwrap();
        assert_eq!(Config::from_toml_str(&text).unwrap(), config);
    }
}
