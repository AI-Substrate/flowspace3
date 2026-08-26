# checks — the quality gate

`harness checks` is the deterministic proof that the code is correct. Run it
before you call any work done; do not substitute your own reading of the diff.

## What it computes deterministically

Three gates, in order, stopping at the first red:

1. `cargo fmt --all --check` — formatting is not a matter of opinion here.
2. `cargo clippy --all-targets -- -D warnings` — lint warnings are failures.
3. `cargo test --all` — the test suite.

Envelope: `ok` when every gate passed · `error` (exit 1) naming the first failing
gate, with the last 40 lines of its output in `error.details` · `degraded` when
the repo has no `Cargo.toml` yet (nothing to prove — this is honest, not a pass).

## What is expected back from you

- A red gate is a verdict, not a suggestion: fix the code, re-run, do not route
  around it. If the gate itself is wrong, fix the gate first (edit
  `extension.ts`), re-run so it points at the real problem, then fix the code.
- Adding a new class of proof (coverage, `cargo audit`, schema checks)? Add it
  to the `GATES` list here rather than to a call site — every caller, including
  `harness boot`, picks it up for free.
