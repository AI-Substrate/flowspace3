//! Discovery against a committed fixture tree — the exact set, both lists.
//!
//! The tree in `fixtures/discovery-tree/` carries its own `.gitignore` (and a
//! nested one under `third_party/`), so the rules under test are the fixture's own,
//! not this repo's. Three of its files are ignored by those rules and are
//! therefore committed with `git add -f`: `build-output/generated.rs`,
//! `app.log` and `secret-notes.md`. They exist on disk precisely so the
//! assertions can prove they do **not** come back — the grep trap: a reviewer
//! searching for `secret-notes` finds the file, the ignore rule, and the test
//! that pins it out of the index.

use std::path::{Path, PathBuf};

use fs3_parsers::Language;
use fs3_parsers::discovery::{Discovery, DiscoverySettings, LanguageFamily, SkipReason, discover};

fn tree() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/discovery-tree")
}

fn found(settings: &DiscoverySettings) -> Discovery {
    discover(&tree(), settings).expect("fixture tree walks")
}

fn kept(discovery: &Discovery) -> Vec<&str> {
    discovery.files.iter().map(|f| f.path.as_str()).collect()
}

fn refused(discovery: &Discovery) -> Vec<(&str, SkipReason)> {
    discovery
        .skipped
        .iter()
        .map(|s| (s.path.as_str(), s.reason))
        .collect()
}

/// The default policy, asserted as an exact set — additions to the fixture
/// tree must be accounted for here or this fails, which is the point.
#[test]
fn the_default_walk_returns_exactly_the_scannable_files() {
    let discovery = found(&DiscoverySettings::default());

    assert_eq!(
        kept(&discovery),
        [
            "README.md",
            "docs/guide.md",
            "notes.txt",
            "src/big_generated.rs",
            "src/lib.rs",
            "src/util.py",
            "third_party/keep.rs",
        ],
    );
    assert_eq!(
        refused(&discovery),
        [
            ("config/app.json", SkipReason::ConfigFormat),
            ("config/settings.yaml", SkipReason::ConfigFormat),
            ("docs/corrupt-binary.md", SkipReason::Binary),
            ("src/empty.rs", SkipReason::TooSmall),
        ],
    );
}

/// What git ignores is out of scope, not refused: it must appear in neither
/// list, or every `node_modules` entry would land in the skip ledger.
#[test]
fn ignored_and_hidden_files_are_absent_from_both_lists() {
    let discovery = found(&DiscoverySettings::default());

    for trap in [
        "build-output/generated.rs", // ignored directory
        "secret-notes.md",           // ignored by name — indexable extension, still gone
        "app.log",                   // ignored by pattern
        "third_party/tool.py",       // ignored by a NESTED .gitignore
        ".hidden/notes.md",          // hidden directory
        ".gitignore",                // hidden file
    ] {
        assert!(!kept(&discovery).contains(&trap), "{trap} was indexed");
        assert!(
            !refused(&discovery).iter().any(|(path, _)| *path == trap),
            "{trap} reached the skip ledger",
        );
    }
}

/// PRD req 41: "a gitignored folder you do want indexed".
#[test]
fn force_include_reaches_into_a_gitignored_folder() {
    let settings = DiscoverySettings {
        force_include: vec!["build-output/".into()],
        ..DiscoverySettings::default()
    };
    let discovery = found(&settings);

    assert_eq!(
        kept(&discovery),
        [
            "README.md",
            "build-output/generated.rs",
            "docs/guide.md",
            "notes.txt",
            "src/big_generated.rs",
            "src/lib.rs",
            "src/util.py",
            "third_party/keep.rs",
        ],
    );
    // The force-include is a named door, not an open one: the other ignored
    // files stay out, and the skip ledger is untouched.
    assert_eq!(
        refused(&discovery),
        refused(&found(&DiscoverySettings::default())),
    );
}

/// Precedence: an explicit refusal beats an explicit inclusion, and a refused
/// file is *reported* — unlike a git-ignored one, someone asked about it.
#[test]
fn exclude_outranks_force_include_and_is_reported() {
    let settings = DiscoverySettings {
        force_include: vec!["build-output/".into()],
        exclude: vec!["build-output/generated.rs".into()],
        ..DiscoverySettings::default()
    };
    let discovery = found(&settings);

    assert!(!kept(&discovery).contains(&"build-output/generated.rs"));
    assert!(
        refused(&discovery).contains(&("build-output/generated.rs", SkipReason::Excluded)),
        "an excluded file must be observable, got {:?}",
        refused(&discovery),
    );
}

/// Exclude globs are gitignore syntax, matched against the relative path —
/// a directory glob and an extension glob both work.
#[test]
fn exclude_globs_cover_directories_and_extensions() {
    let settings = DiscoverySettings {
        exclude: vec!["docs/**".into(), "*.txt".into()],
        ..DiscoverySettings::default()
    };
    let discovery = found(&settings);

    assert_eq!(
        kept(&discovery),
        [
            "README.md",
            "src/big_generated.rs",
            "src/lib.rs",
            "src/util.py",
            "third_party/keep.rs",
        ],
    );
    // `docs/corrupt-binary.md` is refused as *excluded*, not as binary: the
    // exclusion is decided before anything opens the file.
    assert_eq!(
        refused(&discovery),
        [
            ("config/app.json", SkipReason::ConfigFormat),
            ("config/settings.yaml", SkipReason::ConfigFormat),
            ("docs/corrupt-binary.md", SkipReason::Excluded),
            ("docs/guide.md", SkipReason::Excluded),
            ("notes.txt", SkipReason::Excluded),
            ("src/empty.rs", SkipReason::TooSmall),
        ],
    );
}

/// With the ignore rules off, the same tree indexes build output — and the
/// unsupported files it drags in are named rather than silently dropped
/// (PRD req 43).
#[test]
fn turning_gitignore_off_indexes_the_whole_tree() {
    let settings = DiscoverySettings {
        respect_gitignore: false,
        ..DiscoverySettings::default()
    };
    let discovery = found(&settings);

    assert_eq!(
        kept(&discovery),
        [
            "README.md",
            "build-output/generated.rs",
            "docs/guide.md",
            "notes.txt",
            "secret-notes.md",
            "src/big_generated.rs",
            "src/lib.rs",
            "src/util.py",
            "third_party/keep.rs",
            "third_party/tool.py",
        ],
    );
    assert!(refused(&discovery).contains(&("app.log", SkipReason::UnsupportedExtension)));
}

/// The POC's second lever: one 18 MB file cost 0.62 s. A ceiling is a
/// reported decision, not a silent drop.
#[test]
fn the_size_ceiling_reports_the_files_it_refuses() {
    let settings = DiscoverySettings {
        max_file_bytes: 256,
        ..DiscoverySettings::default()
    };
    let discovery = found(&settings);

    assert!(!kept(&discovery).contains(&"src/big_generated.rs"));
    assert!(refused(&discovery).contains(&("src/big_generated.rs", SkipReason::TooLarge)));
    // Everything under the ceiling is unaffected.
    assert!(kept(&discovery).contains(&"src/lib.rs"));
}

/// PRD req 43 is a default, not a law: a repo that wants its YAML indexed
/// says so, and then the same files come back as `Config`.
#[test]
fn config_formats_index_when_the_repo_asks_for_them() {
    let settings = DiscoverySettings {
        index_config_formats: true,
        ..DiscoverySettings::default()
    };
    let discovery = found(&settings);

    assert!(kept(&discovery).contains(&"config/settings.yaml"));
    assert!(kept(&discovery).contains(&"config/app.json"));
    assert_eq!(
        refused(&discovery),
        [
            ("docs/corrupt-binary.md", SkipReason::Binary),
            ("src/empty.rs", SkipReason::TooSmall),
        ],
    );
}

/// Hidden files are one knob, and `.git` is never behind it.
#[test]
fn hidden_files_appear_only_when_asked_for() {
    let settings = DiscoverySettings {
        include_hidden: true,
        ..DiscoverySettings::default()
    };
    let discovery = found(&settings);

    assert!(kept(&discovery).contains(&".hidden/notes.md"));
    // The ignore files themselves have no extension, so they are named as
    // unsupported rather than quietly vanishing.
    assert!(refused(&discovery).contains(&(".gitignore", SkipReason::UnsupportedExtension)));
    assert!(
        refused(&discovery).contains(&("third_party/.gitignore", SkipReason::UnsupportedExtension))
    );
}

/// Each row carries what the pipeline needs next: a relative path, the size
/// that budgets the work, the family that routed it, and the grammar — or
/// `None`, which still scans (as a file element).
#[test]
fn every_row_carries_relative_path_size_family_and_grammar() {
    let discovery = found(&DiscoverySettings::default());
    let row = |path: &str| {
        discovery
            .files
            .iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| panic!("{path} missing"))
    };

    let lib = row("src/lib.rs");
    assert_eq!(lib.bytes, 30);
    assert_eq!(lib.family, LanguageFamily::Source);
    assert_eq!(lib.language, Some(Language::Rust));

    assert_eq!(row("README.md").family, LanguageFamily::Document);
    assert_eq!(row("README.md").language, Some(Language::Markdown));
    assert_eq!(row("src/util.py").language, Some(Language::Python));

    // No grammar, still indexed — the observable "unknown language" outcome.
    assert_eq!(row("notes.txt").family, LanguageFamily::Document);
    assert_eq!(row("notes.txt").language, None);

    assert!(
        discovery.files.iter().all(|f| !f.path.contains('\\')),
        "paths must be `/`-separated and relative, whatever the platform",
    );
}
