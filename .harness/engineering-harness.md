# Engineering harness

> **AGENTS START HERE → `harness instructions`** — the CLI's baked agent
> briefing (envelope contract, role split, discovery loop). Then
> `harness instructions <verb>` per verb.

## Boot command
`harness boot` — checks the Rust toolchain resolves, builds the crate when one
exists (`cargo build --all-targets`), then composes `harness checks` and folds
its verdict in. Prints orientation. Honest today: **degraded** until the crate
exists, because there is nothing yet to build or prove.
Source: `.harness/extensions/boot/`.


## Checks command
`harness checks` — the mandated quality gate, three gates stopping at the first red:
`cargo fmt --all --check` -> `cargo clippy --all-targets -- -D warnings` -> `cargo test --all`.
Agents run it before calling work done; `harness boot` composes it. Add new
classes of proof (coverage, `cargo audit`, schema checks) to the `GATES` list in
`.harness/extensions/checks/extension.ts` so every caller inherits them.
Honest today: **degraded** (no `Cargo.toml`).


## Health check
None yet. flowspace3 has no running service — readiness is "it builds and the
gate is green" (`harness boot`). When a service appears, its health probe belongs
in boot's stage list.


## Interact method
None yet — no runnable surface exists. When one lands (CLI binary, server,
library API), record here how an agent drives it and wrap that as a harness verb.


## Observe method
Only build/test output today, via the `harness checks` envelope
(`error.details` carries the last 40 lines of the first failing gate). No logs,
traces, or screenshots yet — see Back-pressure gaps.


## Deterministic signal inventory
| Signal | Command | Proves | Status |
|---|---|---|---|
| Formatting | `cargo fmt --all --check` | style is not a review topic | wired, unexercised |
| Lint | `cargo clippy --all-targets -- -D warnings` | lint warnings are failures | wired, unexercised |
| Tests | `cargo test --all` | unit/integration behaviour | wired, unexercised |
| Build | `cargo build --all-targets` (via `harness boot`) | the crate compiles | wired, unexercised |
| Toolchain | `cargo --version` (via `harness boot`) | the environment can build at all | **live — passes today** |
| Daemon bounce | `harness daemon bounce --json` | fetched HEAD is current, release builds, the configured listener drains/restarts, and auth + queue surfaces answer | isolated `:17373` drain/restart + 401 tell proven; production transcript is o-prime-owned |

"Wired, unexercised" is the honest reading: the commands are encoded, but no
crate exists yet, so none of them has proved anything.


## Evidence paths
- Envelope output from every verb (`--json`) — the primary evidence surface.
- `.harness/records/` — committed team memory (retros, harness changes).
- `.harness/temp/` — transient session scratch, gitignored, never committed.
- `target/` — cargo build artifacts (gitignored).


## Injection map
<!-- Where the repo's extant dev/SDD flow calls /eng-harness-flow. One row per seam.
     flowspace3 has no in-repo flow surface to weave into: the SDD skills under
     .claude/skills/ are installed copies (overwritten on update), so AGENTS.md
     carries the cues instead. -->

| Seam event | Fires from | What fires it |
|---|---|---|
| session-start (`pre-flight`) | `AGENTS.md` § Engineering harness | agent runs `harness boot --json` before changing anything |
| pre-implement (`pre-coding`) | manual / `/eng-harness-flow --hook pre-coding` | not woven — no plan surface in-repo yet; revisit when a `docs/plans/` flow lands |
| coding | `AGENTS.md` § Engineering harness | agent runs `harness observe "…"` the moment friction bites |
| phase-end (`post-coding`) | manual / `/eng-harness-flow --hook post-coding` | not woven — revisit with the plan surface above |
| plan-complete (`post-flight`) | `AGENTS.md` § Engineering harness | agent drains the buffer into `harness record retro` at session end |


## Back-pressure gaps
Named honestly, in the order they will matter:

1. **Nothing is proved yet.** Every encoded gate is unexercised until the crate
   exists. The first real proof this harness produces is still unwritten.
2. **The Rust assumption is inferred, not confirmed.** `checks`/`boot` were
   shaped from `.gitignore` (Cargo, cargo-mutants, RustRover) alone. No
   `Cargo.toml` confirms it.
3. **No runtime observation.** No smoke path, health probe, log capture, or
   trace. Any claim about runtime behaviour is currently inference.
4. **No CI.** The gate exists locally only; nothing enforces it on push, so
   `harness checks` and whatever CI eventually runs can silently diverge.
5. **No architecture, dependency, or security check.** `cargo audit`, dependency
   rules, and schema checks are all absent — those constraints would be eyeballed.


## Current maturity snapshot
**L1 — Front door.** Governance doc, `.harness/` substrate, an `AGENTS.md`
pointer, and two loadable verbs (`boot`, `checks`) exist and return honest
envelopes. They are not yet L2: no command has been *confirmed* against real
code, because there is no code. L2 arrives with the first green
`harness checks` over an actual crate.
<!-- The single, current L0-L4 level the harness is ACTUALLY at. Updated ONLY at
     the Improve beat (never by boot, which is read-only). See maturity-assessment.md. -->
