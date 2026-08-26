//! The standard deny list — the directories fs3 refuses to walk even when the
//! repository has no `.gitignore` at all.
//!
//! `fixtures/discovery-bare/` deliberately contains **no ignore file of any
//! kind**. Every test here also passes `respect_gitignore: false`, which turns
//! off parent-directory ignore rules too — otherwise this repo's own root
//! `.gitignore` (which lists `target` and `debug`) could explain the result and
//! the test would prove nothing. With git's rules out of the picture entirely,
//! the deny list is the only thing that can account for what is missing.
//!
//! Fixture files under `target/` are committed with `git add -f`, for the same
//! reason.

use std::fs;
use std::path::{Path, PathBuf};

use fs3_parsers::discovery::{
    Discovery, DiscoverySettings, STANDARD_IGNORES, SkipReason, discover,
};

fn bare() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/discovery-bare")
}

/// No git rules at all: only the deny list can explain an absence.
fn ungoverned() -> DiscoverySettings {
    DiscoverySettings {
        respect_gitignore: false,
        ..DiscoverySettings::default()
    }
}

fn found(settings: &DiscoverySettings) -> Discovery {
    discover(&bare(), settings).expect("bare tree walks")
}

fn kept(discovery: &Discovery) -> Vec<&str> {
    discovery.files.iter().map(|f| f.path.as_str()).collect()
}

/// The trap sailfish hit: `node_modules/**/*.js` is real JavaScript, `js` is in
/// the source table, and in a `.gitignore`-less clone nothing else stops it.
#[test]
fn a_repo_with_no_gitignore_still_refuses_the_usual_directories() {
    let discovery = found(&ungoverned());

    assert_eq!(
        kept(&discovery),
        [
            "README.md",
            "builder/main.rs",
            "distribution/notes.md",
            "my-vendor/keep.rs",
            "src/main.rs",
            "src/node_modules_helper.rs",
            "src/target_types.rs",
        ],
    );
    // Pruned directories are out of scope, not refused: a denied `node_modules`
    // must not cost a ledger row per file.
    assert!(
        discovery.skipped.is_empty(),
        "nothing should be refused here, got {:?}",
        discovery.skipped,
    );
}

/// Every name on the list, proven individually — a list is only as good as the
/// least-exercised entry on it.
#[test]
fn every_standard_ignore_name_is_enforced() {
    let discovery = found(&DiscoverySettings {
        // Hidden on, so `.venv`, `.next` and `.cache` are reachable by the
        // walker and can only be missing because the deny list denied them.
        include_hidden: true,
        ..ungoverned()
    });
    let kept = kept(&discovery);

    for denied in [
        ".cache/warm.js",
        ".next/server.js",
        ".venv/lib/site.py",
        "__pycache__/mod.py",
        "build/out.js",
        "dist/bundle.js",
        "node_modules/.bin/tool.js",
        "node_modules/pkg/index.js",
        "target/debug/build_script.rs",
        "vendor/lib.rs",
        "venv/lib/site.py",
    ] {
        assert!(!kept.contains(&denied), "{denied} was indexed");
    }
    // Turning hidden files on must not smuggle a denied directory back in.
    assert_eq!(kept.len(), 7, "unexpected extra files: {kept:?}");
}

/// Whole path components, never substrings. Four separate ways to get this
/// wrong, all pinned.
#[test]
fn the_deny_list_matches_components_not_substrings() {
    let discovery = found(&ungoverned());
    let kept = kept(&discovery);

    for survivor in [
        "src/target_types.rs",        // file whose NAME contains a denied name
        "src/node_modules_helper.rs", // ditto, the loudest one
        "my-vendor/keep.rs",          // directory whose name CONTAINS `vendor`
        "builder/main.rs",            // `builder` is not `build`
        "distribution/notes.md",      // `distribution` is not `dist`
    ] {
        assert!(kept.contains(&survivor), "{survivor} was wrongly denied");
    }
}

/// `scan.standard_ignores = false` — the empty list turns the whole thing off,
/// and the trap comes back exactly as sailfish measured it.
#[test]
fn an_empty_list_turns_the_deny_list_off() {
    let discovery = found(&DiscoverySettings {
        standard_ignores: Vec::new(),
        ..ungoverned()
    });

    assert_eq!(
        kept(&discovery),
        [
            "README.md",
            "__pycache__/mod.py",
            "build/out.js",
            "builder/main.rs",
            "dist/bundle.js",
            "distribution/notes.md",
            "my-vendor/keep.rs",
            "node_modules/pkg/index.js",
            "src/main.rs",
            "src/node_modules_helper.rs",
            "src/target_types.rs",
            "target/debug/build_script.rs",
            "vendor/lib.rs",
            "venv/lib/site.py",
        ],
    );
}

/// A custom list replaces the defaults wholesale rather than extending them —
/// the same shape as `exclude`, and the reason the field is a list and not a
/// bool.
#[test]
fn a_custom_list_replaces_the_defaults_wholesale() {
    let discovery = found(&DiscoverySettings {
        standard_ignores: vec!["builder".into()],
        ..ungoverned()
    });
    let kept = kept(&discovery);

    assert!(
        !kept.contains(&"builder/main.rs"),
        "the override was ignored"
    );
    assert!(
        kept.contains(&"node_modules/pkg/index.js"),
        "the default names should be gone, not merged: {kept:?}",
    );
}

/// The escape hatch: a repo that genuinely vendors its sources says so by name.
#[test]
fn force_include_reaches_into_a_denied_directory() {
    let discovery = found(&DiscoverySettings {
        force_include: vec!["vendor/".into()],
        ..ungoverned()
    });
    let kept = kept(&discovery);

    assert!(
        kept.contains(&"vendor/lib.rs"),
        "force_include did not reach it"
    );
    // Named door, not an open one.
    assert!(!kept.contains(&"node_modules/pkg/index.js"));
    assert!(!kept.contains(&"dist/bundle.js"));
}

/// `.git` is the one name no setting can re-enable: not by emptying the list,
/// not by asking for hidden files, not by forcing it in. Built in a temp
/// directory because git itself refuses to track a path named `.git`.
#[test]
fn git_internals_are_unwalkable_at_any_setting() {
    let tree = std::env::temp_dir().join(format!(
        "fs3-discovery-dotgit-{}-{:?}",
        std::process::id(),
        std::thread::current().id(),
    ));
    let _ = fs::remove_dir_all(&tree);
    fs::create_dir_all(tree.join(".git/hooks")).expect("temp tree");
    fs::write(tree.join(".git/hooks/hook.rs"), "fn hook() {}\n").expect("hook");
    fs::write(tree.join(".git/config"), "[core]\n").expect("config");
    fs::write(tree.join("main.rs"), "fn main() {}\n").expect("source");

    let discovery = discover(
        &tree,
        &DiscoverySettings {
            respect_gitignore: false,
            include_hidden: true,
            standard_ignores: Vec::new(),
            force_include: vec![".git/**".into()],
            ..DiscoverySettings::default()
        },
    )
    .expect("temp tree walks");

    let outcome = (
        discovery
            .files
            .iter()
            .map(|f| f.path.clone())
            .collect::<Vec<_>>(),
        discovery
            .skipped
            .iter()
            .map(|s| (s.path.clone(), s.reason))
            .collect::<Vec<_>>(),
    );
    fs::remove_dir_all(&tree).expect("cleanup");

    assert_eq!(outcome.0, ["main.rs"]);
    assert_eq!(outcome.1, Vec::<(String, SkipReason)>::new());
}

/// The list is `pub` so `fs3-daemon`'s watcher pre-filter can delegate to it
/// instead of keeping its own copy. If that copy ever grows a name this one
/// lacks, the two mechanisms have started to disagree about the same question.
#[test]
fn the_list_is_sorted_and_covers_the_watchers_names() {
    let mut sorted = STANDARD_IGNORES.to_vec();
    sorted.sort_unstable();
    assert_eq!(STANDARD_IGNORES, sorted.as_slice(), "keep the list sorted");

    for watched in [".git", "target", "node_modules"] {
        assert!(
            STANDARD_IGNORES.contains(&watched),
            "{watched} is filtered by the watcher but not by discovery",
        );
    }
}
