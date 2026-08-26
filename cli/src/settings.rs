//! Reading the one setting the CLI needs: where the daemon is.
//!
//! Uses the same `fs3_core::Config` types the daemon does, from the same file,
//! so the two can never disagree about the endpoint. A missing config file
//! means defaults, which is a working local stack.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fs3_core::{CONFIG_DIR_ENV, CONFIG_FILE_NAME, Config, DEFAULT_CONFIG_SUBDIR};

/// The directory the CLI reads configuration from — identical rules to the
/// daemon's: `FS3_CONFIG_DIR`, else `~/.config/flowspace3` (PRD req 28).
pub fn config_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os(CONFIG_DIR_ENV)
        && !dir.is_empty()
    {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("cannot determine the config directory: neither FS3_CONFIG_DIR nor HOME is set")?;
    Ok(home.join(".config").join(DEFAULT_CONFIG_SUBDIR))
}

/// Read the daemon URL from a specific config directory.
///
/// # Errors
/// When the config file exists but is unreadable or invalid.
pub fn daemon_url_from(dir: &Path) -> Result<String> {
    let path = dir.join(CONFIG_FILE_NAME);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Config::default().daemon.url);
        }
        Err(error) => {
            return Err(anyhow::Error::new(error))
                .with_context(|| format!("cannot read {}", path.display()));
        }
    };

    let config = Config::from_toml_str(&text)
        .with_context(|| format!("{} is not valid configuration", path.display()))?;
    Ok(config.daemon.url)
}

/// Read the daemon URL from the discovered config directory.
///
/// # Errors
/// When the directory cannot be found or the config file is unusable.
pub fn daemon_url() -> Result<String> {
    daemon_url_from(&config_dir()?)
}
