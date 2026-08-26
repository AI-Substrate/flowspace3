//! The git layer proved against real repositories.
//!
//! No mocks and no fixtures-in-a-can: every test builds a throwaway repository
//! with the `git` binary, then asserts what fs3 reads back. The load-bearing
//! assertion is the one against `git hash-object` — if fs3's blob ids ever stop
//! being git's blob ids, every incremental decision downstream is built on sand.

use std::path::{Path, PathBuf};
use std::process::Command;

use fs3_core::git::{BlobChange, FileBlob, IdentitySource, TreeSnapshot, diff};
use tempfile::TempDir;

/// A throwaway repository on disk.
struct Fixture {
    dir: TempDir,
}

impl Fixture {
    /// `git init` a fresh repository with a deterministic identity and no
    /// dependence on the developer's global git config.
    fn init() -> Self {
        let dir = tempfile::tempdir().expect("a temp dir");
        let fixture = Fixture { dir };
        fixture.git(["init", "--initial-branch=main"]);
        fixture.git(["config", "user.name", "fs3 fixture"]);
        fixture.git(["config", "user.email", "fixture@flowspace3.invalid"]);
        fixture
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Run git in the fixture, failing loudly with git's own message.
    fn git<const N: usize>(&self, args: [&str; N]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(self.root())
            .output()
            .expect("the git binary must be on PATH for these tests");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout)
            .expect("git speaks utf-8 here")
            .trim()
            .to_string()
    }

    fn write(&self, rela_path: &str, content: &str) -> PathBuf {
        let path = self.root().join(rela_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("a writable temp dir");
        }
        std::fs::write(&path, content).expect("a writable temp file");
        path
    }

    fn remove(&self, rela_path: &str) {
        std::fs::remove_file(self.root().join(rela_path)).expect("the file to exist");
    }

    fn commit(&self, message: &str) -> String {
        self.git(["add", "-A"]);
        self.git(["commit", "-m", message]);
        self.git(["rev-parse", "HEAD"])
    }

    /// Git's own answer for a file's blob id — the oracle.
    fn hash_object(&self, rela_path: &str) -> String {
        self.git(["hash-object", rela_path])
    }

    fn snapshot(&self) -> TreeSnapshot {
        fs3_git::snapshot(self.root()).expect("a snapshot of a real worktree")
    }
}

/// Readable assertion helper: the snapshot's blob for `path`, as hex.
fn blob_hex(snapshot: &TreeSnapshot, path: &str) -> String {
    snapshot
        .files
        .get(path)
        .unwrap_or_else(|| panic!("{path} missing from snapshot: {:?}", snapshot.files.keys()))
        .as_str()
        .to_string()
}

#[test]
fn a_committed_file_hashes_to_the_blob_git_recorded() {
    let repo = Fixture::init();
    repo.write("src/lib.rs", "pub fn main() {}\n");
    let commit = repo.commit("initial");

    let snapshot = repo.snapshot();

    assert_eq!(snapshot.commit.as_deref(), Some(commit.as_str()));
    assert_eq!(
        blob_hex(&snapshot, "src/lib.rs"),
        repo.git(["rev-parse", "HEAD:src/lib.rs"]),
        "a clean tracked file must carry the blob id git committed"
    );
    assert_eq!(snapshot.len(), 1, "only the one file: {:?}", snapshot.files);
}

#[test]
fn an_untracked_file_gets_the_id_git_hash_object_would_print() {
    // PRD req 41: a brand-new file indexes without `git add`, and it must key
    // on the same blob id it will have once it is committed.
    let repo = Fixture::init();
    repo.write("README.md", "# fs3\n");
    repo.commit("initial");
    repo.write("src/new_file.rs", "fn brand_new() {}\n");

    let snapshot = repo.snapshot();

    assert_eq!(
        blob_hex(&snapshot, "src/new_file.rs"),
        repo.hash_object("src/new_file.rs"),
        "untracked blob ids must be git's blob ids"
    );

    // And committing it changes nothing about the id — the proof that indexing
    // before `git add` was not wasted work.
    let before = blob_hex(&snapshot, "src/new_file.rs");
    repo.commit("add the new file");
    assert_eq!(blob_hex(&repo.snapshot(), "src/new_file.rs"), before);
}

#[test]
fn a_modified_but_unstaged_file_reports_its_worktree_blob() {
    let repo = Fixture::init();
    repo.write("src/lib.rs", "pub fn main() {}\n");
    repo.commit("initial");
    let committed = repo.snapshot();

    repo.write("src/lib.rs", "pub fn main() { work(); }\n");
    let dirty = repo.snapshot();

    assert_eq!(
        blob_hex(&dirty, "src/lib.rs"),
        repo.hash_object("src/lib.rs"),
        "the snapshot describes the bytes on disk, not the bytes in HEAD"
    );
    assert_ne!(
        blob_hex(&committed, "src/lib.rs"),
        blob_hex(&dirty, "src/lib.rs")
    );

    let changed = diff(&committed, &dirty).expect("same repository");
    assert_eq!(
        changed.modified,
        vec![BlobChange {
            path: "src/lib.rs".into(),
            before: committed.files["src/lib.rs"].clone(),
            after: dirty.files["src/lib.rs"].clone(),
        }]
    );
    assert!(changed.added.is_empty() && changed.removed.is_empty());
}

#[test]
fn ignored_files_and_the_git_dir_never_enter_a_snapshot() {
    let repo = Fixture::init();
    repo.write(".gitignore", "target/\n*.log\n");
    repo.write("src/lib.rs", "pub fn main() {}\n");
    repo.commit("initial");
    repo.write("target/debug/artifact.bin", "not source");
    repo.write("noisy.log", "chatter");

    let snapshot = repo.snapshot();

    assert_eq!(
        snapshot.files.keys().collect::<Vec<_>>(),
        vec![".gitignore", "src/lib.rs"],
        "gitignored paths, and everything under .git, stay out"
    );
}

#[test]
fn added_modified_and_removed_are_exactly_what_changed_between_two_snapshots() {
    let repo = Fixture::init();
    repo.write("keep.rs", "fn keep() {}\n");
    repo.write("change.rs", "fn before() {}\n");
    repo.write("gone.rs", "fn doomed() {}\n");
    repo.commit("initial");
    let before = repo.snapshot();

    repo.write("change.rs", "fn after() {}\n");
    repo.remove("gone.rs");
    repo.write("nested/added.rs", "fn fresh() {}\n");
    let after = repo.snapshot();

    let changed = diff(&before, &after).expect("same repository");

    assert_eq!(
        changed.added,
        vec![FileBlob {
            path: "nested/added.rs".into(),
            blob: after.files["nested/added.rs"].clone(),
        }]
    );
    assert_eq!(changed.modified.len(), 1);
    assert_eq!(changed.modified[0].path, "change.rs");
    assert_eq!(
        changed.removed,
        vec![FileBlob {
            path: "gone.rs".into(),
            blob: before.files["gone.rs"].clone(),
        }]
    );
    assert_eq!(changed.len(), 3);
}

#[test]
fn a_commit_that_changes_no_bytes_is_no_work_at_all() {
    // The point of blob keying (PRD req 5): moving HEAD is not, by itself, a
    // reason to re-parse anything.
    let repo = Fixture::init();
    repo.write("src/lib.rs", "pub fn main() {}\n");
    let first = repo.commit("initial");
    let before = repo.snapshot();

    repo.git(["commit", "--allow-empty", "-m", "empty"]);
    let after = repo.snapshot();

    assert_ne!(after.commit.as_deref(), Some(first.as_str()));
    assert!(
        diff(&before, &after).expect("same repository").is_empty(),
        "same blobs, new commit: nothing to index"
    );
}

#[test]
fn a_repository_with_a_remote_is_keyed_by_that_remote() {
    let repo = Fixture::init();
    repo.write("README.md", "# fs3\n");
    repo.commit("initial");
    repo.git([
        "remote",
        "add",
        "origin",
        "git@github.com:AI-Substrate/flowspace3.git",
    ]);

    let identity = fs3_git::repo_identity(repo.root()).expect("an identity");

    assert_eq!(identity.key(), "git:github.com/AI-Substrate/flowspace3");
    assert_eq!(identity.source(), IdentitySource::Remote);
    assert_eq!(
        repo.snapshot().identity,
        identity,
        "a snapshot carries the same identity the standalone call reports"
    );
}

#[test]
fn two_clones_of_one_remote_share_an_identity_however_the_url_is_spelled() {
    let ssh = Fixture::init();
    ssh.git([
        "remote",
        "add",
        "origin",
        "git@github.com:AI-Substrate/flowspace3.git",
    ]);
    let https = Fixture::init();
    https.git([
        "remote",
        "add",
        "origin",
        "https://github.com/AI-Substrate/flowspace3",
    ]);

    assert_eq!(
        fs3_git::repo_identity(ssh.root()).expect("an identity"),
        fs3_git::repo_identity(https.root()).expect("an identity"),
        "clones of one repository must share derived content"
    );
}

#[test]
fn a_remoteless_repository_falls_back_to_its_path() {
    let repo = Fixture::init();
    repo.write("README.md", "# local only\n");
    repo.commit("initial");

    let identity = fs3_git::repo_identity(repo.root()).expect("an identity");

    assert_eq!(identity.source(), IdentitySource::Path);
    let expected = std::fs::canonicalize(repo.root()).expect("a real path");
    assert_eq!(
        identity.key(),
        format!("path:{}", expected.to_string_lossy().replace('\\', "/")),
        "no remote to ask: the key is where the repository lives"
    );
}

#[test]
fn a_subdirectory_identifies_and_snapshots_the_whole_repository() {
    let repo = Fixture::init();
    repo.write("src/lib.rs", "pub fn main() {}\n");
    repo.commit("initial");
    repo.git([
        "remote",
        "add",
        "origin",
        "https://example.com/team/thing.git",
    ]);

    let nested = repo.root().join("src");
    let identity = fs3_git::repo_identity(&nested).expect("an identity");
    let snapshot = fs3_git::snapshot(&nested).expect("a snapshot");

    assert_eq!(identity.key(), "git:example.com/team/thing");
    assert_eq!(
        snapshot.files.keys().collect::<Vec<_>>(),
        vec!["src/lib.rs"],
        "paths stay relative to the repository root, not the entry directory"
    );
}

#[test]
fn a_fresh_repository_with_no_commit_still_snapshots_its_files() {
    let repo = Fixture::init();
    repo.write("src/lib.rs", "pub fn main() {}\n");

    let snapshot = repo.snapshot();

    assert_eq!(snapshot.commit, None, "an unborn branch has no commit id");
    assert_eq!(
        blob_hex(&snapshot, "src/lib.rs"),
        repo.hash_object("src/lib.rs")
    );
}

#[test]
fn a_plain_folder_has_an_identity_but_no_snapshot() {
    // PRD req 23: git is the optimisation, never the price of entry. Identity
    // still answers; snapshotting says plainly that there is no worktree here.
    let dir = tempfile::tempdir().expect("a temp dir");
    std::fs::write(dir.path().join("notes.md"), "# not a repo\n").expect("a writable temp file");

    let identity = fs3_git::repo_identity(dir.path()).expect("an identity");
    assert_eq!(identity.source(), IdentitySource::Path);

    let err = fs3_git::snapshot(dir.path()).expect_err("no worktree to snapshot");
    assert!(
        matches!(err, fs3_git::Error::NotAWorktree { .. }),
        "unexpected error: {err}"
    );
    assert!(
        err.to_string()
            .contains(&dir.path().to_string_lossy().to_string()),
        "the message must name the path: {err}"
    );
}

#[test]
fn a_linked_worktree_of_the_same_repository_shares_its_identity() {
    // Worktree-friendly by construction (PRD req 5): a second worktree is not a
    // second repository, so it must not be a second index.
    let repo = Fixture::init();
    repo.write("src/lib.rs", "pub fn main() {}\n");
    repo.commit("initial");
    repo.git([
        "remote",
        "add",
        "origin",
        "https://example.com/team/thing.git",
    ]);

    let elsewhere = tempfile::tempdir().expect("a temp dir");
    let linked = elsewhere.path().join("wt");
    repo.git([
        "worktree",
        "add",
        linked.to_str().expect("utf-8 temp path"),
        "-b",
        "side",
    ]);

    let linked_snapshot = fs3_git::snapshot(&linked).expect("a snapshot of the linked worktree");

    assert_eq!(linked_snapshot.identity, repo.snapshot().identity);
    assert_eq!(
        linked_snapshot.files,
        repo.snapshot().files,
        "same content checked out twice is the same blobs, and therefore no work"
    );
}
