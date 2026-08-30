# w-queue-waste-audit — are the LLM and embedding lanes wasteful, looping, or off-design?

**From**: pij-instant-lynx · 2026-08-30 · Jordan's verbatim intent: "reviewing
the queue history for llm and embedding to make sure we are not being
wasteful, that is not looping and that its processing as designed (a log
analysis please)."

## The job — READ-ONLY forensic analysis, report-first, no code

Analyse the PROD jobs history (Postgres 127.0.0.1:5433, db flowspace3 —
read-only SQL only: BEGIN TRANSACTION READ ONLY) and the daemon's console
history (tmux pane scrollback + any log files) for the summarize (LLM) and
embed lanes. Answer with numbers:

1. **Waste**: are we paying providers for work already done? Group done
   jobs by dedupe_key/content-hash generations — how many summarize/embed
   executions were REPEATS of identical content (same raw_hash) vs genuine
   new work? Quantify: repeated executions × approximate provider cost.
   The content-hash dedupe SHOULD make repeats zero-cost — prove it does
   or show where it leaks (known suspects: conversation re-ingest,
   worktree churn era pre-#69, the empty-string embed rows 68).
2. **Looping**: any job/dedupe_key with attempts > N or many generations
   in a short window? The failed rows (empty-string embeds ~7, dup-root
   scans ~30) — are they retried hot or parked as designed (requeue only
   at boot sweep)? Any oscillation signatures (same key
   pending→done→pending repeatedly)?
3. **As-designed processing**: verify the post-#72/#73/#75 shapes in the
   history: embed calls batching ~16+ texts (not 1), settlement without
   whole-queue scans, LIFO claim order per lane. The whale ingest
   (14.5k-turn conversation, ~04:30-05:30Z today) is your best window —
   measure its summarize drain rate and embed batch sizes from job
   timestamps.
4. **Anything else the history shows** that we have not asked about —
   surprises welcome, evidence required.

## Output

`scratch/queue-waste-audit.md` on the prime-governance worktree
(/Users/jordanknight/substrate/flowspace/fs3-governance/scratch/) — findings
ranked by cost, each with the SQL/log receipt inline, plus a verdict line:
WASTEFUL / CLEAN / MIXED with the one number Jordan should remember.

## Rules

READ-ONLY everywhere: no DML/DDL, no daemon mutation, no root changes, no
restarts. psql via `docker exec flowspace3-db psql -U flowspace3 -d
flowspace3`. Do not clear or drain anything. Report to pij-instant-lynx by
path pointer. Numbered plan-of-attack first, WAIT for ack.
