//! `fs3-daemon` — the fs3 background service.
//!
//! Reads configuration, wires the composition root, serves HTTP on localhost.

use anyhow::{Context, Result};
use fs3_daemon::{AppState, config, http, wiring};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fs3_daemon=info,tower_http=info".into()),
        )
        .init();

    let directory = config::config_dir().context("locating the fs3 config directory")?;
    let configuration = config::load_config_from(&directory)
        .with_context(|| format!("loading configuration from {}", directory.display()))?;

    let address = bind_address(&configuration.daemon.url)?;
    tracing::info!(
        config = %directory.display(),
        embedder = wiring::describe(&configuration.embedder),
        summarizer = wiring::describe(&configuration.summarizer),
        "fs3 daemon starting"
    );

    let state = AppState::from_config(configuration).context("wiring the composition root")?;
    http::serve(state, &address).await
}

/// Turn the configured daemon URL into a bind address.
fn bind_address(url: &str) -> Result<String> {
    let without_scheme = url
        .split_once("://")
        .map_or(url, |(_, remainder)| remainder);
    let authority = without_scheme
        .split('/')
        .next()
        .filter(|authority| !authority.is_empty())
        .with_context(|| format!("daemon.url {url:?} has no host:port"))?;
    if authority.contains(':') {
        Ok(authority.to_string())
    } else {
        Ok(format!("{authority}:80"))
    }
}

#[cfg(test)]
mod tests {
    use super::bind_address;

    #[test]
    fn bind_address_strips_scheme_and_path() {
        assert_eq!(
            bind_address("http://127.0.0.1:7373").unwrap(),
            "127.0.0.1:7373"
        );
        assert_eq!(
            bind_address("http://127.0.0.1:7373/").unwrap(),
            "127.0.0.1:7373"
        );
        assert_eq!(bind_address("127.0.0.1:7373").unwrap(), "127.0.0.1:7373");
        assert_eq!(bind_address("http://localhost").unwrap(), "localhost:80");
    }
}
