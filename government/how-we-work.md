# How we work — Jordan, o-prime, and the agent fleet

### Jordo NOtes

- We are prototyping a new way of working that we will hand off to pij and the harness when we have finished it, so as we do this we need to collect out experiences . We can update this docuemnt as we go to improve wher we run in to issues etc. 
- We need a new harness extension that creates a new worktree and then the latest ordainl plan id folder - &lt;slug&gt; in the docs/plans folder for it. It will scan all worktrees and main for the highest one and create ext. param of the slug. 
  - It then creates empty plan.dd.json etc ready for editing
  - It will create a new impl-suggestion.dd which we will iterate on now. 
- The workflow is this
  - Prime runs the tool
  - Prime writes the plan based on user pre-amble and what it knows
  - Prime writes impl-suggestion (more on this soon)
  - Prime then creates /pij pm on that branch. 
  - Pij pm will handle the implementation and fan ou hte work across coders. 
  - Once all coders are complete and the composition is done (pm will help compose) - then it will run a /pij peer coder. 
  - We need a new settings concept in government. First settings are default models - pms oh my pi (omp) github copilot accont, claude opus 5 high, coders are same, reviewers are gpt 5.6 sol. 
  - Pms are kinda dumb they just orchestarte the work, and fan it out and compose. Prime is who is the product own er and has final say. Human will mainly talk to prime. 
- New stuff we need
  - Handover packets to be done in ddocs too - and we need a template. 
    - This includes differnt ones for Pms, reviwers and coders. so then we just cp template in, edit and then use that in the pij packet. 
    - Templates will include stuff like for the pm, tell it to follow /builder flow and ensure they use nav and close itout properly (incudgin post code / shipping stages so plans get properly architeved etc). 
      - Coders and reviwers need to be using /builder prompts too. 
    - PM packet ddoc will include insturctions on how to work with prime, what work pm should be doing ad what ot defer back to prime. 
  - new implementation guide document
    - So. this is whre it gets tricky. Please review how we did hte owrk earlier wher we bsically split up across coders - each one doing a single service. we need to design our plans where coders can be working on services like this. A plan might be creating a few serices or othewise working on things that can be done in parallels. 
    - This is very architecture dependent, so need impl template ddoc which we store in govenrment that we use to cp the basic one in. It wil include hte how to do this stuff - e.g. we use composable services that we can write and test, then can compose in which owrked super well last time. 
    - I want to iterate more on this part please as i need your help to design this. 
  - The overall mission is a more determinisci way to farm out work to pms, and have them farm out to coders etc using templates and more repeatable and also more "improvable" ways. If we have templates etc then we can improev the packets, and instrutions etc to really get our system down pat. 
    - If its a small task, thent he pm can just code it itself, this will happen when there is no fan out needed. It will then call out to a reviewer. If there is fan out needed or its a multi-phase plan then it will use 1-n coders in parallel adnd use revewier at end after composition. The imp guide from prime wil include these insturctions. so the ddocs will need this section in the template (please show me what templat eou think this doc needs). 

**Author**: pij-instant-lynx (o-prime) · 2026-08-26 · recorded on Jordan's ask ("write up a detailed report on how we've been working with our agents… how I've been directing you and what to expect"). This is the operating manual for this repo's human+agent working model, written from one day of building fs3 from an empty repo to a released-candidate system with \~16 agent seats.

## 1. The shape

```
Jordan (human director — voice, fast, hates ceremony)
  └── o-prime  (one governance seat; single writer of .harness/government/**)
        ├── coder seats, one per DOMAIN, each in its own tmux window
        │     (store, config, scanner, providers, docker/CI, watcher, skills…)
        ├── occasional PM seats for full plans (e.g. s001), running their own coder+reviewer fleets
        └── platform peers outside the repo (pij prime for pij bugs, etc.)
```

One brain per domain. A seat's context is a first-class asset: we RE-OPEN the author seat for work in its domain (rulings/2026-08-26-context-single-responsibility.md) and spawn fresh only for genuinely new domains. Every seat, task, window, native session id (the revive key) and status lives in `worker-roster.md` — kept current because Jordan checks it.

## 2. How Jordan directs

- **Voice-first, conversational.** Asks arrive as spoken-style prose, sometimes mid-turn, often with a transcription typo. O-prime's job is to extract the INTENT and preserve it verbatim where it matters (rulings quote him).
- **Question vs work.** "What do you think / walk me through it / report back" = deliver an assessment and STOP — no code, no dispatch. "Get a coder on it / fire that up / thanks" = execute. When ambiguous, o-prime plays the ask back ("report what I'm asking for") before acting.
- **Numbered lists, one sentence per item** is his preferred playback format for options, sequences, and status. Short replies when he says short.
- **Decisions come one at a time.** When o-prime needs rulings, ask ONE question per message: one sentence of context, one sentence of ask. He answers fast; don't batch or essay.
- **He rules; we record.** A Jordan decision becomes either a ruling file (`.harness/government/rulings/`) quoting his intent, or a doctrine section in the relevant design doc, or a PRD requirement — the same day, before it can drift. Reversals are recorded as reversals (e.g. primes-owe-status-cards).
- **Some things only Jordan does**: merging release PRs ("we wait for the PR leader"), deleting other projects' resources, admin toggles, teardown of ambiguous processes. When in doubt whether an action is his, it is.

## 3. What o-prime does with an ask

Classify, then route:

1. **Question** → investigate (read the tree, run the CLI, check the DB), answer with evidence, stop. The deliverable is the assessment.
2. **Small bounded work in an existing domain** → write a PACKET and send it to the domain's author seat (often as an extension to their open unit).
3. **New domain** → write a BRIEF, spawn a fresh seat (canary-verify it), add it to the roster, give it its own window.
4. **A big rock** → a dd-native PLAN (builder skill), usually preceded by one or more WORKSHOPS where the design decisions get made and become authoritative documents. Plans get validated (/validate-v2), executed by a PM or a single strong coder, reviewed by o-prime + critic, and closed with proof (the "first light" pattern: a live end-to-end run transcript as the plan's exit evidence).
5. **Platform bug** (pij/omp itself) → evidence to the platform prime (ermine), never fixed locally, never worked around silently.

## 4. Briefs, packets, dossiers

- **Brief** (`briefs/w-<name>.md`): opens a seat on a job. Structure that has worked: verbatim Jordan intent up top · "The job" as numbered units · what to read FIRST (doctrine, prior art, LEARNINGS) · what is explicitly DEFERRED (do-not-build list) · Rules & fence (exact paths the seat may touch; whose in-flight files are hands-off) · report-back contract (claim · shas · transcript · service page) · "Deviations = stop-and-ask".
- **Packet**: the lighter-weight follow-on — a pij message extending an open seat's unit with the same elements compressed. Anything substantive is PERSISTED TO A FILE first and the message carries the path (pointer delivery — this also survives the message-truncation bug).
- **Dossier**: o-prime's own handover documents (e.g. `scratch/oprime-handover.md` before a context compaction) — everything a successor needs: board state, active threads, queued work, Jordan's local state, immediate next actions.
- **Workers ack before coding**: plan-of-attack in a few lines, o-prime approves or corrects, THEN code. Mid-work, real design discoveries come back as **stop-and-asks** — the worker states what it measured, the proposed fix, alternatives it rejected and why, and does NOT act if the fix reverses an o-prime ruling. (Live example: the watcher forever-rescan defect — measured on the real binary, fix reversed a ruling, worker held with the fix written and tested until GO.)

## 5. The verification culture

- **Verify-then-relay.** O-prime never forwards a worker claim unverified: check the sha exists, run the command, read the table. "Done" is a claim until verified — the platform enforces this too (assignment verify stamps, anomaly rows for unverified-dones).
- **Evidence-gated PRD.** The requirements register (`docs/plans/prd/base-prd.dd.json`, mutated ONLY via the ddocs CLI) has a state + note per requirement; a check-off requires named evidence (sha, transcript, live-run pointer) in the note. Swept at every acceptance.
- **Mutation-checked fix tests.** A fix's test must FAIL without the fix. Workers state this; reviews check it.
- **Reviews.** O-prime (plus an independent critic for big landings) reviews plan exits; findings are severity-ranked with smallest-fixes, and fixes land before plan close.
- **Identity is verified too.** New seats get a canary before trust; when the platform's identity wobbled (alias rotation), work continued only through the canary-proven canonical id.

## 6. Communication mechanics

- **pij sends**, wire-disciplined: short, pointer-heavy, single-quoted (backticks in a double-quoted send get executed by zsh — learned the hard way).
- **Status cards** at unit edges (`pij report now "<did>" "<next>"`) — primes owe them too.
- **Mid-turn steering**: Jordan and workers can interject while o-prime is working; messages are addressed within the running turn.
- **Known platform quirks** are documented and routed, not suffered: head-truncation of omp→busy-claude messages (workaround: pointer delivery), send-path alias rotation (workaround: always address the canonical id; canary on doubt).

## 7. Documentation trail (who writes what)

- **Workers** write `docs/services/<their-domain>.md` — living pages, updated as their domain evolves; and LEARNINGS.md for prototypes (which become doctrine).
- **O-prime** writes rulings, briefs, reviews, the roster, workshop docs, and keeps the PRD register current.
- **Skills** (`.agents/skills/`) capture repeatable recipes (add-provider, add-language, flowspace) so parallel workers execute them without re-derivation; they grow with each use.
- **Retro drains**: worker friction observations are captured via `harness observe` and drained by o-prime (o-prime-owned; workers list-and-report, never clear) into retro records; the best become encoded improvements.

## 8. Shared-tree discipline (pre-cutover era)

All seats committed directly to main: conventional commits binding · file-scoped adds, never `git add -A` · hunk-audit before commit · push-first (never rebase over a sibling's unstaged work) · stage only at the moment of commit (parked staged changes get swept into siblings' commits — happened twice) · `.claude/` and secrets never committed · atomic cutovers in single pushes. Incidents were unswept surgically and each one amended the ruling.

## 9. The PR era (CUTOVER EXECUTED 2026-08-27)

`rulings/2026-08-26-pr-workflow-cutover.md` is now in force: main is branch-protected, CI runs on `pull_request` only (69d06ca), and the fleet works **worktree-per-coder + PRs** — same briefing discipline, but each coder gets its own worktree + branch for its packet, works there, gates locally (`harness checks` now gives a trustworthy verdict — the tree is all yours), and opens a PR when the done-bar is met. Conventional commits stay binding (release-please reads the merge history). O-prime coordinates review, merge order, and releases; worktrees are tidied at packet end. Section 8's shared-tree discipline is HISTORICAL — the swept-stage incident class died with the shared tree — but its non-tree rules survive (secrets/`.claude/` never committed, file-scoped adds remain good hygiene).

## 9b. Orchestrating pij coder seats — the exact mechanics (o-prime's core craft)

The lifecycle every packet follows; skipping a step has bitten us every time it was tried.

1. **Spawn** (Jordan's chosen shape): `pij spawn --harness pi --bin omp --model github-copilot/claude-opus-5 --effort high --task "<packet>: <one line>"` — then IMMEDIATELY `tmux break-pane -s %<pane> -n <packet-name>` (own window, named for the WORK). Jordan ruled 2026-08-27 to SPREAD work: new domains get NEW seats; re-open a seat only for work inside its established domain (context-single-responsibility).
2. **Canary before anything**: the ready-ping arrives as a pij message with `{spawnId, model, cwd}`; reply demanding id/spawnId/model/cwd/CANARY-OK back, verify every field. The pij#19 phantom-alias defect mints extra registry ids off one process — address ONLY the canaried id, never message or close an alias, and treat post-close alias tombstones as noise.
2b. **Record the seat's pij GENERATION at canary time** (2026-09-02): `pij sessions --json` = legacy (ingesting), `pij-rs list` = rs (NOT ingesting). `pij spawn` is legacy-routed and normally fine; an omp-extension boot or a `pij adopt` lands in rs, and pi/omp — our standing spawn shape — has no env fallback, so an rs seat's entire conversation is lost to the index with no error until pij req-0033. See `government/pij-two-daemons.md`.
3. **Roster** the seat (worker-roster.md, o-prime single-writer) — the ownership authority, since every seat commits under Jordan's git identity.
4. **Dispatch = brief file + pointer message.** Brief anatomy: what Jordan RULED (dated) · current state written to be FALSIFIABLE in one read · constraints WITH reasons · numbered deliverables · PR-era done-bar · out of scope. End the dispatch with "ack with your read before coding." EVERY brief also carries a pointer to `government/pij-two-daemons.md` — pij now runs two daemons routing by verb, and a seat that does not know it will read `pij whoami`'s "no seat in this store" as its own death (2026-09-02; four of the ask--path coder's observations, two BLOCKING, were this).
5. **The ack is the control point.** Strong seats correct the brief at ack time with evidence (source-reads, schema-reads); rule on each point by number, fast. A brief correction backed by evidence outranks the brief.
6. **Edges + evidence**: workers report at work edges and stop-and-ask on surprises; "gate green" claims require BOTH the check-suite existing for the head sha AND the verdict (a never-fired CI run reads as "pending" forever). One watcher per CI run, 60s interval, in a background task — never foreground sleep chains.
7. **Merge is o-prime's**: read the diff against the brief (green is necessary, merged is a decision), squash-merge, verify main, then have the seat tidy its worktree and stand down adopted-idle with successor context recorded in its roster row. Governance edits (roster/PRD/rulings) ride their own oprime/\* branch PRs.
8. **Wire discipline**: pointer delivery for anything long; single-quote message sends (backticks and $() in double quotes are EATEN by the shell — it has damaged commits on main); `pij report now` at your own edges; `harness observe` every friction the moment it bites; the observe drain is o-prime-owned, always.

## 10. What a new agent should expect, in one paragraph

You'll get a brief with a fence and a report-back contract; read the named doctrine first; ack with a short plan of attack and wait for the go; work only inside your fence; commit and push as you go with conventional subjects; verify your own claims before reporting them (shas, runs, transcripts); stop-and-ask the moment reality contradicts your brief or a ruling; write the living service page for what you built; report claim+evidence at the end and expect o-prime to check it before anything is relayed upward. Jordan names the work; o-prime shapes and routes it; you own your domain's truth.