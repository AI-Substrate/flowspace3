-- Migration 0011 - tell apart a job that ran out of patience from one that can
-- never succeed.
--
-- `fail_job` was one verb for two very different endings:
--
--   * three attempts spent against a provider that kept saying no - the work is
--     still wanted, and a later binary may well manage it;
--   * a defect that no amount of running will fix - an unreadable payload, an
--     unknown job kind, a vector of the wrong width.
--
-- Nothing recorded which had happened, so nothing could ever bring the first
-- kind back without bringing the second kind with it. That mattered the moment
-- a fix landed for a whole CLASS of failure: 59 elements of a real index had
-- exhausted their attempts against the embedding model's per-input token cap,
-- and after the guard that truncates oversized inputs there was no path back
-- for them short of hand-written SQL. `fail_job`'s own documentation pointed at
-- the decision-D6 reconciler sweep, which does not exist yet.
--
-- `terminal` is that missing bit. The runner already computes it - a `Failure`
-- carries `retryable`, and the verdict that ends a job knows whether it ended
-- because the failure was hopeless or because the ladder ran out.
--
-- DEFAULT false is deliberate and is the recovery itself: every row that failed
-- before this migration reads "not terminal", which is what lets the jobs
-- already sitting failed on a live index be requeued once by the daemon that
-- first understands the column. A defect among them costs exactly one more
-- claim, fails again, and this time says so - after which it is never picked up
-- again.
ALTER TABLE jobs
    ADD COLUMN terminal BOOLEAN NOT NULL DEFAULT false;

-- The requeue sweep's access path: failed, revivable, of a given kind. Narrow
-- on purpose - it is a boot-time query over the settled history, which is the
-- part of this table that grows without bound.
CREATE INDEX jobs_revivable_idx
    ON jobs (kind)
    WHERE state = 'failed' AND NOT terminal;
