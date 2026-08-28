# checks — the quality gate

`harness checks` is the deterministic proof that the code is correct. Run it
before you call any work done; do not substitute your own reading of the diff.

## What it computes deterministically

Seven gates, in order, stopping at the first red — with the one that WRITES
straddled by the production guard:

1. `cargo metadata --locked` — `Cargo.lock` matches the manifests. First
   because it costs two seconds and no compilation, and because everything
   below it is otherwise a statement about a dependency set that is not the one
   that ships: `release.yml` builds with `--locked`, while a plain `cargo test`
   silently updates the lock and goes green. Fix by running any cargo command
   without `--locked` and committing the result.
2. `fs3-test-db-check` — `FS3_TEST_DATABASE_URL` explicitly selects disposable
   Postgres server credentials. It is a base selector, not the database the
   suite writes: the test runner below mints a child beside it. There is no
   default, deliberately; the refusal prints the command. CI sets it itself.
3. `node --test .harness/extensions/checks/check-result.test.mjs` — the gate's
   own output-retention and failure-classification contract.
4. `cargo fmt --all --check` — formatting is not a matter of opinion here.
5. `cargo clippy --all-targets -- -D warnings` — lint warnings are failures.
6. `fs3-test-suite` — sweeps `fs3_test_*` databases older than the named
   `ORPHAN_SWEEP_AGE`, mints and migrates a unique `fs3_test_<epoch>_<entropy>`
   child with `FreshDatabase`, injects only that URL into `cargo test --all`,
   then force-drops it. The sweep prints both its threshold and swept names.
   The suite needs the compose stack up (`docker compose up -d`) and fails,
   rather than skips, when Postgres is unavailable.
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
7. `cargo run -p fs3-testkit --bin fs3-arch-check` — architecture drift. The
   crate graph is judged against `testkit/arch-allowlist.toml`, which is an
   allow-list: an edge nobody added deliberately is a failure.

Envelope: `ok` only when every gate passed · ordinary `error` for a code/test
failure · `E_CHECKS_INFRASTRUCTURE` when the suite output contains
connection-shaped evidence, even if its child returned zero · `degraded` when
the repo has no `Cargo.toml`. Every red keeps separately labeled last-40-line
stdout and stderr tails in `error.details`, so one stream cannot evict the
other's evidence.

## What is expected back from you

- A red gate is a verdict, not a suggestion: fix the code, re-run, do not route
  around it. If the gate itself is wrong, fix the gate first (edit
  `extension.ts`), re-run so it points at the real problem, then fix the code.
- Adding a new class of proof (coverage, `cargo audit`, schema checks)? Add it
  to the `GATES` list here rather than to a call site — every caller, including
  `harness boot`, picks it up for free.
