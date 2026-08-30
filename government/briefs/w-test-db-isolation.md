# w-test-db-isolation — deterministic gates under concurrent seats

**Ruled by Jordan 2026-08-29 ("brief that database fix, it's important").
Closes backlog row 58.** One packet, one coder seat. Found by flea (fleet
alarm) + nigel (cluster-domain analysis); interim ruling (per-seat DBs by
discipline, rerun-until-green forbidden) becomes durable machinery here.

## The defect

Every seat's `harness checks` points `FS3_TEST_DATABASE_URL` at ONE
`flowspace3_test` on one Postgres. Under concurrent gating a DIFFERENT
innocent test reds per run — loser-by-timing contention. Two aggravations
(nigel + flea, both measured):

- **Per-database isolation does not bound the failure** — the CLUSTER is a
  shared domain: one backend crash triggers cluster-wide recovery that kills
  every connection in every database (observed at 219% CPU under ten
  concurrent suites). The tell is whole-suite-fails-together /
  connection-shaped errors, vs a single assertion failure.
- **checks DISCARDS the evidence** — details keep only the cargo stdout tail
  (no panic text, no tally), so no past red can be classified and no green
  can be proven connection-clean. "A green that means nothing is strictly
  worse than a red that means nothing" (flea).

The mechanism already exists and is unused (tenet-16 shape):
`crates/testkit/src/fresh_database.rs` — `FreshDatabase::create_from` mints an
entropy-named DB and applies embedded migrations. It shipped in #48 for the
sandbox; the gate never adopted it.

## Fix order (BINDING — flea's, ratified; do not reorder)

1. **RETENTION** — `harness checks` (the cargo-test step) captures the failing
   suite's stderr tail into the check details, so every future red carries its
   own evidence.
2. **CLASSIFICATION** — connection-shaped failures (io error, connection
   closed/reset, pool timeout, "terminating connection", whole-suite-fails-
   together) produce a LOUD infrastructure verdict distinct from a test
   failure: the gate says "your infrastructure failed, this red is not about
   your code" — and NEVER banks a PASS over a suite that lost its connection
   mid-run.
3. **FreshDatabase per-run mint** — the test harness (a shared testkit helper
   used by every DB-touching test binary, or a per-run env layer in checks)
   mints `fs3_test_<entropy>` via `FreshDatabase`, runs against it, drops it
   at exit. Two concurrent seats can no longer share a database by accident.
   Also: sweep/drop orphaned `fs3_test_*` DBs from crashed runs (age-based).

Retention first because without it neither of the other two can be verified
against reality; classification before mint because mint narrows but does not
eliminate the cluster-domain failure and we must be able to SEE it.

## Scope fence

- IN: the checks cargo-test step's evidence capture + verdict wording,
  testkit adoption of FreshDatabase for DB-bound tests, orphan sweep, tests
  for all three layers.
- OUT: running a per-seat Postgres cluster (cluster is still shared — named
  limitation, the classification layer is its mitigation); CI runner changes
  (own machines, unaffected); prod daemon config.

## Done-when

- d1: a forced test failure's stderr tail is present in the checks details
  (test proves retention).
- d2: a simulated connection-kill mid-suite yields the infrastructure verdict,
  not a test-failure verdict, and no PASS is banked (test proves it).
- d3: two `cargo test --all` runs launched concurrently from two checkouts
  each mint their own DB, both green, zero cross-talk (proven by run receipt);
  orphaned test DBs from a killed run are swept.
- d4: `harness checks` green; conventional commit; PR into main.
