//! The live watcher: registered roots become OS watchers, filesystem events
//! become `scan_file` jobs.
//!
//! Jordan's ask, verbatim: *"the daemon should automatically watch whatever
//! paths are present on boot, and also if I add a path, it should start
//! watching it."* Both fall out of one reconcile pass rather than being two
//! features — the pass compares the `worktrees` table against the watchers
//! that actually exist, so "already there at boot" and "added a second ago"
//! differ only in when the pass happens to run.
//!
//! # Shape
//!
//! One task owns everything, and there is **no mutex anywhere**. The prototype
//! this is lifted from needed one because axum handlers shared its state; here
//! the supervisor is reached only through `&mut self` on its own reconcile
//! pass, so a pass is:
//!
//! 1. diff `worktrees` against live watchers — start what is new, drop what is gone
//! 2. drain the channel `notify` has been filling from its own thread
//! 3. sweep the debouncer for directories that have gone quiet
//! 4. re-list each one and enqueue what actually changed
//!
//! # Two debounces, at two levels, neither redundant
//!
//! The queue already debounces the JOB: `enqueue_job`'s live-dedupe index plus
//! `not_before = GREATEST(existing, new)` collapses a re-fire into the pending
//! row and pushes its deadline out. That is durable, survives a restart, and
//! this module gets it for free by enqueuing through the same call `roots.rs`
//! uses.
//!
//! What the queue cannot decide is **when to pay for a directory walk**, which
//! is what an event actually costs here. So the in-memory debouncer coalesces
//! events into quiet directories, and the enqueue that follows passes
//! `Duration::ZERO` — the wait already happened, exactly as an explicit `add`
//! passes zero.
//!
//! The maximum-age settle lives on this side too, because that is where the
//! starvation hole is reachable: `GREATEST` only ever moves a deadline
//! FORWARD, so the queue-level debounce has the identical never-settles
//! problem with no escape hatch short of changing store SQL.
//!
//! # What this deliberately does not do
//!
//! It never calls `sync_worktree_files`: that function syncs a WHOLE worktree,
//! so handing it one subdirectory's files would reap every path outside that
//! subdirectory. The consequence is that **deletions are not reaped here** —
//! a deleted file keeps its `worktree_files` row until a full walk (`add` or
//! `scan`) runs. See `docs/services/watcher.md`; the periodic full-walk
//! backstop is a named queued decision, not an oversight.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use async_trait::async_trait;
use fs3_parsers::discovery::{self, DiscoverySettings};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::debounce::{Debouncer, Settled, normalize, prune_nested};
use crate::reconcile::{Pass, Reconcile};
use crate::roots::{SCAN_FILE, ScanFileJob};
use crate::wiring::AppState;

/// How much longer than the debounce window a directory may stay pending.
///
/// Six windows — a minute at the shipped ten-second default. Not configurable
/// because nothing has asked to tune it; the number that matters, the window
/// itself, already is (`indexing.debounce_seconds`). What this bounds is the
/// worst case for a directory containing something written continuously: it is
/// re-listed at least this often instead of never.
const MAX_AGE_WINDOWS: u32 = 6;

/// One watched root, and the OS watcher keeping it alive.
struct Watched {
    /// The worktree row this root came from — carried so a settled directory
    /// becomes a job without a second query.
    worktree_id: i64,
    /// The repository identity, for the same reason.
    identity: String,
    /// Dropping this is the unsubscribe. It is held for that and nothing else:
    /// no method is ever called on it again. Ownership IS the lifecycle, which
    /// is why removing a root cannot leak a watcher.
    _watcher: RecommendedWatcher,
}

/// One filesystem event, already attributed to a root.
struct RawEvent {
    root: PathBuf,
    path: PathBuf,
    /// Stamped where the event was RECEIVED rather than where it is processed,
    /// so a backlog between the watcher thread and the next reconcile pass
    /// cannot make a burst look longer than it was.
    at_ms: u64,
}

/// Keeps live watchers matching the `worktrees` table, and turns what they see
/// into queue rows.
pub struct WatcherSupervisor {
    state: AppState,
    started_at: Instant,
    watched: BTreeMap<PathBuf, Watched>,
    debouncer: Debouncer,
    events: mpsc::UnboundedSender<RawEvent>,
    inbox: mpsc::UnboundedReceiver<RawEvent>,
}

impl WatcherSupervisor {
    /// Build a supervisor from the wired state.
    ///
    /// Reads `indexing.debounce_seconds`, which has shipped as configuration
    /// since the config landed and has had no reader until now.
    #[must_use]
    pub fn new(state: AppState) -> Self {
        let window = Duration::from_secs(state.config.indexing.debounce_seconds);
        let window_ms = u64::try_from(window.as_millis()).unwrap_or(u64::MAX);

        // Unbounded on purpose. `notify` hands events to a callback on its own
        // thread, and that callback cannot await; a bounded channel would have
        // to block that thread — which is how the OS is made to DROP events on
        // macOS and Windows — or drop them itself. Unbounded turns the problem
        // into memory pressure, where it is at least visible, and the
        // debouncer collapses a burst to one entry per directory on the next
        // pass regardless of how many events it holds.
        let (events, inbox) = mpsc::unbounded_channel();

        Self {
            state,
            started_at: Instant::now(),
            watched: BTreeMap::new(),
            debouncer: Debouncer::new(window_ms, window_ms * u64::from(MAX_AGE_WINDOWS)),
            events,
            inbox,
        }
    }

    /// Milliseconds since the supervisor started — the one monotonic clock the
    /// pure core is driven by.
    fn now_ms(&self) -> u64 {
        u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Start watching one root, recursively.
    fn start(&mut self, root: &Path, worktree_id: i64, identity: &str) -> Result<()> {
        let sender = self.events.clone();
        let attributed = root.to_path_buf();
        let started_at = self.started_at;

        let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            // Runs on `notify`'s own thread, outside the Tokio runtime: it may
            // not block and may not await, which is what makes an unbounded
            // sender the only channel that fits.
            match result {
                Ok(event) => {
                    let at_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
                    for path in event.paths {
                        // Lexical normalisation only. A Remove event names a
                        // path that is already gone, so `canonicalize` would
                        // fail precisely when the answer is needed.
                        let _ = sender.send(RawEvent {
                            root: attributed.clone(),
                            path: normalize(&path),
                            at_ms,
                        });
                    }
                }
                Err(error) => tracing::warn!(%error, "watcher reported an error"),
            }
        })
        .context("creating an OS file watcher")?;

        watcher
            .watch(root, RecursiveMode::Recursive)
            .with_context(|| format!("watching {}", root.display()))?;

        self.watched.insert(
            root.to_path_buf(),
            Watched {
                worktree_id,
                identity: identity.to_string(),
                _watcher: watcher,
            },
        );
        // The human subject, never the payload: an event line names a path, and
        // job payloads carry indexed source text that has no business in a log.
        tracing::info!(root = %root.display(), worktree_id, "watching");
        Ok(())
    }

    /// Stop watching one root and forget its in-flight work.
    fn stop(&mut self, root: &Path) {
        // Dropping the `Watched` drops its watcher, which unsubscribes.
        if self.watched.remove(root).is_some() {
            let dropped = self.debouncer.forget_root(root);
            tracing::info!(root = %root.display(), dropped_pending = dropped, "no longer watching");
        }
    }

    /// Bring live watchers in line with the `worktrees` table.
    ///
    /// Returns how many watchers changed hands.
    async fn reconcile_roots(&mut self) -> Result<usize> {
        let registered = fs3_store::list_worktrees(&self.state.db)
            .await
            .context("reading registered worktrees")?;
        let desired: Vec<DesiredRoot> = registered
            .into_iter()
            .map(|worktree| DesiredRoot {
                path: PathBuf::from(&worktree.root_path),
                worktree_id: worktree.id,
                identity: worktree.identity,
            })
            .collect();
        let actual: Vec<PathBuf> = self.watched.keys().cloned().collect();

        let plan = diff_roots(&desired, &actual);
        let mut changed = 0;

        for root in &plan.stop {
            self.stop(root);
            changed += 1;
        }
        for root in &plan.start {
            match self.start(&root.path, root.worktree_id, &root.identity) {
                Ok(()) => changed += 1,
                // A root that cannot be watched — deleted from disk, or out of
                // OS watch descriptors — must not stop the pass. The next one
                // tries again, which is the whole point of reconciling.
                Err(error) => tracing::warn!(
                    root = %root.path.display(),
                    %error,
                    "cannot watch this root — retrying on the next pass"
                ),
            }
        }

        Ok(changed)
    }

    /// Move everything `notify` has queued into the debouncer.
    fn absorb_events(&mut self) {
        while let Ok(event) = self.inbox.try_recv() {
            // A root unwatched between the event landing in the channel and
            // being drained here: drop it rather than resurrect the root.
            if !self.watched.contains_key(&event.root) {
                continue;
            }
            self.debouncer
                .observe(&event.root, &event.path, event.at_ms);
        }
    }

    /// Re-list one settled directory and enqueue what changed.
    ///
    /// Returns how many `scan_file` rows this produced.
    async fn relist(&self, settled: &Settled) -> Result<usize> {
        let Some(watched) = self.watched.get(&settled.root) else {
            // Unwatched between the sweep and here.
            return Ok(0);
        };

        let settings = DiscoverySettings::from(&self.state.config.scan);
        let discovered = match discovery::discover(&settled.directory, &settings) {
            Ok(discovered) => discovered,
            // The directory is gone — the everyday case for a delete or a
            // rename, and not an error. Its files are handled by the full-walk
            // backstop; see the module docs.
            Err(discovery::DiscoveryError::NotADirectory(_)) => return Ok(0),
            Err(error) => {
                return Err(anyhow::Error::new(error)
                    .context(format!("re-listing {}", settled.directory.display())));
            }
        };

        // Discovery reports paths relative to the directory it walked; the
        // store keys on paths relative to the WORKTREE ROOT, so they are
        // rebased before anything else touches them.
        let prefix = worktree_relative(&settled.root, &settled.directory);
        let known = fs3_store::worktree_file_map(&self.state.db, watched.worktree_id)
            .await
            .context("reading the worktree's known blobs")?;

        let mut enqueued = 0;
        for file in &discovered.files {
            let relative = join_relative(&prefix, &file.path);
            let absolute = settled.directory.join(&file.path);

            let blob = match fs3_git::blob_id(&absolute) {
                Ok(blob) => blob,
                // Vanished between the walk and the hash: it is a file that is
                // no longer there, and the next pass will agree.
                Err(fs3_git::Error::Io { .. }) => continue,
                Err(error) => return Err(anyhow::Error::new(error).context("hashing a file")),
            };

            // The whole reason over-reporting is free: content keying means an
            // unchanged file enqueues nothing at all, so a directory walk
            // triggered by one edit costs one job, not a directory's worth.
            if known.get(&relative).map(String::as_str) == Some(blob.as_str()) {
                continue;
            }

            let job = ScanFileJob {
                worktree_id: watched.worktree_id,
                identity: watched.identity.clone(),
                path: relative,
                blob: blob.as_str().to_string(),
            };
            fs3_store::enqueue_job(
                &self.state.db,
                SCAN_FILE,
                &job.dedupe_key(),
                &serde_json::to_value(&job).expect("a scan job always serialises"),
                // Zero, like every other enqueue site: the debounce already
                // happened above, and the queue's own not_before still collapses
                // a re-fire that arrives before the runner drains this row.
                Duration::ZERO,
            )
            .await
            .context("enqueuing a scan job")?;
            enqueued += 1;
        }

        Ok(enqueued)
    }
}

#[async_trait]
impl Reconcile for WatcherSupervisor {
    fn name(&self) -> &'static str {
        "watcher"
    }

    async fn reconcile(&mut self) -> Result<Pass> {
        let mut changed = self.reconcile_roots().await?;

        self.absorb_events();

        let now = self.now_ms();
        let settled = prune_nested(self.debouncer.sweep(now));
        for entry in &settled {
            let enqueued = self.relist(entry).await?;
            changed += enqueued;
            if enqueued > 0 {
                tracing::info!(
                    directory = %entry.directory.display(),
                    events = entry.events,
                    enqueued,
                    settled = ?entry.reason,
                    "re-listed a changed directory"
                );
            }
        }

        Ok(Pass::changed(changed))
    }
}

/// A root the `worktrees` table says should be watched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesiredRoot {
    /// The registered root path, already canonical (`add` canonicalises).
    pub path: PathBuf,
    /// The worktree row id.
    pub worktree_id: i64,
    /// The repository identity.
    pub identity: String,
}

/// What one root reconcile pass has to do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RootPlan {
    /// Roots to begin watching.
    pub start: Vec<DesiredRoot>,
    /// Roots to stop watching.
    pub stop: Vec<PathBuf>,
}

impl RootPlan {
    /// Whether this pass has anything to do at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start.is_empty() && self.stop.is_empty()
    }
}

/// Diff what should be watched against what is.
///
/// Pure, so the interesting cases — boot with an empty set, a root added, a
/// root removed, a root whose path was re-registered — are pinned without a
/// database or a filesystem.
///
/// Overlapping roots are NOT resolved here. If a user adds both `~/code` and
/// `~/code/project`, both are watched and an edit under the nested one is seen
/// twice, producing two `scan_file` rows under two different worktree ids.
/// That is duplicated bookkeeping, not duplicated work: both scans key the same
/// content by blob, so the second finds everything already stored. Absorbing
/// the covered root is the better product answer and is a named queued
/// decision (`docs/services/watcher.md`), not something to guess at here.
#[must_use]
pub fn diff_roots(desired: &[DesiredRoot], actual: &[PathBuf]) -> RootPlan {
    RootPlan {
        start: desired
            .iter()
            .filter(|root| !actual.contains(&root.path))
            .cloned()
            .collect(),
        stop: actual
            .iter()
            .filter(|path| !desired.iter().any(|root| &root.path == *path))
            .cloned()
            .collect(),
    }
}

/// The `/`-separated path of `directory` relative to `root`, or empty when
/// they are the same.
///
/// Empty rather than `"."`: it is a PREFIX, and `join_relative` is what turns
/// it back into a path, so the empty case has to be the one that adds nothing.
fn worktree_relative(root: &Path, directory: &Path) -> String {
    directory
        .strip_prefix(root)
        .map(|relative| {
            relative
                .components()
                .filter_map(|component| component.as_os_str().to_str())
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default()
}

/// Join a worktree-relative prefix to a discovery-relative path.
fn join_relative(prefix: &str, path: &str) -> String {
    if prefix.is_empty() {
        path.to_string()
    } else {
        format!("{prefix}/{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desired(path: &str, id: i64) -> DesiredRoot {
        DesiredRoot {
            path: PathBuf::from(path),
            worktree_id: id,
            identity: format!("git:example.com/r{id}"),
        }
    }

    #[test]
    fn boot_starts_every_registered_root() {
        // Jordan's first ask: whatever is already in the table gets watched,
        // with no separate boot path — this is just a pass against nothing.
        let plan = diff_roots(&[desired("/a", 1), desired("/b", 2)], &[]);
        assert_eq!(plan.start.len(), 2);
        assert!(plan.stop.is_empty());
    }

    #[test]
    fn a_newly_added_root_is_the_only_thing_started() {
        // Jordan's second ask, and the reason it needs no special case: the
        // same diff that handled boot handles the add.
        let plan = diff_roots(
            &[desired("/a", 1), desired("/b", 2)],
            &[PathBuf::from("/a")],
        );
        assert_eq!(plan.start, vec![desired("/b", 2)]);
        assert!(plan.stop.is_empty());
    }

    #[test]
    fn a_root_no_longer_registered_is_stopped() {
        let plan = diff_roots(
            &[desired("/a", 1)],
            &[PathBuf::from("/a"), PathBuf::from("/b")],
        );
        assert!(plan.start.is_empty());
        assert_eq!(plan.stop, vec![PathBuf::from("/b")]);
    }

    #[test]
    fn a_steady_state_pass_does_nothing() {
        let plan = diff_roots(
            &[desired("/a", 1), desired("/b", 2)],
            &[PathBuf::from("/a"), PathBuf::from("/b")],
        );
        assert!(
            plan.is_empty(),
            "the common case must be free: no churn, no log line, no watcher rebuilt"
        );
    }

    #[test]
    fn a_root_whose_worktree_id_changed_is_not_restarted() {
        // Identity is the PATH: re-registering the same directory reuses the
        // watcher. Restarting it would drop pending events for no gain.
        let plan = diff_roots(&[desired("/a", 99)], &[PathBuf::from("/a")]);
        assert!(plan.is_empty());
    }

    #[test]
    fn nested_roots_are_both_watched_rather_than_silently_merged() {
        let plan = diff_roots(&[desired("/code", 1), desired("/code/project", 2)], &[]);
        assert_eq!(
            plan.start.len(),
            2,
            "absorbing the covered root is a product decision, not a diff-time guess"
        );
    }

    #[test]
    fn a_directory_at_the_root_has_an_empty_prefix() {
        assert_eq!(worktree_relative(Path::new("/r"), Path::new("/r")), "");
        assert_eq!(join_relative("", "src/lib.rs"), "src/lib.rs");
    }

    #[test]
    fn a_nested_directory_prefixes_the_discovered_path() {
        assert_eq!(
            worktree_relative(Path::new("/r"), Path::new("/r/crates/core")),
            "crates/core"
        );
        assert_eq!(
            join_relative("crates/core", "src/lib.rs"),
            "crates/core/src/lib.rs",
            "the store keys on worktree-relative paths; discovery reports walk-relative ones"
        );
    }
}
