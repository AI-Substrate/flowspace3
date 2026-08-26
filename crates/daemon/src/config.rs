//! Finding and reading `~/.config/flowspace3/`.
//!
//! The *types* and the layering live in `fs3-core` and are pure. The IO lives
//! here, because reading a file is an effect and effects live at the edges.
//! `fs3-cli` reads the same directory through the same core types.
//!
//! Three things are loaded, in this order:
//!
//! 1. [`load_secrets_from`] — `secrets.env` into the process environment, so a
//!    provider's `api_key_env` can be satisfied by a file the user owns.
//! 2. [`load_effective_from`] — `config.toml` merged under the `FS3_*`
//!    environment overrides.
//! 3. [`AppState::from_config`](crate::AppState) — the composition root, which
//!    hands each service the narrow section it needs.

use std::path::{Path, PathBuf};

use fs3_core::{
    CONFIG_DIR_ENV, CONFIG_FILE_NAME, Config, DEFAULT_CONFIG_SUBDIR, Effective, SECRETS_FILE_NAME,
    Sources, env_overrides, parse_env_file, resolve,
};

/// Why configuration could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// No config directory could be determined at all.
    #[error(
        "cannot determine the config directory: neither {CONFIG_DIR_ENV} nor a home directory \
         is set. Set {CONFIG_DIR_ENV} to a directory containing {CONFIG_FILE_NAME}."
    )]
    NoConfigDir,
    /// A file exists but could not be read.
    #[error("cannot read {path}: {source}")]
    Unreadable {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying IO failure.
        source: std::io::Error,
    },
    /// The configuration is not usable. The message lists every problem.
    #[error("{path} is not valid configuration: {source}")]
    Invalid {
        /// The offending file.
        path: PathBuf,
        /// What core said about it — every problem, not just the first.
        source: fs3_core::Error,
    },
}

/// The directory fs3 reads configuration from.
///
/// `FS3_CONFIG_DIR` wins when set — that is how tests and throwaway
/// environments get an isolated config without touching the user's. Otherwise
/// it is `~/.config/flowspace3` (PRD req 28).
///
/// # Errors
/// [`ConfigError::NoConfigDir`] when neither the override nor `HOME` is set.
pub fn config_dir() -> Result<PathBuf, ConfigError> {
    if let Some(dir) = std::env::var_os(CONFIG_DIR_ENV)
        && !dir.is_empty()
    {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(ConfigError::NoConfigDir)?;
    Ok(home.join(".config").join(DEFAULT_CONFIG_SUBDIR))
}

/// Load the effective configuration from a specific directory: the file merged
/// under the `FS3_*` environment overrides, with the layer each section came
/// from.
///
/// A missing file is not an error: it means "all defaults", which is a fully
/// working offline stack (`provider = "fake"`). It is logged at INFO naming the
/// path to create, because "nothing happened" should still say where to look. A
/// malformed file *is* an error — silently falling back to defaults would hide
/// a typo behind a working daemon.
///
/// # Errors
/// [`ConfigError::Unreadable`] or [`ConfigError::Invalid`].
pub fn load_effective_from(dir: &Path) -> Result<Effective, ConfigError> {
    let path = dir.join(CONFIG_FILE_NAME);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            tracing::info!(
                path = %path.display(),
                "no config file: running on defaults. Create that file to change anything."
            );
            None
        }
        Err(source) => return Err(ConfigError::Unreadable { path, source }),
    };

    let label = path.display().to_string();
    let env = env_overrides(std::env::vars());
    resolve(Sources {
        file_label: &label,
        file_text: text.as_deref(),
        env: &env,
    })
    .map_err(|source| ConfigError::Invalid { path, source })
}

/// Load configuration from a specific directory, discarding the provenance.
///
/// # Errors
/// [`ConfigError::Unreadable`] or [`ConfigError::Invalid`].
pub fn load_config_from(dir: &Path) -> Result<Config, ConfigError> {
    Ok(load_effective_from(dir)?.config)
}

/// Load configuration from the discovered config directory.
///
/// # Errors
/// [`ConfigError`] when the directory cannot be found or the file is unusable.
pub fn load_config() -> Result<Config, ConfigError> {
    load_config_from(&config_dir()?)
}

/// What [`load_secrets_from`] did, in terms safe to log.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SecretsLoaded {
    /// The file consulted, whether or not it existed.
    pub path: PathBuf,
    /// Whether the file was there. Absent is the normal case.
    pub present: bool,
    /// Variables this file put into the environment — **names only**. A value
    /// read from here never leaves this function.
    pub applied: Vec<String>,
    /// Variables the file named that the environment already had. The process
    /// environment wins, so these were left alone.
    pub already_set: Vec<String>,
}

/// Load `secrets.env` from `dir` into the process environment.
///
/// The secrets chain is deliberately *separate* from `config.toml`: config
/// names the variable (`api_key_env`), the environment carries the value. A
/// variable already present in the environment is never overwritten — an
/// explicit `KEY=… fs3-daemon` beats a file the user forgot about.
///
/// # Safety and threads
/// This mutates the process environment, which Rust 2024 makes `unsafe`
/// because another thread may be reading it. Callers **must** call this before
/// starting a runtime or spawning threads; both binaries do it as the first
/// step of `main`.
///
/// # Errors
/// [`ConfigError::Unreadable`] if the file exists but cannot be read,
/// [`ConfigError::Invalid`] if a line is not `KEY=value`.
pub fn load_secrets_from(dir: &Path) -> Result<SecretsLoaded, ConfigError> {
    let path = dir.join(SECRETS_FILE_NAME);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SecretsLoaded {
                path,
                present: false,
                ..SecretsLoaded::default()
            });
        }
        Err(source) => return Err(ConfigError::Unreadable { path, source }),
    };

    let pairs = parse_env_file(&text).map_err(|source| ConfigError::Invalid {
        path: path.clone(),
        source,
    })?;

    let mut loaded = SecretsLoaded {
        path,
        present: true,
        ..SecretsLoaded::default()
    };
    for (name, value) in pairs {
        if std::env::var_os(&name).is_some() {
            loaded.already_set.push(name);
            continue;
        }
        // SAFETY: documented above — this runs before any runtime or thread
        // exists, so no other thread can be reading the environment.
        unsafe {
            std::env::set_var(&name, value);
        }
        loaded.applied.push(name);
    }
    Ok(loaded)
}

/// Load secrets from the discovered config directory.
///
/// # Errors
/// [`ConfigError`] when the directory cannot be found or the file is unusable.
pub fn load_secrets() -> Result<SecretsLoaded, ConfigError> {
    load_secrets_from(&config_dir()?)
}
