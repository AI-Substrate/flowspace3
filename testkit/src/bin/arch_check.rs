//! `fs3-arch-check` — the architecture drift gate.
//!
//! Reads the live workspace graph and judges it against
//! `testkit/arch-allowlist.toml`. Exit 0 means the crate graph still is what
//! workshop 001 says it is. Wired into `harness checks`.

use std::process::ExitCode;

use fs3_testkit::arch;

fn main() -> ExitCode {
    let allowlist = match arch::allowlist() {
        Ok(allowlist) => allowlist,
        Err(error) => {
            eprintln!("arch-check: {error}");
            return ExitCode::FAILURE;
        }
    };

    let graph = match arch::workspace_graph() {
        Ok(graph) => graph,
        Err(error) => {
            eprintln!("arch-check: {error}");
            return ExitCode::FAILURE;
        }
    };

    let violations = arch::check(&graph, &allowlist);
    if violations.is_empty() {
        println!(
            "arch-check: ok - {} crates, {} direct edges, 0 violations",
            graph.crates.len(),
            graph.crates.iter().map(|c| c.deps.len()).sum::<usize>()
        );
        return ExitCode::SUCCESS;
    }

    eprintln!(
        "arch-check: {} architecture violation(s) - the crate graph has drifted from workshop 001",
        violations.len()
    );
    for violation in &violations {
        eprintln!("  - {violation}");
    }
    eprintln!(
        "\nThe allow-list is testkit/arch-allowlist.toml; docs/how/architecture.md explains why."
    );
    ExitCode::FAILURE
}
