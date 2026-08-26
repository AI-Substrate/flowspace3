-- Migration 0005 - the job backlog: the daemon's PG-backed work list.
--
-- Workshop 002, and the locked direction in
-- docs/plans/prd/daemon-worker-architecture.md. One table is the whole queue.
--
-- Decision D1: there is no `dirty_files` table. Dirtiness IS a pending
-- `scan_file` job. The watcher firing five times for one save enqueues five
-- times and gets one row, because `dedupe_key` is unique among live jobs; each
-- re-fire pushes `not_before` further out, which is the ten-second debounce
-- expressed in SQL rather than in a timer somewhere in the daemon.
--
-- Decision D4: workers claim with FOR UPDATE SKIP LOCKED - the boring, proven
-- Postgres pattern. Two workers polling at the same instant take two different
-- jobs instead of blocking on each other, which is what lets an LLM job and an
-- embedding job run concurrently (the fs2 property being kept).

CREATE TABLE jobs (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- 'scan_file' first; 'summarize' and 'embed' are siblings, not subclasses.
    kind        TEXT   NOT NULL,
    -- e.g. 'scan:wt42:src/foo.rs'. The idempotence key for enqueue.
    dedupe_key  TEXT   NOT NULL,
    payload     JSONB  NOT NULL,
    state       TEXT   NOT NULL DEFAULT 'pending',
    priority    INT    NOT NULL DEFAULT 0,
    -- The debounce lives HERE, not in a timer. A re-fire pushes it out.
    not_before  TIMESTAMPTZ NOT NULL DEFAULT now(),
    attempts    INT    NOT NULL DEFAULT 0,
    last_error  TEXT,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT jobs_state_known
        CHECK (state IN ('pending', 'running', 'done', 'failed'))
);

-- PARTIAL on purpose. Uniqueness applies to LIVE jobs only, so a finished
-- 'scan:wt42:src/foo.rs' does not block the next edit to that file from
-- enqueueing a fresh one - while an edit arriving mid-flight still collapses
-- into the row already in the queue.
CREATE UNIQUE INDEX jobs_live_dedupe_idx
    ON jobs (dedupe_key)
    WHERE state IN ('pending', 'running');

-- The claim query's access path: ready work, best priority first, oldest
-- deadline first.
CREATE INDEX jobs_claim_idx ON jobs (state, not_before, priority DESC);
