-- Which agent had a conversation (conversation recall, 2026-10-09).
--
-- "Which repo AND AGENT did this work" had no answer: a conversation row held a
-- session uuid and an anchor, nothing about who was talking. These record the
-- harness that wrote it, every model seen in it (first-seen order), and the pij
-- seat bound to its session. All stay NULL/empty for imports and for agents fs3
-- cannot identify; ingest fills them as it reads.
ALTER TABLE conversations
    ADD COLUMN agent_harness TEXT,
    ADD COLUMN agent_models TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN pij_seat TEXT;

-- Every conversation ingest already read knows its harness from its cursor.
UPDATE conversations c
   SET agent_harness = ic.harness
  FROM ingest_cursors ic
 WHERE ic.conversation_id = c.guid
   AND c.agent_harness IS NULL;
