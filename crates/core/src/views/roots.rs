//! What `add` and `scan` answer with.

use serde::{Deserialize, Serialize};

/// What `POST /roots` and `POST /scan` answer with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootReport {
    /// The repository identity the root keys to (PRD req 35).
    pub identity: String,
    /// How the identity was derived — `remote` or `path`.
    pub identity_source: String,
    /// The absolute root path, as registered.
    pub root_path: String,
    /// The worktree row id.
    pub worktree_id: i64,
    /// Whether dot-prefixed directories are enabled for this root.
    #[serde(default)]
    pub include_hidden: bool,
    /// Files discovery accepted.
    pub files: usize,
    /// Files discovery saw and refused, by reason (PRD req 43: never a silent
    /// gap).
    pub skipped: Vec<SkipCount>,
    /// Directories discovery refused to walk at all, named individually.
    ///
    /// Unaggregated on purpose, unlike [`RootReport::skipped`]: there are
    /// about eleven of these on a real repository and the names ARE the
    /// answer to "why is my code missing", where thousands of file rows would
    /// only be a summary of it.
    pub pruned: Vec<PrunedDirectoryRow>,
    /// Scan jobs enqueued by this call.
    pub enqueued: usize,
    /// Files whose bytes are unchanged since the last scan, so no job was
    /// queued. Zero on a first add; on a re-scan of an untouched tree this is
    /// every file, which is the idempotence claim made visible.
    pub unchanged: usize,
    /// Paths that were registered before and are no longer on disk.
    pub removed: u64,
}

/// One skip reason and how many files hit it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkipCount {
    /// `unsupported-extension`, `config-format`, `too-large`, …
    pub reason: String,
    /// How many files.
    pub count: usize,
}

/// One directory discovery never walked.
///
/// A wire type rather than `fs3_parsers::discovery::PrunedDirectory` for the
/// same reason [`SkipCount`] is one: `fs3-parsers` has no serde dependency and
/// does not need one to say what it found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrunedDirectoryRow {
    /// Relative to the root, `/`-separated.
    pub path: String,
    /// Why — `standard-ignore` today.
    pub reason: String,
    /// What to do about it, when the answer is not "nothing".
    ///
    /// Names `scan.standard_ignores = false` and nothing else: `force_include`
    /// would be the better answer per directory, and it has no `[scan]` key
    /// yet, so a diagnostic pointing there would prescribe a line that cannot
    /// be typed into a config file.
    pub fix: String,
}
