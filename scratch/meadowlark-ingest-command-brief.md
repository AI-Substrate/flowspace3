# Brief for pij-massive-meadowlark — conversation ingest as a first-class harness command

**From**: pij-instant-lynx (flowspace3 o-prime) · 2026-08-30 · Jordan's
verbatim intent: "brief meadowlark to add it as first class command to
harness… probably during hooks i think — small quick command that fires the
agent harness and the session id in… off by default, enabled by in harness
.harness folder setting somewhere (not .env)."

Extends the earlier handover (`scratch/meadowlark-flowspace-handover.md`) —
this narrows it to ONE concrete deliverable, now proven on the flowspace side.

## What is proven and waiting (as of today)

- `flowspace3 conversation ingest --harness <claude|omp|pij|metrics-db>
  --session <id>` is live in prod: reads the harness's OWN session store
  incrementally (durable cursors — a re-run costs only new turns), submits
  in 10-40ms, keeps guid identity stable (no duplicate conversations),
  discovers subagent transcripts, and the whole envelope is agent-first
  JSON with next_action. Measured today: 7,436-turn first ingest, +243-turn
  incremental re-run, 12 subagent conversations auto-discovered.
- The command is CHEAP and ASYNC — it queues and returns; enrichment
  happens daemon-side. Repeat firings collapse into one pending job
  (dedupe key), so firing on every hook is safe.

## The deliverable

A harness-side first-class command + hook wiring:

1. **The command** (e.g. `harness convo sync` — name is yours): detects
   whether flowspace3 is installed (`which flowspace3` + a fast daemon
   probe; silent no-op when absent), resolves the CURRENT harness kind and
   session id from the harness's own context, and fires
   `flowspace3 conversation ingest --harness <kind> --session <id>`
   fire-and-forget. Total added latency budget: tens of ms.
2. **Hook wiring**: fire it from the hooks the harness already owns —
   Jordan says "probably during hooks"; the shapes we discussed in the
   handover: commit-time as primary anchor, boot/session-start as catch-up
   drain (harness has no session-end hook). Your call on exact hook set;
   the dedupe makes over-firing harmless.
3. **Consent switch — OFF BY DEFAULT** (Jordan, explicit): enabled per-repo
   by a setting in the repo's `.harness/` folder (NOT .env, NOT global) —
   e.g. a key in existing harness settings machinery. No setting = no
   ingest, silently. This implements the per-repo opt-in consent Jordan
   ratified earlier (and `HARNESS_NO_TELEMETRY` stays honoured as a
   global kill).
4. **Envelope honesty**: when enabled but flowspace is unreachable, the
   command says so once (not per-hook spam) — a quiet buffered/deferred
   posture, never a hard failure that blocks the hook.

## Verify against

flowspace3 repo (`~/substrate/flowspace/flowspace3`) has the working end:
try the CLI yourself; `flowspace3 conversation --help` documents the
surface. Report status/questions to pij-instant-lynx; product rulings on
the flowspace side route through me, harness-side design is yours.
