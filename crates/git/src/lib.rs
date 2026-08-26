//! The git front door of the incremental pipeline (PRD reqs 5, 35, 41).
//!
//! Three functions, all of them "read git and hand back a value":
//!
//! * [`repo_identity`] — what to call this repository, so clones and worktrees
//!   of the same thing share one identity and therefore share derived content.
//! * [`snapshot`] — what the worktree looks like right now, as `path -> blob id`.
//! * [`fs3_core::diff`] — the pure set difference between two snapshots. It
//!   lives in the core, because it is arithmetic, not IO.
//!
//! ## Why the blob ids are computed, not read
//!
//! A snapshot describes the bytes **on disk**, which is what an indexer has to
//! parse — not the bytes in `HEAD`, and not the bytes that were staged. So every
//! discoverable file is hashed with git's own blob rule
//! (`sha1("blob " + len + "\0" + content)`), which makes an untracked file's id
//! identical to what `git hash-object` would print and lets a brand-new file
//! index without `git add` (PRD req 41).
//!
//! The alternative — trusting the index's recorded blob id when the file's
//! `stat` still matches — is git's own fast path, and it is the named next
//! optimisation. It is deliberately not here yet: a mishandled racy timestamp
//! returns a *stale* id, and a stale id is silent staleness in the store, which
//! is a far worse failure than reading bytes.
//!
//! ## What this crate is not
//!
//! * Not a port. Git is a requirement, not a variable (workshop 001 rule 3), so
//!   there is no trait and nothing to fake — the tests build real repositories.
//! * Not the discovery filter. The file set here is git's answer (tracked plus
//!   untracked-but-not-ignored); the extension allow-list, size ceiling and
//!   force-includes of PRD reqs 41/43 filter it downstream.
//! * Not the non-git path. A plain folder has no snapshot to take — it indexes
//!   by content hash (PRD req 23). [`repo_identity`] still answers for it.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use fs3_core::element::BlobRef;
use fs3_core::git::{RepoIdentity, TreeSnapshot};
use gix::bstr::ByteSlice;

mod error;

pub use error::Error;
/// This crate's result type. The error parameter is defaulted, so a caller that
/// needs a different one keeps the alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Nothing here is interruptible from the outside yet; a shared `false` avoids
/// allocating one per hashed file.
static NEVER_INTERRUPT: AtomicBool = AtomicBool::new(false);

/// Identify the repository containing `path` (PRD req 35).
///
/// The remote URL is the primary key: every clone and every worktree of the same
/// repository derives the same one, which is what lets two worktrees share
/// indexed content. `origin` wins when several remotes exist, because that is
/// what git itself defaults to; otherwise the sole remote is used.
///
/// Falls back to a path key when there is no remote to ask — and when `path` is
/// not in a git repository at all, which is the plain-folder case of PRD req 23.
///
/// # Errors
/// Only IO failures: a path that cannot be resolved to a real location.
pub fn repo_identity(path: &Path) -> Result<RepoIdentity> {
    match gix::discover(path) {
        Ok(repo) => identity_of(&repo),
        Err(_) => Ok(RepoIdentity::from_path(&realpath(path)?)),
    }
}

/// Snapshot the worktree containing `path` as `repo-relative path -> blob id`
/// (PRD reqs 5, 41).
///
/// The file set is git's: every index entry that still exists on disk, plus
/// every untracked file `.gitignore` does not exclude. Directories, symlinks and
/// submodules are not files to parse and are left out. Each id is computed from
/// the current bytes, so a modified-but-unstaged file reports its *worktree*
/// blob and an untracked file reports a real one.
///
/// # Errors
/// [`Error::NotAWorktree`] when `path` is not inside a git worktree (a bare
/// repository included — there is nothing on disk to index). Otherwise IO and
/// git plumbing failures, each naming the file it failed on.
pub fn snapshot(path: &Path) -> Result<TreeSnapshot> {
    let repo = gix::discover(path).map_err(|source| Error::NotAWorktree {
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| Error::BareRepository {
            path: path.to_path_buf(),
        })?
        .to_path_buf();

    let identity = identity_of(&repo)?;
    // An unborn branch (fresh `git init`) has no commit, and that is not an
    // error: the worktree is still full of files to index.
    let commit = repo
        .head_commit()
        .ok()
        .map(|commit| commit.id().to_string());

    let index = repo.index_or_empty()?;
    let mut files = BTreeMap::new();

    // Tracked. Index order is already sorted by path, and stage-0 entries are
    // the resolved ones — a conflicted path has no single content to index.
    for entry in index.entries() {
        if entry.stage() != gix::index::entry::Stage::Unconflicted
            || entry.mode != gix::index::entry::Mode::FILE
                && entry.mode != gix::index::entry::Mode::FILE_EXECUTABLE
        {
            continue;
        }
        let Some(rela_path) = to_utf8_path(entry.path(&index)) else {
            continue;
        };
        insert_if_regular_file(&mut files, &workdir, rela_path)?;
    }

    // Untracked but not ignored. Asking gix for the walk keeps `.gitignore`
    // semantics — nested ignore files, negations, `core.excludesFile` — as git's
    // problem rather than fs3's.
    let mut walk_options = repo.dirwalk_options()?;
    walk_options = walk_options
        .emit_untracked(gix::dir::walk::EmissionMode::Matching)
        .emit_ignored(None)
        .emit_tracked(false)
        .emit_empty_directories(false)
        .emit_pruned(false)
        .recurse_repositories(false);

    let mut collect = gix::dir::walk::delegate::Collect::default();
    repo.dirwalk(
        &index,
        Vec::<gix::bstr::BString>::new(),
        &NEVER_INTERRUPT,
        walk_options,
        &mut collect,
    )?;

    for (entry, _) in collect.into_entries_by_path() {
        if entry.status != gix::dir::entry::Status::Untracked {
            continue;
        }
        let Some(rela_path) = to_utf8_path(entry.rela_path.as_bstr()) else {
            continue;
        };
        insert_if_regular_file(&mut files, &workdir, rela_path)?;
    }

    Ok(TreeSnapshot::new(identity, commit, files))
}

/// Hash a file the way `git hash-object` would.
///
/// Streamed rather than read whole: a snapshot has no size ceiling of its own
/// (that filter belongs to discovery), so one huge file must not become one huge
/// allocation.
///
/// # Errors
/// IO failures, each naming the file.
pub fn blob_id(file: &Path) -> Result<BlobRef> {
    let mut reader = std::fs::File::open(file).map_err(|source| Error::Io {
        path: file.to_path_buf(),
        source,
    })?;
    let len = reader
        .metadata()
        .map_err(|source| Error::Io {
            path: file.to_path_buf(),
            source,
        })?
        .len();

    let id = gix::objs::compute_stream_hash(
        gix::hash::Kind::Sha1,
        gix::object::Kind::Blob,
        &mut reader,
        len,
        &mut gix::progress::Discard,
        &NEVER_INTERRUPT,
    )
    .map_err(|source| Error::Hash {
        path: file.to_path_buf(),
        source,
    })?;

    BlobRef::new(id.to_string()).map_err(Error::Core)
}

/// Add `rela_path` to `files` when it is a regular file that still exists.
///
/// A missing path is not an error: an index entry whose file was deleted, or a
/// walk entry raced by a `rm`, simply is not part of the worktree. Symlinks are
/// skipped — git stores their target as a blob, but there is no source in that
/// to parse, and following them would double-count the target.
fn insert_if_regular_file(
    files: &mut BTreeMap<String, BlobRef>,
    workdir: &Path,
    rela_path: String,
) -> Result<()> {
    if files.contains_key(&rela_path) {
        return Ok(());
    }
    let absolute = workdir.join(&rela_path);
    match std::fs::symlink_metadata(&absolute) {
        Ok(meta) if meta.is_file() => {
            files.insert(rela_path, blob_id(&absolute)?);
            Ok(())
        }
        Ok(_) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::Io {
            path: absolute,
            source,
        }),
    }
}

/// The identity of an open repository: remote first, path second.
fn identity_of(repo: &gix::Repository) -> Result<RepoIdentity> {
    let remote = repo.find_remote("origin").ok().or_else(|| {
        repo.find_default_remote(gix::remote::Direction::Fetch)?
            .ok()
    });

    let from_remote = remote
        .as_ref()
        .and_then(|remote| remote.url(gix::remote::Direction::Fetch))
        .and_then(|url| RepoIdentity::from_remote_parts(url.host(), &url.path.to_str_lossy()));
    if let Some(identity) = from_remote {
        return Ok(identity);
    }

    // No remote, or a remote whose URL carries no identity at all: key on where
    // the repository lives. Worktrees of a remoteless repository are distinct
    // by this key, which is the honest answer — they have nothing shared to
    // point at.
    let root = repo.workdir().unwrap_or_else(|| repo.git_dir());
    Ok(RepoIdentity::from_path(&realpath(root)?))
}

/// Resolve a path to its real, absolute location — symlinks and `..` included —
/// so two spellings of one directory produce one key.
fn realpath(path: &Path) -> Result<std::path::PathBuf> {
    gix::path::realpath(path).map_err(|source| Error::Realpath {
        path: path.to_path_buf(),
        source,
    })
}

/// Repository-relative paths are `/`-separated already; a path that is not UTF-8
/// cannot be a key in the store, so it is skipped rather than lossily renamed.
fn to_utf8_path(path: &gix::bstr::BStr) -> Option<String> {
    path.to_str().ok().map(ToString::to_string)
}
