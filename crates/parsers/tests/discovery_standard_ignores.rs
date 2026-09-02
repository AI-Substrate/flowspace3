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
            "node_modules/pkg/dist/inner.js",
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

/// The names, and only the names.
///
/// `fs3-daemon`'s `debounce::IGNORED_DIRECTORIES` answers the same question
/// with a three-name subset, so this pins that the subset stays a subset.
///
/// It does **not** license swapping that const for this one. sawfish probed
/// exactly that during first-light integration and measured why it would be a
/// regression: `debounce::is_ignored` scans every component of the ABSOLUTE
/// event path, so a repository merely *living* under a directory called
/// `target` is already dead to the watcher — and widening its three names to
/// these eleven would make `~/build/…`, `~/dist/…` and `~/vendor/…` dead too,
/// silently, for roots `add` indexes perfectly. A names-only test cannot see
/// that, because the divergence is about which path the names are matched
/// against; [`the_deny_list_is_root_relative_never_absolute`] is the half that
/// can. The third axis is the toggle: emptying `standard_ignores` must empty
/// the watcher's filter too, and a `const` cannot be turned off.
///
/// The delegation that is actually safe is to the *settings value*
/// (`DiscoverySettings::standard_ignores`), matched root-relatively — then the
/// two filters cannot disagree on any of the three axes rather than just this
/// one.
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

/// The deny list is matched **relative to the root**, never against the root's
/// own absolute path — the half of the contract a names-only test structurally
/// cannot see (sawfish's ask, 2026-08-26).
///
/// Three cases, one rule: what the caller *named* is never second-guessed,
/// what is *under* it still is.
#[test]
fn the_deny_list_is_root_relative_never_absolute() {
    // 1. The root's own name is denied. `flowspace3 add ./target` is an
    //    instruction, not an accident.
    let discovery = discover(&bare().join("target"), &ungoverned()).expect("walks");
    assert_eq!(kept(&discovery), ["debug/build_script.rs"]);

    // 2. A denied name is an ANCESTOR of the root. A checkout that happens to
    //    live under `~/target/` or `~/build/` is an ordinary place to keep
    //    code, and must index like anywhere else.
    let discovery = discover(&bare().join("target/debug"), &ungoverned()).expect("walks");
    assert_eq!(kept(&discovery), ["build_script.rs"]);

    // 3. ...and none of that disarms the list BELOW the root: `pkg/dist/` is
    //    still pruned inside an explicitly-named `node_modules` root. The rule
    //    is root-relative, not "off once you point at something denied".
    let discovery = discover(&bare().join("node_modules"), &ungoverned()).expect("walks");
    assert_eq!(kept(&discovery), ["pkg/index.js"]);
}

/// Build one throwaway tree and hand it to a test. A temp directory, not a
/// fixture, whenever the tree cannot be committed — a path named `.git`, or a
/// case variant that would collide with its own sibling on a case-insensitive
/// volume.
fn temp_tree(label: &str, files: &[(&str, &str)]) -> PathBuf {
    let tree = std::env::temp_dir().join(format!(
        "fs3-discovery-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id(),
    ));
    let _ = fs::remove_dir_all(&tree);
    for (path, body) in files {
        let path = tree.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("temp tree");
        fs::write(path, body).expect("temp file");
    }
    tree
}

/// ASCII-case-insensitive, matching `fs3-daemon`'s watcher filter
/// (`eq_ignore_ascii_case`) — the fourth axis, closed before it could bite
/// (sawfish, 2026-08-26).
///
/// Case sensitivity is a property of the volume, not the platform: on a
/// case-insensitive volume `Dist/` *is* `dist/`, so a case-sensitive prune
/// would index exactly what the watcher refuses to walk. `Dist/` cannot be a
/// committed fixture beside `dist/` for that same reason, hence the temp tree.
#[test]
fn the_deny_list_ignores_ascii_case() {
    let tree = temp_tree(
        "case",
        &[
            ("Dist/bundle.js", "console.log('built');\n"),
            ("NODE_MODULES/pkg/index.js", "module.exports = {};\n"),
            ("Src/main.rs", "fn main() {}\n"),
        ],
    );

    let discovery = discover(&tree, &ungoverned()).expect("temp tree walks");
    let kept = kept(&discovery)
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    fs::remove_dir_all(&tree).expect("cleanup");

    // `Src/` is not on the list at any casing; the other two are.
    assert_eq!(kept, ["Src/main.rs"]);
}

/// The absence has a name. A denied directory puts nothing in either file
/// list, so without this list the only symptom of `Build/` not being indexed
/// is code missing from a search months later.
#[test]
fn every_denied_directory_is_named_in_the_prune_ledger() {
    let discovery = found(&ungoverned());
    let pruned: Vec<(&str, &str)> = discovery
        .pruned
        .iter()
        .map(|dir| (dir.path.as_str(), dir.reason.as_str()))
        .collect();

    assert_eq!(
        pruned,
        [
            (".cache", "hidden"),
            (".next", "hidden"),
            (".venv", "hidden"),
            ("__pycache__", "standard-ignore"),
            ("build", "standard-ignore"),
            ("dist", "standard-ignore"),
            ("node_modules", "standard-ignore"),
            ("target", "standard-ignore"),
            ("vendor", "standard-ignore"),
            ("venv", "standard-ignore"),
        ],
    );
}

/// The directory, never its contents — the property that keeps this ledger
/// eleven rows instead of the 316,609 its contents were measured at.
#[test]
fn the_prune_ledger_never_descends() {
    let discovery = found(&ungoverned());

    assert!(
        discovery.pruned.iter().all(|dir| !dir.path.contains('/')),
        "only top-level refusals here, got {:?}",
        discovery.pruned,
    );
    // `node_modules/pkg/dist` is denied too, and is deliberately absent: the
    // walk stopped at `node_modules` and never learned it existed.
    assert!(
        !discovery
            .pruned
            .iter()
            .any(|dir| dir.path.starts_with("node_modules/")),
        "a pruned directory's insides are never visited, so never reported",
    );
}

/// Turning the policy off empties the ledger with it: nothing was refused, so
/// there is nothing to explain.
#[test]
fn an_empty_deny_list_prunes_nothing_and_reports_nothing() {
    let discovery = found(&DiscoverySettings {
        include_hidden: true,
        standard_ignores: Vec::new(),
        ..ungoverned()
    });

    assert!(discovery.pruned.is_empty(), "{:?}", discovery.pruned);
}

/// Dot-prefixed deny-list names reveal which rule won: hidden policy by
/// default, standard-ignore after hidden entries are enabled.
#[test]
fn hidden_directory_prunes_name_the_effective_rule() {
    let hidden_off = found(&ungoverned());
    for hidden in [".cache", ".next", ".venv"] {
        let pruned = hidden_off
            .pruned
            .iter()
            .find(|dir| dir.path == hidden)
            .unwrap_or_else(|| panic!("{hidden} was not named: {:?}", hidden_off.pruned));
        assert_eq!(pruned.reason.as_str(), "hidden");
    }

    let hidden_on = found(&DiscoverySettings {
        include_hidden: true,
        ..ungoverned()
    });
    for hidden in [".cache", ".next", ".venv"] {
        let pruned = hidden_on
            .pruned
            .iter()
            .find(|dir| dir.path == hidden)
            .unwrap_or_else(|| panic!("{hidden} was not named: {:?}", hidden_on.pruned));
        assert_eq!(pruned.reason.as_str(), "standard-ignore");
    }
}
