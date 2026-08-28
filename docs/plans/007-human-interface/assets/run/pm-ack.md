# PM ack — plan 007-human-interface (pij-team run #3)

Seat: pij-near-carp · spawn s1787883398515-11020 · omp github-copilot/claude-opus-5, effort high
Worktree: /Users/jordanknight/substrate/flowspace/fs3-human-interface · branch 007-human-interface @ e11141a
Prime: pij-instant-lynx (product owner) · date 2026-08-28

## What I read

- `docs/plans/007-human-interface/packet-pm.dd.md` (all 7 sections)
- `docs/plans/007-human-interface/impl-guide.dd.md` + `.dd.json` (BINDING — architecture, fan-out, units, isolation, composition, review, risks)
- `plan.dd.md` (6 ACs, 3 phases) + `assets/tasks/phase-{1,2,3}/tasks.dd.md` (tk-a101..a105, tk-a201..a203, tk-a301..a303)
- `.agents/skills/pij-team/TENETS.md` (13 tenets) and `SKILL.md`
- `pocs/human-render/README.md` + `LEARNINGS.md` (the promotion shape — §4 is the port order, §1 the crate justifications, §3.1 the named prerequisite)
- `.harness/government/settings.dd.md` (pm/coder opus-5 high; reviewer gpt-5.6-sol) and `how-we-work.md` §9b (spawn → canary → roster → pointer dispatch → ack → edges → merge → wire discipline)
- `AGENTS.md` (CLAUDE.md is a symlink to it — ref r5 resolves)

Dogfood receipt: `flowspace3 search "where does the CLI print the JSON envelope to stdout"` → top hit
`el:…/crates/cli/src/main.rs::emit` (score 0.618) — that `emit` is the exact renderer-seam insertion
point for tk-a102. No grep was needed to find it.

## The invariant, in my words

The envelope is produced first and rendered second, always. Rendering is a function of bytes that
already exist; it never reaches back into envelope construction, never reorders a field, never adds
one. tk-a103 goldens are captured from pre-plan main BEFORE any render code exists, so the
byte-identity assertion has a witness that cannot have been contaminated by this plan's work.

## Numbered plan

1. **Phase 1, solo (tk-a101..a105).** Order: a103 goldens FIRST (uncontaminated capture), then a101
   the core decision fn (`resolve(tty, --json, --human, FS3_OUTPUT)`, precedence flags > env > tty,
   table-driven exhaustive tests, Jordan's ruling verbatim in rustdoc), a102 the renderer seam at
   `crates/cli/src/main.rs::emit` (stub renderer; uncovered verbs fall through to JSON honestly),
   a104 the `--watch` NDJSON wire types + `docs/services/event-stream.md` (versioned first line,
   queue transition / scan+enrich completion / root change / heartbeat), a105 vendoring the POC
   fixtures + tui-poc `VERDICT.md` into `assets/inputs/` with shas.
2. **Freeze announcement to prime as a gate** — seams + wire types + golden shas named, no coder
   spawned before your ruling on it.
3. **Phase 2 fan-out, three coders** (omp opus-5 high), each in its own worktree off
   007-human-interface: u-r renderer promotion, u-w daemon event stream, u-t tui verb. Packets by
   `cp` + `ddocs set` + `ddocs build`, delivered by absolute-path pointer; canary demanding reply via
   pij tooling; ack-with-numbered-plan ruled by number before any coder writes code. Every packet
   carries: absolute paths (DL-007/008), ASSUMPTIONS section + prove-in-tree evidence in done
   reports, `PIJ_SESSION_ID` export, no `docker compose up`, `CARGO_INCREMENTAL=0`, the
   rustc-LLVM-IO=disk signature, observed toolchain version + Avail in every report.
4. **Phase 3 composition (mine).** Ask each coder its ASSUMPTIONS before touching a merge. Merge u-w,
   then u-r, then u-t; rehearse the u-t fixture-to-live stream swap on a throwaway ref before
   relying on it. Gate green, allowlist reconciled. First light (tk-a302): one session — human
   output in a TTY, piped byte-check against the a103 goldens, tui live on the real stream during a
   real scan, plus the r2 pij-seat check (a tmux-PTY agent still gets JSON via `FS3_OUTPUT`/`--json`).
   Unit-internal rework at this step = phase-1 defect: stop, record, route to you.
5. **Reviewer** (settings default gpt-5.6-sol) after composition, read-only, priorities per the
   impl-guide review section; findings fixed or refuted with cited evidence.
6. **Close-out.** `/builder` post-flight + archive, `harness plan validate` clean, dd-native
   bookkeeping continuous, PR into main opened and held UNMERGED for you. Coders stood down with
   sha-verified buffer rescues BEFORE any worktree removal. Completion report + a
   prototype-improvement report (what in TENETS/templates/this packet was wrong, missing, confirmed).
7. **Throughout**: status cards at every unit edge (mine and coders'), stale chased via unscoped pij
   anomalies; `harness observe` every friction the moment it bites, list-never-clear (the buffer is
   shared).

## Asks — rule by letter

- **A. Boot is degraded in every worktree, and it is a false negative.** `harness boot --json` →
  `compose: service "db" is not running`, while `docker compose ls` shows project `flowspace3`
  running from the main clone and `flowspace3 status` returns `ok:true` against :5433. `docker
  compose ps` is scoped to the worktree cwd, so the project resolves empty. Captured as DL-001 in
  the shared buffer. **I intend to treat the compose stage as non-blocking for this run** (toolchain,
  crate, build all green) and not run `docker compose up`. Confirm.
- **B. The LEARNINGS §3.1 prerequisite is not in the impl-guide, and it is a collision-surface
  defect (tenet 3).** The POC calls the payload-DTO move "the only design decision" in the
  promotion, and the types are where it says: `Hit` + `SearchResults` at
  `crates/daemon/src/search.rs:84,113` (u-w's fence), `Step` + `DoctorReport` at
  `crates/cli/src/doctor.rs:61,216` (u-r's fence). So u-r needs types that live inside u-w's unit —
  the one shared file the fan-out claims not to have. Three ways out: (i) I do the core-ward DTO
  move in phase 1 as tk-a106, before fan-out, so wave 1 stays zero-shared-file — a faithful move
  preserves serde declaration order, and the a103 goldens captured first are the proof it changed no
  bytes; (ii) u-r hand-mirrors DTOs like the POC's `views.rs`, accepting the drift risk the
  one-envelope decision exists to kill; (iii) u-r renders from untyped `Value`. **I recommend (i)**
  and will take it unless you rule otherwise — it is the option that keeps the seams frozen and the
  collision surface at zero.
- **C. Golden capture provenance.** `flowspace3 status` against the live shared db is
  non-deterministic (queue counts move), so goldens captured from it would be flaky by
  construction. I intend to capture the a103 goldens by building the PRE-PLAN main binary
  (main @ `1ce572b`) and running the covered verbs against a seeded deterministic fixture store via
  testkit — offline, repeatable — recording the source sha and the toolchain version as provenance
  in the goldens' header. Confirm, or name a store you would rather I froze.
- **D. Cross-worktree read.** `scratch/` is gitignored, so `scratch/tui-poc/VERDICT.md` exists only
  in the main clone (`/Users/jordanknight/substrate/flowspace/flowspace3/scratch/tui-poc/`). I will
  READ it and vendor a copy into `assets/inputs/` (never write into that tree). Flagging because it
  crosses a worktree boundary; say stop if that is not sanctioned.

No coder is spawned and no code is written before your GO.
