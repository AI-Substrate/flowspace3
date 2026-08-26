//! The cross-filter fixture: one table of `(root, path)` cases that
//! `fs3-parsers`' discovery prune and `fs3-daemon`'s watcher pre-filter must
//! answer **identically**.
//!
//! ## Why this exists
//!
//! Both crates decide "is this path under a directory nobody indexes?", and
//! for a while both were tested — separately, and in a way that could not see
//! them disagreeing. A test that asserts one crate's name list is a subset of
//! the other's *reads* as proof of agreement while being blind to every other
//! axis. sawfish measured three such axes on the way to a delegation that
//! looked like a one-line const swap and would have been a regression
//! (DL-009):
//!
//! 1. **Root-relativity** — discovery matches components *below the root*; the
//!    watcher matched every component of the absolute event path, so a repo
//!    living under `~/target/myrepo` was dead to it, silently.
//! 2. **The toggle** — `scan.standard_ignores = false` empties discovery's
//!    list; a `const` cannot be turned off.
//! 3. **Case** — the watcher was always `eq_ignore_ascii_case`; discovery's
//!    prune was case-sensitive until 2026-08-26.
//!
//! So this fixture pins the **decision**, not the data. Each side runs the same
//! table through its own filter and asserts the same `walked` answer; a future
//! divergence on any axis turns a build red instead of quietly producing two
//! different indexes.
//!
//! ## Using it
//!
//! ```no_run
//! use fs3_testkit::discovery_filter::{DISCOVERY_FILTER_CASES, build_filter_tree};
//!
//! let tree = std::env::temp_dir().join("my-side-of-the-fixture");
//! build_filter_tree(&tree).unwrap();
//! for case in DISCOVERY_FILTER_CASES {
//!     let root = tree.join(case.root);
//!     // ...ask your filter about `root.join(case.path)`, honouring
//!     // `case.standard_ignores`, and assert the answer equals `case.walked`.
//! }
//! ```
//!
//! The tree is built in a temp directory rather than committed because two of
//! its paths cannot live side by side in a repository: `Dist/` collides with
//! `dist/` on a case-insensitive volume.

use std::fs;
use std::io;
use std::path::Path;

/// One question both filters must answer the same way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FilterCase {
    /// Test name, used in failure messages.
    pub name: &'static str,
    /// The scan root, relative to the fixture tree. `""` is the tree itself.
    pub root: &'static str,
    /// The file, relative to `root`, `/`-separated.
    pub path: &'static str,
    /// The `scan.standard_ignores` setting this case is asked under.
    pub standard_ignores: bool,
    /// `true` when the file must be reachable — indexed by discovery, and an
    /// event on it accepted by the watcher.
    pub walked: bool,
    /// Why, in one line, for the failure message.
    pub why: &'static str,
}

/// The table. Ordinary code first, then the four axes.
pub const DISCOVERY_FILTER_CASES: &[FilterCase] = &[
    FilterCase {
        name: "plain_source",
        root: "",
        path: "src/main.rs",
        standard_ignores: true,
        walked: true,
        why: "ordinary source under an ordinary root",
    },
    FilterCase {
        name: "denied_directory",
        root: "",
        path: "node_modules/pkg/index.js",
        standard_ignores: true,
        walked: false,
        why: "the whole point: dependencies are not indexable work",
    },
    // (a) A root whose OWN name is denied. What the caller named is an
    // instruction, not an accident — `flowspace3 add ./node_modules`.
    FilterCase {
        name: "root_is_itself_denied",
        root: "node_modules",
        path: "pkg/index.js",
        standard_ignores: true,
        walked: true,
        why: "the named root is never second-guessed; the list is root-relative",
    },
    // ...and the same for a root merely LIVING under a denied name, which is
    // the case that was silently killing the watcher.
    FilterCase {
        name: "root_beneath_a_denied_ancestor",
        root: "target/checkout",
        path: "src/main.rs",
        standard_ignores: true,
        walked: true,
        why: "a repo under ~/target/ is an ordinary place to keep code",
    },
    // (b) A genuinely denied directory BENEATH such a root. Root-relativity
    // must not be "the list switches off once the root looks denied".
    FilterCase {
        name: "denied_beneath_a_denied_root",
        root: "node_modules",
        path: "pkg/dist/inner.js",
        standard_ignores: true,
        walked: false,
        why: "the list still bites below the root it was exempted at",
    },
    FilterCase {
        name: "denied_beneath_an_exempt_ancestor",
        root: "target/checkout",
        path: "build/out.js",
        standard_ignores: true,
        walked: false,
        why: "exempting the root does not exempt what is under it",
    },
    // (c) Case. On a case-insensitive volume `Dist/` IS `dist/`, so a filter
    // that reads case-sensitively indexes what the other refuses to walk.
    FilterCase {
        name: "denied_name_in_mixed_case",
        root: "",
        path: "Dist/bundle.js",
        standard_ignores: true,
        walked: false,
        why: "denied names are ASCII-case-insensitive on both sides",
    },
    FilterCase {
        name: "undenied_name_in_mixed_case",
        root: "",
        // `Lib/`, not `Src/`: on a case-insensitive volume the latter would
        // land inside the `src/` this table already uses, and the case would
        // silently stop testing casing.
        path: "Lib/app.rs",
        standard_ignores: true,
        walked: true,
        why: "case-insensitivity must not deny names that are not on the list",
    },
    FilterCase {
        name: "substring_of_a_denied_name",
        root: "",
        path: "builder/main.rs",
        standard_ignores: true,
        walked: true,
        why: "whole components only: `builder` is not `build`",
    },
    // (d) The toggle — the axis with no watcher-side test at all until the
    // delegation lands, and the one that would silently invert the mismatch.
    FilterCase {
        name: "toggle_off_admits_denied_directory",
        root: "",
        path: "node_modules/pkg/index.js",
        standard_ignores: false,
        walked: true,
        why: "scan.standard_ignores = false must empty BOTH filters",
    },
    FilterCase {
        name: "toggle_off_admits_mixed_case",
        root: "",
        path: "Dist/bundle.js",
        standard_ignores: false,
        walked: true,
        why: "nothing survives the toggle, whatever its casing",
    },
    FilterCase {
        name: "toggle_off_keeps_plain_source",
        root: "",
        path: "src/main.rs",
        standard_ignores: false,
        walked: true,
        why: "turning the policy off changes nothing for ordinary code",
    },
];

/// Materialise every file the table names under `base`.
///
/// Idempotent: cases deliberately share paths, and re-running overwrites. The
/// caller owns cleanup.
///
/// # Errors
/// Any filesystem error while creating the tree.
pub fn build_filter_tree(base: &Path) -> io::Result<()> {
    for case in DISCOVERY_FILTER_CASES {
        let path = base.join(case.root).join(case.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, b"// cross-filter fixture\n")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_covers_all_four_axes() {
        let has = |name: &str| DISCOVERY_FILTER_CASES.iter().any(|c| c.name == name);
        for required in [
            "root_is_itself_denied",              // (a)
            "denied_beneath_a_denied_root",       // (b)
            "denied_name_in_mixed_case",          // (c)
            "toggle_off_admits_denied_directory", // (d)
        ] {
            assert!(has(required), "{required} is the axis, do not drop it");
        }
        assert!(
            DISCOVERY_FILTER_CASES.iter().any(|c| !c.standard_ignores),
            "the toggle axis needs at least one case with it off",
        );
    }

    #[test]
    fn case_names_are_unique() {
        let mut names: Vec<&str> = DISCOVERY_FILTER_CASES.iter().map(|c| c.name).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "duplicate case name");
    }

    #[test]
    fn the_tree_materialises_every_case() {
        let tree = std::env::temp_dir().join(format!(
            "fs3-filter-fixture-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        let _ = fs::remove_dir_all(&tree);
        build_filter_tree(&tree).expect("build");

        let missing: Vec<&str> = DISCOVERY_FILTER_CASES
            .iter()
            .filter(|c| !tree.join(c.root).join(c.path).is_file())
            .map(|c| c.name)
            .collect();
        fs::remove_dir_all(&tree).expect("cleanup");

        assert!(missing.is_empty(), "not materialised: {missing:?}");
    }
}
