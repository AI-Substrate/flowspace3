-- Migration 0033 - a binary copy of every vector, for exact ranking.
--
-- A scope ranked exactly reads each of its vectors in full: a key lookup, a
-- heap fetch and a TOAST detoast of 4 KB, about nine page touches per
-- source. On prod's real vectors (H1, 2026-10-15) that is 2.3 s warm for a
-- 303k-source scope on the bench and 9.5 s on prod. Ranking the scope on
-- `binary_quantize(vector)` (128 bytes, read index-only from this table's
-- key) and re-ranking the nearest few by the full vector gives the same top
-- ten - mean recall@10 >= 0.998 at every scope from 14k to 303k sources -
-- 1.8-3.5x faster warm and 3.3-4.5x faster cold
-- (`scratch/search-speed/H1-quantization-findings.md`).
--
-- The table is kept in step by trigger, so every writer is covered,
-- including ones not written yet: an insert or a re-embedded vector writes
-- its bits, a delete removes them. Rows that predate this migration are
-- filled by `flowspace3 doctor exact-index`, never at boot (a full pass over
-- the vectors is a heavy job), which then records the table as complete in
-- `search_index_ready`. Search reads it only when `fs3.exact_rank` is
-- `compact` AND that row exists. The mark is re-checked, by doctor and by the
-- daemon every ten minutes, and cleared the moment any vector lacks its copy
-- (a table-only restore, a manual delete, triggers disabled): compact never
-- ranks on a copy known to be incomplete.
--
-- Locks: the table is new and empty, and its key is built empty, so nothing
-- is built here. The triggers take SHARE ROW EXCLUSIVE on embeddings_1024
-- for a moment, which waits for in-flight embedding writes and blocks new
-- ones until it commits; searches (ACCESS SHARE) are never blocked. Never
-- wait long for that lock: a boot behind a pg_dump or an open psql fails
-- with this error instead of hanging.

SET LOCAL lock_timeout = '5s';

-- The key is the exact arm's lookup, (source_hash, source_kind, model_key),
-- and carries the bits, so ranking a scope never touches the heap once the
-- visibility map is current.
CREATE TABLE embeddings_1024_bq (
    source_hash text NOT NULL,
    source_kind text NOT NULL,
    model_key text NOT NULL,
    chunk_no smallint NOT NULL,
    bq bit(1024) NOT NULL,
    CONSTRAINT embeddings_1024_bq_pkey
        PRIMARY KEY (source_hash, source_kind, model_key, chunk_no) INCLUDE (bq)
);

-- Insert-only growth: vacuum at 5% inserted so index-only scans stay
-- index-only (migration 0031's rule for the other big tables).
ALTER TABLE embeddings_1024_bq SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_analyze_scale_factor = 0.01,
    autovacuum_vacuum_insert_scale_factor = 0.05
);

-- Which explicitly built search structures are complete. One row per
-- structure, written only by the command that completes it.
CREATE TABLE search_index_ready (
    name text PRIMARY KEY,
    ready_at timestamptz NOT NULL DEFAULT now()
);

CREATE FUNCTION fs3_embeddings_bq_write() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    -- No writer moves a vector to another key today; if one ever does, the
    -- copy moves with it rather than being orphaned under the old key.
    IF TG_OP = 'UPDATE'
       AND (OLD.source_hash, OLD.source_kind, OLD.model_key, OLD.chunk_no)
           IS DISTINCT FROM (NEW.source_hash, NEW.source_kind, NEW.model_key, NEW.chunk_no)
    THEN
        DELETE FROM embeddings_1024_bq
         WHERE source_hash = OLD.source_hash AND source_kind = OLD.source_kind
           AND model_key = OLD.model_key AND chunk_no = OLD.chunk_no;
    END IF;
    INSERT INTO embeddings_1024_bq (source_hash, source_kind, model_key, chunk_no, bq)
    VALUES (NEW.source_hash, NEW.source_kind, NEW.model_key, NEW.chunk_no,
            binary_quantize(NEW.vector)::bit(1024))
    ON CONFLICT (source_hash, source_kind, model_key, chunk_no)
        DO UPDATE SET bq = EXCLUDED.bq;
    RETURN NULL;
END $$;

CREATE FUNCTION fs3_embeddings_bq_delete() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM embeddings_1024_bq
     WHERE source_hash = OLD.source_hash AND source_kind = OLD.source_kind
       AND model_key = OLD.model_key AND chunk_no = OLD.chunk_no;
    RETURN NULL;
END $$;

-- TRUNCATE fires no row trigger. Emptying both tables keeps the copy
-- complete (nothing lacks one), so its ready mark stays true.
CREATE FUNCTION fs3_embeddings_bq_truncate() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    TRUNCATE embeddings_1024_bq;
    RETURN NULL;
END $$;

-- `put_embeddings` upserts, so a re-embedded vector arrives as an UPDATE.
CREATE TRIGGER embeddings_1024_bq_write
    AFTER INSERT OR UPDATE OF vector, source_hash, source_kind, model_key, chunk_no
    ON embeddings_1024
    FOR EACH ROW EXECUTE FUNCTION fs3_embeddings_bq_write();
CREATE TRIGGER embeddings_1024_bq_delete AFTER DELETE ON embeddings_1024
    FOR EACH ROW EXECUTE FUNCTION fs3_embeddings_bq_delete();
CREATE TRIGGER embeddings_1024_bq_truncate AFTER TRUNCATE ON embeddings_1024
    FOR EACH STATEMENT EXECUTE FUNCTION fs3_embeddings_bq_truncate();
