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

fn wrong_kind_message(dep: &str, kind: DepKind, allowed: DepKind) -> String {
    Violation::WrongDependencyKind {
        crate_name: "fs3-store".to_string(),
        dep: dep.to_string(),
        kind,
        allowed,
    }
    .to_string()
}

/// A verdict is only half of a diagnostic. The other half is a fix an agent can
/// actually apply, and this branch used to render one that does not exist: a
/// shipped-allow-listed dep found in `[build-dependencies]` was told to change
/// `serde@dependencies` to `serde`, which is neither valid allow-list syntax nor
/// a change — the rule already reads `serde`.
#[test]
fn a_build_script_edge_is_told_a_rule_the_allowlist_can_actually_hold() {
    let message = wrong_kind_message("serde", DepKind::Build, DepKind::Normal);

    assert!(
        !message.contains("@dependencies"),
        "`@dependencies` is not allow-list syntax; the advice invents it: {message}"
    );
    assert!(
        message.contains("`serde@build`"),
        "advice must name the one rule spelling that would permit this edge: {message}"
    );
    assert!(
        message.contains("move serde out of [build-dependencies]"),
        "advice must offer the other real option — not having the edge: {message}"
    );
    // The verdict itself was never wrong, and must survive the rewrite.
    assert!(
        message.contains("declared in [build-dependencies]"),
        "{message}"
    );
}

/// The same defect one step over: allow-listed for tests, found in a build
/// script. `@dev` is not the fix here either.
#[test]
fn a_build_script_edge_allow_listed_for_tests_is_told_the_same_real_fix() {
    let message = wrong_kind_message("cc", DepKind::Build, DepKind::Dev);

    assert!(message.contains("`cc@build`"), "{message}");
    assert!(!message.contains("`cc@dev`"), "{message}");
}

/// The dangerous direction keeps its warning: this is the promotion the kind
/// dimension exists to catch, and its advice was already correct.
#[test]
fn a_promoted_dev_edge_still_gets_the_promotion_warning() {
    let message = wrong_kind_message("fs3-testkit", DepKind::Normal, DepKind::Dev);

    assert!(
        message.contains("ships in the binary"),
        "the promotion warning must not be lost in the rewrite: {message}"
    );
    assert!(message.contains("`fs3-testkit@dev`"), "{message}");
    assert!(message.contains("to `fs3-testkit`"), "{message}");
}

/// No violation the check can actually produce may render advice that points at
/// a suffix the allow-list cannot parse. The RED fixture is the live source of
/// such violations, so read them rather than hand-build them.
#[test]
fn no_rendered_violation_recommends_a_suffix_that_does_not_exist() {
    let fixtures = [
        include_str!("../fixtures/arch/drifted-metadata.json"),
        include_str!("../fixtures/arch/promoted-dev-edge-metadata.json"),
    ];

    let mut rendered = 0;
    for fixture in fixtures {
        let graph = arch::Graph::from_cargo_metadata(fixture).expect("fixture parses");
        for violation in arch::check(&graph, &allowlist()) {
            let message = violation.to_string();
            assert!(
                !message.contains("@dependencies"),
                "no rule can be spelled `@dependencies`: {message}"
            );
            rendered += 1;
        }
    }
    assert!(rendered > 0, "the fixtures must produce violations to read");
}
