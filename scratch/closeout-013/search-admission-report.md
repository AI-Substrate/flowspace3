# search-admission coder report

## Delivery

- PR: https://github.com/AI-Substrate/flowspace3/pull/101
- pushed PR head: `065acfd465ac93a8632f0527959dd5552ecef3ab`
- tested code head: `beee1491be13f3920affc5d257eb580974188360`
- final documentation-only gate receipt commit: `065acfd465ac93a8632f0527959dd5552ecef3ab`
- CI: GitHub Actions gate passed in 5m29s, run `33591137360`, job `100125202572`
- review: HOLD; detached Claude reviewer is inspecting `beee1491`; no pushes after reviewer dispatch
- gate slot: released after green local gate and PR open

The branch rewrites `search_elements` admission without changing its public API. HNSW produces the first materialized page, capped at candidate limit. Caller-eligible elements, raw representatives, smart mappings, smart representatives, and raw/smart source keys resolve once. Admission post-filters only the bounded page. An internal nullable sentinel carries candidate count when a whole page admits zero hits, preserving expansion. JIT is disabled transaction-locally for this statement.

## Evidence

### Measured cause

- old production query: 1,667.423 ms; 962,792 smart-content probes; 3,853,170 total shared hits; 281.374 ms JIT; HNSW itself 12.4 ms / 1,078 buffers
- attributed CPU: 261.2 / 380 CPU-seconds = 68.7%, including 219 CPU-seconds above background

### New shape

Separate `flowspace3-db-test` postmaster on `127.0.0.1:5434` only:

- seed: 50,000 elements; 10,000 smart-content rows; 20,000 embeddings (10k smart + 10k raw)
- `EXPLAIN (ANALYZE, BUFFERS, VERBOSE, FORMAT JSON)`: 108.468 ms; 2,691 shared hits
- HNSW and `candidate_vectors`: 160 rows; smart-content max loops: 1
- childless `<=>` HNSW scan under the candidate Limit; all admission above it
- no correlated smart-content SubPlan; no Materialize over elements; four-field candidate target; no JIT
- old mutation is non-ANALYZE forever and must show correlated SubPlan + Materialize/elements

### Behavioral proof

- search admission tests: 2/2 — twelve old-query golden cases at `1e-6`, plus a 100%-foreign first HNSW page that expands to the full scoped limit
- shape tests: 2/2
- focused store search/filter/collapse suites: 53/53
- focused daemon search/conversation/oversize suites: 55/55
- direct full-suite rerun: green; output `.harness/temp/agent/search-admission-suite-rerun.log`
- final `harness checks`: green at 2026-09-02T04:30:38Z
- PR CI gate: green

The one captured full-suite `streaming::progress_is_reported_while_the_queue_is_still_draining` miss passed three consecutive correctly configured targeted reruns. Classified environmental timing under load; plan 013 changes no provider/progress code. The earlier production-version guard red was ruled false positive: o-prime bounced the daemon and installed migration 0023 while the guard window was open; no test touched production.

## Mutation and filter coverage

- mutation fixture: `crates/store/tests/fixtures/search_admission_old.sql`
- parity golden: `crates/store/tests/fixtures/search_admission_golden.json`
- model/source/distance: `embeddings.rs:509-511`
- kinds and ddoc id/state/schema: `:522-537`
- exact conversation: `:538-539`
- repo/path/worktree via files and conversations: `:540-557`
- deterministic shared raw/summary representatives: `:559-582`
- bounded admission and resolution: `:584-604`
- collapse/provenance/sentinel: `:606-656`, `:742-770`

## Assumptions

1. The daemon continues to call `fs3_store::search_elements` before lexical fusion; daemon fusion behavior is unchanged.
2. Candidate expansion remains the recovery mechanism for selective scopes. It requires pre-admission candidate count even when no row survives; the sentinel is internal and excluded from `SearchHit`.
3. Multiple eligible elements sharing one raw hash and multiple eligible raw hashes sharing one summary are existential, not multiplicative. Deterministic representatives preserve prior result identity and score.
4. The production corpus's HNSW index remains healthy; the required post-bounce production EXPLAIN is the proof, not this assumption.
5. O-prime owns merge and daemon bounce. This coder performs only read-only production proof after that bounce.

## Pending production proof

Blocked until reviewer acceptance, merge, and o-prime bounce:

- AC-0004: production reference EXPLAIN — smart loops <1,000, shared hits <100,000, execution <300 ms
- AC-0005: load-controlled real search — record open-job count immediately before/after each run; interleave scoped/unscoped; count only runs below approximately 50 open jobs; main query <1 s ×3, prior timeout queries <2 s, doc-heavy unscoped query from `/Users/jordanknight/substrate/chainglass` <5 s

## Observation buffer — listed, not cleared

1. `DL-001` degrading — Serena initial-instructions timed out twice; native Rust LSP used.
2. `CONF-001` degrading — rs identity inconsistent across `whoami`, JSON, and node lookup.
3. `DL-002` degrading — worktree compose hard-coded `/flowspace3-db`, colliding with existing container.
4. `DL-003` blocking — builder named absent repo-local ddocs; canonical CLI was global `ddocs`.
5. `DL-004` degrading — rust-analyzer returned zero references for exported `search_elements` despite known callers.
6. `DL-005` blocking — unsafe old-query ANALYZE ran 648 seconds and lost Postgres; permanent repair is non-ANALYZE mutation proof, 30-second shipped timeout, separate postmaster.
7. `DL-006` degrading — first WIP commit attribution note missed; later commits were verified.
8. `DL-007` blocking — new `:5434` postmaster initially timed out.
9. `DL-008` blocking — interrupted initialization omitted host maintenance authentication; o-prime repaired and proved CREATE/DROP.
10. `DL-009` blocking — first join-before-kNN rewrite exceeded 30 seconds; HNSW-first/post-filter design fixed it.
11. `CONF-002` blocking — inline rs ruling truncated and could not be tailed; durable pointer delivery restored it.
12. `DL-010` blocking — first post-filter rewrite lost candidate count on empty pages; sentinel repair restored scoped expansion.
13. `DL-011` blocking — pij rs wire v1/v2 cutover broke inbound delivery; session resumed as `pij-sharp-amistad` and durable rulings preserved continuity.
14. `DL-012` blocking at observation time, later ruled false positive — gate guard observed production 22→23 because o-prime bounced during the held slot; no test process touched production.

Buffer remains shared and uncleared, as required.
