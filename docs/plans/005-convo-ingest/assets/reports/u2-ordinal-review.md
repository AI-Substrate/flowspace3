# Ordinal derivation review — all four readers

Reviewer: u2 (pij-appalling-slug), the seat that wrote the consumer. Read-only,
against `005-convo-ingest` on PM3's branch. Criteria are the pre-registered
rubric in `u2-ordinal-review-rubric.md`; findings name which failure direction
they cause (false MATCH drops a real turn, false MISS duplicates one).

## Verdicts

| reader | derivation | class | verdict |
| --- | --- | --- | --- |
| claude (u1a) | first uuid of `message.id` group | group-derived | **holds with a named risk** — F-A1 |
| omp (u1b) | record `id` | record-derived | **holds** — one note |
| pij ledger (u1b) | `seq` as decimal string | record-derived | **holds** — lowest risk of the four |
| metrics-db (u1d) | first `id` of `message.id` group | group-derived | **holds with a named risk** — F-D1 |

Nothing here blocks composition. Two findings should be fixed or written down
before first light; one of them is a correction to my own earlier claim and
matters more than either reader finding.

---

## F0 — CORRECTION TO MY OWN ANALYSIS. Read this first; it went to prime.

I told you subsequence and containment assertions "catch under-emission and
reordering" and are blind only to over-emission. **The under-emission half is
wrong.** Reviewing the readers made me check it properly.

`assert_ordinals_are_a_subsequence` requires the emitted ordinals to be an
in-order, repeat-free subsequence of the store's ids. Emitting FEWER records is
still a valid subsequence. Emitting MORE (from a broken grouping) is also still
a valid subsequence, because each extra ordinal is a real store id in order.

So the accurate statement is stronger and simpler:

> **The subsequence assertion constrains ORDER, REPEAT-FREENESS and MEMBERSHIP
> in the store's id set. It is blind to CARDINALITY IN BOTH DIRECTIONS.**

What it does catch: reordering, repeats, and an ordinal that is not a real
store id — which is a genuinely useful third leg, and the reason F-A1 below
would be caught on a fixture that exercised it.

`assert_oracle_prose_appears` catches deleted PROSE where an oracle exists, so
it partially covers text loss — but not record-level cardinality in either
direction, and claude has no oracle at all under the phase-1 subset ruling.

This makes the count/equality recommendation MORE necessary, not less: an
independently-derived count is the only thing that catches either direction,
and set equality catches all four failure modes at once. Please correct the
version you sent prime — my wording understated the gap.

---

## F-A1 — claude: a missing uuid becomes an EMPTY ORDINAL. False MATCH.

`crates/providers/src/conversation_sources/claude.rs`, in `record`:

```rust
ordinal: line.uuid.clone().unwrap_or_default(),
```

`line.uuid` is `Option<String>`, so a line without a `uuid` yields `""` as its
ordinal. This is the exact hazard R2 of the rubric names, reached by a default
rather than by a decision.

**Why it is the worse direction.** Two uuid-less lines both derive `""`. The
ledger stores the first and then treats every subsequent one as already seen —
in this poll and in every future poll, forever, because `""` is now a durable
ledger row. Real turns are DROPPED, silently, and no assertion in the suite
fires: `""` is not a store id, so `assert_ordinals_are_a_subsequence` WOULD
catch it, but only on a fixture that contains a uuid-less line, and none does.

**Reachability: I cannot close this from the code.** If claude always writes a
`uuid` then this is unreachable today and the fix is cheap insurance; if it ever
omits one — a partially-flushed line, a future record type, a schema change —
the failure is silent and unrecoverable. The parser already models the field as
optional, which is the reader's own statement that it might be absent.

**Suggested fix, u1a's to make:** refuse rather than default. A line with no
uuid has no expressible ordinal, exactly as omp says of its `title` header, so
skip it explicitly or return a provider error — not `unwrap_or_default()`. If
it is genuinely impossible, say so in a comment and make it a hard error, so
the impossibility is asserted rather than absorbed.

## F-D1 — metrics-db: the QUERY PREDICATE is part of the grouping rule, and is not frozen as such.

The module froze the emit allowlist and the merge key, which is exactly what my
rubric asked for and it is well done. But the grouping runs over **the rows the
query returned**, and the read is:

```sql
select id, tool, event_json, event_ts from metrics
 where event_kind = 5
   and external_session_id = ?1
   and json_extract(event_json, '$.a."1"') = ?2
   and id > ?3
 order by id
```

The scope predicate `?2` is applied PER ROW, not per session. So the row set —
and therefore which row OPENS a group, and therefore the ordinal — depends on
`event_kind = 5` and on the scope expression as much as on the merge key.

**Consequence.** If a session's rows can carry differing scope values, or if
the scope spelling ever changes for the same repository (an identity re-derived
differently, a remote normalised another way, a worktree moved), the same
session yields a different row subset, a group's first `id` can change, and
every stored record looks new. That is the doubling failure reached by touching
something that does not look like the derivation at all — the same shape as the
allowlist hazard the module already documents, one level further out.

**Suggested fix, u1d's to make and cheap:** extend the frozen rule to name the
predicate. Something like: *the ordinal is the smallest `id` in its group, where
the group is drawn from rows matching `event_kind = 5` and the scope expression;
changing either changes the row set and therefore the ordinal.* No code change —
this is a freeze statement, and the module is already written in exactly that
register.

**What I cannot close:** whether one session's rows can genuinely differ in
scope. That is a property of how git-ai stamps `$.a."1"`, not of this reader.
If they cannot, F-D1 downgrades to documentation hygiene.

## F-D2 — metrics-db: the split-group-across-polls artifact is undocumented.

`open_groups` is built per read call and the query filters `id > cursor`, so a
group whose first row sits below the cursor and whose later rows arrive in a
subsequent poll opens a NEW group under the later row's id — permanently.

I checked the arithmetic and **this does not duplicate**: a later rescan reads
from `id > 0`, regroups the whole session, and emits the group under its
original first `id`, which the ledger has already seen and drops. The fragment
stored by the intermediate poll simply stays as its own turn.

So this is correct behaviour, identical to claude's — but claude documents it
explicitly and at length, and metrics-db does not mention it at all. A future
maintainer who notices a session with two turns where the store has one message
needs to find that this is by design, not chase it as a bug.

## Notes that are not defects

- **omp: a record with no `id` is silently dropped.** `record()` returns
  `Option` and `string(value, "id")?` skips. This is documented for the `title`
  header slot and the reasoning is right — no ordinal is expressible for it.
  The behaviour is general, though: any FUTURE record type lacking `id`
  disappears with no error, and per F0 no current assertion sees it. Not worth a
  code change now; worth knowing when a record type is added.
- **omp derives from the record-level `id`, not the inner message id**, exactly
  as frozen. Record-derived, no grouping, no rule dependency. Lowest-risk
  structure of the four together with pij.
- **pij: `seq.to_string()`, record-derived, no grouping, `parent_ordinal: None`.**
  Nothing to flag. The lexicographic trap is closed by the ordinals-are-opaque
  ruling rather than by padding, correctly.
- **claude's first-of-group choice is right and its reasoning is exactly the
  invariant my ledger needs.** First is stable under a rescan because a full
  re-read regroups the same blocks and recomputes the same first uuid; last
  would move as the group grows and defeat the dedupe. Its grouping is
  map-keyed rather than adjacent-run, so a `message.id` that reappears after an
  interleaved `user` record still folds into its original group and the group
  keeps its first block's POSITION — which is what preserves store order.
- **claude's thinking-block insight is the grouping-rule hazard applied
  correctly**, and it is the best single line in the four modules: dropping a
  thinking block's TEXT while keeping its line's group MEMBERSHIP, because the
  first block of a group is routinely a thinking block and skipping those lines
  would move the ordinal and double every stored claude conversation.
- **claude's split-across-polls section is right, including the part that is
  ugly**: a group split across two polls yields two turns permanently and the
  first is never backfilled. Holding the trailing group back would lose the
  final turn of any session that ends mid-message — silent loss on exactly the
  conversation someone is watching live. Correct trade, honestly stated.

## PM3's question: does any ordinal depend on scratch-directory lifetime?

**No.** I checked all four specifically because of the u1b flake.

- claude's `merge_records` does take a `session_dir`, but it flows into
  `blocks_of` for spilled tool-result paths — the ITEMS. The ordinal is
  `line.uuid`, straight from the file's own bytes.
- omp and pij derive from `id` and `seq`, both record content.
- metrics-db derives from a database column.

No ordinal is derived from a path, a directory name, a mtime or anything with a
lifetime. The flake can move or delete a scratch directory and the ordinals of
whatever is read are unaffected. If the flake deletes a directory mid-read the
reader will fail to READ, which is loud, not mis-derive, which would be silent.

## Accepted correction to my own earlier claim

PM3 corrected me that the per-unit counts are not circular for these two
readers: u1a pins a distinct-ordinal count plus the continuation blocks that
must never become ordinals, derived from the store; and PM3 verified u1d's
16-and-10 arithmetic independently with its own SQL against the fixture before
u1d wrote a line. That is right and my "circular" framing was the weaker
argument. The gap is uniformity and the future reader — a count a new reader
can simply omit — not the honesty of the two numbers that exist. F0 above is
the argument that actually carries it.
