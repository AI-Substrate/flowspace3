-- Migration 0014 - ingest cursors: where a native-store read stopped, and the
-- ledger a post-rotation rescan is deduplicated against.
--
-- Plan 005 (req-0027). Every native conversation store is APPEND-ONLY, which is
-- the whole reason ingest is cheap: a second ingest of a session that grew
-- costs only the turns that are new. That bargain needs exactly two durable
-- facts per conversation, and this migration is those two facts.
--
-- Both tables are REF layer. Nothing here holds turn TEXT - the prose went into
-- `turns` and the content layer through 0013. These are bookmarks.
--
-- ═══ WHY A CURSOR IS NOT ENOUGH ══════════════════════════════════════════
--
-- A cursor alone survives the happy path and nothing else. When a session file
-- rotates or is truncated the reader cannot resume - it restarts from zero and
-- reports `rescanned = true` - and what comes back is the WHOLE conversation,
-- not a delta. Appending that blindly duplicates every turn the conversation
-- ever had, and it would look exactly like a busy session.
--
-- So the ledger records, per record, the store's own natural identifier and the
-- turn number it was stored under. That is what makes a rescan a no-op rather
-- than a disaster, and it is the case `ingest_cursors.rs` exists to make
-- impossible.

-- ═══ CURSORS ═════════════════════════════════════════════════════════════
--
-- Keyed by (harness, session_id) rather than by conversation, because that is
-- the pair a reader can resolve BEFORE it knows anything else: the operator
-- names a harness and a session, and the cursor must be findable from just
-- those two. The conversation is carried alongside, so a delete cascades.
--
-- `cursor` is JSONB because `SourceCursor` is a tagged union of three shapes -
-- a byte offset with the inode it belongs to, a ledger seq, a sqlite rowid -
-- and a column per variant would be five nullable columns and a check
-- constraint to say which three are meaningful. The Rust type already refuses
-- the invalid combinations; storing its own serialisation keeps exactly one
-- definition of what a cursor is. A fourth store is then a code change and
-- never a migration, which is the same bargain `turns.items` makes.
CREATE TABLE ingest_cursors (
    -- Which native store. The wire spelling `Harness::as_str` produces.
    harness         TEXT        NOT NULL,
    -- The store's identifier for the conversation: claude/omp session uuid,
    -- pij seat, or metrics-db external_session_id.
    session_id      TEXT        NOT NULL,
    -- Where the turns landed. Cascades, so forgetting a conversation forgets
    -- how to resume it - a cursor into a conversation nobody stores any more
    -- would resume an ingest that appends to nothing.
    conversation_id UUID        NOT NULL REFERENCES conversations(guid) ON DELETE CASCADE,
    -- The serialised `SourceCursor`. Opaque to SQL on purpose.
    cursor          JSONB       NOT NULL,
    -- Server-side, like every other timestamp in this schema: two machines
    -- cannot disagree about when a poll happened.
    last_read_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (harness, session_id),

    -- The closed set `Harness` can express, named so the test that proves it
    -- bites can cite the constraint. A fifth store is a stop-and-ask, and this
    -- is where the database says so.
    CONSTRAINT ingest_cursors_harness_known
        CHECK (harness IN ('claude', 'omp', 'pij', 'metrics-db'))
);

-- "Which conversations am I still tailing?" - the question an ingest status
-- verb asks, and the one a conversation delete answers through the cascade.
CREATE INDEX ingest_cursors_conversation_idx ON ingest_cursors (conversation_id);

-- ═══ THE ORDINAL LEDGER ══════════════════════════════════════════════════
--
-- One row per record ever ingested: the store's natural id, and the `turn_no`
-- it was stored under.
--
-- The mapping is the point, not just the key. Dedupe needs only "have I seen
-- this ordinal", but `turn_no` is the navigation axis (req-0026) AND half the
-- primary key `append_turns` is idempotent on. Carrying the number means a
-- rescan RECOVERS the number a record already had instead of minting a second
-- one for the same content - at identical row count, because the row exists
-- either way. Without it, a rescan that re-sees a stored record can only
-- append it under a fresh number, and the conversation grows a duplicate that
-- the primary key cannot catch because the key itself is different.
CREATE TABLE ingest_ledger (
    harness    TEXT NOT NULL,
    session_id TEXT NOT NULL,
    -- The store's natural identifier: claude `uuid`, omp record `id`, ledger
    -- `seq`, metrics-db `rowid`. TEXT because those are four types and the
    -- reader has already reduced them to one.
    ordinal    TEXT NOT NULL,
    -- The number this record was stored under, dense from 1.
    turn_no    INT  NOT NULL,

    PRIMARY KEY (harness, session_id, ordinal),

    FOREIGN KEY (harness, session_id)
        REFERENCES ingest_cursors (harness, session_id) ON DELETE CASCADE,

    -- Dense from 1, matching `turns_ordinal_positive`.
    CONSTRAINT ingest_ledger_turn_no_positive CHECK (turn_no >= 1)
);

-- The high-water mark: "what number does the next new turn take?" is asked
-- once per poll, and it is a MAX over one session's rows. Descending so the
-- answer is the first entry of the range rather than a scan of all of them -
-- a long-running seat's ledger is thousands of rows and every poll pays this.
CREATE INDEX ingest_ledger_high_water_idx
    ON ingest_ledger (harness, session_id, turn_no DESC);
