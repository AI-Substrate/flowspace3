# Phase 2 execution log

## tk-e203 — u-c search honours the caller checkout

**Status**: complete

### Changes

- Store ranking now constrains code paths and conversation anchors to the resolved caller worktree before `LIMIT`; each hit returns its serving root.
- Search fetches `limit + 1`, returns at most `limit`, and reports measured truncation.
- Search hits expose additive `worktree` provenance.
- The HTTP envelope emits the advisory weak-match hint only for non-empty results below the calibrated 0.50 floor; ranking and filtering are unchanged.
- Canonical search guidance records checkout scope, envelope fields, and the 2026-08-28 Azure calibration snapshot.

### Evidence

- `cargo test -p fs3-daemon --lib search::tests` — 4 passed.
- `FS3_TEST_DATABASE_URL=…/flowspace3_test CARGO_INCREMENTAL=0 cargo test -p fs3-store --test pg_first_light` — 17 passed against unique `FreshDatabase` children; worktree filter contract passed.
- `FS3_TEST_DATABASE_URL=…/flowspace3_test CARGO_INCREMENTAL=0 cargo test -p fs3-daemon --test first_light` — 14 passed against unique `FreshDatabase` children; envelope contract passed.
- `FS3_TEST_DATABASE_URL=…/flowspace3_pij_qualified_knobbler FS3_CONFIG_DIR=.probe-config CARGO_INCREMENTAL=0 harness checks` — 9/9 gates passed in this worktree.
- Shared-stack `probe.sh` was not run: PM reserved the live P3 predicate proof for composition.

### Discoveries & learnings

| Tag | Finding | Resolution |
|---|---|---|
| Noteworthy | Native LSP references were unavailable fleet-wide: no language servers configured and no installed rust-analyzer component. | Reported immediately; exact-identifier sweep covered `SearchFilters`, `SearchHit`, and `SearchResults`. PM deferred toolchain repair until between waves. |
| Noteworthy | `/builder implement` asks for an in-progress dd state, but `builder/plan` permits only `unchecked`, `checked`, `blocked`, `human-skipped`, or `na`. | `tk-e203` remained honestly `unchecked` while active and was set `checked` only after final proof; PM carried the doctrine/schema mismatch upward. |
| Noteworthy | Shared `flowspace3_test` is not safe for a daemon because it contains fleet roots/jobs and ambient config can select paid providers. | No daemon was booted. Focused tests used unique `FreshDatabase` children with in-process `Config::default` fakes; final checks used a unique seat base database plus an empty config directory, then both were removed. |
| Noteworthy | The path filter was reported as potentially post-LIMIT. | Verified it already sits inside the nearest-neighbour CTE before `ORDER BY … LIMIT`; this unit does not change that behavior. The daemon also converts `?` and escapes literal `_` before SQL LIKE. Character classes remain unsupported and were left untouched per PM ruling. |
