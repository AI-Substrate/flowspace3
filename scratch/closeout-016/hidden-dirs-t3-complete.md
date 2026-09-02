# hidden-dirs t2+t3 complete

## T2

Both discovery call sites use `global OR per-root`. Root scan/re-add fixtures prove default-only `src/b.ts`, opted-in `.hidden/a.ts`, and permanent `.git`/`node_modules`/`.venv` denial. Watcher rescans query the current stored policy, proven by enabling hidden, indexing a live hidden file, disabling via re-add without restarting, and rejecting a subsequent hidden file. Independent roots/watch mutations each turned their designated test red; both restored green.

## T3

Default discovery names hidden directory prunes once and add output aggregates them as `hidden=N`; the fixture reports `hidden=2`. Status JSON and human output show `include_hidden` per root. Cwd/absolute-root tree JSON and human output show the resolved root policy. Parser fixture/standard-ignore/subtree tests, daemon ledger/status/tree tests, and CLI renderer tests pass. Removing hidden aggregation turned the designated daemon test red; restored green.

DDoc rows `t2`/`dw-3102`/`bp-0002`/`ac-0002` and `t3`/`dw-3103`/`bp-0003`/`ac-0003` are checked and receipted via global `ddocs`.

Friction: read-only Flowspace semantic search at `:7373` became unavailable during T3; captured via `harness observe`; no production service was touched.

Request: t4 gate slot per the ruled packet. I will run the scoped regression set first, then `harness checks` only once the slot is confirmed free.
