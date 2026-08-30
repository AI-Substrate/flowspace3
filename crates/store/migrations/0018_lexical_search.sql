-- Migration 0018 — indexed, case-insensitive exact text retrieval.
--
-- One trigram index covers both declaration names and source text. The query
-- separately tests `name` to place structural matches ahead of body-only hits,
-- but a combined index avoids paying for two copies of the same trigrams.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE INDEX elements_lexical_trgm_idx
    ON elements USING GIN (lower(name || E'\n' || raw_text) gin_trgm_ops);
