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
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
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
    /// Whether the reader restarted from the beginning.
    pub rescanned: bool,
}

/// What an ingest did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct IngestReport {
    /// Which store was read.
    pub harness: String,
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
    let files = tokio::task::spawn_blocking({
        let source = source_for(harness, &folder, &home, remote.as_deref())?;
        let input = input.clone();
        move || source.resolve(&input)
    })
    .await
    .map_err(|error| join_failure(&error))?
    .map_err(|error| reader_failure(&error.to_string()))?;

    let mut report = IngestReport {
        harness: harness.to_string(),
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

        let cursor = fs3_store::ingest_cursors::load_cursor(&state.db, harness, &file.session_id)
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
            continue;
        }

        // The header must exist before the ledger is asked for the
        // conversation's high-water mark, and before the poll is committed:
        // `ingest_cursors.conversation_id` is a real foreign key, on purpose.
        //
        // `started_at` comes from the FIRST RECORD READ rather than the clock:
        // a conversation began when its first turn did, and an ingest-time
        // stamp would make the same conversation start at a different moment
        // depending on when someone happened to run this.
        let started_at = batch
            .records
            .first()
            .map(|record| record.at.clone())
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());
        let header = Conversation {
            guid: guid.clone(),
            repo_identity: remote.clone(),
            worktree: Some(folder.to_string_lossy().to_string()),
            base_sha: None,
            title: Some(conversation_title(&file.session_id, file.kind)),
            started_at,
        };
        fs3_store::upsert_conversation(&state.db, &header)
            .await
            .map_err(fail)?;

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

        // The backstop that does not depend on the numbering being right:
        // every prepared turn is either accepted or already stored, and any
        // other outcome means the ledger and the turns table disagree about
        // what exists.
        let accounted = appended.accepted.len() + appended.already_stored;
        if accounted != prepared.turns.len() {
            return Err(Failure::new(
                &catalog::QUERY_INVALID,
                format!(
                    "ingest anomaly for {}/{}: prepared {} turns, the store accounted for {} \
                     ({} accepted, {} already stored) — the ledger and the turns table disagree \
                     about what is in this conversation",
                    harness,
                    file.session_id,
                    prepared.turns.len(),
                    accounted,
                    appended.accepted.len(),
                    appended.already_stored
                ),
            )
            .retryable(false));
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
        report.summarized +=
            enrich::enqueue_for_turns(state, &identity, &appended.accepted, floor).await?;
        report.records_read += batch.records.len();
        report.turns_new += appended.accepted.len();
        report.deduped += prepared.deduped;
        report.sessions.push(SessionIngest {
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
            rescanned: batch.rescanned,
        });
    }

    Ok(report)
}

/// The operator-facing next step, which must carry `deduped`.
#[must_use]
pub fn next_after_ingest(report: &IngestReport) -> String {
    format!(
        "read {}, appended {}, deduped {} across {} session file(s). \
         `flowspace3 status` watches the queue drain; then \
         `flowspace3 search \"<question>\" --source conversation`.",
        report.records_read,
        report.turns_new,
        report.deduped,
        report.sessions.len()
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
}
