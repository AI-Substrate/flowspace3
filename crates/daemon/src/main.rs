//! `fs3-daemon` — the fs3 background service.
//!
//! Reads configuration, wires the composition root, serves HTTP on localhost.

use anyhow::{Context, Result, ensure};
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

/// Turn the configured daemon URL into a bind address, refusing any host that
/// is not loopback.
///
/// PRD req 17 / AC-0005: fs3's HTTP surface is local-only. It is
/// unauthenticated and it fronts an index of every repo on the machine, so
/// binding `0.0.0.0` would publish that to the network. A config typo has to be
/// a startup failure, not a silent exposure.
fn bind_address(url: &str) -> Result<String> {
    let without_scheme = url
        .split_once("://")
        .map_or(url, |(_, remainder)| remainder);
    let authority = without_scheme
        .split('/')
        .next()
        .filter(|authority| !authority.is_empty())
        .with_context(|| format!("daemon.url {url:?} has no host:port"))?;

    let (host, port) = split_authority(authority);
    ensure!(
        is_loopback(host),
        "daemon.url {url:?} binds {host:?}, which is not loopback. fs3's HTTP \
         surface is local-only and unauthenticated (PRD req 17) — use \
         127.0.0.1, ::1, or localhost."
    );

    Ok(if port.is_some() {
        authority.to_string()
    } else {
        format!("{authority}:80")
    })
}

/// Split an authority into host and optional port, understanding the bracketed
/// IPv6 form. Splitting `[::1]:7373` on `:` would tear the address apart and
/// leave `[` looking like a hostname.
fn split_authority(authority: &str) -> (&str, Option<&str>) {
    if let Some(rest) = authority.strip_prefix('[') {
        return match rest.split_once(']') {
            Some((host, tail)) => (host, tail.strip_prefix(':')),
            None => (authority, None),
        };
    }
    match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    }
}

/// A loopback address, or the one name that always resolves to one.
///
/// Anything else is refused rather than resolved: a name that happens to point
/// at a loopback address today is not a local-only guarantee.
fn is_loopback(host: &str) -> bool {
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return ip.is_loopback();
    }
    host.eq_ignore_ascii_case("localhost")
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

    /// The finding this kills: `http://0.0.0.0:7373` used to be accepted, and
    /// the daemon then served every interface.
    #[test]
    fn bind_address_refuses_every_non_loopback_host() {
        for url in [
            "http://0.0.0.0:7373",
            "0.0.0.0:7373",
            "http://[::]:7373",
            "http://192.168.1.10:7373",
            "http://example.com:7373",
            "http://0.0.0.0",
        ] {
            let error = bind_address(url)
                .expect_err("a non-loopback bind publishes the local index to the network");
            assert!(
                error.to_string().contains("not loopback"),
                "the refusal must say why, got: {error}"
            );
        }
    }

    #[test]
    fn bind_address_accepts_every_loopback_spelling() {
        assert_eq!(bind_address("http://[::1]:7373").unwrap(), "[::1]:7373");
        assert_eq!(
            bind_address("http://127.0.0.2:7373").unwrap(),
            "127.0.0.2:7373"
        );
        assert_eq!(
            bind_address("http://LocalHost:7373").unwrap(),
            "LocalHost:7373"
        );
    }
}
