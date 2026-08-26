//! The error catalog's docs page must never drift from the catalog.
//!
//! Workshop 004 D2: the registry is code, and `docs/reference/error-codes.md`
//! is *emitted* from it. This test is what makes that true — a code added
//! without regenerating the page fails here, the same way a dependency added
//! without an allow-list line fails the architecture check.
//!
//! Regenerate rather than hand-edit:
//!
//! ```bash
//! FS3_UPDATE_DOCS=1 cargo test -p fs3-core --test error_codes
//! ```
//!
//! One test, not two, deliberately: a second test that *reads* the page races
//! the one that writes it under `FS3_UPDATE_DOCS`, and a flaky gate teaches
//! people to re-run rather than to look.

use std::path::PathBuf;

/// The generated page, relative to this crate.
fn docs_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/reference/error-codes.md")
}

/// How to make the failure go away, printed with every failure mode.
const REGENERATE: &str = "FS3_UPDATE_DOCS=1 cargo test -p fs3-core --test error_codes";

#[test]
fn the_generated_docs_page_matches_the_catalog() {
    let expected = fs3_core::catalog::markdown();
    let path = docs_path();

    if std::env::var_os("FS3_UPDATE_DOCS").is_some() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("creating docs/reference");
        }
        std::fs::write(&path, &expected).expect("writing the generated page");
        return;
    }

    let actual = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{} is missing ({error}).\nRegenerate it:\n    {REGENERATE}",
            path.display()
        )
    });

    // Name the offending code before falling back to a diff of two
    // multi-kilobyte strings: "FS3-E-QUEUE-STALLED has no row" is a fix in one
    // reading, and a character-position mismatch is not.
    for code in fs3_core::catalog::ALL {
        assert!(
            actual.contains(code.as_str()),
            "{} has no row in {} — regenerate:\n    {REGENERATE}",
            code.as_str(),
            path.display()
        );
        assert!(
            actual.contains(code.fix()),
            "{}'s fix is not on the docs page — regenerate:\n    {REGENERATE}",
            code.as_str()
        );
    }

    assert_eq!(
        actual,
        expected,
        "\n{} is stale.\nRegenerate it:\n    {REGENERATE}\n",
        path.display()
    );
}
