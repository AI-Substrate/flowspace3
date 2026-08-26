# checks — the quality gate

`harness checks` is the deterministic proof that the code is correct. Run it
before you call any work done; do not substitute your own reading of the diff.

## What it computes deterministically

Five gates, in order, stopping at the first red:

1. `cargo metadata --locked` — `Cargo.lock` matches the manifests. First
   because it costs two seconds and no compilation, and because everything
   below it is otherwise a statement about a dependency set that is not the one
   that ships: `release.yml` builds with `--locked`, while a plain `cargo test`
   silently updates the lock and goes green. Fix by running any cargo command
   without `--locked` and committing the result.
2. `cargo fmt --all --check` — formatting is not a matter of opinion here.
3. `cargo clippy --all-targets -- -D warnings` — lint warnings are failures.
4. `cargo test --all` — the test suite. Needs the compose stack up
   (`docker compose up -d`): the store tier tests run against real Postgres and
   fail, naming that command, rather than skipping.
5. `cargo run -p fs3-testkit --bin fs3-arch-check` — architecture drift. The
   crate graph is judged against `testkit/arch-allowlist.toml`, which is an
   allow-list: an edge nobody added deliberately is a failure.

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
