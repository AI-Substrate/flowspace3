-- Migration 0008 - the user messages queue: one channel for everything the
-- daemon needs to tell the person driving (PRD req 59, Jordan 2026-08-27).
--
-- Before this table, a feature with news had two bad options: bolt a field onto
-- one verb's payload (so the user only hears it if they happen to run that
-- verb), or smuggle it through `meta` (which is documented as facts about THIS
-- answer, never load-bearing). This table is the third option, and the only
-- one: the envelope carries every live row on every daemon response.
--
-- Identity is the PRODUCER'S key, not a generated id. `update:installed:0.3.1`
-- is a message that either exists or does not; a reconcile loop pushing it on
-- every pass must produce one row, not one per pass. That is why `key` is the
-- primary key and why there is no surrogate.
--
-- There is deliberately no `clear_condition` column. Clearing is level-
-- triggered, exactly like every other piece of daemon state: a producer's pass
-- declares the messages its source should have RIGHT NOW, and `sync_messages`
-- deletes the rest of that source. Storing a predicate here would mean a rules
-- engine in the queue evaluating conditions it does not own. `acked_at` and
-- `expires_at` cover only the two cases a producer genuinely cannot retract:
-- a human dismissing a notice, and one that stops being true by the calendar.

CREATE TABLE user_messages (
    -- The producer's stable identity, e.g. 'update:installed:0.3.1'.
    key         TEXT NOT NULL PRIMARY KEY,
    -- The feature that raised it. A producer owns every row under its own
    -- source and none under anyone else's - that ownership is what makes the
    -- delete half of sync_messages safe.
    source      TEXT NOT NULL,
    severity    TEXT NOT NULL,
    text        TEXT NOT NULL,
    -- NOT NULL on purpose: a message a user cannot act on is a log line.
    next_action TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Set when a re-push changes the wording, so an edited message does not
    -- jump the ordering of one that has been standing longer.
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Explicitly dismissed. Kept rather than deleted so a producer re-pushing
    -- the identical key does not resurrect what the user already waved away.
    acked_at    TIMESTAMPTZ,
    -- Stops being true by the calendar rather than by a state change.
    expires_at  TIMESTAMPTZ,

    CONSTRAINT user_messages_severity_known
        CHECK (severity IN ('info', 'warning', 'error'))
);

-- The read the envelope does on every single daemon response, so it is the one
-- access path worth an index: live rows, oldest first.
CREATE INDEX user_messages_live_idx
    ON user_messages (created_at)
    WHERE acked_at IS NULL;

-- The producer's own scan in sync_messages.
CREATE INDEX user_messages_source_idx ON user_messages (source);
