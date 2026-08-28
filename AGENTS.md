# flowspace3 — agent guide

A Rust workspace building **flowspace3**: semantic code search you run locally
(daemon + CLI, Postgres/pgvector, agent-first JSON envelopes).

## CLI output is audience-aware

- A terminal receives human-readable output by default.
- A pipe, file, CI capture, or agent subprocess receives the JSON envelope with no flag.
- `--json` forces JSON anywhere.
- Export `FS3_OUTPUT=json` when a harness runs inside a PTY (for example tmux) and its terminal probe looks human; this pins the machine shape once.

## Dogfood the product — this is NOT optional

If you are an agent working in this codebase, you **MUST use flowspace3 itself**
while you work. We build a tool for agents; an agent that greps its way around
this repo without ever running the product is skipping the most valuable test
we have.

- **Search with it first.** For any meaning-shaped question ("where is retry
  handled", "what owns the watcher debounce"), run `flowspace3 search "<question>"`
  before reaching for grep. Exact-identifier lookups may still use grep — that
  is the tool's own guidance.
- **Orient with it.** `flowspace3 agents-start-here` and `flowspace3 docs list`
  are the front door; use them the way a fresh outside agent would.
- **Exercise the loop.** Add/status/doctor as your work touches them. If the
  daemon is running, your edits are being indexed live — check that what you
  just wrote is findable.
- **Report EVERY problem to the current prime.** Anything confusing, wrong,
  slow, or silently surprising — a bad `next_action`, a search that missed
  obvious code, a doctor row that lied, an envelope that made you guess —
  gets TWO actions, immediately, not at session end:
  1. `harness observe "<what happened>" --kind difficulty|confusion` (the durable record), and
  2. a short pij message to the current o-prime seat (see
     `.harness/government/worker-roster.md` for who that is) so it can be
     acted on while you are still in context.
  A friction you route around silently is a bug you shipped to every future
  user. Day-one feedback from agents IS the product's test suite.

## Engineering harness

This repo has an engineering harness — the deterministic front door to building,
proving, and improving it. Use it rather than guessing around it.

At session start:

1. `harness --version` — ensure the global CLI is installed
   (`npm i -g @ai-substrate/engineering-harness` if missing)
2. `harness instructions` — the agent briefing (AGENTS START HERE)
3. `harness doctor --json` — what's configured and which extensions loaded
4. `harness boot --json` — prove the environment before changing it

Before you call work done:

- `harness checks` — the mandated quality gate (`cargo fmt --all --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --all`). A red gate
  is a verdict, not a suggestion. If the gate itself is wrong, fix the gate
  first, re-run so it points at the real problem, then fix the code.

`.harness/engineering-harness.md` is the governance doc: what boot proves, the
signal inventory, and the back-pressure gaps named honestly.

If the eng-harness skills are loaded in your CLI, `/eng-harness-flow` routes you
to the right next harness action at any point.

### Capture friction the moment it happens

```bash
harness observe "<what happened>" --kind difficulty --severity degrading \
  --workaround "<what you did>" --suggested-encoding "<how to fix it for the next agent>"
```

Fire on: a retry or backtrack · a search that returned nothing where you expected
matches · a failure you had to guess to interpret · runtime behaviour you could
only infer · hidden or tribal setup · catching yourself thinking "if only there
were a…".

Drain at session end — **BUT the buffer is SHARED across every agent in this tree**, so:
CAPTURE freely (`harness observe`), but the drain (`--list` → `harness record retro` → `--clear`)
is **o-prime-owned** — clearing destroys siblings' live observations. Workers: list and REPORT,
never clear. (Ruled 2026-08-26 after a worker correctly refused the clear.)

Encode, don't document: if you had to infer something twice, that is a missing
command, not a missing doc.

## Coming in cold? Read the operating manual

**`.harness/government/how-we-work.md`** is the complete record of how this repo
is run — how Jordan directs, what o-prime is, how work becomes briefs and briefs
become coder seats, the PR-era workflow, rulings, retros, and what a new agent
should expect. If you have no context, read it before doing anything. The other
standing references: `worker-roster.md` (who did/does what — the ownership
authority), `rulings/` (binding decisions), `docs/plans/prd/base-prd.dd.json`
(the product source of truth), and `.harness/records/retro/` (what has hurt us
and what we encoded in response).

## Working model: worktree-per-coder + PRs (since 2026-08-27)

Main is **branch-protected** — you cannot push to it. The workflow for any
coding packet:

1. Work in **your own git worktree** on a branch named for your packet
   (e.g. `git worktree add ../fs3-<packet> -b <packet>` from the main clone —
   never work inside another seat's tree).
2. Commit as you go on your branch (conventional commits are BINDING —
   release-please reads them: `feat:`/`fix:`/`perf:` bump versions).
3. Gate locally with `harness checks` before declaring done — in your own
   worktree the verdict is trustworthy and entirely yours.
4. Open a **PR into main**. CI runs on the PR (it does not run on pushes);
   a green gate is a merge requirement. O-prime coordinates review, merge
   order, and releases — do not merge your own PR unless told to.
5. Tidy the worktree when the packet lands.

<!-- BEGIN harness:commit-guidance -->
## Committing in this repo

Use `harness commit "<message>" -- <paths>` rather than a chained
`git add … && git commit …`.

A `harness commit` is **verified or named**: it probes the collector ingress,
commits, and then tells you WHICH outcome you got. It never blocks and never
rolls back. The outcomes are:

- **confirmed** — when the collector ingress socket is reachable: harness commits with no trace2 override, waits (bounded) for the `refs/notes/ai` note, and tells you whether it landed. A landed note is the healthy shape, and a miss is reported to you rather than hidden — with the next step named in the command's own output. Nothing was buffered on this path, so there is nothing to drain.
- **buffered and named** — when git's configured trace2 target is a plain FILE, or when the ingress is blocked, absent or unconfigured: the commit is made with its trace2 events going to a buffer file instead of the collector, so attribution is DEFERRED, not lost — and it isn't proven yet either. `harness commit` names the buffer it used; when the configured target is a plain FILE it must be pointed back at the socket first, because while it names a file there is no ingress to replay into. Drain it with `harness doctor telemetry-nudge` from an UNSANDBOXED shell. Recovery is POSIX-ONLY: the drain replays into an af_unix socket, so on a Windows host `harness doctor telemetry-nudge` refuses on platform grounds and drains nothing — the buffered events stay on disk, untouched, until they are drained from a host whose collector ingress is an af_unix socket.
- **NOT VERIFIED on this platform** — when trace2 points at a Windows NAMED PIPE (\\.\pipe\…): the commit is made with no trace2 override (git talks to the pipe as usual), nothing was buffered, nothing was written beside the pipe — and nothing is claimed about attribution, because nothing was measured. Check for yourself with `git notes --ref=ai show HEAD`. Do NOT run `harness doctor telemetry-nudge` — there is no buffer to drain and no replay path for the named-pipe transport, and it will refuse.

A chained or compound `git commit` can **silently lose attribution** — agent
command sandboxes block git-ai's socket, git quietly disables trace2, and the
commit's authorship may later be recorded as human.

Neither shape guarantees delivery. What `harness commit` guarantees is that the
outcome is never silent. Read `harness instructions commit` for the detail.
<!-- END harness:commit-guidance -->

**Ownership questions route to o-prime, never to git.** Every seat commits under
Jordan's git identity, so `git log --format='%an'` is a NULL signal for who wrote
a file — and `refs/notes/ai` can silently lose attribution (a seat-written commit
can read humans-only) and carries no seat slug even when healthy. To learn which
worker owns a file or fence, ask o-prime / read `.harness/government/worker-roster.md`.
(DL-011, 2026-08-26.)
