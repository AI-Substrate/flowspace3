# Worker brief — daemon re-ingestion churn investigation · (seat at canary)

**From**: pij-instant-lynx (o-prime) · **Date**: 2026-08-28 · URGENT, read-only investigation.

## The job

Jordan (verbatim): "Daemon is currently ingesting A LOT but we didnt add
anything new... im worried we are burning tokens re-ingesting things that are
already present!"

Evidence at dispatch (flowspace3 status queue): 478 scan_file pending, 787
summarize pending, embed draining — while no repo was added and no bulk edits
happened. Summarize/embed = paid LLM calls; if this is re-ingestion of
already-present content it is a live token leak.

Deliverables (numbered):

1. WHAT is being processed: sample the pending/running jobs (daemon logs
   ~/.flowspace3 or the tmux pane %50 output, and the PG jobs table on
   localhost:5433 db flowspace3 — READ ONLY) — which roots, which files, which
   job payloads.
2. WHY queued: distinguish (a) legitimately new/changed content (e.g. our
   scratch/skill files written today — the watcher indexes live edits),
   (b) re-scan of UNCHANGED content (hash dedupe should have stopped it),
   (c) boot heal sweeps (missing_embeddings/missing_enrichment) re-queuing,
   (d) watcher misfiring (e.g. rescanning on touch/atime, .harness/temp churn),
   (e) GC-then-reheal loop (reaper deleting what the sweep re-queues — the
   worst case, an infinite paid loop).
3. COST call: are the summarize/embed jobs deduped by content hash before the
   provider is called (i.e. queued-but-cheap) or do they hit the API? Cite
   code paths (crates/daemon, crates/store) and log lines, not guesses.
4. VERDICT + smallest fix: benign backlog (say so plainly) or a defect (name
   the mechanism, the smallest fix, and whether the daemon should be paused —
   pausing is Jordan/o-prime's call, recommend only).

## Rules & fence

- READ ONLY: no writes to the DB, no daemon restarts, no config changes, no
  code edits. Scratch: .harness/temp/w-daemon-churn/** and scratch/ if needed.
- DOGFOOD: try flowspace3 search for code questions before grep.
- `harness observe` frictions; list, never clear.

## Report back

claim · evidence (queries/log lines pasted) · the (a)-(e) classification with
counts · cost call · verdict + recommendation · observations. Deviations =
stop-and-ask. Ack via pij send with your read + numbered plan first (reply
USING PIJ TOOLING - pij send pij-instant-lynx).
