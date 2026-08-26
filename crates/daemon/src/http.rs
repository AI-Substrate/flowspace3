//! The daemon's HTTP surface. Localhost only (PRD req 33).

use anyhow::{Context, Result};
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use fs3_core::Port;

use crate::wiring::AppState;

/// What `GET /health` answers with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Health {
    /// `"ok"` when the daemon is serving.
    pub status: String,
    /// The daemon's version, so a stale binary is visible to the CLI.
    pub version: String,
    /// Which embedder arm the composition root selected.
    pub embedder: String,
    /// Which summarizer arm the composition root selected.
    pub summarizer: String,
}

impl Health {
    /// The single spelling of "healthy", shared by daemon and CLI.
    pub const OK: &'static str = "ok";
}

/// Build the router. Separate from [`serve`] so tests get the real routes
/// without owning a port or a runtime shutdown.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    Json(Health {
        status: Health::OK.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        embedder: state.active_kind(Port::Embedder).to_string(),
        summarizer: state.active_kind(Port::Summarizer).to_string(),
    })
}

/// Serve until the process is asked to stop.
///
/// # Errors
/// When the address cannot be bound or the server fails.
pub async fn serve(state: AppState, address: &str) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("cannot bind {address}"))?;
    let bound = listener.local_addr().context("cannot read bound address")?;
    tracing::info!(%bound, "fs3 daemon listening");

    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("daemon stopped unexpectedly")
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "cannot listen for shutdown signal");
    }
}
