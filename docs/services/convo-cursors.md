# convo-cursors — durable ingest cursors, the ordinal ledger, and the turn normalizer

Plan 005 unit u2. Two SRP services that make a **second** ingest of a
conversation cost only the turns that are new — and make a **rescan**, when the
session file rotated out from under the reader, cost nothing at all.

Neither service reads a native store and neither wires itself in. They are
consumed by the ingest orchestrator at the composition root (unit u4).

## What it is

**Durable cursor persistence** (`fs3_store::ingest_cursors`, migration 0014).
Two tables:

- `ingest_cursors` — one row per `(harness, session_id)`: the serialised
  `SourceCursor`, the conversation it belongs to, and when it was last read.
- `ingest_ledger` — one row per record ever stored: the store's natural
  `ordinal` and the `turn_no` it went in under.

**The pure normalizer** (`fs3_core::conversation_normalize`). Turns the
`RawRecord`s a reader produced into the `Turn`s the intake already accepts:
assigns the number, applies the payload policy, drops what v1 does not store.
No IO, no clock, no store — every interesting decision is testable in memory.

## Key decisions and why

**The cursor alone is not enough.** A reader whose file rotated or was
truncated cannot resume: it restarts from zero and reports
`ReadBatch::rescanned`, and what comes back is the WHOLE conversation rather
than a delta. Appended blindly that stores the conversation twice — silently,
because a duplicated conversation looks exactly like a busy one. The ledger is
what makes a rescan a no-op.

**The ledger MAPS, it does not merely remember.** Dedupe needs only "have I
seen this ordinal", but `turn_no` is the navigation axis (req-0026) and half
the primary key `append_turns` is idempotent on. Storing `ordinal -> turn_no`
means a rescan RECOVERS a record's existing position instead of minting a
second number for the same content, at identical row count. (Improvement on
the original brief, ruled by the plan PM at ack, N4.)

**The high-water mark comes from the stored TURNS, not from the ledger**
(`ledger_view`, PM ruling 2026-08-28). `turn_no` is the conversation's primary
key, so the conversation owns the number; deriving it from a per-session
ledger was an INFERENCE about a one-session-per-conversation mapping, and
inferences about mappings break when a plan grows a feature. Two ordinary
cases break it: a conversation previously filled by `fs3_cli::conversation`
transcript import has turns but no ledger, and two sessions can share one
conversation. In both, an inferred mark restarts at 1, `append_turns` drops
the collisions idempotently, and `commit_poll` records them as stored — turns
that vanish while every call reports success. The `seen` set stays scoped to
the SESSION, because an ordinal means nothing outside the session that minted
it; only the numbering axis is shared.

**Cursor and ledger commit in ONE transaction** (`commit_poll`). A cursor that
advanced without its ledger rows leaves the next rescan unable to recognise
turns that ARE stored, so it appends the conversation again under fresh
`turn_no`s that the `(conversation_id, turn_no)` primary key cannot catch.
Ledger rows without a cursor are merely a re-read; the other way round is a
duplicated conversation.

**A session may not move conversations** (`commit_poll`, PM ruling
2026-08-28). Rebinding a session's `conversation_id` used to be a silent
`DO UPDATE`; it is now `StoreError::SessionRebound` and nothing is written.
The ledger is keyed `(harness, session_id, ordinal)` and carries no
conversation, so its rows would not move with a rebind — the ledger would
insist every record was stored while the newly named conversation held
nothing, `prepare_batch` would dedupe the whole batch to zero, and it would
stay permanently empty with every call reporting success. The real fix is that
resolution is a LOOKUP rather than a mint, which is the composition root's;
this guard is what survives that being got wrong.

**No `CursorStore` trait**, deliberately deviating from the impl-guide's u2
interface column (PM ruling 2026-08-28). `grep -rn "^pub trait\|^trait "
crates/store/src/` returns nothing — the crate has no trait convention to join
— and a trait whose only second implementation is its own test fake does not
clear the workshop 001 rule-3 bar that the `ConversationSource` rustdoc cites
to justify itself. The decisions worth testing without Postgres are pure and
live in core; what is left in the store is SQL, and SQL is proven against
Postgres. **Named trigger for revisiting:** if the u4 orchestrator needs a
PG-free seam, the trait is a small additive change to a unit that already
exists.

**The payload policy lives in core, once** (prime ruling 2026-08-28).
`OUTPUT_HEAD_BYTES` and `shape_turn` moved out of `fs3_daemon::conversations`,
where they were private. The importer must apply the same rules the intake
enforces, and a second implementation of a truncation rule is a rule that
drifts — plan 005's own risk r3. Intake keeps its backstop by delegating.

**`parent_ordinal` is dropped**, and that is a decision. `Turn` has no parent
field because v1 stores sequence, not the reply chain. A test names the drop so
it does not read as an oversight.

**The cursor crosses as TEXT cast to `jsonb`**, exactly as `turns.items` does.
`SourceCursor` is a tagged union of three shapes; a column per variant would be
five nullable columns plus a check constraint to say which three are meaningful.
The Rust type already refuses the invalid combinations, so a fourth store is a
code change and never a migration.

## Gotchas discovered

- **`u64` through JSON is where precision quietly dies.** A device/inode pair
  is a `u64` and real ones are large. Postgres `jsonb` numerics are arbitrary
  precision so the round trip is exact, but this is asserted rather than
  assumed (`a_cursor_survives_the_largest_values_its_types_allow`, at
  `u64::MAX`) — an offset that comes back one byte wrong resumes mid-record
  forever.
- **A repeated ordinal inside ONE batch is not caught by the database.**
  Storing it twice yields two different `turn_no`s, so the
  `(conversation_id, turn_no)` key sees two legitimate rows. `prepare_batch`
  dedupes within the batch as well as against the ledger.
- **A retried poll must not renumber.** `ON CONFLICT DO NOTHING` on the ledger,
  never `DO UPDATE`: an ordinal's number is assigned once, and the number
  already stored is the right answer.
- **Ledger lookups are per batch, not per session.** A long-running seat is
  thousands of rows; a poll only asks about the handful it just read.
- **`harness boot --json` reports `degraded` / `service "db" is not running`
  in a fresh coder worktree** even when Postgres is up as `flowspace3-db` on
  5433. Boot is looking for a compose service in a worktree that never ran
  compose. Do NOT `docker compose up` to "fix" it — `container_name` is pinned,
  so a second up can take the whole fleet's database down.
- **Numbering fixes COLLISION, not DEDUPE across ingest paths.** Turns that
  arrived by transcript import carry no ordinal — there is no ledger row and
  nothing to match them on — so tailing the same session afterwards appends
  the same content beside them rather than recognising it. Deliberate v1
  behaviour, named here so it does not read as an oversight: the alternative
  is matching turns by content hash across two paths that disagree about
  payload shaping, which is a plan of its own. Import a transcript or tail the
  session, not both.

## How to verify it works

```bash
export FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5433/fs3_convo_u2

# The pure decisions — no Postgres needed.
cargo test -p fs3-core conversation_normalize        # 16 tests

# The durable half, against a real database.
cargo test -p fs3-store --test pg_ingest_cursors     # 17 tests

# The regression oracle for the payload-policy move: unmodified, must be green.
cargo test -p fs3-daemon --lib conversations         # 8 tests
```

The four load-bearing tests, all mutation-checked:

- `a_rescan_after_rotation_appends_nothing_through_the_store` — delete the
  `seen` lookup in `prepare_batch` and this fails on the turn count, together
  with `a_rescan_that_grew_stores_only_the_delta` and the two in-memory
  equivalents.
- `two_sessions_on_one_conversation_number_above_each_other` — point
  `ledger_view`'s high-water query back at `ingest_ledger` and this fails on
  the numbers, together with
  `tailing_a_previously_imported_conversation_appends_above_the_import`.
- `an_oversized_tool_result_is_cut_to_its_head_and_says_so` — delete the
  `head.truncate(...)` in `shape_turn` and five truncation tests fail,
  including the multi-byte boundary cases.
- `a_session_may_not_be_rebound_to_another_conversation` — turn the refusal
  back into `DO UPDATE SET conversation_id` and this fails on the error, then
  on the ledger.

## THE SNAP-IN RECIPE

Everything below is for the **composer** to paste. This unit wires nothing in
itself.

### 1. Module lines

Already present on this unit's branch:

```rust
// crates/core/src/lib.rs
pub mod conversation_normalize;
pub use conversation_normalize::{
    OUTPUT_HEAD_BYTES, PreparedBatch, normalize_record, prepare_batch, shape_turn,
};

// crates/store/src/lib.rs
pub mod ingest_cursors;
```

**Optional, composer's call:** every other store module is also re-exported at
the crate root. If you want `fs3_store::load_cursor` rather than
`fs3_store::ingest_cursors::load_cursor`, add:

```rust
// crates/store/src/lib.rs
pub use ingest_cursors::{
    Forgotten, LedgerView, commit_poll, forget_session, ledger_view, load_cursor, sessions_for,
};
```

It was left out because this unit's fence permitted exactly one line in that
file.

### 2. The daemon delegates the payload policy (prime ruling, condition 3 — the composer makes this edit, not u2)

In `crates/daemon/src/conversations.rs`, delete the private `shape`,
`WRITE_FAMILY`, `is_write_family`, `starts_with_family`, `first_line`,
`floor_char_boundary` and the `OUTPUT_HEAD_BYTES` definition, and replace them
with:

```rust
pub use fs3_core::OUTPUT_HEAD_BYTES;

/// Apply workshop 005's payload rulings to one turn.
///
/// Delegates to [`fs3_core::shape_turn`] so the policy has ONE implementation:
/// the importer applies it too, and two copies of a truncation rule drift.
/// Intake still ENFORCES rather than trusts — that backstop is unchanged.
fn shape(turn: Turn) -> Turn {
    fs3_core::shape_turn(turn)
}
```

The eight tests in that module's `mod tests` must stay **green and unmodified**
— they call only `shape` and `OUTPUT_HEAD_BYTES`, both of which survive this
edit. They are the regression oracle for the move; if one needs an edit, the
behaviour changed and it is a stop-and-ask to prime.

### 3. The ingest pipeline at the composition root

Per session, per poll. The store is blocking, the reader is blocking — hand the
reader to `spawn_blocking` exactly as the local ONNX embedder is handled.

```rust
// 0. WHICH conversation is this session? A LOOKUP, never a mint.
//    `Some` means the mapping is already decided and there is nothing to
//    choose. `None` means first ingest — mint exactly one, here, at the
//    composition root. This unit can answer which conversation a session
//    belongs to; it deliberately cannot invent one.
let conversation_id = match fs3_store::ingest_cursors::conversation_for(
    &pool, harness, &session_id,
).await? {
    Some(existing) => existing,
    None => mint_conversation(/* caller-supplied guid, or a fresh one */),
};

// 1. Where did we stop? `None` means "from the beginning" — a first ingest and
//    a forgotten one take the identical path.
let cursor = fs3_store::ingest_cursors::load_cursor(&pool, harness, &session_id).await?;

// 2. Read. Blocking IO, so off the async thread.
let batch = tokio::task::spawn_blocking(move || source.read_incremental(&file, cursor.as_ref()))
    .await
    .map_err(/* join error */)??;

// 3. What does the ledger already know about exactly these records?
let ordinals: Vec<&str> = batch.records.iter().map(|r| r.ordinal.as_str()).collect();
let view = fs3_store::ingest_cursors::ledger_view(
    &pool, harness, &session_id, &conversation_id, &ordinals,
).await?;

// 4. Decide — pure. Dedupes the rescan, numbers the rest, shapes the payloads.
let prepared = fs3_core::prepare_batch(&batch.records, &view.seen, view.next_turn_no);

// 5. Append. Idempotent on (conversation_id, turn_no); `enrich` is the config's
//    size gate, injected the way `append_turns` already expects.
let appended = fs3_store::append_turns(&pool, &conversation_id, &prepared.turns, enrich).await?;

// 6. Record the poll: ledger rows AND the cursor, atomically. Do this even when
//    `prepared.turns` is empty — the reader still moved over bytes.
fs3_store::ingest_cursors::commit_poll(
    &pool, harness, &session_id, &conversation_id, &batch.cursor, &prepared.ledger,
).await?;
```

**Ordering — this is a mis-wiring trap.** `upsert_conversation` must run before
**step 3**, not merely before step 6. `ledger_view` reads the conversation's
own high-water mark, so the conversation must exist before its number can be
read; and `ingest_cursors.conversation_id` is a real foreign key, so the
cursor cannot outlive the conversation it resumes.

**Serialise per CONVERSATION, not per session.** `ledger_view` and
`commit_poll` are separate transactions, so a snapshot can go stale between
them. Two concurrent polls of two DIFFERENT sessions on the SAME conversation
would both read the same high-water mark and collide — the same silent drop,
arriving by another door. This unit does not serialise for you; the
orchestrator must.

**Do NOT cache a `LedgerView` across polls.** It is a per-poll snapshot taken
after the read, with that batch's ordinals. Reusing one numbers the second
batch on top of the first. Named because it is the optimisation a future
reader adds in good faith.

**Handle `SessionRebound`.** `commit_poll` refuses when the session is already
tailing a different conversation, and writes nothing. It means resolution
handed out two different conversations for one session — a bug upstream of
this call, not a retryable condition. Surface it; do not fall back to minting
another conversation, which is the thing it is protecting against.

**Compare the counts.** `append_turns` returns `Appended { accepted, already_stored }`,
so a shortfall is visible: `accepted.len() + already_stored` should equal
`prepared.turns.len()`. Anything else means turns were dropped on conflict,
which is the failure class conversation-scoped numbering exists to prevent —
treat it as an anomaly, not a success.

**Reporting.** `prepared.deduped` is the count a rescan suppressed. It belongs
in the CLI envelope — "read 412, appended 0, deduped 412" is the line that
tells an operator a rotation was handled rather than a poll being mysteriously
idle.

### 4. Config

None. `OUTPUT_HEAD_BYTES` is a constant rather than a knob, per workshop 005's
own sketch: it becomes a knob the day someone has a number that beats it.

### 5. Cleanup verbs, if the CLI wants them

- `fs3_store::ingest_cursors::forget_session(&pool, harness, &session_id)` —
  forget one session; the ledger cascades with the cursor, so a re-ingest is a
  clean first read.
- `fs3_store::ingest_cursors::sessions_for(&pool, &conversation_id)` — every
  session still tailed for a conversation. One Claude conversation is a main
  file plus N sidecars, each cursored separately (recipe gotcha 6).

Removing a conversation needs no new code: `ingest_cursors.conversation_id` is
`ON DELETE CASCADE`, so `delete_conversation` already forgets how to resume it.

## Code pointers

| What | Where |
| --- | --- |
| Cursor + ledger persistence | `crates/store/src/ingest_cursors.rs` |
| Schema | `crates/store/migrations/0014_ingest_cursors.sql` |
| Normalizer + payload policy | `crates/core/src/conversation_normalize.rs` |
| Postgres proof | `crates/store/tests/pg_ingest_cursors.rs` |
| The frozen contract this consumes | `crates/core/src/conversation_source.rs` |
| The intake it feeds | `crates/daemon/src/conversations.rs` |
