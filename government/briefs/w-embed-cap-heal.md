# w-embed-cap-heal — the provider's 8192 cap rejection becomes a re-split signal (backlog row 117)

## What Jordan ruled (2026-09-02)
"have sol code, claude review" — coder on `github-copilot/gpt-5.6-sol-fast-1m`
(omp, effort high), reviewer on Claude. Sequential dispatch: this packet first,
because a peer government's 250-seat backfill is waiting behind it.

## Current state, written to be falsifiable in one read
- Plan 009 chunks embed inputs with `chunk_plan()` (`crates/daemon/src/enrich.rs`)
  using `estimate_tokens()` (`crates/core/src/tokens.rs:58`) — a bytes/3 estimate.
- Content denser than ~2.55 chars/token slips UNDER the ~7,500 window unsplit
  and the provider rejects it: `400 Invalid 'input[0]': maximum input length is
  8192 tokens` (`crates/providers/src/azure_openai.rs:458`, `openai.rs:116`).
- PROD RIGHT NOW: 5 embed jobs `state='failed'`, `attempts=3`, `terminal=false`
  — four `embed:git:github.com/AI-Substrate/pij:raw:…`, one
  `embed:conv:recovery:raw:c5a6be2d…` (a CONVERSATION, 2026-09-01). Max
  observed item 20,872 chars → estimated ~6,957 tokens, real >8,192.
- The impl-guide for 009 predicted exactly this (risk #2) and named the cap
  WARN as the last-resort guard. It fired. This packet closes it.

## The job
1. **Self-heal (the ruling: option c).** When the provider rejects a batch with
   the cap error, do NOT retry the same input into permanent failure. Identify
   the offending item(s), re-split them at a tighter ratio (halve the assumed
   chars/token for THAT input, or bisect), and re-issue. Bounded: a fixed
   maximum of re-split rounds, after which the job fails with a message that
   names the item, its char length, and the ratio reached — never a bare
   provider echo.
2. **Conservative floor (option a, small).** Lower the assumed chars/token
   modestly so the common case never hits the heal path. Measure the chunk-
   count cost on the existing fixture corpus and REPORT it; do not pick a
   number without the measurement.
3. **Drain the five.** After deploy, the five stuck prod jobs must complete
   or fail-with-a-named-reason. o-prime applies the bounce; you provide the
   before/after query and read it back with me.
4. **Mutation-checked test**: a fixture item whose real token count exceeds
   the cap at the assumed ratio; the test FAILS without the heal (permanent
   failure) and PASSES with it (re-split, embedded, N chunks). State the
   mutation in the PR body.

## Read first
`docs/plans/009-embed-split/impl-guide.dd.json` (risk #2, the two-layer
hygiene, f-001's sub-batching in `embed_items`) · `crates/core/src/tokens.rs` ·
`crates/daemon/src/enrich.rs` · the provider cap constants above · backlog rows
117 and the ac-0007 discharge record.

## Deferred — do not build
A real tokenizer dependency (option b). A change to `TOKEN_BUDGET` or batch
sizing beyond what the heal needs. Anything in `crates/cli`. Conversation
ingest itself.

## Fence
`crates/core/src/tokens.rs`, `crates/daemon/src/enrich.rs`, provider error
classification in `crates/providers/src/{openai,azure_openai}.rs`, their
tests, `crates/testkit` fixtures you add. Nothing else without a stop-and-ask.

## Done-bar
`harness checks` green in your worktree · conventional `fix:` commit(s) via
`harness commit` · PR into main with the mutation stated · the five-job drain
evidence after o-prime's bounce · observation buffer LISTED, never cleared.

## Isolation and the wire
Own worktree `../fs3-embed-cap-heal`, branch `embed-cap-heal`, per-seat
`CARGO_TARGET_DIR`, per-run test DBs (`FS3_TEST_DATABASE_URL` guard is
correct behaviour). NEVER test against prod :7373; read-only dogfood is fine.
**Ack before code**: numbered plan to o-prime, WAIT.
**pij is unstable** (read `government/pij-two-daemons.md`). If `pij send`
fails twice, write to `.harness/temp/agent/embed-cap-heal-ack.md` /
`-report.md` and STOP — o-prime polls those paths. Never `pij adopt`.
Report every friction with `harness observe` AND a message to o-prime.
