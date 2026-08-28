# Rulings — u1b (omp reader + pij-ledger reader), PM3, 2026-08-28

Seat `pij-suitable-cormac`. Ruling `assets/ack-u1b.md` (your branch, commit
`4142b3e83b70`) by its own numbering. Items 8, 13 and 14 were already ruled and
are not revisited.

---

## Ruling A / item 7 — RESOLVE THE SPILL FROM THE FILE. APPROVED.

Resolve the body from the spill file when the `artifact://` pointer is present;
fall back to the inline body with `truncated = true` when the file is gone.

**Your measurement decided this, and it decided it against the cheaper answer.**
Three findings, each of which alone would have been enough:

1. **The omp preview is not a prefix of its spill file.** An abbreviated sha
   where the file has forty characters, and no `Author:` line at all. So a
   512-byte head of the inline body and a 512-byte head of the file are
   DIFFERENT TEXT — the payload policy does not make the question moot, which is
   the argument I would have reached for if you had not measured.
2. **Two elisions in the MIDDLE.** A hole is not a truncation. Content that
   exists in the store is absent at positions a head-cap never reaches, so
   storing the inline body loses text that nothing downstream can recover.
3. **`total_bytes` derived inline would be wrong by roughly 3x** (895 + 338
   against a 3,949-byte source). Shipping a number that is confidently wrong is
   worse than shipping no number.

The fallback is right and I am glad you called it non-negotiable: a spill file
can be garbage-collected, and failing an entire conversation because one tool
result aged out would be absurd. Mark it `truncated = true` so the degradation
is visible rather than silent.

Your caveat about what you can ASSERT is the best single paragraph in the ack.
The committed spill is itself sanitiser-capped, so a byte-exact claim against
real store output would be FALSE for these bytes. Testing the resolution
BEHAVIOUR — body comes from the file, begins with the full sha and the `Author:`
line the inline body lacks, missing file degrades to inline — is exactly right.
**Never write a test whose claim is untrue of the bytes it runs on, even when it
would pass.**

Globbing `<session-dir>/<n>.*` on the numeric artifact id: approved, and the
reason (extensions vary — `.bash.log` for some ids, `.bash-original.log` for
others) belongs in the service page.

Note for u1a, which I am relaying to it: claude's preview IS a faithful prefix
and claude states its true size, so the two readers resolve spills for DIFFERENT
reasons and with different confidence. Both resolve. That symmetry is now a
requirement, not a preference.

## Item 6 — REJECTED AS WRITTEN. Do not assume one text block.

You measured that no record in this fixture carries more than one non-empty text
block, and concluded `body` is "that single block's text". The measurement is
sound; the conclusion is a fixture fact promoted to a store invariant, and it
would ship a **silent dropper**: the day an omp record carries two text blocks,
your reader keeps the first and discards the second, with no error, no flag and
no test failure. A turn would simply be missing half its prose, in production,
forever.

**Required instead:** handle N blocks. Concatenate them in order, exactly as
u1a merges claude's per-block records, so that one block is the common case
rather than the only case your code can express. Add a named test constructed
with two text blocks proving both survive. The cost is a fold instead of a
`first()`; the benefit is that a store change degrades into a slightly odd turn
instead of silent data loss.

This is the same class as your own item 8 correction — you rejected a rule keyed
on a tool's NAME in favour of its observable property, and this is a rule keyed
on a sample's SHAPE. It is the third instance today across three seats, which
tells me it is a property of how the packets were written rather than of any one
seat's reading.

## Item 5 — APPROVED, and the timestamp catch earns a named test.

`ordinal` = `id`, `parent_ordinal` = `parentId`, `at` = the RECORD-level
`timestamp`.

Your measurement that a toolResult's INNER `message.timestamp` is
epoch-milliseconds while the record-level `timestamp` is ISO-8601 is exactly the
kind of thing that produces a plausible-looking corruption: the contract
specifies RFC 3339, and keying on the inner field would emit integers where
timestamps belong, on 72 of 117 records, and still parse. Write a named test
that pins the record-level field for a toolResult.

The role and source table is approved. On the `[pij from` prefix mapping to
`TurnSource::Peer`: approved — it is the axis the oracle spells `pij_in` and the
axis the enum exists for — with one condition. It is a HEURISTIC over a wire
convention, not a store field, so say that in the service page and make a
non-matching user record fall through to Human/Human rather than erroring. A
convention nobody enforces will eventually not hold, and when it does not, the
reader should degrade to a slightly less precise turn.

## Item 4 — APPROVED, with the unknown-type rule.

Emitting 117 of 193 is legal (the claim is a subsequence) and each of your drops
is justified. The `title` reasoning is the strongest — no `id` field means no
ordinal is possible, so it could not be emitted even if we wanted it.

**Condition, identical to the one I gave u1a and u1d:** an unknown record type
is a DROP, never an error and never a panic. Add a test feeding a type your
allowlist has never heard of, asserting it is dropped silently while surrounding
records still parse. Three readers, one rule.

## Items 1, 2, 3, 9, 10 — approved as written.

Item 2 deserves a note: declining `tempfile` and following testkit's own
precedent means your unit changes NO `Cargo.toml` at all. Compare u1d, which
needs `rusqlite` and consequently needed a fence extension, a workspace
dependency row and a prime-level cost conversation. Not adding a dependency is
a real deliverable.

Item 9's point that the compaction record sits IN the parent chain, so dropping
it breaks the chain across the seam, is the argument that makes ac-0005
non-negotiable. Put it in the service page in those words.

Item 10: never treating `size == offset` as "nothing changed", and never caching
line 0's title, are both correct and both non-obvious. The title slot's
byte-stability is a load-bearing assumption of this entire store — if omp ever
makes that slot variable-width, every byte cursor breaks at once. Say so in the
service page so the next person meets the assumption before they meet the bug.

## Items 11, 12 — approved.

Emitting receipts is right: the ledger is the only store in the fleet that
records delivery state, and a receipt is a real event. One condition — the
receipt body is SYNTHESISED text, not store prose, so it will be embedded and
searched like any other turn. Pin its rendering in a named test and document the
format in the service page. A rendering that drifts between versions makes two
identical receipts read as different turns.

`ordinal` as the decimal STRING form of `seq` is right and is pinned by
`build_pij`, as you found.

## Items 15-19 — approved.

Your refusal to report a green subset as proof for the pij store, where exactly
one prose turn exists, is the correct reading of the tk-c105 subset-oracle
ruling. Treat the structural section as the done-bar and say in the service page
why the prose check is nearly empty for this store. A future reader who sees
`assert_oracle_prose_appears` passing on one turn should learn what that does
and does not prove from your page, not by reading the oracle.

---

**Go on everything.** The only work item that changes is item 6: fold over N
blocks, do not take the first. Everything else stands as you wrote it.

Disk discipline unchanged — Avail in every message, escalate at 15Gi mid-step.
