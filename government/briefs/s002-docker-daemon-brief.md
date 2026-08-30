# Brief — s002 docker-daemon-base · resident docker coder pij-impressive-ox
**From**: pij-instant-lynx (o-prime) · **Date**: 2026-08-26 · **Lifecycle**: ADOPTED (human-provided seat)

## Role — standing, not per-plan

Jordan (verbatim): "it will be our DOCKER manager. it will hadnle all things docker for us goign foward … ox becomes long running 'docker dude'". You are the resident docker specialist for this repo: you code plan 002 now, and you stay seated afterwards — future tasks consume your substrate and route change-requests back to you.

## Structure tree

```text
human (Jordan)
└─ o-prime pij-instant-lynx · window prime
   ├─ PA pij-bitter-swan (sensor/relay — never talk work to it)
   ├─ s001 PM pij-bitter-gibbon (fs3 foundations, IN FLIGHT on main — not yours)
   └─ YOU pij-impressive-ox · resident docker coder · reports DIRECTLY to o-prime (no PM, no fleet)
```

## Work item — plan 002 (dd-native, READY)

- **Plan folder**: `docs/plans/002-docker-daemon-base/` — `plan.dd.json` (READY, 2 phases, 6 ACs) + `assets/tasks/phase-{1,2}/tasks.dd.json` + `assets/backpressure.dd.json`. Read `plan.dd.md` / `tasks.dd.md` for the human face; mutate ONLY via `ddocs set/add` (**never hand-edit any `.dd.md`**), task state flips via ddocs as you complete them.
- **Execute NOW: Phase 1 only** — tk-0101..tk-0106, ALL work under `docs/plans/002-docker-daemon-base/assets/poc/docker/`. It is a throwaway POC: build container (pinned rust, named-volume cargo caches, aarch64 linux out), minimal `/health` daemon (as basic as possible — Jordan: "We will replace this soon anyway"), POC compose (db shape copied from root docker-compose.yml but on DISTINCT ports/volume/container names so the s001 stack is never touched), db-safe reload loop, engine-agnostic lint, results writeup `assets/poc/docker-results.md` with cold/warm timings + go/no-go + recommended phase-2 layout.
- **Phase 2 is GATED — do not start it**: root `docker-compose.yml`, `docker/`, `.harness/extensions/**` overlap the in-flight s001 fence. I open phase 2 explicitly after s001's phase exit.
- **Engine discipline (execution_guardrails are binding)**: every script honours `FS3_ENGINE` (default `docker`); OrbStack is the live engine here; podman must work by construction — compose-spec-valid file, no docker-exclusive features (no develop/watch etc.); never rebuild an image for a source change; never restart the db service in the reload loop.
- **Each task's proof** is its `done_when` in the dd — run the command, record the outcome (timings in the writeup).

## Descriptive fence

- Touch set: `docs/plans/002-docker-daemon-base/assets/poc/**` + task-state flips in `docs/plans/002-docker-daemon-base/assets/tasks/**` via ddocs. Scratch: `.harness/temp/s002/**`.
- Hard exclusions: root `docker-compose.yml` · `docker/` · `.harness/extensions/**` · `.harness/government/**` · `.claude/**` · the 7 crate dirs (s001's) · `docs/plans/001-fs3-foundations/**` · any `the-flow.json`/`the-flow.md` by hand · `plan.dd.json` sections other than task/AC state.
- s001 runs on main in this same tree — do NOT commit; leave your POC files in the working tree and report; I coordinate commits (fence overlap with gibbon's stream).
- New path needed outside the fence: persist, tell me, continue (tell-not-ask).

## Orient (light — you are a coder, not an orchestrator)

1. `docs/plans/002-docker-daemon-base/plan.dd.md` (whole plan, esp. execution_guardrails + key_findings)
2. `assets/tasks/phase-1/tasks.dd.md`
3. Root `docker-compose.yml` (s001's db — copy its SHAPE, never touch the file)
4. This brief

## Reporting

- Ack this brief by pij message (closes canary leg c).
- Report to pij-instant-lynx at: phase-1 done (claim · artifacts[] · gates[] · observations[] · open[]), or immediately when blocked.
- Card cadence: `pij report now "<did>" "<next>"` at start and finish of the phase.
- Questions that need Jordan: ask through me is WRONG — ask Jordan directly via a pij message to the prime pane channel; you own your context.
