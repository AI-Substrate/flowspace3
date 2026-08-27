# pij-team — prototype experiences log

Living log while we prototype the prime→pm→coders→reviewer pipeline. Every
friction, surprise, or improvement idea lands here AS IT HAPPENS (newest at the
top of each section), so the templates/packets/extension can be improved before
we hand the pattern to pij and the harness. Also `harness observe` each one.

## Decisions taken while prototyping

- 2026-08-28 — dd schemas for the new doc types live in `.dd/schemas/pij-team/`
  (settings, packet, impl-guide); templates live in the skill folder and are
  `cp`'d into plan folders. One shared `packet` schema for all three roles;
  the role field + template selects the flavour.
- 2026-08-28 — settings live at `.harness/government/settings.dd.json`
  (o-prime single-writer). First settings: model defaults per role
  (pm/coder = omp github-copilot/claude-opus-5 high; reviewer = gpt-5.6-sol,
  id verified against `pij models`).
- 2026-08-28 — impl-guide default isolation = worktree-per-coder branching off
  the PM's plan branch, PM merges units back (retro 008: shared-tree was the
  #1 hurt cluster; era-2 cutover ruling). Fences partition write intent, not
  the build — waves must respect build deps.

- 2026-08-28 — TENETS.md added (Jordan: give me the core tenets incl. the
  importance of the arch split; source = scratch/reconstruct manifesto 04/07 +
  retro evidence). It is a LIVING doc: packets cite it by path, every run must
  improve it, and the graduation target is harness/pij first-class substrate.

- 2026-08-28 — Jordan ruling: the skill is TECH-AGNOSTIC (works in any repo);
  Rust/this-repo mechanics are worked examples only, instantiated per-plan in
  the impl-guide. And each run gets a telemetry analysis pass (xoxarle) over
  the seats' actual transcripts to drive template iteration.

- 2026-08-28 — Graduation path made concrete (Jordan): pij-massive-meadowlark,
  the harness-engineering prime, will absorb pij-team into the harness as
  FIRST CLASS after our initial trials + fixes complete. It is trialling the
  technique on its current bun work now; compare-notes session follows both
  runs. Everything in this folder is therefore written to be handed over:
  tenets, templates, schemas, and this log are the absorption spec.

## Frictions / open issues

- 2026-08-28 (duck, team-new POC): untracked skill/schema folders are INVISIBLE
  in fresh worktrees — ddocs build dies E401 there while passing on main
  (DL-032; fix = commit the substrate, done via PR). ddocs resolves schemas
  from CWD not the document's ancestors — scaffold tooling must keep cwd =
  worktree root (CONF-009). `git worktree remove` needs --force on a fresh
  plan worktree (untracked plan folder, DL-033). `harness plan new` writes
  meta.ordinal as number 5 while the folder says 005 — never assume string
  equality. `pij whoami` exists but was not discoverable (DL-034 amended).
- 2026-08-28 (meadowlark, bun run): ordinal-minting before human approval is a
  bad fit for investigate-mode work (F2) — scaffold extension gains a
  --propose/dry-run mode (compute + print, mint nothing until GO).

## Template improvement ideas

- (add as they arise)
