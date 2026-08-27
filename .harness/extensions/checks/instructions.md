# checks — the quality gate

`harness checks` is the deterministic proof that the code is correct. Run it
before you call any work done; do not substitute your own reading of the diff.

## What it computes deterministically

Six gates, in order, stopping at the first red — with the one that WRITES
wrapped in a seventh check that straddles it:

1. `cargo metadata --locked` — `Cargo.lock` matches the manifests. First
   because it costs two seconds and no compilation, and because everything
   below it is otherwise a statement about a dependency set that is not the one
   that ships: `release.yml` builds with `--locked`, while a plain `cargo test`
   silently updates the lock and goes green. Fix by running any cargo command
   without `--locked` and committing the result.
2. `fs3-test-db-check` — `FS3_TEST_DATABASE_URL` names the database the test
   gate may WRITE to. There is no default, deliberately: until 2026-08-27 the
   test helpers fell back to the shipped address, which on a developer machine
   is the real store, and a `harness checks` run migrated a production database
   through `flowspace3 doctor` (which repairs, so it applies migrations). Export
   it at something disposable; the refusal prints the command. CI sets it
   itself.
3. `cargo fmt --all --check` — formatting is not a matter of opinion here.
4. `cargo clippy --all-targets -- -D warnings` — lint warnings are failures.
5. `cargo test --all` — the test suite. Needs the compose stack up
   (`docker compose up -d`): the store tier tests run against real Postgres and
   fail, naming that command, rather than skipping.
   **Straddled by `prodguard`**: the schema version of the database this machine
   calls PRODUCTION is read immediately before and immediately after this gate,
   and any difference fails the run. Every other defence in this repo is a rule
   about a leak path somebody already knows about — the `testdb` gate above, the
   `fs3_testkit::spawn` seal, the `spawn_isolation` source scan, the daemon's own
   boot refusal. Each was written the day after something got through it, and on
   2026-08-27 migration 0012 got through all of them via a test that spawns the
   real daemon and calls none of the helpers. A before/after comparison does not
   need to know the path, so it catches the class rather than the instance. The
   after-snapshot runs even when the tests FAILED: a run can migrate production
   on its way down, and that is the more serious of the two findings, so it is
   reported first. `absent` (no production store here) and `same-as-test` (CI,
   where the shipped default legitimately names a disposable service container)
   are passing answers, not skips — see
   `crates/daemon/src/bin/migration_guard.rs`. The probe never writes, never
   migrates and never creates; a guard that repairs is a guard that can cause
   the incident it watches for, which is exactly what calling
   `flowspace3 doctor` here would be.
6. `cargo run -p fs3-testkit --bin fs3-arch-check` — architecture drift. The
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
