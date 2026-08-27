//! No test may build a `flowspace3` subprocess environment by hand.
//!
//! # Why a source scan
//!
//! Twice now the production database has been written to by a test run, and
//! both times every individual piece was behaving as designed. The second time
//! (2026-08-27, migration 0012) the leak was a test that spawned the real
//! daemon with `FS3_CONFIG_DIR` set and nothing else: config resolution fell
//! through to [`fs3_core::DatabaseConfig::DEFAULT_URL`] — the SHIPPED address,
//! which on a developer machine is the real store — and daemon boot migrated
//! it before serving.
//!
//! [`fs3_testkit::spawn::sealed`] makes the correct environment available. This
//! test makes it the ONLY one. Availability was never the problem: the scrub
//! and the pin both already existed on 2026-08-27, in two different files,
//! neither of which had both — so faithfully copying either one produced a
//! leak. A convention that is 50% correct in every instance is not a convention
//! anybody can follow, and asking the next author to remember the other half is
//! how this happened the second time.
//!
//! Same muscle as `arch_drift.rs` and the error-code drift test: the thing that
//! could rot is checked mechanically rather than remembered.

use std::path::{Path, PathBuf};

/// The repo root, from this crate's manifest directory (`crates/testkit`).
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/testkit sits two levels below the workspace root")
        .to_path_buf()
}

/// Every `.rs` file under any crate's `tests/` directory.
fn test_sources() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let crates = workspace_root().join("crates");
    let entries = std::fs::read_dir(&crates)
        .unwrap_or_else(|error| panic!("reading {} ({error})", crates.display()));

    for entry in entries.flatten() {
        let tests = entry.path().join("tests");
        if tests.is_dir() {
            collect_rs(&tests, &mut found);
        }
    }

    found.sort();
    found
}

fn collect_rs(directory: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, into);
        } else if path.extension().is_some_and(|e| e == "rs") {
            into.push(path);
        }
    }
}

/// Collapse whitespace so a call split across lines reads as one expression.
///
/// Line-by-line matching would miss the rustfmt-wrapped form, which is the
/// form every one of these calls actually takes.
fn flattened(source: &str) -> String {
    source.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The argument of each `Command::new(...)`, by balanced parentheses.
fn command_new_arguments(flat: &str) -> Vec<String> {
    const NEEDLE: &str = "Command::new(";
    let mut arguments = Vec::new();
    let bytes = flat.as_bytes();
    let mut cursor = 0;

    while let Some(offset) = flat[cursor..].find(NEEDLE) {
        let open = cursor + offset + NEEDLE.len();
        let mut depth = 1_usize;
        let mut index = open;
        while index < bytes.len() && depth > 0 {
            match bytes[index] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            index += 1;
        }
        arguments.push(flat[open..index.saturating_sub(1)].to_string());
        cursor = index;
    }

    arguments
}

/// The rule: a test constructs a `flowspace3` subprocess through
/// [`fs3_testkit::sealed`], or not at all.
#[test]
fn no_test_constructs_a_flowspace3_command_by_hand() {
    let mut violations = Vec::new();

    for path in test_sources() {
        // This file quotes the very patterns it hunts — its own `NEEDLE`
        // constant is the literal `Command::new(`, and the scanner test below
        // builds sample calls out of `CARGO_BIN_EXE_flowspace3`. Scanning
        // itself, it reports itself. Skipping by `file!()` rather than by a
        // name-shaped guess keeps the exclusion exactly one file wide, and it
        // cannot go stale under a rename.
        if display(&path) == file!() {
            continue;
        }

        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("reading {} ({error})", path.display()));
        for argument in command_new_arguments(&flattened(&source)) {
            // `Command::new("git")` and the auto-update test's throwaway shell
            // script are not the binary under seal. Naming flowspace3 is what
            // makes a spawn this test's business.
            if argument.contains("CARGO_BIN_EXE_flowspace3")
                || argument.contains("flowspace3_binary")
            {
                violations.push(format!("{}: Command::new({argument})", display(&path)));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "These tests build a `flowspace3` subprocess environment by hand:\n  {}\n\n\
         An unsealed spawn inherits the developer's `FS3_*` overrides and, with no\n\
         `[database]` section to find, resolves DatabaseConfig::DEFAULT_URL — the\n\
         SHIPPED address, which on a developer machine is the PRODUCTION store. The\n\
         daemon migrates it at boot. That is the 2026-08-27 incident, twice over.\n\n\
         Use `fs3_testkit::sealed(binary, config_dir, TestDatabase::…)`, which scrubs\n\
         every inherited FS3_* and pins both the config directory and the database.",
        violations.join("\n  ")
    );
}

/// A scan that matches nothing passes forever.
///
/// This is the half of the check that rots silently: rename the helper, move
/// the tests, break the walker, and the assertion above goes green on an empty
/// set. So assert the scan can still SEE the spawning tests it exists to
/// govern.
#[test]
fn the_scan_still_sees_the_tests_it_governs() {
    let sources = test_sources();
    assert!(
        sources.len() > 20,
        "only {} test sources found — the walker is not reaching them",
        sources.len()
    );

    let sealing: Vec<_> = sources
        .iter()
        .filter(|path| {
            std::fs::read_to_string(path)
                .map(|text| text.contains("fs3_testkit::sealed"))
                .unwrap_or(false)
        })
        .map(|path| display(path))
        .collect();

    // The five known subprocess-spawning suites: health, boot_contract,
    // daemon_logging, docs_bundle, ping. Fewer means one stopped sealing, or
    // stopped being found.
    assert!(
        sealing.len() >= 5,
        "only {} test file(s) spawn through the seal: {sealing:?}\n\
         Either a spawning test stopped using `fs3_testkit::sealed` — which the \
         other test in this file would have caught — or this scan stopped finding \
         it, which nothing else would catch.",
        sealing.len()
    );
}

/// The argument scanner has to survive rustfmt's line wrapping, which is the
/// shape every real call takes.
#[test]
fn the_scanner_reads_calls_that_span_lines() {
    let wrapped = "let c = Command::new(\n    env!(\"CARGO_BIN_EXE_flowspace3\"),\n);";
    let arguments = command_new_arguments(&flattened(wrapped));
    assert_eq!(arguments.len(), 1, "one call, found {arguments:?}");
    assert!(
        arguments[0].contains("CARGO_BIN_EXE_flowspace3"),
        "the nested `env!(...)` parens must not truncate the argument: {:?}",
        arguments[0]
    );

    // And it must not flag the innocent ones.
    let innocent = flattened("Command::new(\"git\").args(args); Command::new(&installed)");
    let arguments = command_new_arguments(&innocent);
    assert_eq!(arguments.len(), 2);
    assert!(
        arguments
            .iter()
            .all(|a| !a.contains("CARGO_BIN_EXE_flowspace3")),
        "spawning git or a throwaway script is not this test's business: {arguments:?}"
    );
}

fn display(path: &Path) -> String {
    path.strip_prefix(workspace_root())
        .unwrap_or(path)
        .display()
        .to_string()
}
