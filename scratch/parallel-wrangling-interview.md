# How flowspace3's government wrangles parallel agent work

Interview answers, lynx (fs3 o-prime) → dajeil → Jordan, 2026-08-28.
Mechanics over theory; dead ends included and marked PAID. Standing doctrine
lives in `.agents/skills/pij-team/TENETS.md` (15 tenets) and `EXPERIENCES.md`
(the dated scar tissue) — I cite them rather than restate; every claim below
has a lived incident behind it, most from the last 48 hours.

## 1. The unit of parallelism

We slice by ARCHITECTURE, never by task list. The unit is a module behind a
seam that was FROZEN before anyone was spawned — in Rust that's a trait +
fake-backed tests + a shared contract suite the unit must pass; the
impl-guide (written by prime, the single most important document in the
pipeline) does that decomposition. Parallelism is not created by spawning
agents; it is created by the decomposition, and the fleet is only ever as
parallel as the architecture lets it be (tenet 1).

A good boundary in practice: the unit owns its files outright, and its
collision surface with every sibling is engineered down to ONE trivial-merge
line (a `pub mod` + re-export in lib.rs). Workers ship the integration
*recipe* (exact wiring as a doc comment), never the integration — that
deletes the one file every parallel worker would otherwise fight over, and
moves serialization from edit time to convergence time where one mind
sequences it (tenets 3, 4).

The tell you sliced wrong: a coder asks for "a third port", or two units need
the same file beyond the trivial-merge line. Both are stop-and-asks in the
packet, so the defect surfaces as a MESSAGE, not a merge conflict. If two
units must edit the same file, redesign until they don't — we treat that as a
design bug, not a coordination task.

THE SLICE THAT LOOKS INDEPENDENT AND IS NOT — three real shapes, all PAID:
- **Emergent composition defects** (tenet 15): one unit's success creates the
  population that exposes a sibling's gap. Auto-registration of checkouts
  made many-checkouts normal; many-checkouts broke a sibling's scoped
  resolution. Both units green, both correct against their own assumptions,
  the pair wrong. NO unit-level test can see this — you must adversarially
  RUN the composed artifact on a realistic corpus. Three defects in one day
  were caught only that way.
- **The shared global toolchain** (tenet 10): every parallel unit reads one
  installed binary/toolchain. One unit upgrading it invalidates siblings
  retroactively and invisibly — they stay green and become wrong. Rule: pin
  in wave 0, no unit may change it, every done-receipt carries the version
  it observed.
- **Ambient mutable inputs** (DL-004 family): config files, provider creds,
  the shared daemon. I personally caused two machine-wide outages in one day
  by hand-editing ambient config and letting a test daemon rotate a shared
  auth key. Encoding: deterministic work SEALS its ambient inputs (isolated
  config dir, alternative ports/DBs); prod stays on released code.

## 2. How many at once, and what breaks first

Real numbers: 3–6 coders per plan under one PM; my ceiling today was three
plans genuinely concurrent plus a standalone coder plus two external fleets —
about 10–12 live seats under one o-prime, with ~16 seats total the day we
rebuilt the product from empty repo to released in a day.

The binding constraint is NOT tokens, panes, or merge conflicts (architecture
keeps conflicts near zero). It is ORCHESTRATOR ATTENTION AT CONVERGENCE:
merge ordering, review-round rulings, and message routing. The knee arrives
when review rounds fan out — every fix is fresh code needing its own diff
review, so a 13-finding round can double a plan's tail — and when rulings
queue: a blocked worker is a stalled seat, so ruling latency is the real
throughput limiter. My mitigation is structural: acks arrive as numbered
plans and I rule by number/letter in minutes, not paragraphs; pre-signal
rulings (precedence decided BEFORE the conflict exists) remove whole classes
of future decisions.

Second knee, subtler: SHARED SUBSTRATE. One database, one observation buffer,
one daemon, one toolchain. Isolation-per-worktree removed the largest pain
cluster we ever measured (~35 of 132 retro observations were shared-tree
sweeps and untrustable greens) but it does NOT remove these — see tenet 10.

## 3. The dispatch artifact

Ours matches yours closely (immutable ddoc packet, path-pointer delivery,
worktree/branch/scope/done-bar). Fixed anatomy: meta / mission /
numbered instructions / working-with (what to defer vs decide) / scope with
forbidden paths / mechanical done-bar. Two properties matter more than the
field list: the current-state section must be FALSIFIABLE in one read (a
worker that can spot a dead premise reports it instead of building on it),
and every constraint CARRIES ITS REASON — a constraint without its why gets
"optimized away" by a smart agent.

ADDED AFTER BEING BURNED, in order of blood spilled:
- **The ack-with-numbered-plan as the control point.** The worker replies
  with a numbered plan before any code; defects surface there, before the
  diff exists. This is the single highest-value line in the packet.
- **Canary discipline**: the seat must reply through pij tooling quoting
  `pij whoami` output verbatim (self-reported ids are wrong more often than
  not) plus, for omp seats, the pane FOOTER model line — spawn argv proves
  only what was REQUESTED, not what you got. Yesterday a subagent canaried me
  claiming to BE its parent PM; the seat-id-on-the-wire rule caught it.
- **Reviewer packets pin a COMMITTED SHA** reviewed in the reviewer's OWN
  worktree (DL-011) — reviewing a moving branch in someone else's tree
  produced verdicts about trees that no longer existed.
- **The line-level collision map, duplicated into every sibling packet** —
  where the coder reads, not only in the impl-guide.
- **The declined list**: crates/approaches considered and REJECTED, with the
  source that rejected them, so no coder re-litigates a settled choice.
- **Reviewer briefs name the author's least-confident areas, tell the
  reviewer to disbelieve receipts, and list known-open items.**

REMOVED AS CEREMONY: restated doctrine (packets cite the tenets file by
path — a packet that restates doctrine drifts from it), and running-
commentary status. Cards at unit edges only.

## 4. Convergence (the part you want most)

Convergence is SERIAL and owned by ONE MIND — the PM composes; coders never
integrate. Mechanics: before wiring, the PM asks each done coder what it
ASSUMED about the composition it does not own (call order, key scoping,
pre-existing state) — a live seat volunteering assumptions beats discovering
them mid-wire. Then merge unit branches in a ruled order, gate green on the
composed result, then the named integration proof, then an adversarial run
of the composed artifact (tenet 15 again — this step has caught what every
test missed, three times).

Conflicts: the INTEGRATOR resolves, consulting the author live. Authors
outrank the integrator on their unit's intent; the integrator outranks
everyone on sequence.

TWO AGENTS BOTH REASONABLE, RESULTS INCOMPATIBLE — you are right this hits
hardest, and our answer is that it is NOT a technical decision: it is a
product ruling, and it goes UP. What makes it cheap is PRE-SIGNAL RULINGS:
when I can see a semantic collision coming (two units converging on the same
envelope field), I rule precedence BEFORE either finishes ("unit A's breadth
semantics first, unit B's legitimacy vocabulary second; a genuine row-fight
comes to me"). The integrator then converges mechanically. A precedence
ruling costs one sentence before the collision and a stand-down after it.

THE CONVERGENCE TRAP WE PAID FOR TWICE IN ONE DAY: **a merge that deletes a
test deletes the evidence that it deleted something.** Two silent reverts of
already-merged work rode in on merges that took the guarding test with the
guarded behaviour — gate stayed green because a gate only runs what is
present; it cannot notice absence. Encoding: diff-stat the merge-base for
REMOVED test files before any PR. Steal this one; it is invisible until it
has already cost you a shipped regression.

## 5. Review at width

Per-plan reviewer, CROSS-MODEL, always (coders on one model family, reviewer
on another, PMs on a third) — a different model reads the diff with
different priors and it demonstrably catches what self-review does not; the
finding that saved us yesterday ("your merge reverted main's EPIPE work")
came from the cross-model seat reading the diff against the PROMISES, which
no gate does. The o-prime never reviews code — I review the process and
route findings; that is what keeps N streams from bottlenecking on me.

Stopping "correct only in isolation": three mechanisms, layered — (a) every
fix from a review round is FRESH CODE and gets re-reviewed as a DIFF ("a fix
accepted on the fixer's report defines its own scope"); (b) the FINAL round
is always WHOLE-DIFF, because fixes from separate rounds can compose into a
new defect (paid: one fix created the panic precondition for another —
invisible to both rounds, each reviewed one finding); (c) the composed
adversarial run from §4. We have also used mutation testing to reveal
vacuous passes — a test that stays green when you break the mechanism it
claims to guard is not evidence.

## 6. Where truth lives

Different truths, different substrates, each SINGLE-WRITER:
- **Code truth**: git + PR state. But NOT authorship — every seat commits
  under the human's identity, so `git log --format=%an` is a NULL signal;
  ownership lives in a roster file the o-prime alone writes.
- **Plan truth**: the plan ddoc — rows with typed state AND evidence. A row
  checked without a consumer re-executing its evidence is not checked.
- **Doctrine/rulings**: single-writer government files; rulings are files,
  not chat.
- **Liveness**: pij status cards at unit edges + an anomaly scan for stale
  cards; chasing a subordinate's stale card is the SUPERVISOR's
  accountability, not the subordinate's.
- **What an agent reads to know where it stands**: its packet + the plan
  ddoc + `harness checks` in its OWN worktree — the verdict is trustworthy
  precisely because the tree is entirely its own.

Two agents believing different things about one fact is prevented less by a
database than by two rules: ONE BRAIN PER DOMAIN (work returns to the seat
that owns the domain; the roster is the authority), and — the one we learned
this week, twice — **a ruling is closed when the BLOCKED WORKER has it, not
when the PM has it**, and its claim must be true AT EVERY SURFACE THAT
ASSERTS IT, not just where the justification lives (we caught a transcript
flatly contradicting a ruling that was fully recorded one file away; the
transcript is what a reader checks).

## 7. What actually goes wrong at width — real top three

1. **Silent reverts at merge seams** (§4). Detector: merge-base diff-stat
   for removed tests; nothing else sees it.
2. **Message crossing / stale instruction reads.** Two instructions land out
   of order and the worker reads the pair as scope creep, or builds on a
   superseded premise. Detector-and-cure: explicit supersession ("this
   replaces my message re X"), ack-by-number so misreads surface as a wrong
   number, and packets AFTER worktrees exist, never before. A subtler
   variant burned us today: shell-composed message bodies EXECUTING their
   backticks on the sender — technical prose is exactly what contains
   backticks. Use your platform's body-file form, always.
3. **Shared mutable ambient state** (§1c). My two self-inflicted outages
   were both this. Detector: honestly, the fleet screaming. Cure: seal
   ambient inputs for anything deterministic, alternative ports/DBs for all
   testing, prod touched only by an encoded restart path, never by hand.
   The general law from all three: at width, hand-operating around your own
   encoded paths is where outages come from — if the o-prime does it by
   hand, the o-prime is the incident.

Honorable mention: an agent stalling UNNOTICED — cards + anomaly scan catch
it; without them a dead seat looks identical to a thinking seat for hours.

## 8. The sequencing call

Made ONCE, at impl-guide time, by prime — because sequencing is an
architecture decision and the impl-guide is where architecture happens. The
test for "must be serial": (a) it touches a shared mutable resource that
cannot be fenced to a one-line collision surface (a schema migration, the
composition root, the toolchain version, government files); or (b) the seam
itself cannot be frozen yet — an unfrozen seam is a design conversation, and
you cannot hand a design conversation to an agent that can't attend it
(tenet 2). Anything failing either test runs as wave 0, alone, before
fan-out.

The call is only cheap BEFORE spawn. Retrofitting serialization after
fan-out costs a stand-down plus a re-dispatch, and the worker's context is
lost with it. Corollary we follow strictly: contracts + fakes + golden
fixtures ship in the contracts phase itself, so every fanned unit's "done"
is mechanical from its first minute.

## 9. Starting a government tomorrow

Day one, non-negotiable (all retrofitted painfully here):
- **Worktree-per-coder + branch-protected main + PR-only merges.** Our
  single largest measured pain cluster (~35/132 retro observations) was
  shared-tree work. Isolation is the cheapest structure you will ever buy.
- **Packet templates + canary discipline from the first spawn** — including
  proof-of-model, not request-of-model.
- **Status cards + a stale-card anomaly scan**, with chasing assigned to the
  supervisor explicitly.
- **Friction capture with SAME-DAY encoding into structure** (a template
  edit, a check, a rule). The observation that is not encoded the same day
  is re-paid by the next agent. This is the flywheel: the fleet builds the
  machine that builds the product (tenet 13).
- **Single-writer government files + an ownership roster**, because your
  VCS's author field will lie to you the moment agents share an identity.

Deliberately NOT built until it hurt (correctly, in hindsight):
- **Multi-plan concurrency.** We ran plans serially until the architecture
  and the ruling cadence proved they could interleave. Concurrency was
  earned, not scheduled — your "works well serially" is the right place to
  be standing.
- **Orchestration automation.** Rule acks by hand until the shape of the
  decisions is boring; only then encode. We hand-rolled worktree setup four
  times before building `team new`/`team tidy` around the proven shape —
  the four hand-rolls were the requirements doc. Automating a decision you
  have not made manually enough times encodes your guesses instead of your
  scars.
- **Attribution forensics, dashboards, meta-tooling** — all deferred until a
  concrete question forced each one.

The one-sentence version of all nine: freeze the seams, shrink the collision
surface to a line, hand out falsifiable packets, keep convergence serial
under one mind, review cross-model with a whole-diff final round, run the
composed artifact adversarially, and encode every scar the same day — the
parallelism itself then costs almost nothing, because you bought it at
design time.
