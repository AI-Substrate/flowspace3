# Ordinal derivation review — the rubric, published BEFORE any reader lands

Reviewer: u2 (pij-appalling-slug), the seat that wrote the consumer. Assigned
by PM3 2026-08-28. Read-only; reviews are performed against the plan branch
when PM3 says a reader is merged, never against a live worktree.

This file is deliberately written before I have seen a single reader
derivation, so the criteria can be challenged on their own merits rather than
after they have already produced verdicts.

---

## Rulings that have already changed this rubric

Recorded here rather than silently folded in, because a rubric that quietly
drops a rejected idea loses the lesson that rejected it.

- **R3 stands; the brief's "monotonic in store order" is struck as a ledger
  criterion** (PM3, 2026-08-28). It was never a requirement of the consumer.
- **The lexicographic remedy in R3 was WRONG and I could not have seen why.**
  I offered two fixes: zero-pad the pij seq, or rule that ordinals are opaque.
  PM3 ruled against padding with evidence I had no access to — the committed
  expectations are BYTE-PINNED to the unpadded strings (`build_pij`
  stringifies `seq` with `id_key seq`, ordinals 118..167), so padding would
  fail `assert_ordinals_are_a_subsequence` against the very fixture it was
  meant to protect. The remedy would have broken the thing it guarded. The
  ruling took the other half: **ordinals are opaque identities, nothing ever
  orders them, and any comparison other than equality is meaningless** — a
  written rule in every service page instead of a padded string. Flagging the
  trap was right; picking the remedy from outside the fixture's constraints
  was not, and that is a reviewer's standing hazard worth naming: a remedy
  proposed without sight of what is pinned can cost more than the trap.
- **Unclosable item 1 (metrics-db rowid across VACUUM) is CLOSED**, by PM3
  reading the git-ai schema — see that item below.

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
means something, that reader silently regresses.

**RULED (PM3, 2026-08-28):** ordinals are OPAQUE IDENTITIES — nothing orders
them and any comparison other than equality is meaningless — written into every
service page beside the derivation. My other suggestion, zero-padding at
derivation, was rejected with evidence: the expectations are byte-pinned to the
unpadded strings, so padding would fail the fixture assertion it was meant to
protect. See the rulings section at the top.

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

1. ~~**Whether the underlying store guarantees its own id is stable.**~~
   **CLOSED 2026-08-28 by PM3, with the schema.** The worry was that sqlite
   `rowid` is not stable across `VACUUM` unless the column is an
   `INTEGER PRIMARY KEY` alias, which would break R1 for every stored
   metrics-db ordinal at once and produce both failure directions together.
   git-ai's table is
   `CREATE TABLE metrics ( id INTEGER PRIMARY KEY AUTOINCREMENT, event_json
   TEXT NOT NULL, ... )` — an explicit `INTEGER PRIMARY KEY`, so it IS the
   rowid alias and `VACUUM` cannot renumber it; sqlite only renumbers tables
   without one. `AUTOINCREMENT` additionally guarantees an id is never reused
   after a delete, which closes item 2 for this store as well. **Metrics-db
   ordinals are durable.** u1d has been told to derive from the named `id`
   column rather than the bare `rowid` keyword, and to quote that schema line
   in its service page — it is a load-bearing property of a database we do not
   control.
2. **Whether a store reuses an id after deletion.** Same class of question, and
   now closed for metrics-db only, by the `AUTOINCREMENT` above. Still open for
   claude, omp and the pij ledger: I can see what a reader DERIVES from, but
   not whether the store that mints it will ever mint it twice.
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

---

## Asked by PM3: do the structural expectations catch a broken grouping, and
## is a shared numeric expectation needed at composition?

Evidence I was given: u1d mutated its reader to stop merging `message.id`
groups — 22 records emitted where 16 are correct — and BOTH
`assert_ordinals_are_a_subsequence` and `assert_oracle_prose_appears` still
passed. u1a reproduced it on its own store: 20 turns instead of 13, subsequence
still green.

### Why they miss it, and why that generalises

This is not a gap in those two assertions; it is what they are.

- **A subsequence assertion cannot detect OVER-emission.** Splitting one group
  into three emits more ordinals, but they are still real store ids, still in
  order, still without repeats — so they are still a valid subsequence. The
  assertion constrains ORDER and MEMBERSHIP. It says nothing about CARDINALITY.
- **A containment assertion cannot detect over-emission either.** Splitting a
  group scatters prose across more records; it deletes none, so the oracle's
  prose still appears.

Generalised, and worth writing into the testkit's own docs so the next reader
does not assume otherwise: **subsequence and containment assertions catch
under-emission and reordering. They are structurally blind to over-emission.**
A broken grouping is over-emission. The two assertions we have are the two
that cannot see it.

### My answer: neither option as posed. Per-fixture number, shared assertion.

**Per-unit count tests are not enough**, for two reasons that have nothing to
do with the seats' diligence. They are per-seat, so the next reader added to
this system can simply not write one and nothing will notice. And a count a
seat derived by RUNNING ITS OWN READER is circular — it pins whatever the code
did on the day it was written, which makes it a change-detector rather than a
correctness test. Still worth having, but it does not prove the grouping is
right; it proves the grouping has not moved.

**A composition-level numeric expectation is the wrong altitude.** It is
further from the defect, it runs later, and when it fails it cannot say WHICH
reader mis-grouped. Composition already plans a turns-count-versus-oracle check
in the first-light proof; that is a good backstop and a bad primary.

**What I would do instead:** put the expected record count in each fixture's
committed `expectations.json` — the file that is already byte-pinned and
already carries `grade_of_proof` — and add ONE shared assertion in
`fs3_testkit::Expectations` that every reader's contract run calls. That buys
the uniformity of a shared check with the locality of a per-fixture number, in
the place readers already consult, and a new reader cannot forget it because
the contract suite calls it.

**The condition that makes it worth anything:** the number must be derived
INDEPENDENTLY of the reader — from the oracle where the oracle covers that
store, otherwise from a hand-count recorded in the fixture's `PROVENANCE.md` —
and `grade_of_proof` must say which. A count harvested from the reader's own
output is circular no matter which file it lives in. Given the phase-1 ruling
that the pinned reconvo.py has no claude-native reader, claude's number will be
a hand-count and should be graded as one, exactly as its ordinals already are.

**One stronger option where it is available:** for oracle-covered stores,
assert the emitted ordinal set EQUALS the expected set rather than being a
subsequence of it. Equality catches over-emission, under-emission and
substitution at once. It is only possible where the expected set is genuinely
known, which is why it is an addition for those stores and not a replacement
for the subsequence rule everywhere.

This is a judgement, not a measurement. The measurable part is the first
paragraph: two assertions were shown blind to over-emission by two seats
independently, and that is a property of the assertions rather than of those
two readers.
