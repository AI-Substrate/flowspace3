# ts-grammar step 4 / stop-and-ask 002 — local gate requires database

## Evidence

- Parser/core regression is green: `cargo test -p fs3-core -p fs3-parsers` ran 311 tests across 12 suites; all passed.
- `harness checks` was invoked in the granted exclusive slot and stopped before the quality gate because `fs3-test-db-check` refuses when `FS3_TEST_DATABASE_URL` is unset.
- Exact gate message: `REFUSING TO RUN: FS3_TEST_DATABASE_URL is not set.`
- No database, daemon, or prod target was started or touched, preserving the packet's parsers/core-only constraint.
- Friction captured as `.harness/temp/agent/session-buffer.md` entry `DL-001`.

## Ruling needed

Choose the proof route:

1. Keep the packet's no-database boundary and use the plan-authorized CI gate on the exact PR SHA after t5; or
2. Amend the boundary to permit `FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test` for a local `harness checks` rerun.

The exclusive local gate slot is released now. No cargo/harness gate remains running.
