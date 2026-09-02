# w-db-cpu-profile — why is the flowspace3 Postgres (OrbStack) eating this machine? (Jordan, 2026-09-02)

## Jordan, verbatim
"another thing to check is the db in orbstack is blowing up the cpu on this machine - something bad going on!" … "is there some kind of profiling we can run on it, its real bad... if so get an agent on it"

## Your job: a READ-ONLY profile of prod, with measured causes, ranked fixes, and NO changes
You are an investigator, not a fixer. You may READ anything; you may WRITE only your report and scratch under `.harness/temp/agent/` in the main clone (`/Users/jordanknight/substrate/flowspace/flowspace3`). You do not restart, reconfigure, VACUUM, CREATE EXTENSION, kill queries, or touch :7373's daemon. If a measurement needs a change (e.g. `pg_stat_statements` needs `shared_preload_libraries` + a restart), you write "NEEDS RESTART" and o-prime takes it to Jordan.

## Facts already measured (do not re-derive; extend)
- Container `flowspace3-db` (postgres + pgvector, port 5433) serves PROD for the daemon on :7373 AND every coder's per-run test databases (row 124/126). It runs on postgres DEFAULTS: shared_buffers 128MB, work_mem 4MB, effective_cache_size 4GB (row 122). `embeddings_1024_vector_idx` (HNSW) = 2,286MB; embeddings 289k rows; jobs table ~495k done rows; elements 237k.
- Right now: container ~26% CPU / 355MB RSS; OrbStack Helper 30% steady on the host (peaked 197% earlier today); host load 58–124 through the day; top host consumers are node/tmux processes, not postgres.
- Container log shows `checkpoint complete … write=135–158 s` every ~5 min (checkpoints taking most of the interval) and one backend crash at 22:48Z (concurrent CREATE DATABASE, row 126).
- Active statements at sample time: the search nearest CTE (`WITH candidate_vectors AS MATERIALIZED …`) and `UPDATE jobs SET state … WHERE id = $1` (IO wait).
- Queue: summarize 2,097 pending / 24 running; embed 16 running; scan_file 462 pending.
- `pg_stat_statements` is NOT installed. Extensions: plpgsql, vector, pg_trgm.

## What to measure (numbered deliverables)
1. **Time series, 10 min, 5 s cadence**: `docker stats` for flowspace3-db (CPU%, mem, block IO) alongside `pg_stat_activity` (count active, wait_event_type histogram, the top 3 statements by age) alongside host load. One CSV, one chart-free summary: when CPU spikes, what was running.
2. **Where the CPU goes without pg_stat_statements**: sample `pg_stat_activity` at 1 s for 5 min and histogram normalised query shapes (regexp the literals out). Report the top 10 shapes by sample count. This is a poor man's profile and it is enough to name the hog.
3. **The jobs table hot path**: how often does the daemon poll (`SELECT … FOR UPDATE SKIP LOCKED` or similar — find the shape), what indexes serve it, `pg_stat_user_tables` seq_scan vs idx_scan for `jobs`, n_dead_tup, last_autovacuum. 495k done rows + constant UPDATEs is a bloat/hot-page candidate.
4. **Checkpoint/WAL churn**: from the container log, checkpoint cadence and write times over the last 6 h; `pg_stat_bgwriter`/`pg_stat_checkpointer`; `checkpoint_completion_target`, `max_wal_size`. Are we checkpointing constantly because of test-DB create/drop and job churn?
5. **The search CTE**: `EXPLAIN (ANALYZE, BUFFERS)` on ONE real search query (copy it from `pg_stat_activity` while a search runs, or from `crates/store/src/embeddings.rs`) against prod — read-only. Buffers hit vs read tells us whether 128MB shared_buffers is the cost (row 122 hypothesis).
6. **Autovacuum / bloat**: `pg_stat_progress_vacuum`, and estimated bloat for `jobs`, `embeddings_1024`, `elements`, `turns` (pgstattuple is not installed — use the standard bloat-estimate query).
7. **OrbStack itself**: is the helper's 30% baseline the VM (`orb` CLI stats if present, `docker system df`, the 7 idle `voska/hass-mcp` containers + `fs3-linuxtest` — do they cost anything?). Say what the OrbStack overhead is independent of postgres.
8. **Report**: `.harness/temp/agent/db-cpu-profile-report.md` — measured causes ranked by CPU-seconds attributable, each with the fix, its cost, whether it needs a restart, and which backlog row it maps to (122, 126, 110, 120). Then a one-paragraph answer to Jordan's actual question: is the DB the thing eating the machine, or is it the fleet, and what is the single highest-leverage change.

## Method
Dogfood: `flowspace3 search "<question>"` before grep for anything meaning-shaped in this codebase (e.g. "how does the daemon poll the jobs table"). Every number cites the command that produced it. Capture friction with `harness observe` the moment it bites; list, never clear.

## Channel
Your seat is rs-resident; `pij send pij-instant-lynx` WILL fail (req-0034) — do not try, do not `pij adopt`. Write `.harness/temp/agent/db-cpu-profile-ack.md` (canary: pij id from `pij whoami`, spawnId, model, cwd, CANARY-OK, then your numbered plan) and START — this is an investigation with no code, so you do not wait for a ruling; o-prime reads your ack and interrupts by `pij-rs send` only if something is wrong. Interim findings to `-interim-NNN.md` as you get them; the report when done. `pij report now` at edges.
