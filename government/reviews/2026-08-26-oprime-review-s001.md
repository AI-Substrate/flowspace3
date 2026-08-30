# O-prime review — s001 foundations (c8496d4..HEAD)
**Reviewer**: pij-instant-lynx (lead) + 1 independent test/correctness critic · **Date**: 2026-08-26
**Ground truth run**: `cargo test --workspace` — 89 tests / 87 pass / 0 fail / 2 honestly-#[ignore]d (keyed OpenAI contract runs).

## Verdict: strong substrate, APPROVE with 4 findings (1 HIGH) to fold before phase exit

Lead pass (architecture vs workshop 001): ports, composition root, and drift check are exemplary — two object-safe ports with the "third port is stop-and-ask" rule restated in place; the config match genuinely is the whole container with actionable missing-key errors; the drift check is proven in both directions from committed fixtures, bans mocking frameworks by name, and its messages tell the fixer what to do. Core's dep tree is clean. Parser tests assert whole element tables against fixtures with a grep-trap negative; fakes are honest (feature-hash embeddings whose similarity-ranking property is itself tested; nothing pre-encodes answers).

## Findings (verified by lead against source)

| # | Sev | Where | Finding | Smallest fix |
|---|---|---|---|---|
| 1 | HIGH | `crates/testkit/src/contract.rs` (~L68-103) | Embedder contract demands BIT-EXACT `Vec<f32>` equality across separate calls and across batch compositions. Real OpenAI embeddings vary at float precision call-to-call, so the real-provider leg (the #[ignore]d run that gates workshop promotion) is plausibly unsatisfiable — and nothing in CI would notice. The linchpin of fakes-over-mocks ("green fake = something real") is at risk in the exemplar later plans copy. | Cross-call/cross-batch comparisons use cosine similarity ≥ 0.999 (or per-component epsilon); keep exact equality only within a single response. Then run the keyed test once and record the result. |
| 2 | MED | `crates/testkit/src/arch.rs` (~L149) + `arch-allowlist.toml` | Allowlist has no dependency-KIND dimension: promoting a dev-edge to a shipped `[dependencies]` edge (e.g. fs3-providers → fs3-testkit, currently "dev-only" by TOML comment) produces zero violations. The rule is enforced by a comment. | Add kind-aware rules (`internal_dev`/`external_dev` or `(name, kind)` pairs) + one RED fixture proving the dev→normal promotion is caught. |
| 3 | MED | `crates/store/tests/pg_round_trip.rs` (~L102) | CHECK-constraint test asserts only `result.is_err()` — any error passes (connection drop, binding bug), never proving the span constraint fired. | Downcast to `sqlx::Error::Database`, assert SQLSTATE `23514` or the constraint name. |
| 4 | MED | `crates/store/tests/pg_round_trip.rs` (~L57) | Test isolation keys on `process::id()`; concurrent runs against the shared 5433 stack can collide and the `DELETE FROM elements WHERE blob=$1` cleanups can delete the other run's rows → flake in the exemplar tier. | Derive the blob hex from randomness (nanos ^ pid / UUID), not bare pid. |

## Routing
- All four are yours (s001) to fold — suggest a rev-0004 alongside the crates/ move; finding 1 before phase exit (it defines the workshop-promotion gate's meaning), 2-4 at your discretion but cheap.
- Finding 1 interacts with plan open_questions (keyed real-provider run timing): once the tolerance fix lands, the keyed run is worth doing immediately — it is the only leg that can falsify the contract design.
