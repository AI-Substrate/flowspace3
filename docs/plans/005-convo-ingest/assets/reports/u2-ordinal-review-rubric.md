# Ordinal derivation review — the rubric, published BEFORE any reader lands

Reviewer: u2 (pij-appalling-slug), the seat that wrote the consumer. Assigned
by PM3 2026-08-28. Read-only; reviews are performed against the plan branch
when PM3 says a reader is merged, never against a live worktree.

This file is deliberately written before I have seen a single reader
derivation, so the criteria can be challenged on their own merits rather than
after they have already produced verdicts.

---

## What the consumer ACTUALLY requires

Stated from the code that consumes ordinals — `fs3_core::prepare_batch` and
`fs3_store::ingest_cursors` — rather than from what sounds right.

### R1. Byte-for-byte stability across re-reads. HARD REQUIREMENT.

Dedupe is exact string membership: `seen: BTreeSet<String>` and
`seen.contains(record.ordinal.as_str())`. Nothing is trimmed, normalised,
case-folded or parsed. The same record read on two different days must produce
an identical string or the ledger cannot recognise it.

Fails if a derivation includes anything volatile: a byte offset, a line number,
an index within a batch, a file path, a timestamp, or a hash of content that
gets shaped differently by a later payload rule.

**Positional derivations are the specific trap.** After a truncation the reader
restarts from zero and positions shift; an ordinal derived from position is
then different for the same record, every record looks new, and the whole
conversation duplicates. Silently.

### R2. Uniqueness within the session, for the LIFETIME of the session. HARD.

Two distinct records sharing an ordinal is a false MATCH: the second is treated
as already stored and is dropped. A turn disappears and nothing reports it.

This must hold across the whole session history, not merely within one batch —
the ledger is durable, so a collision with a record ingested six weeks ago has
the same effect as one in the same poll.

**No empty ordinals.** An empty string is a legal `String` and my code will
accept it, so two records that both failed to derive an id would collide on
`""` and the second would vanish. A reader that cannot derive an ordinal must
stop and ask, not fall back.

### R3. Arrival order — NOT ordinal order. This corrects the brief.

PM3's third criterion was "monotonic in store order". I want to be precise
rather than agreeable: **my code does not require ordinals to be monotonic, or
ordered, or comparable at all.** `prepare_batch` assigns `turn_no` by POSITION
IN THE SLICE. What it requires is that records arrive in store order, which is
the reader's contract already (`ReadBatch::records` — "in store order").

Ordinal monotonicity matters to the reader's own cursor, not to the ledger.

**But there is a latent trap worth naming now.** The pij ledger's ordinal is
`seq` rendered as a decimal string, so lexicographic order is not numeric order
— `"10" < "9"`. Nothing in my unit orders ordinals today (`BTreeSet` is used
for membership only, and the ledger's own ordering query is on `turn_no`). The
day someone adds "resume from `MAX(ordinal)`" or an `ORDER BY ordinal` that
means something, that reader silently regresses. Either zero-pad at derivation
or record "ordinals are opaque, never ordered" as a rule. I mention it because
it costs nothing now and is expensive to discover later.

### R4. Group-derived ordinals carry an extra dependency. THE SHARP ONE.

Two of the four frozen derivations are group-derived — claude's *first line
uuid of a merged group*, metrics-db's *first rowid of a group* — and two are
record-derived — omp's record id, pij's `seq`.

A record-derived ordinal depends on one datum. A group-derived ordinal depends
on a datum AND ON THE GROUPING RULE. If the grouping rule ever changes — the
record-type allowlist widens, a new content-block kind joins a merge, a
previously-skipped line starts being included — the group's first element
changes, the ordinal changes, and every stored record of every affected
conversation looks new. The conversation doubles, silently, on the next poll.

So the two group-derived readers carry strictly more risk than the two
record-derived ones, and their grouping rule is as frozen as the derivation
itself. That is not a defect to fix; it is a fact to write into their service
pages, because A2 (ordinal derivation is a persisted contract) applies to the
grouping rule too, and nobody has said so yet.

### R5. What is explicitly NOT required — so no reader over-engineers

- **Cross-session uniqueness is NOT needed.** `seen` is scoped to
  `(harness, session_id)`. Two claude sessions may both mint the same uuid
  string with no consequence. `two_sessions_keep_separate_ledgers` proves it.
- **Ordinals need not be short, sortable, numeric, or human-readable.** They
  are opaque keys. `TEXT` in the ledger, `String` in the domain.
- **Ordinals need not encode position or time.** `turn_no` carries sequence and
  `at` carries time; an ordinal that tries to carry either is coupling itself
  to something it does not need.

---

## The two failure directions, named so verdicts can be specific

| direction | mechanism | symptom |
| --- | --- | --- |
| **False match** — two records, one ordinal | dedupe treats the second as stored | a real turn is DROPPED; conversation is missing content; no error anywhere |
| **False miss** — one record, two ordinals over time | dedupe recognises nothing | the conversation DUPLICATES on the next rescan; looks like a busy session |

Every finding I report will name which of these it causes. A finding that
cannot be traced to one of them is style, and I will label it as such rather
than smuggle it in as a defect.

---

## What I will NOT be able to determine from reader code alone

Declared in advance, because a clean bill I cannot lean on is worse than a
hedge.

1. **Whether the underlying store guarantees its own id is stable.** The
   specific worry is metrics-db: **sqlite `rowid` is not stable across `VACUUM`
   unless the column is an `INTEGER PRIMARY KEY` alias.** If git-ai ever vacuums
   its metrics database, rowids can be renumbered — which breaks R1 for every
   stored ordinal at once and produces both failure directions simultaneously.
   Answering this needs the git-ai schema, not the reader's code. I will flag it
   and say I cannot close it.
2. **Whether a store reuses an id after deletion.** Same class of question.
3. **Whether the grouping rule is genuinely deterministic** for inputs I have no
   fixture for. I can only judge the cases the committed fixtures exercise; a
   derivation can be correct on every fixture and wrong on a shape nobody
   harvested.
4. **Whether two readers could ever be pointed at one session.** That is
   resolution's behaviour, not a reader's, and it is A1 territory — now guarded
   by `SessionRebound`, but the guard refuses the write rather than proving the
   mapping.

---

## Output format per reader

For each: a verdict of **holds** / **holds with a named risk** / **does not
hold**, then R1 through R4 each answered in one or two sentences with the
evidence line, then anything I could not determine and what would close it.
Short. A review nobody reads has the same value as no review.
