//! Repo identity and blob-keyed change detection — the pure half (PRD reqs 5, 35).
//!
//! fs3's incremental story is "derived data is keyed by git blob SHA". This
//! module holds the *values* that story is told in — what a repository is
//! called, what a worktree looked like at one instant, and what changed between
//! two such instants — with zero IO. Reading git is [`fs3-git`]'s job; deciding
//! what the bytes mean is this module's.
//!
//! The split matters because the interesting logic is the boring logic: the key
//! normalisation that makes a clone and a worktree agree on one identity, and
//! the set difference that decides which blobs pay for parse/summarise/embed.
//! Both are pure functions over data, so both are unit-testable without a repo
//! on disk.
//!
//! [`fs3-git`]: https://docs.rs/fs3-git

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::element::BlobRef;
use crate::error::{Error, Result};

/// How a repository's identity was derived (PRD req 35).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentitySource {
    /// From the repository's remote URL — the preferred key, because every
    /// clone and every worktree of the same repository derives the same one.
    Remote,
    /// From the folder's path, because there was no remote to ask (a local-only
    /// repository, or PRD req 23's plain non-git folder).
    Path,
}

/// The primary key of a repository (PRD req 35).
///
/// A repository is keyed by its remote URL when it has one, so clones and
/// worktrees of the same repository share an identity — and therefore share
/// derived content in the store. Remoteless repositories and plain folders fall
/// back to a path key, which is deterministic but machine-local by nature.
///
/// The key is prefixed by its source (`git:` / `path:`) so the two key spaces
/// can never collide in a column that holds both.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RepoIdentity {
    key: String,
    source: IdentitySource,
}

impl RepoIdentity {
    /// Build a remote-derived identity from a parsed URL's `host` and `path`.
    ///
    /// Parsing the URL is the caller's job (fs3-git hands over what `gix` read),
    /// which keeps a URL parser out of the functional core. What happens here is
    /// the *canonicalisation* every transport spelling has to survive:
    ///
    /// ```text
    /// git@github.com:AI-Substrate/flowspace3.git  ->  git:github.com/AI-Substrate/flowspace3
    /// https://GitHub.com/AI-Substrate/flowspace3/ ->  git:github.com/AI-Substrate/flowspace3
    /// /srv/git/flowspace3.git                     ->  git:/srv/git/flowspace3
    /// ```
    ///
    /// The host is lowercased (DNS is case-insensitive); the path is not, because
    /// on most forges it is not. A `.git` suffix and surrounding slashes are
    /// noise and are dropped.
    ///
    /// Returns `None` when the URL carries neither host nor path — there is no
    /// key in that, and a blank key would silently merge unrelated repositories.
    pub fn from_remote_parts(host: Option<&str>, path: &str) -> Option<Self> {
        let host = host.unwrap_or_default().trim().to_ascii_lowercase();
        let path = normalise_remote_path(path);
        // With a host, the path names a repository *on* that host, so its
        // leading slash is punctuation. Without one, the path IS the location —
        // a local or `file://` remote — and dropping the slash would turn an
        // absolute path into a relative one.
        let path = if host.is_empty() {
            path.as_str()
        } else {
            path.trim_start_matches('/')
        };

        let key = match (host.is_empty(), path.is_empty()) {
            (true, true) => return None,
            (true, false) => format!("git:{path}"),
            (false, true) => format!("git:{host}"),
            (false, false) => format!("git:{host}/{path}"),
        };
        Some(RepoIdentity {
            key,
            source: IdentitySource::Remote,
        })
    }

    /// Build the fallback identity from an **already absolute** path.
    ///
    /// Resolving symlinks and relative segments is IO, so it happens in fs3-git;
    /// this function only normalises the spelling — separators to `/`, no
    /// trailing slash — so the same directory always produces the same key.
    pub fn from_path(root: &Path) -> Self {
        let text = root.to_string_lossy().replace('\\', "/");
        let trimmed = text.trim_end_matches('/');
        let body = if trimmed.is_empty() { &text } else { trimmed };
        RepoIdentity {
            key: format!("path:{body}"),
            source: IdentitySource::Path,
        }
    }

    /// The key as stored: `git:host/path` or `path:/abs/path`.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Where the key came from.
    pub fn source(&self) -> IdentitySource {
        self.source
    }
}

impl std::fmt::Display for RepoIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.key)
    }
}

/// Strip a remote path down to its identifying part: no trailing slash, no
/// `.git` suffix, `\` normalised to `/` so a Windows-spelled local remote and
/// its POSIX twin agree. A leading slash is left for the caller to judge.
fn normalise_remote_path(path: &str) -> String {
    let path = path.trim().replace('\\', "/");
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    path.trim_end_matches('/').to_string()
}

/// What one worktree looked like at one instant, keyed the way fs3 indexes.
///
/// [`Self::files`] maps repository-relative path (always `/`-separated) to the
/// git blob id of the bytes **on disk**, not the bytes in HEAD: a file modified
/// but not staged reports the id its current content would get from
/// `git hash-object`, and an untracked-but-not-ignored file appears with a real
/// blob id rather than not at all (PRD req 41 — new files index without
/// `git add`).
///
/// The map is ordered, so a snapshot has one canonical serialisation and tests
/// can assert it whole.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeSnapshot {
    /// Which repository this is a snapshot of.
    pub identity: RepoIdentity,
    /// The `HEAD` commit id, or `None` on an unborn branch.
    ///
    /// Provenance only: nothing is keyed by it. Keying by commit would make
    /// every commit a full re-index, which is exactly what blob keying avoids.
    pub commit: Option<String>,
    /// Repository-relative path → blob id of the content on disk.
    pub files: BTreeMap<String, BlobRef>,
}

impl TreeSnapshot {
    /// Assemble a snapshot from its parts.
    pub fn new(
        identity: RepoIdentity,
        commit: Option<String>,
        files: BTreeMap<String, BlobRef>,
    ) -> Self {
        TreeSnapshot {
            identity,
            commit,
            files,
        }
    }

    /// An empty snapshot of `identity` — the "nothing indexed yet" state, whose
    /// [`diff`] against a real snapshot is a full index.
    pub fn empty(identity: RepoIdentity) -> Self {
        TreeSnapshot {
            identity,
            commit: None,
            files: BTreeMap::new(),
        }
    }

    /// Number of files in the snapshot.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether the snapshot holds no files at all.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// A path and the blob its content hashes to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileBlob {
    /// Repository-relative, `/`-separated.
    pub path: String,
    /// The blob id of the file's content.
    pub blob: BlobRef,
}

/// A path whose content changed, with both blob ids.
///
/// `before` is kept because it is the store key of the derived rows that just
/// went stale — the caller needs it to retire them, not just to log it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobChange {
    /// Repository-relative, `/`-separated.
    pub path: String,
    /// The blob id the old snapshot had.
    pub before: BlobRef,
    /// The blob id the new snapshot has.
    pub after: BlobRef,
}

/// The work one snapshot-to-snapshot transition implies.
///
/// Every vector is sorted by path, because [`diff`] walks two ordered maps in
/// lockstep — determinism is free here, so tests get to assert exact sets.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedSet {
    /// Paths present in the new snapshot only — parse, summarise, embed.
    pub added: Vec<FileBlob>,
    /// Paths in both, with different blobs — re-index; retire `before`.
    pub modified: Vec<BlobChange>,
    /// Paths present in the old snapshot only — retire their derived rows.
    pub removed: Vec<FileBlob>,
}

impl ChangedSet {
    /// Whether anything at all changed.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.removed.is_empty()
    }

    /// Total number of changed paths across all three kinds.
    pub fn len(&self) -> usize {
        self.added.len() + self.modified.len() + self.removed.len()
    }
}

/// Diff two snapshots by blob id (PRD req 5).
///
/// A path whose blob id is unchanged is *not* reported, whatever its mtime,
/// whatever the commit — that identity is the whole point of blob keying, and
/// it is what makes creating a worktree cost nothing.
///
/// Both maps are ordered, so this is a single linear merge walk: no hashing, no
/// intermediate set, one allocation per reported change.
///
/// # Errors
/// Returns [`Error::SnapshotMismatch`] when the snapshots describe different
/// repositories. Diffing across repositories would report every file as both
/// added and removed — a plausible caller mistake that must not look like work.
pub fn diff(old: &TreeSnapshot, new: &TreeSnapshot) -> Result<ChangedSet> {
    if old.identity != new.identity {
        return Err(Error::SnapshotMismatch {
            old: old.identity.key().to_string(),
            new: new.identity.key().to_string(),
        });
    }

    let mut changed = ChangedSet::default();
    let mut olds = old.files.iter().peekable();
    let mut news = new.files.iter().peekable();

    loop {
        match (olds.peek(), news.peek()) {
            (Some((old_path, old_blob)), Some((new_path, new_blob))) => {
                match old_path.cmp(new_path) {
                    std::cmp::Ordering::Equal => {
                        if old_blob != new_blob {
                            changed.modified.push(BlobChange {
                                path: (*new_path).clone(),
                                before: (*old_blob).clone(),
                                after: (*new_blob).clone(),
                            });
                        }
                        olds.next();
                        news.next();
                    }
                    std::cmp::Ordering::Less => {
                        changed.removed.push(FileBlob {
                            path: (*old_path).clone(),
                            blob: (*old_blob).clone(),
                        });
                        olds.next();
                    }
                    std::cmp::Ordering::Greater => {
                        changed.added.push(FileBlob {
                            path: (*new_path).clone(),
                            blob: (*new_blob).clone(),
                        });
                        news.next();
                    }
                }
            }
            (Some((old_path, old_blob)), None) => {
                changed.removed.push(FileBlob {
                    path: (*old_path).clone(),
                    blob: (*old_blob).clone(),
                });
                olds.next();
            }
            (None, Some((new_path, new_blob))) => {
                changed.added.push(FileBlob {
                    path: (*new_path).clone(),
                    blob: (*new_blob).clone(),
                });
                news.next();
            }
            (None, None) => break,
        }
    }

    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blob(hex: &str) -> BlobRef {
        BlobRef::new(hex).expect("test digest is well formed")
    }

    fn snapshot(identity: &RepoIdentity, files: &[(&str, &str)]) -> TreeSnapshot {
        TreeSnapshot::new(
            identity.clone(),
            None,
            files
                .iter()
                .map(|(path, hex)| ((*path).to_string(), blob(hex)))
                .collect(),
        )
    }

    fn identity() -> RepoIdentity {
        RepoIdentity::from_remote_parts(Some("github.com"), "/AI-Substrate/flowspace3.git")
            .expect("host and path are present")
    }

    #[test]
    fn every_transport_spelling_of_one_remote_yields_one_key() {
        let ssh =
            RepoIdentity::from_remote_parts(Some("github.com"), "AI-Substrate/flowspace3.git");
        let https =
            RepoIdentity::from_remote_parts(Some("GitHub.com"), "/AI-Substrate/flowspace3/");
        let bare = RepoIdentity::from_remote_parts(Some("github.com"), "/AI-Substrate/flowspace3");

        assert_eq!(ssh, https);
        assert_eq!(https, bare);
        assert_eq!(
            ssh.expect("parsed").key(),
            "git:github.com/AI-Substrate/flowspace3"
        );
    }

    #[test]
    fn a_hostless_remote_keys_on_its_path_alone() {
        let identity =
            RepoIdentity::from_remote_parts(None, "/srv/git/flowspace3.git").expect("path present");
        assert_eq!(identity.key(), "git:/srv/git/flowspace3");
        assert_eq!(identity.source(), IdentitySource::Remote);
    }

    #[test]
    fn a_remote_with_neither_host_nor_path_is_no_identity_at_all() {
        assert_eq!(RepoIdentity::from_remote_parts(None, "   "), None);
        assert_eq!(RepoIdentity::from_remote_parts(Some(""), "/"), None);
    }

    #[test]
    fn path_identities_are_prefixed_and_slash_normalised() {
        let posix = RepoIdentity::from_path(Path::new("/Users/j/code/fs3/"));
        assert_eq!(posix.key(), "path:/Users/j/code/fs3");
        assert_eq!(posix.source(), IdentitySource::Path);

        let windows = RepoIdentity::from_path(Path::new(r"C:\code\fs3"));
        assert_eq!(windows.key(), "path:C:/code/fs3");
    }

    #[test]
    fn the_two_key_spaces_cannot_collide() {
        let remote = RepoIdentity::from_remote_parts(None, "/srv/git/fs3").expect("path present");
        let local = RepoIdentity::from_path(Path::new("/srv/git/fs3"));
        assert_ne!(remote, local);
        assert_ne!(remote.key(), local.key());
    }

    #[test]
    fn an_unchanged_blob_is_not_work_however_the_snapshots_were_taken() {
        let id = identity();
        let mut old = snapshot(&id, &[("src/lib.rs", "aaaaaaa")]);
        let mut new = snapshot(&id, &[("src/lib.rs", "aaaaaaa")]);
        old.commit = Some("1111111111111111111111111111111111111111".into());
        new.commit = Some("2222222222222222222222222222222222222222".into());

        let changed = diff(&old, &new).expect("same repository");

        assert!(changed.is_empty(), "same blob, different commit: no work");
        assert_eq!(changed.len(), 0);
    }

    #[test]
    fn added_modified_and_removed_are_reported_separately_and_in_path_order() {
        let id = identity();
        let old = snapshot(
            &id,
            &[
                ("a/keep.rs", "1111111"),
                ("b/change.rs", "2222222"),
                ("c/gone.rs", "3333333"),
            ],
        );
        let new = snapshot(
            &id,
            &[
                ("a/keep.rs", "1111111"),
                ("b/change.rs", "4444444"),
                ("d/new.rs", "5555555"),
                ("e/newer.rs", "6666666"),
            ],
        );

        let changed = diff(&old, &new).expect("same repository");

        assert_eq!(
            changed.added,
            vec![
                FileBlob {
                    path: "d/new.rs".into(),
                    blob: blob("5555555")
                },
                FileBlob {
                    path: "e/newer.rs".into(),
                    blob: blob("6666666")
                },
            ]
        );
        assert_eq!(
            changed.modified,
            vec![BlobChange {
                path: "b/change.rs".into(),
                before: blob("2222222"),
                after: blob("4444444"),
            }]
        );
        assert_eq!(
            changed.removed,
            vec![FileBlob {
                path: "c/gone.rs".into(),
                blob: blob("3333333")
            }]
        );
        assert_eq!(changed.len(), 4);
    }

    #[test]
    fn the_first_index_of_a_repository_is_a_diff_against_nothing() {
        let id = identity();
        let new = snapshot(&id, &[("README.md", "abcdef0"), ("src/lib.rs", "abcdef1")]);

        let changed = diff(&TreeSnapshot::empty(id), &new).expect("same repository");

        assert_eq!(changed.added.len(), 2);
        assert!(changed.modified.is_empty() && changed.removed.is_empty());
    }

    #[test]
    fn diffing_across_repositories_is_an_error_not_a_rebuild() {
        let mine = snapshot(&identity(), &[("src/lib.rs", "aaaaaaa")]);
        let theirs = snapshot(
            &RepoIdentity::from_remote_parts(Some("github.com"), "someone/else.git")
                .expect("parsed"),
            &[("src/lib.rs", "bbbbbbb")],
        );

        assert_eq!(
            diff(&mine, &theirs),
            Err(Error::SnapshotMismatch {
                old: "git:github.com/AI-Substrate/flowspace3".into(),
                new: "git:github.com/someone/else".into(),
            })
        );
    }
}
