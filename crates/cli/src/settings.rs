//! Reading configuration on the CLI side.
//!
//! Uses the same `fs3_core` types and the same layering the daemon does, from
//! the same directory, so the two can never disagree. Only the IO lives here:
//! read the file, collect `FS3_*` from the environment, hand both to
//! [`fs3_core::resolve`]. A missing config file means defaults, which is a
//! working local stack.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fs3_core::{
    CONFIG_DIR_ENV, CONFIG_FILE_NAME, DEFAULT_CONFIG_SUBDIR, Effective, SECRETS_FILE_NAME, Sources,
    env_overrides, parse_env_file, resolve,
};

/// The directory the CLI reads configuration from — identical rules to the
/// daemon's: `FS3_CONFIG_DIR`, else `~/.config/flowspace3` (PRD req 28).
///
/// # Errors
/// When neither `FS3_CONFIG_DIR` nor `HOME` is set.
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

/// Where the config file is, whether or not it exists.
#[must_use]
pub fn config_path(dir: &Path) -> PathBuf {
    dir.join(CONFIG_FILE_NAME)
}

/// Where the secrets file is, whether or not it exists.
#[must_use]
pub fn secrets_path(dir: &Path) -> PathBuf {
    dir.join(SECRETS_FILE_NAME)
}

/// Load the effective configuration from a specific directory: defaults, then
/// `config.toml`, then `FS3_*` overrides — with the layer each section came
/// from.
///
/// # Errors
/// When the file exists but is unreadable, or the merged configuration is not
/// usable. The message lists every problem at once.
pub fn load_effective_from(dir: &Path) -> Result<Effective> {
    let path = config_path(dir);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(anyhow::Error::new(error))
                .with_context(|| format!("cannot read {}", path.display()));
        }
    };

    let label = path.display().to_string();
    let env = env_overrides(std::env::vars());
    resolve(Sources {
        file_label: &label,
        file_text: text.as_deref(),
        env: &env,
    })
    .with_context(|| format!("configuration from {}", path.display()))
}

/// Load `secrets.env` from `dir` into the process environment, returning the
/// variable **names** it supplied.
///
/// Same contract as the daemon's loader: config names the variable, the
/// environment carries the value, and a variable already set is never
/// overwritten. Values are never returned, logged, or printed.
///
/// # Safety and threads
/// Mutates the process environment, so it must run before any runtime or
/// thread exists — `main` calls it first.
///
/// # Errors
/// When the file exists but cannot be read or contains a line that is not
/// `KEY=value`.
pub fn load_secrets_from(dir: &Path) -> Result<Vec<String>> {
    let path = secrets_path(dir);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(anyhow::Error::new(error))
                .with_context(|| format!("cannot read {}", path.display()));
        }
    };

    let pairs = parse_env_file(&text).with_context(|| format!("reading {}", path.display()))?;
    let mut applied = Vec::new();
    for (name, value) in pairs {
        if std::env::var_os(&name).is_some() {
            continue;
        }
        // SAFETY: called from the first statement of `main`, before the Tokio
        // runtime is built, so no other thread can be reading the environment.
        unsafe {
            std::env::set_var(&name, value);
        }
        applied.push(name);
    }
    Ok(applied)
}

/// Read the daemon URL from a specific config directory.
///
/// # Errors
/// When the config file exists but is unreadable or invalid.
pub fn daemon_url_from(dir: &Path) -> Result<String> {
    Ok(load_effective_from(dir)?.config.daemon.url)
}

/// Read the daemon URL from the discovered config directory.
///
/// # Errors
/// When the directory cannot be found or the config file is unusable.
pub fn daemon_url() -> Result<String> {
    daemon_url_from(&config_dir()?)
}
