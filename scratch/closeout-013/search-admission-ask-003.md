# search-admission STOP-and-ask 003 — plan-shape tripwire crashed shared Postgres

## Verdict

STOP. The first focused `search_plan_shape` run is red and affected the shared `:5433` postmaster.

Command:

```text
FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5433/flowspace3_test_search_admission \
  cargo test -p fs3-store search_plan_shape -- --nocapture
```

Observed:

- The test seeded its own newly created database with exactly 50,000 `elements`, 10,000 `smart_content`, and 10,000 smart embeddings.
- It ran for 648.02 seconds, then failed at the shared EXPLAIN fetch with `UnexpectedEof: expected to read 5 bytes, got 0 bytes at EOF`.
- Immediate read-only `psql` against the container returned `FATAL: the database system is in recovery mode`.
- The helper created a database named with prefix `fs3_search_plan_`; the exact suffix was not emitted before the connection loss. No cleanup command has been attempted.
- Because shipped and old EXPLAIN calls currently share one helper/failure line, the output does not prove which EXPLAIN lost the connection. I will not infer it and will not rerun.

This is the packet's red-tripwire condition. No full store suite or harness gate was run; o-prime already holds that slot for plan 012.

## Current uncommitted state

- Old-query parity golden passes independently.
- `search_elements` has an uncommitted one-time admitted-elements/smart-map/admitted-sources rewrite.
- The new shape test is uncommitted and currently unsafe.
- No production daemon request was made, but the test database shares the production Postgres postmaster on `:5433`.

## Ruling needed

1. Who owns confirming postmaster recovery and removing only my leaked `fs3_search_plan_%` database?
2. May I redesign the mutation proof so only the shipped query receives bounded `EXPLAIN (ANALYZE, ...)`, while the pathological old query is checked with bounded non-ANALYZE `EXPLAIN (FORMAT JSON)` for the correlated subplan/materialize shape?
3. What hard `statement_timeout` should guard the shipped prod-shaped EXPLAIN (proposed: 30 seconds), and should timeout itself be a red contract verdict?
4. Should I retain the current query rewrite for inspection, or revert it before further work?

No further code or database work until this ruling.
