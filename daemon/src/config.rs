//! Finding and reading `~/.config/flowspace3/config.toml`.
//!
//! The *types* live in `fs3-core` and are pure. The IO lives here, because
//! reading a file is an effect and effects live at the edges. `fs3-cli` reads
//! the same file through the same core types.

use std::path::{Path, PathBuf};

use fs3_core::{CONFIG_DIR_ENV, CONFIG_FILE_NAME, Config, DEFAULT_CONFIG_SUBDIR};

/// Why configuration could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// No config directory could be determined at all.
    #[error(
        "cannot determine the config directory: neither {CONFIG_DIR_ENV} nor a home directory \
         is set. Set {CONFIG_DIR_ENV} to a directory containing {CONFIG_FILE_NAME}."
    )]
    NoConfigDir,
    /// The config file exists but could not be read.
    #[error("cannot read {path}: {source}")]
    Unreadable {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying IO failure.
        source: std::io::Error,
    },
    /// The config file is not valid configuration.
    #[error("{path} is not valid configuration: {source}")]
    Invalid {
        /// The offending file.
        path: PathBuf,
        /// What core said about it.
        source: fs3_core::Error,
    },
}

/// The directory fs3 reads configuration from.
///
/// `FS3_CONFIG_DIR` wins when set — that is how tests and throwaway
/// environments get an isolated config without touching the user's. Otherwise
/// it is `~/.config/flowspace3` (PRD req 28).
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

/// Load configuration from a specific directory.
///
/// A missing file is not an error: it means "all defaults", which is a fully
/// working offline stack (`provider = "fake"`). A malformed file *is* an error
/// — silently falling back to defaults would hide a typo behind a working
/// daemon.
///
/// # Errors
/// [`ConfigError::Unreadable`] or [`ConfigError::Invalid`].
pub fn load_config_from(dir: &Path) -> Result<Config, ConfigError> {
    let path = dir.join(CONFIG_FILE_NAME);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Config::default());
        }
        Err(source) => return Err(ConfigError::Unreadable { path, source }),
    };
    Config::from_toml_str(&text).map_err(|source| ConfigError::Invalid { path, source })
}

/// Load configuration from the discovered config directory.
///
/// # Errors
/// [`ConfigError`] when the directory cannot be found or the file is unusable.
pub fn load_config() -> Result<Config, ConfigError> {
    load_config_from(&config_dir()?)
}
