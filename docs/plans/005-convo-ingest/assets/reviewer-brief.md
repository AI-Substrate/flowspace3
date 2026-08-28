# Reviewer brief — plan 005-convo-ingest

You are the cross-model reviewer. `packet-reviewer.dd.md` is your packet; this
file fills the scope fields it left as placeholders and tells you where the
bodies are buried.

## Scope

| field | value |
| --- | --- |
| worktree | `/Users/jordanknight/substrate/flowspace/fs3-convo-ingest` (the PM's; READ-ONLY to you except `docs/plans/005-convo-ingest/assets/reviews/`) |
| branch | `005-convo-ingest`, 65 commits, 114 files, +27,364 / −107 vs `main` |
| base | `main` — diff with `git diff main...005-convo-ingest` |
| scratch db | `postgres://flowspace3:flowspace3@127.0.0.1:5433/fs3_convo_ingest` via `FS3_TEST_DATABASE_URL`. NEVER `docker compose up`; Postgres is already up for the fleet on `:5433`. |
| PM | `pij-pale-silkworm` — verdict and findings by `pij send`, one file pointer, no pasted bodies |

## What was built

Four native session stores become searchable conversations. Readers in
`crates/providers/src/conversation_sources/{claude,omp,pij_ledger,metrics_db}.rs`
behind a phase-1-frozen port in `crates/core/src/conversation_source.rs`;
durable cursors and an ordinal ledger in `crates/store/src/ingest_cursors.rs`;
a pure normaliser in `crates/core/src/conversation_normalize.rs`; the join in
`crates/core/src/conversation_join.rs`; the composition root in
`crates/daemon/src/convo_ingest.rs`; the CLI verb and the `harness convo`
extension.

## Where I would look first, if I were you

These are the places I am least confident, named so you spend your review where
it can find something rather than where I already looked.

1. **`crates/daemon/src/convo_ingest.rs` — the pipeline order.** The header
   upsert must precede `ledger_view` (foreign key) and `commit_poll`. I
   restructured it twice. Check the empty-batch and known-conversation paths in
   particular.
2. **The accounting backstop.** `accepted + already_stored == prepared.turns.len()`
   or the ingest fails. Is that reachable? Is failing the right call, versus
   reporting and continuing?
3. **`conversation_guid` determinism.** A v8 uuid from a sha256 of
   `(harness, session_id)`. If two different sessions can collide, ac-0002's
   mechanism becomes a data-loss mechanism. I believe not; I would like it
   checked by someone who did not write it.
4. **`discover_folder` retry.** Added after first light. It retries resolve ONCE
   with a folder read out of the session's own `cwd`. Consider: a session whose
   `cwd` no longer exists, two stores holding the same id, and the claude path,
   which I could not exercise live.
5. **The cardinality claim** in `crates/testkit/src/expectations.rs` and its
   generator `docs/plans/005-convo-ingest/assets/inputs/tools/oracle_expectations.py`.
   It is a SECOND implementation of four allowlists and two grouping rules. If
   my derivation and a reader are wrong the same way, the test agrees with the
   bug. I mutation-checked it against two regressions; a third pair of eyes on
   the derivations themselves is worth more than re-running my mutations.
6. **Turns that should be there and are not.** u1a named this as the one class
   its fixtures cannot rule out: a reader silently DROPPING a record it should
   keep is legal under every structural claim, because fewer records is a valid
   subsequence. The cardinality claim narrows it but shares the derivation.

## Claims I have already made, with their evidence

Judge these; do not take them. Everything below is in
`docs/plans/005-convo-ingest/plan.dd.md` (ACs, each with a receipt) and
`docs/services/convo-ingest.md` (the first-light transcript).

- `harness checks` green, nine gates including arch, on the composed branch.
- `harness plan validate` — 0 errors, 0 warnings.
- First light ingested the PM's own live omp session by pij id: 739 turns, then
  752 on a re-poll (delta only), then 804. Searchable by meaning. The native
  route landed in the SAME conversation.
- Submit latency 10–40 ms; five rapid submits collapse to one queued job.

## Rulings you should know before calling something wrong

All are in the impl-guide's architecture section, which is BINDING:

- Readers live in `providers`, not `parsers` (SA1). `ConversationSource` is the
  third port (SA2), blocking not async.
- Thinking blocks are dropped AT THE READER, in every reader. Measured: claude
  carries 21 thinking blocks with ZERO bytes of text; omp carries 42,161 bytes.
- An ordinal is an OPAQUE identity — nothing orders ordinals. The pij ordinal is
  a decimal string, so lexicographic order is not numeric order; zero-padding
  was proposed and refused because the committed expectations pin the strings.
- A merged group's ordinal is its FIRST member. The grouping rule is part of the
  persisted contract — for metrics-db that includes the row-selection predicate.
- A group split across two polls yields TWO turns, permanently. Ruled twice, for
  claude and metrics-db, against one seat's own recommendation.
- Composition is wiring: unit-internal rework here is a phase-1 contract defect.
  If you find one, say so — that is a finding, not a fix request.

## Known-open, already recorded — not findings

- **Ingest starves behind enrichment** (first light). Deferred by prime to a
  follow-up packet, `w-ingest-lane`. Do not re-report; DO report if you think it
  is worse than described.
- **`unwrap_or_default` on `Row::at` in `metrics_db.rs`** yields an empty-string
  timestamp on a row with neither an ISO stamp nor `event_ts`. Found by its own
  author after stand-down, unreachable on the committed fixture, awaiting a PM
  ruling on whether to fix in-plan. If you have a view on that call, say so.
- **`TurnSource::Peer` is detected from a body prefix**, in two readers, because
  no store records a flag. Recorded as debt.
