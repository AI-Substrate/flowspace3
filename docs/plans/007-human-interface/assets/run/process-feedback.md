# Process feedback — pij-team run #3 (plan 007), written as it happened

The prototype-improvement duty (packet i9), kept as a running log rather than
reconstructed at the end, because the second half of a run never remembers what
the first half cost. PM seat: `pij-near-carp`. Prime: `pij-instant-lynx`.

## What the templates got RIGHT, and should not be touched

1. **The ack as the control point.** Both real defects in this plan surfaced at
   an ack, before a line of code existed: the payload-DTO fence violation (mine,
   to prime) and the fixture-to-live swap seam (u-t's, to me). Neither would have
   been cheap to find in a diff. The template's insistence on "ack with a
   NUMBERED plan, no code first" is the highest-value line in it.
2. **Current state written to be falsifiable.** The PM packet's claims were
   checkable in one read (`worktree exists; plan committed at f3cc640`), and one
   of them — the impl-guide's zero-shared-files claim — turned out to be FALSE
   and was caught in twenty minutes because it was stated precisely enough to
   test.
3. **Constraints carrying their reasons.** Every constraint I inherited told me
   why, so when the "seeded testkit store" constraint met a repo with no
   deterministic store, I could satisfy the REASON (determinism for a byte
   witness) rather than the letter, and say so with evidence. A reasonless
   constraint would have produced either a bad golden or a stall.

## What was MISSING and cost time

1. **The impl-guide claimed zero shared files without checking the types.** It
   named the unit paths correctly but not the payload DTOs the renderer must
   read, and those lived inside another unit's fence
   (`crates/daemon/src/search.rs`). Suggested encoding: the impl-guide template
   should require a line per unit naming **what the unit READS that it does not
   own** — consumption is where fences actually collide, not production.
2. **No template line about pre-existing PROOF that must survive.** The plan's
   invariant was "the envelope must not move", but nothing in the packet
   template asks a coder to name the check that proves the invariant still
   holds. I added it by hand to all three coder packets ("if
   `envelope_goldens` goes red you changed the agent contract, STOP"). That
   should be a template field: *the tripwire, and what a red one means*.
3. **The canary asks a seat for an identity it cannot see.** All three fresh
   seats mis-stated their own pij id — two said "unknown", one gave its worktree
   name as `pijId` and its branch as `spawnId`. `pij whoami` exists and is the
   canonical answer (prime, 2026-08-28); the canary instruction should say "run
   `pij whoami` and quote its output" rather than "tell me your id".
4. **Nothing in the templates says where SEARCH resolves.** Every coder works in
   a worktree; the index covers the main clone; so every hit resolves to a path
   outside the seat's tree, and one coder lost a search to it before reporting.
   Until plan 006 lands, packets should carry the two-step explicitly: use the
   hit to find the FILE, then read the same relative path in your own worktree.

## Frictions captured this run (`harness observe`, buffer NOT cleared)

| id | what |
|---|---|
| DL-001 | `harness boot` reports compose "not running" in every worktree — `docker compose ps` is cwd-scoped, the shared db is up. Trains seats to ignore a red stage. |
| DL-002 | `pij report now` rejects a >280-char field only after the command runs, with no length in `--help`. |
| DL-003 | A test stub server copied from the established `ping.rs` pattern blocks forever in `accept()` when the case never connects — cost a 900s and a 240s timeout. Encode a bounded stub helper in `fs3-testkit`. |
| DL-004 | Seats cannot state their own pij id (see above). |
| CONF-001 | `ddocs` schema discovery does not walk up to the repo root; `ddocs set` fails from inside a plan folder and works from the root. |
| (coder) | `lean-ctx ls` on a Cargo registry path returned the repo tree instead. |
| (coder) | LSP `references` on a freshly-added exported symbol returned none while a real callsite existed. |

## Decisions worth stealing for the next run

- **Freeze the proof, not just the interface.** The byte-goldens were captured
  from the PRE-PLAN binary through the SAME harness that later asserts them. A
  witness minted by the code it polices is not a witness, and the harness
  refuses to let a capture run report success.
- **Write the collision map into every packet, identically.** Three coders in
  one crate, each told the same line-level map of who may touch which region of
  `main.rs` and `Cargo.toml`. Nobody had to guess what a sibling was doing.
- **Name the DECLINED options in a grant.** u-r's allowlist grant lists the four
  crates it may add AND the ones already considered and rejected, with reasons,
  so a crate choice cannot be silently re-litigated mid-unit.
- **A criterion checked before its risk has passed is a criterion nobody
  re-checks.** ac-0002 was provable at the end of phase 1 and was deliberately
  left unchecked, because it must hold after the renderer lands.

---

# Part 2 — what the rest of the run taught, written at close-out

## The three template changes I would make tomorrow

1. **A unit must declare what it READS, not only what it owns.** The
   impl-guide's zero-shared-files claim was false because it listed unit PATHS
   and never asked what each unit CONSUMES. u-r needed types that lived inside
   u-w's fence. Consumption is where fences actually collide.
2. **A packet needs a TRIPWIRE field.** "The check that proves this plan's
   invariant still holds, and what a red one means." I added it by hand to all
   three coder packets and it worked — every coder stopped correctly on a red
   golden instead of investigating it away. It should not depend on the PM
   remembering.
3. **The canary should ask for `pij whoami` output, not for an identity.** All
   three seats mis-stated their own id, because a seat cannot see the id the
   registry minted for it. Prime confirmed `pij whoami` is the canonical answer,
   and the reviewer's canary used it and was clean.

## The two rules this run produced, both learned the hard way

- **A ruling is not closed when the PM has the answer — it is closed when the
  BLOCKED WORKER has it.** Prime approved u-w's dependency; I recorded it in the
  impl-guide and did not relay it, and u-w sat blocked for an hour before asking
  again. (DL-007.)
- **A ruling recorded where it was made is not recorded where it is READ.** The
  goldens exemption was fully reasoned in PROVENANCE.md while the first-light
  transcript still flatly claimed no golden was modified. The reviewer caught it,
  I fixed it — and then made the same mistake again by adding a NOTE to ac-0003
  while its claim still asserted ssh. Two instances in one session, from one
  cause: the checked surface is the claim, not the explanation beside it.

## What the cross-model review was worth

Seven findings. The two HIGHs were both against the PM, and neither was
reachable by the gate:

- The broken-pipe contract was missing because the test that proves it had never
  been on this branch — a gate only runs what is present, so ABSENCE is
  invisible to it. `flowspace3 status | head` panicked with exit 101.
- Standing PRD req 59 messages were swallowed in human mode by my own
  duplicate-diagnosis fix, because no render surface draws messages and nothing
  asserted they survive rendering.

Both now carry the test that would have caught them. The four MEDIUMs went back
to the units that owned them; all four returned fixed with proofs, and none
needed arguing. **A reviewer that reads the diff against the PROMISES finds what
a gate cannot: things that are absent, and things that are claimed.**

## Hazards this run hit that the tenets do not yet name

1. **The base moves under a plan.** Main gained daemon authentication, then five
   more commits, while three coders built. The first symptom was a coder's live
   smoke test failing against a door that had been locked. Suggested encoding: a
   staleness signal when the branch is behind `origin/main` on `crates/`.
2. **A merge that deletes a test deletes the evidence that it deleted
   something.** Both of my silent losses this session took a test with them.
   Cheap check: warn when a merge REMOVES test files present on the merge-base.
3. **A streaming test with no deadline fails by looking like work.** u-w's hung
   for 58 minutes; my own goldens harness cost 900 seconds to the same shape. A
   bounded stub helper in the testkit would have prevented both.
4. **A shared worktree cannot host a reviewer and a reconciling PM at once.** I
   invalidated two of the reviewer's gate runs by merging main underneath it.
   Either give the reviewer its own checkout, or the PM freezes the tree while a
   review round runs. My scheduling error, and a template-level fix.
5. **A false production alarm costs more than a missing one.** The migration
   guard reported "production version 13 -> blank" when it had merely failed to
   PARSE a config that a newer binary had written. Two seats stopped work
   believing they might have written to production. They were right to stop —
   which is exactly why the alarm must not be able to cry wolf.

## What the architecture actually bought, measured

Three coders working simultaneously inside one crate produced **two conflicts**
across the entire convergence — `Cargo.lock` and the arch allowlist, both
additive. `crates/cli/src/main.rs` auto-merged every single time, because the
packets carried an identical line-level map of who may touch which region. The
named composition risk (u-t's fixture-to-live swap) cost nothing, because by
composition time u-t had already been pointed at the real client seam.

That is tenet 1 and tenet 3 paying out in the currency they promised: the
architecture, not the orchestration, is what made three parallel agents cheap.
