//! Discovery's half of the cross-filter fixture.
//!
//! `fs3_testkit::discovery_filter` holds one table of `(root, path)` cases that
//! this crate's deny-list prune and `fs3-daemon`'s watcher pre-filter must
//! answer identically. This file runs the table through `discover`; the
//! watcher's half runs the same table through `debounce::is_ignored` (owned by
//! sailfish, landing after the v0.2.0 merge).
//!
//! The point is to pin the **decision** rather than the data. A test asserting
//! that one crate's name list is a subset of the other's reads as proof of
//! agreement while being blind to root-relativity, the config toggle, and case
//! — the three axes sawfish measured (DL-009), each of which had already
//! diverged.
//!
//! The tree is built in a temp directory: `Dist/` and `dist/` cannot both be
//! committed to a repository that anyone might clone onto a case-insensitive
//! volume.

use std::fs;

use fs3_parsers::discovery::{DiscoverySettings, STANDARD_IGNORES, discover};
use fs3_testkit::discovery_filter::{DISCOVERY_FILTER_CASES, build_filter_tree};

#[test]
fn discovery_answers_the_cross_filter_table() {
    let tree = std::env::temp_dir().join(format!(
        "fs3-crossfilter-discovery-{}-{:?}",
        std::process::id(),
        std::thread::current().id(),
    ));
    let _ = fs::remove_dir_all(&tree);
    build_filter_tree(&tree).expect("fixture tree");

    // Everything except the deny list is neutralised: no gitignore semantics
    // (the watcher has none either), hidden files on (the watcher does not
    // filter them), and a size window wide enough to be irrelevant. What is
    // left is the one question both filters answer.
    let mut failures: Vec<String> = Vec::new();
    for case in DISCOVERY_FILTER_CASES {
        let settings = DiscoverySettings {
            respect_gitignore: false,
            include_hidden: true,
            standard_ignores: if case.standard_ignores {
                STANDARD_IGNORES
                    .iter()
                    .map(|name| name.to_string())
                    .collect()
            } else {
                Vec::new()
            },
            ..DiscoverySettings::default()
        };
        let found = discover(&tree.join(case.root), &settings).expect("walks");
        let walked = found.files.iter().any(|file| file.path == case.path);
        if walked != case.walked {
            failures.push(format!(
                "{}: expected walked={} got {} — {}",
                case.name, case.walked, walked, case.why,
            ));
        }
    }
    fs::remove_dir_all(&tree).expect("cleanup");

    assert!(
        failures.is_empty(),
        "cross-filter disagreement:\n{}",
        failures.join("\n")
    );
}
