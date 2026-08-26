# Stream brief — s001-fs3-foundations
**From**: pij-instant-lynx (o-prime) · **Date**: 2026-08-26 · **Lifecycle**: ADOPTED (human-provided seat), provisional until preamble

## Structure tree

```text
human (Jordan)
└─ o-prime pij-instant-lynx · window prime
   ├─ PA pij-bitter-swan · window fs3-pa (sensor/relay only — never talk work to it)
   └─ this stream pij-bitter-gibbon · PM s001 · repo root window
```

## Work item

- **Plan folder**: `docs/plans/001-fs3-foundations/` — **dd-native**: `plan.dd.json` (READY,
  Simple, CS-3, 14 tasks, 8 ACs) + `assets/tasks/phase-1/tasks.dd.json` + `assets/backpressure.dd.json`.
  `harness plan ready` answers **ready** (survey satisfied, all criteria claimed). Read
  `plan.dd.md` for the human face; mutate ONLY via `ddocs set/add` (never hand-edit `.dd.md`).
- **Workspace**: the MAIN TREE at `/Users/jordanknight/substrate/flowspace/flowspace3`
  (human construction ruling: Jordan seated this PM in the root; single stream, near-bare
  repo — worktree isolation buys nothing yet). **Branch**: work DIRECTLY ON `main` (Jordan's ruling 2026-08-26: "it can just work in this branch to get it all set up") — no stream branch, commit to main as you go.
- **First act — commit the planning substrate** to your branch: `docs/`, `.harness/`
  (government + extensions), AGENTS.md, skills-lock. **EXCLUDE `.claude/`** — its 9
  installed skill copies await Jordan's ruling; never stage that dir.
- **Landing**: commits land directly on `main` (ruled above); `/builder 8 ship` ceremony not required for this stream — the phase exit gates (harness checks/boot green) are the landing bar.
- **Human ask, verbatim**: "your pm for the work is bitter-gibbon. it will use an oh-my-pi
  (omp) agent harnes using Github opus 5 high high coder and github sol gpt 5.6 review
  high please. brief it now."
- **Build config (PRE-CONFIRMED — do not stall at WAITING_FOR_BUILD_CONFIG)**: the ask
  above IS the human's coder/reviewer confirmation: coder = **omp** harness
  (`pij spawn --harness pi --bin omp`), model **claude-opus-5** (github-copilot), effort
  **high**; reviewer = **gpt-5.6-sol** (github-copilot), effort **high**. Run the phase
  via `/pij pair` (coder + cross-model reviewer fleet).
- **Current flow state**: `docs/plans/001-fs3-foundations/the-flow.json` — nav at `plan`
  (done, backpressure receipted); you drive `phase-1` → `review-1` via `harness flow`
  (CLI only — never hand-edit the-flow files).
- **Prior art (paths only)**: `docs/plans/001-fs3-foundations/assets/workshops/001-architecture.md`
  (AUTHORITATIVE — 5 rules, crate graph, 2 ports, fakes-over-mocks; contradicting it is
  stop-and-ask) · `base-prd.md` (43 reqs) · `assets/poc/treesitter-results.md` (feasibility
  + 11 learnings) · `assets/backpressure.dd.json` (proof plan) ·
  `validations/plan-validation.md` · fs2 read-only at `/Users/jordanknight/substrate/fs2/flow_squared`.

## Descriptive fence

- Expected touch set: root `Cargo.toml`, `core/**`, `parsers/**`, `providers/**`,
  `store/**`, `testkit/**`, `daemon/**`, `cli/**`, `docker-compose.yml`, `README.md`,
  `docs/how/**`, `.harness/extensions/**` (checks/boot updates are tasks tk-000c/d),
  `docs/plans/001-fs3-foundations/**` (task states via ddocs, execution log, the-flow via CLI).
- Scratch: `.harness/temp/s001/**`.
- Hard exclusions: `.harness/government/**` (o-prime single-writer) · `.claude/**` ·
  `base-prd.md` (content edits need a ruling) · any `the-flow.json`/`the-flow.md`/
  `.the-flow-state.json` by hand.
- Overlaps: none — no sibling streams exist.
- New worktree-local path: persist, tell the o-prime, continue (tell-not-ask).

## Orient stack

1. Invoke `/pij prime`; stream triage loads the orchestrator module.
2. Portable global orient: `<pij skill>/references/prime/orient-global.md`
3. Local orient: `.harness/government/orient-local.md`
4. This brief
5. Invoke `/thesis` through the host skill mechanism
6. Human preamble + preamble checkpoint (report to pij-instant-lynx)
7. Protocol/ritual pages only on demand

## Assignment and reporting

- Provisional until your preamble report lands with pij-instant-lynx.
- Report at preamble, every phase checkpoint, and ship:
  `claim · artifacts[] · shas[] · gates[] · observations[] · open[]` (§ C10 wire discipline).
- Card cadence: `pij report now "<did>" "<next>"` at every unit edge — you're a PM, you owe one.
- Work on main is notify-only while you are the only stream; tell me before any push to a remote.
- Fleet packets inherit this fence with narrower task allowlists.
- Proof of done: each task's done_when in the dd; the phase's exit = `harness checks` +
  `harness boot` green + `cargo test --workspace` green; open decisions (real-provider
  run timing; docs-link check) live in plan open_questions — ask Jordan directly if
  they block you (never proxy through me).
