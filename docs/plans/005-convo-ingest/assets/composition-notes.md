# Composition notes — phase 3 working set (PM3)

Facts I measured or ruled that unit u4 (the orchestrator, CLI verb and
extension) must honour. Written as I learn them rather than at composition,
because two PM seats have already died mid-flight on this plan and the
expensive loss both times was undocumented in-flight knowledge.

Not a substitute for the units' snap-in recipes — those are in each unit's
`docs/services/*.md`. This is what is true ACROSS them.

## Measured 2026-08-28: `pij sessions` is richer than recipe §2 describes

The recipe says the join is `pij sessions` giving pij-id · harness ·
harness-session-uuid, and that **the uuid SHAPE routes** the store (v4 →
claude/copilot, v7 `01a0…` → omp). Measured against the live registry (908
rows), three corrections:

1. **Use `pij sessions --json`, never the table.** It emits objects with
   `pijId`, `harness`, `harnessSessionId`, `boundModel`, `parentId`,
   `spawnedBy`, `gitCommonDir`, `prime`. Parsing the human table would make the
   resolver hostage to column widths.

2. **Route on the `harness` field; the uuid shape is a CHECK, not the router.**
   The recipe's shape rule is not sufficient: `harness` counts are pi 800,
   claude 65, copilot 41, codex 2 — and claude and copilot are BOTH v4 uuids.
   Shape alone cannot separate the store that holds a claude session from the
   one that holds a copilot session, and copilot exists only in metrics-db.
   The explicit field disambiguates them; keep the shape rule as a consistency
   assertion (a v7 `01a0…` uuid claiming `harness: claude` means the registry
   is lying and the resolver should say so rather than guess).

3. **pij spells omp as `pi`.** Every omp seat registers as `harness: "pi"`.
   Phase 1 already anticipated this — `Harness::from_str` accepts `"omp" | "pi"`
   — so this is confirmation, not a defect. It is written down because a
   resolver author who trusts `Harness::as_str()` round-tripping (`"omp"`)
   against the registry's spelling (`"pi"`) gets an empty join and no error.

4. **`transcriptPath` is empty for pi rows.** The table advertises the column
   and the JSON omits it for exactly the harness this plan's first-light target
   uses. omp file resolution therefore goes the recipe's way — slug directory
   plus `*_<uuid>.jsonl` glob — and the resolver must not take a transcript
   path as available.

5. **`gitCommonDir` is present and is the MAIN CLONE for worktree-resident
   seats.** Every wave-1 seat, all of them working in their own worktrees,
   reports `/Users/jordanknight/substrate/flowspace/flowspace3/.git`. It is
   therefore usable as a folder DEFAULT (strip `/.git`) but it is not evidence
   of where a seat's shell actually was. This is the same root cause as the
   `pij whoami` finding in DL-005: pij registers a seat against its clone, not
   its worktree.

First-light target (tk-c305) confirmed available: this PM seat is
`pij-pale-silkworm` → `01a045f4-edc2-7000-8dc7-47d6d5677147`, harness `pi`.

## Ruled 2026-08-28: what u2's numbering fix obliges u4 to do

Unit u2 self-reported, after reporting done, that turn numbering was scoped per
`(harness, session_id)` while `turn_no` is the primary key of the
CONVERSATION. I ruled the authoritative fix (derive the high-water mark from
`MAX(turn_no)` on the turns table) over an orchestrator-side rule. Consequences
that land on me:

- **`ledger_view` now takes `conversation_id`**, so `upsert_conversation` must
  run before it — the recipe's original ordering note pinned only
  `upsert_conversation` before `commit_poll`, which is no longer sufficient.
- **Serialise ingest per CONVERSATION, not per session.** Once numbering is
  per conversation, two concurrent polls of two DIFFERENT sessions on the SAME
  conversation both read the same `MAX(turn_no)` and collide — the same silent
  drop by a different door. One Claude conversation is a main file plus N
  sidecars, so multi-session-per-conversation is the normal case here, not an
  exotic one.
- **The `LedgerView` is a per-poll snapshot**: taken after the read, with that
  batch's ordinals, used for that batch only. Never cached across polls.
- **`deduped` goes in the CLI envelope** (tk-c303 lists turns_new, turns_total,
  conversations, next_action — `deduped` is an addition, and binding). "read
  412, appended 0, deduped 412" is the only line that distinguishes a handled
  rotation from an idle poll. Without it, a silently duplicated or silently
  dropped conversation looks exactly like a quiet one.
- **Compare appended count against `prepared.turns.len()`** if the store
  surfaces it, and treat a shortfall as an anomaly rather than a success — a
  backstop that does not depend on my having reasoned correctly about
  numbering. Question outstanding with u2 on whether `append_turns` reports it.
- **Dedupe does not cover prior transcript-imported turns.** They carry no
  ordinal, so a tail of the same content appends beside them. Correct for v1;
  it must be documented at the surface, not discovered by an operator.

## Recipe step 2 is mine (prime ruling, condition 3)

In `crates/daemon/src/conversations.rs`: delete the private `shape`,
`WRITE_FAMILY`, `is_write_family`, `starts_with_family`, `first_line`,
`floor_char_boundary` and the `OUTPUT_HEAD_BYTES` definition; `pub use
fs3_core::OUTPUT_HEAD_BYTES` and delegate `shape()` to `fs3_core::shape_turn`.
**The eight tests in that module stay green and UNMODIFIED** — they are the
regression oracle for the move. If one needs an edit, the behaviour changed and
it is a stop-and-ask to prime, not a composition fix.

Until I make that edit, core and daemon both define `OUTPUT_HEAD_BYTES`. That
is an expected interim, not a defect.

## Standing composition rule

Composition is wiring, not building. Unit-internal rework needed at this seam
is a phase-1 contract defect: stop, record (`harness observe` + the skill's
EXPERIENCES.md), get it ruled by prime. The u2 numbering fix was NOT that — it
was inside u2's own surface, found before composition, and fixed by the live
seat that owned it.
