-- Migration 0031 - autovacuum thresholds that fire on million-row tables.
--
-- Postgres vacuums a table once 20% of its rows are dead and analyzes it once
-- 10% have changed. On the big tables that is hundreds of thousands of rows,
-- so it never happened: on 2026-10-14 prod's elements held 245k dead rows,
-- had never been vacuumed, and was last analyzed mid-backfill. Its stale row
-- estimates turned a one-row queue probe into a sequential scan every five
-- seconds (1.2 billion blocks read) and the keyword leg's estimate into 8 rows
-- against 10,623. Approved by Jordan, 2026-10-14.
--
-- 2% dead / 1% changed, per table. Insert-only growth (vectors, turns) is
-- vacuumed at 5% so the visibility map stays current for index-only scans.
--
-- embeddings_1024 is the exception for VACUUM, not for ANALYZE. Vacuuming it
-- also vacuums its HNSW index (13 GB on prod), which is the expensive part:
-- the one-off prod VACUUM on 2026-10-14 took 50 minutes and saturated the
-- machine. At 2% (~32k dead rows) that would run often, so it vacuums at 10%
-- dead and 10% inserted; statistics still refresh at 1%, because ANALYZE
-- only samples.
--
-- The two biggest tables also vacuum slowly on purpose: a 10 ms cost delay
-- (the default is 2 ms) makes a long vacuum trickle beside live searches
-- instead of spiking the shared I/O and CPU they need.
ALTER TABLE elements SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_analyze_scale_factor = 0.01,
    autovacuum_vacuum_insert_scale_factor = 0.05,
    autovacuum_vacuum_cost_delay = 10
);
ALTER TABLE embeddings_1024 SET (
    autovacuum_vacuum_scale_factor = 0.10,
    autovacuum_analyze_scale_factor = 0.01,
    autovacuum_vacuum_insert_scale_factor = 0.10,
    autovacuum_vacuum_cost_delay = 10
);
ALTER TABLE turns SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_analyze_scale_factor = 0.01,
    autovacuum_vacuum_insert_scale_factor = 0.05
);
ALTER TABLE smart_content SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_analyze_scale_factor = 0.01,
    autovacuum_vacuum_insert_scale_factor = 0.05
);
ALTER TABLE worktree_files SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_analyze_scale_factor = 0.01
);
ALTER TABLE jobs SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_analyze_scale_factor = 0.01
);
