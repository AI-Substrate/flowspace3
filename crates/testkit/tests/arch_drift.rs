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

/// The negative proof for the dependency-KIND dimension: `fs3-providers` may
/// use `fs3-testkit` in its tests, and only there. Promoting that edge into
/// `[dependencies]` ships the fakes inside the real provider crate.
///
/// The fixture differs from the clean one by exactly one JSON line — the edge's
/// `kind` — so any violation here is that promotion and nothing else. Before
/// the allow-list carried kinds, this fixture produced ZERO violations and the
/// rule was enforced only by a TOML comment.
#[test]
fn promoting_a_dev_edge_into_the_shipped_binary_is_caught() {
    let graph = arch::Graph::from_cargo_metadata(include_str!(
        "../fixtures/arch/promoted-dev-edge-metadata.json"
    ))
    .expect("fixture is cargo-metadata shaped");

    let violations = arch::check(&graph, &allowlist());

    assert_eq!(
        violations,
        vec![Violation::WrongDependencyKind {
            crate_name: "fs3-providers".to_string(),
            dep: "fs3-testkit".to_string(),
            kind: DepKind::Normal,
            allowed: DepKind::Dev,
        }],
        "the fixture's only difference from the clean one is the promoted edge"
    );

    let message = violations[0].to_string();
    assert!(message.contains("dev-dependencies"), "{message}");
    assert!(message.contains("dependencies]"), "{message}");
    assert!(message.contains("arch-allowlist.toml"), "{message}");
}

/// Privilege runs one way. An edge cleared to ship is also cleared for tests,
/// so a shipped dependency appearing in `[dev-dependencies]` is not drift —
/// otherwise the kind dimension would just be a second way to spell equality.
#[test]
fn a_shipped_edge_may_also_be_used_by_tests() {
    let mut graph =
        arch::Graph::from_cargo_metadata(include_str!("../fixtures/arch/clean-metadata.json"))
            .expect("fixture is cargo-metadata shaped");
    let core = graph
        .crates
        .iter_mut()
        .find(|c| c.name == "fs3-core")
        .expect("fixture has fs3-core");
    core.deps.push(arch::Dep {
        name: "serde".to_string(),
        kind: DepKind::Dev,
    });

    assert_eq!(
        arch::check(&graph, &allowlist()),
        Vec::new(),
        "serde is allow-listed for fs3-core as a shipped edge, so using it in \
         tests too is not drift"
    );
}

/// A build-dependency is a separate axis: a shipped edge does not license one.
#[test]
fn a_shipped_edge_does_not_license_a_build_script_edge() {
    let mut graph =
        arch::Graph::from_cargo_metadata(include_str!("../fixtures/arch/clean-metadata.json"))
            .expect("fixture is cargo-metadata shaped");
    let core = graph
        .crates
        .iter_mut()
        .find(|c| c.name == "fs3-core")
        .expect("fixture has fs3-core");
    core.deps.push(arch::Dep {
        name: "serde".to_string(),
        kind: DepKind::Build,
    });

    assert_eq!(
        arch::check(&graph, &allowlist()),
        vec![Violation::WrongDependencyKind {
            crate_name: "fs3-core".to_string(),
            dep: "serde".to_string(),
            kind: DepKind::Build,
            allowed: DepKind::Normal,
        }]
    );
}

/// A typo in the suffix must fail the parse rather than quietly becoming part
/// of a crate name — a rule named `tokio@devv` would match nothing and silently
/// forbid the edge it was meant to allow.
#[test]
fn an_unknown_kind_suffix_fails_the_allowlist_parse() {
    let error =
        toml::from_str::<arch::Allowlist>("[crates.fs3-core]\nexternal = [\"tokio@devv\"]\n")
            .expect_err("an unknown kind suffix must not parse");

    let message = error.to_string();
    assert!(message.contains("tokio@devv"), "{message}");
    assert!(message.contains("@dev"), "{message}");
}
