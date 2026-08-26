---
name: builder
description: |
  The single front door to the SDD pipeline (docs/plans/): research → plan → workshop → ADR → tasks → implement → review → ship. Use when the user wants to plan, research, specify/architect (one plan doc: business spec + implementation plan), clarify, workshop, write an ADR, break work into phase tasks, implement/build a phase, update progress, code-review, ship a plan, or start/resume/adopt a plan flow. Guided mode coaches from durable on-disk state; direct jump runs one stage: /builder <id> <verb> [flags] — 1a explore, 1b plan, 2c workshop, 3a adr, 5 tasks, 6 implement, 6a progress, 7 review, 7b post-flight (close-out + archive), 8 ship, 8c reconcile. Ids and verbs resolve alone; printed commands carry both.
---

# /builder — SDD pipeline dispatch

**v2**

One public skill for the whole SDD pipeline, built to the flow-architecture pattern (`docs/skills-pipeline/flow-architecture.md`): the **sub-skills** (contract-bound verbs, one per stage) live in [`references/stages/`](./references/stages/), the guided-mode engine in [`references/00-routing.md`](./references/00-routing.md), the coaching voice in [`references/coach.md`](./references/coach.md). New to the flow? [`references/getting-started.md`](./references/getting-started.md).

**Progressive disclosure is the contract: load exactly one sub-skill for the current step — never read all of them up front.**

## Two load paths

**Guided** — `/the-flow` (no args, or `<slug>` / `<ord>-<slug>`):

1. Read `references/00-routing.md` (entry paths, state contract, the Graph) **and** `references/coach.md` (rail, narration, print-then-offer).
2. Resolve fresh / resume / adopt per 00-routing.md; load **only** the current stage's sub-skill when a step is accepted, and coach the seam.
3. Guided mode owns all the-flow state, and drives **all** of it — position *and* session bag — through `harness flow nav` calls: the flight plan (`the-flow.json` → `the-flow.md`) is the single state substrate (plan 024 — the CLI is the generator; **nothing is hand-written**; run the capability precheck first, § Prerequisite). **Before the first flight-plan mutation of a session, also load [`references/flight-plan-ops.md`](./references/flight-plan-ops.md)** — the nav model, the spine-vs-excursion rule, and the verb flags + gotchas (loaded on demand, not up front; sub-skills never load it).

**Direct jump** — `/builder <id> <verb> [flags]`:

1. Resolve the stage via the Registry below. Id and verb each resolve alone (typing `6` ≡ typing `implement`); when both are given they must name the same stage — if they disagree, show the Registry and ask which was meant (never guess).
2. Read **only** that sub-skill and follow it with the given flags (same flags the verb has always taken).
3. No coach, no rail, no state writes — and **no harness seams** (harness orchestration is the guided engine's job — direct-jump runs the bare verb, harness-less by design). Artifacts land where they always did; the next guided run discovers them by existence and catches state up.

A sub-skill may lazily pull `references/00-routing.md` § Shared conventions when it cites it — that is still progressive disclosure (the pattern's sanctioned exception 1). Reading sub-skills for stages you are not executing is not.

## Registry

**This table is the master** (the Graph master is 00-routing.md § Graph; `references/getting-started.md` is a rendered view). It assigns the flow's ids, binds verbs to modules, and states each contract.

| id | verb | module | consumes → produces | flags |
|---|---|---|---|---|
| 1a | explore | `references/stages/10-explore.md` | intent → `research-dossier.md` | `"<intent>"` |
| 1b | plan | `references/stages/20-plan.md` | intent, dossier?, workshops?, coverage? → `<slug>-plan.md` (one doc: business spec + impl plan, **always both** in one atomic pass; gates G1–G7; auto-runs `/validate-v2`; § Re-entry for mid-plan clarifications) | `"<intent>"` `[--simple]` `[--skip-clarify]` |
| 2c | workshop | `references/stages/25-workshop.md` | plan/spec?, topic → `workshops/*.md` (authoritative decisions) | `<plan> "<topic>"` `[--from-spec]` `[--list]` |
| 3a | adr | `references/stages/35-adr.md` | plan/spec context → `docs/adr/*.md` | `"<decision>"` |
| 5 | tasks | `references/stages/50-phase-tasks.md` | plan → `tasks/<phase>/tasks.md` + context brief | `--phase "<Phase N: Title>" --plan "<path>"` |
| 6 | implement | `references/stages/60-implement.md` | plan, tasks? → code + `execution.log.md` (exactly one phase) | `--plan "<path>"` `[--phase "<Phase N: Title>"]` `[--subtask "<ORD-slug>"]` |
| 6a | progress | `references/stages/62-progress.md` | task outcome → updated task table + execution log (read by the implement verb after each task) | `--plan --phase --task --status` |
| 7 | review | `references/stages/70-review.md` | plan, code → `assets/reviews/*.md` | `--plan "<path>"` `[--phase "<Phase N: Title>"]` |
| 7b | post-flight | `references/stages/75-post-flight.md` | all phases + reviews done (pre-ship) → `assets/post-flight.md` close-out note + the whole plan folder archived to `docs/plans/archive/<ord>-<slug>/` (`git mv`, layout intact); the terminal harness close-out fires here (engine-owned seam) — runs whether or not the work ever ships | `--plan "<path>"` |
| 8 | ship | `references/stages/80-ship.md` | archived plan (7b moved it — resolve `--plan` under `docs/plans/archive/`), review → pushed branch + PR (repo-guidance-aware) + watched CI checks; push & PR-open **each behind a confirm**, merge optional; **flushes telemetry (no confirm)** | `--plan "<path>"` `[--base "<branch>"]` `[--no-watch]` `[--draft]` |
| 8c | reconcile | `references/stages/80-merge.md` | (conditional excursion — divergent base) → reconcile/merge plan; **merge executes only on typed `PROCEED`** | `--plan "<path>"` `[--target "<branch>"]` |
| sync | sync | `references/00-routing.md` | flight plan + plan artifacts → **reconciled** flight plan: backfills every past/present/future phase + workshop + harness seam-node that current knowledge implies; **idempotent** (a complete spine writes nothing), advisory, CLI-only, never advances `nav` | (none) — auto-fired every guided entry; also invokable on demand |

Module missing at its path → say so and stop. Never improvise a stage from memory.

> **`8 ship` is the terminal spine stage; `7b post-flight` sits between the last review and ship** — the close-out that runs even when nothing ships (harvest + archive; shipping is optional and later). **`8c reconcile` is a conditional excursion** (fired only when the base has meaningfully diverged), never on the spine. Typed `merge` and the legacy `plan-8-v2-merge` resolve to `8c reconcile` (alias table below).
> **`sync` is a maintenance verb, not a journey stage** — it has no ordinal id, produces no stage artifact, and never moves the cursor or runs a stage. It is the engine's **every-entry spine-reconcile pass** (the routine lives in `references/00-routing.md` § Reconcile the spine), exposed as a verb so anyone can run it on demand (and so a direct-jump-built or hand-adopted plan can be repaired in one call). Distinct from `8c reconcile` (which merges a divergent git base — unrelated). See invariant #11.

## Command grammar

Printed commands are always `/builder <id> <verb> [flags]`, e.g. `/builder 6 implement --plan "<path>"`.
Id or verb each resolve alone; printed form always carries both, never a bare number; mismatched pair → show the Registry and ask. This section is the grammar's **single definition** — every command surface anywhere else (narration, state files, views) is rendered from it plus a Registry row, never hand-written.
The maintenance verb `sync` resolves alone (its id and verb are the same token): `/builder sync` runs the spine-reconcile pass on demand. It takes no stage flags; it never advances the journey.

## Old-slug translation & aliases (read-time)

Docs and **legacy** state files written before the consolidation may carry commands naming retired skill slugs (e.g. a `pending_command` in a leftover `.the-flow-state.json`, read only during the one-shot resume backfill — § State). Translate at read time — never execute a retired slug; the live source of a pending command is now `nav.next` + the Command grammar, rendered at read time (never stored). Flags carry over unchanged. **Targets are stored in id+flag form** (never as full command strings) and rendered through the Command grammar + Registry when printed or written into state.

| retired slug / typed alias | → target (id + flags) |
|---|---|
| `plan-1a-v2-explore` | `1a explore` |
| `plan-1b-v3-specify-and-clarify` | `1b plan` |
| `plan-1b-v2-specify` | `1b plan` |
| `plan-2-v2-clarify` | `1b plan` (module § Re-entry) |
| `plan-2c-v2-workshop` | `2c workshop` |
| `plan-3-v3-architect` | `1b plan` |
| `plan-3-v2-architect` | `1b plan` |
| typed `specify` | `1b plan` |
| typed `architect` or id `3` | `1b plan` |
| `plan-3a-v2-adr` | `3a adr` |
| `plan-5-v2-phase-tasks-and-brief` | `5 tasks` |
| `plan-6-v2-implement-phase` | `6 implement` |
| `plan-6-v2-implement-phase-companion` | `6 implement` *(companion mode retired — runs plain)* |
| `plan-6a-v2-update-progress` | `6a progress` |
| `plan-7-v2-code-review` | `7 review` |
| `plan-8-v2-merge` | `8c reconcile` |
| typed `merge` | `8c reconcile` |
| typed `6c` or `companion` | `6 implement` *(companion mode retired — runs plain)* |
| typed `archive` or `close-out` | `7b post-flight` |

**Unmapped slug → print the bare stage alias and ask — never guess.** (An unrecognised `/plan-*` command: show the Registry's ids/verbs and ask which stage was meant.)

## Hard invariants (every stage, both load paths)

1. **Print first, then offer to run.** Print the exact command in a copyable block — rendered via § Command grammar (id **and** verb, never a bare number), so the reader sees what it will do without knowing the ids — then offer to run it; one accepted step per turn (guided). **One exception, by design:** the harness *router call* (`/eng-harness-flow --hook … --json`) **auto-fires** at each seam — it's read-only/advisory and positional, so it can't be forgotten as context grows or compacts; only the *action it routes to* follows print-then-offer (call-only depth — `references/harness-seams.md` § How the engine presents a seam).
2. **Nothing irreversible without explicit confirmation.** Outward-facing actions each gate: the **ship** verb's push and PR-open are **separate** confirms (a "yes" to push is not a "yes" to open a PR); the **reconcile** merge (8c) and any immediate merge execute **only** after the user types `PROCEED` — never on a generic "yes". *(One deliberate exception: **ship flushes telemetry without a confirm** via `harness telemetry sync` — a counts-only, out-of-tree push to `refs/harness-telemetry/*` that publishes no work, touches no branch/PR, and is reversible, so it is not an outward-facing gate.)*
3. **Never run `/compact`** — it is a user-typed CLI built-in. Recommend: "type `/compact` yourself, then re-run `/the-flow`".
4. **Never gate, score, or block the human.** No thresholds, no compliance floors; the user may decline any offered action — workshops, the backpressure survey, compaction. **Scope note: human-declinable is never agent-skippable.** Harness chores/seams are **mandatory for the agent**: the `/eng-harness-flow --hook … --json` call **auto-fires positionally at every seam** (per #1/#9), and the routed action runs only on the **human's explicit acceptance** — silence is neither acceptance nor decline. A decline is **two real CLI calls, ordered**: `harness flow comment --path <flow-path> --node <chore-id> --kind decision --source user --text "<verbatim words>"`, then `harness flow status --path <flow-path> --node <chore-id> --to skipped` — never the agent's own judgment. A missing router or an `UNAVAILABLE`/`noop` envelope resolves a chore as a **completed, receipted attempt** (`done`), never a skip and never a permanent blocker. No wording anywhere ("advisory", "recommended", `importance`) is agent licence to skip (the doctrine block, `references/harness-seams.md`).
5. **Never fabricate an insight.** Ground every narrated detail in a real artifact; if you can't read it, say so and fall back to file existence / git status.
6. **Never hand-edit the flight plan, and never hand-author a state file.** `the-flow.md` is always regenerated from `the-flow.json` by `harness flow render`; `the-flow.json` itself is mutated **only** through `harness flow` calls (plan 024 — the CLI is the generator). There is **no `.the-flow-state.json`** — all the-flow state (position + session `bag`) lives in `the-flow.json`, and the CLI is its only writer. Guided mode requires a capable CLI (§ Prerequisite).
7. **Agent bookkeeping into the flight plan awaits the v2 `harness flow agent` verb** — until it lands, `agents[]` stays unpopulated and is never hand-edited (per invariant #6).
8. **No time estimates anywhere** — Complexity Score (CS 1–5) only (`references/00-routing.md` § Shared conventions).
9. **Harness = one door, auto-fired at each seam.** Every harness touchpoint is `/eng-harness-flow --hook …` (permanent `--event` alias) — never name or invoke its child skills. The router *call* **auto-fires** mechanically from the durable `nav.now` position at each seam (so it survives long/compacted context); the routed *action* stays print-then-offer (call-only). **Only the real router invocation satisfies a seam** — prior or manual evidence (e.g. a bare `harness boot`) never does. Harness-seam orchestration is **flow-owned** (`references/harness-seams.md`); sub-skills are **orchestration-blind** — they carry no seam/routing/lifecycle-hook knowledge, but a verb may still `harness observe` a friction it hits the moment it bites (with an execution-log note as the fallback when harness-less, e.g. direct-jump), since capturing friction is not orchestration (invariant #14).
10. **Every stage is a deep-think task** — reason as thoroughly as the stage warrants.
11. **Keep the spine complete — reconcile every guided entry.** On **every** guided entry (resume *and* adopt), before narrating, the engine runs the **spine-reconcile pass** (`references/00-routing.md` § Reconcile the spine, exposed as the `sync` verb): it diffs the plan's full phase/workshop roster + the harness seam-node set against `the-flow.json` and **backfills whatever current knowledge implies but the flight plan is missing** — all past/present/future phases, every workshop, and — **unconditionally under D1** — the per-phase harness seam nodes (provisioning affects only whether they're *run*, never whether they exist). It is **idempotent** (a complete spine writes nothing), **advisory in what it FINDS**, **CLI-only** (invariant #6), and **never gates, never advances `nav`, never runs a stage**. The **mechanical enforcer** is the Tier-1 run-now step in `references/00-routing.md` § CLI-driven cadence (step 3, sibling to `render`) — **not** prose memory: it rides the positional "a long or compacted session cannot skip it" guarantee, so **RUN is mandatory + mechanical** (render-class) while what it **FINDS** stays advisory/non-gating. The user must never have to ask "make sure all phases/chores are represented" — that omission is the bug this invariant exists to kill. Direct-jump does **not** auto-reconcile (harness-less by design); the `sync` verb runs the pass on demand.
12. **Orient every turn — read `nav`, run `orient`, re-ground on the spine.** Before acting on any guided turn, read the durable position (`harness flow nav show`) and run **`harness flow orient`** — it prints the rail + the `nav.now` node's `label`/`command`/**authored `instructions[]`** + the chores due here. This is **positional and mechanical** — a **Tier-1 cadence step, a sibling to `render` and the spine-reconcile (#11)** — run unconditionally every entry, **never an offered beat**, riding the same *"a long or compacted session cannot skip it"* guarantee as the seam auto-fire (#9). It is the fix for "cheap models don't follow the flow": "what do I do next" becomes a **read, not an inference**, so even a weak model (or a freshly-`/compact`-ed context) re-grounds on the spine every turn instead of drifting off it — the node's `instructions[]` are re-read each turn, never remembered. The orient **read** is mandatory + mechanical; what it surfaces gates nothing for the **human** (#4) — but for the **agent** each listed due chore is work to resolve, not information to note. Two mechanical consequences of the read: **(a)** a non-empty `due_chores` makes loading `references/harness-seams.md` **mandatory before any further flow mutation** (the trigger is the CLI read, never an "am I at a harness edge?" judgment); **(b)** never `nav set --now` off a node until its `due_chores` is empty **and** — since terminal nodes leave that read — a read-only inspection of the node's anchored chores in `the-flow.json` shows every terminal chore carrying its receipt comment: resolve each chore by a real attempt (`done` + receipt — a missing/unavailable router is itself a receipted attempt) or the human's two-call decline (receipt-first/status-second; exact commands in #4 and the doctrine block). *(Guaranteed enforcement on an adversarially-weak model ultimately needs a harness-side per-turn hook — the skill can only instruct; that bigger ask is noted, not blocking.)*
13. **Tokens are expensive — spend them where they change the outcome.** Every token costs: your thinking, tool calls, code, and emitted artifacts alike. Do the work the stage actually needs — build the plan, write the code, run the checks; **never under-build to save tokens** (that trades a cheap token now for an expensive re-do later). But past that floor, be **direct**: shortest path to the real result, no restating what a link carries, no narration a glance already gives. This is the whole-operation form of the artifact rule (`references/00-routing.md` § Artifact Elegance, the seven-function line test) and the coach's lean-narration doctrine (`references/coach.md`) — one posture, applied everywhere. When a line, a tool call, or a paragraph doesn't change a decision or move the work, cut it.
14. **Don't apologise — fix.** The environment is a target of work, not an obstacle to apologise for and route around — and "us" is agents and humans together, each inheriting what the last left behind. Discriminate by cause: a mistake **you** made, fix silently and move on; a **hard wall or a proof-gap** the environment imposes (a missing command, a check that can't run, a misleading error) — fix it if it's small and reversible, otherwise capture it the moment it bites with `harness observe "<what>" --kind difficulty|confusion`; a **papercut** you can route around, note it on the *second* strike, not the first (no MUST-FIX neurosis — the requested work still ships). Every difficulty paid forward — through `harness observe` or the phase-end drain that `/eng-harness-flow` runs — is one the next agent or teammate never re-hits. This is the standing second objective at every seam; the layers below cite it, they don't restate it.

## State

Durable state **is** the flight plan — `nav` (position + the free-form `bag`) + node statuses in `docs/plans/<ord>-<slug>/the-flow.json` (rendered to `the-flow.md`; after `7b post-flight` archives the plan, the same files under `docs/plans/archive/<ord>-<slug>/`). **No separate state file** — the CLI is the only state writer. The plan folder keeps only `<slug>-plan.md`, `the-flow.json`, `the-flow.md`, and `original-ask.md` at its root; every other stage artifact lives under `assets/` (`references/00-routing.md` § Plan-folder layout). Contract, write ownership, and the Graph: `references/00-routing.md`; harness-seam orchestration (detection, seam map, node emission, upstream contract): `references/harness-seams.md`. Sub-skills own their *stage* artifacts (spec / plan / tasks / execution log / reviews), never write the-flow state, and carry no harness knowledge.

## Prerequisite — a capable `harness flow` CLI (capability + version floor)

Guided mode drives the flight plan (`the-flow.json` → `the-flow.md`) **exclusively** through the `harness flow` verb family (plan 024). It is a hard runtime dependency:

- **Capability precheck — run once per guided session, before the first flight-plan mutation.** Probe `harness flow --help` (and, for a version floor, `harness --version`). If `harness` is missing, or the `flow` verb family is absent (an older CLI), or its surface is too old to carry `create`/`insert-node`/`nav`/`rail`/`render` → **error-and-stop** with the honest hedge: *"the-flow needs a capable `harness flow` CLI (plan 024). Run `harness update` (or `npm i -g @ai-substrate/engineering-harness`), then re-run `/the-flow`."* Do **not** fall back to hand-cranking the JSON — the CLI is the only writer.
- **No adoption required.** This is *not* the engineering-harness loop. the-flow's flight plans live in `docs/plans/<ord>-<slug>/the-flow.json` and need **no** `.harness/` setup, no governance doc, no harness adoption — only the global CLI on `$PATH` (`harness` is an ambient tool like `git`). This skill remains the single **source** of the flight-plan schema (`references/flight-plan.schema.json`); the CLI carries an allowlisted **generated copy** (bundled by `scripts/gen-flows.mjs`, plan 081, superseding plan-024 grill 6/7) that `check:flows` guards against drift, so bare `harness flow create flight-plan` works even without this skill installed — and an explicit `--schema` still wins when passed. Nothing is installed into the consuming repo either way.
- **Clean break (`E308`).** Pre-024 hand-cranked flows (a `the-flow.json` with no `provenance` block) are **not** migrated — the CLI returns `E308` (legacy-format) on read. That is an honest stop, not a bug: re-create with `harness flow create flight-plan --schema <skill base>/references/flight-plan.schema.json --agent the-flow` (`<skill base>` = this skill's dir, e.g. `~/.claude/skills/builder`; the `--agent the-flow` titles the rebuilt rail `[the-flow]`; any prior `.md` stays as a static record).
- **Version skew is a runtime-dependency gap, not an auto-fallback.**
  - *Forward skew* (skill needs a newer CLI than installed) → the capability precheck above stops with "run `harness update`".
  - *Reverse skew* (an **old** the-flow that still hand-cranks the JSON, run against a **new** CLI) → the old hand-crank's write produces a `the-flow.json` the new CLI then reads as `E308` (no `provenance`) — a clean stop, **not** silent divergence. The fix is to update the skill, never to special-case it.
- **Deploy order — CLI first, then skill.** Always land a capable `harness flow` (publish / `harness update`) **before** deploying this migrated skill, so the precheck passes the moment the skill goes live. Rollback is additive/reversible: revert the skill + `harness update --pin <prev>` (the CLI is a global npm package).
