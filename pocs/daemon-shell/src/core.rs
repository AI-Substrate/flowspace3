//! The part of the daemon worth keeping: the debounce + dirty-set logic, as
//! pure functions over values.
//!
//! Nothing in this module touches the filesystem, the clock, the network, or a
//! thread. Time arrives as `now_ms` — milliseconds on a monotonic clock the
//! caller owns — and paths arrive as already-normalised [`PathBuf`]s. That is
//! what makes the interesting behaviour (coalescing, settling, nesting,
//! ignore rules) testable in microseconds instead of in wall-clock seconds,
//! and it is the split the real fs3 daemon should copy: `notify` and `axum`
//! are shells around this.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

/// Directory names whose contents are never interesting.
///
/// Hardcoded on purpose — a prototype does not need a config surface for this,
/// and the real daemon will want gitignore semantics rather than a name list.
/// The names are matched against whole path COMPONENTS, not substrings, so
/// `src/target_types.rs` is not mistaken for a build directory.
///
/// Matching is ASCII-case-INSENSITIVE on every platform. That deliberately
/// over-matches on Linux (a directory genuinely named `Target` is skipped),
/// because the alternative under-matches on the case-insensitive volumes mac
/// and Windows ship by default — where `Target` and `target` are the same
/// directory. Case sensitivity is a property of the VOLUME, not the OS (APFS
/// can be either), so no `cfg!` can get this right; the real daemon has to ask
/// the filesystem.
pub const IGNORED_DIRECTORIES: &[&str] = &[".git", "target", "node_modules"];

/// Why a filesystem event did not reach the dirty set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rejected {
    /// The path lies inside one of [`IGNORED_DIRECTORIES`].
    Ignored,
    /// The path is not under the root the event was attributed to. A watcher
    /// should never produce this; it is kept as a distinct outcome so that a
    /// backend which does is visible rather than silently trusted.
    OutsideRoot,
}

/// What [`Debouncer::observe`] did with an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Observed {
    /// First event for this path in this debounce window; a new pending entry.
    Opened,
    /// Folded into an existing pending entry, pushing its settle time out.
    Coalesced,
    /// Dropped, with the reason.
    Rejected(Rejected),
}

/// A path that has seen events and is waiting out its quiet period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    /// The watched root the path was attributed to.
    pub root: PathBuf,
    /// Monotonic ms of the first event in this window.
    pub first_event_ms: u64,
    /// Monotonic ms of the most recent event; the settle clock restarts here.
    pub last_event_ms: u64,
    /// How many raw events were folded into this entry.
    pub events: u64,
}

/// A path that has been quiet for the whole debounce window: work to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dirty {
    /// The path, as the watcher reported it.
    pub path: PathBuf,
    /// The watched root it belongs to.
    pub root: PathBuf,
    /// Monotonic ms of the first event that opened the window.
    pub first_event_ms: u64,
    /// Monotonic ms of the last event before the path went quiet.
    pub last_event_ms: u64,
    /// Monotonic ms at which the sweep declared it settled.
    pub settled_at_ms: u64,
    /// Raw events folded into this one dirty entry — the coalescing ratio.
    pub events: u64,
}

impl Dirty {
    /// How long the burst of activity lasted, first event to last.
    #[must_use]
    pub fn burst_ms(&self) -> u64 {
        self.last_event_ms.saturating_sub(self.first_event_ms)
    }

    /// Overshoot past the debounce window: how late the sweep noticed.
    ///
    /// With a `window_ms` debounce this is `settled_at - last_event - window`,
    /// i.e. the sweep interval's contribution, and it is the number that says
    /// whether the daemon reacts promptly once things go quiet.
    #[must_use]
    pub fn settle_lag_ms(&self, window_ms: u64) -> u64 {
        self.settled_at_ms
            .saturating_sub(self.last_event_ms)
            .saturating_sub(window_ms)
    }
}

/// The debounce state machine: events in, settled dirty paths out.
///
/// One instance covers every watched root, because coalescing is per PATH and
/// a path belongs to exactly one root (see [`nesting_conflict`], which is what
/// keeps that true).
#[derive(Debug, Clone)]
pub struct Debouncer {
    window_ms: u64,
    pending: BTreeMap<PathBuf, Pending>,
    ignored_events: u64,
}

impl Debouncer {
    /// A debouncer with the given quiet period in milliseconds.
    #[must_use]
    pub fn new(window_ms: u64) -> Self {
        Self {
            window_ms,
            pending: BTreeMap::new(),
            ignored_events: 0,
        }
    }

    /// The configured quiet period.
    #[must_use]
    pub fn window_ms(&self) -> u64 {
        self.window_ms
    }

    /// How many paths are mid-window right now.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// How many raw events were dropped by the ignore rules.
    #[must_use]
    pub fn ignored_events(&self) -> u64 {
        self.ignored_events
    }

    /// Pending paths under `root`, for per-root reporting.
    #[must_use]
    pub fn pending_under(&self, root: &Path) -> usize {
        self.pending.values().filter(|p| p.root == root).count()
    }

    /// Fold one filesystem event into the pending set.
    ///
    /// `now_ms` must be monotonically non-decreasing across calls; a clock that
    /// goes backwards would let a path settle early. That is exactly why this
    /// takes the time rather than reading it: the caller owns the one clock.
    pub fn observe(&mut self, root: &Path, path: &Path, now_ms: u64) -> Observed {
        if !path.starts_with(root) {
            return Observed::Rejected(Rejected::OutsideRoot);
        }
        if is_ignored(path) {
            self.ignored_events += 1;
            return Observed::Rejected(Rejected::Ignored);
        }
        match self.pending.get_mut(path) {
            Some(entry) => {
                entry.last_event_ms = now_ms.max(entry.last_event_ms);
                entry.events += 1;
                Observed::Coalesced
            }
            None => {
                self.pending.insert(
                    path.to_path_buf(),
                    Pending {
                        root: root.to_path_buf(),
                        first_event_ms: now_ms,
                        last_event_ms: now_ms,
                        events: 1,
                    },
                );
                Observed::Opened
            }
        }
    }

    /// Remove and return every path that has been quiet for the whole window.
    ///
    /// Returned in path order (the map is a [`BTreeMap`]), which makes a
    /// transcript of a 100-file burst readable instead of a hash shuffle.
    pub fn sweep(&mut self, now_ms: u64) -> Vec<Dirty> {
        let settled: Vec<PathBuf> = self
            .pending
            .iter()
            .filter(|(_, p)| now_ms.saturating_sub(p.last_event_ms) >= self.window_ms)
            .map(|(path, _)| path.clone())
            .collect();

        settled
            .into_iter()
            .map(|path| {
                let entry = self
                    .pending
                    .remove(&path)
                    .expect("key came from this map an instant ago");
                Dirty {
                    path,
                    root: entry.root,
                    first_event_ms: entry.first_event_ms,
                    last_event_ms: entry.last_event_ms,
                    settled_at_ms: now_ms,
                    events: entry.events,
                }
            })
            .collect()
    }

    /// Drop every pending path under `root`, returning how many were dropped.
    ///
    /// Called when a root is unwatched. The alternative — letting in-flight
    /// paths settle after their root is gone — would hand the consumer work
    /// for a tree it no longer watches, which is the removal race the real
    /// daemon has to answer for.
    pub fn forget_root(&mut self, root: &Path) -> usize {
        let before = self.pending.len();
        self.pending.retain(|_, entry| entry.root != root);
        before - self.pending.len()
    }
}

/// Is this path inside a directory nobody cares about?
///
/// Component-wise, and it deliberately also matches when the ignored name is
/// the FINAL component, so the `target` directory itself — not just its
/// contents — is uninteresting.
#[must_use]
pub fn is_ignored(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(name) => IGNORED_DIRECTORIES
            .iter()
            .any(|ignored| name.eq_ignore_ascii_case(ignored)),
        _ => false,
    })
}

/// Why a candidate watch root cannot join the set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "conflict", content = "with")]
pub enum Conflict {
    /// The exact path is already watched.
    Duplicate(PathBuf),
    /// An existing root already covers the candidate (candidate is inside it).
    CoveredBy(PathBuf),
    /// The candidate would cover an existing root (existing is inside it).
    Covers(PathBuf),
}

/// Decide whether `candidate` may be added to `existing`.
///
/// Overlapping recursive watches are refused rather than merged. Two watchers
/// over the same file mean the same edit lands twice with two different root
/// attributions, and the dirty set — keyed by path — would then flip-flop over
/// which root owns it. Refusing is one line to explain; merging is a lifetime
/// of "which root did this come from".
///
/// Both paths are expected to be normalised already (see [`normalize`]);
/// comparing un-normalised paths would make `./a` and `a` look unrelated.
#[must_use]
pub fn nesting_conflict(existing: &[PathBuf], candidate: &Path) -> Option<Conflict> {
    for root in existing {
        if root == candidate {
            return Some(Conflict::Duplicate(root.clone()));
        }
        if candidate.starts_with(root) {
            return Some(Conflict::CoveredBy(root.clone()));
        }
        if root.starts_with(candidate) {
            return Some(Conflict::Covers(root.clone()));
        }
    }
    None
}

/// Flatten `.` and `..` out of a path without asking the filesystem.
///
/// This is the portable half of normalisation; the caller pairs it with
/// [`std::fs::canonicalize`] when the path exists. It exists separately
/// because `canonicalize` FAILS on a path that has been deleted, and a delete
/// event is exactly when a watcher needs to reason about a path it can no
/// longer stat.
///
/// `..` is popped lexically, which is wrong in the presence of symlinks — a
/// known and accepted prototype limitation, and the reason real roots go
/// through `canonicalize` at add time.
#[must_use]
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(Component::ParentDir);
                }
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        // Not a real path and never touched: these tests do no I/O.
        PathBuf::from("/repo")
    }

    fn under(rest: &str) -> PathBuf {
        root().join(rest)
    }

    #[test]
    fn first_event_opens_a_window() {
        let mut debouncer = Debouncer::new(10_000);
        assert_eq!(
            debouncer.observe(&root(), &under("src/lib.rs"), 1_000),
            Observed::Opened
        );
        assert_eq!(debouncer.pending_len(), 1);
    }

    #[test]
    fn repeat_events_coalesce_into_one_entry() {
        let mut debouncer = Debouncer::new(10_000);
        debouncer.observe(&root(), &under("src/lib.rs"), 1_000);
        for tick in 1..5 {
            assert_eq!(
                debouncer.observe(&root(), &under("src/lib.rs"), 1_000 + tick * 10),
                Observed::Coalesced
            );
        }
        assert_eq!(debouncer.pending_len(), 1);

        let dirty = debouncer.sweep(1_040 + 10_000);
        assert_eq!(dirty.len(), 1, "five events, one dirty path");
        assert_eq!(dirty[0].events, 5);
        assert_eq!(dirty[0].first_event_ms, 1_000);
        assert_eq!(dirty[0].last_event_ms, 1_040);
    }

    #[test]
    fn a_later_event_pushes_the_settle_time_out() {
        let mut debouncer = Debouncer::new(1_000);
        debouncer.observe(&root(), &under("a.txt"), 0);
        // 900ms in — would have settled at 1000 — another event arrives.
        debouncer.observe(&root(), &under("a.txt"), 900);

        assert!(
            debouncer.sweep(1_000).is_empty(),
            "the second event restarted the quiet period"
        );
        assert!(debouncer.sweep(1_899).is_empty(), "one ms early");
        assert_eq!(debouncer.sweep(1_900).len(), 1, "exactly at the boundary");
    }

    #[test]
    fn a_continuous_stream_never_settles() {
        let mut debouncer = Debouncer::new(1_000);
        for tick in 0..100 {
            debouncer.observe(&root(), &under("busy.log"), tick * 500);
            assert!(
                debouncer.sweep(tick * 500).is_empty(),
                "a file written every 500ms under a 1s debounce must stay pending"
            );
        }
        // This is the starvation property, stated as a test rather than a
        // comment: a perpetually busy file is invisible until it stops. The
        // real daemon needs a maximum-age escape hatch; this one has none.
        assert_eq!(debouncer.pending_len(), 1);
    }

    #[test]
    fn independent_paths_settle_independently() {
        let mut debouncer = Debouncer::new(1_000);
        debouncer.observe(&root(), &under("early.rs"), 0);
        debouncer.observe(&root(), &under("late.rs"), 500);

        let first = debouncer.sweep(1_000);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].path, under("early.rs"));

        let second = debouncer.sweep(1_500);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].path, under("late.rs"));
    }

    #[test]
    fn sweeping_removes_what_it_returns() {
        let mut debouncer = Debouncer::new(100);
        debouncer.observe(&root(), &under("once.rs"), 0);
        assert_eq!(debouncer.sweep(100).len(), 1);
        assert_eq!(debouncer.pending_len(), 0);
        assert!(
            debouncer.sweep(10_000).is_empty(),
            "a settled path is handed over exactly once"
        );
    }

    #[test]
    fn a_hundred_file_burst_yields_a_hundred_entries_in_path_order() {
        let mut debouncer = Debouncer::new(10_000);
        for index in 0..100u64 {
            // Zero-padded so lexical order is numeric order.
            debouncer.observe(&root(), &under(&format!("burst/f{index:03}.txt")), index);
        }
        let dirty = debouncer.sweep(10_099);
        assert_eq!(dirty.len(), 100);
        assert_eq!(dirty[0].path, under("burst/f000.txt"));
        assert_eq!(dirty[99].path, under("burst/f099.txt"));
        assert!(
            dirty.iter().all(|entry| entry.events == 1),
            "distinct paths do not coalesce with each other"
        );
    }

    #[test]
    fn ignored_directories_are_dropped_and_counted() {
        let mut debouncer = Debouncer::new(1_000);
        for noisy in [
            ".git/index",
            "target/debug/build.log",
            "node_modules/x/y.js",
        ] {
            assert_eq!(
                debouncer.observe(&root(), &under(noisy), 0),
                Observed::Rejected(Rejected::Ignored)
            );
        }
        assert_eq!(debouncer.pending_len(), 0);
        assert_eq!(debouncer.ignored_events(), 3);
    }

    #[test]
    fn ignore_matches_components_not_substrings() {
        assert!(!is_ignored(Path::new("/repo/src/target_types.rs")));
        assert!(!is_ignored(Path::new("/repo/gitignore-notes.md")));
        assert!(is_ignored(Path::new("/repo/target")));
        assert!(is_ignored(Path::new("/repo/crates/core/target/x")));
    }

    #[test]
    fn an_event_outside_its_root_is_refused() {
        let mut debouncer = Debouncer::new(1_000);
        assert_eq!(
            debouncer.observe(&root(), Path::new("/elsewhere/a.rs"), 0),
            Observed::Rejected(Rejected::OutsideRoot)
        );
        assert_eq!(debouncer.pending_len(), 0);
        assert_eq!(
            debouncer.ignored_events(),
            0,
            "an out-of-root event is a watcher bug, not ignore-list noise"
        );
    }

    #[test]
    fn unwatching_a_root_drops_only_its_pending_paths() {
        let mut debouncer = Debouncer::new(10_000);
        let other = PathBuf::from("/other");
        debouncer.observe(&root(), &under("a.rs"), 0);
        debouncer.observe(&root(), &under("b.rs"), 0);
        debouncer.observe(&other, &other.join("c.rs"), 0);

        assert_eq!(debouncer.forget_root(&root()), 2);
        assert_eq!(debouncer.pending_len(), 1);
        assert_eq!(debouncer.pending_under(&other), 1);
        assert!(
            debouncer.sweep(20_000).iter().all(|d| d.root == other),
            "a removed root must not hand out work after removal"
        );
    }

    #[test]
    fn settle_lag_reports_only_the_sweep_overshoot() {
        let dirty = Dirty {
            path: under("a.rs"),
            root: root(),
            first_event_ms: 100,
            last_event_ms: 400,
            settled_at_ms: 10_550,
            events: 4,
        };
        assert_eq!(dirty.burst_ms(), 300);
        assert_eq!(dirty.settle_lag_ms(10_000), 150);
        assert_eq!(
            dirty.settle_lag_ms(20_000),
            0,
            "a window wider than the observation saturates instead of underflowing"
        );
    }

    /// A filename that is not valid UTF-8 cannot be reported over JSON.
    ///
    /// `serde`'s `Path` impl refuses rather than lossily transcoding, so one
    /// such file anywhere under a watched root turns `GET /dirty` into a
    /// serialization failure for the WHOLE set — every other dirty path goes
    /// with it. Linux allows any byte but `/` and NUL in a filename, so this
    /// is reachable in the field, not a curiosity.
    ///
    /// Unix-only because that is the only place the bad value can be built:
    /// macOS rejects invalid UTF-8 at the filesystem, and Windows paths are
    /// UTF-16 (whose unpaired surrogates are the same problem in a different
    /// encoding). The real daemon has to answer this — with lossy display plus
    /// a byte-exact key, or by refusing the path with a named error.
    #[cfg(unix)]
    #[test]
    fn a_non_utf8_path_cannot_be_serialized() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let dirty = Dirty {
            path: PathBuf::from(OsStr::from_bytes(b"/repo/\xff\xfe.rs")),
            root: root(),
            first_event_ms: 0,
            last_event_ms: 0,
            settled_at_ms: 1,
            events: 1,
        };
        let error = serde_json::to_string(&dirty).expect_err("must refuse, not transcode");
        assert!(
            error.to_string().contains("UTF-8"),
            "the failure names the encoding, not a mystery: {error}"
        );
    }

    #[test]
    fn nesting_conflicts_name_the_root_they_collide_with() {
        let existing = vec![PathBuf::from("/a/b"), PathBuf::from("/c")];

        assert_eq!(
            nesting_conflict(&existing, Path::new("/a/b")),
            Some(Conflict::Duplicate(PathBuf::from("/a/b")))
        );
        assert_eq!(
            nesting_conflict(&existing, Path::new("/a/b/deep")),
            Some(Conflict::CoveredBy(PathBuf::from("/a/b")))
        );
        assert_eq!(
            nesting_conflict(&existing, Path::new("/a")),
            Some(Conflict::Covers(PathBuf::from("/a/b")))
        );
        assert_eq!(nesting_conflict(&existing, Path::new("/d")), None);
    }

    #[test]
    fn a_sibling_with_a_shared_prefix_is_not_nested() {
        // The bug every string-prefix implementation has: "/a/bc" starts with
        // the TEXT "/a/b" but is not inside the DIRECTORY "/a/b".
        let existing = vec![PathBuf::from("/a/b")];
        assert_eq!(nesting_conflict(&existing, Path::new("/a/bc")), None);
    }

    #[test]
    fn normalize_flattens_dot_and_dotdot_without_touching_the_disk() {
        assert_eq!(
            normalize(Path::new("/a/./b/../c")),
            PathBuf::from("/a/c"),
            "lexical only — no filesystem access, so it works on deleted paths"
        );
        assert_eq!(normalize(Path::new("a/b/")), PathBuf::from("a/b"));
        assert_eq!(
            normalize(Path::new("../x")),
            PathBuf::from("../x"),
            "a leading .. has nothing to pop and is kept"
        );
    }
}
