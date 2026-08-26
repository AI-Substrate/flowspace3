# flowspace3 — agent guide

A Rust workspace. The crate does not exist yet; the engineering harness was
stood up first, so the proof surface is ready before there is code to prove.

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
