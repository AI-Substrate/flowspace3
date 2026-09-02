-- Migration 0023 - live jobs stay small, uniquely owned, and cheaply countable.
--
-- A failed-but-revivable job is still live work. Before this migration it released
-- its dedupe key, so another enqueue could mint a second row before boot recovery
-- moved the first one back to pending. Keep one owner and retire only redundant
-- failed rows; the existing unique index already guarantees at most one
-- pending/running row per key.
WITH ranked AS (
    SELECT id,
           row_number() OVER (
               PARTITION BY dedupe_key
               ORDER BY (state IN ('pending', 'running')) DESC, id DESC
           ) AS owner
      FROM jobs
     WHERE state IN ('pending', 'running')
        OR (state = 'failed' AND NOT terminal)
)
UPDATE jobs
   SET terminal = true,
       updated_at = now()
 WHERE id IN (SELECT id FROM ranked WHERE owner > 1)
   AND state = 'failed';

DROP INDEX jobs_live_dedupe_idx;
CREATE UNIQUE INDEX jobs_live_dedupe_idx
    ON jobs (dedupe_key)
    INCLUDE (kind, state, last_error, terminal)
    WHERE state IN ('pending', 'running')
       OR (state = 'failed' AND NOT terminal);

-- Each bounded purge starts at the oldest completed row instead of rescanning
-- the history it is shrinking.
CREATE INDEX jobs_done_retention_idx
    ON jobs (updated_at, id)
    WHERE state = 'done';

-- Ordinary status reads the latest failure separately from the live census;
-- terminal failures therefore need their own ordered serving path.
CREATE INDEX jobs_failed_recent_idx
    ON jobs (updated_at DESC)
    WHERE state = 'failed' AND last_error IS NOT NULL;

-- One durable receipt survives daemon restarts and costs status one primary-key
-- lookup. The daemon records a completed sweep only after every bounded delete
-- has finished, so this row never claims a partial run was complete.
CREATE TABLE job_retention_state (
    singleton       BOOLEAN PRIMARY KEY DEFAULT true CHECK (singleton),
    last_purge_at   TIMESTAMPTZ,
    purged_last_run BIGINT NOT NULL DEFAULT 0 CHECK (purged_last_run >= 0)
);
INSERT INTO job_retention_state (singleton) VALUES (true);
