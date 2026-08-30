# w-arch-compare-pij — compare flowspace3's Rust architecture to pij's rust port

**From**: pij-instant-lynx (o-prime) · 2026-08-30 · requested by pij-dominant-
vicuna (pij o-prime) on Jordan's behalf: "have a gpt-5.6-sol-fast OMP agent
review and COMPARE flowspace3's Rust architecture to our pij port's, allowing
for the deliberately differing implementations — where we diverged from your
mold and whether the divergence is principled or drift."

## The two sides

- **flowspace3 (the mold)**: this repo, main. Read plan-001 (docs/plans/) for
  the architecture doctrine, then the workspace itself: crates/core (pure),
  store (sqlx Postgres), parsers, providers, daemon (axum; key published
  before bind), cli, testkit. Architecture allowlist gate in testkit.
- **pij rust port**: AI-Substrate/pij branch s108/rust-port (head ~dab48c70).
  8-crate hexagonal workspace: core (7 async ports, no tokio/IO), store
  (sqlx SQLite, WAL-on-connection, BEGIN IMMEDIATE), tmux, harnesses (5
  adapters), transport, daemon (axum + publish-key-AFTER-bind — NB: that is
  their row-53-informed choice and differs from fs3's publish-BEFORE-bind;
  examine both and judge), cli, testkit (fakes + pij-gate). Rulings:
  docs/plans/108-rust-port/assets/workshops/001-architecture.md. 181 tests,
  arch gate green. Waves 0-2 done.

## Deliberate divergences (do NOT report as defects; judge them)

- queue-first delivery (their R4)
- v1-native event schema (their R7)
- async-trait over RPITIT (their R3-AMEND-2)

## Deliverable

A short compare report at
`/Users/jordanknight/substrate/flowspace/flowspace3/scratch/arch-compare-pij.md`:

1. **Convergent choices** — where both codebases independently or by
   inheritance settled the same shape (one line each).
2. **Divergences, each with a verdict** — `principled` (defensible for their
   domain), `drift` (unjustified deviation from the mold), or
   `superior-adopt` (theirs is better; fs3 should take it). Evidence per
   verdict: file:line on both sides, one sentence why.
3. **What they should have taken but didn't** — fs3 patterns absent from the
   port that would pay for themselves (with the concrete fs3 cite).
4. Confidence/limits section (what you could not verify).

READ-ONLY on both repos. Report the path back to pij-instant-lynx, who
relays to pij-dominant-vicuna. No urgency; correctness over speed.
