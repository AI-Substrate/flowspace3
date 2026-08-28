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

## Composition correction — scoped representative resolution

Composition exposed scoped hits with `repo`, `path`, and `worktree` all null. The nearest-candidate gate admitted a raw hash because some caller-anchored element carried it, but the later representative resolver independently chose the globally lowest-id element with that hash. When the same element body appeared in several file blobs, that representative could belong only to another checkout; the provenance LEFT JOINs then resolved nothing while preserving the row.

The representative resolver now repeats the caller anchor before its `ORDER BY … LIMIT`, so shared content reports the caller-held path and address. A daemon-side hard guard drops and WARN-logs any scoped row that still resolves without provenance, including the raw hash and caller scope; the guard is diagnostic defence, not the primary fix, because using it alone would under-fill a page.

### Regression evidence

- RED before fix: `scoped_search_resolves_the_element_held_by_the_caller` failed with `identity: None, root_path: None, path: None` for the foreign lowest-id blob.
- GREEN after fix: the same test passed; its minimal fixture puts one raw element in two file blobs, indexes foreign main first so it wins the broken lowest-id race, then scopes to the later feature blob held by the caller.
- Full affected suites: store `pg_first_light` 18/18, daemon `first_light` 14/14, daemon `conversation_query` 7/7.
- Mutation proof on the minimal fixture: removing only the representative anchor made the test fail with the null provenance triplet; restoring it made the test pass.
- Independent reviewer evidence found 227 raw hashes where the caller passed the candidate gate while the global lowest-id element belonged to a blob the caller did not hold (`assets/reviews/runtime/uc-resolver-mismatch.json`).
