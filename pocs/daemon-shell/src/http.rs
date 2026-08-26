//! The axum shell. Local-only, JSON in, JSON out, no auth — the same shape the
//! fs3 daemon has (PRD req 33) and the same reason: it fronts an index of every
//! repo on the machine, so it must never leave loopback.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::core::{Conflict, Dirty};
use crate::watcher::{Added, RootReport, SWEEP_INTERVAL, Supervisor};

/// `GET /health` — is the process serving?
#[derive(Debug, Serialize)]
pub struct Health {
    /// Always `"ok"`; a non-200 or a dead socket is the negative answer.
    pub status: &'static str,
    /// So a stale binary is visible to whoever is driving it.
    pub version: &'static str,
}

/// `GET /status` — everything a human needs to decide the daemon is sane.
#[derive(Debug, Serialize)]
pub struct Status {
    /// Milliseconds since the supervisor started.
    pub uptime_ms: u64,
    /// The configured quiet period.
    pub debounce_ms: u64,
    /// How often the sweep runs; the resolution of `settle_lag_ms`.
    pub sweep_interval_ms: u64,
    /// One entry per watched root, in path order.
    pub roots: Vec<RootReport>,
    /// Raw events across all roots.
    pub total_events: u64,
    /// Paths mid-debounce across all roots.
    pub total_pending: usize,
    /// Settled paths waiting to be collected.
    pub total_dirty: usize,
}

/// Body of `POST /watch` and `DELETE /watch`.
#[derive(Debug, Deserialize)]
pub struct WatchRequest {
    /// The directory to start or stop watching. Relative paths resolve against
    /// the DAEMON's working directory, not the caller's — a prototype-grade
    /// footgun that the real CLI must avoid by always sending absolute paths.
    pub path: PathBuf,
}

/// Success body of `POST /watch`.
#[derive(Debug, Serialize)]
pub struct Watching {
    /// The canonicalised root, which may differ from what was requested
    /// (`/tmp/x` → `/private/tmp/x` on macOS, `C:\r` → `\\?\C:\r` on Windows).
    pub root: PathBuf,
}

/// Success body of `DELETE /watch`.
#[derive(Debug, Serialize)]
pub struct Unwatched {
    /// The canonicalised root that stopped being watched.
    pub root: PathBuf,
    /// Whether it was watched at all; `false` comes back as 404.
    pub was_watching: bool,
}

/// `GET /dirty` — the debounced dirty set.
#[derive(Debug, Serialize)]
pub struct DirtyReport {
    /// Settled paths, in path order.
    pub dirty: Vec<Dirty>,
    /// How many entries are in `dirty`.
    pub count: usize,
    /// Paths still inside their debounce window — work that is coming.
    pub pending: usize,
}

/// Success body of `DELETE /dirty`.
#[derive(Debug, Serialize)]
pub struct Drained {
    /// How many entries the caller just took ownership of.
    pub drained: usize,
}

/// Anything that went wrong, as one shape.
#[derive(Debug, Serialize)]
pub struct Failure {
    /// Human-readable, including the causal chain from `anyhow`.
    pub error: String,
    /// Present when the refusal was an overlap with an existing root.
    ///
    /// Flattened, so the wire shape is one flat object —
    /// `{"error": "…", "conflict": "covered_by", "with": "/repo"}` — rather
    /// than a nested `conflict.conflict`.
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub conflict: Option<Conflict>,
}

impl Failure {
    fn message(error: impl std::fmt::Display) -> Self {
        Self {
            error: error.to_string(),
            conflict: None,
        }
    }
}

/// Build the router.
///
/// Separate from serving so a test can drive the real routes over a real
/// ephemeral port without owning the process lifetime.
pub fn router(supervisor: Arc<Supervisor>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/status", get(status))
        .route("/watch", post(watch))
        .route("/watch", delete(unwatch))
        .route("/dirty", get(dirty))
        .route("/dirty", delete(drain))
        .with_state(supervisor)
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn status(State(supervisor): State<Arc<Supervisor>>) -> Json<Status> {
    let roots = supervisor.report();
    Json(Status {
        uptime_ms: supervisor.now_ms(),
        debounce_ms: supervisor.debounce_ms(),
        sweep_interval_ms: u64::try_from(SWEEP_INTERVAL.as_millis()).unwrap_or(u64::MAX),
        total_events: roots.iter().map(|r| r.events).sum(),
        total_pending: roots.iter().map(|r| r.pending).sum(),
        total_dirty: roots.iter().map(|r| r.dirty).sum(),
        roots,
    })
}

async fn watch(
    State(supervisor): State<Arc<Supervisor>>,
    Json(request): Json<WatchRequest>,
) -> Result<(StatusCode, Json<Watching>), (StatusCode, Json<Failure>)> {
    match supervisor.watch(&request.path) {
        Ok(Added::Watching(root)) => Ok((StatusCode::CREATED, Json(Watching { root }))),
        // 409, not 400: the request is well-formed, the SET is what refuses it.
        Ok(Added::Rejected(conflict)) => Err((
            StatusCode::CONFLICT,
            Json(Failure {
                error: describe(&conflict),
                conflict: Some(conflict),
            }),
        )),
        // 400 rather than 500: a path that does not exist is the caller's
        // mistake, and `anyhow`'s chain names which layer refused.
        Err(error) => Err((
            StatusCode::BAD_REQUEST,
            Json(Failure::message(format!("{error:#}"))),
        )),
    }
}

async fn unwatch(
    State(supervisor): State<Arc<Supervisor>>,
    Json(request): Json<WatchRequest>,
) -> Result<Json<Unwatched>, (StatusCode, Json<Failure>)> {
    match supervisor.unwatch(&request.path) {
        Ok(Some(root)) => Ok(Json(Unwatched {
            root,
            was_watching: true,
        })),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(Failure::message(format!(
                "{} is not watched",
                request.path.display()
            ))),
        )),
        Err(error) => Err((
            StatusCode::BAD_REQUEST,
            Json(Failure::message(format!("{error:#}"))),
        )),
    }
}

/// Deliberately NON-destructive.
///
/// A `GET` that empties the thing it reports is a `GET` you cannot retry, and
/// every crashed consumer between the read and the acknowledgement loses work.
/// Reading and taking are split: `GET /dirty` is idempotent, `DELETE /dirty`
/// is the acknowledgement. That makes the delivery guarantee at-LEAST-once and
/// puts the choice in the consumer's hands, which is what the real daemon
/// wants for a queue that feeds an expensive re-scan.
async fn dirty(State(supervisor): State<Arc<Supervisor>>) -> Json<DirtyReport> {
    let entries = supervisor.dirty();
    let pending = supervisor.report().iter().map(|r| r.pending).sum();
    Json(DirtyReport {
        count: entries.len(),
        dirty: entries,
        pending,
    })
}

async fn drain(State(supervisor): State<Arc<Supervisor>>) -> Json<Drained> {
    Json(Drained {
        drained: supervisor.drain_dirty(),
    })
}

fn describe(conflict: &Conflict) -> String {
    match conflict {
        Conflict::Duplicate(root) => format!("{} is already watched", root.display()),
        Conflict::CoveredBy(root) => format!(
            "already covered by the watched root {} — overlapping recursive watches would \
             report every edit twice",
            root.display()
        ),
        Conflict::Covers(root) => format!(
            "would contain the watched root {} — remove that root first",
            root.display()
        ),
    }
}
