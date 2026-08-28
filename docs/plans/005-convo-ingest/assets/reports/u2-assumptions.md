# u2 assumptions disclosure — what this unit takes on faith about u4

Seat pij-appalling-slug, unit u2 (cursor state + normalizer), branch
005-convo-u2.

Every item below is something my code assumes and does not enforce. Each one
says what breaks if the assumption is false, whether it breaks LOUDLY or
SILENTLY, and the cheapest guard. Ordered by how expensive it would be to
discover late.

Silent failures first, because those are the ones that cost a plan.

---

## A1 — A session must always resolve to the SAME conversation. NOW ENFORCED.

**Status: closed by a guard.** Ruled by PM3 on 2026-08-28 and implemented;
this entry is kept because the reasoning is still the reason the guard exists.

**Was assumed:** the orchestrator derives `conversation_id` deterministically
from `(harness, session_id)`, so a given session belongs to one conversation
forever.

**What the code used to do:** `commit_poll` upserted
`ON CONFLICT (harness, session_id) DO UPDATE SET conversation_id =
EXCLUDED.conversation_id`, silently REBINDING the cursor to whatever
conversation it was handed.

**Why that was the worst shape available:** `ingest_ledger` has no
`conversation_id` column — it is keyed `(harness, session_id, ordinal)` — so
ledger rows do NOT move with a rebind. Afterwards the ledger still insists
every record was stored while the newly named conversation holds no turns,
`prepare_batch` dedupes the entire batch to zero, and that conversation stays
permanently empty. The CLI says fine, the ledger says stored, the conversation
says nothing. A failure that reports success, invisible from every angle.

**What the code does now:** `commit_poll` reads the stored `conversation_id`
`FOR UPDATE` inside its transaction, compares as `uuid` rather than as text so
a difference in spelling is not mistaken for a difference in identity, and
returns [`StoreError::SessionRebound`] naming both conversations. Nothing is
written — not the cursor, not the ledger. Proved by
`a_session_may_not_be_rebound_to_another_conversation`, which asserts the
refusal, the untouched cursor, the untouched ledger and the untouched second
conversation.

**Still the composer's, and it is the real fix:** resolution becomes a LOOKUP,
not a mint. The guard survives that being got wrong; it does not replace it.

## A2 — A reader's ordinal derivation is a PERSISTED contract. SILENT.

**Assumed:** for a given record, a reader emits the same `RawRecord::ordinal`
today and in six months.

**What breaks:** ordinals are the dedupe key and they are stored. If u1a/u1b/u1d
ever change how an ordinal is derived — message uuid to composite key, rowid to
something more stable, any normalisation of case or prefix — every previously
stored record looks NEW. The next poll appends the whole conversation again
above the existing turns. Nothing errors; the conversation simply doubles.

**Not caught by anything.** Not by my tests, not by the contract suite, not by
the primary key.

**Cheapest guard:** treat a reader's ordinal derivation as a schema-shaped
decision. If one changes, the migration is `forget_session` for every session of
that harness, which resets the ledger and re-reads from zero — the turns already
stored keep their numbers and the rescan dedupes against nothing, so the
conversation grows a duplicate anyway. **Honest answer: there is no clean
recovery in v1.** The cheap thing is a rule — ordinal derivation does not change
without a plan — and I would rather it be written down than assumed.

## A3 — Numbering follows the order records arrive in. SILENT if violated.

**Assumed:** one `prepare_batch` call gets one session's records in store
order, exactly as the reader emitted them.

**What breaks:** `prepare_batch` assigns `turn_no` by position in the slice. If
u4 ever merges, sorts, or interleaves batches from several `SessionFile`s before
calling it, `turn_no` order stops matching `at` order — and `turn_no` is the
navigation axis, so a windowed read (-10/+20 around a hit) returns turns that
are adjacent by number but scattered in time. It reads as a subtly incoherent
conversation rather than as an error.

**Cheapest guard:** one batch per session file per call. If merging is ever
wanted, sort before `prepare_batch` and never after.

## A4 — `rescanned` is deliberately NOT consulted by my code. LOUD if misread.

**Assumed:** the orchestrator runs the SAME path whether or not
`ReadBatch::rescanned` is set.

**Why:** dedupe is unconditional on purpose. It also covers the crash window
between `append_turns` and `commit_poll`, where a delta was stored but the
cursor never advanced — that batch comes back with `rescanned = false` and is
still a duplicate. Branching on `rescanned` would skip dedupe exactly where it
is still needed.

**What to avoid:** do not write `if batch.rescanned { dedupe } else { append }`.
The flag is diagnostic and belongs in the envelope beside `prepared.deduped`;
it is not a control signal for this unit.

## A5 — `forget_session` forgets how to RESUME, not what was stored. LOUD.

**Assumed:** nobody expects it to delete turns.

It removes the cursor and cascades the ledger. The turns stay in the
conversation. A re-ingest afterwards is a clean first read, which — because the
numbering now comes from the conversation — appends the whole session AGAIN
above the existing turns rather than colliding with them.

**Consequence for the CLI:** if a "reset ingest" verb is ever exposed, it must
either delete the conversation's turns too or say plainly that it will duplicate
them. `forget_session` alone is a resume-reset, not an undo.

---

## Two smaller notes

- **`ledger_view` with an empty ordinal slice is safe** — `= ANY('{}')` matches
  nothing, and you still get the conversation's high-water mark. An empty poll
  needs no special case.
- **`commit_poll` can fail on the foreign key** if the conversation was deleted
  between resolve and commit. That is a benign race (a user removed the
  conversation mid-ingest), not corruption. Treat it as a skip, not a crash.

---

## What I did NOT assume

For completeness, so the absence is deliberate rather than forgotten: I assume
nothing about u4's config shape, its CLI envelope, its scheduling, its
concurrency model beyond the per-conversation serialisation already in the
recipe, or the order in which sessions are polled. The unit is order-independent
across sessions and holds no state between calls.
