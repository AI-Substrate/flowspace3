# Process feedback — plan 006, pij-team validation run #2

**From**: pij-parliamentary-leopon (PM) · 2026-08-28
**For**: `.agents/skills/pij-team/EXPERIENCES.md` and the tenets

Run shape: prime + PM + two coders + one cross-model reviewer, investigate-then-build,
one plan branch, worktree-per-coder, PR held for prime.

## What the tenets got right, with the evidence

**Tenet 2 (freeze the seams before you spawn anyone)** — in investigate mode the
seam-freezing *is* phase 1, and it paid immediately: four provisional units
became two. u-b was refuted outright (identical content already cost zero
provider round trips), u-d was absorbed into u-a (both ends missing one
detector), and a fifth candidate — a default result cap — already existed in the
code. **Roughly half the provisionally-planned work was retired by measurement
before a coder was spawned.** That number is the argument for the mode.

**Tenet 3 (engineer the collision surface down)** — the ruled units owned
disjoint files with `boot.rs` as the only shared one, touched by a single unit.
Both merges were clean, and composition was wiring rather than negotiation. The
tenet works exactly as advertised.

**Tenet 5 (make done mechanical)** — the strongest single decision of the run:
each unit's done bar was a *predicate flip in the probe harness's receipt*
(`p1_auto_discovered` 0→1, `p4_root_still_registered` 1→0). "Done" became a
re-run, not an opinion, and the phase-1 harness became the phase-2 regression
suite at no extra cost.

**Tenet 11 (evidence over conclusions)** — worker measurements outranked the
orchestrator three times, and every time the worker was right: knobbler refuted
my glob-vs-LIKE claim with source and tests; louse diagnosed a probe miss as
bootstrap-vs-steady-state rather than accepting a failure; amphibian corrected
both my fixture design and my invariant framing.

## What was wrong or missing

### 1. `d4` contradicted a behavioural done bar (amended mid-run)

The coder template says workers ship the integration *recipe* and never wire.
u-a's done bar was a live probe flip, which is unreachable if the supervisor is
never in the composition root. The seat caught the contradiction at ack — the
control point working as designed. Ruled: wire when the file is uncontended AND
the done bar is behavioural; ship the recipe regardless. Prime amended the
template the same hour.

### 2. Packets must exist in the worktree before the seat boots (`DL-003`)

I created both coder worktrees, *then* committed their packets. Both seats
booted into trees where their own dispatch file did not exist. One refused to
guess and reported `ENOENT` — correct behaviour, and my cost, not its.
**Encode**: the dispatch step should refuse when the packet path is absent in
the target worktree. The packet is the interface to a worker; its absence should
be a refusal, not a discovery the worker makes.

### 3. Safety rules must name the forbidden OPERATION, never the resource (`CONF-002`)

I wrote "never point at the live database" and meant "never write to it". A
coder read it as a total ban and withdrew a valid read-only calibration table,
nearly rebuilding a synthetic corpus to re-derive an approved number. **The
conservative agent over-complies by discarding valid work, and that failure is
quieter than under-compliance because nothing errors.**

### 4. Isolation was tribal knowledge with a bill attached (`DL-004`)

The recipe for running a daemon safely was four environment overrides. Two
independent seats got it wrong the same way within minutes, and it cost real
provider calls: a shared "throwaway" database had accumulated 15 roots and a
6,520-job backlog, and ambient config selected a real provider. The primitive
already existed for the *test* tier (`FreshDatabase`) and was unreachable from
the *live* tier. Encoded the same day as `flowspace3 daemon --sandbox` (#48).
**The general shape: when two seats independently reinvent a worse version of an
existing primitive, the primitive is in the wrong place.**

### 5. `harness plan pr-body` refuses while any AC is open (`DL-005`)

It encodes "every criterion closes before the PR". This plan's last AC is a
claim about live retrieval, and measuring it *is* the go-live event — so the
plan with the strongest evidence discipline was the one the evidence tool
refused to serve. **Encode**: render open ACs as a `PENDING` section with their
notes. A PR that names what is not yet proven is more honest than one that
cannot be written.

### 6. `builder`'s `[~]` has no home in the ddocs state enum

Valid states are `unchecked|checked|blocked|human-skipped|na`. Both coders hit
it. The honest workaround — leave the task unchecked, record execution
evidence, check on proof — is fine, but the discipline and the schema disagree
about whether in-progress is a state.

## Three additions I would make to the tenets

### A. A measurement has preconditions, and a predicate must refuse when they fail

First light ran with fake providers for fleet safety. The P3 retrieval
predicates came back `0` — which reads as "no version resolution, no leak" and
was actually "retrieval carries no semantics". I nearly reported it as success.
Proof it was an artifact: 8 divergent vectors existed and a query quoting the
marker function's own body verbatim scored **0.1889** against unrelated files.

The harness now reads the embedder from the daemon's boot line and emits
`unmeasurable-fake-embedder` rather than a number it cannot support. **A
misleading zero is worse than a refusal** — and the same instinct caught me
about to ship a "must be 0" gate on a population that can never be 0.

The rule needed one more turn of the screw, found by driving the new
`daemon --sandbox` verb: the gate originally refused on `fake` and *proceeded*
on `unknown`. But the sandbox logs beside its own temp config, where the script
was not looking — so a real run could have reported P3 verdicts with the
embedder unidentified, computing the answer on faith. **An unverified
PRECONDITION must refuse exactly like an unverified RESULT.** Absence of proof
is not proof.

### B. Composition is where EMERGENT defects appear — units can each be correct and the pair wrong

The plan's most important defect was invisible to both units' tests and to first
light. Search returned rows with null path, repo and worktree. The mechanism:
the candidate gate proved a caller-anchored element carried the vector's
`raw_hash`, but the representative resolver then picked the globally lowest-id
element with that hash *without re-applying the caller scope* — so with one body
in several blobs it chose a foreign one, and the provenance `LEFT JOIN`s
returned nulls.

It required **many checkouts of one repo**, which only became the normal shape
*because u-a auto-registers worktrees*. u-a's success created the population that
exposed u-c's gap. Neither unit alone could produce it.

The generalisation, which then found a second instance: **a scope filter over
content-addressed storage must be applied at every step that CHOOSES a row, not
only at the step that ADMITS one.** The reviewer applied that rule one chooser
downstream and found the smart-content path — where the symptom differs (a
silent false negative, not a null leak), which is why the raw-only fixture could
not have caught it.

**Three times in this run, the thing that caught a defect was RUNNING the
composed artifact rather than reading it.** All three were invisible to passing
tests: the spend incident, the null-path leak, and a `set -e` trap in my own
harness that made the committed probe abort mid-run on the normal isolated-daemon
shape.

### C. The law generalises outward, one surface at a time

The rule this plan discovered kept extending, and each extension was found the
same way — by looking at what a *caller* received rather than at what a step
returned:

1. **Choosing** — a scope filter over content-addressed storage must be applied
   at every step that CHOOSES a row, not only where one is ADMITTED. Found in
   the raw representative resolver (f-001).
2. **The next chooser** — the reviewer applied that rule one step downstream and
   found the smart-content chooser doing the same thing, with a different
   symptom: a silent false negative rather than a null leak (f-005).
3. **Rendering** — scope must be VISIBLE at every surface that RENDERS a chosen
   row, not merely enforced where it was chosen. Identical-looking rows from two
   checkouts in a human interface is the same defect with a nicer font (the TUI
   question, ruled by prime: show provenance only when it disambiguates —
   honest at ambiguity, silent when unambiguous, the same shape as the
   weak-match hint).

A defect class is not closed when the instance is fixed. It is closed when
someone has walked the rule to the next surface and found nothing.

## On the reviewer

Five findings, three live, and **three of the five were things the PM owned or
caused**: the probe abort (my code), the absence-grace hole (my ruling — I
required a two-pass grace and an `Err` between two absences still removed), and
the smart-chooser class-mate (my generalisation, not chased). A cross-model
reviewer that audits the orchestrator's own work, not just the coders', is worth
more than one that checks diffs against a brief.
