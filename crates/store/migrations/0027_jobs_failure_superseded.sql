-- Migration 0027 - a failure that later work fixed is no longer "the last error".
--
-- `status` reported the newest failed row by `updated_at`. A terminal failure
-- keeps its row for good, and the next enqueue of the same key gets a fresh row,
-- so one bad ingest went on being "the last error" for weeks after the same
-- session had ingested cleanly again (job 3237054, 2026-09-25 to 2026-10-09).
--
-- The success has to be recorded ON THE FAILED ROW. Done rows are purged after
-- `indexing.job_retention_days` (default 1), so "a later done row exists" stops
-- being true a day after a key goes quiet, and the stale failure would come back.
-- `complete_job` sets `superseded_by` on every earlier failed row with the same
-- kind and dedupe key. No foreign key: the done row it names may be purged.
ALTER TABLE jobs ADD COLUMN superseded_by BIGINT;

-- Failures that a still-retained done row already proves were fixed. Older
-- evidence was purged; those failures stay reported until the key next succeeds.
UPDATE jobs AS failed
   SET superseded_by = latest.id
  FROM (
      SELECT DISTINCT ON (kind, dedupe_key) id, kind, dedupe_key, updated_at
        FROM jobs
       WHERE state = 'done'
       ORDER BY kind, dedupe_key, updated_at DESC, id DESC
  ) AS latest
 WHERE failed.state = 'failed'
   AND failed.kind = latest.kind
   AND failed.dedupe_key = latest.dedupe_key
   AND failed.updated_at < latest.updated_at;

-- `last_failure()` serves only failures nothing has fixed.
DROP INDEX jobs_failed_recent_idx;
CREATE INDEX jobs_failed_recent_idx
    ON jobs (updated_at DESC)
    WHERE state = 'failed' AND last_error IS NOT NULL AND superseded_by IS NULL;

-- `complete_job()` finds the failures it supersedes by key without scanning
-- history. Failed rows are few, so this index stays small.
CREATE INDEX jobs_failed_unsuperseded_key_idx
    ON jobs (dedupe_key)
    WHERE state = 'failed' AND superseded_by IS NULL;
