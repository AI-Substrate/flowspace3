//! Registering a root, and turning it into scan work.
//!
//! This is where the three landed pieces meet, and the grain each is used at is
//! the whole design:
//!
//! * [`fs3_git::repo_identity`] answers **what repository is this**, by walking
//!   UP from the added path. A subdirectory of a checkout keys to the
//!   repository, not to itself, so content indexed from one frame is shared
//!   rather than duplicated.
//! * [`fs3_parsers::discovery::discover`] answers **which files are worth
//!   indexing**, walking DOWN from the added path and no higher. Its own
//!   `WalkBuilder` is rooted there, so "strictly under the added root" is
//!   structural rather than a filter applied afterwards — and `parents(true)`
//!   means a `.gitignore` ABOVE the root still binds.
//! * [`fs3_git::blob_id`] answers **what are these bytes**, per file.
//!
//! What is deliberately NOT used: `fs3_git::snapshot`. It enumerates everything
//! git can see in the whole worktree and hashes all of it, while discovery hands
//! back the files fs3 will actually parse — 133 rather than 3,200 on this
//! repository. Per-file blob ids over the filtered set is both the cheaper walk
//! and the one whose frame matches the row we store.
//!
//! Blob ids are frame-independent, so the content layer dedupes across frames:
//! adding a subdirectory and later the whole repository re-parses and
//! re-enriches nothing that was already paid for.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fs3_parsers::discovery::{self, DiscoverySettings};
use fs3_store::PgPool;
use serde::{Deserialize, Serialize};

use crate::wiring::AppState;
use fs3_core::EventKind;
use fs3_core::views::roots::{PrunedDirectoryRow, RootReport, SkipCount};

/// The job kind a file scan is queued under.
pub const SCAN_FILE: &str = "scan_file";

/// Emit at most one count-driven progress line per this many walked files.
pub const SCAN_PROGRESS_FILES: u64 = 256;

/// Bound visible silence during a slow walk.
pub const SCAN_PROGRESS_INTERVAL: Duration = Duration::from_millis(1_000);

struct ScanProgress {
    root: String,
    root_path: String,
    files_seen: u64,
    enqueued: u64,
    since_emit: u64,
    last_emit: Instant,
}

impl ScanProgress {
    fn new(root: String, root_path: String) -> Self {
        Self {
            root,
            root_path,
            files_seen: 0,
            enqueued: 0,
            since_emit: 0,
            last_emit: Instant::now(),
        }
    }

    fn tick(&mut self, state: &AppState, current: &str) {
        self.since_emit += 1;
        if self.since_emit < SCAN_PROGRESS_FILES
            && self.last_emit.elapsed() < SCAN_PROGRESS_INTERVAL
        {
            return;
        }
        self.publish(state, Some(current.to_string()));
    }

    fn pulse(&mut self, state: &AppState, current: &str) {
        if self.last_emit.elapsed() >= SCAN_PROGRESS_INTERVAL {
            self.publish(state, Some(current.to_string()));
        }
    }

    fn finish(&mut self, state: &AppState) {
        self.publish(state, None);
    }

    fn publish(&mut self, state: &AppState, current: Option<String>) {
        state.emit(EventKind::ScanProgress {
            root: self.root.clone(),
            root_path: self.root_path.clone(),
            files_seen: self.files_seen,
            enqueued: self.enqueued,
            current,
        });
        self.since_emit = 0;
        self.last_emit = Instant::now();
    }
}

/// What `POST /roots` and `POST /scan` take.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootRequest {
    /// The directory to register or re-scan.
    pub path: String,
    /// Explicit per-root hidden-directory choice; absent preserves stored state.
    #[serde(default)]
    pub include_hidden: Option<bool>,
}

/// What one `scan_file` job needs to do its work.
///
/// The worktree id and the relative path rather than an absolute one: an
/// absolute path in a queue row is a fact about the machine that enqueued it,
/// and the root it belongs to can move. The runner rebuilds the absolute path
/// from the worktree's current `root_path`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanFileJob {
    /// Which registered worktree the path belongs to.
    pub worktree_id: i64,
    /// The repository identity — carried so the runner can resolve per-repo
    /// provider overrides without a second query.
    pub identity: String,
    /// Path relative to the worktree root, `/`-separated.
    pub path: String,
    /// The blob the file hashed to when it was enqueued.
    pub blob: String,
}

impl ScanFileJob {
    /// The dedupe key: one live job per `(worktree, path)`.
    ///
    /// Not per blob: a file edited twice before the queue drains must collapse
    /// into ONE pending scan of its latest content, and keying by blob would
    /// leave a job pointing at bytes that are already history.
    #[must_use]
    pub fn dedupe_key(&self) -> String {
        format!("scan:{}:{}", self.worktree_id, self.path)
    }
}

/// Register `root`, sync every discovered path→blob mapping, and enqueue only
/// files whose current parsed content cannot be reused.
///
/// Idempotence is content-addressed as well as path-addressed: an unchanged
/// path enqueues nothing, and a new worktree reuses blobs already parsed by
/// this parser version. The mapping is still written before this decision, so
/// the reused content resolves from the new worktree immediately.
///
/// # Errors
/// Discovery failures (an unreadable root, an uncompilable glob), git failures,
/// and store failures, each mapped to its own catalog code by the caller.
pub async fn add_root(
    state: &AppState,
    root: &Path,
    include_hidden: Option<bool>,
) -> Result<RootReport, RootError> {
    scan_root(
        state,
        root,
        include_hidden,
        "added",
        fs3_store::JOB_PRIORITY_DEFAULT,
    )
    .await
}

/// Register a newly discovered root and promote its initial scan work.
///
/// Kept crate-private: explicit add/rescan and watcher paths remain ordinary
/// background work; only the lifecycle detector may select the raised lane.
pub(crate) async fn add_root_with_priority(
    state: &AppState,
    root: &Path,
    priority: fs3_store::JobPriority,
) -> Result<RootReport, RootError> {
    scan_root(state, root, None, "added", priority).await
}

async fn scan_root(
    state: &AppState,
    root: &Path,
    requested_include_hidden: Option<bool>,
    change: &'static str,
    priority: fs3_store::JobPriority,
) -> Result<RootReport, RootError> {
    let root = canonical(root)?;
    let identity = fs3_git::repo_identity(&root)?;
    let root_path = root.to_string_lossy().to_string();
    let identity_key = identity.key().to_string();
    let existing = fs3_store::find_worktree(&state.db, &root_path).await?;
    let root_include_hidden = requested_include_hidden
        .or_else(|| existing.as_ref().map(|worktree| worktree.include_hidden))
        .unwrap_or(false);

    // Discovery decides what is worth indexing; git decides what the bytes are.
    let mut settings = DiscoverySettings::from(&state.config.scan);
    settings.include_hidden |= root_include_hidden;
    let discovery = discovery::discover(&root, &settings)?;
    let mut progress = ScanProgress::new(identity_key.clone(), root_path.clone());

    let is_new_worktree = existing.is_none();
    let worktree_id =
        fs3_store::register_worktree(&state.db, &identity, &root_path, ref_name(&root).as_deref())
            .await?;
    if let Some(include_hidden) = requested_include_hidden {
        fs3_store::set_worktree_include_hidden(&state.db, worktree_id, include_hidden).await?;
    }

    // Hash every accepted file, then write the map in one call. Hashing first
    // means a file that vanishes mid-walk is simply absent from the map rather
    // than a half-written row.
    let mut files = Vec::with_capacity(discovery.files.len());
    for file in &discovery.files {
        progress.files_seen += 1;
        match fs3_git::blob_id(&root.join(&file.path)) {
            Ok(blob) => files.push((file.path.clone(), blob)),
            // A file that disappeared between the walk and the hash is not an
            // error: it is a file that is no longer there, and the next scan
            // will agree.
            Err(fs3_git::Error::Io { .. }) => {}
            Err(error) => return Err(error.into()),
        }
        progress.tick(state, &file.path);
    }

    let known = known_blobs(&state.db, worktree_id).await?;
    let blob_hashes: Vec<&str> = files.iter().map(|(_, blob)| blob.as_str()).collect();
    let parsed_by_current_version =
        fs3_store::blobs_with_parser_version(&state.db, crate::scan::PARSER_VERSION, &blob_hashes)
            .await?;
    let removed = fs3_store::sync_worktree_files(&state.db, worktree_id, &files).await?;

    // One `ddocs` probe per corpus event, AFTER registration (so the snapshot
    // is keyed by a real worktree_id) and BEFORE the scan batch is enqueued.
    // Ordering is load-bearing: a batch that started before its snapshot
    // existed would index the whole root with an absent snapshot and produce a
    // corpus-wide silent degradation that looks exactly like success.
    state
        .set_ddoc_tooling(worktree_id, crate::ddoc::probe(&root).await)
        .await;

    let mut unchanged = 0;
    for (path, blob) in &files {
        let same_blob = known.get(path.as_str()).map(String::as_str) == Some(blob.as_str());
        let parser_is_current = parsed_by_current_version.contains(blob.as_str());
        if !needs_scan_job(path, same_blob, parser_is_current, is_new_worktree) {
            unchanged += 1;
        } else {
            let job = ScanFileJob {
                worktree_id,
                identity: identity_key.clone(),
                path: path.clone(),
                blob: blob.as_str().to_string(),
            };
            fs3_store::enqueue_job_with_priority(
                &state.db,
                SCAN_FILE,
                &job.dedupe_key(),
                &serde_json::to_value(&job).expect("a scan job always serialises"),
                Duration::ZERO,
                priority,
            )
            .await?;
            progress.enqueued += 1;
        }
        // The enqueue phase can itself be slow against a remote store. It is
        // part of the same walk and must not reintroduce visible silence.
        progress.pulse(state, path);
    }
    progress.finish(state);

    let report = RootReport {
        identity: identity_key.clone(),
        identity_source: match identity.source() {
            fs3_core::IdentitySource::Remote => "remote".to_string(),
            fs3_core::IdentitySource::Path => "path".to_string(),
        },
        root_path: root_path.clone(),
        worktree_id,
        include_hidden: root_include_hidden,
        files: files.len(),
        skipped: skip_counts(&discovery),
        pruned: pruned_rows(&discovery),
        enqueued: progress.enqueued as usize,
        unchanged,
        removed,
    };
    state.emit(EventKind::RootChanged {
        change: change.to_string(),
        root: identity_key,
        root_path,
        files: report.files as i64,
    });
    Ok(report)
}

/// Re-scan a root that is already registered.
///
/// Same walk, same diff, same enqueue — the only difference is that an
/// unregistered path is refused rather than added. `add` is how a root joins;
/// `scan` is how it is refreshed, and conflating them would make a typo'd path
/// silently register a new root.
///
/// # Errors
/// [`RootError::NotRegistered`] when nothing was added at this path, plus
/// everything [`add_root`] can raise.
pub async fn rescan_root(state: &AppState, root: &Path) -> Result<RootReport, RootError> {
    let root = canonical(root)?;
    let path = root.to_string_lossy().to_string();
    if fs3_store::find_worktree(&state.db, &path).await?.is_none() {
        return Err(RootError::NotRegistered(path));
    }
    scan_root(
        state,
        &root,
        None,
        "rescanned",
        fs3_store::JOB_PRIORITY_DEFAULT,
    )
    .await
}

/// The path→blob map the store already holds for this worktree.
async fn known_blobs(
    pool: &PgPool,
    worktree_id: i64,
) -> Result<std::collections::HashMap<String, String>, fs3_store::StoreError> {
    let worktrees = fs3_store::list_worktrees(pool).await?;
    if !worktrees.iter().any(|w| w.id == worktree_id) {
        return Ok(std::collections::HashMap::new());
    }
    fs3_store::worktree_file_map(pool, worktree_id).await
}

/// Whether this path needs work beyond its already-synced worktree mapping.
///
/// Ordinary parsed blobs are reusable when a worktree is first registered.
/// An existing worktree whose mapping vanished must still run the scan so its
/// stored tree re-emits enrichment after a partial write or recovery. Ddoc
/// trees also carry graph/tooling-derived state from the presenting worktree,
/// so a newly mapped ddoc runs once even when its blob is already parsed.
fn needs_scan_job(
    path: &str,
    same_blob: bool,
    parser_is_current: bool,
    is_new_worktree: bool,
) -> bool {
    !parser_is_current
        || (!same_blob && (!is_new_worktree || fs3_parsers::is_ddoc_source(Path::new(path))))
}

/// Group the skip ledger by reason. The whole ledger would be thousands of rows
/// on a real repository; the counts are what a human reads, and the per-file
/// detail stays available from discovery itself.
fn skip_counts(discovery: &discovery::Discovery) -> Vec<SkipCount> {
    let mut counts: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    for skipped in &discovery.skipped {
        *counts.entry(skipped.reason.as_str()).or_default() += 1;
    }
    for pruned in &discovery.pruned {
        if pruned.reason == discovery::PruneReason::Hidden {
            *counts.entry(pruned.reason.as_str()).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .map(|(reason, count)| SkipCount {
            reason: reason.to_string(),
            count,
        })
        .collect()
}

/// Name every directory discovery refused to walk — **not** aggregated.
///
/// The opposite call from [`skip_counts`], for the opposite reason. Skips are
/// thousands of files, so a count is what a human can read. Prunes are about
/// eleven directories, and the names are the entire value: a denied directory
/// puts nothing in either file list, so without this the only symptom of
/// `Build/` not being indexed is code missing from search results.
fn pruned_rows(discovery: &discovery::Discovery) -> Vec<PrunedDirectoryRow> {
    discovery
        .pruned
        .iter()
        .map(|pruned| PrunedDirectoryRow {
            path: pruned.path.clone(),
            reason: pruned.reason.as_str().to_string(),
            fix: match pruned.reason {
                discovery::PruneReason::Hidden => {
                    "index hidden directories with `flowspace3 add <root> --include-hidden`"
                        .to_string()
                }
                discovery::PruneReason::StandardIgnore => {
                    "index it anyway with `[scan] standard_ignores = false`".to_string()
                }
            },
        })
        .collect()
}

/// The branch this worktree is on.
///
/// Always `None` today, and named rather than hidden: `fs3-git` exposes
/// identity and blobs, not refs, and widening that crate's surface for a
/// display string would be a dependency bought with nothing.
/// `worktrees.ref_name` stays NULL until something actually reads it.
fn ref_name(_root: &Path) -> Option<String> {
    None
}

/// Resolve to an absolute path, refusing one that is not there.
///
/// A relative path from an HTTP client is resolved against the DAEMON's working
/// directory, which is almost never what the caller meant — so the error names
/// the resolved path, not the one that was typed.
fn canonical(root: &Path) -> Result<PathBuf, RootError> {
    let resolved = std::fs::canonicalize(root)
        .map_err(|_| RootError::NotFound(root.to_string_lossy().to_string()))?;
    if !resolved.is_dir() {
        return Err(RootError::NotADirectory(
            resolved.to_string_lossy().to_string(),
        ));
    }
    Ok(resolved)
}

/// Why a root could not be registered or re-scanned.
#[derive(Debug, thiserror::Error)]
pub enum RootError {
    /// The path does not exist.
    #[error("no such path: {0}")]
    NotFound(String),
    /// The path exists but is a file.
    #[error("not a directory: {0}")]
    NotADirectory(String),
    /// `scan` was called for a path nobody added.
    #[error("no root is registered at {0}")]
    NotRegistered(String),
    /// Reading git failed.
    #[error(transparent)]
    Git(#[from] fs3_git::Error),
    /// The discovery walk could not start.
    #[error(transparent)]
    Discovery(#[from] fs3_parsers::discovery::DiscoveryError),
    /// The store refused.
    #[error(transparent)]
    Store(#[from] fs3_store::StoreError),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One live job per (worktree, path) — NOT per blob. A file edited twice
    /// before the queue drains must collapse into one scan of its latest
    /// content; keying by blob would leave a job pointing at bytes that are
    /// already history.
    #[test]
    fn the_dedupe_key_is_the_path_not_the_content() {
        let job = |blob: &str| ScanFileJob {
            worktree_id: 42,
            identity: "git:github.com/AI-Substrate/flowspace3".to_string(),
            path: "crates/core/src/lib.rs".to_string(),
            blob: blob.to_string(),
        };
        assert_eq!(job("aaaa").dedupe_key(), job("bbbb").dedupe_key());
        assert_eq!(job("aaaa").dedupe_key(), "scan:42:crates/core/src/lib.rs");
    }

    /// Two worktrees holding the same relative path are different work.
    #[test]
    fn two_worktrees_do_not_share_a_scan_job() {
        let job = |worktree_id: i64| ScanFileJob {
            worktree_id,
            identity: "x".to_string(),
            path: "src/lib.rs".to_string(),
            blob: "aaaa".to_string(),
        };
        assert_ne!(job(1).dedupe_key(), job(2).dedupe_key());
    }

    #[test]
    fn current_blobs_are_reused_only_for_safe_new_worktree_paths() {
        assert!(!needs_scan_job("src/lib.rs", false, true, true));
        assert!(needs_scan_job("src/lib.rs", false, false, true));
        assert!(needs_scan_job("src/lib.rs", false, true, false));
        assert!(needs_scan_job("docs/plan.dd.json", false, true, true));
        assert!(!needs_scan_job("docs/plan.dd.json", true, true, true));
    }

    #[test]
    fn a_scan_job_round_trips_through_its_payload() {
        let job = ScanFileJob {
            worktree_id: 7,
            identity: "path:/srv/api".to_string(),
            path: "src/main.rs".to_string(),
            blob: "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391".to_string(),
        };
        let value = serde_json::to_value(&job).unwrap();
        assert_eq!(serde_json::from_value::<ScanFileJob>(value).unwrap(), job);
    }

    #[test]
    fn a_missing_root_names_the_path_it_could_not_find() {
        let error = canonical(Path::new("/definitely/not/here")).unwrap_err();
        assert!(matches!(error, RootError::NotFound(_)));
        assert!(error.to_string().contains("/definitely/not/here"));
    }
}
