# 012b fresh database follow-up report

## Delivery

- Branch: `012b-fresh-db-followups`
- Commit: `def413c13205aa63a0f02ec7ea51a7fc0c27ea92`
- PR: https://github.com/AI-Substrate/flowspace3/pull/99
- Exact-head CI: pass in 5m20s, run `33588739891`
- Commit attribution: `harness commit` direct-verified; `refs/notes/ai` landed.

## Shipped

1. `unique_seed_created_at` accepts only timestamps from 2020-01-01 through now+1h. The coupling to `unique_seed`'s low-64-bit nanosecond layout is pinned by a within-one-second round-trip test.
2. Create-path test instrumentation is constructed inside `create_database_permit`, leaving the shipped `create_database` body with one ordinary permit acquisition and no `cfg(test)` lines. N=8 and cross-runtime tests remain on the real create path.
3. Attributed DDL probe shipped executable at `bin/ac-0001-ddl-probe.sh`, byte-identical to the governance source except both usage paths. ac-0001 now points at the shipped command.
4. Full review record committed under `docs/plans/012-fresh-db-serialise/assets/reviews/` at the ruled hashes.
5. Active plan, implementation-guide, backpressure, and task text target the landed `flowspace3-db-test` postmaster on :5434 and record the reviewer delta APPROVE status. Historical :5433 measurements remain verbatim in immutable receipts/review evidence. Plan status is `shipped`.

## Evidence

- Clamp green: `cargo test -p fs3-testkit legacy_seeded_database_names_have_plausible_ages` → 1 passed (`artifact://146`). Clamp removed → red on pre-2020 bound (`artifact://136`); restored.
- Hook green: `cargo test -p fs3-store serialised` → 2 passed (`artifact://144`). Semaphore expanded to 8 → both tests red at 8 concurrent creates (`artifact://140`); restored.
- Shipped probe `--check`: guards passed and no work ran.
- Shipped real probe: `store: exit=0 samples=173 max_concurrent_ddl=1 samples_with_ddl=104 samples_over_1=0 foreign_ddl_max=0`.
- Review hashes: JSON `12c2eba781e4ad71ab3bd4c46ac84b95`; Markdown `e88a596ae8e5296f629058cc7e59f894`.
- `ddocs validate`: plan, implementation guide, backpressure, tasks, and review record all zero errors/warnings.
- Rust-analyzer diagnostics: clean for both changed Rust files.
- Exact-head CI gate: pass in 5m20s.

## Assumptions

- Historical review and execution receipts stay historical; they are not rewritten from :5433 to :5434.
- Production orphan operations remain o-prime-owned.
- This follow-up changes only the reviewer-named clamp/test construction plus durable proof artifacts; serialization behavior remains the independently approved 16 → 2 → 1 result.

## Shared observation buffer — listed, not cleared

- `DL-001` Rust LSP returned false-empty FreshDatabase references; Serena cross-check timed out.
- `DL-002` real-time orphan fixtures selected ambient shared-server databases.
- `DL-003` commit `05d7d87` attribution note missed despite connected ingress.
- `DL-004` old shared test server was shutting down during a read-only count.
- `DL-005` the original checkpoint-count gate measured the wrong invariant.
- `DL-006` parallel `ddocs set` calls raced on one temporary rename.
- `DL-007` commit `f3aec31` attribution note also missed despite connected ingress.
- `DL-008` an earlier exclusive gate failed on ENOSPC.
- `DL-009` rs does not support the promised `pij whoami --json` envelope.
- `CONF-001` pushed rs seats do not support `pij inbox --wait`.
- `CONF-002` restart handoff named review artifacts outside this worktree before the ledger was copied in.
- `DL-010` `flowspace3 status` timed out despite healthy doctor/search.
- `DL-011` Rust LSP returned zero references for heavily used `fs3_store::create_database`.
- `DL-012` the first copied DDL probe was the obsolete unattributed :5433 script.
- `DL-013` LSP tooling created an untracked `.serena/`; common-dir exclude now prevents recurrence.
- `DL-014` the rs wire-v2 cutover temporarily made inbox/send replies unreadable.

Buffer remains intact for o-prime-owned draining.
