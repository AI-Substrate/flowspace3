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
use axum::middleware;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use fs3_core::Port;

use crate::answer::{Answer, IntoFailure, failed, ok};
use crate::auth::Auth;
use crate::conversations::{IntakeReport, IntakeRequest};
use crate::read::{GetPayload, GetRequest, TreeRequest, TreeResult};
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
/// without owning a port or a runtime shutdown. Authentication is one outer
/// layer, so every current and future route inherits it automatically.
pub fn router(state: AppState, auth: Auth) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/roots", post(add_root).get(status))
        .route("/status", get(status))
        .route("/scan", post(scan))
        .route("/remove", post(remove))
        .route("/gc", post(gc))
        .route("/conversations", post(conversations).get(conversation_list))
        .route("/conversations/remove", post(conversation_remove))
        .route("/ask", post(ask))
        .route("/search", get(search))
        .route("/get", get(get_address))
        .route("/tree", get(tree))
        .with_state(state)
        .layer(middleware::from_fn_with_state(auth, crate::auth::require))
}

/// `POST /ask` — answer a question by running a bounded, grounded tool loop.
///
/// Synchronous today: one request runs the whole loop and returns when it is
/// done, which can be tens of seconds. The async-job posture with a streamed
/// progress feed is the named follow-up, deferred until the event wire lands —
/// the tool-call trace in the report is what that feed will carry.
async fn ask(
    State(state): State<AppState>,
    Json(request): Json<crate::ask::AskRequest>,
) -> Answer<crate::ask::AskReport> {
    const COMMAND: &str = "ask";
    if let Err(failure) = crate::schema::guard(&state.db).await {
        return failed(&state, COMMAND, failure).await;
    }

    // Scope rides in `meta` exactly as it does for search, and for a sharper
    // reason: an answer drawn from one repository when the caller expected all
    // of them is indistinguishable from a wrong answer unless the scope is on
    // the envelope.
    let scope =
        crate::scope::resolve(&state, request.repo.as_deref(), request.cwd.as_deref()).await;
    let meta = serde_json::json!({ "scope": scope });

    match crate::ask::ask(&state, &request, scope.clone()).await {
        Ok(report) => {
            let next = match (&report.answer, report.citations.is_empty()) {
                (None, _) => {
                    "the loop hit a bound before answering — raise [agent] max_iterations or \
                     token_budget, or ask a narrower question"
                }
                (Some(_), true) => {
                    "answered without reading any address — treat that answer with suspicion and \
                     check `flowspace3 status`, because a grounded answer cites what it read"
                }
                (Some(_), false) => {
                    "verify any claim with `flowspace3 get <address>` on the citations, or ask a \
                     follow-up question"
                }
            };
            ok(&state, COMMAND, report)
                .await
                .0
                .with_meta(meta)
                .with_next_action(crate::scope::steer(&scope, next))
                .into()
        }
        Err(error) => {
            let failure = crate::answer::IntoFailure::into_failure(error);
            failed(&state, COMMAND, failure).await
        }
    }
}

async fn remove(
    State(state): State<AppState>,
    Json(request): Json<RootRequest>,
) -> Answer<crate::remove::RemoveReport> {
    const COMMAND: &str = "remove";
    if let Err(failure) = crate::schema::guard(&state.db).await {
        return failed(&state, COMMAND, failure).await;
    }
    if !std::path::Path::new(&request.path).is_absolute() {
        return failed(
            &state,
            COMMAND,
            crate::remove::must_be_absolute(&request.path),
        )
        .await;
    }
    match crate::remove::remove(&state, &request.path).await {
        Ok(report) => {
            let next = crate::remove::next_after_remove(&report);
            ok(&state, COMMAND, report)
                .await
                .0
                .with_next_action(next)
                .into()
        }
        Err(failure) => failed(&state, COMMAND, failure).await,
    }
}

async fn conversation_list(
    State(state): State<AppState>,
    Query(request): Query<crate::conversations::ListRequest>,
) -> Answer<crate::conversations::ConversationList> {
    const COMMAND: &str = "conversation list";
    if let Err(failure) = crate::schema::guard(&state.db).await {
        return failed(&state, COMMAND, failure).await;
    }
    match crate::conversations::list(&state, &request).await {
        Ok(list) => {
            let next = if list.conversations.is_empty() {
                "nothing is indexed yet — `flowspace3 conversation import <file>` stores a \
                 transcript"
                    .to_string()
            } else {
                "`flowspace3 tree <address>` outlines any of them; \
                 `flowspace3 search \"<question>\" --source conversation` searches their turns"
                    .to_string()
            };
            ok(&state, COMMAND, list)
                .await
                .0
                .with_next_action(next)
                .into()
        }
        Err(failure) => failed(&state, COMMAND, failure).await,
    }
}

async fn conversation_remove(
    State(state): State<AppState>,
    Json(request): Json<crate::conversations::RemoveRequest>,
) -> Answer<crate::conversations::RemoveReport> {
    const COMMAND: &str = "conversation remove";
    if let Err(failure) = crate::schema::guard(&state.db).await {
        return failed(&state, COMMAND, failure).await;
    }
    match crate::conversations::remove(&state, &request).await {
        Ok(report) => {
            let next = crate::conversations::next_after_remove(&report);
            ok(&state, COMMAND, report)
                .await
                .0
                .with_next_action(next)
                .into()
        }
        Err(failure) => failed(&state, COMMAND, failure).await,
    }
}

async fn conversations(
    State(state): State<AppState>,
    Json(request): Json<IntakeRequest>,
) -> Answer<IntakeReport> {
    const COMMAND: &str = "conversation import";
    if let Err(failure) = crate::schema::guard(&state.db).await {
        return failed(&state, COMMAND, failure).await;
    }
    match crate::conversations::intake(&state, request).await {
        Ok(report) => {
            let next = crate::conversations::next_after_intake(&report);
            ok(&state, COMMAND, report)
                .await
                .0
                .with_next_action(next)
                .into()
        }
        Err(failure) => failed(&state, COMMAND, failure).await,
    }
}

async fn gc(State(state): State<AppState>) -> Answer<crate::remove::GcCounts> {
    const COMMAND: &str = "gc";
    if let Err(failure) = crate::schema::guard(&state.db).await {
        return failed(&state, COMMAND, failure).await;
    }
    match crate::remove::collect(&state).await {
        Ok(counts) => {
            let next = crate::remove::next_after_gc(&counts);
            ok(&state, COMMAND, counts)
                .await
                .0
                .with_next_action(next)
                .into()
        }
        Err(failure) => failed(&state, COMMAND, failure).await,
    }
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

    // Workshop 003 D6: a bare search is about the repository the caller is
    // standing in. The scope rides in `meta` on BOTH outcomes, because "no
    // index for the active model" and "the wrong repository answered" look
    // identical to a caller who cannot see which repository was asked.
    let scope =
        crate::scope::resolve(&state, request.repo.as_deref(), request.cwd.as_deref()).await;

    match crate::search::search(&state, &request, &scope).await {
        Ok(outcome) => {
            // The third cause is the one nobody guesses: vectors are only read
            // under the model_key that wrote them, so searching with a
            // different embedder than the one that indexed returns nothing
            // while the index looks full. Naming doctor here is what turns
            // that from a mystery into one command.
            //
            // When `empty_because` is present the surface knows more than
            // that, and the steer says the known thing instead of listing
            // suspects: guessing out loud next to a fact we hold is how a user
            // ends up rephrasing a query that was never the problem.
            let next = match (&outcome.empty_because, outcome.results.is_empty()) {
                (Some(reason), _) => reason.detail.as_str(),
                (None, true) => {
                    "nothing matched — widen with a shorter query, drop --min-score, check \
                     `flowspace3 status` in case indexing has not finished, or run `flowspace3 \
                     doctor`: a search only reads vectors written by the ACTIVE embedder, so a \
                     provider change since indexing returns nothing from a full index"
                }
                (None, false) => {
                    "read a hit in full with `flowspace3 get <address>`, browse its file with \
                     `flowspace3 tree <address>`, or narrow with --path/--repo"
                }
            };
            let meta = serde_json::json!({
                "scope": scope,
                "empty_because": outcome.empty_because,
            });
            let results = SearchResults {
                results: outcome.results,
            };
            let next = crate::scope::steer(&scope, next);
            let next = if crate::ask_hint::looks_like_question(&request.q) {
                format!("{next} — {}", crate::ask_hint::HINT)
            } else {
                next
            };
            ok(&state, COMMAND, results)
                .await
                .0
                .with_meta(meta)
                .with_next_action(next)
                .into()
        }
        Err(failure) => failed::<SearchResults>(&state, COMMAND, failure)
            .await
            .0
            .with_meta(serde_json::json!({ "scope": scope }))
            .into(),
    }
}

async fn get_address(
    State(state): State<AppState>,
    Query(request): Query<GetRequest>,
) -> Answer<GetPayload> {
    const COMMAND: &str = "get";
    if let Err(failure) = crate::schema::guard(&state.db).await {
        return failed(&state, COMMAND, failure).await;
    }

    let scope =
        crate::scope::resolve(&state, request.repo.as_deref(), request.cwd.as_deref()).await;
    match crate::read::get(&state, &request, &scope).await {
        Ok((result, parser_version)) => {
            let next = crate::read::next_after_get(&result);
            ok(&state, COMMAND, result)
                .await
                .0
                .with_meta(serde_json::json!({
                    "scope": scope,
                    // Which parse answered. A bumped parser leaves the previous
                    // version's rows in place until a re-scan, so this is the
                    // difference between "current" and "what is still stored".
                    "parser_version": parser_version,
                    "parser_version_current": parser_version == crate::scan::PARSER_VERSION,
                }))
                // Through `steer`, like search and tree: a scope warning is
                // invisible in `data`, so a consumer reading only `data` and
                // `next_action` would never learn that the address it just
                // read was resolved in a repository it is not standing in.
                .with_next_action(crate::scope::steer(&scope, &next))
                .into()
        }
        Err(failure) => failed::<GetPayload>(&state, COMMAND, failure)
            .await
            .0
            .with_meta(serde_json::json!({ "scope": scope }))
            .into(),
    }
}

async fn tree(
    State(state): State<AppState>,
    Query(request): Query<TreeRequest>,
) -> Answer<TreeResult> {
    const COMMAND: &str = "tree";
    if let Err(failure) = crate::schema::guard(&state.db).await {
        return failed(&state, COMMAND, failure).await;
    }

    let scope =
        crate::scope::resolve(&state, request.repo.as_deref(), request.cwd.as_deref()).await;
    let meta = serde_json::json!({ "scope": scope });

    match crate::read::tree(&state, &request, &scope).await {
        Ok(result) => {
            let next = crate::read::next_after_tree(&result);
            ok(&state, COMMAND, result)
                .await
                .0
                .with_meta(meta)
                .with_next_action(crate::scope::steer(&scope, &next))
                .into()
        }
        Err(failure) => failed::<TreeResult>(&state, COMMAND, failure)
            .await
            .0
            .with_meta(meta)
            .into(),
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
pub async fn serve(state: AppState, address: &str, auth: Auth) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("cannot bind {address}"))?;
    serve_listener(state, listener, auth).await
}

/// Serve on a listener whose port is already reserved.
///
/// Sandbox boot uses this to publish the exact ephemeral port without a
/// release-and-rebind race.
pub(crate) async fn serve_listener(
    state: AppState,
    listener: tokio::net::TcpListener,
    auth: Auth,
) -> Result<()> {
    let bound = listener.local_addr().context("cannot read bound address")?;
    tracing::info!(%bound, "fs3 daemon listening");

    axum::serve(listener, router(state, auth))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("daemon stopped unexpectedly")
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "cannot listen for shutdown signal");
    }
}
