//! The architecture drift check, proved in both directions.
//!
//! A check nobody has watched fail is a check nobody knows works. The negative
//! proof here is a committed manifest carrying a forbidden edge — `sqlx` in the
//! functional core — so it is re-runnable on every `cargo test`, not a
//! one-shot violate-and-revert.

use fs3_testkit::arch::{self, DepKind, Violation};

fn allowlist() -> arch::Allowlist {
    arch::allowlist().expect("the committed allow-list must parse")
}

/// The real graph, judged by the real check. This is the positive proof, and it
/// runs in `cargo test` as well as in `harness checks`.
#[test]
fn the_live_workspace_has_no_architecture_drift() {
    let graph = arch::workspace_graph().expect("cargo metadata should describe this workspace");
    assert_eq!(
        graph.crates.len(),
        7,
        "workshop 001 specifies exactly 7 crates, found: {:?}",
        graph.crates.iter().map(|c| &c.name).collect::<Vec<_>>()
    );

    let violations = arch::check(&graph, &allowlist());
    assert!(
        violations.is_empty(),
        "architecture drift:\n{}",
        violations
            .iter()
            .map(|v| format!("  - {v}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The check is not vacuous: a well-formed graph passes it.
#[test]
fn a_clean_fixture_graph_is_green() {
    let graph =
        arch::Graph::from_cargo_metadata(include_str!("../fixtures/arch/clean-metadata.json"))
            .expect("fixture is cargo-metadata shaped");
    assert_eq!(arch::check(&graph, &allowlist()), Vec::new());
}

/// The negative proof for AC-03: adding `sqlx` to `fs3-core` turns the check RED.
#[test]
fn sqlx_in_the_functional_core_is_caught() {
    let graph =
        arch::Graph::from_cargo_metadata(include_str!("../fixtures/arch/drifted-metadata.json"))
            .expect("fixture is cargo-metadata shaped");

    let violations = arch::check(&graph, &allowlist());

    assert_eq!(
        violations,
        vec![Violation::ForbiddenExternal {
            crate_name: "fs3-core".to_string(),
            dep: "sqlx".to_string(),
            kind: DepKind::Normal,
        }],
        "the only difference from the clean fixture is sqlx in core, so it must be \
         the only violation"
    );

    // The message has to tell an agent what to do about it, not just that it is bad.
    let message = violations[0].to_string();
    assert!(message.contains("fs3-core"), "{message}");
    assert!(message.contains("sqlx"), "{message}");
    assert!(message.contains("arch-allowlist.toml"), "{message}");
}

/// Mocking frameworks are refused by name, with their own message, wherever
/// they appear — including dev-dependencies, which is where they always start.
#[test]
fn a_mocking_framework_is_refused_workspace_wide() {
    let mut graph =
        arch::Graph::from_cargo_metadata(include_str!("../fixtures/arch/clean-metadata.json"))
            .expect("fixture is cargo-metadata shaped");
    let core = graph
        .crates
        .iter_mut()
        .find(|c| c.name == "fs3-core")
        .expect("fixture has fs3-core");
    core.deps.push(arch::Dep {
        name: "mockall".to_string(),
        kind: DepKind::Dev,
    });

    let violations = arch::check(&graph, &allowlist());
    assert_eq!(
        violations,
        vec![Violation::BannedEverywhere {
            crate_name: "fs3-core".to_string(),
            dep: "mockall".to_string(),
            kind: DepKind::Dev,
        }]
    );
    assert!(
        violations[0].to_string().contains("fakes over mocks"),
        "the violation should cite the rule it enforces: {}",
        violations[0]
    );
}

/// An undeclared crate-to-crate edge is drift even when both crates are ours.
#[test]
fn an_inverted_dependency_direction_is_caught() {
    let mut graph =
        arch::Graph::from_cargo_metadata(include_str!("../fixtures/arch/clean-metadata.json"))
            .expect("fixture is cargo-metadata shaped");
    let core = graph
        .crates
        .iter_mut()
        .find(|c| c.name == "fs3-core")
        .expect("fixture has fs3-core");
    core.deps.push(arch::Dep {
        name: "fs3-store".to_string(),
        kind: DepKind::Normal,
    });

    assert_eq!(
        arch::check(&graph, &allowlist()),
        vec![Violation::ForbiddenInternal {
            crate_name: "fs3-core".to_string(),
            dep: "fs3-store".to_string(),
            kind: DepKind::Normal,
        }]
    );
}
