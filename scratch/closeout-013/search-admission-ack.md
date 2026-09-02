# search-admission coder acknowledgment

## CANARY

- `CANARY-OK`
- pij id: `pij-imperial-weasel` (`pij whoami`)
- spawnId: `s1788313083636-82923` (`PIJ_SPAWN_ID`)
- model: `github-copilot/gpt-5.6-sol-fast-1m`
- cwd/worktree root: `/Users/jordanknight/substrate/flowspace/fs3-search-admission`
- branch: `013-search-admission`
- seat/delivery: rs; o-prime `pij-binding-magpie`; I did not and will not run `pij adopt`
- Rust LSP: `rust-analyzer` configured; not started until a request needs it
- pre-code tree status: untracked `.serena/`; left untouched

## Read of the unit and measured reason

This unit changes only the nearest-search SQL in `crates/store/src/embeddings.rs` plus standalone tests/goldens/fixtures in `crates/store/tests/**`. Public store APIs, scoring, candidate expansion, chunk collapse, migrations, indexes, daemon code, and envelope shapes stay unchanged.

The profile attributes **261.2 of 380 CPU-seconds (68.7% of DB CPU)** to the search CTE, including **219 CPU-seconds above the background floor**. One reference search performs **962,792 `smart_content` probes** and **3,851,137 of 3,853,170 shared-buffer hits (99.95%)** to return 40 rows, taking **1,667 ms**; a second run reaches **1,698,017 probes / 6,793,986 buffers / 2,725 ms**. The HNSW scan itself is only **12.4 ms / 1,078 buffers**. Expansion can rerun the query up to nine times. The target is smart-content loops below 1,000, total shared hits below 100,000, and execution below 300 ms on the production reference query, with identical ranking.

Source correction/evidence: the current worktree's `candidate_vectors` CTE already selects only `(source_hash, source_kind, chunk_no, distance)`; there is no vector column to remove in this branch. The pathological correlated admission remains at `embeddings.rs:568-618`, so the substantive rewrite is still required.

Filter semantics I will preserve at every chooser/resolver where shared hashes can otherwise cross scope:

- vector-side: `model_key`, `source`, `max_distance`;
- element classification: `kinds`, `id_kinds`, `gate_open` (including unknown-state behavior), `ddoc_schema`, exact `conversation` address prefix;
- ownership/anchor: `repo`, `path`, and `worktree`, through both `worktree_files/worktrees/repos` and `turns/conversations`;
- output behavior: `limit`, candidate count/under-fill expansion, deterministic smart-map tie order, element representative order, `DISTINCT ON` collapse, chunk number, distance/scoring, and final order.

## Numbered implementation plan

1. Capture the old query's top-N addresses and scores as the committed parity golden for limits 10 and 40, exercising repo, path glob, vector source, element kind, and conversation filters; keep the old SQL as the explicit mutation oracle.
2. Add the prod-shaped fixture and EXPLAIN JSON test first: at least 50k elements and 10k smart-content rows, assert smart-content node loops are at most `candidate_limit`, and reject `Materialize` over a sequential scan of `elements`. Prove the tripwire turns red with the old SQL.
3. Rewrite admission into one-time sets: build the caller-filtered admitted-element relation once; map eligible smart `(text_hash, raw_hash)` once; expose deduplicated raw/smart admission keys; semi-join candidate embeddings to those keys so duplicate elements or mappings cannot duplicate candidates or change `candidate_count`.
4. Preserve the full predicate matrix above in admission and in smart/element representative selection. Keep bind positions and one-statement NULL-means-any behavior unchanged. Preserve the current four-field `candidate_vectors` projection.
5. Run the parity test against the old-query golden at `1e-6` score tolerance and verify limit/expansion/collapse behavior, especially shared summaries, shared raw hashes, conversation anchors, and smart-map tie ordering.
6. Run focused store tests, then the existing store and daemon search suites one cargo invocation at a time. Ask o-prime for the `harness checks` slot; a red shape tripwire means the per-candidate plan remains, a red parity tripwire means admission/ranking semantics changed, and either is STOP-and-report rather than a workaround.
7. Update deterministic task rows and the execution log after each task, commit through `harness commit`, open the PR with the old-SQL mutation and EXPLAIN before/after, then hold for o-prime's bounce.
8. After the bounce, run only the required read-only production EXPLAIN and real `flowspace3 search` timings; record AC-0004/0005 receipts and report done. Never use production `:7373` as a test target.

## Scope and assumptions

- Test DB: isolated per-run database on `:5433`, seat-specific label; never production.
- No index, migration, dependency, public API, daemon, harness-government, or plan-flow-state changes.
- Any required path outside `crates/store/src/embeddings.rs`, `crates/store/tests/**`, or a new owned fixture is a stop-and-ask.
- Assumption to verify in tests: transforming correlated existential admission into deduplicated admission keys preserves existential multiplicity (one admitted candidate regardless of how many eligible elements or smart mappings match).

## Canary friction already captured

- `DL-001`: Serena `initial_instructions` MCP timed out twice; the native Rust LSP surface is configured and will be used instead.
- `CONF-001`: rs `pij whoami` returned `pij-imperial-weasel`, while `whoami --json` refused `E-RS` and `pij node show` returned `E-NOID`; spawn metadata came from `PIJ_SPAWN_ID`. No adoption attempted.

No code changes before the ruling. Waiting for `.harness/temp/agent/search-admission-prime-reply-001.md`.

## Pre-code harness receipt

- Harness CLI: `0.14.0`.
- `harness boot --json`: `degraded`; toolchain/crate/build passed, including `cargo build --all-targets`; compose database service is not running.
- After the ruling, the first executable prerequisite is the isolated `:5433` test stack—not production.
- Conversation ingest could not resolve this rs session identity, consistent with `CONF-001`; nothing was ingested.
- No product code changed while awaiting the ruling.
