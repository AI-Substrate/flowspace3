-- Plan 005: a child conversation knows its parent.
--
-- Claude writes a subagent's conversation into a SIDECAR file beside the
-- session that spawned it, and the two are genuinely separate conversations —
-- a sidecar folded into the parent's sequence makes both unreadable, which is
-- why `SessionKind` exists at all. But they are RELATED, and until now that
-- relationship lived only in an in-memory `SessionFile` and an ingest report
-- the worker discarded. After a job settled, nothing could navigate from a
-- child conversation to its parent (cross-model review, finding F-002).
--
-- Nullable because most conversations have no parent: every omp session, every
-- pij ledger, every metrics-db session, and every claude MAIN file.
--
-- Deliberately NOT a foreign key to conversations(guid), for the same reason
-- `repo_identity` is not one: ingest order is not guaranteed. A sidecar can be
-- resolved and stored before its parent has any turns, and a constraint here
-- would make the arrival order of two independent files a correctness
-- condition. The id is derivable rather than looked up — it is the same
-- deterministic derivation the parent's own row uses — so a dangling value
-- means the parent has not been ingested YET, which is a state to report, not
-- an integrity violation to refuse.
ALTER TABLE conversations
    ADD COLUMN parent_conversation_id UUID;

-- The navigation this exists for: every child of a conversation. Partial,
-- because the column is null for the overwhelming majority of rows.
CREATE INDEX conversations_parent
    ON conversations (parent_conversation_id)
    WHERE parent_conversation_id IS NOT NULL;
