---
name: pij-team
description: Deterministic prime→pm→coders→reviewer delivery pipeline — scaffold a plan worktree, author plan + impl-guide, dispatch a pij PM from templated ddoc packets, fan out coders per composable-service units, compose, cross-model review. Use when farming a plan out to a team, or when asked to run the pij-team flow.
---

# pij-team — templated team delivery (PROTOTYPE, dogfooding)

We are prototyping this way of working before handing it to pij and the harness
**as first-class substrate** — that graduation is the end state; this skill is
the staging ground. The absorber is named: the harness-engineering prime
(pij-massive-meadowlark) takes this in after our initial trials and fixes, so
everything in this folder is written as its absorption spec. **Read `TENETS.md` first** — the doctrine (arch split as
the multiplier, frozen seams, mechanical done, structure over trust) that every
packet cites instead of restating. **Every friction goes in `EXPERIENCES.md`
the moment it bites** (plus `harness observe`), and every run must leave the
tenets and templates better — improving them IS the mission, not a side effect.

**Dogfood flowspace3 — every seat, every repo it indexes.** For any
meaning-shaped question ("where is X handled", "what owns Y"), run
`flowspace3 search "<question>"` BEFORE grep; orient with
`flowspace3 agents-start-here`; report every search miss or confusing envelope
as friction. This binds primes, PMs, coders, and reviewers alike — an agent
that greps its way around an indexed repo is skipping the product's best test.

**Tech-agnostic**: this skill works in any repo, any stack. Repo-specific
mechanics live in each plan's impl-guide (which instantiates the tenets in the
local stack); anything here marked "(Rust example)" / "(this repo)" is an
illustration, not a requirement.

## The shape

- **Prime** (o-prime) is the **product owner** — talks to the human, writes the
  plan and the impl-guide, has final say. Prime does not orchestrate coders.
- **PM** is deliberately dumb: it orchestrates — fans work out per the
  impl-guide, composes, gets review, closes the flow out. Small task, no
  fan-out → the PM codes it itself, then calls the reviewer.
- **Coders** each build ONE unit (composable service): written and tested in
  isolation behind a settled interface, per `/builder` implement discipline.
- **Reviewer** is cross-model, fires after composition (or solo-pm coding),
  judges plan fidelity + composition seams, read-only.
- Default models per role: `.harness/government/settings.dd.md` (o-prime edits
  via ddocs; a plan's impl-guide may override).

## The workflow

1. **Prime runs the scaffold tool** — `harness team new <slug>` (extension, see
   § Extension below): creates the worktree + branch and the next-ordinal
   `docs/plans/<ord>-<slug>/` inside it, with empty `plan.dd.json` (via
   `harness plan new`) and `impl-guide.dd.json` + the packet templates copied
   from this skill's `templates/`.
2. **Prime writes the plan** (dd-native, from the user pre-amble + what it
   knows; `/builder` planning discipline applies).
3. **Prime writes the impl-guide** — the architecture-level HOW: units,
   interfaces (settled BEFORE fan-out), waves, isolation mode, composition
   steps, fan-out decision. Template: `templates/impl-guide.dd.json`.
4. **Prime spawns the PM** on that branch (`/pij` spawn per settings defaults),
   canary-verifies, fills `packet-pm.dd.json`, delivers by path pointer,
   requires ack-with-numbered-plan, rules the ack.
5. **PM executes**: fans out coders (each dispatched with a filled
   `packet-coder-<unit>.dd.json`), composes per the impl-guide, then spawns the
   reviewer with `packet-reviewer.dd.json`. PM defers product questions to
   prime; prime defers to the human.
6. **Close-out**: findings fixed/refuted, `/builder` post-flight + archive, PR
   into main; prime coordinates merge. Buffers rescued before worktrees die.
7. **Run analysis**: after each run, the telemetry seat (currently
   pij-squealing-xoxarle) analyses the PM's and coders' actual transcripts
   (pij ledgers + native session stores) to find where packets, tenets, or
   templates were ignored, misread, or missing — findings land in
   `EXPERIENCES.md` and drive the next template iteration. Observing how the
   seats actually behave IS how this skill improves.

**Consuming repos**: a repo trialling this skill keeps its OWN experiences log
(its scratch/ or government area — never write into this repo's tree) and
brings a pointer to the compare-notes session; confirmed cross-repo findings
get folded into this folder by ITS owner (flowspace3 o-prime) with attribution.

## Templates (in `templates/`)

| file | schema (`.dd/schemas/pij-team/`) | seeded for |
|---|---|---|
| `impl-guide.dd.json` | `pij-team/impl-guide` | prime → pm: the HOW |
| `packet-pm.dd.json` | `pij-team/packet` | the PM's dispatch |
| `packet-coder.dd.json` | `pij-team/packet` | one per unit |
| `packet-reviewer.dd.json` | `pij-team/packet` | the review dispatch |

Usage: `cp` the template into the plan folder, edit via `ddocs set/add`
(never hand-edit the rendered `.dd.md`), `ddocs build`, deliver the path.

## Extension (spec — not yet built)

`harness team new <slug>`, a repo harness extension:

1. Scan `git worktree list` + main's `docs/plans/` (and each worktree's) for the
   highest `NNN-` ordinal; next = max+1.
2. `git worktree add ../fs3-<slug> -b <ord>-<slug>` from the main clone.
3. In the new worktree: `harness plan new <slug> --ordinal <ord>` →
   `docs/plans/<ord>-<slug>/` with empty plan ddocs.
4. Copy `impl-guide.dd.json` + the three packet templates from this skill's
   `templates/` into the plan folder; `ddocs build` each.
5. Print an envelope: worktree path, branch, plan folder, next_action
   ("prime: write the plan, then the impl-guide").

Until it exists, prime performs steps 1–4 by hand (that is part of the
prototype — capture what hurts).
