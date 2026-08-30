-- Migration 0019 - make a lane miss independent of every other lane's backlog.
--
-- Separate indexes preserve the claim queries' global priority-first LIFO
-- ordering. A `(kind, priority, id)` composite makes the one-kind lookup cheap,
-- but PostgreSQL must sort the general worker's scan+summarize union because
-- `kind` precedes the ordering keys. Lane predicates narrow both populations
-- while leaving `(priority DESC, id DESC)` as each index's ordered edge.
-- `not_before` remains an eligibility gate covered for index-only rejection.

DROP INDEX jobs_claim_idx;
CREATE INDEX jobs_claim_general_idx
    ON jobs (priority DESC, id DESC)
    INCLUDE (not_before)
    WHERE state = 'pending' AND kind IN ('scan_file', 'summarize');
CREATE INDEX jobs_claim_embed_idx
    ON jobs (priority DESC, id DESC)
    INCLUDE (not_before)
    WHERE state = 'pending' AND kind = 'embed';
