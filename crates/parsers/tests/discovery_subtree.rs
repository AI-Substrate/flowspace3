//! `discover_subtree` — one event's directory, judged by the whole root's rules.
//!
//! The watcher re-lists the directory a filesystem event named, because
//! re-walking the worktree per event is the cost its debouncer exists to avoid.
//! Walking that directory as its own ROOT silently changes the answer: every
//! directory-shaped refusal — a trailing-slash `.gitignore` pattern, the hidden
//! filter, the `.git` refusal, the standard deny list — is decided when the
//! walker is offered the DIRECTORY entry, and a walk that starts below one is
//! never offered it. Measured on a live daemon before this existed: 886
//! gitignored files indexed from a single event, 4,436 vectors bought for them,
//! all of it reaped again by the next full walk.
//!
//! Every test here therefore states its claim as an agreement: whatever
//! `discover` says about the whole root, `discover_subtree` says about the part
//! of it the caller asked about. Trees are built in a temp directory because
//! git cannot track a fixture whose whole point is being git-ignored.

use std::fs;
use std::path::{Path, PathBuf};

use fs3_parsers::discovery::{DiscoverySettings, discover, discover_subtree};

/// A tree this test owns, named for the test that built it.
fn tree(label: &str) -> PathBuf {
    let tree = std::env::temp_dir().join(format!(
        "fs3-subtree-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id(),
    ));
    let _ = fs::remove_dir_all(&tree);
    tree
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("a file has a parent")).expect("temp tree");
    fs::write(path, contents).expect("writing a fixture file");
}

/// The worktree shape the defect was measured in: an ignored tree, a hidden
/// tree, and real source beside them.
fn built(label: &str) -> PathBuf {
    let root = tree(label);
    write(&root, ".gitignore", "scratch/\n");
    write(&root, "src/main.rs", "fn main() {}\n");
    write(&root, "scratch/old/notes.md", "# scratch\n");
    write(&root, ".claude/agent.md", "# hidden\n");
    write(&root, "node_modules/pkg/index.js", "module.exports = {};\n");
    root
}

fn paths(discovery: &fs3_parsers::discovery::Discovery) -> Vec<String> {
    discovery.files.iter().map(|f| f.path.clone()).collect()
}

/// The headline. `scratch/` is a DIRECTORY pattern and the event lands one
/// level below it, which is exactly the case a per-file gitignore match cannot
/// catch: `scratch/old/notes.md` does not match `scratch/`, and never will.
#[test]
fn a_directory_inside_a_gitignored_tree_is_not_walked_at_all() {
    let root = built("gitignored");
    let settings = DiscoverySettings::default();

    let subtree = discover_subtree(&root, &root.join("scratch/old"), &settings)
        .expect("the walk runs")
        .is_none();
    let whole = paths(&discover(&root, &settings).expect("the root walks"));
    fs::remove_dir_all(&root).expect("cleanup");

    assert!(
        subtree,
        "a walk from the root would never have descended into scratch/old"
    );
    assert_eq!(
        whole,
        ["src/main.rs"],
        "and the root walk agrees — that is the answer the subtree walk has to \
         give"
    );
}

/// The half that is not gitignore at all, and the half the original root-cause
/// write-up missed: `.claude/` and `.harness/` were 92 of the polluted rows on
/// a live daemon, refused by the hidden filter rather than by any ignore file.
#[test]
fn a_hidden_directory_is_not_walked_either() {
    let root = built("hidden");
    let settings = DiscoverySettings::default();

    let refused = discover_subtree(&root, &root.join(".claude"), &settings)
        .expect("the walk runs")
        .is_none();
    fs::remove_dir_all(&root).expect("cleanup");

    assert!(
        refused,
        "the hidden filter prunes it when the root is walked"
    );
}

/// And the deny list, which holds whether or not the repo has a `.gitignore`.
#[test]
fn a_denied_directory_is_not_walked_either() {
    let root = built("denied");
    let settings = DiscoverySettings::default();

    let refused = discover_subtree(&root, &root.join("node_modules/pkg"), &settings)
        .expect("the walk runs")
        .is_none();
    fs::remove_dir_all(&root).expect("cleanup");

    assert!(refused, "standard_ignores prunes node_modules by name");
}

/// The other direction, and the one a fix that simply refused everything would
/// fail: an indexed directory still yields its files, keyed relative to the
/// ROOT rather than to itself.
///
/// The path shape is the contract the store depends on — `worktree_files` keys
/// on worktree-relative paths — so a subtree result that reported `main.rs`
/// would map the file to the wrong place while looking perfectly healthy.
#[test]
fn an_indexed_directory_yields_its_files_keyed_from_the_root() {
    let root = built("indexed");
    let settings = DiscoverySettings::default();

    let subtree = discover_subtree(&root, &root.join("src"), &settings)
        .expect("the walk runs")
        .expect("src is walked");
    fs::remove_dir_all(&root).expect("cleanup");

    assert_eq!(paths(&subtree), ["src/main.rs"]);
}

/// A subtree walk must never see a sibling. This is what makes it cost a
/// subtree rather than a root walk, and a restriction that leaked would make
/// the watcher re-list the whole worktree on every event.
#[test]
fn a_subtree_walk_reports_nothing_outside_the_subtree() {
    let root = tree("siblings");
    write(&root, "src/main.rs", "fn main() {}\n");
    write(&root, "src2/other.rs", "fn other() {}\n");
    write(&root, "top.rs", "fn top() {}\n");
    let settings = DiscoverySettings::default();

    let subtree = discover_subtree(&root, &root.join("src"), &settings)
        .expect("the walk runs")
        .expect("src is walked");
    fs::remove_dir_all(&root).expect("cleanup");

    // `src2` is the classic string-prefix trap: it starts with `src` and is not
    // under it.
    assert_eq!(paths(&subtree), ["src/main.rs"]);
}

/// The whole root is still a legal subtree of itself, and must answer exactly
/// what `discover` answers — otherwise the watcher's root-level events take a
/// different code path from `add`.
#[test]
fn the_root_is_its_own_subtree() {
    let root = built("root");
    let settings = DiscoverySettings::default();

    let subtree = discover_subtree(&root, &root, &settings)
        .expect("the walk runs")
        .expect("the root is walked");
    let whole = discover(&root, &settings).expect("the root walks");
    fs::remove_dir_all(&root).expect("cleanup");

    assert_eq!(subtree, whole);
}

/// `force_include` reaches into a gitignored tree for the whole-root walk, so
/// it has to reach there for the subtree walk too. A reachability check that
/// only asked the default pass would silently turn the feature off for every
/// watcher event — the config would still be honoured by `add` and quietly
/// ignored by the daemon that runs all day.
#[test]
fn a_force_included_subtree_is_still_reached() {
    let root = built("forced");
    let settings = DiscoverySettings {
        force_include: vec!["scratch/**".into()],
        ..DiscoverySettings::default()
    };

    let subtree = discover_subtree(&root, &root.join("scratch/old"), &settings)
        .expect("the walk runs")
        .expect("the repo insisted on this tree");
    let whole = paths(&discover(&root, &settings).expect("the root walks"));
    fs::remove_dir_all(&root).expect("cleanup");

    assert_eq!(paths(&subtree), ["scratch/old/notes.md"]);
    assert!(
        whole.contains(&"scratch/old/notes.md".to_string()),
        "and the root walk agrees: {whole:?}"
    );
}

/// A directory outside the root is not part of that root's walk, and saying so
/// is the honest answer — a walk of somebody else's tree is the one outcome
/// that must never happen by accident.
#[test]
fn a_directory_outside_the_root_is_not_reached() {
    let root = tree("outside");
    write(&root, "inside/main.rs", "fn main() {}\n");
    let stranger = tree("outside-stranger");
    write(&stranger, "main.rs", "fn main() {}\n");

    let answer =
        discover_subtree(&root, &stranger, &DiscoverySettings::default()).expect("the walk runs");
    fs::remove_dir_all(&root).expect("cleanup");
    fs::remove_dir_all(&stranger).expect("cleanup");

    assert!(answer.is_none());
}

/// A directory that has been deleted is an ERROR rather than an empty answer,
/// and the watcher depends on the difference: `NotADirectory` is its
/// everyday delete-or-rename path, while `None` means "indexed content lives
/// nowhere near here".
#[test]
fn a_vanished_directory_is_reported_as_such() {
    let root = built("vanished");
    let error = discover_subtree(&root, &root.join("src/gone"), &DiscoverySettings::default())
        .expect_err("a missing directory is not a walk");
    fs::remove_dir_all(&root).expect("cleanup");

    assert!(
        matches!(
            error,
            fs3_parsers::discovery::DiscoveryError::NotADirectory(_)
        ),
        "got {error:?}"
    );
}
