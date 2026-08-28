//! Ingesting a native agent conversation into the index (plan 005, tk-c302).
//!
//! The composition root for `ConversationSource`. Everything interesting was
//! decided in a unit: the readers turn a store's bytes into semantic records,
//! `fs3_store::ingest_cursors` remembers where each session stopped and which
//! ordinals are already stored, and `fs3_core::prepare_batch` numbers and
//! shapes what is new. This module is the plumbing between them, and it is
//! deliberately the only place that knows all four exist.
//!
//! # The pipeline, per session file
//!
//! Lookup the conversation, upsert its header, load the cursor, read (blocking,
//! off the async thread), ask the ledger about exactly the ordinals just read,
//! decide purely, append idempotently, then record the poll — ledger rows and
//! cursor in ONE transaction, even when nothing was appended, because the
//! reader still moved over bytes.
//!
//! # Two rules that are not obvious
//!
//! **Resolution is a LOOKUP, never a mint that guesses.** A session's
//! conversation id is derived deterministically from `(harness, session_id)`,
//! so the two addressing routes — by pij seat and by native session id — land
//! the SAME conversation (plan ac-0002), and a lost cursor row cannot cause a
//! second copy of a conversation to be created. `ingest_cursors::commit_poll`
//! refuses to rebind a session that already points somewhere else, which is the
//! backstop if this derivation is ever changed.
//!
//! **Serialise per CONVERSATION, not per session.** Turn numbers come from the
//! conversation's own stored turns, so two concurrent polls of two DIFFERENT
//! sessions of one conversation would read the same high-water mark and
//! collide. One claude conversation is a main file plus N sidecars, so that is
//! the normal case here and not an exotic one.

use std::path::{Path, PathBuf};

use fs3_core::conversation_source::{ConversationSource, Harness, IngestInput, SessionKind};
use fs3_core::envelope::Failure;
use fs3_core::{
    Conversation, ConversationId, Element, catalog, content_hash, earns_summary, prepare_batch,
};
use fs3_providers::conversation_sources::{
    claude::ClaudeSource, metrics_db::MetricsDbSource, metrics_db::RepoScope, omp::OmpSource,
    pij_ledger::PijLedgerSource,
};
use serde::{Deserialize, Serialize};

use crate::conversations::UNANCHORED;
use crate::enrich;
use crate::runner::fail;
use crate::wiring::AppState;

/// What an ingest was asked for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IngestRequest {
    /// The pij seat, when addressing by seat.
    #[serde(default)]
    pub pij_id: Option<String>,
    /// The harness's own session id, when addressing directly.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Which store, when addressing directly.
    #[serde(default)]
    pub harness: Option<String>,
    /// The workspace the conversation happened in. Defaults, for the seat
    /// route, to the git directory pij recorded for that seat.
    #[serde(default)]
    pub folder: Option<String>,
}

/// What one session file contributed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionIngest {
    /// The conversation it landed in.
    pub guid: String,
    /// The store's session id — a sidecar has its own.
    pub session_id: String,
    /// Whether this is the conversation itself or a child of it.
    pub kind: String,
    /// The parent's session id, for a child conversation.
    pub parent_session_id: Option<String>,
    /// Records the reader produced this poll.
    pub records_read: usize,
    /// Turns newly stored.
    pub turns_new: usize,
    /// Records the ledger recognised and suppressed.
    pub deduped: usize,
    /// Turns queued for enrichment from this file.
    pub summarized: usize,
    /// Whether the reader restarted from the beginning.
    pub rescanned: bool,
}

/// What an ingest did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct IngestReport {
    /// Which store was read.
    pub harness: String,
    /// The workspace the session was actually found under, which is not always
    /// the one the caller named — see `discover_folder`.
    pub folder: String,
    /// One row per session file, main first.
    pub sessions: Vec<SessionIngest>,
    /// Records read across every file.
    pub records_read: usize,
    /// Turns newly stored.
    pub turns_new: usize,
    /// Records suppressed because the ledger already had them. NEVER omit this
    /// from an operator-facing envelope: "read 412, appended 0, deduped 412" is
    /// the only line that distinguishes a handled rotation from an idle poll.
    pub deduped: usize,
    /// Turns queued for enrichment.
    pub summarized: usize,
    /// Session files another poll of the same conversation was already
    /// holding. Nonzero means this run did NOT read them and re-queued itself.
    pub contended: usize,
}

/// The job kind that does the reading.
///
/// Ingest is fired from HARNESS HOOKS, which run often and must not wait: the
/// route ENQUEUES and returns, and the daemon's own runner does the work. The
/// queue's dedupe key is the address, and `enqueue_job` upserts among live
/// jobs, so a burst of hook firings collapses into ONE pending job rather than
/// one job per firing — the same mechanism the watcher's debounce relies on.
pub const INGEST_SESSION: &str = "ingest_session";

/// What the route returns, immediately.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IngestAccepted {
    /// The address the job will resolve.
    pub address: String,
    /// The queue key it collapses on.
    pub dedupe_key: String,
    /// Always true — a refusal is a failure envelope, not this.
    pub accepted: bool,
}

/// Enqueue an ingest and return without touching a session store.
///
/// # Errors
/// [`catalog::QUERY_INVALID`] for an address that is not one of the two
/// shapes; store failures mapped by their own codes.
pub async fn submit(state: &AppState, request: &IngestRequest) -> Result<IngestAccepted, Failure> {
    submit_after(state, request, std::time::Duration::ZERO).await
}

/// Enqueue an ingest to run no sooner than `delay`.
///
/// # Errors
/// [`catalog::QUERY_INVALID`] for an address that is not one of the two
/// shapes; store failures mapped by their own codes.
pub async fn submit_after(
    state: &AppState,
    request: &IngestRequest,
    delay: std::time::Duration,
) -> Result<IngestAccepted, Failure> {
    // Validated HERE rather than in the worker, so a mistyped address is a
    // synchronous error the caller can see rather than a job that fails later
    // where a hook will never look.
    let address = match (&request.pij_id, &request.session_id, &request.harness) {
        (Some(seat), None, None) => format!("pij/{seat}"),
        (None, Some(session), Some(harness)) => {
            let _: Harness = harness.parse().map_err(|error: fs3_core::Error| {
                Failure::new(&catalog::QUERY_INVALID, error.to_string()).retryable(false)
            })?;
            format!("{harness}/{session}")
        }
        _ => {
            return Err(Failure::new(
                &catalog::QUERY_INVALID,
                "address a conversation either by seat (--pij) or by session (--session with \
                 --harness), never both and never neither"
                    .to_string(),
            )
            .retryable(false));
        }
    };

    let folder = request.folder.clone().unwrap_or_default();
    let dedupe_key = format!("ingest:{address}@{folder}");
    let payload = serde_json::to_value(request).map_err(|error| {
        Failure::new(
            &catalog::QUERY_INVALID,
            format!("the ingest request is not serialisable: {error}"),
        )
    })?;

    fs3_store::enqueue_job(&state.db, INGEST_SESSION, &dedupe_key, &payload, delay)
        .await
        .map_err(fail)?;

    Ok(IngestAccepted {
        address,
        dedupe_key,
        accepted: true,
    })
}

/// The runner's entry point: do the work the route enqueued.
///
/// # Errors
/// Whatever [`ingest`] returns; a malformed payload is terminal.
pub async fn run(state: &AppState, payload: serde_json::Value) -> Result<(), Failure> {
    let request: IngestRequest = crate::runner::payload(payload)?;
    let report = ingest(state, &request).await?;

    // A contended file was NOT read by this run, and the poll that held the
    // lock may have read BEFORE the bytes this job was fired for existed.
    // Settling successfully would lose that delta until something else happened
    // to fire an ingest.
    //
    // This is a RETRYABLE FAILURE rather than a self-enqueue. Round 4 of review
    // showed why the enqueue could not work: `enqueue_job`'s conflict target
    // covers pending AND RUNNING rows, so re-queueing from inside the running
    // job collides with that very row and only moves its deadline — and the
    // runner then settles the same row `done`. The intended retry never
    // existed. Failing retryably hands the job to the machinery that already
    // knows how to back off and re-run it.
    if report.contended > 0 {
        return Err(Failure::new(
            &catalog::QUEUE_JOB_FAILED,
            format!(
                "{} session file(s) were being polled by another ingest of the same \
                 conversation; this run did not read them",
                report.contended
            ),
        )
        .retryable(true));
    }
    Ok(())
}

/// What to tell an operator who just submitted one.
#[must_use]
pub fn next_after_submit(accepted: &IngestAccepted) -> String {
    format!(
        "queued {} for ingest. `flowspace3 status` watches the queue drain; then \
         `flowspace3 search \"<question>\" --source conversation`. Repeat firings of this \
         address collapse into one job while it is still pending.",
        accepted.address
    )
}

/// Where a session was ACTUALLY recorded, asked of the store rather than
/// inferred from a slug.
///
/// The seat route defaults `folder` to the git directory pij recorded, and pij
/// registers a worktree-resident seat against its MAIN CLONE — while omp and
/// claude slug by the seat's real working directory. For a fleet that works in
/// worktrees, which is this one, the default is therefore wrong more often
/// than it is right, and first light hit it on its first run.
///
/// The fix does NOT un-slug a directory name: a slug joins path components
/// with `-`, so `-substrate-flowspace-fs3-convo-ingest` is ambiguous and
/// inverting it would guess. Both stores record the working directory INSIDE
/// the session — omp on its `session` header, claude on its content rows — so
/// the session is asked instead, and the answer is exact.
///
/// Returns `None` when no store directory holds the id, which is a genuinely
/// unknown session rather than a misaddressed one.
fn discover_folder(harness: Harness, session_id: &str, home: &Path) -> Option<PathBuf> {
    let (root, matches): (PathBuf, fn(&str, &str) -> bool) = match harness {
        Harness::Omp => (home.join(".omp/agent/sessions"), |name, id| {
            name.ends_with(&format!("_{id}.jsonl"))
        }),
        Harness::Claude => (home.join(".claude/projects"), |name, id| {
            name == format!("{id}.jsonl")
        }),
        // The ledger is addressed by seat and the metrics store is one
        // database: neither has a workspace-slugged directory to search.
        Harness::PijLedger | Harness::MetricsDb => return None,
    };

    for slug_dir in std::fs::read_dir(&root).ok()?.flatten() {
        let Ok(entries) = std::fs::read_dir(slug_dir.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !matches(name, session_id) {
                continue;
            }
            if let Some(cwd) = cwd_of(&entry.path()) {
                return Some(cwd);
            }
        }
    }
    None
}

/// The working directory a session file records, from the first record that
/// carries one.
fn cwd_of(path: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(path).ok()?;
    for line in text.lines().take(64) {
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(cwd) = record.get("cwd").and_then(serde_json::Value::as_str) {
            return Some(PathBuf::from(cwd));
        }
    }
    None
}

/// The native session id an input addresses.
fn session_id_of(input: &IngestInput) -> &str {
    match input {
        IngestInput::Pij { id, .. } => id,
        IngestInput::Native { session_id, .. } => session_id,
    }
}

/// The same address, pointed at a different workspace.
fn with_folder(input: IngestInput, folder: PathBuf) -> IngestInput {
    match input {
        IngestInput::Pij { id, .. } => IngestInput::Pij { id, folder },
        IngestInput::Native {
            session_id,
            harness,
            ..
        } => IngestInput::Native {
            session_id,
            harness,
            folder,
        },
    }
}

/// The conversation a session belongs to, derived rather than minted.
///
/// A uuid-shaped value from `sha256("fs3-convo-v1:<harness>/<session_id>")`,
/// with the version and variant nibbles set so it is a well-formed v8 uuid.
/// Deterministic on purpose: the seat route and the native route resolve to the
/// same `(harness, session_id)` and must therefore land the same conversation
/// (ac-0002), and forgetting a cursor must make a re-ingest a re-read rather
/// than a second copy of the conversation.
#[must_use]
pub fn conversation_guid(harness: Harness, session_id: &str) -> ConversationId {
    let seed = format!("fs3-convo-v1:{harness}/{session_id}");
    let digest = content_hash(seed.as_bytes());
    let hex: String = digest
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(32)
        .collect();
    let bytes = hex.as_bytes();
    let group = |from: usize, to: usize| String::from_utf8_lossy(&bytes[from..to]).to_string();
    // Version 8 (custom) and the RFC 4122 variant, so this cannot be mistaken
    // for a store's own random uuid.
    let text = format!(
        "{}-{}-8{}-a{}-{}",
        group(0, 8),
        group(8, 12),
        group(13, 16),
        group(17, 20),
        group(20, 32)
    );
    ConversationId::new(text).expect("a hex digest laid out as 8-4-4-4-12 is a conversation id")
}

/// The workspace-slug directory name a store uses for `folder`.
///
/// The two conventions differ and the difference is measured, not assumed
/// (impl-guide, MEASURED 2026-08-28): claude slugs the ABSOLUTE path, while omp
/// strips the home prefix first — `-substrate-flowspace-flowspace3` rather than
/// `-Users-jordanknight-substrate-flowspace-flowspace3`.
#[must_use]
pub fn workspace_slug(harness: Harness, folder: &Path, home: &Path) -> String {
    let path = if harness == Harness::Omp {
        folder.strip_prefix(home).unwrap_or(folder)
    } else {
        folder
    };
    let text = path.to_string_lossy();
    let trimmed = text.trim_start_matches('/');
    format!("-{}", trimmed.replace('/', "-"))
}

/// Build the reader for a store, rooted under `home` and scoped to `folder`.
fn source_for(
    harness: Harness,
    folder: &Path,
    home: &Path,
    remote_url: Option<&str>,
) -> Result<Box<dyn ConversationSource>, Failure> {
    Ok(match harness {
        Harness::Claude => Box::new(ClaudeSource::new(
            home.join(".claude/projects")
                .join(workspace_slug(Harness::Claude, folder, home)),
        )),
        Harness::Omp => Box::new(OmpSource::from_home(home)),
        Harness::PijLedger => Box::new(PijLedgerSource::from_home(home)),
        Harness::MetricsDb => {
            let remote = remote_url.ok_or_else(|| {
                Failure::new(
                    &catalog::QUERY_INVALID,
                    "the git-ai metrics store is machine-wide, so an ingest from it MUST be \
                     scoped to a repository — and this folder has no git remote to scope by"
                        .to_string(),
                )
                .with_fix("ingest from a checkout with an `origin` remote, or address the session in its native store instead")
                .retryable(false)
            })?;
            Box::new(MetricsDbSource::new(
                home.join(".git-ai/metrics.sqlite3"),
                RepoScope::remote_url(remote),
            ))
        }
    })
}

/// Ingest one addressed conversation and every child of it.
///
/// # Errors
/// [`catalog::QUERY_INVALID`] for an address this build cannot resolve; store
/// and reader failures mapped by their own codes.
pub async fn ingest(state: &AppState, request: &IngestRequest) -> Result<IngestReport, Failure> {
    let home = home_dir()?;
    let (input, harness) = address(request, &home)?;
    let folder = input_folder(&input);
    let remote = remote_url(&folder);
    let mut folder = folder;
    let mut input = input;
    let mut resolved = tokio::task::spawn_blocking({
        let source = source_for(harness, &folder, &home, remote.as_deref())?;
        let input = input.clone();
        move || source.resolve(&input)
    })
    .await
    .map_err(|error| join_failure(&error))?;

    // The folder we were handed may be the wrong one — see `discover_folder`.
    // Ask the store where the session actually lives, then resolve again. Only
    // once: a second miss is a session no store holds.
    if resolved.is_err()
        && let Some(found) = discover_folder(harness, session_id_of(&input), &home)
        && found != folder
    {
        folder = found;
        input = with_folder(input, folder.clone());
        resolved = tokio::task::spawn_blocking({
            let source = source_for(harness, &folder, &home, remote.as_deref())?;
            let input = input.clone();
            move || source.resolve(&input)
        })
        .await
        .map_err(|error| join_failure(&error))?;
    }

    let files = resolved.map_err(|error| reader_failure(&error.to_string()))?;

    let mut report = IngestReport {
        harness: harness.to_string(),
        folder: folder.to_string_lossy().to_string(),
        ..IngestReport::default()
    };
    let floor = state.config.indexing.turn_summary_min_bytes;

    for file in files {
        // Step 0: LOOKUP, never a mint that guesses. An existing row is the
        // answer; its absence means this is a first ingest and the derivation
        // below is what a first ingest agrees on.
        let existing =
            fs3_store::ingest_cursors::conversation_for(&state.db, harness, &file.session_id)
                .await
                .map_err(fail)?;
        let guid = existing
            .clone()
            .unwrap_or_else(|| conversation_guid(harness, &file.session_id));

        // SERIALISE PER CONVERSATION — for real this time. The queue does not
        // do it: `SERIAL_KINDS` means claimed one at a time, not RUN one at a
        // time, so `drain` can have several ingest jobs in flight up to
        // `worker_concurrency`. And two live queue keys can address ONE
        // conversation — first light had `ingest:pij/<seat>@<folder>` and
        // `ingest:omp/<uuid>@<folder>` for the same session — so the dedupe key
        // cannot serialise it either. Cross-model review found the claim in the
        // docs and the absence in the code.
        //
        // A Postgres advisory lock keyed on the conversation is the smallest
        // thing that actually serialises: turn numbers come from the
        // conversation's own stored turns, so two polls reading one high-water
        // mark before either commits is the loss path.
        let outcome =
            fs3_store::ingest_cursors::try_with_conversation_lock(&state.db, &guid, || async {
                let cursor =
                    fs3_store::ingest_cursors::load_cursor(&state.db, harness, &file.session_id)
                        .await
                        .map_err(fail)?;

                // Blocking IO, so off the async thread — exactly as the local ONNX
                // embedder is handled.
                let batch = tokio::task::spawn_blocking({
                    let source = source_for(harness, &folder, &home, remote.as_deref())?;
                    let file = file.clone();
                    move || source.read_incremental(&file, cursor.as_ref())
                })
                .await
                .map_err(|error| join_failure(&error))?
                .map_err(|error| reader_failure(&error.to_string()))?;

                // A session that has produced nothing and has never been stored is not
                // a conversation yet: creating an empty header and a cursor for it
                // would leave a row nothing can ever fill in.
                let known = existing.is_some();
                if batch.records.is_empty() && !known {
                    return Ok(None);
                }

                // The header must exist before the ledger is asked for the
                // conversation's high-water mark, and before the poll is committed:
                // `ingest_cursors.conversation_id` is a real foreign key, on purpose.
                //
                // `started_at` comes from the FIRST RECORD READ rather than the clock:
                // a conversation began when its first turn did, and an ingest-time
                // stamp would make the same conversation start at a different moment
                // depending on when someone happened to run this.
                //
                // And when this poll read NOTHING there is deliberately no fallback
                // stamp: an epoch default would be a date nobody chose, absorbed
                // silently, which is the pattern u2 caught in claude's ordinal and u1a
                // then found again in its own timestamps. A poll with no records has
                // nothing to date the conversation by, and the header it would be
                // dating already exists — so the upsert is SKIPPED rather than fed a
                // number.
                if let Some(first) = batch.records.first() {
                    let header = Conversation {
                        guid: guid.clone(),
                        repo_identity: remote.clone(),
                        worktree: Some(folder.to_string_lossy().to_string()),
                        base_sha: None,
                        title: Some(conversation_title(&file.session_id, file.kind)),
                        started_at: first.at.clone(),
                        // The link the reader discovered, made DURABLE.
                        // Derived rather than looked up: the parent's
                        // conversation id is the same deterministic function of
                        // (harness, its session id) that the parent's own row
                        // uses, so this needs no query and cannot race a parent
                        // that has not been ingested yet.
                        parent: file
                            .parent_session_id
                            .as_deref()
                            .map(|parent| conversation_guid(harness, parent)),
                    };
                    fs3_store::upsert_conversation(&state.db, &header)
                        .await
                        .map_err(fail)?;
                }

                let ordinals: Vec<&str> = batch
                    .records
                    .iter()
                    .map(|record| record.ordinal.as_str())
                    .collect();
                let view = fs3_store::ingest_cursors::ledger_view(
                    &state.db,
                    harness,
                    &file.session_id,
                    &guid,
                    &ordinals,
                )
                .await
                .map_err(fail)?;

                let prepared = prepare_batch(&batch.records, &view.seen, view.next_turn_no);
                let appended = fs3_store::append_turns(&state.db, &guid, &prepared.turns, {
                    move |element: &Element| earns_summary(&element.raw_text, floor)
                })
                .await
                .map_err(fail)?;

                // THE BACKSTOP. See `ledger_disagreement` for why it reads
                // `already_stored` alone.
                if let Some(detail) = ledger_disagreement(
                    prepared.turns.len(),
                    appended.already_stored,
                    harness,
                    &file.session_id,
                    &guid,
                ) {
                    return Err(Failure::new(&catalog::QUERY_INVALID, detail).retryable(false));
                }

                // Record the poll even when nothing was appended: the reader still
                // moved over bytes, and a cursor that did not advance re-reads them.
                fs3_store::ingest_cursors::commit_poll(
                    &state.db,
                    harness,
                    &file.session_id,
                    &guid,
                    &batch.cursor,
                    &prepared.ledger,
                )
                .await
                .map_err(fail)?;

                let identity = remote.clone().unwrap_or_else(|| UNANCHORED.to_string());
                let summarized =
                    enrich::enqueue_for_turns(state, &identity, &appended.accepted, floor).await?;
                Ok(Some(SessionIngest {
                    guid: guid.as_str().to_string(),
                    session_id: file.session_id.clone(),
                    kind: match file.kind {
                        SessionKind::Main => "main".to_string(),
                        SessionKind::Subagent => "subagent".to_string(),
                    },
                    parent_session_id: file.parent_session_id.clone(),
                    records_read: batch.records.len(),
                    turns_new: appended.accepted.len(),
                    deduped: prepared.deduped,
                    summarized,
                    rescanned: batch.rescanned,
                }))
            })
            .await
            .map_err(fail)?;

        // `None` means another poll of this conversation holds the lock. It is
        // reading the same bytes, so there is nothing to wait for and nothing
        // to redo — leave the work to whichever poll got there first.
        let Some(outcome) = outcome else {
            // Another poll of this conversation holds the lock. It is reading
            // the same bytes, so there is nothing to redo — but it may have
            // read BEFORE the bytes this run was fired for arrived, so
            // reporting success here would settle a job that never read them.
            // Counted, and re-queued below.
            report.contended += 1;
            continue;
        };
        let Some(session) = outcome? else {
            continue;
        };
        report.records_read += session.records_read;
        report.turns_new += session.turns_new;
        report.deduped += session.deduped;
        report.summarized += session.summarized;
        report.sessions.push(session);
    }

    Ok(report)
}

/// Whether the ordinal ledger and the turns table disagree about what is stored.
///
/// `prepare_batch` removes every seen and within-batch duplicate BEFORE
/// `append_turns`, so `already_stored` counts ONLY turns the ledger classified
/// as new. Any nonzero value is therefore a disagreement: a concurrent poll
/// numbering against the same high-water mark, or a ledger that lost rows a
/// cursor advanced past.
///
/// It is a function rather than an inline `if` so the condition itself is
/// testable. Two rounds of cross-model review landed on this guard — the first
/// version was an arithmetic identity that could never fire, the second added a
/// `deduped == 0` qualifier that silenced it on the ordinary
/// rescan-plus-growth batch — and neither defect was reachable from a test
/// while the condition lived inside the pipeline.
#[must_use]
pub fn ledger_disagreement(
    prepared: usize,
    already_stored: usize,
    harness: Harness,
    session_id: &str,
    guid: &ConversationId,
) -> Option<String> {
    (already_stored > 0).then(|| {
        format!(
            "ingest anomaly for {harness}/{session_id}: the ledger said {prepared} turns were \
             new, and the store already had {already_stored} of them — the ordinal ledger and \
             the turns table disagree about what is in conversation {}. Another poll of this \
             conversation probably numbered against the same high-water mark.",
            guid.as_str()
        )
    })
}

/// The operator-facing next step, which must carry `deduped`.
#[must_use]
pub fn next_after_ingest(report: &IngestReport) -> String {
    format!(
        "read {}, appended {}, deduped {} across {} session file(s) under {}. \
         `flowspace3 status` watches the queue drain; then \
         `flowspace3 search \"<question>\" --source conversation`.",
        report.records_read,
        report.turns_new,
        report.deduped,
        report.sessions.len(),
        report.folder
    )
}

/// The home directory the stores live under.
fn home_dir() -> Result<PathBuf, Failure> {
    std::env::var("HOME").map(PathBuf::from).map_err(|_| {
        Failure::new(
            &catalog::QUERY_INVALID,
            "HOME is not set, and every native session store is addressed beneath it".to_string(),
        )
        .retryable(false)
    })
}

/// Resolve the request into the reader's address vocabulary.
///
/// The seat route runs the `pij sessions` join; the native route needs no join.
/// Both reduce to `(harness, session_id)`, which is what makes ac-0002 a
/// property of the shape rather than a coincidence.
fn address(request: &IngestRequest, home: &Path) -> Result<(IngestInput, Harness), Failure> {
    let folder = request
        .folder
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| home.to_path_buf());

    match (&request.pij_id, &request.session_id, &request.harness) {
        (Some(seat), None, None) => {
            let rows = pij_sessions()?;
            let bound = fs3_core::resolve_seat(&rows, seat).map_err(|error| {
                Failure::new(&catalog::QUERY_INVALID, error.to_string())
                    .with_fix("check `pij sessions` for the seat, or address the session directly with --session and --harness")
                    .retryable(false)
            })?;
            let folder = request
                .folder
                .as_ref()
                .map(PathBuf::from)
                .or(bound.folder)
                .unwrap_or_else(|| home.to_path_buf());
            Ok((
                IngestInput::Native {
                    session_id: bound.session_id,
                    harness: bound.harness,
                    folder,
                },
                bound.harness,
            ))
        }
        (None, Some(session_id), Some(harness)) => {
            let harness: Harness = harness.parse().map_err(|error: fs3_core::Error| {
                Failure::new(&catalog::QUERY_INVALID, error.to_string()).retryable(false)
            })?;
            if harness == Harness::PijLedger {
                return Ok((
                    IngestInput::Pij {
                        id: session_id.clone(),
                        folder,
                    },
                    harness,
                ));
            }
            Ok((
                IngestInput::Native {
                    session_id: session_id.clone(),
                    harness,
                    folder,
                },
                harness,
            ))
        }
        _ => Err(Failure::new(
            &catalog::QUERY_INVALID,
            "address a conversation either by seat (--pij) or by session (--session with \
             --harness), never both and never neither"
                .to_string(),
        )
        .retryable(false)),
    }
}

/// The `pij sessions` join table, read once per ingest.
fn pij_sessions() -> Result<Vec<fs3_core::SessionRow>, Failure> {
    let output = std::process::Command::new("pij")
        .args(["sessions", "--json"])
        .output()
        .map_err(|error| {
            Failure::new(
                &catalog::QUERY_INVALID,
                format!("`pij sessions --json` could not be run: {error}"),
            )
            .with_fix(
                "address the session directly with --session and --harness, which needs no join",
            )
            .retryable(false)
        })?;
    if !output.status.success() {
        return Err(Failure::new(
            &catalog::QUERY_INVALID,
            format!(
                "`pij sessions --json` failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        )
        .retryable(false));
    }
    fs3_core::parse_rows(&String::from_utf8_lossy(&output.stdout))
        .map_err(|error| Failure::new(&catalog::QUERY_INVALID, error.to_string()).retryable(false))
}

/// The workspace an input names.
fn input_folder(input: &IngestInput) -> PathBuf {
    match input {
        IngestInput::Pij { folder, .. } | IngestInput::Native { folder, .. } => folder.clone(),
    }
}

/// The repository's `origin` URL, as the machine-wide metrics store stamps it.
///
/// Deliberately the RAW url rather than [`fs3_core::RepoIdentity`]: git-ai
/// records the remote verbatim and the metrics reader scopes by equality on it,
/// so a canonicalised key would match nothing. A repository with no remote
/// yields `None`, which is a REFUSAL for the machine-wide store rather than a
/// fallback — there is no safe unscoped read of a store that holds every
/// project on the machine.
fn remote_url(folder: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!url.is_empty()).then_some(url)
}

/// A title a human can recognise in a listing.
fn conversation_title(session_id: &str, kind: SessionKind) -> String {
    match kind {
        SessionKind::Main => format!("session {session_id}"),
        SessionKind::Subagent => format!("subagent {session_id}"),
    }
}

fn join_failure(error: &tokio::task::JoinError) -> Failure {
    Failure::new(
        &catalog::QUERY_INVALID,
        format!("the blocking reader task did not finish: {error}"),
    )
}

fn reader_failure(message: &str) -> Failure {
    Failure::new(&catalog::QUERY_INVALID, message.to_string()).retryable(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_addressing_routes_derive_the_same_conversation() {
        // ac-0002 by construction: both routes reduce to (harness, session_id),
        // so they cannot disagree about which conversation a session is.
        let by_seat = conversation_guid(Harness::Omp, "01a045f4-edc2-7000-8dc7-47d6d5677147");
        let by_native = conversation_guid(Harness::Omp, "01a045f4-edc2-7000-8dc7-47d6d5677147");
        assert_eq!(by_seat, by_native);
    }

    #[test]
    fn the_same_session_id_in_two_stores_is_two_conversations() {
        // metrics.sqlite3 MIRRORS claude session ids, so the id alone does not
        // identify a conversation — the store is part of its identity.
        let native = conversation_guid(Harness::Claude, "a5a5588f-0979-439f-a1bf-ddf185a089c7");
        let mirrored =
            conversation_guid(Harness::MetricsDb, "a5a5588f-0979-439f-a1bf-ddf185a089c7");
        assert_ne!(native, mirrored);
    }

    #[test]
    fn a_derived_guid_is_a_well_formed_conversation_id() {
        let guid = conversation_guid(Harness::PijLedger, "pij-appalling-slug");
        let text = guid.as_str();
        assert_eq!(text.len(), 36, "8-4-4-4-12 plus four hyphens: {text}");
        assert_eq!(
            &text[14..15],
            "8",
            "version nibble marks it derived, not random"
        );
        assert_eq!(&text[19..20], "a", "RFC 4122 variant");
        // Round-trips through the validator the intake surface uses.
        ConversationId::new(text.to_string()).expect("re-parses");
    }

    #[test]
    fn omp_strips_the_home_prefix_and_claude_does_not() {
        // MEASURED, and the reason resolution differs per store. Getting this
        // backwards resolves to a directory that does not exist and reports an
        // empty conversation rather than an error.
        let home = Path::new("/Users/jordanknight");
        let folder = Path::new("/Users/jordanknight/substrate/flowspace/flowspace3");
        assert_eq!(
            workspace_slug(Harness::Omp, folder, home),
            "-substrate-flowspace-flowspace3"
        );
        assert_eq!(
            workspace_slug(Harness::Claude, folder, home),
            "-Users-jordanknight-substrate-flowspace-flowspace3"
        );
    }

    #[test]
    fn a_folder_outside_home_still_slugs_for_omp() {
        let home = Path::new("/Users/jordanknight");
        let folder = Path::new("/opt/work/repo");
        assert_eq!(workspace_slug(Harness::Omp, folder, home), "-opt-work-repo");
    }

    /// A scratch home holding one omp session under `slug`, whose `session`
    /// header names `cwd`.
    fn omp_home(slug: &str, session_id: &str, cwd: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock is after 1970")
            .as_nanos();
        let home = std::env::temp_dir().join(format!("fs3-convo-ingest-{nanos}-{unique}"));
        let dir = home.join(".omp/agent/sessions").join(slug);
        std::fs::create_dir_all(&dir).expect("a scratch sessions directory");
        let body = format!(
            "{{\"type\":\"title\",\"title\":\"\"}}\n\
             {{\"type\":\"session\",\"id\":\"{session_id}\",\"cwd\":\"{cwd}\"}}\n"
        );
        std::fs::write(
            dir.join(format!("2026-08-28T01-21-14-690Z_{session_id}.jsonl")),
            body,
        )
        .expect("a scratch session file");
        home
    }

    #[test]
    fn the_anomaly_guard_fires_on_a_mixed_batch_too() {
        // The guard has been wrong twice, and BOTH defects were invisible while
        // the condition lived inline in the pipeline. Round 1: it compared
        // accepted + already_stored against prepared.turns.len(), which is an
        // arithmetic identity. Round 2: it added `deduped == 0`, which silenced
        // it on the ordinary rescan-plus-growth batch — one already-seen
        // ordinal makes deduped nonzero, and a colliding NEW turn would then be
        // classified already-stored while the cursor committed past it.
        //
        // `prepare_batch` removes every seen record BEFORE `append_turns`, so
        // `already_stored` counts only turns the ledger called new. The batch
        // having ALSO deduped something is irrelevant, which is exactly what
        // the second defect got wrong — so the mixed case is pinned here.
        let guid = conversation_guid(Harness::Omp, "a-session");

        assert!(
            ledger_disagreement(3, 0, Harness::Omp, "a-session", &guid).is_none(),
            "nothing already stored is the healthy shape"
        );

        let mixed = ledger_disagreement(1, 1, Harness::Omp, "a-session", &guid)
            .expect("a batch that also deduped still reports a disagreement");
        assert!(
            mixed.contains("the store already had 1 of them"),
            "and it says what disagreed: {mixed}"
        );

        let clean = ledger_disagreement(4, 2, Harness::Claude, "b-session", &guid)
            .expect("a batch that deduped nothing reports one too");
        assert!(clean.contains("4 turns were new"));
    }

    #[test]
    fn a_worktree_seat_is_found_even_though_pij_names_its_main_clone() {
        // The defect first light hit on its first run: pij records a seat's
        // gitCommonDir, which is the MAIN CLONE, while omp slugs by the seat's
        // real working directory — its WORKTREE. Every seat of this fleet is
        // worktree-resident, so the default is wrong more often than right.
        let session = "01a045f4-edc2-7000-8dc7-47d6d5677147";
        let worktree = "/Users/x/substrate/flowspace/fs3-convo-ingest";
        let home = omp_home("-substrate-flowspace-fs3-convo-ingest", session, worktree);

        // What pij would have handed us: the main clone.
        let from_pij = Path::new("/Users/x/substrate/flowspace/flowspace3");
        assert_eq!(
            workspace_slug(Harness::Omp, from_pij, &home),
            "-Users-x-substrate-flowspace-flowspace3",
            "the clone-derived slug is not where the session lives"
        );

        let found = discover_folder(Harness::Omp, session, &home).expect("the session is found");
        assert_eq!(
            found,
            PathBuf::from(worktree),
            "discovery returns the cwd the STORE recorded, not an un-slugged guess"
        );
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn discovery_reads_the_stores_own_cwd_rather_than_inverting_a_slug() {
        // The slug joins path components with `-`, so `fs3-convo-ingest` is
        // indistinguishable from three nested directories. Inverting it would
        // guess; asking the session cannot.
        let session = "01a045f4-edc2-7000-8dc7-47d6d5677147";
        let cwd = "/Users/x/substrate/flowspace/fs3-convo-ingest";
        let home = omp_home("-substrate-flowspace-fs3-convo-ingest", session, cwd);
        let found = discover_folder(Harness::Omp, session, &home).expect("found");
        assert_ne!(
            found,
            PathBuf::from("/Users/x/substrate/flowspace/fs3/convo/ingest"),
            "an un-slugged path would have split the hyphens into directories"
        );
        assert_eq!(found, PathBuf::from(cwd));
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_session_no_store_holds_is_not_discovered() {
        let home = omp_home(
            "-somewhere",
            "aaaaaaaa-0000-7000-8000-000000000000",
            "/tmp/x",
        );
        assert!(
            discover_folder(Harness::Omp, "ffffffff-0000-7000-8000-000000000000", &home).is_none(),
            "an unknown session is unknown, not misaddressed"
        );
        std::fs::remove_dir_all(&home).ok();
    }
}
