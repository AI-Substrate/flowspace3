-- Migration 0016 - newest ready work wins within its declared priority.
--
-- `not_before` remains the retry/debounce eligibility gate. It cannot also be
-- recency: parking or retrying an old row moves that timestamp and would make
-- the old job masquerade as newly enqueued work. `id` is immutable insertion
-- order, so descending id is the honest LIFO signal.

DROP INDEX jobs_claim_idx;
CREATE INDEX jobs_claim_idx
    ON jobs (priority DESC, id DESC)
    INCLUDE (not_before)
    WHERE state = 'pending';
