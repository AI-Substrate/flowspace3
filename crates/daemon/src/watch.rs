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

    /// The roots this supervisor currently holds an OS watcher for.
    ///
    /// Exists so a test can assert that a removed root stops being watched
    /// (PRD req 57) without reaching into private state or waiting on
    /// filesystem events. Reading it is the only way to observe the diff's
    /// effect: the plan itself is pure and already tested, but "the plan was
    /// applied" is a different claim.
    #[must_use]
    pub fn watched_roots(&self) -> Vec<PathBuf> {
        self.watched.keys().cloned().collect()
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

        let root_include_hidden =
            fs3_store::worktree_include_hidden(&self.state.db, watched.worktree_id)
                .await
                .context("reading the worktree's hidden-directory policy")?
                .unwrap_or(false);
        let mut settings = DiscoverySettings::from(&self.state.config.scan);
        settings.include_hidden |= root_include_hidden;
        let discovered =
            match discovery::discover_subtree(&settled.root, &settled.directory, &settings) {
                Ok(Some(discovered)) => discovered,
                // A walk from the ROOT would never have descended here: an
                // event inside a gitignored, hidden or denied tree. Returning
                // before `record_walk` is the whole fix — those files never
                // enter `worktree_files`, so they never enter the ping-pong
                // where this pass admits them and the next full walk reaps
                // them, buying a summary and a vector for each one on the way
                // past. Measured before it existed: 886 gitignored files, 4,436
                // paid vectors, all of it garbage by the next walk.
                Ok(None) => {
                    tracing::debug!(
                        directory = %settled.directory.display(),
                        "an event inside a directory fs3 does not index"
                    );
                    return Ok(0);
                }
                // The directory is gone — the everyday case for a delete or a
                // rename, and not an error. Its files are handled by the full-walk
                // backstop; see the module docs.
                Err(discovery::DiscoveryError::NotADirectory(_)) => return Ok(0),
                Err(error) => {
                    return Err(anyhow::Error::new(error)
                        .context(format!("re-listing {}", settled.directory.display())));
                }
            };

        // Discovery reports subtree paths relative to the WORKTREE ROOT, which
        // is already the shape the store keys on, so nothing is rebased here.
        // The prefix survives for `record_walk`, which needs to know which
        // slice of the worktree map this walk is entitled to replace.
        let prefix = worktree_relative(&settled.root, &settled.directory);
        let known = fs3_store::worktree_file_map(&self.state.db, watched.worktree_id)
            .await
            .context("reading the worktree's known blobs")?;

        let mut enqueued = 0;
        // The walked directory's own path -> blob answer, which becomes this
        // subtree's slice of the worktree map below.
        let mut walked: Vec<(String, fs3_core::BlobRef)> =
            Vec::with_capacity(discovered.files.len());

        for file in &discovered.files {
            let absolute = settled.root.join(&file.path);

            let blob = match fs3_git::blob_id(&absolute) {
                Ok(blob) => blob,
                // Vanished between the walk and the hash: it is a file that is
                // no longer there, and the next pass will agree.
                Err(fs3_git::Error::Io { .. }) => continue,
                Err(error) => return Err(anyhow::Error::new(error).context("hashing a file")),
            };

            let unchanged = known.get(&file.path).map(String::as_str) == Some(blob.as_str());
            walked.push((file.path.clone(), blob.clone()));
            // The whole reason over-reporting is free: content keying means an
            // unchanged file enqueues nothing at all, so a directory walk
            // triggered by one edit costs one job, not a directory's worth.
            if unchanged {
                continue;
            }

            let job = ScanFileJob {
                worktree_id: watched.worktree_id,
                identity: watched.identity.clone(),
                path: file.path.clone(),
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

        self.record_walk(watched.worktree_id, &prefix, &known, walked)
            .await?;

        Ok(enqueued)
    }

    /// Write the worktree's path→blob map back, with the walked subtree
    /// replaced by what the walk just found.
    ///
    /// Without this the blob diff above is blind to every file the WATCHER
    /// discovered: only `add`/`scan` write this map, so a watcher-found file is
    /// never in `known` and is therefore re-enqueued on every subsequent event
    /// in its directory, forever. Measured before the fix on a live daemon:
    /// `src/second.rs` scanned five times, `src/third.rs` three, for three
    /// unrelated edits.
    ///
    /// The whole map is written rather than a delta, because that is the only
    /// shape `sync_worktree_files` has — and handing it just the subtree would
    /// reap every path outside it. Entries outside the walked prefix are
    /// carried through verbatim, so nothing beyond this directory is touched.
    ///
    /// The reap that DOES happen is the useful one: a path under the prefix
    /// that the walk no longer found is dropped, so a deleted file stops being
    /// findable at the next event rather than at the next full walk.
    ///
    /// Race: a concurrent `add_root` on the same worktree is also a
    /// read-modify-write of this map, so the later writer wins. The cost of
    /// losing that race is one redundant re-enqueue on the next event, which
    /// content keying makes free — worth far less than the transaction it would
    /// take to prevent.
    async fn record_walk(
        &self,
        worktree_id: i64,
        prefix: &str,
        known: &std::collections::HashMap<String, String>,
        walked: Vec<(String, fs3_core::BlobRef)>,
    ) -> Result<()> {
        let mut full = walked;

        for (path, blob) in known {
            // Everything the walk found is under the prefix by construction
            // (discovery reports the subtree's paths relative to the worktree
            // root), so one check covers both "this subtree is being replaced"
            // and "this exact path was just re-listed".
            if under_prefix(path, prefix) {
                continue;
            }
            // A stored blob that no longer parses is a row this daemon did not
            // write; carrying it through unvalidated would be worse than
            // dropping it, because `sync_worktree_files` would reject the whole
            // transaction.
            match fs3_core::BlobRef::new(blob.clone()) {
                Ok(blob) => full.push((path.clone(), blob)),
                Err(error) => {
                    tracing::warn!(path, %error, "dropping an unreadable worktree_files row");
                }
            }
        }

        let removed = fs3_store::sync_worktree_files(&self.state.db, worktree_id, &full)
            .await
            .context("recording the walked directory's blobs")?;
        if removed > 0 {
            tracing::info!(
                removed,
                prefix = if prefix.is_empty() { "." } else { prefix },
                "paths under a re-listed directory are gone"
            );
        }
        Ok(())
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
/// Empty rather than `"."`: it is a PREFIX into the worktree map, and the
/// empty case has to be the one that covers everything.
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

/// Is this worktree-relative path inside the walked subtree?
///
/// An empty prefix means the walk was the worktree root, and everything is
/// inside it. Otherwise the path must be the directory itself or sit below a
/// `/` boundary — the check every string-prefix implementation gets wrong,
/// where `src2/x.rs` looks like it is under `src`.
fn under_prefix(path: &str, prefix: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    path == prefix
        || (path.len() > prefix.len()
            && path.starts_with(prefix)
            && path.as_bytes()[prefix.len()] == b'/')
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
    }

    #[test]
    fn a_nested_directory_names_its_slice_of_the_worktree_map() {
        assert_eq!(
            worktree_relative(Path::new("/r"), Path::new("/r/crates/core")),
            "crates/core"
        );
    }

    #[test]
    fn an_empty_prefix_covers_the_whole_worktree() {
        // The walk was the root itself, so nothing is "outside" it and the
        // whole map is replaced by what the walk found.
        assert!(under_prefix("src/lib.rs", ""));
        assert!(under_prefix("", ""));
    }

    #[test]
    fn a_prefix_matches_the_directory_and_what_is_under_it() {
        assert!(under_prefix("src", "src"));
        assert!(under_prefix("src/lib.rs", "src"));
        assert!(under_prefix("src/deep/x.rs", "src"));
    }

    #[test]
    fn a_prefix_stops_at_a_path_boundary_not_a_string_one() {
        // This is the bug that would silently reap a sibling directory: the
        // walked prefix `src` must not swallow `src2`, because everything
        // "under the prefix" that the walk did not find gets DELETED.
        assert!(!under_prefix("src2/lib.rs", "src"));
        assert!(!under_prefix("srcx", "src"));
        assert!(!under_prefix("other/src/lib.rs", "src"));
    }
}
