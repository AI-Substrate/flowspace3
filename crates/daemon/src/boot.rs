//! Booting the daemon: config, the composition root, the runner, HTTP.
//!
//! This used to be `fs3-daemon`'s `main`. The daemon now ships INSIDE the
//! `flowspace3` binary as `flowspace3 daemon` (PRD req 51, Jordan 2026-08-26):
//! one file to install, one version, and no way for a CLI and a daemon of
//! different vintages to meet. The crate is unchanged in every other respect —
//! it is still the composition root, and still the only crate that sees every
//! other one.
//!
//! What did NOT move here is the secrets chain. Putting `secrets.env` into the
//! process environment is only sound while the process is single-threaded, so
//! it has to happen before a runtime exists — and the CLI already does exactly
//! that, first thing, for every verb. Doing it again here would be a second
//! implementation of a rule that is easy to get subtly wrong, so [`run`]
//! assumes the environment is already loaded and says so.

use anyhow::{Context, Result, ensure};
use fs3_core::{Config, Port, redact_url_password};

use crate::wiring::AppState;
use crate::{config, http};

/// Run the daemon until it is asked to stop.
///
/// Must be called from OUTSIDE a Tokio runtime: it builds its own, because the
/// caller's `main` is deliberately not `#[tokio::main]`.
///
/// # Errors
/// A configuration that cannot be read, a `daemon.url` that is not loopback, a
/// store that cannot be migrated, or an address that cannot be bound — all
/// startup failures on purpose (PRD req 37).
pub fn run() -> Result<()> {
    let directory = config::config_dir().context("locating the fs3 config directory")?;

    let configuration = config::load_effective_from(&directory)
        .with_context(|| format!("loading configuration from {}", directory.display()))?;

    let address = bind_address(&configuration.config.daemon.url)?;
    tracing::info!(
        config = %directory.display(),
        daemon = %configuration.layer("daemon"),
        database = %configuration.layer("database"),
        embedder = %configuration.config.selected(Port::Embedder, None),
        summarizer = %configuration.config.selected(Port::Summarizer, None),
        repos = configuration.config.repos.len(),
        "fs3 daemon starting"
    );

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the Tokio runtime")?
        .block_on(serve(configuration.config, address))
}

async fn serve(configuration: Config, address: String) -> Result<()> {
    let state = AppState::from_config(configuration).context("wiring the composition root")?;
    let database = redact_url_password(&state.config.database.url);

    // The daemon is the single writer, so startup is the only migration point.
    // It is also the only moment where refusing to run is cheaper than running:
    // a writer that cannot reach its own schema has nothing useful to serve, so
    // this fails loud rather than starting into a guaranteed error per request.
    fs3_store::migrate(&state.db).await.with_context(|| {
        format!(
            "applying store migrations to {database} — if the store is not running: {}",
            fs3_store::COMPOSE_UP
        )
    })?;
    tracing::info!(%database, "store schema is current");

    // Recover anything a previous process died holding, BEFORE the runner can
    // claim. A row left `running` has no lease and no heartbeat, so nothing
    // else would ever move it — and because `scan_file` dedupes on
    // (worktree, path), it would silently absorb every future add or scan of
    // that file. One SIGKILL during a large index would otherwise make those
    // files permanently unindexable, reported as success.
    //
    // Sound only here: fs3 is the single writer (PRD req 20), so at this
    // instant no worker exists to be holding a claim.
    match fs3_store::requeue_running(&state.db).await {
        Ok(0) => {}
        Ok(swept) => tracing::warn!(
            swept,
            "requeued jobs left running by a previous process — it did not shut down cleanly"
        ),
        Err(error) => tracing::error!(%error, "cannot requeue jobs left running"),
    }

    // The worker loop is a background task rather than a second process: it
    // shares the composition root's provider Arcs (and therefore their HTTP
    // clients and Entra token cache), and the queue's own SKIP LOCKED claim is
    // what makes concurrency safe, so nothing is gained by isolating it.
    //
    // It is spawned BEFORE the server starts listening, so a root added by the
    // very first request is already being drained by the time the response is
    // written.
    let workers = state.config.indexing.worker_concurrency;
    tracing::info!(workers, "starting the job runner");
    tokio::spawn(crate::runner::run_forever(state.clone(), workers));

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
