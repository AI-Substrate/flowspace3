//! The daemon's HTTP surface. Localhost only (PRD req 33).
//!
//! Every endpoint answers a workshop-004 envelope, and none of them chooses an
//! HTTP status: [`crate::answer::Answer`] derives it from the error's own code
//! (D4). `/health` is the one exception, and deliberately so — it is the probe a
//! CLI uses to decide whether the daemon exists at all, so it must answer
//! before, and independently of, everything that can be wrong behind it.
//!
//! # The schema gate
//!
//! Every endpoint that touches the database runs [`crate::schema::guard`] first.
//! It is one indexed read, and it converts the failure mode "a column is
//! missing, here is a Postgres error naming it" into "your schema is two
//! migrations behind, run `flowspace3 doctor`". The daemon still migrates at
//! boot; the guard is for the case boot cannot cover — a database that moved
//! underneath a running daemon.

use anyhow::{Context, Result};
use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use fs3_core::Port;

use crate::answer::{Answer, IntoFailure, failed, ok};
use crate::roots::{RootReport, RootRequest};
use crate::search::{SearchRequest, SearchResults};
use crate::status::StatusReport;
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
        .route("/roots", post(add_root).get(status))
        .route("/status", get(status))
        .route("/scan", post(scan))
        .route("/search", get(search))
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

async fn add_root(
    State(state): State<AppState>,
    Json(request): Json<RootRequest>,
) -> Answer<RootReport> {
    const COMMAND: &str = "add";
    if let Err(failure) = crate::schema::guard(&state.db).await {
        return failed(&state, COMMAND, failure).await;
    }
    match crate::roots::add_root(&state, std::path::Path::new(&request.path)).await {
        Ok(report) => {
            let next = next_after_scan(&report);
            ok(&state, COMMAND, report)
                .await
                .0
                .with_next_action(next)
                .into()
        }
        Err(error) => failed(&state, COMMAND, error.into_failure()).await,
    }
}

async fn scan(
    State(state): State<AppState>,
    Json(request): Json<RootRequest>,
) -> Answer<RootReport> {
    const COMMAND: &str = "scan";
    if let Err(failure) = crate::schema::guard(&state.db).await {
        return failed(&state, COMMAND, failure).await;
    }
    match crate::roots::rescan_root(&state, std::path::Path::new(&request.path)).await {
        Ok(report) => {
            let next = next_after_scan(&report);
            ok(&state, COMMAND, report)
                .await
                .0
                .with_next_action(next)
                .into()
        }
        Err(error) => failed(&state, COMMAND, error.into_failure()).await,
    }
}

async fn status(State(state): State<AppState>) -> Answer<StatusReport> {
    const COMMAND: &str = "status";
    if let Err(failure) = crate::schema::guard(&state.db).await {
        return failed(&state, COMMAND, failure).await;
    }
    match crate::status::report(&state).await {
        Ok(report) => {
            let next = if report.queue.iter().any(|row| row.state == "pending") {
                "work is still queued — re-run `flowspace3 status` until it is empty, then search"
            } else {
                "the queue is empty — `flowspace3 search \"<question>\"` will answer from the index"
            };
            ok(&state, COMMAND, report)
                .await
                .0
                .with_next_action(next)
                .into()
        }
        Err(failure) => failed(&state, COMMAND, failure).await,
    }
}

async fn search(
    State(state): State<AppState>,
    Query(request): Query<SearchRequest>,
) -> Answer<SearchResults> {
    const COMMAND: &str = "search";
    if let Err(failure) = crate::schema::guard(&state.db).await {
        return failed(&state, COMMAND, failure).await;
    }
    match crate::search::search(&state, &request).await {
        Ok(results) => {
            let next = if results.results.is_empty() {
                // The third cause is the one nobody guesses: vectors are only
                // read under the model_key that wrote them, so searching with
                // a different embedder than the one that indexed returns
                // nothing while the index looks full. Naming doctor here is
                // what turns that from a mystery into one command.
                "nothing matched — widen with a shorter query, drop --min-score, check \
                 `flowspace3 status` in case indexing has not finished, or run `flowspace3 \
                 doctor`: a search only reads vectors written by the ACTIVE embedder, so a \
                 provider change since indexing returns nothing from a full index"
            } else {
                "open a hit at its path and span, or narrow with --path/--repo"
            };
            ok(&state, COMMAND, results)
                .await
                .0
                .with_next_action(next)
                .into()
        }
        Err(failure) => failed(&state, COMMAND, failure).await,
    }
}

/// What a caller typically does after a scan, given what the scan found.
fn next_after_scan(report: &RootReport) -> String {
    if report.enqueued == 0 {
        format!(
            "nothing changed — {} files already indexed; `flowspace3 search \"<question>\"` \
             answers from the existing index",
            report.unchanged
        )
    } else {
        format!(
            "{} scan jobs queued — poll `flowspace3 status` until the queue is empty, then search",
            report.enqueued
        )
    }
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
