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
- **`append_turns` DOES report the shortfall — answered by u2, 2026-08-28.** It
  returns `Appended { accepted: Vec<Element>, already_stored: usize }`, so
  `accepted.len() + already_stored` must equal `prepared.turns.len()` and any
  other result means turns went somewhere unaccounted for. Treat it as a HARD
  ANOMALY, not a logged statistic: under conversation-scoped numbering a
  collision should not arise at all, so `already_stored` above zero on a batch
  `prepare_batch` called entirely new means the ledger and the turns table have
  disagreed about what exists. It is a tripwire. It also cannot tell whether the
  row already under that number holds the same content or different content,
  which is why it must never be allowed to read as routine.
- **Dedupe does not cover prior transcript-imported turns.** They carry no
  ordinal, so a tail of the same content appends beside them. Correct for v1;
  it must be documented at the surface, not discovered by an operator.

## Cross-reader invariants I ruled during wave 1 — verify these at composition

Three readers, written by three seats that never speak to each other. These are
the rules I gave more than one of them, and the composer's job is to confirm
they actually hold rather than trust that each seat read its own ruling the same
way. A reader that diverges here is a defect wearing a dialect's clothes.

1. **An unknown record or event type is a DROP — never an error, never a
   panic.** Given to u1a (claude), u1b (omp/pij) and u1d (metrics-db)
   identically, each with a required test feeding a type the allowlist has never
   heard of and asserting the surrounding records still parse. An ingest must
   not fail because a store grew a bookkeeping row.
2. **A merged group's ordinal is the FIRST member's id** — first line uuid for
   claude, first rowid for metrics-db. The ordinal is u2's dedupe key, so it
   must be identical when a later full re-read regroups the same blocks.
   Last-of-group would change between polls and the dedupe would miss.
3. **A group straddling a poll boundary EMITS AS SEEN and yields two turns.**
   Ruled for u1a and then again for u1d against its own recommendation. The
   rejected alternative — hold back the trailing group — loses the final turn of
   any session that ends on a group, permanently and silently. Consequence to
   verify at composition: the earlier turn keeps only its first blocks FOREVER,
   because u2's ledger deduplicates the later rescan against the same ordinal.
   Nothing is lost, nothing duplicates, one message reads as two turns. Both
   service pages must state the permanence.
4. **Spilled tool output is resolved from the spill file, with a fallback to the
   inline text when the file is gone.** True for claude (u1a) and omp (u1b), for
   different reasons and with different confidence: claude's preview is a
   faithful prefix that states the true size, while omp's is lossy in the middle
   and states nothing. u1b measured both. A search that finds a tool result in
   one harness and not the other is the failure this closes.
5. **Key on what is structurally true, never on what the sample makes look
   true.** The same correction arrived from three seats independently: u1a found
   a packet enumerating record types from one session of two; u1b found a rule
   keyed on a tool's NAME rather than its observable `arguments.path` property,
   and separately proposed reading one text block because this fixture only ever
   has one; u1d found a repo scope keyed on a substring of conversation prose
   rather than the first-class field the store indexes. Three seats, three
   packets, one defect shape — it is a property of how the packets were written,
   and it belongs in the process report.

## u2's five disclosed assumptions — the orchestrator honours all of them

Source: `assets/reports/u2-assumptions.md` on branch `005-convo-u2` (commit
`da20efc`), volunteered after the unit reported done. Ordered by cost of late
discovery. A1 and A2 are the expensive ones and both fail SILENTLY.

**A1 — a session is assumed to resolve to the SAME conversation forever, and
nothing enforced it.** `commit_poll` upserted `ON CONFLICT (harness,
session_id) DO UPDATE SET conversation_id`, so it would silently REBIND. The
ledger has no conversation column — it is keyed `(harness, session_id,
ordinal)` — so ledger rows do NOT move with the rebind: afterwards the ledger
swears every record is stored while the new conversation holds nothing,
`prepare_batch` dedupes the whole batch to zero, and that conversation stays
permanently empty while every call reports success. Triggered by any path that
MINTS a conversation id rather than looking one up.
  - **Ruled**: `commit_poll` REFUSES a conversation change (u2 applying it) —
    a session moving conversations is a bug, not an update.
  - **Mine at the composition root**: resolution is a LOOKUP, never a mint.
    Both halves, because either alone leaves the other's mistake live.

**A2 — a reader's ordinal derivation is a PERSISTED CONTRACT.** If any reader
changes how it derives an ordinal, every stored record looks new and the
conversation silently doubles; `forget_session` does not rescue it, it re-reads
from zero and duplicates anyway. Broadcast to all three reader seats as a fleet
rule while they were still in context, and each states its derivation in its
service page as a frozen fact with the doubling consequence spelled out. The
derivations, now frozen: claude = first line uuid of a merged group; omp =
the record-level `id` (8-hex handle, never the session uuid, never the inner
message id); pij ledger = `seq` rendered as a decimal string; metrics-db =
first rowid of a group, rowid otherwise.

**A3** — numbering follows arrival order: one batch per session file per call,
and never merge or sort after `prepare_batch`.

**A4** — `rescanned` is deliberately NOT consulted and must never become an
`if`. The reason is the one I would have missed: dedupe also covers the crash
window between `append_turns` and `commit_poll`, where `rescanned` is FALSE and
the batch is still a duplicate.

**A5** — `forget_session` is a resume-reset, not an undo. A CLI reset verb must
delete turns too, or say plainly that it will duplicate them.

u2 also recorded what it did NOT assume — nothing about config shape, envelope,
scheduling, poll order, or concurrency beyond the per-conversation serialisation
already in the recipe. That absence is deliberate, which tells me those
questions are mine rather than forgotten.

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
