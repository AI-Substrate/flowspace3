//! The watcher's pure core: which filesystem events matter, and when a burst
//! of them has gone quiet enough to pay for a directory walk.
//!
//! Lifted from the `pocs/daemon-shell` prototype (see its `LEARNINGS.md`) with
//! the one thing that prototype deliberately left out — a **maximum settle
//! age** — put back. Nothing here touches the filesystem, the clock, the
//! network, or a thread: time arrives as `now_ms` on a monotonic clock the
//! caller owns. That is what lets the interesting behaviour be pinned in
//! microseconds instead of in `sleep` calls.
//!
//! # Why the unit is a DIRECTORY
//!
//! The prototype keyed pending work by file path. This does not, and the
//! reason is the finding that prototype exists for: `notify`'s recursive mode
//! is *emulated* on inotify — one watch per directory, installed by walking the
//! tree — so files created inside a brand-new directory produce **no events at
//! all**. The signal that survives on every backend is the directory's own
//! event. So the rule is "a dirty directory means re-list that directory", and
//! the key is therefore the **enclosing directory of the event path**, always,
//! with no attempt to decide whether the path was a file or a directory.
//!
//! Deciding would mean asking the filesystem, and a delete event names a path
//! that is already gone — precisely when the answer is needed and impossible.
//! Taking the parent unconditionally is correct for both cases: for a file it
//! is the directory whose contents changed, and for a directory it is the
//! directory whose listing changed, whose recursive re-list covers the child
//! anyway.
//!
//! The saving is not incidental. A 100-file burst measured 341 raw events on
//! macOS; keyed by directory that is **one** pending entry and one walk.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// Directory names whose contents never mean indexable work.
///
/// This is a pre-filter, not the indexing rule: `discovery::discover` owns what
/// is worth scanning, including full gitignore semantics. The filter exists
/// because `.git` is where the noise is — a single `git status` or `git commit`
/// rewrites index files continuously, and `.git` is not in anybody's
/// `.gitignore`, so without this every git operation would buy a directory
/// walk. `target` and `node_modules` are usually gitignored and so usually
/// redundant here; they are named anyway because "usually" is not a guarantee
/// and the cost of an extra name is one string comparison.
///
/// Matched against whole path COMPONENTS, so `src/target_types.rs` is not
/// mistaken for a build directory.
pub const IGNORED_DIRECTORIES: &[&str] = &[".git", "target", "node_modules"];

/// Why an event never reached the pending set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejected {
    /// Inside one of [`IGNORED_DIRECTORIES`].
    Ignored,
    /// Not under the root it was attributed to. A watcher should never produce
    /// this; it is a distinct outcome so that a backend which does is visible
    /// rather than silently trusted.
    OutsideRoot,
}

/// What [`Debouncer::observe`] did with an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Observed {
    /// First event for this directory in this window.
    Opened,
    /// Folded into an existing entry, pushing its settle time out.
    Coalesced,
    /// Dropped, with the reason.
    Rejected(Rejected),
}

/// Why a directory settled when it did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettleReason {
    /// Nothing has happened here for a whole debounce window.
    Quiet,
    /// The window never closed, but the entry hit its maximum age.
    ///
    /// This is the escape hatch the prototype lacked. A file written faster
    /// than the window — a log inside a watched repository is the everyday
    /// case — restarts the window on every event and would otherwise stay
    /// pending forever, taking every other change in its directory down with
    /// it. The cost of settling anyway is one extra walk; the cost of not
    /// doing so is a directory that is silently never indexed again.
    MaxAge,
}

/// A directory that has been quiet long enough to re-list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settled {
    /// The directory to re-list. Always at or below [`Settled::root`].
    pub directory: PathBuf,
    /// The watched root it belongs to.
    pub root: PathBuf,
    /// Monotonic ms of the first event in the window.
    pub first_event_ms: u64,
    /// Monotonic ms of the last event before it settled.
    pub last_event_ms: u64,
    /// Monotonic ms at which the sweep declared it settled.
    pub settled_at_ms: u64,
    /// Raw events folded into this one entry — the coalescing ratio.
    pub events: u64,
    /// Quiet, or forced out by age.
    pub reason: SettleReason,
}

/// A directory waiting out its quiet period.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pending {
    root: PathBuf,
    first_event_ms: u64,
    last_event_ms: u64,
    events: u64,
}

/// Events in, settled directories out.
#[derive(Clone, Debug)]
pub struct Debouncer {
    window_ms: u64,
    max_age_ms: u64,
    pending: BTreeMap<PathBuf, Pending>,
    ignored_events: u64,
}

impl Debouncer {
    /// A debouncer with a quiet period and a ceiling on how long an entry may
    /// stay pending.
    ///
    /// `max_age_ms` is clamped to at least `window_ms`: a ceiling below the
    /// window would settle everything on its first event, quietly turning the
    /// debounce off. That is a configuration mistake worth absorbing rather
    /// than a state worth having.
    #[must_use]
    pub fn new(window_ms: u64, max_age_ms: u64) -> Self {
        Self {
            window_ms,
            max_age_ms: max_age_ms.max(window_ms),
            pending: BTreeMap::new(),
            ignored_events: 0,
        }
    }

    /// The configured quiet period.
    #[must_use]
    pub fn window_ms(&self) -> u64 {
        self.window_ms
    }

    /// The configured ceiling on pending age.
    #[must_use]
    pub fn max_age_ms(&self) -> u64 {
        self.max_age_ms
    }

    /// How many directories are mid-window.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// How many raw events the ignore filter dropped.
    #[must_use]
    pub fn ignored_events(&self) -> u64 {
        self.ignored_events
    }

    /// Fold one filesystem event into the pending set, keyed by its enclosing
    /// directory.
    ///
    /// `now_ms` must be monotonically non-decreasing across calls; a clock that
    /// went backwards would let an entry settle early. That is exactly why this
    /// takes the time rather than reading it — the caller owns the one clock.
    pub fn observe(&mut self, root: &Path, event_path: &Path, now_ms: u64) -> Observed {
        if !event_path.starts_with(root) {
            return Observed::Rejected(Rejected::OutsideRoot);
        }
        if is_ignored(event_path) {
            self.ignored_events += 1;
            return Observed::Rejected(Rejected::Ignored);
        }

        let directory = enclosing_directory(root, event_path);
        match self.pending.get_mut(&directory) {
            Some(entry) => {
                entry.last_event_ms = now_ms.max(entry.last_event_ms);
                entry.events += 1;
                Observed::Coalesced
            }
            None => {
                self.pending.insert(
                    directory,
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

    /// Remove and return every directory that has gone quiet, or that has been
    /// pending for too long.
    ///
    /// Returned in path order (the map is a [`BTreeMap`]), which is what makes
    /// [`prune_nested`] a single linear pass.
    pub fn sweep(&mut self, now_ms: u64) -> Vec<Settled> {
        let ready: Vec<(PathBuf, SettleReason)> = self
            .pending
            .iter()
            .filter_map(|(directory, entry)| {
                if now_ms.saturating_sub(entry.last_event_ms) >= self.window_ms {
                    Some((directory.clone(), SettleReason::Quiet))
                } else if now_ms.saturating_sub(entry.first_event_ms) >= self.max_age_ms {
                    Some((directory.clone(), SettleReason::MaxAge))
                } else {
                    None
                }
            })
            .collect();

        ready
            .into_iter()
            .map(|(directory, reason)| {
                let entry = self
                    .pending
                    .remove(&directory)
                    .expect("key came from this map an instant ago");
                Settled {
                    directory,
                    root: entry.root,
                    first_event_ms: entry.first_event_ms,
                    last_event_ms: entry.last_event_ms,
                    settled_at_ms: now_ms,
                    events: entry.events,
                    reason,
                }
            })
            .collect()
    }

    /// Drop every pending directory under `root`, returning how many went.
    ///
    /// Called when a root stops being watched. Letting them settle afterwards
    /// would hand the daemon work for a tree it no longer tracks.
    pub fn forget_root(&mut self, root: &Path) -> usize {
        let before = self.pending.len();
        self.pending.retain(|_, entry| entry.root != root);
        before - self.pending.len()
    }
}

/// Drop every settled directory that a settled ANCESTOR already covers.
///
/// `discover` walks recursively, so re-listing `/repo/src` also lists
/// `/repo/src/deep`. Walking both would parse the same files twice for nothing.
/// Requires `settled` to be sorted by path, which [`Debouncer::sweep`] gives.
#[must_use]
pub fn prune_nested(settled: Vec<Settled>) -> Vec<Settled> {
    let mut kept: Vec<Settled> = Vec::with_capacity(settled.len());
    for entry in settled {
        // Sorted order means an ancestor, if there is one, is the most recent
        // thing kept from the same root — but a different root may sit between
        // them, so the check walks back rather than peeking once.
        let covered = kept
            .iter()
            .any(|ancestor| entry.directory.starts_with(&ancestor.directory));
        if !covered {
            kept.push(entry);
        }
    }
    kept
}

/// The directory whose listing this event changed.
///
/// Clamped to `root`: an event on the root itself has a parent OUTSIDE the
/// watched tree, and re-listing that would walk the user's whole home
/// directory. It is also never `None` in practice for the same reason, but the
/// clamp makes that structural rather than assumed.
fn enclosing_directory(root: &Path, event_path: &Path) -> PathBuf {
    match event_path.parent() {
        Some(parent) if parent.starts_with(root) => parent.to_path_buf(),
        _ => root.to_path_buf(),
    }
}

/// Is this path inside a directory nobody cares about?
///
/// ASCII-case-insensitive on every platform. That over-matches on Linux and
/// under-matches nowhere; case sensitivity is a property of the VOLUME rather
/// than the OS (APFS can be either), so no `cfg!` gets it right, and erring
/// toward ignoring noise is the cheap direction to be wrong in.
#[must_use]
pub fn is_ignored(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(name) => IGNORED_DIRECTORIES
            .iter()
            .any(|ignored| name.eq_ignore_ascii_case(ignored)),
        _ => false,
    })
}

/// Flatten `.` and `..` out of a path without asking the filesystem.
///
/// Event paths go through this and never through `canonicalize`, because a
/// delete event names a path that no longer exists — `canonicalize` fails
/// exactly when the watcher most needs to reason about the path. Roots are
/// canonicalized instead, at registration, which is where the path is
/// guaranteed to be there.
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

    const WINDOW: u64 = 10_000;
    const MAX_AGE: u64 = 60_000;

    fn root() -> PathBuf {
        // Never touched: these tests do no I/O.
        PathBuf::from("/repo")
    }

    fn under(rest: &str) -> PathBuf {
        root().join(rest)
    }

    #[test]
    fn an_event_is_keyed_by_its_enclosing_directory() {
        let mut debouncer = Debouncer::new(WINDOW, MAX_AGE);
        assert_eq!(
            debouncer.observe(&root(), &under("src/lib.rs"), 0),
            Observed::Opened
        );
        let settled = debouncer.sweep(WINDOW);
        assert_eq!(settled.len(), 1);
        assert_eq!(
            settled[0].directory,
            under("src"),
            "the unit of work is the directory to re-list, not the file"
        );
    }

    #[test]
    fn a_hundred_files_in_one_directory_are_one_pending_entry() {
        let mut debouncer = Debouncer::new(WINDOW, MAX_AGE);
        for index in 0..100u64 {
            debouncer.observe(&root(), &under(&format!("burst/f{index:03}.txt")), index);
        }
        assert_eq!(
            debouncer.pending_len(),
            1,
            "the measured 3-5x event amplification must cost ONE walk, not a hundred"
        );

        let settled = debouncer.sweep(99 + WINDOW);
        assert_eq!(settled.len(), 1);
        assert_eq!(settled[0].directory, under("burst"));
        assert_eq!(settled[0].events, 100);
        assert_eq!(settled[0].reason, SettleReason::Quiet);
    }

    #[test]
    fn an_event_on_the_root_itself_settles_the_root_not_its_parent() {
        let mut debouncer = Debouncer::new(WINDOW, MAX_AGE);
        debouncer.observe(&root(), &root(), 0);
        let settled = debouncer.sweep(WINDOW);
        assert_eq!(
            settled[0].directory,
            root(),
            "re-listing the parent of a watched root would walk outside the tree"
        );
    }

    #[test]
    fn a_later_event_pushes_the_settle_time_out() {
        let mut debouncer = Debouncer::new(1_000, MAX_AGE);
        debouncer.observe(&root(), &under("a/x.rs"), 0);
        debouncer.observe(&root(), &under("a/y.rs"), 900);

        assert!(
            debouncer.sweep(1_000).is_empty(),
            "the second event restarted the quiet period"
        );
        assert!(debouncer.sweep(1_899).is_empty(), "one ms early");
        assert_eq!(debouncer.sweep(1_900).len(), 1, "exactly at the boundary");
    }

    #[test]
    fn a_perpetually_written_file_settles_anyway_at_the_maximum_age() {
        // The prototype's known hole, now closed: a file written every 500ms
        // under a 1s window restarts the window forever. Without a ceiling its
        // whole directory would never be indexed again.
        let mut debouncer = Debouncer::new(1_000, 5_000);
        let mut settled = Vec::new();
        for tick in 0..20u64 {
            let now = tick * 500;
            debouncer.observe(&root(), &under("logs/app.log"), now);
            settled.extend(debouncer.sweep(now));
        }

        assert!(
            !settled.is_empty(),
            "a continuously written directory must still be re-listed"
        );
        assert!(
            settled.iter().all(|s| s.reason == SettleReason::MaxAge),
            "and it settles by age, never by quiet — it is never quiet"
        );
        assert_eq!(
            settled[0].settled_at_ms, 5_000,
            "the ceiling fires exactly at max_age after the FIRST event"
        );
    }

    #[test]
    fn the_maximum_age_clock_restarts_with_the_next_window() {
        // The forced settle must not leave a directory permanently overdue,
        // firing on every subsequent sweep. Once handed over, the entry is
        // gone, and the next burst's ceiling is measured from ITS first event.
        //
        // Every event here is closer together than the window, so `Quiet`
        // never fires and the only thing under test is the age clock.
        let mut debouncer = Debouncer::new(1_000, 2_000);
        for now in [0, 500, 1_000, 1_500] {
            debouncer.observe(&root(), &under("a/x.rs"), now);
        }
        let first = debouncer.sweep(2_000);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].reason, SettleReason::MaxAge);
        assert!(
            debouncer.sweep(2_001).is_empty(),
            "handed over exactly once, not overdue forever"
        );

        // A new burst opens at 2_100, so its ceiling is 4_100 — not 2_000
        // carried over from the entry that already settled.
        for now in [2_100, 2_600, 3_000, 3_500, 4_000] {
            debouncer.observe(&root(), &under("a/x.rs"), now);
        }
        assert!(
            debouncer.sweep(4_099).is_empty(),
            "one ms before the NEW entry's ceiling"
        );
        let second = debouncer.sweep(4_100);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].reason, SettleReason::MaxAge);
        assert_eq!(second[0].first_event_ms, 2_100);
    }

    #[test]
    fn a_max_age_below_the_window_is_clamped_rather_than_obeyed() {
        let debouncer = Debouncer::new(10_000, 5);
        assert_eq!(
            debouncer.max_age_ms(),
            10_000,
            "a ceiling under the window would silently disable the debounce"
        );
    }

    #[test]
    fn quiet_beats_age_when_both_are_due() {
        let mut debouncer = Debouncer::new(1_000, 1_000);
        debouncer.observe(&root(), &under("a/x.rs"), 0);
        let settled = debouncer.sweep(10_000);
        assert_eq!(
            settled[0].reason,
            SettleReason::Quiet,
            "the reason reported is the honest one: it really had gone quiet"
        );
    }

    #[test]
    fn sweeping_removes_what_it_returns() {
        let mut debouncer = Debouncer::new(100, MAX_AGE);
        debouncer.observe(&root(), &under("a/x.rs"), 0);
        assert_eq!(debouncer.sweep(100).len(), 1);
        assert_eq!(debouncer.pending_len(), 0);
        assert!(
            debouncer.sweep(10_000).is_empty(),
            "a settled directory is handed over exactly once"
        );
    }

    #[test]
    fn independent_directories_settle_independently() {
        let mut debouncer = Debouncer::new(1_000, MAX_AGE);
        debouncer.observe(&root(), &under("early/x.rs"), 0);
        debouncer.observe(&root(), &under("late/x.rs"), 500);

        let first = debouncer.sweep(1_000);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].directory, under("early"));

        let second = debouncer.sweep(1_500);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].directory, under("late"));
    }

    #[test]
    fn ignored_directories_are_dropped_and_counted() {
        let mut debouncer = Debouncer::new(1_000, MAX_AGE);
        for noisy in [
            ".git/index",
            ".git/objects/ab/cdef",
            "target/debug/build.log",
            "node_modules/pkg/m.js",
        ] {
            assert_eq!(
                debouncer.observe(&root(), &under(noisy), 0),
                Observed::Rejected(Rejected::Ignored)
            );
        }
        assert_eq!(
            debouncer.pending_len(),
            0,
            "a git commit must not buy a directory walk"
        );
        assert_eq!(debouncer.ignored_events(), 4);
    }

    #[test]
    fn ignore_matches_components_not_substrings() {
        assert!(!is_ignored(Path::new("/repo/src/target_types.rs")));
        assert!(!is_ignored(Path::new("/repo/gitignore-notes.md")));
        assert!(is_ignored(Path::new("/repo/target")));
        assert!(is_ignored(Path::new("/repo/crates/core/target/x")));
    }

    #[test]
    fn an_event_outside_its_root_is_refused_without_counting_as_noise() {
        let mut debouncer = Debouncer::new(1_000, MAX_AGE);
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
    fn unwatching_a_root_drops_only_its_pending_directories() {
        let mut debouncer = Debouncer::new(WINDOW, MAX_AGE);
        let other = PathBuf::from("/other");
        debouncer.observe(&root(), &under("a/x.rs"), 0);
        debouncer.observe(&root(), &under("b/x.rs"), 0);
        debouncer.observe(&other, &other.join("c/x.rs"), 0);

        assert_eq!(debouncer.forget_root(&root()), 2);
        assert_eq!(debouncer.pending_len(), 1);
        assert!(
            debouncer.sweep(WINDOW * 10).iter().all(|s| s.root == other),
            "a removed root must not hand out work after removal"
        );
    }

    fn settled_at(directory: &str) -> Settled {
        Settled {
            directory: PathBuf::from(directory),
            root: root(),
            first_event_ms: 0,
            last_event_ms: 0,
            settled_at_ms: 0,
            events: 1,
            reason: SettleReason::Quiet,
        }
    }

    #[test]
    fn a_nested_directory_is_dropped_because_its_ancestor_covers_it() {
        let pruned = prune_nested(vec![
            settled_at("/repo/src"),
            settled_at("/repo/src/deep"),
            settled_at("/repo/src/deep/deeper"),
            settled_at("/repo/tests"),
        ]);
        let kept: Vec<&Path> = pruned.iter().map(|s| s.directory.as_path()).collect();
        assert_eq!(
            kept,
            vec![Path::new("/repo/src"), Path::new("/repo/tests")],
            "discover walks recursively, so an ancestor's walk already lists the rest"
        );
    }

    #[test]
    fn a_sibling_sharing_a_textual_prefix_is_not_nested() {
        // The bug every string-prefix implementation has: "/repo/srcx" starts
        // with the TEXT "/repo/src" but is not inside that DIRECTORY.
        let pruned = prune_nested(vec![settled_at("/repo/src"), settled_at("/repo/srcx")]);
        assert_eq!(pruned.len(), 2);
    }

    #[test]
    fn pruning_keeps_directories_from_different_roots_apart() {
        let mut a = settled_at("/a/deep");
        a.root = PathBuf::from("/a");
        let mut b = settled_at("/b");
        b.root = PathBuf::from("/b");
        let pruned = prune_nested(vec![a, b]);
        assert_eq!(pruned.len(), 2, "unrelated roots never cover each other");
    }

    #[test]
    fn normalize_flattens_dot_and_dotdot_without_touching_the_disk() {
        assert_eq!(normalize(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
        assert_eq!(normalize(Path::new("a/b/")), PathBuf::from("a/b"));
        assert_eq!(
            normalize(Path::new("../x")),
            PathBuf::from("../x"),
            "a leading .. has nothing to pop and is kept"
        );
    }
}
