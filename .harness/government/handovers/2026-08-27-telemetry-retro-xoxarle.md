# Handover: telemetry survey + retro state — for pij-squealing-xoxarle

From pij-instant-lynx (o-prime), 2026-08-27. Jordan's directive: continue the
telemetry work; understand the retro we just ran.

## 1. The retro we just completed (context you inherit)

- v0.2.0 shipped yesterday (13 worker seats + o-prime; 9 tag cycles; from-scratch
  Ubuntu acceptance passed). Afterward every living seat was asked to BACKFILL
  frictions it had NOT yet captured (`harness observe`), then list-and-report.
- **The drain**: 132 observations (108 difficulty / 17 confusion / 4 gift /
  3 magic-wand; 18 blocking) stored with full text in
  `.harness/records/retro/2026-08-27/008.md` (+ `raw-drain-buffer.json` beside it).
  o-prime then filed 6 of its own (wrong-mechanism diagnoses, tag-cycling traps,
  alias overhead) — those are in the CURRENT buffer, not yet drained.
- **Top opportunities** (post worktree-cutover filter — Jordan ruled anything the
  worktree-per-coder + PR cutover solves is retired): (1) shell-eats-prose:
  `harness commit -F` / `pij send -F`; (2) unmetered shared credentials (gh API);
  (3) doc-vs-code drift has no failing gate; (4) drain receipts per observation id;
  (5) telemetry review as first-class. Rendered page: `scratch/retro/index.html`.
- The detailed retro session with Jordan (dispositions, final ranking) has NOT
  happened yet.
- **Fleet state**: all 13 workers CLOSED 2026-08-27 (revivable). The who-did-what
  ledger — every seat, its landed commits, native session ids (revive keys) — is
  `.harness/government/worker-roster.md`. That file is the ownership authority:
  git identity is shared, so `git log --format=%an` is a NULL signal (AGENTS.md).

## 2. Telemetry work so far

**Authority doc**: `~/substrate/harness-engineering/scratch/answers/telemetry-survey-for-lynx.md`
— written for us by pij-respectable-clam (harness-engineering seat, plan 090
telemetry remediation). Read it FIRST. It contains: the store inventory, copy-paste
queries, seat-attribution chain, difficulty-proxy ranking, caveats, the F1–F6
dogfood feedback asks, and the (a)/(b)/(c) first-class classification.

**What we validated on this machine (all measured, 2026-08-27):**

- `refs/harness-telemetry/**` = **0 refs** (old collector off by default — expected).
- `refs/notes/ai` = **207 commits** with attribution notes + JSON footers.
- metrics-db (`~/.git-ai/internal/metrics-db`, sqlite, event_kind=5 = transcripts):
  flowspace3 sessions = o-prime (a5a5588f…, 9,555 events), PA bitter-swan
  (222c2c9d…, 3,871), ermine (c800c9ff…, 89), Jordan's two opus bootstrap
  sessions (4d0b06f2…, b1d6f4fb…), + subagent sessions (agent-…, linked via
  external_parent_session_id). 961 stderr-bearing events repo-wide, 0 interrupts.
- bash-checkpoints-db (`~/.git-ai/internal/bash-checkpoints-db`, **30-day
  retention**): 1,262 flowspace3 commands with full text + ns timings; columns
  are `original_cwd`/`repo_work_dir`/`start_time_ns` (clam's sketch had older
  names — fixed queries are in this repo's session history and trivially rederived
  from `.schema bash_checkpoint_calls`).

**KEY FINDINGS (the dogfood gold — do not lose these):**

- **F1 coverage hole confirmed**: all 13 worker seats ran under the `pi` (omp)
  harness, which git-ai does NOT instrument → **zero transcripts, zero bash rows**
  for every worker. Only claude/copilot-bound seats have rich telemetry.
- **Mis-attribution**: worker commits' `refs/notes/ai` footers name o-prime's
  claude session uuid for code o-prime never wrote (proof: c7670cd, wren's fs3-git
  crate → a5a5588f). An uninstrumented harness's commit apparently inherits the
  nearest instrumented session. This corrupts per-seat surveys silently.
- **The seat↔session join**: `pij sessions` (plain CLI verb) is the join table —
  columns pij-id / harness / harness-session-uuid / model / parent. For
  claude/copilot seats the harness-session column IS the metrics-db
  `external_session_id`. For pi-harness seats the id is an omp uuid (01a03…)
  that appears in NO git-ai store — that missing hop is finding F2.
- Commit-note footer path: `git notes --ref=ai show <sha>` → JSON footer →
  `sessions.<s_id>.agent_id.id` = agent session uuid.

## 3. What remains (the actual task)

1. Run the full survey per clam's §2C for the sessions that HAVE data (o-prime,
   PA, subagents): failures, durations, interrupts, retry-similarity — produce
   the per-session difficulty table.
2. Write the F1–F6 field report (clam's addendum lists exactly what plan 090
   needs; F1 is answered above: 0/13 workers with transcripts, name harness=pi
   per miss; F6 needs measured token totals vs orchestrator guess).
3. Deliver as markdown + pij pointer to pij-respectable-clam (findings land in
   docs/plans/090-telemetry-remediation/assets/research/ with our attribution).
4. Report completion + anything surprising to the current o-prime
   (pij-instant-lynx) — and per AGENTS.md, dogfood flowspace3 while you work and
   observe every friction.

Retention clock: the bash db keeps 30 days; the v0.2.0 window is safe today but
do not sit on this for a week.

## 4. Where the work is logged (the full map)

- **Who did what**: `.harness/government/worker-roster.md` — every seat, landed
  commits, revive keys. THE ownership authority (git identity is shared).
- **Retro records**: `.harness/records/retro/2026-08-26/001-007.md` (earlier
  drains) and `2026-08-27/008.md` (the big one, 132 entries + raw JSON).
- **Rulings**: `.harness/government/rulings/` — Jordan's binding decisions
  (PR-workflow cutover, primes-owe-status-cards, etc.).
- **How we work**: `.harness/government/how-we-work.md` — the operating manual
  (briefs, dossiers, packets, worktrees, merges).
- **Briefs**: `.harness/government/briefs/` — per-worker task briefs.
- **Design docs**: `docs/plans/prd/*.md` (doctor-design, daemon-worker-architecture
  with the RULED reconcile-loop doctrine), `docs/plans/` for plan folders.

## 5. The PRD and deterministic documents (ddocs)

**The main PRD — where we keep everything** — is
`docs/plans/prd/base-prd.dd.json`, rendered to its sibling
`docs/plans/prd/base-prd.dd.md`. 57 requirements (req-0001..req-0057), each with
`state` (checked/unchecked) and an evidence `note` naming the landing commits.
It is the single product source of truth: every feature Jordan rules in gets a
req; every ship flips its state with evidence.

**Deterministic documents (dd)**: a document is JSON (the `.dd.json` is the
authority), validated against a schema, and the human-readable `.dd.md` sibling
is GENERATED — never hand-edited. Every element has a canonical stable address
(e.g. `docs/plans/prd/base-prd.dd.json#requirements/req-0056/state`), so agents
cite and edit precise fields instead of prose-matching. The toolchain is the
`ddocs` CLI:

    ddocs agents-start-here          # orientation (run this once)
    ddocs validate <path>            # schema + link validation
    ddocs build <path>               # regenerate the .dd.md sibling

Workflow for ANY PRD change: edit the .dd.json (surgically), `ddocs build`,
`ddocs validate`, commit BOTH files together via `harness commit`. Never edit
the .dd.md directly — it is build output.
