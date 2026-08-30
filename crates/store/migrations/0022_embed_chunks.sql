-- Migration 0021 - let one source own every vector needed to cover its text.
--
-- The old key allowed one vector per (source, kind, model), so content beyond
-- the embedding model's per-input cap could only be discarded. `chunk_no`
-- names each overlapping window while keeping the original content hash as the
-- ownership key used by the pre-check and reconciler.
--
-- DEFAULT 0 grandfathers every existing vector without re-embedding or a
-- tuple-by-tuple backfill: pre-chunking vectors are exactly chunk zero. Keep the
-- `truncated` inventory beside them so a later backfill can identify vectors
-- that still cover only a prefix.
ALTER TABLE embeddings_1024
    ADD COLUMN chunk_no SMALLINT NOT NULL DEFAULT 0;

ALTER TABLE embeddings_1024
    DROP CONSTRAINT embeddings_1024_pkey;

ALTER TABLE embeddings_1024
    ADD PRIMARY KEY (source_hash, source_kind, chunk_no, model_key);
