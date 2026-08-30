# Orient — local (lever 2)
**Scope**: THIS REPO (flowspace3) · generated 2026-08-26 · o-prime `pij-instant-lynx` single writer

## What this project is

flowspace3: a ground-up rebuild of flowspace2 (**fs2**, `/Users/jordanknight/substrate/fs2/flow_squared`) —
semantic code search: parse a codebase into a code graph (tree-sitter), enrich with LLM
summaries + embeddings, query by meaning via CLI/MCP. Same idea as fs2, but with Jordan's
changes, improvements, and **simplifications**.
Do NOT copy fs2's architecture by default; it is prior art, not the spec.

**Design pillars named by Jordan (2026-08-26, spine 102700 — verbatim direction, pre-plan):**
- **Rust**, tree-sitter used **directly** for AST parsing.
- **No SCIP/sourcegraph** toolchain for cross-file rels.
- **No graph store / pickle** — all data in a **central Postgres + pgvector** ("all our
  stuff available in one place").
- **Worktree-friendly by construction**: no copy-graph-then-rescan; diff changed files by
  git commit/blob ids — "basically a git tree of the files but with the FS version of it
  in pg-vector".
- Keep fs2's enrichment shape: split file → summarize file + every method/component
  (e.g. md sections) → embed; online or local LLM/embedders; parallelizable.

## Mandatory orient reads

- fs2 prior art: `/Users/jordanknight/substrate/fs2/flow_squared/README.md` (pipeline, data model)
- fs2 plan history: `/Users/jordanknight/substrate/fs2/flow_squared/docs/plans/` (43 numbered plans)
- This repo's harness docs once landed (engineering harness being stood up by `pij-managerial-peacock`)
- PRD / simplification brief: **does not exist yet** — Jordan names the changes; ask, never invent

## What matters here

- Simplification is a stated goal — challenge fs2's weight (pickle/NetworkX store, ~40-file
  adapter/fake matrix, multi-provider sprawl, SCIP toolchain, layered config) before porting it.
- fs2 CLI is installed globally (`fs2`) and its self-graph works — use it to interrogate prior art.
- Store-native governance: spine events + project `flowspace3-ground-up-rebuild-of-flowspace2-fs2-s`.

## Harness surface

Harness installed (peacock, 2026-08-26, commit ec66578; CLI 0.13.0). Extensions report
degraded until the cargo workspace lands (plan 001 turns them green).

| Need | Command | Evidence |
|---|---|---|
| Discover/boot | `harness boot` | toolchain → cargo build → checks + orientation |
| Cheap proof | `harness checks` | fmt --check · clippy -D warnings · cargo test (stops at first red) |
| Full proof | `harness checks` + `cargo test --workspace` + compose PG integration | plan-001 exit bar |
| Plan integrity | `harness plan validate docs/plans/<ord>-<slug>` | dd graph check |

## Repo mechanics — derive, do not copy

| Question | This repo's answer |
|---|---|
| Cheap quality gate | `harness checks` |
| Full pre-ship gate | `harness checks` + `harness boot` + `cargo test --workspace` |
| Notify-only worktree actions | ordinary isolated reads/edits/gates/commits |
| Never-stage list | `.harness/government/` stays committable; graph/index artifacts (fs2 analogue `.fs2/`) — confirm once the store design lands |
| Flow-state rule | `.the-flow-state.json`, `the-flow.json`, `the-flow.md` — the-flow/builder guided mode sole writer |
| Worktree root | `pij-worktrees/` (sibling pattern) — reserve at brief time |
| Worktree naming | `s<ord>-<slug>` |
| Base branch | `main` (currently at Initial commit `001d1c2`) |
| Landing policy | `/builder 8 ship` → PR/CI/confirmed merge (harness pending) |
| Fleet defaults | per machine norm: copilot coder + cross-model reviewer via `/pij pair` |
| Human digest channel | Jordan in the prime pane; self-identified pij message otherwise |

## Current portfolio context

- Project record: `flowspace3-ground-up-rebuild-of-flowspace2-fs2-s` (prime `pij-instant-lynx`)
- In flight: engineering-harness standup — seat `pij-managerial-peacock` (task `asg-willowy-hornet`)
- Next intake: Jordan's list of changes/improvements/simplifications vs fs2 → becomes the first
  real work item(s); nothing is dispatched until named.
