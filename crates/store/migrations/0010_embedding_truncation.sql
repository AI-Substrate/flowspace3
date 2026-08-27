-- Migration 0010 - say when a vector only covers part of its content.
--
-- Embedding models have a per-input token cap (8192 for the text-embedding-3
-- family). fs3 had no guard for it anywhere, so an element bigger than the cap
-- was answered with `400 Invalid 'input[0]': maximum input length is 8192
-- tokens`, retried three times into the same answer, and then failed for good:
-- 59 elements of a real repository were permanently unsearchable, with nothing
-- in the index saying so.
--
-- The fix truncates an oversized input to fit. That trade - a vector of the
-- first N tokens instead of no vector at all - is worth making, and it is NOT
-- worth making silently: a truncated vector answers queries about the head of
-- an element and is blind to its tail, which looks exactly like a working
-- vector until somebody searches for something in the tail.
--
-- So the fact is recorded beside the vector it qualifies.
--
-- A COLUMN rather than the `extras` JSONB that `smart_content` uses, because
-- this is a property of the VECTOR (this row's own text was cut), not of a
-- summary's payload, and because it has to be aggregable: "how many of my
-- vectors are partial" is a question `count(*) FILTER (WHERE truncated)`
-- answers on an index scan and a JSONB probe does not.
--
-- The default makes this free for every existing row and rewrites nothing:
-- Postgres 11+ stores a non-volatile column default as table metadata rather
-- than writing it into every tuple. Existing rows read `false`, which is TRUE
-- of them: nothing before this migration could truncate, because nothing
-- before this migration would have been accepted by the provider at all.
ALTER TABLE embeddings_1024
    ADD COLUMN truncated BOOLEAN NOT NULL DEFAULT false;

-- DELIBERATELY NOT part of the primary key, and this is the fork worth naming.
--
-- The key stays `(source_hash, source_kind, model_key)` and `source_hash` stays
-- the hash of the ORIGINAL text, not of the prefix that was embedded. A
-- truncated embedding IS the embedding for that content: the pre-check that
-- makes re-emission free asks "does this hash have a vector", the reconciler
-- asks the same, and re-keying on the prefix would make both answer "no"
-- forever - re-embedding the same element on every scan, for ever, and calling
-- it a cache miss.
--
-- The consequence, stated rather than left implicit: raising the cap (a bigger
-- model, a better tokenizer) does NOT automatically re-embed what was
-- truncated under the old one, because the key did not move. This column is
-- what makes that recoverable - the rows to redo are exactly
-- `WHERE truncated`.
CREATE INDEX embeddings_1024_truncated_idx
    ON embeddings_1024 (model_key)
    WHERE truncated;
