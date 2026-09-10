//! File-driven scheduling of the existing native conversation readers.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use fs3_core::views::status::{
    ConversationHarnessStatus, ConversationIngestReceipt, ConversationState, ConversationsStatus,
};
use fs3_core::{ConversationSource, Harness, IngestInput, SessionFile, SessionKind, SourceCursor};
use fs3_providers::conversation_sources::claude::ClaudeSource;
use fs3_store::ingest_cursors;

use crate::convo_ingest::{IngestRequest, cwd_of, submit_after_at_home};
use crate::{AppState, Pass, Reconcile};

/// Scheduling policy; the shared reconcile runner supplies five-second ticks.
#[derive(Clone, Copy, Debug)]
pub struct PollConfig {
    /// Zero disables discovery and submission altogether.
    pub every_ticks: u32,
    /// Only cursorless files outside this window are skipped.
    pub lookback_days: u32,
    /// Submission policy, deliberately not another user configuration knob.
    pub max_per_pass: std::num::NonZeroUsize,
    /// Upper limit: effective spacing is min(stagger, poll cadence / cap).
    /// Enqueue upserts use GREATEST(not_before); spacing beyond a cadence
    /// would let fast repeated polls postpone pending jobs indefinitely.
    pub stagger: Duration,
}

impl Default for PollConfig {
    fn default() -> Self {
        Self {
            every_ticks: 12,
            lookback_days: 14,
            max_per_pass: std::num::NonZeroUsize::new(10).expect("nonzero poll cap"),
            stagger: Duration::from_secs(6),
        }
    }
}

impl PollConfig {
    fn effective_stagger(self) -> Duration {
        let cadence = Duration::from_secs(u64::from(self.every_ticks) * 5);
        self.stagger
            .min(cadence.div_f64(self.max_per_pass.get() as f64))
    }
}

/// Stall means THREE successful terminal ingest attempts without cursor/read-ACK
/// progress. Failed revisions are unreadable; pending/running never age into stalled.
/// IDs and attempt history are process-local: after restart, three FRESH
/// attempts are required. A retained ID with no row is NO INFORMATION, never
/// progress or an attempt. The separate three-cadence poll watchdog is unchanged.
const STALL_PASSES: u32 = 3;

#[derive(Debug)]
pub(crate) struct PollHealth {
    snapshot: ConversationsStatus,
    observed: Instant,
    cadence: Duration,
    ingests: BTreeMap<&'static str, ConversationIngestReceipt>,
    no_content: HashMap<PathBuf, NoContentAck>,
}

impl PollHealth {
    pub(crate) fn new(every_ticks: u32) -> Self {
        Self {
            snapshot: ConversationsStatus {
                state: if every_ticks == 0 {
                    ConversationState::Disabled
                } else {
                    ConversationState::Flowing
                },
                state_reason: Some(
                    if every_ticks == 0 {
                        "conversation polling is disabled"
                    } else {
                        "awaiting the first conversation poll"
                    }
                    .to_owned(),
                ),
                last_poll_at: None,
                harnesses: [Harness::Claude, Harness::Omp]
                    .into_iter()
                    .map(|harness| ConversationHarnessStatus {
                        harness: harness.to_string(),
                        tracked: 0,
                        behind: 0,
                        unreadable: 0,
                        newest_ingest_at: None,
                        newest_ingest: None,
                    })
                    .collect(),
            },
            observed: Instant::now(),
            cadence: Duration::from_secs(u64::from(every_ticks) * 5),
            ingests: BTreeMap::new(),
            no_content: HashMap::new(),
        }
    }

    pub(crate) fn report(&self, now: Instant) -> ConversationsStatus {
        let mut report = self.snapshot.clone();
        for (harness, receipt) in &self.ingests {
            let index = report
                .harnesses
                .iter()
                .position(|row| row.harness == *harness)
                .unwrap_or_else(|| {
                    // Manual sources can report spend without being polled.
                    report.harnesses.push(ConversationHarnessStatus {
                        harness: (*harness).to_owned(),
                        tracked: 0,
                        behind: 0,
                        unreadable: 0,
                        newest_ingest_at: None,
                        newest_ingest: None,
                    });
                    report.harnesses.len() - 1
                });
            report.harnesses[index].newest_ingest_at = Some(receipt.at.clone());
            report.harnesses[index].newest_ingest = Some(receipt.clone());
        }
        if report.state != ConversationState::Disabled
            && now.saturating_duration_since(self.observed)
                >= self.cadence.saturating_mul(STALL_PASSES)
        {
            report.state = ConversationState::Stalled;
            report.state_reason =
                Some("no successful conversation poll within three cadences".to_owned());
        }
        report
    }

    pub(crate) fn record_ingest(&mut self, harness: Harness, receipt: ConversationIngestReceipt) {
        self.ingests.insert(harness.as_str(), receipt);
    }

    pub(crate) fn acknowledge_no_content(&mut self, path: PathBuf, ack: NoContentAck) {
        self.no_content.insert(path, ack);
    }

    fn record(&mut self, cfg: PollConfig, stats: &PollStats) {
        self.observed = Instant::now();
        self.cadence = Duration::from_secs(u64::from(cfg.every_ticks) * 5);
        self.snapshot = ConversationsStatus {
            state: if stats.stalled {
                ConversationState::Stalled
            } else {
                ConversationState::Flowing
            },
            state_reason: if stats.stalled {
                Some(format!(
                    "three completed ingest attempts without read progress: {} behind, {} in flight, {} outcomes unavailable, {} unreadable",
                    stats.behind, stats.in_flight, stats.unknown, stats.unreadable
                ))
            } else if stats.behind > 0 {
                Some(format!(
                    "catching up: {} behind, {} in flight, {} outcomes unavailable, {} unreadable; {} submitted this pass",
                    stats.behind, stats.in_flight, stats.unknown, stats.unreadable, stats.enqueued
                ))
            } else if stats.unreadable > 0 {
                Some(format!(
                    "{} unreadable; waiting for a file change",
                    stats.unreadable
                ))
            } else {
                None
            },
            last_poll_at: Some(crate::wiring::now()),
            harnesses: stats.harnesses.clone(),
        };
    }
}

/// Runs the existing readers by enqueueing requests, never by running jobs.
///
/// Each due pass prioritizes (1) known files changed since the last scan and
/// (2) files awaiting their first post-boot service opportunity, newest mtime
/// first within each tier. A probe/submission consumes New priority; waiting
/// under the cap does not. At least one slot remains for rotating cold backlog.
/// AC-0008: a 13 MB live OMP session must not wait behind 100 stale Claude
/// files. Path order provides backlog rotation, never a live priority signal.
///
/// # Snap-in recipe
///
/// `[indexing] conversation_poll_ticks = 12` polls every minute; `0` disables.
/// `conversation_lookback_days = 14` limits only cursorless files. Inject the
/// native-store home, never FS3_CONFIG_DIR, and construct:
/// ```ignore
/// let poller = ConvoPoller::new(state.clone(), home, PollConfig {
///     every_ticks: state.config.indexing.conversation_poll_ticks,
///     lookback_days: state.config.indexing.conversation_lookback_days,
///     ..PollConfig::default() // at most 10 requests/pass, six-second stagger
/// });
/// reconcilers.push(Box::new(poller));
/// ```
/// Boot omits the disabled reconciler; AppState still reports disabled.
pub struct ConvoPoller {
    state: AppState,
    home: PathBuf,
    cfg: PollConfig,
    ticks: u32,
    folders: HashMap<PathBuf, CachedFolder>,
    after: Option<PathBuf>,
    lag: HashMap<PathBuf, SubmittedRead>,
    /// Process-local negative read ACKs, invalidated like NoContentAck by the
    /// attempted file revision, never by the unrelated cwd-probe cache.
    unreadable: HashMap<PathBuf, FileStamp>,
    observed: Option<HashMap<PathBuf, ObservedFile>>,
    ack_epoch: Instant,
}

impl ConvoPoller {
    #[must_use]
    pub fn new(state: AppState, home: PathBuf, cfg: PollConfig) -> Self {
        Self {
            state,
            home,
            ticks: cfg.every_ticks.saturating_sub(1),
            cfg,
            folders: HashMap::new(),
            after: None,
            lag: HashMap::new(),
            unreadable: HashMap::new(),
            observed: None,
            ack_epoch: Instant::now(),
        }
    }

    fn due(&mut self) -> bool {
        if self.cfg.every_ticks == 0 {
            return false;
        }
        self.ticks = self.ticks.saturating_add(1);
        if self.ticks < self.cfg.every_ticks {
            return false;
        }
        self.ticks = 0;
        true
    }

    async fn folder<F>(&mut self, path: &Path, stamp: FileStamp, read: F) -> Result<Option<PathBuf>>
    where
        F: FnOnce(&Path) -> Option<PathBuf> + Send + 'static,
    {
        match self.folders.get(path) {
            Some(CachedFolder::Found(folder)) => return Ok(Some(folder.clone())),
            Some(CachedFolder::Missing(previous)) if *previous == stamp => {
                tracing::debug!(path = %path.display(), "unchanged cwd miss; skipping read");
                return Ok(None);
            }
            _ => {}
        }
        let previously_missing = matches!(self.folders.get(path), Some(CachedFolder::Missing(_)));
        let owned = path.to_owned();
        let folder = tokio::task::spawn_blocking(move || read(&owned)).await?;
        if let Some(folder) = &folder {
            self.folders
                .insert(path.to_owned(), CachedFolder::Found(folder.clone()));
        } else {
            self.folders
                .insert(path.to_owned(), CachedFolder::Missing(stamp));
            if previously_missing {
                tracing::debug!(path = %path.display(), "changed session still has no readable recorded cwd");
            } else {
                tracing::warn!(path = %path.display(), "session has no readable recorded cwd; waiting for a file change");
            }
        }
        Ok(folder)
    }

    async fn poll(&mut self) -> Result<PollStats> {
        let home = self.home.clone();
        // Discovery stats files, not transcript contents. Read cwd only for the
        // capped selection, so an old archive cannot turn boot into a full read.
        let sessions = tokio::task::spawn_blocking(move || discover(&home)).await??;
        let mut observed: HashMap<_, _> = sessions
            .iter()
            .flat_map(|session| &session.files)
            .map(|file| {
                let new_pending = self.observed.as_ref().is_some_and(|previous| {
                    previous
                        .get(&file.file.path)
                        .is_none_or(|seen| seen.new_pending)
                });
                (
                    file.file.path.clone(),
                    ObservedFile {
                        stamp: file.stamp,
                        new_pending,
                    },
                )
            })
            .collect();
        self.unreadable
            .retain(|path, stamp| observed.get(path).is_some_and(|file| file.stamp == *stamp));
        let acknowledgments: HashMap<_, _> = self
            .state
            .conversations
            .read()
            .await
            .no_content
            .iter()
            .filter(|(_, ack)| ack.read_started >= self.ack_epoch)
            .map(|(path, ack)| (path.clone(), *ack))
            .collect();
        // Read retained job IDs BEFORE any enqueue upsert can replace an outcome.
        let job_ids: Vec<i64> = self.lag.values().map(|pending| pending.job_id).collect();
        let outcomes: HashMap<_, _> = fs3_store::ingest_job_outcomes(&self.state.db, &job_ids)
            .await?
            .into_iter()
            .map(|outcome| (outcome.id, outcome))
            .collect();
        let mut submitted_ids = HashSet::new();
        let now = SystemTime::now();
        let window = Duration::from_secs(u64::from(self.cfg.lookback_days) * 86_400);
        let mut stats = PollStats::default();
        let mut candidates = Vec::new();
        let mut lag = HashMap::new();
        let mut current_cursors = HashMap::new();
        for harness in [Harness::Claude, Harness::Omp] {
            let mut summary = ConversationHarnessStatus {
                harness: harness.to_string(),
                tracked: 0,
                behind: 0,
                unreadable: 0,
                newest_ingest_at: None,
                newest_ingest: None,
            };
            let ids: Vec<&str> = sessions
                .iter()
                .filter(|session| session.harness == harness)
                .flat_map(|session| {
                    session
                        .files
                        .iter()
                        .map(|file| file.file.session_id.as_str())
                })
                .collect();
            let cursors = ingest_cursors::load_cursors(&self.state.db, harness, &ids).await?;
            for session in sessions.iter().filter(|session| session.harness == harness) {
                let eligible = |file: &FileSnapshot| {
                    is_eligible(file, cursors.get(&file.file.session_id), now, window)
                };
                if !session.files.iter().any(eligible) {
                    stats.skipped += 1;
                    continue;
                }
                summary.tracked += 1;
                let main = &session.files[0];
                if matches!(self.folders.get(&main.file.path), Some(CachedFolder::Missing(stamp)) if *stamp == main.stamp)
                {
                    tracing::debug!(path = %main.file.path.display(), "unchanged cwd miss; skipping read");
                    stats.skipped += 1;
                    continue;
                }
                for file in session.files.iter().filter(|file| eligible(file)) {
                    let progress = cursors.get(&file.file.session_id);
                    let ack = acknowledgments.get(&file.file.path);
                    if !behind(file, progress, ack) {
                        self.unreadable.remove(&file.file.path);
                    }
                    if ack.is_some_and(|ack| ack.stamp == file.stamp) {
                        observed
                            .get_mut(&file.file.path)
                            .expect("observed file")
                            .new_pending = false;
                    }
                    if behind(file, progress, ack)
                        && let Some(previous) = self.lag.get(&file.file.path)
                        && previous.cursor.as_ref() == progress
                        && !ack.is_some_and(|ack| {
                            ack.read_started >= previous.submitted_at || ack.stamp == previous.stamp
                        })
                    {
                        if outcomes
                            .get(&previous.job_id)
                            .is_some_and(|row| row.state == "failed")
                        {
                            // A file may have changed since submission. A failure
                            // acknowledges only that attempted revision, not the
                            // newer bytes discovered by this poll.
                            self.unreadable
                                .insert(file.file.path.clone(), previous.stamp);
                            continue;
                        }
                        let mut pending = previous.clone();
                        pending.observe(outcomes.get(&pending.job_id));
                        lag.insert(file.file.path.clone(), pending);
                    }
                }
                if session.files.iter().any(|file| {
                    eligible(file) && self.unreadable.get(&file.file.path) == Some(&file.stamp)
                }) {
                    summary.unreadable += 1;
                }
                if session.files.iter().any(|file| {
                    eligible(file)
                        && self.unreadable.get(&file.file.path) != Some(&file.stamp)
                        && behind(
                            file,
                            cursors.get(&file.file.session_id),
                            acknowledgments.get(&file.file.path),
                        )
                }) {
                    summary.behind += 1;
                    let changed = session
                        .files
                        .iter()
                        .any(|file| match self.observed.as_ref() {
                            None => cursors.contains_key(&file.file.session_id),
                            Some(previous) => previous
                                .get(&file.file.path)
                                .is_some_and(|seen| seen.stamp != file.stamp),
                        });
                    let fresh = session
                        .files
                        .iter()
                        .any(|file| observed[&file.file.path].new_pending);
                    let priority = if changed {
                        Priority::Live
                    } else if fresh {
                        Priority::New
                    } else {
                        Priority::Backlog
                    };
                    candidates.push(Candidate {
                        session,
                        priority,
                        newest: session
                            .files
                            .iter()
                            .map(|file| file.stamp.modified)
                            .max()
                            .expect("a session has its main file"),
                    });
                }
            }
            stats.harnesses.push(summary);
            current_cursors.insert(harness, cursors);
        }
        candidates.sort_by(|left, right| {
            left.priority.cmp(&right.priority).then_with(|| {
                if left.priority == Priority::Backlog {
                    left.session.files[0]
                        .file
                        .path
                        .cmp(&right.session.files[0].file.path)
                } else {
                    right.newest.cmp(&left.newest)
                }
            })
        });
        let backlog_start =
            candidates.partition_point(|candidate| candidate.priority != Priority::Backlog);
        let cap = self.cfg.max_per_pass.get();
        let priority_count = backlog_start.min(cap - usize::from(backlog_start < candidates.len()));
        if let Some(after) = &self.after {
            let backlog = &mut candidates[backlog_start..];
            let start =
                backlog.partition_point(|candidate| candidate.session.files[0].file.path <= *after);
            backlog.rotate_left(start);
        }
        // Pending is not completion: rotate the cold tier even while earlier
        // jobs remain pending, without allowing live arrivals to consume it all.
        let (prioritized, backlog) = candidates.split_at(backlog_start);
        for (index, candidate) in prioritized
            .iter()
            .take(priority_count)
            .chain(backlog)
            .take(cap)
            .enumerate()
        {
            let session = candidate.session;
            let path = &session.files[0].file.path;
            if index >= priority_count {
                self.after = Some(path.clone());
            }
            let Some(folder) = self.folder(path, session.files[0].stamp, cwd_of).await? else {
                for file in &session.files {
                    observed
                        .get_mut(&file.file.path)
                        .expect("observed file")
                        .new_pending = false;
                }
                stats.skipped += 1;
                continue;
            };
            let delay = self
                .cfg
                .effective_stagger()
                .saturating_mul(u32::try_from(stats.enqueued).unwrap_or(u32::MAX));
            let submitted_at = Instant::now();
            let queued = submit_after_at_home(
                &self.state,
                &session.request(&folder),
                delay,
                self.home.clone(),
                if session.harness == Harness::Omp {
                    path.parent()
                } else {
                    None
                },
            )
            .await
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
            submitted_ids.insert(queued.job_id);
            let cursors = &current_cursors[&session.harness];
            for file in &session.files {
                observed
                    .get_mut(&file.file.path)
                    .expect("observed file")
                    .new_pending = false;
                let cursor = cursors.get(&file.file.session_id);
                if is_eligible(file, cursor, now, window)
                    && self.unreadable.get(&file.file.path) != Some(&file.stamp)
                    && behind(file, cursor, acknowledgments.get(&file.file.path))
                {
                    let pending =
                        lag.entry(file.file.path.clone())
                            .or_insert_with(|| SubmittedRead {
                                cursor: cursor.cloned(),
                                stamp: file.stamp,
                                submitted_at,
                                job_id: queued.job_id,
                                no_progress_attempts: 0,
                                last_outcome: None,
                            });
                    pending.resubmitted(queued.job_id, file.stamp, submitted_at);
                }
            }
            stats.enqueued += 1;
        }
        stats.behind = stats.harnesses.iter().map(|summary| summary.behind).sum();
        stats.unreadable = stats
            .harnesses
            .iter()
            .map(|summary| summary.unreadable)
            .sum();
        let retained_ids: HashSet<_> = lag.values().map(|pending| pending.job_id).collect();
        stats.in_flight = retained_ids
            .iter()
            .filter(|id| {
                submitted_ids.contains(id)
                    || outcomes
                        .get(id)
                        .is_some_and(|row| matches!(row.state.as_str(), "pending" | "running"))
            })
            .count();
        stats.unknown = retained_ids
            .iter()
            .filter(|id| !submitted_ids.contains(id) && !outcomes.contains_key(id))
            .count();
        stats.stalled = lag
            .values()
            .any(|pending| pending.no_progress_attempts >= STALL_PASSES);
        self.lag = lag;
        self.observed = Some(observed);
        self.state
            .conversations
            .write()
            .await
            .record(self.cfg, &stats);
        Ok(stats)
    }
}

#[async_trait::async_trait]
impl Reconcile for ConvoPoller {
    fn name(&self) -> &'static str {
        "conversations"
    }

    async fn reconcile(&mut self) -> Result<Pass> {
        if self.cfg.every_ticks == 0 {
            let mut health = self.state.conversations.write().await;
            health.snapshot.state = ConversationState::Disabled;
            health.snapshot.state_reason = Some("conversation polling is disabled".to_owned());
        }
        if !self.due() {
            return Ok(Pass::QUIET);
        }
        let stats = self.poll().await?;
        if stats.enqueued == 0 && stats.behind == 0 {
            tracing::debug!(
                enqueued = stats.enqueued,
                behind = stats.behind,
                unreadable = stats.unreadable,
                skipped = stats.skipped,
                "polled native conversations"
            );
        } else {
            tracing::info!(
                enqueued = stats.enqueued,
                behind = stats.behind,
                unreadable = stats.unreadable,
                skipped = stats.skipped,
                "polled native conversations"
            );
        }
        Ok(Pass::changed(stats.enqueued))
    }
}

#[derive(Debug, Default)]
struct PollStats {
    enqueued: usize,
    behind: usize,
    unreadable: usize,
    skipped: usize,
    in_flight: usize,
    unknown: usize,
    harnesses: Vec<ConversationHarnessStatus>,
    stalled: bool,
}

fn behind(file: &FileSnapshot, cursor: Option<&SourceCursor>, ack: Option<&NoContentAck>) -> bool {
    if ack.is_some_and(|ack| ack.stamp == file.stamp) {
        return false;
    }
    match cursor {
        Some(SourceCursor::ByteOffset {
            device,
            inode,
            offset,
        }) => file.stamp.size != *offset || file.stamp.identity != (*device, *inode),
        // A foreign cursor is left to the reader to diagnose, never mistaken
        // for an up-to-date native file.
        _ => true,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Priority {
    Live,
    New,
    Backlog,
}

struct Candidate<'a> {
    session: &'a Session,
    priority: Priority,
    newest: SystemTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileStamp {
    size: u64,
    modified: SystemTime,
    identity: (u64, u64),
}

impl FileStamp {
    pub(crate) fn read(path: &Path) -> std::io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        Ok(Self {
            size: metadata.len(),
            modified: metadata.modified()?,
            identity: fs3_providers::conversation_sources::tail::identity(&metadata),
        })
    }

    pub(crate) fn identifies(self, cursor: &SourceCursor) -> bool {
        matches!(cursor, SourceCursor::ByteOffset { device, inode, .. }
            if self.identity == (*device, *inode))
    }
}

/// A successful read found no records in this exact file revision. This is
/// process-local, NOT a durable cursor: an unchanged headerless file costs at
/// most one ingest job per poller lifetime. A new ConvoPoller ignores older
/// ACKs and may read it once again. Stamp changes always make it eligible again;
/// a partial-line cursor is never advanced to file size to manufacture quiet.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NoContentAck {
    pub(crate) stamp: FileStamp,
    pub(crate) read_started: Instant,
}

enum CachedFolder {
    Found(PathBuf),
    Missing(FileStamp),
}

#[derive(Clone)]
struct SubmittedRead {
    cursor: Option<SourceCursor>,
    stamp: FileStamp,
    submitted_at: Instant,
    job_id: i64,
    no_progress_attempts: u32,
    last_outcome: Option<(i64, i32)>,
}

impl SubmittedRead {
    fn observe(&mut self, outcome: Option<&fs3_store::IngestJobOutcome>) {
        let Some(outcome) = outcome else { return }; // Retention is not progress.
        if outcome.id != self.job_id || outcome.state != "done" {
            return;
        }
        let revision = (outcome.id, outcome.attempts);
        if self.last_outcome != Some(revision) {
            self.no_progress_attempts = self.no_progress_attempts.saturating_add(1);
            self.last_outcome = Some(revision);
        }
    }

    fn resubmitted(&mut self, job_id: i64, stamp: FileStamp, submitted_at: Instant) {
        // A newly submitted attempt must not inherit a consumed terminal revision.
        if self.last_outcome.is_some_and(|(id, _)| id == job_id) {
            self.last_outcome = None;
        }
        self.job_id = job_id;
        self.stamp = stamp;
        self.submitted_at = submitted_at;
    }
}

fn is_eligible(
    file: &FileSnapshot,
    cursor: Option<&SourceCursor>,
    now: SystemTime,
    window: Duration,
) -> bool {
    cursor.is_some() || now.duration_since(file.stamp.modified).unwrap_or_default() <= window
}

struct ObservedFile {
    stamp: FileStamp,
    new_pending: bool,
}

struct FileSnapshot {
    file: SessionFile,
    stamp: FileStamp,
}

struct Session {
    harness: Harness,
    id: String,
    files: Vec<FileSnapshot>,
}

impl Session {
    fn request(&self, folder: &Path) -> IngestRequest {
        IngestRequest {
            pij_id: None,
            session_id: Some(self.id.clone()),
            harness: Some(self.harness.to_string()),
            folder: Some(folder.to_string_lossy().into_owned()),
        }
    }
}

fn entries(path: &Path) -> Result<Vec<std::fs::DirEntry>> {
    match std::fs::read_dir(path) {
        Ok(entries) => entries
            .collect::<std::io::Result<Vec<_>>>()
            .with_context(|| format!("listing {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error).with_context(|| format!("listing {}", path.display())),
    }
}

fn discover(home: &Path) -> Result<Vec<Session>> {
    let mut sessions = Vec::new();
    for (harness, root) in [
        (Harness::Claude, home.join(".claude/projects")),
        (Harness::Omp, home.join(".omp/agent/sessions")),
    ] {
        for directory in entries(&root)? {
            if !directory.file_type()?.is_dir() {
                continue;
            }
            for entry in entries(&directory.path())? {
                if !entry.file_type()?.is_file() {
                    continue;
                }
                let path = entry.path();
                if path
                    .extension()
                    .is_none_or(|extension| extension != "jsonl")
                {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                    continue;
                };
                let id = match harness {
                    Harness::Claude => stem,
                    Harness::Omp => match stem.rsplit_once('_') {
                        Some((_, id)) if !id.is_empty() => id,
                        _ => continue,
                    },
                    _ => unreachable!(),
                }
                .to_owned();
                let files = if harness == Harness::Claude {
                    ClaudeSource::new(directory.path()).resolve(&IngestInput::Native {
                        session_id: id.clone(),
                        harness,
                        folder: PathBuf::new(),
                    })?
                } else {
                    vec![SessionFile {
                        path,
                        session_id: id.clone(),
                        parent_session_id: None,
                        kind: SessionKind::Main,
                        harness,
                    }]
                };
                let files = files
                    .into_iter()
                    .map(|file| {
                        let metadata = std::fs::metadata(&file.path)?;
                        Ok(FileSnapshot {
                            file,
                            stamp: FileStamp {
                                size: metadata.len(),
                                modified: metadata.modified()?,
                                identity: fs3_providers::conversation_sources::tail::identity(
                                    &metadata,
                                ),
                            },
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                sessions.push(Session { harness, id, files });
            }
        }
    }
    sessions.sort_by(|left, right| left.files[0].file.path.cmp(&right.files[0].file.path));
    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs3_core::{Config, DatabaseConfig};
    use fs3_testkit::FreshDatabase;

    #[cfg(unix)]
    #[test]
    fn omp_manual_slug_matches_native_home_temp_and_absolute_rules() {
        let home = tempfile::tempdir().unwrap();
        let workspace = home.path().join("project-with-hyphens:tag");
        std::fs::create_dir(&workspace).unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let cases = [
            (home.path(), "-".to_owned()),
            (workspace.as_path(), "-project-with-hyphens-tag".to_owned()),
            (
                temporary.path(),
                format!(
                    "-tmp-{}",
                    temporary.path().file_name().unwrap().to_string_lossy()
                ),
            ),
            (
                Path::new("/fs3-abs-project-with-hyphens"),
                "--fs3-abs-project-with-hyphens--".to_owned(),
            ),
        ];
        for (folder, expected) in cases {
            assert_eq!(
                fs3_providers::conversation_sources::omp::session_slug(folder, home.path()),
                expected,
                "OMP 18.1.14 native naming rule for {}",
                folder.display()
            );
            assert_eq!(
                crate::convo_ingest::workspace_slug(Harness::Omp, folder, home.path()),
                expected
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn omp_missing_cwd_preserves_lexical_suffix() {
        let scratch = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(scratch.path()).unwrap();
        let home = root.join("home");
        std::fs::create_dir(&home).unwrap();
        let missing = root.join("never-created/../missing-project");
        assert!(std::fs::canonicalize(&missing).is_err());
        let expected = format!(
            "-tmp-{}-missing-project",
            root.file_name().unwrap().to_string_lossy()
        );
        assert_eq!(
            fs3_providers::conversation_sources::omp::session_slug(&missing, &home),
            expected
        );
        assert_eq!(
            crate::convo_ingest::workspace_slug(Harness::Omp, &missing, &home),
            expected
        );
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn convo_poll_omp_tmp_uses_observed_directory_and_manual_slug() {
        let config = tempfile::tempdir().unwrap();
        let (database, state) = stack().await;
        std::fs::write(
            config.path().join("config.toml"),
            format!("[database]\nurl = {:?}\n", database.url()),
        )
        .unwrap();
        let sealed = fs3_testkit::sealed(
            &std::env::current_exe().unwrap(),
            config.path(),
            fs3_testkit::TestDatabase::FromConfigFile,
        );
        let home = PathBuf::from(
            sealed
                .get_envs()
                .find(|(key, _)| *key == "HOME")
                .unwrap()
                .1
                .unwrap(),
        );
        let workspace = tempfile::Builder::new()
            .prefix("fs3-omp-tmp-")
            .tempdir_in("/tmp")
            .unwrap();
        let physical = std::fs::canonicalize(workspace.path()).unwrap();
        assert!(physical.starts_with("/private/tmp"));
        let root = home.join(".omp/agent/sessions");
        let native = root.join(format!(
            "--{}--",
            physical
                .to_string_lossy()
                .trim_start_matches('/')
                .replace('/', "-")
        ));
        std::fs::create_dir_all(&native).unwrap();
        let filename = "2026-09-09T00-00-00_tmp-session.jsonl";
        let path = native.join(filename);
        let header = serde_json::json!({"type":"session", "cwd":workspace.path()});
        let turn = |id: &str| serde_json::json!({"type":"message","id":id,"timestamp":"2026-09-09T00:00:00Z","message":{"role":"user","content":[{"type":"text","text":"a real tmp workspace turn"}]}});
        let initial = format!("{header}\n{}\n", turn("first"));
        std::fs::write(&path, &initial).unwrap();
        let old = root.join(format!(
            "-{}",
            workspace
                .path()
                .to_string_lossy()
                .trim_start_matches('/')
                .replace('/', "-")
        ));
        assert!(!old.exists(), "the old cwd-derived directory never existed");
        let mut poller = ConvoPoller::new(state.clone(), home.clone(), PollConfig::default());
        assert_eq!(poller.poll().await.unwrap().enqueued, 1);
        let payload: serde_json::Value =
            sqlx::query_scalar("SELECT payload FROM jobs WHERE kind='ingest_session'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(
            payload["omp_session_dir"],
            native.to_string_lossy().as_ref()
        );
        assert_eq!(run_ingests(&state, &home).await.len(), 1);
        assert_eq!(poller.poll().await.unwrap().behind, 0);

        // Explicit session-dir layouts need not match today's derived slug.
        // Moving the native directory proves the queue actually carries it.
        let explicit = root.join("explicit-session-directory");
        std::fs::rename(&native, &explicit).unwrap();
        std::fs::write(
            explicit.join(filename),
            format!("{initial}{}\n", turn("second")),
        )
        .unwrap();
        assert_eq!(poller.poll().await.unwrap().enqueued, 1);
        assert_eq!(run_ingests(&state, &home).await.len(), 1);
        assert_eq!(poller.poll().await.unwrap().behind, 0);
        let delivered = fs3_store::conversation_delivery(
            &state.db,
            &crate::convo_ingest::conversation_guid(Harness::Omp, "tmp-session"),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(delivered.turns, 2);

        let manual_dir = root.join(crate::convo_ingest::workspace_slug(
            Harness::Omp,
            workspace.path(),
            &home,
        ));
        std::fs::create_dir_all(&manual_dir).unwrap();
        std::fs::write(
            manual_dir.join("2026-09-09T00-00-00_manual-tmp.jsonl"),
            format!("{header}\n{}\n", turn("manual")),
        )
        .unwrap();
        let request = IngestRequest {
            pij_id: None,
            session_id: Some("manual-tmp".to_owned()),
            harness: Some("omp".to_owned()),
            folder: Some(workspace.path().to_string_lossy().into_owned()),
        };
        let report = crate::convo_ingest::ingest_at_home(&state, &request, home.clone())
            .await
            .unwrap();
        assert_eq!(report.turns_new, 1);
        crate::convo_ingest::verify_at_home(
            &state,
            &crate::convo_ingest::VerifyRequest {
                pij_id: None,
                session_id: request.session_id,
                harness: request.harness,
            },
            home,
        )
        .await
        .unwrap();
        database.destroy(state.db.clone()).await;
    }

    #[tokio::test]
    async fn convo_poll_failed_revision_is_unreadable_until_it_changes() {
        let home = tempfile::tempdir().unwrap();
        let bad = claude_session(home.path(), "a-unreadable");
        let good = claude_session(home.path(), "z-progressing");
        let (database, state) = stack().await;
        let mut poller =
            ConvoPoller::new(state.clone(), home.path().to_owned(), PollConfig::default());
        assert_eq!(poller.poll().await.unwrap().enqueued, 2);
        let job = fs3_store::claim_job(&state.db, &[crate::convo_ingest::INGEST_SESSION])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job.payload["session_id"], "a-unreadable");
        fs3_store::fail_job(&state.db, job.id, "fixture unreadable source", false)
            .await
            .unwrap();
        assert_eq!(run_ingests(&state, home.path()).await.len(), 1);
        for _ in 0..6 {
            let stats = poller.poll().await.unwrap();
            assert_eq!(
                (
                    stats.enqueued,
                    stats.behind,
                    stats.in_flight,
                    stats.unreadable
                ),
                (0, 0, 0, 1)
            );
            let status = state.conversations.read().await.report(Instant::now());
            assert_eq!(status.state, ConversationState::Flowing);
            assert_eq!(status.harnesses[0].unreadable, 1);
            assert!(status.state_reason.unwrap().contains("1 unreadable"));
            let row = fs3_store::ingest_job_outcomes(&state.db, &[job.id])
                .await
                .unwrap()
                .remove(0);
            assert_eq!(
                row.state, "failed",
                "classify failure before submission can revive this row"
            );
            assert_eq!(
                row.attempts, 1,
                "no enqueue may erase the observed failed attempt"
            );
        }
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(good)
            .unwrap()
            .write_all(claude_record(&home.path().join("workspace"), "good-append").as_bytes())
            .unwrap();
        let stats = poller.poll().await.unwrap();
        assert_eq!((stats.enqueued, stats.behind, stats.unreadable), (1, 1, 1));
        assert!(!stats.stalled);
        assert_eq!(run_ingests(&state, home.path()).await.len(), 1);
        std::fs::OpenOptions::new()
            .append(true)
            .open(bad)
            .unwrap()
            .write_all(claude_record(&home.path().join("workspace"), "recovered").as_bytes())
            .unwrap();
        let stats = poller.poll().await.unwrap();
        assert_eq!((stats.enqueued, stats.behind, stats.unreadable), (1, 1, 0));
        let revived = fs3_store::ingest_job_outcomes(&state.db, &[job.id])
            .await
            .unwrap()
            .remove(0);
        assert_eq!(
            revived.state, "pending",
            "a revision change revives the same failed job"
        );
        assert_eq!(
            revived.attempts, 0,
            "the revival resets attempts only after failure was classified"
        );
        assert_eq!(run_ingests(&state, home.path()).await.len(), 1);
        let completed = fs3_store::ingest_job_outcomes(&state.db, &[job.id])
            .await
            .unwrap()
            .remove(0);
        assert_eq!(
            completed.state, "done",
            "the previously failed row is actually ingested and completed"
        );
        let stats = poller.poll().await.unwrap();
        assert_eq!((stats.enqueued, stats.behind, stats.unreadable), (0, 0, 0));
        database.destroy(state.db.clone()).await;
    }

    #[tokio::test]
    async fn convo_poll_failed_old_attempt_does_not_ack_newer_bytes() {
        let home = tempfile::tempdir().unwrap();
        let path = claude_session(home.path(), "changing");
        let (database, state) = stack().await;
        let mut poller =
            ConvoPoller::new(state.clone(), home.path().to_owned(), PollConfig::default());
        assert_eq!(poller.poll().await.unwrap().enqueued, 1);
        let job = fs3_store::claim_job(&state.db, &[crate::convo_ingest::INGEST_SESSION])
            .await
            .unwrap()
            .unwrap();
        fs3_store::fail_job(&state.db, job.id, "old revision failed", false)
            .await
            .unwrap();
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(path)
            .unwrap()
            .write_all(claude_record(&home.path().join("workspace"), "newer").as_bytes())
            .unwrap();
        let stats = poller.poll().await.unwrap();
        assert_eq!((stats.enqueued, stats.behind, stats.unreadable), (1, 1, 0));
        assert!(!stats.stalled);
        assert_eq!(run_ingests(&state, home.path()).await.len(), 1);
        assert_eq!(poller.poll().await.unwrap().behind, 0);
        database.destroy(state.db.clone()).await;
    }

    #[test]
    fn terminal_attempt_revisions_ignore_time_collisions_and_missing_rows_are_unknown() {
        let home = tempfile::tempdir().unwrap();
        let path = claude_session(home.path(), "revision");
        let stamp = FileStamp::read(&path).unwrap();
        let mut pending = SubmittedRead {
            cursor: None,
            stamp,
            submitted_at: Instant::now(),
            job_id: 42,
            no_progress_attempts: 0,
            last_outcome: None,
        };
        let mut row = fs3_store::IngestJobOutcome {
            id: 42,
            dedupe_key: "fixture".to_owned(),
            state: "done".to_owned(),
            updated_at: "2026-09-06T00:00:00.000000Z".to_owned(),
            attempts: 1,
        };
        pending.observe(Some(&row));
        pending.observe(Some(&row));
        assert_eq!(
            pending.no_progress_attempts, 1,
            "same terminal revision counted once"
        );
        row.attempts = 2; // Same ID, state and updated_at, distinct completed attempt.
        pending.observe(Some(&row));
        assert_eq!(pending.no_progress_attempts, 2);
        pending.observe(None);
        assert_eq!(
            pending.no_progress_attempts, 2,
            "retention cannot clear or increment a counter"
        );
        row.state = "failed".to_owned();
        row.attempts = 3;
        pending.observe(Some(&row));
        assert_eq!(
            pending.no_progress_attempts, 2,
            "failed attempts are unreadable, not successful no-progress attempts"
        );
        row.state = "running".to_owned();
        row.attempts = 4;
        pending.observe(Some(&row));
        row.state = "pending".to_owned();
        pending.observe(Some(&row));
        assert_eq!(
            pending.no_progress_attempts, 2,
            "in-flight states are not completed attempts"
        );
    }

    #[tokio::test]
    async fn convo_poll_cwd_negative_cache_reads_once_per_stamp_and_demotes_new() {
        use std::sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        };
        struct Capture(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Capture {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let output = Arc::new(Mutex::new(Vec::new()));
        let writer = Arc::clone(&output);
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_max_level(tracing::Level::DEBUG)
            .with_writer(move || Capture(Arc::clone(&writer)))
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        let home = tempfile::tempdir().unwrap();
        let (database, state) = stack().await;
        let mut poller =
            ConvoPoller::new(state.clone(), home.path().to_owned(), PollConfig::default());
        poller.poll().await.unwrap(); // Empty boot snapshot, so the next file is New.
        let path = claude_session(home.path(), "missing-cwd");
        std::fs::write(&path, "{\"type\":\"system\"}\n").unwrap();
        let first = poller.poll().await.unwrap();
        assert_eq!((first.enqueued, first.behind), (0, 1));
        assert!(!poller.observed.as_ref().unwrap()[&path].new_pending);
        let reads = Arc::new(AtomicUsize::new(0));
        let reader = {
            let reads = Arc::clone(&reads);
            move |path: &Path| {
                reads.fetch_add(1, Ordering::SeqCst);
                cwd_of(path)
            }
        };
        let stamp = FileStamp::read(&path).unwrap();
        for _ in 0..6 {
            assert_eq!(
                poller.folder(&path, stamp, reader.clone()).await.unwrap(),
                None
            );
            let stats = poller.poll().await.unwrap();
            assert_eq!((stats.enqueued, stats.behind), (0, 0));
        }
        assert_eq!(
            reads.load(Ordering::SeqCst),
            0,
            "cached miss never reopens the file"
        );
        std::fs::write(&path, "{\"type\":\"cost-state\"}\n").unwrap();
        let changed = FileStamp::read(&path).unwrap();
        assert_eq!(
            poller.folder(&path, changed, reader.clone()).await.unwrap(),
            None
        );
        assert_eq!(
            poller.folder(&path, changed, reader.clone()).await.unwrap(),
            None
        );
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        let logs = String::from_utf8(output.lock().unwrap().clone()).unwrap();
        assert_eq!(
            logs.lines()
                .filter(|line| line.contains("WARN") && line.contains("no readable recorded cwd"))
                .count(),
            1
        );
        assert!(
            logs.lines()
                .any(|line| line.contains("DEBUG") && line.contains("unchanged cwd miss"))
        );
        let folder = home.path().join("workspace");
        std::fs::write(&path, format!("{}\n", serde_json::json!({"cwd":folder}))).unwrap();
        assert_eq!(
            poller
                .folder(&path, FileStamp::read(&path).unwrap(), reader)
                .await
                .unwrap(),
            Some(folder)
        );
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        database.destroy(state.db.clone()).await;
    }

    fn zero_record_file(home: &Path, harness: Harness, id: &str) -> PathBuf {
        let folder = home.join("workspace");
        std::fs::create_dir_all(&folder).unwrap();
        let path = if harness == Harness::Claude {
            claude_session(home, id)
        } else {
            let path = home
                .join(".omp/agent/sessions/-workspace")
                .join(format!("2026-09-06_{id}.jsonl"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            path
        };
        let text = if harness == Harness::Claude {
            format!(
                "{}\n{}\n{}\n",
                serde_json::json!({"type":"mode"}),
                serde_json::json!({"type":"system","cwd":folder}),
                serde_json::json!({"type":"cost-state"})
            )
        } else {
            format!(
                "{}\n{}\n",
                serde_json::json!({"type":"session","id":id,"cwd":folder,"timestamp":"2026-09-06T00:00:00Z"}),
                serde_json::json!({"type":"model_change","id":"model","timestamp":"2026-09-06T00:00:00Z"})
            )
        };
        std::fs::write(&path, text).unwrap();
        path
    }

    #[tokio::test]
    async fn convo_poll_zero_records_ack_once_per_lifetime_and_quiet_from_second_pass() {
        let home = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for index in 0..5 {
            paths.push(zero_record_file(
                home.path(),
                Harness::Claude,
                &format!("zero-claude-{index}"),
            ));
        }
        for index in 0..2 {
            paths.push(zero_record_file(
                home.path(),
                Harness::Omp,
                &format!("zero-omp-{index}"),
            ));
        }
        let (database, state) = stack().await;
        let cfg = PollConfig {
            every_ticks: 1,
            ..PollConfig::default()
        };
        for lifetime in 1..=2 {
            // SAME store and AppState, but a new poller must not inherit old ACKs.
            let mut poller = ConvoPoller::new(state.clone(), home.path().to_owned(), cfg);
            assert_eq!(poller.reconcile().await.unwrap().changed, 7);
            assert_eq!(run_ingests(&state, home.path()).await.len(), 7);
            for pass in 2..=6 {
                let stats = poller.poll().await.unwrap();
                assert_eq!(stats.enqueued, 0, "lifetime {lifetime}, pass {pass}");
                assert_eq!(
                    stats.behind, 0,
                    "an actual zero-record ingest ACK is not behind"
                );
                let status = state.conversations.read().await.report(Instant::now());
                assert_eq!(status.state, ConversationState::Flowing);
                assert!(status.harnesses.iter().all(|row| row.behind == 0));
                let jobs: Vec<(String, i64)> =
                    sqlx::query_as("SELECT kind,count(*) FROM jobs GROUP BY kind ORDER BY kind")
                        .fetch_all(&state.db)
                        .await
                        .unwrap();
                assert_eq!(jobs, vec![("ingest_session".to_owned(), lifetime * 7)]);
            }
            let rows: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM conversations),(SELECT count(*) FROM turns),(SELECT count(*) FROM ingest_cursors)").fetch_one(&state.db).await.unwrap();
            assert_eq!(
                rows,
                (0, 0, 0),
                "no invented header, timestamp, turns, or durable cursor"
            );
            if lifetime == 2 {
                use std::io::Write;
                std::fs::OpenOptions::new()
                    .append(true)
                    .open(&paths[0])
                    .unwrap()
                    .write_all(
                        claude_record(&home.path().join("workspace"), "first-real-turn").as_bytes(),
                    )
                    .unwrap();
                assert_eq!(
                    poller.reconcile().await.unwrap().changed,
                    1,
                    "stamp change invalidates the ACK"
                );
                assert_eq!(run_ingests(&state, home.path()).await.len(), 1);
                assert_eq!(poller.poll().await.unwrap().behind, 0);
                let delivery = fs3_store::conversation_delivery(
                    &state.db,
                    &crate::convo_ingest::conversation_guid(Harness::Claude, "zero-claude-0"),
                )
                .await
                .unwrap()
                .unwrap();
                assert_eq!(delivery.turns, 1);
            }
        }
        database.destroy(state.db.clone()).await;
    }

    #[tokio::test]
    async fn convo_poll_zero_records_checkpoint_existing_header_and_preserve_torn_line() {
        let home = tempfile::tempdir().unwrap();
        let path = zero_record_file(home.path(), Harness::Claude, "existing-empty");
        let (database, state) = stack().await;
        let guid = crate::convo_ingest::conversation_guid(Harness::Claude, "existing-empty");
        fs3_store::upsert_conversation(
            &state.db,
            &fs3_core::Conversation {
                guid: guid.clone(),
                repo_identity: None,
                worktree: None,
                base_sha: None,
                title: None,
                started_at: "2026-09-01T00:00:00Z".to_owned(),
                parent: None,
            },
        )
        .await
        .unwrap();
        let mut poller =
            ConvoPoller::new(state.clone(), home.path().to_owned(), PollConfig::default());
        assert_eq!(poller.poll().await.unwrap().enqueued, 1);
        run_ingests(&state, home.path()).await;
        let complete_offset = std::fs::metadata(&path).unwrap().len();
        let cursor = ingest_cursors::load_cursor(&state.db, Harness::Claude, "existing-empty")
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(cursor, SourceCursor::ByteOffset { offset, .. } if offset == complete_offset)
        );
        assert_eq!(poller.poll().await.unwrap().behind, 0);

        let turn = claude_record(&home.path().join("workspace"), "completed-torn-turn");
        let cut = turn.len() - 2; // Leave the final closing brace and newline unwritten.
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&turn.as_bytes()[..cut])
            .unwrap();
        assert_eq!(poller.poll().await.unwrap().enqueued, 1);
        run_ingests(&state, home.path()).await;
        let cursor = ingest_cursors::load_cursor(&state.db, Harness::Claude, "existing-empty")
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(cursor, SourceCursor::ByteOffset { offset, .. } if offset == complete_offset)
        );
        assert!(complete_offset < std::fs::metadata(&path).unwrap().len());
        let quiet = poller.poll().await.unwrap();
        assert_eq!(
            (quiet.enqueued, quiet.behind),
            (0, 0),
            "unchanged incomplete bytes are acknowledged, never skipped by the cursor"
        );
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&turn.as_bytes()[cut..])
            .unwrap();
        assert_eq!(poller.poll().await.unwrap().enqueued, 1);
        run_ingests(&state, home.path()).await;
        assert_eq!(
            fs3_store::conversation_delivery(&state.db, &guid)
                .await
                .unwrap()
                .unwrap()
                .turns,
            1
        );
        assert_eq!(poller.poll().await.unwrap().behind, 0);
        database.destroy(state.db.clone()).await;
    }

    #[tokio::test]
    async fn convo_poll_11_and_12_cold_files_remain_flowing_for_six_healthy_passes() {
        for total in [11, 12] {
            let home = tempfile::tempdir().unwrap();
            for index in 0..total {
                claude_session(home.path(), &format!("healthy-{index}"));
            }
            let (database, state) = stack().await;
            let mut poller =
                ConvoPoller::new(state.clone(), home.path().to_owned(), PollConfig::default());
            for pass in 1..=6 {
                let stats = poller.poll().await.unwrap();
                assert!(stats.in_flight <= 10, "in-flight work obeys the cap");
                assert_eq!(
                    state
                        .conversations
                        .read()
                        .await
                        .report(Instant::now())
                        .state,
                    ConversationState::Flowing,
                    "{total} files, pass {pass}"
                );
                run_ingests(&state, home.path()).await;
            }
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind='ingest_session'")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(count, total);
            assert_eq!(poller.poll().await.unwrap().behind, 0);
            database.destroy(state.db.clone()).await;
        }
    }

    #[tokio::test]
    async fn convo_poll_priority_serves_tracked_omp_and_fresh_claude_ahead_of_100_backlog_files() {
        let home = tempfile::tempdir().unwrap();
        let cold: Vec<String> = (0..100)
            .map(|index| format!("backlog-{index:03}"))
            .collect();
        for id in &cold {
            claude_session(home.path(), id);
        }
        let folder = home.path().join("workspace");
        let omp = home
            .path()
            .join(".omp/agent/sessions/-workspace/2026-09-06_tracked-omp.jsonl");
        std::fs::create_dir_all(omp.parent().unwrap()).unwrap();
        let omp_turn = |id: &str| serde_json::json!({"type":"message","id":id,"timestamp":"2026-09-06T00:00:00Z","message":{"role":"user","content":[{"type":"text","text":"live OMP turn"}]}});
        std::fs::write(
            &omp,
            format!(
                "{}\n{}\n",
                serde_json::json!({"type":"session","cwd":folder}),
                omp_turn("omp-first")
            ),
        )
        .unwrap();
        let (database, state) = stack().await;
        crate::convo_ingest::ingest_at_home(
            &state,
            &IngestRequest {
                pij_id: None,
                harness: Some("omp".to_owned()),
                session_id: Some("tracked-omp".to_owned()),
                folder: Some(folder.to_string_lossy().into_owned()),
            },
            home.path().to_owned(),
        )
        .await
        .unwrap();
        let mut poller = ConvoPoller::new(
            state.clone(),
            home.path().to_owned(),
            PollConfig {
                every_ticks: 1,
                ..PollConfig::default()
            },
        );
        assert_eq!(poller.reconcile().await.unwrap().changed, 10);
        let before: i64 = sqlx::query_scalar("SELECT coalesce(max(id),0) FROM jobs")
            .fetch_one(&state.db)
            .await
            .unwrap();

        use std::io::Write;
        writeln!(
            std::fs::OpenOptions::new().append(true).open(&omp).unwrap(),
            "{}",
            omp_turn("omp-appended")
        )
        .unwrap();
        let fresh_folder = home.path().join("aaa-fresh-project");
        std::fs::create_dir_all(&fresh_folder).unwrap();
        let fresh = home
            .path()
            .join(".claude/projects")
            .join(crate::convo_ingest::workspace_slug(
                Harness::Claude,
                &fresh_folder,
                home.path(),
            ))
            .join("fresh-claude.jsonl");
        std::fs::create_dir_all(fresh.parent().unwrap()).unwrap();
        std::fs::write(&fresh, claude_record(&fresh_folder, "fresh-turn")).unwrap();
        assert!(
            fresh < *poller.after.as_ref().unwrap(),
            "fresh path sorts BEFORE the old rotation cursor"
        );
        assert_eq!(poller.reconcile().await.unwrap().changed, 10);
        let queued: Vec<(String,String)> = sqlx::query_as("SELECT payload->>'harness',payload->>'session_id' FROM jobs WHERE kind = 'ingest_session' AND id > $1 ORDER BY id")
            .bind(before).fetch_all(&state.db).await.unwrap();
        assert_eq!(
            queued[0],
            ("omp".to_owned(), "tracked-omp".to_owned()),
            "live OMP must be enqueued on the NEXT due pass"
        );
        assert_eq!(
            queued[1],
            ("claude".to_owned(), "fresh-claude".to_owned()),
            "fresh Claude must ignore path order on the NEXT due pass"
        );
        assert!(queued.iter().any(|(_, id)| id.starts_with("backlog-")));
        run_ingests(&state, home.path()).await;
        for _ in 0..20 {
            assert_eq!(
                state
                    .conversations
                    .read()
                    .await
                    .report(Instant::now())
                    .state,
                ConversationState::Flowing
            );
            if poller.reconcile().await.unwrap().changed == 0 {
                break;
            }
            run_ingests(&state, home.path()).await;
        }
        assert_eq!(poller.poll().await.unwrap().behind, 0);
        let cold_ids: Vec<&str> = cold.iter().map(String::as_str).collect();
        assert_eq!(
            ingest_cursors::load_cursors(&state.db, Harness::Claude, &cold_ids)
                .await
                .unwrap()
                .len(),
            100
        );
        let omp_delivery = fs3_store::conversation_delivery(
            &state.db,
            &crate::convo_ingest::conversation_guid(Harness::Omp, "tracked-omp"),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(omp_delivery.turns, 2);
        database.destroy(state.db.clone()).await;
    }

    #[tokio::test]
    async fn convo_poll_priority_reserves_a_backlog_slot_during_continuous_new_arrivals() {
        let home = tempfile::tempdir().unwrap();
        for index in 0..12 {
            claude_session(home.path(), &format!("cold-{index:02}"));
        }
        let (database, state) = stack().await;
        let cfg = PollConfig {
            every_ticks: 1,
            max_per_pass: std::num::NonZeroUsize::new(3).unwrap(),
            ..PollConfig::default()
        };
        let mut poller = ConvoPoller::new(state.clone(), home.path().to_owned(), cfg);
        poller.reconcile().await.unwrap();
        run_ingests(&state, home.path()).await;
        let base_time = SystemTime::now() - Duration::from_secs(100);
        for round in 0..3 {
            let before: i64 = sqlx::query_scalar("SELECT coalesce(max(id),0) FROM jobs")
                .fetch_one(&state.db)
                .await
                .unwrap();
            for index in 0..3 {
                let path = claude_session(home.path(), &format!("new-{round}-{index}"));
                std::fs::File::open(path)
                    .unwrap()
                    .set_times(
                        std::fs::FileTimes::new()
                            .set_modified(base_time + Duration::from_secs(round * 10 + index * 2)),
                    )
                    .unwrap();
            }
            assert_eq!(poller.reconcile().await.unwrap().changed, 3);
            let queued: Vec<String> = sqlx::query_scalar("SELECT payload->>'session_id' FROM jobs WHERE kind='ingest_session' AND id > $1 ORDER BY id")
                .bind(before).fetch_all(&state.db).await.unwrap();
            assert_eq!(
                queued[0],
                format!("new-{round}-2"),
                "newest mtime wins, not path order"
            );
            assert_eq!(queued[1], format!("new-{round}-1"));
            assert!(
                queued[2].starts_with("cold-"),
                "a continuous live stream cannot starve the reserved backlog slot"
            );
            run_ingests(&state, home.path()).await;
        }
        for _ in 0..20 {
            if poller.reconcile().await.unwrap().changed == 0 {
                break;
            }
            run_ingests(&state, home.path()).await;
        }
        assert_eq!(
            poller.poll().await.unwrap().behind,
            0,
            "new and cold work both drain"
        );
        database.destroy(state.db.clone()).await;
    }

    async fn stack() -> (FreshDatabase, AppState) {
        let database = FreshDatabase::create("convo-poll").await;
        let state = AppState::from_config(Config {
            database: DatabaseConfig {
                url: database.url(),
            },
            ..Config::default()
        })
        .expect("fake stack");
        fs3_store::migrate(&state.db)
            .await
            .expect("migrate isolated database");
        (database, state)
    }

    fn claude_record(folder: &Path, ordinal: &str) -> String {
        format!(
            "{}\n",
            serde_json::json!({
                "type": "user", "uuid": ordinal, "cwd": folder,
                "timestamp": "2026-09-01T00:00:00Z",
                "message": { "role": "user", "content": "a user turn" }
            })
        )
    }

    fn claude_session(home: &Path, id: &str) -> PathBuf {
        let folder = home.join("workspace");
        std::fs::create_dir_all(&folder).unwrap();
        let root = home
            .join(".claude/projects")
            .join(crate::convo_ingest::workspace_slug(
                Harness::Claude,
                &folder,
                home,
            ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(format!("{id}.jsonl"));
        std::fs::write(&path, claude_record(&folder, "main-turn")).unwrap();
        path
    }

    async fn run_ingests(state: &AppState, home: &Path) -> Vec<IngestRequest> {
        let jobs: Vec<(i64, serde_json::Value)> = sqlx::query_as(
            "SELECT id, payload FROM jobs WHERE kind = 'ingest_session' AND state = 'pending' ORDER BY id"
        ).fetch_all(&state.db).await.unwrap();
        let mut requests = Vec::new();
        for (id, payload) in jobs {
            let (request, directory) = crate::convo_ingest::decode_job(payload).unwrap();
            crate::convo_ingest::ingest_with_directory_at_home(
                state,
                &request,
                home.to_owned(),
                directory,
            )
            .await
            .expect("existing ingest pipeline");
            sqlx::query("UPDATE jobs SET state = 'done' WHERE id = $1")
                .bind(id)
                .execute(&state.db)
                .await
                .unwrap();
            requests.push(request);
        }
        requests
    }

    #[tokio::test]
    async fn convo_poll_fast_cadence_delivers_append_within_cadence_plus_spacing() {
        let home = tempfile::tempdir().unwrap();
        let first = claude_session(home.path(), "a-active");
        let appended = claude_session(home.path(), "b-appended");
        let (database, state) = stack().await;
        let cfg = PollConfig {
            every_ticks: 1,
            ..PollConfig::default()
        };
        assert_eq!(cfg.effective_stagger(), Duration::from_millis(500));
        assert_eq!(
            PollConfig::default().effective_stagger(),
            Duration::from_secs(6)
        );
        let mut poller = ConvoPoller::new(state.clone(), home.path().to_owned(), cfg);
        poller.poll().await.unwrap();
        run_ingests(&state, home.path()).await;
        use std::io::Write;
        for path in [appended, first] {
            std::fs::OpenOptions::new()
                .append(true)
                .open(path)
                .unwrap()
                .write_all(
                    claude_record(&home.path().join("workspace"), "appended-turn").as_bytes(),
                )
                .unwrap();
        }
        let started = Instant::now();
        let bound = Duration::from_secs(5) + cfg.effective_stagger();
        assert_eq!(poller.poll().await.unwrap().enqueued, 2);
        // Unlike run_ingests, this worker claims only ACTUALLY eligible jobs;
        // the second appended session must wait for its recorded not_before.
        tokio::time::timeout(bound, async {
            loop {
                if let Some(job) =
                    fs3_store::claim_job(&state.db, &[crate::convo_ingest::INGEST_SESSION])
                        .await
                        .unwrap()
                {
                    let request: IngestRequest = serde_json::from_value(job.payload).unwrap();
                    crate::convo_ingest::ingest_at_home(&state, &request, home.path().to_owned())
                        .await
                        .unwrap();
                    sqlx::query("UPDATE jobs SET state = 'done' WHERE id = $1")
                        .bind(job.id)
                        .execute(&state.db)
                        .await
                        .unwrap();
                    if request.session_id.as_deref() == Some("b-appended") {
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("appended file ingested within 5s cadence + 0.5s spacing");
        assert!(started.elapsed() <= bound);
        let delivery = fs3_store::conversation_delivery(
            &state.db,
            &crate::convo_ingest::conversation_guid(Harness::Claude, "b-appended"),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(delivery.turns, 2);
        database.destroy(state.db.clone()).await;
    }

    #[tokio::test]
    async fn convo_poll_first_unchanged_and_sidecar_append_use_durable_file_cursors() {
        let home = tempfile::tempdir().unwrap();
        let main = claude_session(home.path(), "claude-main");
        let folder = home.path().join("workspace");
        let sidecar = main
            .parent()
            .unwrap()
            .join("claude-main/subagents/agent-child.jsonl");
        std::fs::create_dir_all(sidecar.parent().unwrap()).unwrap();
        std::fs::write(&sidecar, claude_record(&folder, "child-turn")).unwrap();
        let omp_root = home.path().join(".omp/agent/sessions/-workspace");
        std::fs::create_dir_all(&omp_root).unwrap();
        std::fs::write(omp_root.join("2026-09-01T00-00-00_omp-main.jsonl"), format!("{}\n{}\n",
            serde_json::json!({"type":"session", "cwd":folder}),
            serde_json::json!({"type":"message","id":"omp-turn","timestamp":"2026-09-01T00:00:00Z","message":{"role":"user","content":[{"type":"text","text":"omp turn"}]}})
        )).unwrap();
        let (database, state) = stack().await;
        let mut poller = ConvoPoller::new(
            state.clone(),
            home.path().to_owned(),
            PollConfig {
                every_ticks: 1,
                ..PollConfig::default()
            },
        );
        assert_eq!(poller.reconcile().await.unwrap().changed, 2);
        let keys: Vec<String> = sqlx::query_scalar(
            "SELECT dedupe_key FROM jobs WHERE kind = 'ingest_session' ORDER BY dedupe_key",
        )
        .fetch_all(&state.db)
        .await
        .unwrap();
        assert_eq!(
            keys,
            [
                format!("ingest:claude/claude-main@{}", folder.display()),
                format!("ingest:omp/omp-main@{}", folder.display())
            ]
        );
        assert_eq!(run_ingests(&state, home.path()).await.len(), 2);
        assert_eq!(poller.reconcile().await.unwrap(), Pass::QUIET);
        let cursors = ingest_cursors::load_cursors(
            &state.db,
            Harness::Claude,
            &["claude-main", "agent-child", "missing"],
        )
        .await
        .unwrap();
        assert_eq!(cursors.len(), 2);
        assert!(
            cursors
                .values()
                .all(|cursor| matches!(cursor, SourceCursor::ByteOffset { .. }))
        );
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&sidecar)
            .unwrap()
            .write_all(claude_record(&folder, "child-appended").as_bytes())
            .unwrap();
        assert_eq!(poller.reconcile().await.unwrap().changed, 1);
        let requests = run_ingests(&state, home.path()).await;
        assert_eq!(requests[0].session_id.as_deref(), Some("claude-main"));
        assert_eq!(poller.reconcile().await.unwrap(), Pass::QUIET);
        database.destroy(state.db.clone()).await;
    }

    #[tokio::test]
    async fn convo_poll_disabled_never_discovers_or_connects() {
        let home = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.indexing.conversation_poll_ticks = 0;
        let state = AppState::from_config(config).unwrap();
        assert_eq!(
            state
                .conversations
                .read()
                .await
                .report(Instant::now())
                .state,
            ConversationState::Disabled,
            "disabled before the first poll"
        );
        let mut poller = ConvoPoller::new(
            state,
            home.path().to_owned(),
            PollConfig {
                every_ticks: 0,
                ..PollConfig::default()
            },
        );
        assert_eq!(poller.reconcile().await.unwrap(), Pass::QUIET);
    }

    #[tokio::test]
    async fn convo_poll_identity_ignores_a_phantom_recorded_id_and_caches_actual_cwd() {
        let home = tempfile::tempdir().unwrap();
        let path = claude_session(home.path(), "file-session");
        std::fs::write(home.path().join("pij-sessions.json"),
            r#"[{"pij_id":"pij-phantom","harness":"claude","harness_session":"recorded-but-never-written"}]"#).unwrap();
        let (database, state) = stack().await;
        let mut poller = ConvoPoller::new(
            state.clone(),
            home.path().to_owned(),
            PollConfig {
                every_ticks: 1,
                ..PollConfig::default()
            },
        );
        assert_eq!(poller.reconcile().await.unwrap().changed, 1);
        let requests = run_ingests(&state, home.path()).await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].session_id.as_deref(), Some("file-session"));
        assert_eq!(requests[0].pij_id, None);
        assert_eq!(
            requests[0].folder.as_deref(),
            home.path().join("workspace").to_str()
        );

        // A positive cwd lookup is cached per path; no transcript reread on a poll.
        std::fs::write(&path, "metadata temporarily unavailable\n").unwrap();
        assert_eq!(
            poller
                .folder(&path, FileStamp::read(&path).unwrap(), cwd_of)
                .await
                .unwrap(),
            Some(home.path().join("workspace"))
        );
        database.destroy(state.db.clone()).await;
    }

    #[tokio::test]
    async fn convo_poll_cold_start_healthy_catch_up_stays_flowing() {
        cold_start_state_proof(true).await;
    }

    #[tokio::test]
    async fn convo_poll_cold_start_pending_queue_is_fair_and_stays_flowing() {
        cold_start_state_proof(false).await;
    }

    async fn cold_start_state_proof(healthy: bool) {
        let home = tempfile::tempdir().unwrap();
        for index in 0..100 {
            claude_session(home.path(), &format!("session-{index:02}"));
        }
        let old = claude_session(home.path(), "old-cursorless");
        std::fs::File::open(&old)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(SystemTime::UNIX_EPOCH))
            .unwrap();
        let (database, state) = stack().await;
        let mut poller =
            ConvoPoller::new(state.clone(), home.path().to_owned(), PollConfig::default());
        let mut total = 0;
        for pass in 1..=10 {
            let stats = poller.poll().await.unwrap();
            assert_eq!(stats.enqueued, 10, "every pass obeys the cold-start cap");
            if healthy {
                assert_eq!(
                    (stats.behind, stats.in_flight),
                    (100 - usize::try_from((pass - 1) * 10).unwrap(), 10),
                    "backlog falls only as cursors advance; in-flight stays capped"
                );
            } else {
                assert_eq!(
                    (stats.behind, stats.in_flight),
                    (100, usize::try_from(pass * 10).unwrap()),
                    "pending work remains behind while unique in-flight jobs grow"
                );
            }
            let status = state.conversations.read().await.report(Instant::now());
            assert_eq!(status.state, ConversationState::Flowing, "pass {pass}");
            assert!(
                status
                    .state_reason
                    .unwrap()
                    .contains(&format!("{} in flight", stats.in_flight))
            );
            assert_eq!(
                stats.skipped, 1,
                "old cursorless file is visible as skipped"
            );
            total += stats.enqueued;
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'ingest_session'")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(
                count,
                pass * 10,
                "later keys progress without duplicate jobs, whether prior jobs are pending or done"
            );
            if healthy {
                assert_eq!(run_ingests(&state, home.path()).await.len(), 10);
            }
        }
        assert_eq!(total, 100);
        let delays: Vec<f64> = sqlx::query_scalar("SELECT extract(epoch FROM (not_before - updated_at))::float8 FROM jobs WHERE kind = 'ingest_session' ORDER BY id").fetch_all(&state.db).await.unwrap();
        for (index, delay) in delays.into_iter().enumerate() {
            assert_eq!(
                delay,
                ((index % 10) * 6) as f64,
                "stagger recorded in actual queue eligibility"
            );
        }
        assert!(
            !poller.folders.contains_key(&old),
            "lookback skips before reading any transcript content"
        );
        if !healthy {
            assert_eq!(run_ingests(&state, home.path()).await.len(), 100);
        }
        let stats = poller.poll().await.unwrap();
        assert_eq!(stats.enqueued, 0);
        assert_eq!(stats.behind, 0);
        assert_eq!(stats.skipped, 1);
        assert_eq!(
            state
                .conversations
                .read()
                .await
                .report(Instant::now())
                .state,
            ConversationState::Flowing
        );

        // Once tracked, age never excludes an appended file from polling.
        let tracked = old.parent().unwrap().join("session-00.jsonl");
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&tracked)
            .unwrap()
            .write_all(
                claude_record(&home.path().join("workspace"), "aged-but-new-turn").as_bytes(),
            )
            .unwrap();
        std::fs::File::open(&tracked)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(SystemTime::UNIX_EPOCH))
            .unwrap();
        assert_eq!(poller.poll().await.unwrap().enqueued, 1);
        database.destroy(state.db.clone()).await;
    }

    #[test]
    fn convo_poll_health_stale_boundary_and_boot_pending_are_explicit() {
        let home = tempfile::tempdir().unwrap();
        assert!(home.path().is_dir());
        let health = PollHealth::new(12);
        let pending = health.report(health.observed);
        assert_eq!(pending.state, ConversationState::Flowing);
        assert_eq!(pending.last_poll_at, None);
        assert!(pending.state_reason.unwrap().contains("awaiting"));
        assert_eq!(
            health
                .report(health.observed + Duration::from_secs(179))
                .state,
            ConversationState::Flowing
        );
        assert_eq!(
            health
                .report(health.observed + Duration::from_secs(180))
                .state,
            ConversationState::Stalled
        );
        let disabled = PollHealth::new(0);
        assert_eq!(
            disabled
                .report(disabled.observed + Duration::from_secs(3600))
                .state,
            ConversationState::Disabled
        );
    }

    #[tokio::test]
    async fn convo_poll_health_counts_done_attempts_and_recovers() {
        let home = tempfile::tempdir().unwrap();
        let path = claude_session(home.path(), "stalled-session");
        let (database, state) = stack().await;
        let cfg = PollConfig {
            every_ticks: 1,
            ..PollConfig::default()
        };
        let mut poller = ConvoPoller::new(state.clone(), home.path().to_owned(), cfg);
        poller.reconcile().await.unwrap();
        // Pending, then running, are in flight even over many due passes.
        for _ in 0..6 {
            poller.reconcile().await.unwrap();
            let status = state.conversations.read().await.report(Instant::now());
            assert_eq!(status.state, ConversationState::Flowing);
            assert!(status.state_reason.unwrap().contains("1 in flight"));
        }
        let running = fs3_store::claim_job(&state.db, &[crate::convo_ingest::INGEST_SESSION])
            .await
            .unwrap()
            .unwrap();
        for _ in 0..6 {
            poller.reconcile().await.unwrap();
            assert_eq!(poller.lag[&path].no_progress_attempts, 0);
            assert_eq!(
                state
                    .conversations
                    .read()
                    .await
                    .report(Instant::now())
                    .state,
                ConversationState::Flowing
            );
        }
        fs3_store::complete_job(&state.db, running.id)
            .await
            .unwrap();
        poller.reconcile().await.unwrap();
        assert_eq!(poller.lag[&path].no_progress_attempts, 1);
        for attempt in 2..=3 {
            let job = fs3_store::claim_job(&state.db, &[crate::convo_ingest::INGEST_SESSION])
                .await
                .unwrap()
                .unwrap();
            fs3_store::complete_job(&state.db, job.id).await.unwrap();
            sqlx::query("UPDATE jobs SET updated_at='2026-09-06T00:00:00Z' WHERE id=$1")
                .bind(job.id)
                .execute(&state.db)
                .await
                .unwrap();
            poller.reconcile().await.unwrap();
            assert_eq!(
                poller.lag[&path].no_progress_attempts, attempt,
                "each successful terminal attempt without read progress is counted"
            );
            let row = fs3_store::ingest_job_outcomes(&state.db, &[job.id])
                .await
                .unwrap()
                .remove(0);
            assert_eq!(row.state, "done");
            assert_eq!(
                row.attempts, 1,
                "completed rows remain terminal; the next attempt has its own job"
            );
            let status = state.conversations.read().await.report(Instant::now());
            assert_eq!(
                status.state,
                if attempt == 3 {
                    ConversationState::Stalled
                } else {
                    ConversationState::Flowing
                }
            );
            assert!(status.state_reason.unwrap().contains("1 in flight"));
        }
        // A new poller has no retained IDs/attempts, even over the same store.
        drop(poller);
        let mut restarted = ConvoPoller::new(state.clone(), home.path().to_owned(), cfg);
        restarted.reconcile().await.unwrap();
        assert_eq!(restarted.lag[&path].no_progress_attempts, 0);
        assert_eq!(
            state
                .conversations
                .read()
                .await
                .report(Instant::now())
                .state,
            ConversationState::Flowing
        );
        run_ingests(&state, home.path()).await;
        restarted.reconcile().await.unwrap();
        let status = state.conversations.read().await.report(Instant::now());
        assert_eq!(status.state, ConversationState::Flowing);
        assert_eq!(status.harnesses[0].behind, 0);
        assert!(status.harnesses[0].newest_ingest_at.is_some());
        database.destroy(state.db.clone()).await;
    }

    #[tokio::test]
    async fn convo_verify_no_file_is_distinct_from_present_but_unindexed() {
        use crate::convo_ingest::{VerifyRequest, verify_at_home};
        let (database, state) = stack().await;
        for harness in ["claude", "omp"] {
            let home = tempfile::tempdir().unwrap();
            let request = IngestRequest {
                pij_id: None,
                session_id: Some("lazy-session".to_owned()),
                harness: Some(harness.to_owned()),
                folder: None,
            };
            let verify = VerifyRequest {
                pij_id: None,
                session_id: request.session_id.clone(),
                harness: request.harness.clone(),
            };
            let failure = verify_at_home(&state, &verify, home.path().to_owned())
                .await
                .unwrap_err();
            assert_eq!(failure.code, "FS3-E-QUERY-CONVERSATION-NO-SESSION-FILE");
            assert!(failure.fix.contains("not persisted"));
            let failure = submit_after_at_home(
                &state,
                &request,
                Duration::ZERO,
                home.path().to_owned(),
                None,
            )
            .await
            .unwrap_err();
            assert_eq!(failure.code, "FS3-E-QUERY-CONVERSATION-NO-SESSION-FILE");
            let pending: i64 =
                sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'ingest_session'")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(pending, if harness == "claude" { 0 } else { 1 });
            let path = if harness == "claude" {
                home.path()
                    .join(".claude/projects/-workspace/lazy-session.jsonl")
            } else {
                home.path()
                    .join(".omp/agent/sessions/-workspace/2026-09-06_lazy-session.jsonl")
            };
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            // No cwd or content: presence must not depend on successful parsing.
            std::fs::write(&path, []).unwrap();
            let failure = verify_at_home(&state, &verify, home.path().to_owned())
                .await
                .unwrap_err();
            assert_eq!(failure.code, "FS3-E-QUERY-CONVERSATION-NOT-FOUND");
            assert!(
                submit_after_at_home(
                    &state,
                    &request,
                    Duration::ZERO,
                    home.path().to_owned(),
                    None
                )
                .await
                .unwrap()
                .response
                .accepted
            );
        }
        database.destroy(state.db.clone()).await;
    }

    #[tokio::test]
    async fn convo_poll_and_boot_commit_seam_submit_share_one_live_job() {
        let home = tempfile::tempdir().unwrap();
        claude_session(home.path(), "shared-session");
        let (database, state) = stack().await;
        let mut poller =
            ConvoPoller::new(state.clone(), home.path().to_owned(), PollConfig::default());
        assert_eq!(poller.reconcile().await.unwrap().changed, 1);
        let request = IngestRequest {
            pij_id: None,
            session_id: Some("shared-session".to_owned()),
            harness: Some("claude".to_owned()),
            folder: Some(home.path().join("workspace").to_string_lossy().into_owned()),
        };
        let seam = submit_after_at_home(
            &state,
            &request,
            Duration::ZERO,
            home.path().to_owned(),
            None,
        )
        .await
        .unwrap();
        let keys: Vec<String> =
            sqlx::query_scalar("SELECT dedupe_key FROM jobs WHERE kind = 'ingest_session'")
                .fetch_all(&state.db)
                .await
                .unwrap();
        assert_eq!(keys, [seam.response.dedupe_key]);
        assert_eq!(run_ingests(&state, home.path()).await.len(), 1);
        assert_eq!(poller.poll().await.unwrap().enqueued, 0);
        database.destroy(state.db.clone()).await;
    }

    #[tokio::test]
    async fn convo_poll_truncated_file_enqueues_then_uses_existing_cursor_reset() {
        let home = tempfile::tempdir().unwrap();
        let path = claude_session(home.path(), "truncated-session");
        let (database, state) = stack().await;
        let mut poller =
            ConvoPoller::new(state.clone(), home.path().to_owned(), PollConfig::default());
        poller.poll().await.unwrap();
        run_ingests(&state, home.path()).await;
        assert_eq!(
            poller.poll().await.unwrap().enqueued,
            0,
            "same size and identity are quiet"
        );
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(0)
            .unwrap();
        let stats = poller.poll().await.unwrap();
        assert_eq!(stats.enqueued, 1);
        assert_eq!(stats.behind, 1, "truncation is visible as behind");
        run_ingests(&state, home.path()).await;
        assert_eq!(poller.poll().await.unwrap().enqueued, 0);
        let cursor = ingest_cursors::load_cursor(&state.db, Harness::Claude, "truncated-session")
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(cursor, SourceCursor::ByteOffset { offset: 0, .. }));
        database.destroy(state.db.clone()).await;
    }

    // The shared reader's non-Unix identity is (0, 0); only size changes are
    // detectable there. This proves actual inode replacement where supported.
    #[cfg(unix)]
    #[tokio::test]
    async fn convo_poll_same_size_replacement_enqueues_and_same_identity_is_quiet() {
        let home = tempfile::tempdir().unwrap();
        let path = claude_session(home.path(), "replaced-session");
        let (database, state) = stack().await;
        let mut poller =
            ConvoPoller::new(state.clone(), home.path().to_owned(), PollConfig::default());
        poller.poll().await.unwrap();
        run_ingests(&state, home.path()).await;
        assert_eq!(poller.poll().await.unwrap().enqueued, 0);
        let original = std::fs::metadata(&path).unwrap();
        let replacement = path.with_extension("replacement");
        std::fs::copy(&path, &replacement).unwrap();
        std::fs::rename(replacement, &path).unwrap();
        let replaced = std::fs::metadata(&path).unwrap();
        assert_eq!(original.len(), replaced.len());
        assert_ne!(
            fs3_providers::conversation_sources::tail::identity(&original),
            fs3_providers::conversation_sources::tail::identity(&replaced)
        );
        let stats = poller.poll().await.unwrap();
        assert_eq!(
            stats.enqueued, 1,
            "same-size new identity must reach the reader"
        );
        assert_eq!(stats.behind, 1);
        run_ingests(&state, home.path()).await;
        assert_eq!(poller.poll().await.unwrap().enqueued, 0);
        let delivered = fs3_store::conversation_delivery(
            &state.db,
            &crate::convo_ingest::conversation_guid(Harness::Claude, "replaced-session"),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            delivered.turns, 1,
            "existing ledger dedupes the unchanged replacement bytes"
        );
        database.destroy(state.db.clone()).await;
    }
}
