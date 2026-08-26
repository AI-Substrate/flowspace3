//! The `notify` shell: watched roots, their OS watchers, and the sweep that
//! turns pending paths into dirty ones.
//!
//! Everything that decides anything lives in [`crate::core`]. This module only
//! owns the parts that cannot be pure: OS watchers, a clock, a mutex, and a
//! channel between `notify`'s own thread and the Tokio runtime.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::core::{Conflict, Debouncer, Dirty, Observed, Rejected, nesting_conflict, normalize};

/// How often the sweep asks the debouncer what has gone quiet.
///
/// This is the resolution of "we noticed": with a 10s window and a 250ms sweep,
/// a settled path is reported between 10.00s and 10.25s after its last event.
/// Making it smaller costs a wakeup; making it larger is invisible to a human
/// but shows up directly in the measured settle lag.
pub const SWEEP_INTERVAL: Duration = Duration::from_millis(250);

/// One event, already attributed to a root, on its way from `notify`'s thread
/// into the runtime.
#[derive(Debug)]
pub struct RawEvent {
    root: PathBuf,
    path: PathBuf,
    kind: EventKind,
    /// Stamped where the event was RECEIVED, not where it is processed, so a
    /// backlog in the channel cannot silently inflate the measured latency.
    at_ms: u64,
}

/// Per-root counters, reported by `GET /status`.
#[derive(Debug, Clone, Serialize)]
pub struct RootReport {
    /// The canonicalised root.
    pub path: PathBuf,
    /// How long the root has been watched.
    pub watching_for_ms: u64,
    /// Raw `notify` events received for this root.
    pub events: u64,
    /// Events dropped because the path is inside an ignored directory.
    pub ignored_events: u64,
    /// Paths currently inside their debounce window.
    pub pending: usize,
    /// Settled paths waiting in the dirty set.
    pub dirty: usize,
}

/// A watched root and the OS watcher keeping it alive.
struct Root {
    added_at: Instant,
    events: u64,
    ignored_events: u64,
    /// Dropping the watcher is what stops the OS-level subscription. It is
    /// held purely for that: nothing ever calls a method on it again.
    _watcher: RecommendedWatcher,
}

/// Everything the HTTP handlers and the sweep share.
pub struct Supervisor {
    started_at: Instant,
    debounce: Duration,
    state: Mutex<State>,
    events: mpsc::UnboundedSender<RawEvent>,
}

struct State {
    roots: BTreeMap<PathBuf, Root>,
    debouncer: Debouncer,
    dirty: BTreeMap<PathBuf, Dirty>,
}

/// What `POST /watch` produced.
#[derive(Debug)]
pub enum Added {
    /// Now watching this canonicalised root.
    Watching(PathBuf),
    /// Refused: it overlaps a root already being watched.
    Rejected(Conflict),
}

impl Supervisor {
    /// Build a supervisor and the receiver its event pump must drain.
    ///
    /// The channel is UNBOUNDED on purpose. `notify` hands events to a
    /// callback on its own thread; that callback cannot await, so a bounded
    /// channel would have to either block the watcher thread (which makes the
    /// OS drop events on macOS and Windows) or drop events itself. Unbounded
    /// moves the backpressure problem into memory, where at least it is
    /// visible — and the debouncer collapses a burst to one entry per path
    /// almost immediately.
    #[must_use]
    pub fn new(debounce: Duration) -> (Arc<Self>, mpsc::UnboundedReceiver<RawEvent>) {
        let (events, receiver) = mpsc::unbounded_channel();
        let supervisor = Arc::new(Self {
            started_at: Instant::now(),
            debounce,
            state: Mutex::new(State {
                roots: BTreeMap::new(),
                debouncer: Debouncer::new(u64::try_from(debounce.as_millis()).unwrap_or(u64::MAX)),
                dirty: BTreeMap::new(),
            }),
            events,
        });
        (supervisor, receiver)
    }

    /// Milliseconds since the supervisor started — the one monotonic clock.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// The configured debounce window.
    #[must_use]
    pub fn debounce(&self) -> Duration {
        self.debounce
    }

    /// Start watching `requested`, recursively.
    ///
    /// # Errors
    /// When the path does not exist, is not a directory, cannot be
    /// canonicalised, or the OS refuses the watch.
    pub fn watch(self: &Arc<Self>, requested: &Path) -> Result<Added> {
        let path = canonical_root(requested)?;

        {
            let state = self.state.lock().expect("supervisor state mutex poisoned");
            let existing: Vec<PathBuf> = state.roots.keys().cloned().collect();
            if let Some(conflict) = nesting_conflict(&existing, &path) {
                return Ok(Added::Rejected(conflict));
            }
        }

        // The watcher is created BEFORE the lock is retaken: `notify` does real
        // work here (an FSEvents stream, an inotify fd) and it must not happen
        // while HTTP handlers are blocked on the state mutex.
        let sender = self.events.clone();
        let supervisor = Arc::clone(self);
        let root_for_events = path.clone();
        let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            // This closure runs on `notify`'s thread, outside the Tokio
            // runtime. It may not block and may not await; `UnboundedSender`
            // is the only channel that satisfies both.
            match result {
                Ok(event) => {
                    let at_ms = supervisor.now_ms();
                    for raw in event.paths {
                        let _ = sender.send(RawEvent {
                            root: root_for_events.clone(),
                            // Lexical normalisation only: a Remove event names
                            // a path that no longer exists, so `canonicalize`
                            // would fail exactly when it matters.
                            path: normalize(&raw),
                            kind: event.kind,
                            at_ms,
                        });
                    }
                }
                Err(error) => tracing::warn!(%error, "watcher reported an error"),
            }
        })
        .context("creating an OS file watcher")?;

        watcher
            .watch(&path, RecursiveMode::Recursive)
            .with_context(|| format!("watching {}", path.display()))?;

        let mut state = self.state.lock().expect("supervisor state mutex poisoned");
        // Re-check under the lock: two concurrent POSTs could both have passed
        // the first check. The loser drops its watcher here, which unsubscribes.
        let existing: Vec<PathBuf> = state.roots.keys().cloned().collect();
        if let Some(conflict) = nesting_conflict(&existing, &path) {
            return Ok(Added::Rejected(conflict));
        }
        state.roots.insert(
            path.clone(),
            Root {
                added_at: Instant::now(),
                events: 0,
                ignored_events: 0,
                _watcher: watcher,
            },
        );
        tracing::info!(root = %path.display(), "watching");
        Ok(Added::Watching(path))
    }

    /// Stop watching `requested`, discarding its pending and dirty paths.
    ///
    /// Returns the canonicalised root that was removed, or `None` if it was
    /// not watched.
    ///
    /// # Errors
    /// When the path cannot be canonicalised — including the case where the
    /// watched directory has been DELETED, which is why removal falls back to
    /// a lexical match.
    pub fn unwatch(&self, requested: &Path) -> Result<Option<PathBuf>> {
        let path = canonical_root(requested).unwrap_or_else(|_| normalize(requested));

        let mut state = self.state.lock().expect("supervisor state mutex poisoned");
        // Dropping the `Root` drops its watcher, which is the unsubscribe.
        // There is no explicit `unwatch` call: ownership IS the lifecycle.
        if state.roots.remove(&path).is_none() {
            return Ok(None);
        }
        let dropped = state.debouncer.forget_root(&path);
        let before = state.dirty.len();
        state.dirty.retain(|_, entry| entry.root != path);
        tracing::info!(
            root = %path.display(),
            dropped_pending = dropped,
            dropped_dirty = before - state.dirty.len(),
            "unwatched"
        );
        Ok(Some(path))
    }

    /// Fold one received event into the debouncer.
    fn absorb(&self, raw: &RawEvent) {
        let mut state = self.state.lock().expect("supervisor state mutex poisoned");
        // A root removed between the event landing in the channel and being
        // drained here: its counters are gone, so drop the event rather than
        // resurrect the root.
        let Some(root) = state.roots.get_mut(&raw.root) else {
            return;
        };
        root.events += 1;
        let outcome = state.debouncer.observe(&raw.root, &raw.path, raw.at_ms);
        if outcome == Observed::Rejected(Rejected::Ignored) {
            // The root was present a line ago and the lock has not been
            // released, so this lookup cannot fail; `if let` rather than
            // `expect` keeps a future refactor from turning that into a panic.
            if let Some(root) = state.roots.get_mut(&raw.root) {
                root.ignored_events += 1;
            }
        }
        tracing::debug!(
            path = %raw.path.display(),
            kind = ?raw.kind,
            outcome = ?outcome,
            "event"
        );
    }

    /// Promote everything that has gone quiet into the dirty set.
    ///
    /// Returns the newly settled entries so the caller can log them; the real
    /// daemon would enqueue a scan here instead.
    pub fn sweep(&self) -> Vec<Dirty> {
        let now = self.now_ms();
        let mut state = self.state.lock().expect("supervisor state mutex poisoned");
        let settled = state.debouncer.sweep(now);
        for entry in &settled {
            // Re-settling a path already in the dirty set overwrites it: the
            // newest observation wins, and the consumer still sees one entry.
            state.dirty.insert(entry.path.clone(), entry.clone());
        }
        settled
    }

    /// The current dirty set, newest-settled last.
    #[must_use]
    pub fn dirty(&self) -> Vec<Dirty> {
        let state = self.state.lock().expect("supervisor state mutex poisoned");
        state.dirty.values().cloned().collect()
    }

    /// Empty the dirty set, returning how many entries were handed over.
    pub fn drain_dirty(&self) -> usize {
        let mut state = self.state.lock().expect("supervisor state mutex poisoned");
        let count = state.dirty.len();
        state.dirty.clear();
        count
    }

    /// The debounce window in milliseconds — the same number the debouncer
    /// was built with, in the unit the HTTP surface reports.
    #[must_use]
    pub fn debounce_ms(&self) -> u64 {
        u64::try_from(self.debounce.as_millis()).unwrap_or(u64::MAX)
    }

    /// Per-root counters for `GET /status`.
    #[must_use]
    pub fn report(&self) -> Vec<RootReport> {
        let state = self.state.lock().expect("supervisor state mutex poisoned");
        state
            .roots
            .iter()
            .map(|(path, root)| RootReport {
                path: path.clone(),
                watching_for_ms: u64::try_from(root.added_at.elapsed().as_millis())
                    .unwrap_or(u64::MAX),
                events: root.events,
                ignored_events: root.ignored_events,
                pending: state.debouncer.pending_under(path),
                dirty: state.dirty.values().filter(|d| &d.root == path).count(),
            })
            .collect()
    }
}

/// Drain the watcher channel into the debouncer, forever.
///
/// Split from the sweep so a flood of events cannot delay a settle decision,
/// and a slow settle cannot back up the channel.
pub async fn pump(supervisor: Arc<Supervisor>, mut events: mpsc::UnboundedReceiver<RawEvent>) {
    while let Some(raw) = events.recv().await {
        supervisor.absorb(&raw);
    }
}

/// Sweep on a fixed interval, forever.
pub async fn sweeper(supervisor: Arc<Supervisor>) {
    let mut ticker = tokio::time::interval(SWEEP_INTERVAL);
    // A stalled runtime must not produce a burst of catch-up ticks; one late
    // sweep is a sweep, not four.
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        for entry in supervisor.sweep() {
            tracing::info!(
                path = %entry.path.display(),
                events = entry.events,
                burst_ms = entry.burst_ms(),
                settle_lag_ms = entry.settle_lag_ms(supervisor.debounce_ms()),
                "dirty"
            );
        }
    }
}

/// Resolve a requested root to the absolute, symlink-free path the watcher
/// will actually report events under.
///
/// `canonicalize` is what makes root comparison and event attribution honest,
/// and it is where the platforms differ loudly:
///
/// * **macOS** — `/tmp` and `/var` are symlinks into `/private`, so a root
///   added as `/tmp/x` becomes `/private/tmp/x`. FSEvents reports the resolved
///   form, so canonicalising is not cosmetic: skip it and every event looks
///   out-of-root.
/// * **Windows** — the result carries the `\\?\` extended-length prefix
///   (`\\?\C:\repo`). It compares and watches fine, but it is ugly in JSON and
///   a client that echoes it back un-prefixed still matches, because
///   `canonicalize` re-applies the prefix on the way in.
/// * **Linux** — the boring case, and the reason it is easy to ship a watcher
///   that only works there.
fn canonical_root(requested: &Path) -> Result<PathBuf> {
    let path = requested
        .canonicalize()
        .with_context(|| format!("resolving {}", requested.display()))?;
    if !path.is_dir() {
        bail!("{} is not a directory", path.display());
    }
    Ok(path)
}
