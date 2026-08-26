//! daemon-shell — a prototype of the host-native half of the fs3 daemon.
//!
//! Three layers, deliberately separable:
//!
//! * [`core`] — pure debounce and dirty-set logic. No clock, no I/O, no
//!   threads. This is the part worth lifting into the real daemon verbatim.
//! * [`watcher`] — the `notify` shell: OS watchers, one monotonic clock, the
//!   channel out of the watcher thread, and the sweep.
//! * [`http`] — the axum shell: loopback-only JSON over TCP.
//!
//! Read `LEARNINGS.md` before copying any of it.

pub mod core;
pub mod http;
pub mod watcher;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};

pub use watcher::Supervisor;

/// Start the watcher tasks and serve HTTP until ctrl-c.
///
/// Returns the bound address through `on_bind` before serving, so a caller
/// that asked for port 0 can discover which port it actually got.
///
/// # Errors
/// When the address cannot be bound or the server fails.
pub async fn serve(
    address: SocketAddr,
    debounce: Duration,
    on_bind: impl FnOnce(SocketAddr, Arc<Supervisor>),
) -> Result<()> {
    let (supervisor, events) = Supervisor::new(debounce);

    tokio::spawn(watcher::pump(Arc::clone(&supervisor), events));
    tokio::spawn(watcher::sweeper(Arc::clone(&supervisor)));

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("cannot bind {address}"))?;
    let bound = listener.local_addr().context("cannot read bound address")?;
    on_bind(bound, Arc::clone(&supervisor));

    tracing::info!(
        %bound,
        debounce_ms = supervisor.debounce_ms(),
        "daemon-shell listening"
    );

    axum::serve(listener, http::router(supervisor))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server stopped unexpectedly")
}

/// The only signal handling in the prototype, and the only one that is
/// portable: `tokio::signal::ctrl_c` maps to `SIGINT` on unix and to the
/// console control handler on Windows. `SIGTERM`/`SIGHUP` handling would be
/// unix-only and is deliberately absent — see LEARNINGS.
async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "cannot listen for shutdown signal");
    }
    tracing::info!("shutting down");
}
