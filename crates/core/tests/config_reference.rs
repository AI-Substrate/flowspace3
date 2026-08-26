//! The configuration reference must never drift from the config types.
//!
//! PRD req 58 asks for a page documenting EVERY option the binary reads, "kept
//! current as options land". A page that is only kept current by remembering is
//! a page that goes stale on the first busy afternoon, so this test is what
//! keeps it true: it serialises the default [`Config`], walks every key the
//! shape produces, and fails naming any key that has no row.
//!
//! It cannot check prose, and does not try. What it proves is coverage — that
//! no option exists in the code and nowhere on the page.

use std::path::PathBuf;

use fs3_core::Config;

fn reference_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/reference/configuration.md")
}

/// Every `section.key` the default configuration produces, as TOML sees them.
///
/// Read from the serialised shape rather than a hand-written list, because a
/// hand-written list is the thing that goes stale.
fn keys() -> Vec<(String, String)> {
    let table: toml::Table = toml::Table::try_from(Config::default())
        .expect("the default configuration must serialise to a TOML table");

    let mut keys = Vec::new();
    for (section, value) in &table {
        let Some(inner) = value.as_table() else {
            continue;
        };
        for key in inner.keys() {
            keys.push((section.clone(), key.clone()));
        }
    }
    keys
}

#[test]
fn every_configuration_key_has_a_row_in_the_reference() {
    let path = reference_path();
    let page = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is missing ({error})", path.display()));

    // Section headings, so a section that two ports SHARE (`[embedder]` and
    // `[summarizer]` are the same shape and the same table) counts for both
    // without the page having to repeat itself.
    let headings: Vec<&str> = page
        .lines()
        .filter(|line| line.starts_with("## "))
        .collect();

    let mut missing = Vec::new();
    for (section, key) in keys() {
        let documented = page.contains(&format!("| `{key}` |"));
        let sectioned = headings.iter().any(|heading| {
            heading.contains(&format!("`[{section}]`"))
                || heading.contains(&format!("`[{section}."))
        });
        if !documented || !sectioned {
            missing.push(format!("{section}.{key}"));
        }
    }

    assert!(
        missing.is_empty(),
        "these configuration keys have no row in {}: {}\nAdd one row per key — \
         the page is the reference PRD req 58 asks for.",
        path.display(),
        missing.join(", ")
    );
}

/// The environment layer is half the reference's value: an option nobody can
/// override from the environment is a different option in a container.
#[test]
fn every_key_documents_its_environment_override() {
    let path = reference_path();
    let page = std::fs::read_to_string(&path).expect("the reference page");

    let mut missing = Vec::new();
    for (section, key) in keys() {
        // `[providers]` and `[repos]` are maps of user-named tables: their keys
        // are not reachable through the FS3_<SECTION>__<KEY> grammar at all,
        // and the page says so rather than inventing names.
        if section == "providers" || section == "repos" {
            continue;
        }
        let name = format!("FS3_{}__{}", section.to_uppercase(), key.to_uppercase());
        if !page.contains(&name) {
            missing.push(name);
        }
    }

    assert!(
        missing.is_empty(),
        "these environment overrides are not named in {}: {}",
        path.display(),
        missing.join(", ")
    );
}

/// Every provider kind is a configuration shape of its own, and the one people
/// most often get wrong. A new kind with no section here is a gap.
#[test]
fn every_provider_kind_has_its_own_section() {
    let page = std::fs::read_to_string(reference_path()).expect("the reference page");

    for kind in ["fake", "openai", "azure_openai"] {
        assert!(
            page.contains(&format!("### `kind = \"{kind}\"`")),
            "the {kind} provider has no section in the configuration reference"
        );
    }
}
