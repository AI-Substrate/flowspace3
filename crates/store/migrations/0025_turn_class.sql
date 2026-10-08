-- Migration 0025 - label each conversation turn with what it IS.
--
-- On the live index, the top twenty hits for a real question were all echoes:
-- agents' own `flowspace3 search` calls and the envelopes those calls printed.
-- `turn_class` lets search hide that retrieval traffic by default while every
-- other turn, and every turn of the caller's own session, stays findable.
--
-- The column lives on `elements` because that is where search admits rows,
-- and a turn element's address is per occurrence (`conv:<guid>#t<n>`), so
-- the label can depend on the turns around it, not only on shared content.
--
-- NULL means "not classified yet" for turns written before this migration,
-- and "not a turn" for every code and document element. The daemon backfills
-- the existing turns from the same classifier ingest uses; there is no SQL
-- copy of the rules to drift. Unclassified turns are never hidden.
ALTER TABLE elements
    ADD COLUMN turn_class TEXT;

-- NOT VALID: every existing row is NULL, so a validating scan of the whole
-- table would prove nothing; new and updated rows are checked as usual.
ALTER TABLE elements
    ADD CONSTRAINT elements_turn_class_known
    CHECK (turn_class IN ('work', 'tool_call', 'tool_result',
                          'retrieval_call', 'retrieval_result'))
    NOT VALID;

-- The backfill's work queue. It shrinks to nothing as turns are classified,
-- so it costs nothing once the backfill is done.
CREATE INDEX elements_turn_unclassified
    ON elements (id)
    WHERE kind = 'turn' AND turn_class IS NULL;
