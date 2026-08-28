# Conversation sources: omp and the pij ledger

Two readers of the [`ConversationSource`] port, shipped together as plan-005
unit u1b: `fs3_providers::conversation_sources::omp::OmpSource` and
`fs3_providers::conversation_sources::pij_ledger::PijLedgerSource`.

They share a store family (line-oriented jsonl) and share their framing
([`tail::read_lines`]), and they share nothing else. The dialects are genuinely
different, and one of them — the cursor — is different on purpose.

---

## THE ORDINAL DERIVATION IS FROZEN

`RawRecord::ordinal` is the key the durable cursor ledger deduplicates on. It is
written to Postgres and it outlives every process here.

| store | ordinal | is |
| --- | --- | --- |
| omp | the **record-level `id` field**, verbatim | an 8-hex handle, e.g. `a932507b` |
| pij ledger | the **`seq` field as a decimal string** | `"118"`, not `118`, not `"seq-118"` |

**Changing either of these silently doubles every stored conversation.** Not
"may cause duplicates" — doubles them, on the next poll, with every call
reporting success. Every already-stored record fails to match its new ordinal,
so it is inserted again as a brand-new turn. There is no clean recovery:
forgetting the session re-reads from zero and duplicates it a third time.

Specifically **do not**: swap omp's record-level `id` for the session uuid or
the inner `message.id`; render pij's `seq` as an integer or pad it; change
first-of-group to last-of-group; "tidy" either into a hash of the line.

If you have found a reason to change a derivation, that is a message to the
plan's PM *before* it ships and a plan *after*. It is also pinned externally —
the committed expectations are generated with `id_key="id"` for omp and
`id_key="seq"` for pij, both stringified, so any other rendering fails
`assert_ordinals_are_a_subsequence` before it can reach a database.

### An ordinal is an OPAQUE IDENTITY. Nothing ever orders one.

Equality is the only meaningful comparison. `<`, `>`, `sort`, `max`,
`ORDER BY`, "resume from the highest ordinal" — all meaningless here, and all
plausible-looking.

The trap is specific and someone will walk into it in good faith: pij's ordinal
is a **decimal string**, so lexicographic order is not numeric order — `"10"`
sorts before `"9"`, and `"118".."167"` happens to sort correctly only because
every value in that range is the same width. The obvious remedy, zero-padding
at derivation, is **RULED AGAINST** (PM3, 2026-08-28): the committed
expectations pin these ordinals as the strings `118` through `167`, because the
driver stringifies `seq` under `id_key="seq"`. Padding would change every one of
them and fail `assert_ordinals_are_a_subsequence` against a byte-pinned fixture.
The remedy would break the thing it was protecting.

Ordering is carried by **arrival order**, not by the ordinal. `read_incremental`
returns records in store order — that is the trait's contract, and the
normaliser numbers turns by position in the slice. Nothing needs to sort an
ordinal, and nothing may start to.

### Why these two derivations are the low-risk pair

Both are **record-derived**: the omp record-level `id`, and the pij `seq`. They
depend on a datum and nothing else.

The other two readers' ordinals are **group-derived** (claude's first-uuid-of-
group, metrics-db's first-rowid-of-group), so they depend on a datum *and* on
the grouping rule — which means widening an allowlist there silently changes
every ordinal and doubles the conversation.

Here it does not. The record allowlist below can change without touching a
single ordinal. What is frozen on this side is the derivation alone; there is no
grouping rule to freeze with it.

---

## omp

### Resolution: the slug is not claude's slug

An omp session is one file at
`<sessions-root>/<slug>/<timestamp>_<uuid>.jsonl`.

The slug **strips the home prefix**. A workspace at
`/Users/agent/substrate/flowspace/flowspace3` lives under
`-substrate-flowspace-flowspace3` — *not* the `-Users-agent-substrate-...` form
recipe §0 predicts. That convention is claude's alone, and a resolver built from
it finds no directory and reports an empty conversation rather than an error.

`resolve` re-globs on **every** call, per the trait.

It returns exactly **one** `SessionFile`. The `<session>/` directory beside the
file holds spilled tool *output* — a payload, not a conversation with roles and
a sequence. That is a different thing from a claude subagent sidecar, which is
its own conversation and is why `SessionKind` exists.

### Line 0 is a 256-byte title slot, rewritten in place

**This is a load-bearing assumption of the whole store.** An omp session file is
not purely append-only: its first line is a fixed-width title record that the
harness overwrites as the session is renamed. A byte-offset cursor survives that
only because the slot's width never changes — measured across all 98 omp session
files on the harvest machine, every one exactly 256 bytes, with
`len(title) + len(pad)` constant.

Two consequences the reader honours:

- **Never treat `size == offset` as "nothing changed".** The file mutates
  without growing.
- **Never cache line 0's title.** A reader that reads it once serves a stale
  title forever; re-read the line if you want the current one.

If omp ever makes that slot variable-width, **every byte cursor for this store
is invalidated at once**. This paragraph is the notice.

### The record allowlist

117 of the 193 committed records are emitted: `message` (114), `compaction` (1),
`custom_message` (2).

| dropped | why |
| --- | --- |
| `title` | carries no `id` at all, so no ordinal is expressible |
| `session`, `model_change`, `thinking_level_change` | not turns |
| `custom` / `tool_execution_start` | the **mirror** of a call the assistant record already carries; emitting both is what makes a naive tool count double |

Emitting fewer records than the store holds is legal: the committed claim is a
**subsequence**, not equality.

**An unknown record type is a drop, never an error and never a panic.** omp
really does emit `ttsr_injection`, `branch_summary` and `service_tier_change`,
none of which appear in the committed window. A reader that errored on a new
type would turn a routine harness upgrade into a dead ingest. All three plan-005
readers share this rule.

### Timestamps: take the record-level field

The record-level `timestamp` is ISO-8601. On a `toolResult`, the **inner**
`message.timestamp` is epoch-milliseconds. `RawRecord::at` is specified as
RFC 3339, so keying on the inner field would emit integers where timestamps
belong — on 72 of the 117 emitted records — and still parse. Pinned by
`a_tool_result_takes_the_record_level_iso_timestamp`.

### `xd://` tool calls are keyed on the PATH, never the tool name

omp encodes its in-process tools as ordinary tool calls whose `arguments.path`
carries an `xd://<tool>` URL. Recipe gotcha 2 calls these "virtual `write`
calls"; the fixture shows that is too narrow. Of the five `xd://` calls in the
window, **four are `write` and one is a `read`** (line 93). A rule keyed on
`name == "write"` misses the fifth and reports a file read that never happened.

The rule is therefore a property of the arguments: any call whose
`arguments.path` starts with `xd://` is an invocation of the tool that path
names, and `tool` becomes the suffix (`pij_send`).

This is not cosmetic. `fs3_daemon::conversations::shape` elides the **write
family by tool name**. Leave an `xd://pij_send` call named `write` and the index
gains a fictional file edit whose "path" is the first line of a pij message.
The remap is what keeps that policy pointed at real writes.

Relatedly: a genuine write's verbatim input is emitted **path first**, because
`shape` stores `first_line(text)` as the elided path.

### Spilled tool output is resolved from the artifact file

omp truncates oversized output itself, spills the raw bytes to
`<session>/<artifact-id>.<something>.log`, and leaves an
`[raw output: artifact://<n>]` marker inline. One of the 72 tool results in the
committed window is such a case.

**The inline body is not a prefix of the spilled file.** Measured: the inline
text abbreviates a git sha to seven characters where the file has forty, omits
the `Author:` line entirely, and carries **two elisions in the middle**
(`[+503]`, `[+338]`) rather than one cut tail. So a 512-byte head of each is
*different text* and the payload policy does not make the question moot;
`total_bytes` derived from the inline `[+N]` markers would be wrong by roughly
3x (895 + 338 against a 3,949-byte source).

So the reader resolves the body from the file. The artifact id is the **numeric
prefix** and the lookup globs on it, because the extension varies in the real
store — `9`, `10`, `11`, `85` are `.bash.log` while `30`, `37`, `41`, `65` are
`.bash-original.log`.

**When the spill file is gone, fall back to the inline body and mark it
`truncated`.** An artifact can be garbage-collected; failing an entire
conversation because one tool result aged out would be absurd. The degradation
is visible rather than silent.

Claude spills too, and u1a resolves it — but for different reasons and with
different confidence: claude's preview *is* a faithful prefix and claude states
its true size. Both readers resolve. That symmetry is a requirement: a search
that finds a tool result in one harness and not the other is a defect wearing a
dialect's clothes.

### Compaction is a first-class record and is never dropped

Recipe gotcha 5 predicts an injected user turn. omp emits a dedicated
`compaction` record instead, carrying `summary`, `shortSummary`, `tokensBefore`
and `firstKeptEntryId`.

It maps to a **system-source** turn and is never dropped — plan acceptance
criterion **ac-0005**. It is also the single most likely thing to be silently
lost, because the reference oracle drops it: `read_omp` handles only
`type == "message"`. Its absence from the expectations' `subset` section is *by
construction*, not a divergence; the `structural` section is what holds the
reader to it.

And the argument that makes it non-negotiable: **the compaction record sits IN
the parent chain.** The injected continuation turn's `parentId` is the
compaction's own `id`, so a reader that allowlists only `message` records drops
the sole marker that context was rebuilt **and breaks the chain across the
seam**.

---

## pij ledger

### Resolution: the seat is the address

`<root>/<seat>/events.ndjson`. There is no uuid to join through, so
`IngestInput::Pij` and `IngestInput::Native { harness: PijLedger }` land in the
same place. One `SessionFile`, no children — a spawned peer gets its own seat
and therefore its own conversation.

### The cursor is a sequence, and that is the point

`SourceCursor::Seq { seq }`. It survives the file being rewritten entirely,
which a byte offset does not — that is the whole reason the variant exists, and
it is why this reader reports `rescanned: false` unconditionally. There is no
rotation for a seq cursor to be confused by.

Each poll re-frames the whole file through `tail::read_lines` and selects on
`seq`. **That is a full-file re-read per poll**, with an O(1)-per-line filter.
Ruled acceptable 2026-08-28: the ledger is small, `seq` is the store's only
monotonic key, and a second cursor mechanism inside one reader is complexity
bought against a number nobody has measured a need for. If that changes, bring
measurements first.

The two readers therefore **refuse each other's cursors**: omp errors on `Seq`,
the ledger errors on `ByteOffset` and `RowId`. Read as zero, either would
silently re-ingest an entire conversation.

### Items come from the dedicated events, never the message blocks

The ledger records a tool twice: once as an assistant `message` whose content
carries `toolCall` blocks, and once as a first-class `tool_call` event. Mapping
both would double every tool in the index — the same failure as omp's
`tool_execution_start` mirror. The dedicated events win, because they also carry
the `toolCallId` that pairs a result to its call.

### Receipts are emitted, and their rendering is PINNED

The ledger is the only store in the fleet that records delivery state, so a
receipt is a real event and is kept. The committed window holds two: seq 122
(`queued`, non-delivered) and seq 127 (`delivered`).

A receipt has no prose of its own — three fields and nothing human — so its body
is **synthesised**:

```
→ <to>: delivery <state> (<messageId>)
```

That text gets embedded and searched like any other turn, so a rendering that
drifts between versions makes two identical receipts read as two different
turns. It is pinned by `the_receipt_rendering_is_pinned` and it matches the
reference oracle's shape, so a human diffing the two sees one string.

---

## Shared by both readers

### Peer attribution is a heuristic over a wire convention

A user turn whose text begins `[pij from` within its first 200 characters is
mapped to `TurnSource::Peer`; anything else falls through to
`TurnSource::Human`. Ten of the eleven user records in the omp window match.

**This is a convention, not a store field.** Neither store records a "who put
this here" axis. A convention nobody enforces will eventually not hold, and when
it does not, the reader degrades to a slightly less precise turn — it never
errors.

### Prose is a fold over N text blocks, not the first one

No record in either committed fixture carries more than one non-empty text
block. That is a fact about a sample, not an invariant of the store, so both
readers concatenate every text block in order. A reader that took the first
would silently discard the second the day either store emits one, with no error
and no failing test. `thinking` blocks are excluded: model scratch space, not
prose the store attributes to the turn.

The single-block case gains no separator, no padding and no reordering — the
oracle hashes the store's verbatim text.

### What the oracle checks do and do not prove

`assert_oracle_prose_appears` is strong for omp (15 prose turns matched
verbatim: 4 assistant, 1 human, 10 pij_in; `pij_out` and `tool_call` are held
only to their counts, since the oracle renders those through its own Python
helpers and a Rust reader imitating that would prove imitation, not agreement).

**It is nearly empty for the pij ledger, and that is measured, not assumed.**
The oracle yields 3 turns from 50 records — `read_pij_ledger` emits only
`receipt` and `message` events, keeps only role `user`/`assistant`, and requires
a text block — and exactly **one** of those three is a prose kind. This window is
tool-heavy: 13 of its 14 text blocks sit on role `toolResult`.

So a green `every_pij_oracle_prose_turn_appears` is not evidence the reader is
right. **The structural claim is this store's done-bar.** Read the
`grade_of_proof` field in `expectations.json` before you trust or blame either.

### The test fixtures grow with real bytes

The contract suite writes on purpose, so it runs over a scratch copy and the
committed fixtures stay byte-identical. The scratch copy is seeded with a real
**prefix** of the committed file; `grow()` appends the real **remaining lines**;
one final real line is held back to be torn in half for the incomplete-record
case. Nothing is synthesised — a fixture that grew by something its store would
never write would prove the suite, not the reader.

Counts are computed **from the bytes**, never hand-written, so regenerating a
fixture cannot silently invalidate them. The omp prefix ends just past the
compaction seam, putting the seam on the read boundary rather than safely inside
one side of it.

---

## Snap-in recipe

Everything the composition root needs. **Written by u1b, wired by the composer.**

### 1. The module lines

Already present in `crates/providers/src/conversation_sources/mod.rs`, kept
alphabetical:

```rust
pub mod omp;
pub mod pij_ledger;
```

### 2. Construction

Both readers take their root by injection — nothing is discovered from the
environment, so a test can point at a scratch directory and a slug never depends
on who is running it.

```rust
use fs3_providers::conversation_sources::{omp::OmpSource, pij_ledger::PijLedgerSource};

// Conventional layout beneath a home directory:
//   omp -> <home>/.omp/agent/sessions
//   pij -> <home>/.pij
let omp = OmpSource::from_home(&home);
let pij = PijLedgerSource::from_home(&home);

// Or explicitly, when the roots are configured separately:
let omp = OmpSource::new(&omp_sessions_root, &home);   // home is needed for the slug
let pij = PijLedgerSource::new(&pij_root);
```

`OmpSource` needs `home` as well as its sessions root **because the slug is
derived by stripping home**. Passing the wrong home yields a directory that does
not exist, which surfaces as a resolve error naming the slug rule.

### 3. Registration

Both implement `fs3_core::ConversationSource`, which is object-safe and
**blocking**. Route by `Harness`:

```rust
let source: &dyn ConversationSource = match harness {
    Harness::Omp => &omp,
    Harness::PijLedger => &pij,
    // u1a, u1d
};
```

Hand every call to `spawn_blocking`, exactly as the local ONNX embedder is
handled — every method is file IO.

### 4. Config shape

Two paths, both optional, both defaulting to the conventional layout under the
invoking user's home:

| key | default | used for |
| --- | --- | --- |
| `conversations.omp_sessions_root` | `<home>/.omp/agent/sessions` | omp session files |
| `conversations.pij_root` | `<home>/.pij` | pij seat ledgers |
| `conversations.home` | the process's home | the omp slug derivation |

### 5. What the composer must know

- **`resolve` must be called on every poll**, not cached. The trait says so and
  the omp reader honours it.
- **Persist the cursor per conversation**, and never hand one store's cursor to
  another — both readers refuse, loudly, but the refusal is an error the
  orchestrator has to not swallow.
- **`rescanned: true` means the records are the WHOLE file**, not a delta. Dedupe
  on `RawRecord::ordinal` before appending. The pij reader never sets it; the omp
  reader sets it on inode change or truncation.
- **`OmpSource` errors on `IngestInput::Pij`.** The seat→session join is the
  orchestrator's job; resolve the native session id first.

[`ConversationSource`]: ../../crates/core/src/conversation_source.rs
[`tail::read_lines`]: ../../crates/providers/src/conversation_sources/tail.rs
