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
use axum::body::{Body, Bytes};
use axum::extract::{Query, State};
use axum::http::{Response, header};
use axum::middleware;
use axum::routing::{get, post};
use axum::{Json, Router};
use fs3_core::events::{EventKind, HEARTBEAT_MS, Hello};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::ReceiverStream;

use fs3_core::{Port, catalog};

use crate::answer::{Answer, IntoFailure, failed, ok};
use crate::auth::Auth;
use crate::conversations::{IntakeReport, IntakeRequest};
use crate::read::{GetRequest, TreeRequest};
use crate::roots::RootRequest;
use crate::search::{SearchOutcome, SearchRequest};
use crate::wiring::AppState;
use fs3_core::views::read::{GetPayload, TreeResult};
use fs3_core::views::remove::{GcCounts, RemoveReport};
use fs3_core::views::roots::RootReport;
use fs3_core::views::search::SearchResults;
use fs3_core::views::status::StatusReport;

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

/// What `GET /refs` asks for.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct RefsRequest {
    /// Repository-relative source path or fully qualified dd address whose
    /// incoming ddoc rows are wanted.
    pub path: String,
    /// Restrict to one repository identity, or `all`.
    #[serde(default)]
    pub repo: Option<String>,
    /// The caller's working directory, used for default repository scope.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Maximum rows to return. Defaults to the search surface's limit ceiling.
    #[serde(default)]
    pub limit: Option<i64>,
}

/// One exact inverse-index answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RefHit {
    /// The source row's positional dd address; paste directly into `ddocs get`.
    pub address: String,
    /// The repository-relative file this row references, for path lookups.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// dd relation spelling, verbatim.
    pub rel: String,
    /// JSONPath at which the relation was declared.
    pub location: String,
}

impl From<fs3_store::DdocFileRef> for RefHit {
    fn from(row: fs3_store::DdocFileRef) -> Self {
        RefHit {
            address: row.address,
            path: Some(row.path),
            rel: row.rel,
            location: row.location,
        }
    }
}

impl From<fs3_store::DdocCitation> for RefHit {
    fn from(row: fs3_store::DdocCitation) -> Self {
        RefHit {
            address: row.address,
            path: None,
            rel: row.rel,
            location: row.location,
        }
    }
}

/// Exact ddoc rows referencing one source path or citing one dd address.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RefsResult {
    pub results: Vec<RefHit>,
}

/// Build the router. Separate from [`serve`] so tests get the real routes
/// without owning a port or a runtime shutdown. Authentication is one outer
/// layer, so every current and future route inherits it automatically.
pub fn router(state: AppState, auth: Auth) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/roots", post(add_root).get(status))
        .route("/status", get(status))
        .route("/events", get(events))
        .route("/scan", post(scan))
        .route("/remove", post(remove))
        .route("/gc", post(gc))
        .route("/conversations", post(conversations).get(conversation_list))
        .route("/conversations/remove", post(conversation_remove))
        .route("/conversations/ingest", post(conversation_ingest))
        .route("/ask", post(ask))
        .route("/search", get(search))
        .route("/get", get(get_address))
        .route("/refs", get(refs))
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

    // Request filters are resolved before checking or invoking the chat port.
    // An unknown transcript is a bad request, not a model-generated empty
    // answer, and must cost zero model turns.
    let corpus = match crate::ask::resolve_corpus(&state, &request, &scope).await {
        Ok(corpus) => corpus,
        Err(failure) => {
            return failed(&state, COMMAND, failure)
                .await
                .0
                .with_meta(meta)
                .into();
        }
    };

    // The verdict rides the ENVELOPE, not the prose. A daemon wired to the
    // offline fake is healthy and cannot answer anything, and it used to say
    // so only in `grounded` and a next_action — while `ok` stayed true, which
    // is the field our own documentation tells consumers to branch on. So a
    // caller banked a placeholder as a finding. This is a failure, before any
    // model call is made and before anything is spent.
    let agent = state.agent_for(scope.repo.as_deref());
    if !agent.can_answer() {
        let failure = fs3_core::envelope::Failure::new(
            &fs3_core::catalog::PROVIDER_CANNOT_ANSWER,
            format!(
                "the agent port is wired to `{}`, which cannot answer questions",
                agent.key()
            ),
        );
        return failed(&state, COMMAND, failure).await;
    }

    match crate::ask::ask(&state, &request, scope.clone(), corpus).await {
        Ok(report)
            if report.stopped == "answered"
                && report
                    .answer
                    .as_deref()
                    .is_some_and(|answer| !answer.trim().is_empty()) =>
        {
            let next = if report.grounded && !report.citations.is_empty() {
                "verify any claim with `flowspace3 get <address>` on the citations, or ask a \
                 follow-up question"
            } else {
                "TREAT WITH SUSPICION — this answer cites no address that was read in full, \
                 so there is nothing to verify it against; re-ask more narrowly, or check \
                 `flowspace3 status` in case the index is empty"
            };
            ok(&state, COMMAND, report)
                .await
                .0
                .with_meta(meta)
                .with_next_action(crate::scope::steer(&scope, next))
                .into()
        }
        Ok(report) => {
            let (code, message, next) = match report.stopped.as_str() {
                "token_budget" => (
                    &fs3_core::catalog::QUERY_ASK_TOKEN_BUDGET,
                    "ask exhausted its token budget before synthesizing an answer".to_string(),
                    "ask a narrower question or raise `[agent] token_budget`; partial evidence is \
                     retained in error.details.evidence",
                ),
                "max_iterations" => (
                    &fs3_core::catalog::QUERY_ASK_ITERATION_LIMIT,
                    "ask reached its iteration limit before synthesizing an answer".to_string(),
                    "ask a narrower question or raise `[agent] max_iterations`; partial evidence \
                     is retained in error.details.evidence",
                ),
                "provider_failure" => (
                    &fs3_core::catalog::PROVIDER_FAILED,
                    report.failure.clone().unwrap_or_else(|| {
                        "the chat provider failed before synthesizing an answer".to_string()
                    }),
                    "retry after checking the active chat provider; partial evidence is retained \
                     in error.details.evidence",
                ),
                // Defence in depth for the fleet-observed impossible shape:
                // `answered` without answer text is a failure, never success-shaped null.
                _ => (
                    &fs3_core::catalog::PROVIDER_FAILED,
                    "ask stopped without producing answer text".to_string(),
                    "retry after checking the active chat provider; no empty answer is accepted",
                ),
            };
            let failure = fs3_core::envelope::Failure::new(code, message)
                .with_detail("stopped", &report.stopped)
                .with_detail("grounded", false)
                .with_detail("iterations", report.iterations)
                .with_detail("tokens_used", report.tokens_used)
                .with_detail("evidence", report.partial_evidence());
            failed(&state, COMMAND, failure)
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
) -> Answer<RemoveReport> {
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
            let next = crate::conversations::next_after_list(&list);
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

async fn conversation_ingest(
    State(state): State<AppState>,
    Json(request): Json<crate::convo_ingest::IngestRequest>,
) -> Answer<crate::convo_ingest::IngestAccepted> {
    const COMMAND: &str = "conversation ingest";
    if let Err(failure) = crate::schema::guard(&state.db).await {
        return failed(&state, COMMAND, failure).await;
    }
    // ENQUEUE ONLY. Ingest is fired from harness hooks, which run often and
    // must not wait on a store read: the route validates the address, upserts
    // one job, and returns. The runner does the reading.
    match crate::convo_ingest::submit(&state, &request).await {
        Ok(report) => {
            let next = crate::convo_ingest::next_after_submit(&report);
            ok(&state, COMMAND, report)
                .await
                .0
                .with_next_action(next)
                .into()
        }
        Err(failure) => failed(&state, COMMAND, failure).await,
    }
}

async fn gc(State(state): State<AppState>) -> Answer<GcCounts> {
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

#[derive(Debug, Default, Deserialize)]
struct EventQuery {
    heartbeat_ms: Option<u64>,
}

/// A live NDJSON feed. Each response owns its heartbeat and bounded output
/// queue; neither can block the daemon's shared event producer.
async fn events(State(state): State<AppState>, Query(query): Query<EventQuery>) -> Response<Body> {
    let heartbeat_ms = query.heartbeat_ms.unwrap_or(HEARTBEAT_MS).max(1);
    let subscription = state.subscribe();
    let (sender, receiver) = tokio::sync::mpsc::channel(AppState::event_capacity());

    let mut hello = Hello::new(env!("CARGO_PKG_VERSION"));
    hello.heartbeat_ms = heartbeat_ms;
    sender
        .try_send(Ok::<_, Infallible>(ndjson(&hello)))
        .expect("a new subscriber queue accepts its hello");

    tokio::spawn(stream_events(state, subscription, sender, heartbeat_ms));

    Response::builder()
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .body(Body::from_stream(ReceiverStream::new(receiver)))
        .expect("the event response has valid static headers")
}

fn ndjson(value: &impl Serialize) -> Bytes {
    let mut line = serde_json::to_vec(value).expect("frozen event types always serialize");
    line.push(b'\n');
    Bytes::from(line)
}

async fn stream_events(
    state: AppState,
    mut subscription: tokio::sync::broadcast::Receiver<fs3_core::Event>,
    sender: tokio::sync::mpsc::Sender<Result<Bytes, Infallible>>,
    heartbeat_ms: u64,
) {
    let period = Duration::from_millis(heartbeat_ms);
    let mut heartbeat = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut sequence = 0;

    loop {
        let line = tokio::select! {
            biased;
            received = subscription.recv() => match received {
                Ok(event) => {
                    heartbeat.reset();
                    ndjson(&event)
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed)
                | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => break,
            },
            _ = heartbeat.tick() => {
                sequence += 1;
                ndjson(&state.event(EventKind::Heartbeat { seq: sequence }))
            }
        };

        // Full means this connection is slower than the producer. Drop it;
        // an indexing task must never await a dashboard's socket.
        if sender.try_send(Ok::<_, Infallible>(line)).is_err() {
            break;
        }
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
            let next = if !report.inconsistencies.is_empty() {
                "element-tree inconsistencies were reported — follow each row's `next_action` before trusting affected reads"
            } else if report.queue.iter().any(|row| row.state == "pending") {
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

/// Advisory text for a result below the calibrated search confidence floor.
const WEAK_MATCH_HINT: &str =
    "Weak match: describe the component in its own vocabulary rather than asking a question.";

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
            let weak_match = outcome.is_weak_match();
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
            let ddoc_notice = ddoc_degradation_notice(&state, &scope).await;
            let next = next_after_search(&outcome);
            let next = match ddoc_notice {
                Some(notice) => format!("{notice} — {next}"),
                None => next,
            };
            let mut meta = serde_json::json!({
                "scope": scope,
                "empty_because": outcome.empty_because,
                "truncation": {
                    "limit": outcome.limit,
                    "truncated": outcome.truncated,
                },
            });
            if weak_match {
                meta["hint"] = serde_json::Value::String(WEAK_MATCH_HINT.to_string());
            }
            let results = SearchResults {
                results: outcome.results,
                composition: outcome.composition,
            };
            let next = crate::scope::steer(&scope, &next);
            let next = if crate::ask_hint::looks_like_question(&request.q) {
                format!("{next} — {}", crate::ask_hint::HINT)
            } else {
                next
            };
            let next = if weak_match {
                format!("{WEAK_MATCH_HINT} — then: {next}")
            } else {
                next.to_string()
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
fn next_after_search(outcome: &SearchOutcome) -> String {
    if let Some(reason) = &outcome.empty_because {
        return match &reason.hint {
            Some(hint) => format!("{} — {hint}", reason.detail),
            None => reason.detail.clone(),
        };
    }
    if outcome.results.is_empty() {
        return "nothing matched — widen with a shorter query, drop --min-score, check \
                `flowspace3 status` in case indexing has not finished, or run `flowspace3 \
                doctor`: a search only reads vectors written by the ACTIVE embedder, so a \
                provider change since indexing returns nothing from a full index"
            .to_string();
    }
    "read a hit in full with `flowspace3 get <address>`, browse its file with \
     `flowspace3 tree <address>`, or narrow with --path/--repo"
        .to_string()
}

/// Report unavailable ddocs tooling only when the request maps to one exact worktree.
///
/// Cwd-scoped searches carry that root in [`crate::scope::Scope::worktree`],
/// which maps deterministically to the live tooling snapshot. Explicit-repo and
/// all-repo scopes may cover several worktrees and deliberately stay silent: an
/// absent snapshot elsewhere is not evidence about the corpus that answered.
/// Store lookup failure also stays silent because tooling absence was not proven.
async fn ddoc_degradation_notice(
    state: &AppState,
    scope: &crate::scope::Scope,
) -> Option<&'static str> {
    let root = scope.worktree.as_deref()?;
    let worktree = fs3_store::find_worktree(&state.db, root).await.ok()??;
    state
        .ddoc_snapshot(worktree.id)
        .await?
        .is_absent()
        .then_some(
            "the `ddocs` binary is unavailable: rows are indexed and searchable, but link edges, \
             gate-terminal membership and derived state are unavailable until `ddocs` is on PATH",
        )
}

async fn refs(
    State(state): State<AppState>,
    Query(request): Query<RefsRequest>,
) -> Answer<RefsResult> {
    const COMMAND: &str = "refs";
    if let Err(failure) = crate::schema::guard(&state.db).await {
        return failed(&state, COMMAND, failure).await;
    }

    let target = request.path.trim();
    if target.is_empty() {
        return failed(
            &state,
            COMMAND,
            fs3_core::envelope::Failure::new(
                &catalog::QUERY_INVALID,
                "refs needs a repository-relative source path or fully qualified dd address",
            ),
        )
        .await;
    }
    let is_address = target.contains('#');
    if is_address {
        let qualified = matches!(
            fs3_core::DdocAddress::parse(target),
            Ok(address) if !address.file.is_empty()
        );
        if !qualified {
            let fix = "copy the fully qualified `<file>#<section>/<id>` address from a \
                       `flowspace3 search` or `flowspace3 get` result";
            return failed(
                &state,
                COMMAND,
                fs3_core::envelope::Failure::new(
                    &catalog::QUERY_INVALID,
                    "a bare or malformed dd address cannot identify the document being cited",
                )
                .with_fix(fix),
            )
            .await
            .0
            .with_next_action(fix)
            .into();
        }
    }
    let limit = request.limit.unwrap_or(crate::search::MAX_LIMIT);
    if !(1..=crate::search::MAX_LIMIT).contains(&limit) {
        return failed(
            &state,
            COMMAND,
            fs3_core::envelope::Failure::new(
                &catalog::QUERY_INVALID,
                format!(
                    "--limit must be between 1 and {}, got {limit}",
                    crate::search::MAX_LIMIT
                ),
            ),
        )
        .await;
    }

    let scope =
        crate::scope::resolve(&state, request.repo.as_deref(), request.cwd.as_deref()).await;
    let rows = if is_address {
        fs3_store::rows_citing(
            &state.db,
            scope.repo.as_deref(),
            target,
            crate::scan::PARSER_VERSION,
            limit,
        )
        .await
        .map(|rows| rows.into_iter().map(RefHit::from).collect())
    } else {
        fs3_store::rows_referencing(
            &state.db,
            scope.repo.as_deref(),
            target,
            crate::scan::PARSER_VERSION,
            limit,
        )
        .await
        .map(|rows| rows.into_iter().map(RefHit::from).collect())
    };
    match rows {
        Ok(results) => {
            let result = RefsResult { results };
            let next = if result.results.is_empty() {
                if is_address {
                    "no indexed ddoc rows cite that address — this is a successful empty answer"
                } else {
                    "no indexed ddoc rows reference that source path — this is a successful empty answer"
                }
            } else {
                "paste any address above into `ddocs get` or `flowspace3 get` to read the source row"
            };
            ok(&state, COMMAND, result)
                .await
                .0
                .with_meta(serde_json::json!({ "scope": scope }))
                .with_next_action(crate::scope::steer(&scope, next))
                .into()
        }
        Err(error) => failed(&state, COMMAND, error.into_failure()).await,
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

/// Serve on an already-bound listener until the shared shutdown state changes.
///
/// Boot publishes the daemon key immediately before calling this function, so
/// starting the accept loop here cannot expose a daemon with unpublished
/// credentials.
pub(crate) async fn serve_listener(
    state: AppState,
    listener: tokio::net::TcpListener,
    auth: Auth,
    mut shutdown: tokio::sync::watch::Receiver<crate::runner::Shutdown>,
) -> Result<()> {
    let bound = listener.local_addr().context("cannot read bound address")?;
    tracing::info!(%bound, "fs3 daemon listening");

    let mut forced = shutdown.clone();
    let server = std::future::IntoFuture::into_future(
        axum::serve(listener, router(state, auth)).with_graceful_shutdown(async move {
            while *shutdown.borrow() == crate::runner::Shutdown::Running
                && shutdown.changed().await.is_ok()
            {}
        }),
    );
    tokio::pin!(server);
    tokio::select! {
        result = &mut server => result.context("daemon stopped unexpectedly"),
        () = async move {
            while *forced.borrow() != crate::runner::Shutdown::Forced
                && forced.changed().await.is_ok()
            {}
        } => {
            tracing::warn!("forced shutdown abandoned active HTTP requests");
            Ok(())
        }
    }
}
