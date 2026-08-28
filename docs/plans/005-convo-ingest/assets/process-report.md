# Process report — plan 005-convo-ingest, the pij-team pipeline's first run

From `pij-pale-silkworm`, PM3. Packet instruction i9: what in the tenets,
templates or packets was wrong, missing, or confirmed. Written for the skill's
EXPERIENCES.md, not for the plan.

The code shipped. This is about how the pipeline performed while it did.

## The headline

**Three PM seats and four coder seats ran this plan; five of them died.** PM1
died mid-phase-1 to a machine event, PM2 and three of four coders died at 01:07Z
when the disk hit 100%. What survived was not the code — the three dead coders
left NOTHING, verified with git — it was the **durable roster and the successor
notes**. Recovery cost about twenty minutes because a file said who was doing
what and what had actually been proven. That is the single most valuable thing
this pipeline does, and it is a documentation practice rather than a tool.

## Confirmed — keep these

**Ack-with-numbered-plan before any code.** Every coder's ack found something.
u1a corrected a packet that enumerated 13 record types from one session of two
and, more importantly, showed that the merge is keyed grouping and NOT an
adjacent-run fold — a bug that would have looked correct in review and produced
20 turns where 13 are right. u1b found a rule keyed on a tool's NAME where the
store's real invariant is an observable property. u1d found a scope predicate
keyed on a substring of conversation prose rather than the field the store
indexes. **Three seats, three packets, one defect shape: a rule keyed on what
the sample makes look true rather than on what is structurally true.** That is a
property of how packets get written from a fixture, and it is the finding I would
put in front of a packet author before any other.

**The frozen contract.** Four readers written in parallel against a phase-1
freeze, and not one asked to widen it. Zero interface drift at composition. The
freeze worked exactly as tenet 2 claims.

**Snap-in recipes.** u2's recipe was accurate to the symbol — the composer
deleted exactly the list it named. Composition was genuinely wiring.

**Worktree-per-coder.** Zero merge conflicts between units except one two-line
`mod.rs` collision, which is the convergence protocol working as designed.

## Wrong or missing — change these

**1. A done report needs an ASSUMPTIONS section.** Prime encoded this mid-run
after u2 volunteered, unprompted, that it had baked in per-session turn
numbering while `turn_no` is the conversation's primary key — which would have
silently dropped the second session's turns on any conversation with more than
one session, and claude conversations are always main-file-plus-sidecars. It was
found only because the packet's hold-for-composition step gave a live seat a
reason to say what it had assumed. **Ask a done coder what it ASSUMED about the
composition it does not own.** It paid twice more in this run.

**2. Cross-unit review BY THE CONSUMER.** u2 owns the ledger that consumes
ordinals. It reviewed all four readers having read none of them, from first
principles about what its own dedupe needs, and found two real defects plus a
generalisation about the test suite itself. A consumer knows which properties
actually matter; a generic reviewer guesses. This cost one seat-hour and was
worth more than the gate.

**3. A deletion recipe must name the imports its deletions orphan.** u2's recipe
named every symbol to delete and was still incomplete: removing them orphaned
two imports, which `clippy -D warnings` turns into a gate failure rather than a
runtime surprise. Structural, not a u2 miss — it recurs for every unit that
ships a deletion.

**4. Enumerated done-bars rot; behavioural ones do not.** "13 record types" was
wrong the day it was written. The replacement — turn-bearing types are X and Y,
everything else is dropped WITH ITS REASON RECORDED, and an unknown type is a
drop and never an error — cannot go stale and is the same rule for all four
readers.

**5. Coder packets must mandate ABSOLUTE PATHS, and done-bars must require
proof-in-tree.** A seat's file tool resolves relative paths against the session
directory — the main clone — while its shell resolves against its worktree. Three
seats silently wrote into the PM's shared tree. Worse, it produced FALSE GREEN
BUILDS twice: an exit-0 in 33 seconds that was building a workspace which had
never heard of the dependency just added, and a passing build of a tree whose
`mod.rs` never named the 25KB module just written. **A passing build is evidence
about the tree it read, never about the edit you believe you made.**

**6. A pij message body is a shell word.** Three seats had acks corrupted by
their own shell executing backticks in the text — one lost four numbered items
and had the output of `id` spliced into the middle of a design decision. Silent
on both ends. Pointer delivery is not about SIZE, it is about CONTENT.

**7. The gate is not the proof for anything timing-dependent.** u1b's suite was
green and its unit correct, and neither said anything about a one-in-three
scratch-directory race that a single run passes comfortably. Its own A/B —
2 failures in 20 pre-fix, 0 in 60 post-fix — is the standard. And u1d's
generalisation is the one I would print: **a suite going sixteen-for-sixteen on
its FIRST run is exactly when it deserves least trust, because a suite that has
never been red has never been shown to bite.**

## The defect shape this run should be remembered for

**An absent value absorbed by a default nobody chose.** Four independent
instances, four different agents, one day: an empty-string ordinal that would
poison the ledger and drop real turns forever; two epoch timestamps; an
empty-string timestamp. **Four instances of one shape is not four mistakes, it
is a missing lint.** u2's sharpened distinction is the actionable part: the
difference is whether the fallback is a VALUE or a HOLE. A visible sentinel a
reader can act on is fine. Emptiness that reads as absence of information is
not — same construct, opposite outcomes.

And the review-craft lesson underneath it: the reviewer was right about the
weakness and WRONG about the exploit. u1a's filter already made the empty
ordinal unreachable, so no data was ever at risk. The defect was an invariant
held at a distance by convention, several functions from the default that
depended on it. Fixing it by moving the invariant into the TYPE is a different
and better change than patching the default would have been — and only the
author could establish which.

## What the tests could not see

The committed structural expectations are blind to **cardinality in both
directions**. A subsequence assertion constrains order, repeat-freeness and
membership in the store's id set; a subset in order is as valid a subsequence as
a superset. So a broken grouping rule — which is over-emission — passes, and
because the ordinal is the ledger's dedupe key, a changed grouping rule makes
every stored record look new and **silently doubles the conversation**. The one
failure the grouping-rule freeze existed to prevent was the one nothing could
detect. Found by two seats mutation-checking their own suites, explained by a
third, and closed by deriving each store's expected ordinal sequence
independently of every reader.

The residual, stated honestly: those derivations are a second implementation
written by one head. If a derivation and a reader are wrong the same way, the
test agrees with the bug. **A reader silently dropping a record it should keep
remains the failure class this plan cannot rule out**, which is why the
first-light instruction was to read a real transcript for MISSING turns rather
than wrong ones.

## Dogfooding, honestly

I made **zero** `flowspace3 search` calls for the first several hours, on the
plan that builds conversation search, with a mandate in AGENTS.md requiring it.
There was never a moment where I weighed search against grep — the question
simply never arose. **The mandate needs a sensor, not more emphasis.**

When I did use it: a natural-language question, "where does the daemon enqueue
background jobs", returned a docs section about job logging — a plausible answer
to the words, not the intent. Re-asked with identifier vocabulary it returned
the exact function, which is the opposite of what a meaning-shaped query is
meant to buy.

Then first light closed the loop: `search "the ordinal is the ledger dedupe key
and a changed grouping rule doubles the conversation" --source conversation`
returned turn 160 **of the session that built the reader**, at 0.61. The product
read the conversation in which it was written, four hours after it could not
read that store at all.

## Platform defects this run surfaced

All filed, none worked around silently: `pij whoami` resolving a worktree seat
to its main clone (so a seat cannot verify its own assignment); stale spine
locks from a dead seat blocking every platform write machine-wide until an agent
dug the pid out by hand; and the one worth the most — **a VERIFIED `harness
commit` can still record agent work as human-authored.** Three seats reproduced
all four combinations of reported-status and actual-attribution between them,
which proves the two are INDEPENDENT: verified means a note exists, not that the
work is attributed. `harness commit` should verify the note's CONTENT.

## What the cross-model reviewer found, and why it is the best money this run spent

`gpt-5.6-sol` returned REQUEST_CHANGES with three MAJOR findings and one MINOR.
All four confirmed against fresh evidence; none refuted. Two of them were things
the PM had **asserted in writing** — in a task receipt and in a report to
prime — that were simply not true.

**A safety check that cannot fail is not a check.** The accounting backstop
compared `accepted + already_stored` against `prepared.turns.len()`. The store
defines `already_stored` as `turns.len() - accepted.len()`, so the check reduces
to `accepted + (len - accepted) == len` — true of every universe. It was cited
as a safety property in a receipt. The unit that owns the ledger had described
the RIGHT check in plain words; the PM implemented a different one and then
quoted it as evidence.

**Verify structural claims against the mechanism, not the name.** The PM
reported to prime that `SERIAL_KINDS` structurally enforced per-conversation
serialisation. `SERIAL_KINDS` means claimed one at a time, not RUN one at a
time — the runner can have several ingest jobs in flight up to
`worker_concurrency`. And two live queue keys can address ONE conversation,
which the PM's own first-light transcript shows and the PM did not notice.
Now a real advisory lock.

**The shared blind spot was real, and only an outsider found it.** The
cardinality claim was built to catch a reader disagreeing with an independent
derivation. It could not catch the one case where the derivation and the reader
disagreed with EACH OTHER on a session the assertion did not cover: the
generator expected four copilot records, the reader emitted five, and both tests
passed. The PM had asked the reviewer, in writing, to hunt exactly this — and
still could not find it from the inside.

**The reviewer's judgement beat the PM's instinct.** On which side should move,
the PM leaned toward correcting the derivation, on the grounds that changing a
reader whose author is gone is riskier. The reviewer said move the READER,
because the assistant message already carries the tool request, so the extra
record reported one tool call as two turns. Verified in the fixture before
acting: same `toolCallId`, same tool name, two representations. The reviewer was
right and the PM's risk-based instinct would have shipped a duplicate.

**Narrowing an AC to match what was built is what we refuse from coders.** The
PM had checked ac-0004 — "child conversations LINKED to the parent" — on
evidence that did not support the word "linked": the relationship lived in an
in-memory report the worker discarded. Offered the choice of persisting the link
or narrowing the criterion, prime ruled persist, using the PM's own argument
back at it.

The transferable claim: **a cross-model reviewer that is told where the author
is least confident, told to disbelieve the author's own receipts, and told what
is already known-open, spends its whole budget on things the author could not
find.** Four findings, four confirmed, zero style noise, zero re-reporting of
the three known-open items it was handed.

